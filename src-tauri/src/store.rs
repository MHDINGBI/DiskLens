use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// "openai" | "cli_claude" | "cli_codex"
    pub kind: String,
    pub base_url: String,
    pub model: String,
    /// Clé protégée par DPAPI (hex). Jamais renvoyée au frontend.
    #[serde(default)]
    pub key_enc: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Llama {
    pub exe: String,
    pub models_dir: String,
    pub model_file: String,
    pub port: u16,
    pub ctx: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub providers: Vec<Provider>,
    pub active: String,
    pub llama: Llama,
}

fn p(id: &str, name: &str, kind: &str, base: &str, model: &str) -> Provider {
    Provider { id: id.into(), name: name.into(), kind: kind.into(), base_url: base.into(), model: model.into(), key_enc: String::new() }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            providers: vec![
                p("claude", "Claude (abonnement, CLI)", "cli_claude", "", ""),
                p("codex", "Codex (abonnement, CLI)", "cli_codex", "", ""),
                p("llamacpp", "llama.cpp (local)", "openai", "http://127.0.0.1:8080/v1", ""),
                p("ollama", "Ollama (local)", "openai", "http://127.0.0.1:11434/v1", ""),
                p("deepseek", "DeepSeek (API)", "openai", "https://api.deepseek.com/v1", "deepseek-chat"),
                p("zai", "Z.ai GLM (API)", "openai", "https://api.z.ai/api/paas/v4", "glm-4.6"),
            ],
            active: "claude".into(),
            llama: Llama {
                exe: String::new(),
                models_dir: "D:\\INTELLIGENCE ARTICIELLE\\MODELS".into(),
                model_file: String::new(),
                port: 8080,
                ctx: 8192,
            },
        }
    }
}

pub fn load(dir: &Path) -> Settings {
    let mut s: Settings = fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    // ajoute les fournisseurs par défaut manquants (mises à jour futures)
    for d in Settings::default().providers {
        if !s.providers.iter().any(|x| x.id == d.id) {
            s.providers.push(d);
        }
    }
    s
}

pub fn save(dir: &Path, s: &Settings) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let t = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(dir.join("settings.json"), t).map_err(|e| e.to_string())
}

pub fn data_file(dir: &Path, name: &str) -> PathBuf {
    let _ = fs::create_dir_all(dir);
    dir.join(name)
}

// ---------- DPAPI ----------

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

pub fn protect(plain: &str) -> Result<String, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let mut input = CRYPT_INTEGER_BLOB { cbData: plain.len() as u32, pbData: plain.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        let ok = CryptProtectData(&mut input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out);
        if ok == 0 {
            return Err("Chiffrement DPAPI impossible".into());
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Ok(to_hex(&bytes))
    }
}

pub fn unprotect(hex: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let mut data = from_hex(hex)?;
    let mut input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        let ok = CryptUnprotectData(&mut input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out);
        if ok == 0 {
            return None;
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        String::from_utf8(bytes).ok()
    }
}
