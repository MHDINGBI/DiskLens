use crate::store::{unprotect, Provider};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(serde::Deserialize, Clone)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

const NO_WINDOW: u32 = 0x0800_0000;

pub async fn complete(p: &Provider, system: &str, history: Vec<Msg>) -> Result<String, String> {
    match p.kind.as_str() {
        "openai" => openai(p, system, history).await,
        "cli_claude" | "cli_codex" => {
            let prompt = flatten(system, &history);
            let kind = p.kind.clone();
            let model = p.model.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if kind == "cli_claude" { run_claude(&prompt, &model) } else { run_codex(&prompt, &model) }
            })
            .await
            .map_err(|e| e.to_string())?
        }
        k => Err(format!("Type de fournisseur inconnu: {k}")),
    }
}

fn flatten(system: &str, history: &[Msg]) -> String {
    let mut s = format!("{system}\n\n");
    for m in history {
        let who = if m.role == "user" { "Utilisateur" } else { "Assistant" };
        s.push_str(&format!("[{who}]\n{}\n\n", m.content));
    }
    s.push_str("[Assistant]\n");
    s
}

async fn openai(p: &Provider, system: &str, history: Vec<Msg>) -> Result<String, String> {
    let url = format!("{}/chat/completions", p.base_url.trim_end_matches('/'));
    let mut msgs = vec![json!({"role": "system", "content": system})];
    for m in history {
        msgs.push(json!({"role": m.role, "content": m.content}));
    }
    let mut body = json!({"messages": msgs, "temperature": 0.2, "stream": false});
    if !p.model.is_empty() {
        body["model"] = json!(p.model);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(&body);
    if let Some(k) = unprotect(&p.key_enc) {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await.map_err(|e| format!("Connexion à {url} impossible: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status}: {}", v["error"]["message"].as_str().unwrap_or(&v.to_string())));
    }
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Réponse vide du modèle".to_string())
}

fn tmp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

fn run_claude(prompt: &str, model: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--tools", "", "--no-session-persistence"]);
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    let mut child = cmd
        .current_dir(tmp_dir())
        .creation_flags(NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("CLI 'claude' introuvable: {e}"))?;
    child.stdin.take().unwrap().write_all(prompt.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run_codex(prompt: &str, model: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let outfile = tmp_dir().join(format!("disklens_codex_{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&outfile);
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "codex", "exec", "--skip-git-repo-check", "-s", "read-only", "-o"]);
    cmd.arg(&outfile);
    if !model.is_empty() {
        cmd.args(["-m", model]);
    }
    cmd.arg("-");
    let mut child = cmd
        .current_dir(tmp_dir())
        .creation_flags(NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("CLI 'codex' introuvable: {e}"))?;
    child.stdin.take().unwrap().write_all(prompt.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&outfile).unwrap_or_default();
    let _ = std::fs::remove_file(&outfile);
    if text.trim().is_empty() {
        return Err(format!("Codex n'a rien renvoyé. {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")));
    }
    Ok(text.trim().to_string())
}

/// Extrait le premier objet JSON équilibré d'une réponse (ignore les ```json et le texte autour).
pub fn extract_json(s: &str) -> Option<Value> {
    let b = s.as_bytes();
    let start = s.find('{')?;
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for i in start..b.len() {
        let c = b[i];
        if in_str {
            if esc { esc = false } else if c == b'\\' { esc = true } else if c == b'"' { in_str = false }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&s[start..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}
