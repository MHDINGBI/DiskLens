use crate::scan::{Arena, Kind};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Clone)]
pub struct Candidate {
    pub id: String,
    pub category: String,
    pub path: String,
    pub size: u64,
    /// safe = se régénère seul, low = se réinstalle, medium = à vérifier
    pub risk: &'static str,
    pub reason: String,
    pub age_days: Option<u32>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn age_days(ts: u32) -> Option<u32> {
    if ts == 0 {
        return None;
    }
    Some(((now().saturating_sub(ts as u64)) / 86_400) as u32)
}

/// Chemins connus (relatifs au profil utilisateur) -> (catégorie, risque, raison).
fn known_user_paths() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("AppData\\Local\\Temp", "Fichiers temporaires", "safe", "Dossier TEMP de l'utilisateur, recréé par Windows."),
        ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache", "Cache navigateur", "safe", "Cache Chrome, se reconstruit en naviguant."),
        ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Code Cache", "Cache navigateur", "safe", "Cache de code JS Chrome."),
        ("AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache", "Cache navigateur", "safe", "Cache Edge, se reconstruit."),
        ("AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Code Cache", "Cache navigateur", "safe", "Cache de code JS Edge."),
        ("AppData\\Local\\npm-cache", "Cache de paquets", "safe", "Cache npm, retéléchargé au besoin."),
        ("AppData\\Local\\pnpm\\store", "Cache de paquets", "low", "Store pnpm: les projets liés devront retélécharger."),
        ("AppData\\Local\\pip\\Cache", "Cache de paquets", "safe", "Cache pip."),
        ("AppData\\Local\\CrashDumps", "Rapports d'erreur", "safe", "Dumps de plantage."),
        ("AppData\\Local\\NVIDIA\\DXCache", "Cache graphique", "safe", "Shaders recompilés au besoin."),
        ("AppData\\Local\\D3DSCache", "Cache graphique", "safe", "Cache de shaders DirectX."),
        (".cargo\\registry", "Cache de paquets", "low", "Sources/crates Cargo, retéléchargés au prochain build."),
        (".nuget\\packages", "Cache de paquets", "low", "Paquets NuGet, restaurés à la compilation."),
        (".gradle\\caches", "Cache de paquets", "low", "Cache Gradle."),
        (".claude\\projects", "Cache des harnais IA", "medium", "Transcripts de sessions Claude Code; supprimer perd l'historique /resume."),
        (".claude\\file-history", "Cache des harnais IA", "low", "Sauvegardes de fichiers modifiés par Claude Code (annulation)."),
        (".claude\\shell-snapshots", "Cache des harnais IA", "safe", "Instantanés de shell Claude Code, recréés à chaque session."),
        (".claude\\debug", "Cache des harnais IA", "safe", "Logs de debug Claude Code."),
        (".claude\\todos", "Cache des harnais IA", "safe", "Listes de tâches de sessions passées."),
        (".claude\\paste-cache", "Cache des harnais IA", "safe", "Contenus collés mis en cache."),
        (".claude\\image-cache", "Cache des harnais IA", "safe", "Images collées mises en cache."),
        (".claude\\statsig", "Cache des harnais IA", "safe", "Cache de télémétrie/feature flags."),
        ("AppData\\Local\\claude-cli-nodejs", "Cache des harnais IA", "safe", "Cache et logs du CLI Claude."),
        (".codex\\sessions", "Cache des harnais IA", "medium", "Historique des sessions Codex."),
        (".codex\\log", "Cache des harnais IA", "safe", "Logs Codex."),
        (".local\\share\\opencode", "Cache des harnais IA", "medium", "Sessions et données OpenCode."),
        (".hermes\\sessions", "Cache des harnais IA", "medium", "Historique des sessions Hermes."),
        (".hermes\\logs", "Cache des harnais IA", "safe", "Logs Hermes."),
        (".gemini\\tmp", "Cache des harnais IA", "safe", "Fichiers temporaires Gemini CLI."),
        (".cache", "Cache de paquets", "low", "Caches divers d'outils CLI (modèles HuggingFace inclus: vérifier)."),
    ]
}

fn known_system_paths() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("C:\\Windows\\Temp", "Fichiers temporaires", "safe", "TEMP système."),
        ("C:\\Windows\\SoftwareDistribution\\Download", "Windows Update", "safe", "Mises à jour déjà installées (nécessite droits admin)."),
        ("C:\\ProgramData\\Microsoft\\Windows\\WER", "Rapports d'erreur", "safe", "Rapports d'erreurs Windows."),
        ("C:\\Windows.old", "Ancienne installation Windows", "medium", "Reste d'une mise à niveau Windows; supprimer via Nettoyage de disque (droits admin)."),
        ("C:\\$WINDOWS.~BT", "Windows Update", "low", "Fichiers d'installation d'une mise à niveau Windows."),
    ]
}

pub fn find_candidates(arenas: &HashMap<String, Arena>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let user = std::env::var("USERPROFILE").unwrap_or_default();
    let push = |out: &mut Vec<Candidate>, cat: &str, path: String, size: u64, risk, reason: String, age: Option<u32>| {
        if size < 1024 * 1024 {
            return;
        }
        let id = format!("c{}", out.len() + 1);
        out.push(Candidate { id, category: cat.into(), path, size, risk, reason, age_days: age });
    };

    for arena in arenas.values() {
        // 1) Chemins connus
        let mut known: Vec<(String, &str, &str, &str)> = known_system_paths()
            .into_iter()
            .map(|(p, c, r, w)| (p.to_string(), c, r, w))
            .collect();
        if !user.is_empty() {
            for (rel, c, r, w) in known_user_paths() {
                known.push((format!("{user}\\{rel}"), c, r, w));
            }
        }
        for (p, c, r, w) in known {
            if let Some(i) = arena.resolve(&p) {
                let n = &arena.nodes[i as usize];
                push(&mut out, c, p, n.size, risk_str(r), w.to_string(), None);
            }
        }

        // 2) Parcours: dossiers régénérables, gros fichiers anciens
        let mut stack: Vec<u32> = vec![0];
        while let Some(i) = stack.pop() {
            let n = &arena.nodes[i as usize];
            if n.kind == Kind::File {
                let age = age_days(n.newest);
                let path = arena.path_of(i);
                let in_dl = path.to_lowercase().contains("\\downloads\\");
                let lname = n.name.to_ascii_lowercase();
                let installer = lname.ends_with(".exe") || lname.ends_with(".msi") || lname.ends_with(".msix");
                if lname.ends_with(".vhdx") && n.size >= 1024 * 1024 * 1024 && (lname == "ext4.vhdx" || lname.contains("docker")) {
                    push(&mut out, "Disque virtuel WSL/Docker", path, n.size, "medium",
                        "Disque virtuel WSL/Docker; ne se réduit pas seul (compacter avec diskpart/Optimize-VHD).".into(), age);
                } else if in_dl && installer && n.size >= 20 * 1024 * 1024 && age.map_or(false, |a| a > 30) {
                    push(&mut out, "Installeurs téléchargés", path, n.size, "low",
                        "Ancien installateur dans Téléchargements (plus de 30 jours) ; installation non vérifiée.".into(), age);
                } else if n.size >= 200 * 1024 * 1024 && age.map_or(false, |a| a > 365) {
                    push(&mut out, "Gros fichiers inactifs", path, n.size, "medium",
                        "Gros fichier non modifié depuis plus d'un an.".into(), age);
                } else if in_dl && n.size >= 100 * 1024 * 1024 && age.map_or(false, |a| a > 90) {
                    push(&mut out, "Téléchargements volumineux", path, n.size, "medium",
                        "Gros fichier dans Téléchargements, vieux de plus de 90 jours.".into(), age);
                }
                continue;
            }
            if n.kind != Kind::Dir {
                continue;
            }
            let name = n.name.to_ascii_lowercase();
            let has_child = |c: &str| n.children.iter().any(|&k| arena.nodes[k as usize].name.eq_ignore_ascii_case(c));
            let regen: Option<(&str, &'static str, &str)> = match name.as_str() {
                "node_modules" => Some(("Dépendances (node_modules)", "low", "Se réinstalle avec npm/pnpm install.")),
                "target" if has_child("debug") || has_child("release") => {
                    Some(("Build Rust (target)", "low", "Artefacts de compilation Cargo, régénérés par cargo build."))
                }
                "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache" | ".tox" => {
                    Some(("Cache de build", "safe", "Cache Python régénéré automatiquement."))
                }
                "cache" | ".cache" | "code cache" | "gpucache" | "cacheddata" | "shadercache" | "dxcache" | "glcache" => {
                    Some(("Cache de build", "safe", "Dossier de cache, recréé automatiquement par l'application."))
                }
                ".next" | ".nuxt" | ".turbo" | ".parcel-cache" => {
                    Some(("Cache de build", "safe", "Cache de framework web, régénéré au build."))
                }
                ".venv" | "venv" if has_child("scripts") || has_child("lib") || has_child("bin") => {
                    Some(("Environnement Python", "low", "Venv recréable avec pip install -r requirements."))
                }
                _ => None,
            };
            if let Some((cat, risk, why)) = regen {
                // Activité du projet = le plus récent entre ses fichiers propres et ses frères (hors candidat)
                let parent = n.parent;
                let (mut act, mut proj) = (0u32, String::new());
                if parent != u32::MAX {
                    let p = &arena.nodes[parent as usize];
                    act = p.own_newest;
                    for &k in &p.children {
                        if k != i {
                            act = act.max(arena.nodes[k as usize].newest);
                        }
                    }
                    proj = arena.path_of(parent);
                }
                let age = age_days(act);
                let mut reason = why.to_string();
                if let Some(a) = age {
                    if a > 180 {
                        reason = format!("{why} Projet inactif depuis {a} jours ({proj}).");
                    }
                }
                push(&mut out, cat, arena.path_of(i), n.size, risk_str(risk), reason, age);
                continue; // ne pas descendre dans un candidat
            }
            stack.extend(n.children.iter().copied());
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    for (k, c) in out.iter_mut().enumerate() {
        c.id = format!("c{}", k + 1);
    }
    out
}

fn risk_str(r: &str) -> &'static str {
    match r {
        "safe" => "safe",
        "low" => "low",
        _ => "medium",
    }
}

// ---------- Doublons ----------

#[derive(Serialize)]
pub struct DupGroup {
    pub size: u64,
    pub wasted: u64,
    pub paths: Vec<String>,
}

fn hash_partial(path: &str, size: u64) -> Option<[u8; 32]> {
    let mut f = fs::File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    let n = f.read(&mut buf).ok()?;
    h.update(&buf[..n]);
    if size > 128 * 1024 {
        f.seek(SeekFrom::Start(size / 2)).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
        f.seek(SeekFrom::End(-(64 * 1024))).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

fn hash_full(path: &str) -> Option<[u8; 32]> {
    let mut f = fs::File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

/// Groupes de fichiers de même taille (étape rapide, sous verrou).
pub fn dup_candidates(arenas: &HashMap<String, Arena>, min_size: u64) -> Vec<(u64, Vec<String>)> {
    let mut by_size: HashMap<u64, Vec<String>> = HashMap::new();
    for arena in arenas.values() {
        for (i, n) in arena.nodes.iter().enumerate() {
            if n.kind == Kind::File && n.size >= min_size {
                by_size.entry(n.size).or_default().push(arena.path_of(i as u32));
            }
        }
    }
    by_size.into_iter().filter(|(_, v)| v.len() > 1).collect()
}

pub fn find_duplicates(groups: Vec<(u64, Vec<String>)>) -> Vec<DupGroup> {
    use rayon::prelude::*;
    let mut out: Vec<DupGroup> = groups
        .into_par_iter()
        .flat_map(|(size, paths)| {
            // les tailles sont allouées (multiples de 4 Ko): on départage par hash partiel puis complet
            let mut by_part: HashMap<[u8; 32], Vec<String>> = HashMap::new();
            for p in paths {
                let real = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                if let Some(h) = hash_partial(&p, real) {
                    by_part.entry(h).or_default().push(p);
                }
            }
            let mut res = Vec::new();
            for (_, ps) in by_part.into_iter().filter(|(_, v)| v.len() > 1) {
                let mut by_full: HashMap<[u8; 32], Vec<String>> = HashMap::new();
                for p in ps {
                    if let Some(h) = hash_full(&p) {
                        by_full.entry(h).or_default().push(p);
                    }
                }
                for (_, mut same) in by_full.into_iter().filter(|(_, v)| v.len() > 1) {
                    same.sort();
                    res.push(DupGroup { size, wasted: size * (same.len() as u64 - 1), paths: same });
                }
            }
            res
        })
        .collect();
    out.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    out
}

// ---------- Suppression ----------

fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn sans_prefixe_verbatim(p: &str) -> &str {
    p.strip_prefix(r"\\?\").unwrap_or(p)
}

/// Valide un chemin avant une action sur le disque. Les composants inexistants en fin de
/// chemin sont admis pour les destinations de quarantaine ; les ancêtres doivent exister.
pub(crate) fn validate_action_path(path: &str) -> Result<String, String> {
    use std::os::windows::fs::MetadataExt;

    let input = path.replace('/', "\\");
    let b = input.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return Err("chemin local absolu requis".into());
    }
    if input.chars().any(|c| c.is_control()) || input[3..].contains(':') {
        return Err("chemin Windows invalide".into());
    }
    let parts: Vec<&str> = input[3..].split('\\').collect();
    if parts.iter().any(|p| *p == "." || *p == ".." || p.ends_with('.') || p.ends_with(' ')) {
        return Err("composant de chemin ambigu".into());
    }
    if parts.iter().take(parts.len().saturating_sub(1)).any(|p| p.is_empty()) {
        return Err("séparateur de chemin ambigu".into());
    }

    let mut existing = PathBuf::from(&input);
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err("racine de disque introuvable".into());
                };
                missing.push(name.to_os_string());
                if !existing.pop() {
                    return Err("racine de disque introuvable".into());
                }
            }
            Err(e) => return Err(format!("chemin inaccessible : {e}")),
        }
    }
    for ancestor in existing.ancestors() {
        let md = fs::symlink_metadata(ancestor).map_err(|e| format!("ancêtre inaccessible : {e}"))?;
        if md.file_attributes() & 0x400 != 0 {
            return Err("point de jonction ou lien de réanalyse dans le chemin".into());
        }
    }
    let mut result = existing.canonicalize().map_err(|e| format!("chemin inaccessible : {e}"))?;
    for part in missing.iter().rev() {
        result.push(part);
    }
    let normalized = sans_prefixe_verbatim(&result.to_string_lossy()).to_string();
    let n = normalized.as_bytes();
    if n.len() < 3 || !n[0].is_ascii_alphabetic() || n[1] != b':' || n[2] != b'\\' {
        return Err("chemin local absolu requis".into());
    }
    Ok(normalized)
}

fn env_root(name: &str) -> String {
    let Ok(value) = std::env::var(name) else { return String::new() };
    let canonical = fs::canonicalize(&value).ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or(value);
    norm(sans_prefixe_verbatim(&canonical))
}

/// Retourne Some(raison) si le chemin ne doit jamais être supprimé.
pub fn protected(path: &str) -> Option<String> {
    let canonical = match validate_action_path(path) {
        Ok(p) => p,
        Err(e) => return Some(e),
    };
    let p = norm(&canonical);
    if p.len() <= 3 {
        return Some("racine de disque".into());
    }
    let user = env_root("USERPROFILE");
    let drive = &p[..2];
    let system_windows = {
        let root = env_root("SystemRoot");
        if root.is_empty() { env_root("WINDIR") } else { root }
    };
    let windows = format!("{drive}\\windows");
    let allowed_in_windows = [format!("{system_windows}\\temp"), format!("{system_windows}\\softwaredistribution\\download")];
    let under = |root: &str| p == root || p.starts_with(&format!("{root}\\"));

    if (under(&windows) || (!system_windows.is_empty() && under(&system_windows)))
        && !allowed_in_windows.iter().any(|a| p.starts_with(&format!("{a}\\"))) {
        return Some("dossier système Windows".into());
    }
    let wer = format!("{drive}\\programdata\\microsoft\\windows\\wer");
    let programdata = env_root("ProgramData");
    for r in [format!("{drive}\\program files"), format!("{drive}\\program files (x86)"), format!("{drive}\\programdata\\microsoft"), format!("{drive}\\recovery"), format!("{drive}\\boot"), format!("{drive}\\efi"), env_root("ProgramFiles"), env_root("ProgramFiles(x86)"), if programdata.is_empty() { String::new() } else { format!("{programdata}\\microsoft") }] {
        if !r.is_empty() && under(&r) && !p.starts_with(&format!("{wer}\\")) {
            return Some("programmes / système".into());
        }
    }
    let name = p.rsplit('\\').next().unwrap_or("");
    if p.contains("\\system volume information") || p.contains("\\$recycle.bin") {
        return Some("zone système protégée".into());
    }
    if ["pagefile.sys", "hiberfil.sys", "swapfile.sys", "bootmgr", "ntuser.dat"].contains(&name) {
        return Some("fichier système".into());
    }
    if !user.is_empty() {
        let protected_dirs = ["", "\\appdata", "\\appdata\\local", "\\appdata\\roaming", "\\desktop", "\\documents", "\\pictures", "\\videos", "\\music", "\\downloads", "\\onedrive"];
        if protected_dirs.iter().any(|d| p == format!("{user}{d}")) {
            return Some("dossier utilisateur racine (supprime son contenu à la place)".into());
        }
    }
    for root in [env_root("OneDrive"), env_root("OneDriveCommercial"), env_root("OneDriveConsumer"), if user.is_empty() { String::new() } else { format!("{user}\\onedrive") }] {
        if root.is_empty() { continue; }
        if p == root || ["desktop", "documents", "pictures", "images", "bureau"].iter().any(|name| p == format!("{root}\\{name}")) {
            return Some("dossier OneDrive ou dossier personnel sauvegardé".into());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if under(&norm(&dir.to_string_lossy())) {
                return Some("dossier de l'application".into());
            }
        }
    }
    None
}

#[derive(Serialize)]
pub struct DeleteResult {
    pub path: String,
    pub ok: bool,
    pub freed: u64,
    pub error: Option<String>,
}

/// Envoie à la corbeille (annulable) via SHFileOperation. Ne libère l'espace qu'au vidage de la corbeille.
fn to_recycle_bin(path: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{SHFileOperationW, SHFILEOPSTRUCTW};
    let mut from: Vec<u16> = std::ffi::OsStr::new(path).encode_wide().collect();
    from.extend([0, 0]); // double terminateur
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = 3; // FO_DELETE
    op.pFrom = from.as_ptr();
    op.fFlags = (0x0040 | 0x0010 | 0x0004 | 0x0400) as _; // ALLOWUNDO | NOCONFIRMATION | SILENT | NOERRORUI
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc == 0 && op.fAnyOperationsAborted == 0 { Ok(()) } else { Err(format!("Corbeille refusée (code {rc})")) }
}

pub fn delete_path(path: &str, size: u64, log: &Path, trash: bool) -> DeleteResult {
    if let Some(why) = protected(path) {
        return DeleteResult { path: path.into(), ok: false, freed: 0, error: Some(format!("Protégé: {why}")) };
    }
    let md = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => return DeleteResult { path: path.into(), ok: false, freed: 0, error: Some(e.to_string()) },
    };
    let res: Result<(), String> = if trash {
        to_recycle_bin(path)
    } else if md.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    };
    let (ok, error) = match res {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e)),
    };
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log) {
        let _ = writeln!(f, "{}\t{}\t{}\t{}", now(), if ok { if trash { "CORBEILLE" } else { "OK" } } else { "ECHEC" }, size, path);
    }
    DeleteResult { path: path.into(), ok, freed: if ok { size } else { 0 }, error }
}

#[cfg(test)]
mod tests {
    use super::{protected, validate_action_path};

    #[test]
    fn refuse_traversee_depuis_temp_windows() {
        assert!(protected(r"C:\Windows\Temp\..\System32\drivers").is_some());
    }

    #[test]
    fn refuse_chemin_relatif() {
        assert!(protected(r"AppData\Local\Temp\cache").is_some());
    }

    #[test]
    fn refuse_windows_sur_autre_volume() {
        assert!(protected(r"D:\Windows\System32\drivers").is_some());
    }

    #[test]
    fn accepte_destination_absente_sous_dossier_existant() {
        let path = std::env::temp_dir().join(format!("disklens-test-{}-absent", std::process::id()));
        let checked = validate_action_path(&path.to_string_lossy()).unwrap();
        assert!(checked.ends_with(&format!("disklens-test-{}-absent", std::process::id())));
    }

    #[test]
    fn protege_racine_de_dossier_one_drive_personnel() {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let desktop = std::path::Path::new(&profile).join("OneDrive").join("Desktop");
            assert!(protected(&desktop.to_string_lossy()).is_some());
        }
    }
}
