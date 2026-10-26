//! Démarrage de Windows : liste des programmes lancés à l'ouverture de session, et désactivation
//! **réversible** via StartupApproved ou le mode de démarrage des services. Rien n'est supprimé.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN32: &str = "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved";
const SERVICES: &str = "SYSTEM\\CurrentControlSet\\Services";
const SERVICES_STATE: &str = "demarrage_services.json";
static SERVICES_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Clone)]
struct ServiceSaved {
    start: u32,
    delayed: bool,
    commande: String,
    editeur: String,
}

struct ServiceConfig {
    entree: Entree,
    service_type: u32,
    start: u32,
    delayed: bool,
}

#[derive(Serialize, Clone)]
pub struct Entree {
    /// source|nom : identifiant stable passé à `demarrage_appliquer`
    pub id: String,
    pub nom: String,
    pub source: String,
    pub commande: String,
    pub chemin: String,
    pub editeur: String,
    pub signe: bool,
    pub actif: bool,
    /// garder = sécurité / pilote / Microsoft, facultatif = se lance sans être utile, inconnu = à juger
    pub suggestion: &'static str,
    pub note: String,
    /// Écrire dans HKLM ou une tâche système demande les droits administrateur
    pub admin: bool,
}

#[derive(Serialize)]
pub struct Resultat {
    pub id: String,
    pub ok: bool,
    pub detail: String,
}

/// Remplace les %VARIABLES% d'environnement.
pub fn developper(s: &str) -> String {
    let mut out = String::new();
    let mut reste = s;
    while let Some(a) = reste.find('%') {
        out.push_str(&reste[..a]);
        let apres = &reste[a + 1..];
        match apres.find('%') {
            Some(b) => {
                match std::env::var(&apres[..b]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => { out.push('%'); out.push_str(&apres[..b]); out.push('%'); }
                }
                reste = &apres[b + 1..];
            }
            None => { out.push('%'); out.push_str(apres); return out; }
        }
    }
    out.push_str(reste);
    out
}

/// Extrait le chemin de l'exécutable d'une ligne de commande.
pub fn extraire_exe(cmd: &str) -> String {
    let c = developper(cmd.trim());
    if let Some(r) = c.strip_prefix('"') {
        return r.split('"').next().unwrap_or("").to_string();
    }
    let bas = c.to_lowercase();
    match bas.find(".exe") {
        Some(i) => c[..i + 4].to_string(),
        None => c.split_whitespace().next().unwrap_or("").to_string(),
    }
}

/// Suggestion à partir du nom et de l'éditeur (mots entiers, pas le chemin) : sécurité/pilotes = garder.
pub fn suggerer(nom: &str, editeur: &str, _chemin: &str) -> (&'static str, String) {
    let tout = format!("{nom} {editeur}").to_lowercase();
    let mots: Vec<&str> = tout.split(|c: char| !c.is_alphanumeric()).filter(|m| !m.is_empty()).collect();
    let a = |k: &str| mots.contains(&k);
    // Applications Microsoft qui n'ont pas besoin de démarrer avec Windows
    if a("onedrive") {
        return ("facultatif", "Synchronisation OneDrive : désactive-la seulement si tu n'utilises pas OneDrive.".into());
    }
    if a("teams") || a("lists") || a("skype") || a("outlook") {
        return ("facultatif", "Application qui peut s'ouvrir à la demande plutôt qu'au démarrage.".into());
    }
    const GARDER: [&str; 15] = ["securityhealth", "defender", "intel", "nvidia", "amd", "realtek", "synaptics", "touchpad", "bluetooth", "antivirus", "kaspersky", "bitdefender", "eset", "rtkauduservice", "windows"];
    if GARDER.iter().any(|k| a(k)) || mots.iter().any(|m| m.starts_with("rtkaud") || m.starts_with("securityhealth")) || (a("microsoft") && !a("update")) {
        return ("garder", "Sécurité, pilote ou composant Microsoft : mieux vaut le laisser.".into());
    }
    const FACULTATIF: [&str; 13] = ["spotify", "discord", "steam", "zoom", "adobe", "acrobat", "epic", "dropbox", "update", "updater", "launcher", "assistant", "toolbar"];
    if FACULTATIF.iter().any(|k| a(k)) || mots.iter().any(|m| m.ends_with("updater") || m.ends_with("update")) {
        return ("facultatif", "Programme qui peut se lancer à la demande plutôt qu'au démarrage.".into());
    }
    ("inconnu", String::new())
}

/// 12 octets StartupApproved : 02 = activé, 03 = désactivé (+ horodatage FILETIME).
pub fn est_desactive(octets: &[u8]) -> bool {
    octets.first().map_or(false, |b| b & 1 == 1)
}

pub fn octets_etat(actif: bool, filetime: u64) -> Vec<u8> {
    let mut v = vec![0u8; 12];
    if actif {
        v[0] = 2;
    } else {
        v[0] = 3;
        v[4..12].copy_from_slice(&filetime.to_le_bytes());
    }
    v
}

fn filetime_maintenant() -> u64 {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (s + 11_644_473_600) * 10_000_000
}

fn lire_approuve(racine: &RegKey, sous: &str, nom: &str) -> bool {
    racine
        .open_subkey(format!("{APPROVED}\\{sous}"))
        .and_then(|k| k.get_raw_value(nom))
        .map(|v| est_desactive(&v.bytes))
        .unwrap_or(false)
}

fn ecrire_approuve(racine: &RegKey, sous: &str, nom: &str, actif: bool) -> Result<(), String> {
    let (k, _) = racine.create_subkey(format!("{APPROVED}\\{sous}")).map_err(|e| e.to_string())?;
    let val = winreg::RegValue { bytes: octets_etat(actif, filetime_maintenant()), vtype: REG_BINARY };
    k.set_raw_value(nom, &val).map_err(|e| e.to_string())
}

fn construire(source: &str, nom: &str, commande: &str, actif: bool, admin: bool) -> Entree {
    let chemin = if std::path::Path::new(commande).is_file() { commande.to_string() } else { extraire_exe(commande) };
    let (signe, editeur) = if !chemin.is_empty() && std::path::Path::new(&chemin).is_file() {
        (crate::sante::signature_valide(&chemin), crate::sante::editeur_fichier(&chemin))
    } else {
        (false, String::new())
    };
    let (suggestion, mut note) = suggerer(nom, &editeur, &chemin);
    if !chemin.is_empty() && !std::path::Path::new(&chemin).exists() {
        note = "Le fichier n'existe plus : l'entrée est orpheline.".into();
    }
    Entree { id: format!("{source}|{nom}"), nom: nom.into(), source: source.into(), commande: commande.into(), chemin, editeur, signe, actif, suggestion, note, admin }
}

fn dossiers_demarrage() -> Vec<(&'static str, PathBuf, bool)> {
    let mut v = Vec::new();
    if let Ok(a) = std::env::var("APPDATA") {
        v.push(("Dossier Démarrage (utilisateur)", PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup"), false));
    }
    if let Ok(p) = std::env::var("ProgramData") {
        v.push(("Dossier Démarrage (commun)", PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs\\StartUp"), true));
    }
    v
}

fn lire_etat_services(dir: &Path) -> Result<HashMap<String, ServiceSaved>, String> {
    match fs::read(dir.join(SERVICES_STATE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("état des services illisible : {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(format!("état des services inaccessible : {e}")),
    }
}

fn ecrire_etat_services(dir: &Path, etat: &HashMap<String, ServiceSaved>) -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(etat).map_err(|e| e.to_string())?;
    let dest = dir.join(SERVICES_STATE);
    match fs::metadata(&dest) {
        Ok(md) if md.permissions().readonly() => return Err("état des services en lecture seule : remplacement refusé".into()),
        Ok(_) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(format!("état des services inaccessible : {e}")),
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos()).unwrap_or(0);
    let temp = dir.join(format!(".{SERVICES_STATE}.{}.{nonce}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| e.to_string())?;
    if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(format!("écriture de l'état des services impossible : {e}"));
    }
    drop(file);
    let from: Vec<u16> = temp.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let to: Vec<u16> = dest.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        let _ = fs::remove_file(&temp);
        return Err(format!("enregistrement de l'état des services impossible : {e}"));
    }
    Ok(())
}

fn mode_restauration(saved: &ServiceSaved) -> &'static str {
    if saved.delayed { "delayed-auto" } else { "auto" }
}

fn service_eligible(service_type: u32, start: u32, saved: bool, entree: &Entree, windows: &str) -> bool {
    if service_type & 0x30 == 0 || service_type & 0x03 != 0 || !(start == 2 || start == 3 && saved) {
        return false;
    }
    if entree.chemin.is_empty() || entree.editeur.is_empty() || entree.suggestion == "garder" {
        return false;
    }
    let editeur = entree.editeur.to_lowercase();
    if editeur.contains("microsoft") || editeur.starts_with("windows") {
        return false;
    }
    let identite = format!("{} {}", entree.nom, entree.editeur).to_lowercase();
    if ["antivirus", "endpoint", "securityhealth", "defender", "eset", "bitdefender", "sophos", "crowdstrike", "sentinelone", "trend micro", "trendmicro", "kaspersky", "malwarebytes", "norton", "symantec", "mcafee", "avast", "fortinet", "carbon black", "carbonblack"]
        .iter().any(|mot| identite.contains(mot)) {
        return false;
    }
    let chemin = entree.chemin.replace('/', "\\").to_lowercase();
    let windows = windows.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    !(chemin == windows || chemin.starts_with(&format!("{windows}\\")) || chemin.contains("\\windows\\system32\\"))
}

fn lire_service(nom: &str) -> Option<ServiceConfig> {
    if nom.is_empty() || nom.contains('\\') || nom.contains('/') { return None; }
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!("{SERVICES}\\{nom}")).ok()?;
    let service_type: u32 = key.get_value("Type").ok()?;
    let start: u32 = key.get_value("Start").ok()?;
    let delayed = key.get_value::<u32, _>("DelayedAutoStart").unwrap_or(0) != 0;
    let commande: String = key.get_value("ImagePath").ok()?;
    let entree = construire("Service", nom, &commande, start == 2, true);
    Some(ServiceConfig { entree, service_type, start, delayed })
}

fn lister_services(dir: &Path) -> Result<Vec<Entree>, String> {
    let _lock = SERVICES_LOCK.lock().map_err(|e| e.to_string())?;
    let saved = lire_etat_services(dir)?;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(keys) = hklm.open_subkey(SERVICES) else { return Ok(Vec::new()) };
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut out = Vec::new();
    for nom in keys.enum_keys().filter_map(|x| x.ok()) {
        let Some(mut config) = lire_service(&nom) else { continue };
        let was_saved = saved.contains_key(&nom.to_lowercase());
        if !service_eligible(config.service_type, config.start, was_saved, &config.entree, &windows) { continue; }
        if config.start == 3 {
            config.entree.note = "Mode manuel choisi par DiskLens : réactiver restaure le démarrage automatique au prochain redémarrage.".into();
        } else {
            config.entree.note = "Service tiers automatique : désactiver passe en mode manuel au prochain redémarrage, sans arrêter le service.".into();
        }
        out.push(config.entree);
    }
    Ok(out)
}

fn lister(dir: &Path) -> Result<Vec<Entree>, String> {
    let mut out = Vec::new();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for (source, racine, chemin, sous, admin) in [
        ("HKCU Run", &hkcu, RUN, "Run", false),
        ("HKLM Run", &hklm, RUN, "Run", true),
        ("HKLM Run (32 bits)", &hklm, RUN32, "Run32", true),
    ] {
        let Ok(k) = racine.open_subkey(chemin) else { continue };
        for (nom, val) in k.enum_values().filter_map(|x| x.ok()) {
            let cmd = val.to_string();
            let desactive = lire_approuve(racine, sous, &nom);
            out.push(construire(source, &nom, &cmd, !desactive, admin));
        }
    }
    for (source, dossier, admin) in dossiers_demarrage() {
        let racine = if admin { &hklm } else { &hkcu };
        let Ok(rd) = std::fs::read_dir(&dossier) else { continue };
        for f in rd.filter_map(|x| x.ok()) {
            let nom = f.file_name().to_string_lossy().to_string();
            if nom.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let desactive = lire_approuve(racine, "StartupFolder", &nom);
            out.push(construire(source, &nom, &f.path().to_string_lossy(), !desactive, admin));
        }
    }
    out.extend(taches_ouverture_session());
    out.extend(lister_services(dir)?);
    Ok(out)
}

fn powershell(script: &str) -> String {
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Tâches planifiées déclenchées à l'ouverture de session, hors tâches Microsoft.
fn taches_ouverture_session() -> Vec<Entree> {
    let s = powershell(
        "try{Get-ScheduledTask -ErrorAction Stop | Where-Object { $_.TaskPath -notlike '\\Microsoft\\*' -and ($_.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskLogonTrigger' }) } | ForEach-Object { \"$($_.TaskPath)$($_.TaskName)|$($_.State)|$(($_.Actions | Select-Object -First 1).Execute)\" }}catch{}",
    );
    s.lines()
        .filter_map(|l| {
            let v: Vec<&str> = l.split('|').collect();
            if v.len() < 3 {
                return None;
            }
            Some(construire("Tâche planifiée", v[0].trim(), v[2].trim(), v[1].trim() != "Disabled", true))
        })
        .collect()
}

fn cible_autorisee<'a>(id: &str, activer: bool, inventaire: &'a [Entree]) -> Result<&'a Entree, String> {
    let entree = inventaire.iter().find(|e| e.id == id).ok_or("entrée absente de l'inventaire actuel")?;
    if !activer && entree.suggestion == "garder" {
        return Err("composant de sécurité, pilote ou système : désactivation refusée".into());
    }
    Ok(entree)
}

fn verifier_service_actuel(current: &ServiceConfig, attendu: &Entree, saved: Option<&ServiceSaved>, activer: bool, windows: &str) -> Result<(), String> {
    if !service_eligible(current.service_type, current.start, saved.is_some(), &current.entree, windows)
        || current.entree.id != attendu.id || current.entree.commande != attendu.commande || current.entree.editeur != attendu.editeur {
        return Err("service modifié ou devenu critique depuis l'inventaire : relancer l'analyse".into());
    }
    if activer {
        let previous = saved.ok_or("aucun état antérieur DiskLens pour ce service")?;
        if current.start != 3 || previous.start != 2 || previous.commande != current.entree.commande || previous.editeur != current.entree.editeur {
            return Err("configuration initiale du service différente : réactivation refusée".into());
        }
    } else if current.start != 2 {
        return Err("service non automatique : désactivation refusée".into());
    }
    Ok(())
}

fn configurer_service(nom: &str, mode: &str) -> Result<(), String> {
    let output = std::process::Command::new("sc.exe")
        .args(["config", nom, "start=", mode])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("sc.exe inaccessible : {e}"))?;
    if output.status.success() { Ok(()) } else { Err("configuration du service refusée (droits administrateur requis ?)".into()) }
}

fn appliquer_service(dir: &Path, nom: &str, activer: bool, attendu: &Entree) -> Result<(), String> {
    let _lock = SERVICES_LOCK.lock().map_err(|e| e.to_string())?;
    let mut saved = lire_etat_services(dir)?;
    let key = nom.to_lowercase();
    let current = lire_service(nom).ok_or("service introuvable ou configuration inaccessible")?;
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    verifier_service_actuel(&current, attendu, saved.get(&key), activer, &windows)?;
    if activer {
        let previous = saved.get(&key).ok_or("aucun état antérieur DiskLens pour ce service")?;
        configurer_service(nom, mode_restauration(previous))?;
        saved.remove(&key);
        ecrire_etat_services(dir, &saved)?;
    } else {
        let old = saved.insert(key.clone(), ServiceSaved {
            start: current.start, delayed: current.delayed,
            commande: current.entree.commande.clone(), editeur: current.entree.editeur.clone(),
        });
        ecrire_etat_services(dir, &saved)?;
        if let Err(e) = configurer_service(nom, "demand") {
            match old { Some(v) => { saved.insert(key, v); }, None => { saved.remove(&key); } }
            let _ = ecrire_etat_services(dir, &saved);
            return Err(e);
        }
    }
    Ok(())
}

fn appliquer_un(id: &str, activer: bool) -> Result<(), String> {
    let (source, nom) = id.split_once('|').ok_or("identifiant invalide")?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match source {
        "HKCU Run" => ecrire_approuve(&hkcu, "Run", nom, activer),
        "HKLM Run" => ecrire_approuve(&hklm, "Run", nom, activer).map_err(|e| format!("{e} (droits administrateur requis)")),
        "HKLM Run (32 bits)" => ecrire_approuve(&hklm, "Run32", nom, activer).map_err(|e| format!("{e} (droits administrateur requis)")),
        "Dossier Démarrage (utilisateur)" => ecrire_approuve(&hkcu, "StartupFolder", nom, activer),
        "Dossier Démarrage (commun)" => ecrire_approuve(&hklm, "StartupFolder", nom, activer).map_err(|e| format!("{e} (droits administrateur requis)")),
        "Tâche planifiée" => {
            if nom.contains('"') || nom.contains('\n') {
                return Err("nom de tâche invalide".into());
            }
            let o = std::process::Command::new("schtasks.exe")
                .args(["/change", "/tn", nom, if activer { "/enable" } else { "/disable" }])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| e.to_string())?;
            if o.status.success() { Ok(()) } else { Err("refusé (droits administrateur requis ?)".into()) }
        }
        _ => Err("source inconnue".into()),
    }
}

#[tauri::command]
pub async fn demarrage_liste(st: tauri::State<'_, crate::AppState>) -> Result<Vec<Entree>, String> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || lister(&dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn demarrage_appliquer(st: tauri::State<'_, crate::AppState>, ids: Vec<String>, activer: bool) -> Result<Vec<Resultat>, String> {
    let log = crate::store::data_file(&st.dir, "demarrage.log");
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        let inventaire = lister(&dir)?;
        Ok::<_, String>(ids.into_iter()
            .map(|id| {
                let r = cible_autorisee(&id, activer, &inventaire).and_then(|entree| {
                    if entree.source == "Service" {
                        appliquer_service(&dir, &entree.nom, activer, entree)
                    } else {
                        appliquer_un(&id, activer)
                    }
                });
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
                    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                    let _ = writeln!(f, "{t}\t{}\t{}\t{id}", if activer { "ACTIVE" } else { "DESACTIVE" }, if r.is_ok() { "OK" } else { "ECHEC" });
                }
                match r {
                    Ok(()) => Resultat { id, ok: true, detail: String::new() },
                    Err(e) => Resultat { id, ok: false, detail: e },
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factice(id: &str, suggestion: &'static str) -> Entree {
        Entree {
            id: id.into(), nom: id.into(), source: "HKCU Run".into(), commande: "C:\\app.exe".into(),
            chemin: "C:\\app.exe".into(), editeur: String::new(), signe: false, actif: true,
            suggestion, note: String::new(), admin: false,
        }
    }

    #[test]
    fn services_tiers_automatiques_et_manuels_memorises_seulement() {
        let mut entree = factice("Service|Acme", "inconnu");
        entree.chemin = r"C:\Program Files\Acme\agent.exe".into();
        entree.editeur = "Acme".into();
        assert!(service_eligible(0x10, 2, false, &entree, r"C:\Windows"));
        assert!(!service_eligible(0x10, 3, false, &entree, r"C:\Windows"));
        assert!(service_eligible(0x10, 3, true, &entree, r"C:\Windows"));
        assert!(!service_eligible(0x01, 2, false, &entree, r"C:\Windows"));
        entree.chemin = r"C:\Windows\System32\agent.exe".into();
        assert!(!service_eligible(0x10, 2, false, &entree, r"C:\Windows"));
        entree.chemin = r"C:\Program Files\Acme\agent.exe".into();
        entree.editeur = "Microsoft Corporation".into();
        assert!(!service_eligible(0x10, 2, false, &entree, r"C:\Windows"));
        entree.editeur = "CrowdStrike, Inc.".into();
        assert!(!service_eligible(0x10, 2, false, &entree, r"C:\Windows"));
        entree.editeur = "Acme".into();
        entree.nom = "ESET Security Service".into();
        assert!(!service_eligible(0x10, 2, false, &entree, r"C:\Windows"));
    }

    #[test]
    fn service_modifie_depuis_l_inventaire_est_refuse() {
        let mut attendu = factice("Service|Acme", "inconnu");
        attendu.source = "Service".into();
        attendu.chemin = r"C:\Acme\agent.exe".into();
        attendu.commande = attendu.chemin.clone();
        attendu.editeur = "Acme".into();
        let mut autre = attendu.clone();
        autre.commande = r"C:\Autre\agent.exe".into();
        let current = ServiceConfig { entree: autre, service_type: 0x10, start: 2, delayed: false };
        assert!(verifier_service_actuel(&current, &attendu, None, false, r"C:\Windows").is_err());
    }

    #[test]
    fn service_reactivation_retablit_le_demarrage_differe() {
        let saved = ServiceSaved { start: 2, delayed: true, commande: r"C:\Acme\agent.exe".into(), editeur: "Acme".into() };
        assert_eq!(mode_restauration(&saved), "delayed-auto");
        let normal = ServiceSaved { delayed: false, ..saved };
        assert_eq!(mode_restauration(&normal), "auto");
    }

    #[test]
    fn etat_services_remplacement_atomique_et_cible_lecture_seule() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("disklens-services-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join(SERVICES_STATE);
        let record = ServiceSaved { start: 2, delayed: false, commande: r"C:\Acme\agent.exe".into(), editeur: "Acme".into() };
        let mut state = HashMap::new();
        state.insert("acme".into(), record.clone());
        ecrire_etat_services(&dir, &state).unwrap();
        assert!(!lire_etat_services(&dir).unwrap()["acme"].delayed);

        state.get_mut("acme").unwrap().delayed = true;
        ecrire_etat_services(&dir, &state).unwrap();
        assert!(lire_etat_services(&dir).unwrap()["acme"].delayed);
        let previous = fs::read(&path).unwrap();

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        state.insert("autre".into(), record);
        let refused = ecrire_etat_services(&dir, &state);
        let intact = fs::read(&path).unwrap() == previous;
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&dir).unwrap();
        assert!(refused.is_err());
        assert!(intact);
    }

    #[test]
    fn action_refuse_id_absent_du_scan_frais() {
        let liste = [factice("HKCU Run|Present", "facultatif")];
        assert!(cible_autorisee("Tâche planifiée|\\Microsoft\\Critique", false, &liste).is_err());
    }

    #[test]
    fn action_refuse_desactivation_critique() {
        let liste = [factice("HKCU Run|Defender", "garder")];
        assert!(cible_autorisee("HKCU Run|Defender", false, &liste).is_err());
        assert!(cible_autorisee("HKCU Run|Defender", true, &liste).is_ok());
    }

    #[test]
    fn action_accepte_desactivation_facultative() {
        let liste = [factice("HKCU Run|Optionnel", "facultatif")];
        assert!(cible_autorisee("HKCU Run|Optionnel", false, &liste).is_ok());
    }

    /// Lecture seule sur la vraie machine : `cargo test fumee -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fumee() {
        for e in lister(&std::env::temp_dir()).unwrap() {
            println!("{:<9} {:<32} actif={:<5} {:<10} {:<28} {}", e.suggestion, e.nom, e.actif, e.source, e.editeur, e.note);
        }
    }

    /// Aller-retour réel sur une valeur factice de HKCU (supprimée à la fin).
    #[test]
    #[ignore = "écrit dans le registre réel HKCU"]
    fn desactiver_puis_reactiver() {
        let nom = "DiskLensTestFactice";
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        assert!(!lire_approuve(&hkcu, "Run", nom));
        appliquer_un(&format!("HKCU Run|{nom}"), false).unwrap();
        assert!(lire_approuve(&hkcu, "Run", nom));
        appliquer_un(&format!("HKCU Run|{nom}"), true).unwrap();
        assert!(!lire_approuve(&hkcu, "Run", nom));
        if let Ok(k) = hkcu.open_subkey_with_flags(format!("{APPROVED}\\Run"), KEY_SET_VALUE) {
            let _ = k.delete_value(nom);
        }
        assert!(appliquer_un("source inconnue|x", false).is_err());
    }

    #[test]
    fn exe_entre_guillemets() {
        assert_eq!(extraire_exe("\"C:\\Program Files\\A B\\a.exe\" --tray"), "C:\\Program Files\\A B\\a.exe");
    }

    #[test]
    fn exe_sans_guillemets() {
        assert_eq!(extraire_exe("C:\\Tools\\x.exe /silent"), "C:\\Tools\\x.exe");
    }

    #[test]
    fn variables() {
        std::env::set_var("DL_TEST_VAR", "C:\\Z");
        assert_eq!(developper("%DL_TEST_VAR%\\a.exe"), "C:\\Z\\a.exe");
        assert_eq!(developper("%INCONNUE_XYZ%\\a"), "%INCONNUE_XYZ%\\a");
    }

    #[test]
    fn etat_startup_approved() {
        assert!(!est_desactive(&octets_etat(true, 0)));
        let d = octets_etat(false, 5);
        assert!(est_desactive(&d));
        assert_eq!(d.len(), 12);
        assert!(est_desactive(&[6]) == false && est_desactive(&[3]));
    }

    #[test]
    fn suggestions() {
        assert_eq!(suggerer("SecurityHealth", "", "").0, "garder");
        assert_eq!(suggerer("Discord", "Discord Inc.", "").0, "facultatif");
        assert_eq!(suggerer("OneDrive", "Microsoft Corporation", "").0, "facultatif");
        assert_eq!(suggerer("Truc", "", "").0, "inconnu");
        assert_eq!(suggerer("Teams", "Microsoft Corporation", "").0, "facultatif");
        assert_eq!(suggerer("ClaudeCodeUsageMonitor", "Code Zeno Pty Ltd", "C:\\x\\reset.exe").0, "inconnu");
        assert_eq!(suggerer("RtkAudUService", "Realtek Semiconductor", "").0, "garder");
    }
}
