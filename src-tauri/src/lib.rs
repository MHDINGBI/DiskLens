mod clean;
mod llm;
mod onedrive;
mod demarrage;
mod sante;
mod sauvegarde;
mod registre;
mod residus;
mod scan;
mod store;
mod rapport_mensuel;
mod cli;

use clean::{Candidate, DeleteResult, DupGroup};
use scan::{Arena, Kind, Progress};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use store::Settings;
use tauri::{Emitter, Manager, State};

struct AppState {
    arenas: Mutex<HashMap<String, Arena>>,
    cancel: Arc<AtomicBool>,
    scanning: AtomicBool,
    settings: Mutex<Settings>,
    dir: PathBuf,
    llama: Mutex<Option<Child>>,
    ai_items: Mutex<HashMap<String, Candidate>>,
}

type R<T> = Result<T, String>;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------- Disques ----------

#[derive(Serialize)]
struct Drive {
    key: String,
    mount: String,
    label: String,
    total: u64,
    free: u64,
    scanned_at: Option<u64>,
}

#[tauri::command]
fn list_drives(st: State<AppState>) -> Vec<Drive> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let arenas = st.arenas.lock().unwrap();
    let mut v: Vec<Drive> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().into_owned();
            let key = mount.trim_end_matches('\\').to_uppercase();
            Drive {
                scanned_at: arenas.get(&key).map(|a| a.scanned_at),
                key,
                mount,
                label: d.name().to_string_lossy().into_owned(),
                total: d.total_space(),
                free: d.available_space(),
            }
        })
        .collect();
    v.sort_by(|a, b| a.key.cmp(&b.key));
    v
}

// ---------- Scan ----------

#[derive(Serialize, Clone)]
struct ScanProgress {
    files: u64,
    bytes: u64,
    dirs: u64,
    errors: u64,
    current: String,
}

#[derive(Serialize)]
struct ScanSummary {
    key: String,
    size: u64,
    files: u32,
    errors: u64,
    secs: f32,
    cancelled: bool,
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    ts: u64,
    total: u64,
    free: u64,
    dirs: Vec<(String, u64)>,
    #[serde(default)]
    files: Vec<(String, u64)>,
}

#[tauri::command]
async fn start_scan(app: tauri::AppHandle, st: State<'_, AppState>, root: String) -> R<ScanSummary> {
    if st.scanning.swap(true, Ordering::SeqCst) {
        return Err("Un scan est déjà en cours".into());
    }
    st.cancel.store(false, Ordering::SeqCst);
    let key = root.trim_end_matches('\\').to_uppercase();
    let prog = Arc::new(Progress::default());
    let cancel = st.cancel.clone();
    let done = Arc::new(AtomicBool::new(false));

    // ticker de progression
    {
        let (prog, done, app) = (prog.clone(), done.clone(), app.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let _ = app.emit("scan-progress", ScanProgress {
                    files: prog.files.load(Ordering::Relaxed),
                    bytes: prog.bytes.load(Ordering::Relaxed),
                    dirs: prog.dirs.load(Ordering::Relaxed),
                    errors: prog.errors.load(Ordering::Relaxed),
                    current: prog.current.lock().map(|c| c.clone()).unwrap_or_default(),
                });
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        });
    }

    let t0 = Instant::now();
    let root2 = key.clone();
    let (prog2, cancel2) = (prog.clone(), cancel.clone());
    let arena = tauri::async_runtime::spawn_blocking(move || scan::scan_root(&root2, &prog2, &cancel2))
        .await
        .map_err(|e| e.to_string());
    done.store(true, Ordering::Relaxed);
    st.scanning.store(false, Ordering::SeqCst);
    let arena = arena?;
    let cancelled = cancel.load(Ordering::Relaxed);
    let summary = ScanSummary {
        key: key.clone(),
        size: arena.nodes[0].size,
        files: arena.nodes[0].files,
        errors: arena.errors,
        secs: t0.elapsed().as_secs_f32(),
        cancelled,
    };
    if !cancelled {
        save_snapshot(&st.dir, &key, &arena);
        st.arenas.lock().unwrap().insert(key, arena);
    }
    Ok(summary)
}

#[tauri::command]
fn cancel_scan(st: State<AppState>) {
    st.cancel.store(true, Ordering::SeqCst);
}

fn save_snapshot(dir: &PathBuf, key: &str, a: &Arena) {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let (mut total, mut free) = (0, 0);
    for d in disks.iter() {
        if d.mount_point().to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(key) {
            total = d.total_space();
            free = d.available_space();
        }
    }
    let mut dirs: Vec<(String, u64)> = Vec::new();
    let mut stack = vec![(0u32, 0u32)];
    while let Some((i, depth)) = stack.pop() {
        let n = &a.nodes[i as usize];
        if n.kind != Kind::Dir || n.size < 300 * 1024 * 1024 {
            continue;
        }
        dirs.push((a.path_of(i), n.size));
        if depth < 3 {
            stack.extend(n.children.iter().map(|&c| (c, depth + 1)));
        }
    }
    dirs.sort_by(|x, y| y.1.cmp(&x.1));
    dirs.truncate(200);
    let file = store::data_file(dir, &format!("snapshots_{}.json", key.replace(':', "")));
    let mut all: Vec<Snapshot> = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut files: Vec<_> = a.nodes.iter().enumerate().filter(|(_,n)| n.kind == Kind::File)
        .map(|(i,n)| (a.path_of(i as u32), n.size)).collect();
    files.sort_by(|a,b| b.1.cmp(&a.1));
    files.truncate(20);
    all.push(Snapshot { ts: now(), total, free, dirs, files });
    if all.len() > 60 {
        all.drain(0..all.len() - 60);
    }
    let _ = std::fs::write(&file, serde_json::to_string(&all).unwrap_or_default());
}

// ---------- Exploration ----------

#[derive(Serialize)]
struct Item {
    name: String,
    path: String,
    kind: Kind,
    size: u64,
    files: u32,
    age_days: Option<u32>,
}

#[derive(Serialize)]
struct Listing {
    path: String,
    name: String,
    size: u64,
    files: u32,
    items: Vec<Item>,
    hidden_count: usize,
    hidden_size: u64,
}

fn age_of(ts: u32) -> Option<u32> {
    if ts == 0 { None } else { Some((now().saturating_sub(ts as u64) / 86_400) as u32) }
}

fn arena_for<'a>(m: &'a HashMap<String, Arena>, path: &str) -> R<&'a Arena> {
    let key = path.split('\\').next().unwrap_or("").to_uppercase();
    m.get(&key).ok_or_else(|| format!("{key} n'est pas encore analysé"))
}

#[tauri::command]
fn get_children(st: State<AppState>, path: String, limit: usize) -> R<Listing> {
    let m = st.arenas.lock().unwrap();
    let a = arena_for(&m, &path)?;
    let idx = a.resolve(&path).ok_or("Dossier introuvable (supprimé ?)")?;
    let n = &a.nodes[idx as usize];
    let base = a.path_of(idx);
    let sep = if base.ends_with('\\') { "" } else { "\\" };
    let items: Vec<Item> = n
        .children
        .iter()
        .take(limit)
        .map(|&c| {
            let ch = &a.nodes[c as usize];
            Item {
                name: ch.name.to_string(),
                path: if ch.kind == Kind::Small { base.clone() } else { format!("{base}{sep}{}", ch.name) },
                kind: ch.kind,
                size: ch.size,
                files: ch.files,
                age_days: age_of(ch.newest),
            }
        })
        .collect();
    let hidden = n.children.len().saturating_sub(limit);
    let hidden_size = n.children.iter().skip(limit).map(|&c| a.nodes[c as usize].size).sum();
    Ok(Listing { path: base, name: n.name.to_string(), size: n.size, files: n.files, items, hidden_count: hidden, hidden_size })
}

/// Top N fichiers ou dossiers sous `path`. Dossiers: on écarte ceux dont un seul enfant
/// concentre >70 % (le vrai gros est plus bas), pour éviter la redondance parent/enfant.
#[tauri::command]
fn top_items(st: State<AppState>, path: String, kind: String, limit: usize, ext: Option<String>, min_age_days: Option<u32>) -> R<Vec<Item>> {
    let m = st.arenas.lock().unwrap();
    let a = arena_for(&m, &path)?;
    let root = a.resolve(&path).ok_or("Dossier introuvable (supprimé ?)")?;
    let want_dir = kind == "dir";
    let exts: Vec<String> = ext.map(|e| e.split(',').map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let mut found: Vec<(u64, u32)> = Vec::new();
    let mut stack = vec![root];
    while let Some(i) = stack.pop() {
        let n = &a.nodes[i as usize];
        match n.kind {
            Kind::File if !want_dir => {
                let ok_ext = exts.is_empty() || n.name.rsplit_once('.').map_or(false, |(_, x)| exts.contains(&x.to_ascii_lowercase()));
                let ok_age = min_age_days.map_or(true, |d| age_of(n.newest).map_or(false, |x| x >= d));
                if ok_ext && ok_age { found.push((n.size, i)); }
            }
            Kind::Dir => {
                if want_dir && i != root {
                    let big_child = n.children.iter().map(|&c| a.nodes[c as usize].size).max().unwrap_or(0);
                    let concentrated = a.nodes[n.children.iter().copied().max_by_key(|&c| a.nodes[c as usize].size).unwrap_or(i) as usize].kind == Kind::Dir
                        && big_child * 10 > n.size * 7;
                    let ok_age = min_age_days.map_or(true, |d| age_of(n.newest).map_or(false, |x| x >= d));
                    if !concentrated && ok_age { found.push((n.size, i)); }
                }
                stack.extend(n.children.iter().copied());
            }
            _ => {}
        }
    }
    found.sort_by(|x, y| y.0.cmp(&x.0));
    Ok(found.into_iter().take(limit).map(|(_, i)| {
        let n = &a.nodes[i as usize];
        Item { name: n.name.to_string(), path: a.path_of(i), kind: n.kind, size: n.size, files: n.files, age_days: age_of(n.newest) }
    }).collect())
}

#[tauri::command]
fn reveal(path: String) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn();
}

// ---------- Nettoyage déterministe ----------

#[tauri::command]
async fn candidates(st: State<'_, AppState>) -> R<Vec<Candidate>> {
    let list = {
        let m = st.arenas.lock().unwrap();
        clean::find_candidates(&m)
    };
    Ok(list)
}

#[tauri::command]
async fn duplicates(app: tauri::AppHandle, st: State<'_, AppState>, min_mb: u64) -> R<Vec<DupGroup>> {
    let _ = app.emit("dup-start", ());
    let sizes = {
        let m = st.arenas.lock().unwrap();
        clean::dup_candidates(&m, (min_mb.max(4)) * 1024 * 1024)
    };
    tauri::async_runtime::spawn_blocking(move || clean::find_duplicates(sizes))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct DelItem {
    path: String,
    size: u64,
}

#[tauri::command]
async fn delete_paths(st: State<'_, AppState>, items: Vec<DelItem>, trash: bool) -> R<Vec<DeleteResult>> {
    let log = store::data_file(&st.dir, "deletions.log");
    let results: Vec<DeleteResult> = tauri::async_runtime::spawn_blocking(move || {
        items.iter().map(|it| clean::delete_path(&it.path, it.size, &log, trash)).collect()
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut m = st.arenas.lock().unwrap();
    for r in results.iter().filter(|r| r.ok) {
        let key = r.path.split('\\').next().unwrap_or("").to_uppercase();
        if let Some(a) = m.get_mut(&key) {
            if let Some(i) = a.resolve(&r.path) {
                a.remove(i);
            }
        }
    }
    Ok(results)
}

#[tauri::command]
fn protection_check(paths: Vec<String>) -> Vec<Option<String>> {
    paths.iter().map(|p| clean::protected(p)).collect()
}

// ---------- Rapport local et planification explicite ----------

#[tauri::command]
async fn mensuel_etat(st: State<'_, AppState>) -> R<rapport_mensuel::EtatMensuel> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || rapport_mensuel::etat(&dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn mensuel_generer(st: State<'_, AppState>) -> R<rapport_mensuel::ResumeMensuel> {
    rapport_mensuel::generer(&st.dir).await
}

#[tauri::command]
async fn mensuel_planifier(st: State<'_, AppState>, active: bool, seuil_libre_pct: u8) -> R<rapport_mensuel::EtatMensuel> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || rapport_mensuel::planifier(&dir, active, seuil_libre_pct)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn mensuel_ouvrir(st: State<AppState>, id: String, format: String) -> R<()> {
    rapport_mensuel::ouvrir(&st.dir, &id, &format)
}

/// Traiter la CLI avant de construire l'application évite une fenêtre pour la tâche mensuelle.
pub fn traiter_cli(args: Vec<String>) -> Option<Result<String, String>> {
    cli::traiter(args)
}

// ---------- Réglages ----------

#[derive(Serialize)]
struct ProviderDto {
    id: String,
    name: String,
    kind: String,
    base_url: String,
    model: String,
    has_key: bool,
}

#[derive(Serialize)]
struct SettingsDto {
    providers: Vec<ProviderDto>,
    active: String,
    llama: store::Llama,
}

fn dto(s: &Settings) -> SettingsDto {
    SettingsDto {
        providers: s
            .providers
            .iter()
            .map(|p| ProviderDto {
                id: p.id.clone(),
                name: p.name.clone(),
                kind: p.kind.clone(),
                base_url: p.base_url.clone(),
                model: p.model.clone(),
                has_key: !p.key_enc.is_empty(),
            })
            .collect(),
        active: s.active.clone(),
        llama: s.llama.clone(),
    }
}

#[tauri::command]
fn get_settings(st: State<AppState>) -> SettingsDto {
    dto(&st.settings.lock().unwrap())
}

#[derive(Deserialize)]
struct ProviderIn {
    id: String,
    base_url: String,
    model: String,
}

#[tauri::command]
fn save_settings(st: State<AppState>, providers: Vec<ProviderIn>, active: String, llama: store::Llama) -> R<SettingsDto> {
    let mut s = st.settings.lock().unwrap();
    for pi in providers {
        if let Some(p) = s.providers.iter_mut().find(|p| p.id == pi.id) {
            p.base_url = pi.base_url;
            p.model = pi.model;
        }
    }
    s.active = active;
    s.llama = llama;
    store::save(&st.dir, &s)?;
    Ok(dto(&s))
}

#[tauri::command]
fn set_key(st: State<AppState>, id: String, key: String) -> R<SettingsDto> {
    let mut s = st.settings.lock().unwrap();
    let enc = if key.is_empty() { String::new() } else { store::protect(&key)? };
    if let Some(p) = s.providers.iter_mut().find(|p| p.id == id) {
        p.key_enc = enc;
    }
    store::save(&st.dir, &s)?;
    Ok(dto(&s))
}

#[tauri::command]
async fn test_provider(st: State<'_, AppState>, id: String) -> R<String> {
    let p = provider(&st, Some(&id))?;
    let msgs = vec![llm::Msg { role: "user".into(), content: "Réponds uniquement par le mot OK.".into() }];
    llm::complete(&p, "Tu es un assistant concis.", msgs).await
}

fn provider(st: &AppState, id: Option<&str>) -> R<store::Provider> {
    let s = st.settings.lock().unwrap();
    let want = id.unwrap_or(&s.active);
    s.providers.iter().find(|p| p.id == want).cloned().ok_or_else(|| "Fournisseur introuvable".to_string())
}

// ---------- llama.cpp ----------

#[tauri::command]
fn llama_models(dir: String) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().map_or(false, |x| x.eq_ignore_ascii_case("gguf")))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[tauri::command]
fn llama_start(st: State<AppState>) -> R<String> {
    use std::os::windows::process::CommandExt;
    let l = st.settings.lock().unwrap().llama.clone();
    if l.exe.is_empty() || !std::path::Path::new(&l.exe).exists() {
        return Err("llama-server.exe introuvable: renseigne son chemin dans les réglages.".into());
    }
    if l.model_file.is_empty() {
        return Err("Choisis un modèle .gguf.".into());
    }
    let mut g = st.llama.lock().unwrap();
    if let Some(c) = g.as_mut() {
        if c.try_wait().map_err(|e| e.to_string())?.is_none() {
            return Err("llama-server tourne déjà.".into());
        }
    }
    let model = PathBuf::from(&l.models_dir).join(&l.model_file);
    let child = std::process::Command::new(&l.exe)
        .args(["-m"])
        .arg(&model)
        .args(["--port", &l.port.to_string(), "--ctx-size", &l.ctx.to_string(), "--jinja", "-ngl", "99"])
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|e| e.to_string())?;
    *g = Some(child);
    Ok(format!("llama-server démarré sur le port {} (chargement du modèle en cours…)", l.port))
}

#[tauri::command]
fn llama_stop(st: State<AppState>) {
    if let Some(mut c) = st.llama.lock().unwrap().take() {
        let _ = c.kill();
    }
}

// ---------- IA ----------

const PRIVACY: &str = "Tu reçois uniquement des noms, chemins, tailles et dates: jamais le contenu des fichiers.";

fn human(b: u64) -> String {
    let u = ["o", "Ko", "Mo", "Go", "To"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, u[i])
}

#[tauri::command]
async fn ai_explain(st: State<'_, AppState>, path: String) -> R<serde_json::Value> {
    let ctx = {
        let m = st.arenas.lock().unwrap();
        let a = arena_for(&m, &path)?;
        let i = a.resolve(&path).ok_or("Introuvable")?;
        let n = &a.nodes[i as usize];
        let kids: Vec<String> = n
            .children
            .iter()
            .take(14)
            .map(|&c| format!("- {} ({})", a.nodes[c as usize].name, human(a.nodes[c as usize].size)))
            .collect();
        format!(
            "Chemin: {}\nType: {}\nTaille: {}\nFichiers: {}\nDernière modification: il y a {} jours\nContenu principal:\n{}",
            a.path_of(i),
            if n.kind == Kind::Dir { "dossier" } else { "fichier" },
            human(n.size),
            n.files,
            age_of(n.newest).map_or("?".to_string(), |d| d.to_string()),
            kids.join("\n")
        )
    };
    let p = provider(&st, None)?;
    let system = format!(
        "Tu es un expert Windows qui aide à libérer de l'espace disque en sécurité. {PRIVACY}\n\
         Réponds UNIQUEMENT par un objet JSON, sans texte autour, de la forme:\n\
         {{\"resume\":\"2 phrases max: à quoi sert cet élément\",\"risque\":\"sans_risque|faible|moyen|eleve\",\"supprimable\":true|false,\"se_regenere\":true|false,\"conseil\":\"action recommandée, 1 phrase\"}}\n\
         Sois prudent: en cas de doute, risque \"moyen\" ou \"eleve\" et supprimable=false."
    );
    let out = llm::complete(&p, &system, vec![llm::Msg { role: "user".into(), content: ctx }]).await?;
    Ok(llm::extract_json(&out).unwrap_or_else(|| serde_json::json!({"resume": out, "risque": "moyen", "supprimable": false, "se_regenere": false, "conseil": "Réponse non structurée du modèle."})))
}

#[derive(Serialize)]
struct ChatReply {
    message: String,
    picks: Vec<Pick>,
}

#[derive(Serialize)]
struct Pick {
    id: String,
    path: String,
    size: u64,
    category: String,
    risk: &'static str,
    note: String,
}

fn build_ai_items(st: &AppState) -> Vec<Candidate> {
    let m = st.arenas.lock().unwrap();
    let mut items: Vec<Candidate> = clean::find_candidates(&m).into_iter().take(80).collect();
    let mut extra: Vec<(String, u64, bool)> = Vec::new();
    for a in m.values() {
        let mut stack = vec![(0u32, 0u32)];
        while let Some((i, d)) = stack.pop() {
            let n = &a.nodes[i as usize];
            if n.kind == Kind::Small {
                continue;
            }
            let big = (n.kind == Kind::File && n.size >= 1024 * 1024 * 1024)
                || (n.kind == Kind::Dir && (2..=4).contains(&d) && n.size >= 2 * 1024 * 1024 * 1024);
            if big {
                let p = a.path_of(i);
                if clean::protected(&p).is_none() {
                    extra.push((p, n.size, n.kind == Kind::File));
                }
            }
            if n.kind == Kind::Dir && d < 4 {
                stack.extend(n.children.iter().map(|&c| (c, d + 1)));
            }
        }
    }
    extra.sort_by(|a, b| b.1.cmp(&a.1));
    let taken: Vec<String> = items.iter().map(|c| c.path.to_lowercase()).collect();
    let mut k = 0;
    for (p, size, is_file) in extra {
        let pl = p.to_lowercase();
        if taken.iter().any(|t| pl.starts_with(t.as_str()) || t.starts_with(&pl)) {
            continue;
        }
        k += 1;
        if k > 40 {
            break;
        }
        items.push(Candidate {
            id: format!("e{k}"),
            category: if is_file { "Gros fichier".into() } else { "Gros dossier".into() },
            path: p,
            size,
            risk: "medium",
            reason: "Élément volumineux à examiner".into(),
            age_days: None,
        });
    }
    items
}

#[tauri::command]
async fn ai_chat(st: State<'_, AppState>, history: Vec<llm::Msg>) -> R<ChatReply> {
    let items = build_ai_items(&st);
    let mut listing = String::new();
    for c in &items {
        listing.push_str(&format!(
            "{} | {} | {} | {} | risque:{} | âge:{} | {}\n",
            c.id, c.category, c.path, human(c.size), c.risk,
            c.age_days.map_or("?".to_string(), |d| format!("{d}j")), c.reason
        ));
    }
    let disks = list_drives_inner();
    *st.ai_items.lock().unwrap() = items.iter().map(|c| (c.id.clone(), c.clone())).collect();
    let system = format!(
        "Tu es l'assistant de nettoyage de disque de l'utilisateur (Windows). {PRIVACY}\n\
         Disques: {disks}\n\
         Éléments proposables (id | catégorie | chemin | taille | risque | âge | raison):\n{listing}\n\
         RÈGLES: ne propose QUE des ids de cette liste. Respecte strictement les contraintes de l'utilisateur (dossiers à ne pas toucher, quantité visée). \
         Préfère d'abord les éléments sans risque / se régénérant. Ne fais jamais la suppression toi-même: tu proposes, l'utilisateur valide.\n\
         Réponds UNIQUEMENT par un objet JSON: {{\"message\":\"réponse courte en français, markdown léger\",\"select\":[\"c1\",\"e2\"],\"notes\":{{\"c1\":\"pourquoi, 8 mots max\"}}}}\n\
         Si l'utilisateur pose une simple question, laisse select vide."
    );
    let p = provider(&st, None)?;
    let out = llm::complete(&p, &system, history).await?;
    let map = st.ai_items.lock().unwrap();
    let Some(v) = llm::extract_json(&out) else {
        return Ok(ChatReply { message: out, picks: vec![] });
    };
    let notes = v["notes"].as_object();
    let picks = v["select"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .filter_map(|id| map.get(id))
                .filter(|c| clean::protected(&c.path).is_none())
                .map(|c| Pick {
                    id: c.id.clone(),
                    path: c.path.clone(),
                    size: c.size,
                    category: c.category.clone(),
                    risk: c.risk,
                    note: notes.and_then(|n| n.get(&c.id)).and_then(|s| s.as_str()).unwrap_or(&c.reason).to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ChatReply { message: v["message"].as_str().unwrap_or(&out).to_string(), picks })
}

fn list_drives_inner() -> String {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| format!("{} libre {} / {}", d.mount_point().to_string_lossy(), human(d.available_space()), human(d.total_space())))
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Serialize)]
struct Growth {
    path: String,
    size: u64,
    delta: i64,
}

#[derive(Serialize)]
struct ReportData {
    key: String,
    history: Vec<(u64, u64, u64)>,
    prev_ts: Option<u64>,
    cur_ts: Option<u64>,
    growth: Vec<Growth>,
}

fn load_snaps(dir: &PathBuf, key: &str) -> Vec<Snapshot> {
    let file = store::data_file(dir, &format!("snapshots_{}.json", key.replace(':', "")));
    std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[tauri::command]
fn report_data(st: State<AppState>, key: String) -> ReportData {
    let snaps = load_snaps(&st.dir, &key);
    let history = snaps.iter().map(|s| (s.ts, s.total, s.free)).collect();
    let mut growth = Vec::new();
    let (mut prev_ts, mut cur_ts) = (None, None);
    if snaps.len() >= 2 {
        let (prev, cur) = (&snaps[snaps.len() - 2], &snaps[snaps.len() - 1]);
        prev_ts = Some(prev.ts);
        cur_ts = Some(cur.ts);
        let old: HashMap<&str, u64> = prev.dirs.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        for (p, s) in &cur.dirs {
            let delta = *s as i64 - old.get(p.as_str()).copied().unwrap_or(0) as i64;
            if delta.abs() > 100 * 1024 * 1024 {
                growth.push(Growth { path: p.clone(), size: *s, delta });
            }
        }
        growth.sort_by(|a, b| b.delta.abs().cmp(&a.delta.abs()));
        growth.truncate(15);
    }
    ReportData { key, history, prev_ts, cur_ts, growth }
}

#[tauri::command]
async fn ai_report(st: State<'_, AppState>, key: String) -> R<String> {
    let snaps = load_snaps(&st.dir, &key);
    if snaps.is_empty() {
        return Err("Aucune analyse enregistrée pour ce disque.".into());
    }
    let mut ctx = format!("Disque {key}\nHistorique (date epoch, utilisé, libre):\n");
    for s in snaps.iter().rev().take(12).rev() {
        ctx.push_str(&format!("- {} | utilisé {} | libre {}\n", s.ts, human(s.total.saturating_sub(s.free)), human(s.free)));
    }
    let rd = report_data(st.clone(), key.clone());
    if rd.growth.is_empty() {
        ctx.push_str("\nPas encore de comparaison (une seule analyse ou peu de changements).\n");
    } else {
        ctx.push_str("\nPlus fortes variations depuis l'analyse précédente:\n");
        for g in &rd.growth {
            ctx.push_str(&format!("- {} : {} ({}{})\n", g.path, human(g.size), if g.delta >= 0 { "+" } else { "-" }, human(g.delta.unsigned_abs())));
        }
    }
    let cands = {
        let m = st.arenas.lock().unwrap();
        clean::find_candidates(&m)
    };
    let recoverable: u64 = cands.iter().filter(|c| c.risk != "medium").map(|c| c.size).sum();
    ctx.push_str(&format!("\nEspace récupérable sans risque notable (caches, builds): {}\n", human(recoverable)));
    for c in cands.iter().take(8) {
        ctx.push_str(&format!("- {} : {} ({})\n", c.path, human(c.size), c.category));
    }
    let system = format!(
        "Tu rédiges un rapport hebdomadaire d'état de disque pour l'utilisateur. {PRIVACY}\n\
         En français, markdown léger, 6 à 10 lignes: 1) état global et tendance, 2) ce qui a grossi/diminué, 3) anomalies éventuelles, 4) 3 actions concrètes priorisées avec gains estimés. N'invente aucun chiffre absent des données."
    );
    let p = provider(&st, None)?;
    llm::complete(&p, &system, vec![llm::Msg { role: "user".into(), content: ctx }]).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("app data dir");
            let settings = store::load(&dir);
            app.manage(AppState {
                arenas: Mutex::new(HashMap::new()),
                cancel: Arc::new(AtomicBool::new(false)),
                scanning: AtomicBool::new(false),
                settings: Mutex::new(settings),
                dir,
                llama: Mutex::new(None),
                ai_items: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::Destroyed = e {
                if let Some(st) = w.app_handle().try_state::<AppState>() {
                    if let Some(mut c) = st.llama.lock().unwrap().take() {
                        let _ = c.kill();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_drives, start_scan, cancel_scan, get_children, top_items, reveal, candidates, duplicates,
            delete_paths, protection_check, get_settings, save_settings, set_key, test_provider,
            llama_models, llama_start, llama_stop, ai_explain, ai_chat, report_data, ai_report,
            mensuel_etat, mensuel_generer, mensuel_planifier, mensuel_ouvrir,
            residus::logiciel_desinstaller, residus::logiciel_residus_apres,
            residus::residus_scan, residus::residus_chercher, residus::residus_clean, residus::residus_lots, residus::residus_purge_lot,
            onedrive::onedrive_scan, onedrive::onedrive_liberer,
            demarrage::demarrage_liste, demarrage::demarrage_appliquer, sante::sante_processus, sante::sante_protection, sante::sante_securite, sauvegarde::sauvegarde_etat, sauvegarde::sauvegarde_ouvrir,
            registre::registre_scan, registre::registre_point_restauration, registre::registre_clean
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de DiskLens");
}
