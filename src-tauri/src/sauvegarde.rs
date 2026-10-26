//! État des sauvegardes (lecture seule) : Historique des fichiers, points de restauration,
//! OneDrive (dossiers connus), image système. Ne crée ni ne modifie aucune sauvegarde.
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FRAIS_JOURS: u32 = 30;

#[derive(Serialize, Clone)]
pub struct SvEtat {
    pub id: &'static str,
    pub titre: &'static str,
    /// vert = récent, orange = ancien/partiel, rouge = absent, gris = non vérifiable
    pub statut: &'static str,
    pub detail: String,
    pub age_jours: Option<u32>,
    /// Page « ms-settings: » à ouvrir pour agir
    pub reglage: Option<&'static str>,
}

fn maintenant() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn age(ts: u64) -> u32 {
    (maintenant().saturating_sub(ts) / 86_400) as u32
}

fn statut_age(j: u32) -> &'static str {
    if j <= FRAIS_JOURS { "vert" } else { "orange" }
}

fn mtime(p: &std::path::Path) -> Option<u64> {
    std::fs::metadata(p).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

fn balise(xml: &str, nom: &str) -> Option<String> {
    let a = xml.find(&format!("<{nom}>"))? + nom.len() + 2;
    let b = xml[a..].find(&format!("</{nom}>"))? + a;
    Some(xml[a..b].trim().to_string())
}

/// FILETIME (100 ns depuis 1601) -> secondes Unix.
fn filetime_unix(v: u64) -> Option<u64> {
    (v / 10_000_000).checked_sub(11_644_473_600)
}

fn historique_fichiers() -> SvEtat {
    let mut e = SvEtat { id: "filehistory", titre: "Historique des fichiers", statut: "rouge",
        detail: "Non configuré.".into(), age_jours: None, reglage: Some("ms-settings:backup") };
    let base = match std::env::var("LOCALAPPDATA") {
        Ok(v) => PathBuf::from(v).join("Microsoft\\Windows\\FileHistory\\Configuration"),
        Err(_) => return e,
    };
    let cfg = base.join("Config1.xml");
    let Ok(xml) = std::fs::read_to_string(&cfg) else { return e };
    let cible = balise(&xml, "TargetUrl").unwrap_or_default();
    let actif = balise(&xml, "ProtectedUpToTime").and_then(|v| v.parse::<u64>().ok()).and_then(filetime_unix);
    match actif {
        Some(ts) => {
            let j = age(ts);
            e.statut = statut_age(j);
            e.age_jours = Some(j);
            e.detail = format!("Dernière protection il y a {j} j. Destination : {}.", if cible.is_empty() { "inconnue" } else { &cible });
        }
        None => {
            e.statut = "orange";
            e.detail = format!("Configuré (destination : {}) mais aucune sauvegarde datée trouvée.", if cible.is_empty() { "inconnue" } else { &cible });
        }
    }
    e
}

fn points_restauration() -> SvEtat {
    let mut e = SvEtat { id: "restore", titre: "Points de restauration", statut: "gris",
        detail: String::new(), age_jours: None, reglage: Some("ms-settings:about") };
    let actif = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore")
        .and_then(|k| k.get_value::<u32, _>("RPSessionInterval"))
        .ok();
    let sortie = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "try{(Get-ComputerRestorePoint -ErrorAction Stop | Sort-Object SequenceNumber -Descending | Select-Object -First 1).CreationTime}catch{'ERR'}"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let txt = sortie.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    // Format WMI : yyyymmddHHMMSS.ffffff+zzz
    if txt.len() >= 8 && txt.chars().take(8).all(|c| c.is_ascii_digit()) {
        let (y, m, d): (i64, i64, i64) = (txt[0..4].parse().unwrap_or(0), txt[4..6].parse().unwrap_or(0), txt[6..8].parse().unwrap_or(0));
        let ts = jours_civils(y, m, d) as u64 * 86_400;
        let j = age(ts);
        e.statut = statut_age(j);
        e.age_jours = Some(j);
        e.detail = format!("Dernier point il y a {j} j.");
    } else if txt.is_empty() || txt == "ERR" {
        e.detail = "Accès limité : la liste des points de restauration demande les droits administrateur.".into();
    } else {
        e.statut = "rouge";
        e.detail = "Aucun point de restauration (protection système désactivée ou jamais créé).".into();
    }
    if actif.is_none() && e.statut == "gris" {
        e.detail.push_str(" Protection système non confirmée.");
    }
    e
}

/// Jours depuis 1970-01-01 pour une date civile (algorithme de H. Hinnant).
pub fn jours_civils(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn onedrive_dossiers() -> SvEtat {
    let mut e = SvEtat { id: "onedrive", titre: "OneDrive (Bureau, Documents, Images)", statut: "rouge",
        detail: String::new(), age_jours: None, reglage: Some("ms-settings:backup") };
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders") else {
        e.statut = "gris";
        e.detail = "Dossiers utilisateur illisibles.".into();
        return e;
    };
    let mut ok = Vec::new();
    let mut ko = Vec::new();
    for (val, nom) in [("Desktop", "Bureau"), ("Personal", "Documents"), ("My Pictures", "Images")] {
        let v: String = k.get_value(val).unwrap_or_default();
        if v.to_lowercase().contains("onedrive") { ok.push(nom) } else { ko.push(nom) }
    }
    e.statut = if ko.is_empty() { "vert" } else if ok.is_empty() { "rouge" } else { "orange" };
    e.detail = match (ok.is_empty(), ko.is_empty()) {
        (_, true) => "Bureau, Documents et Images sont sauvegardés dans OneDrive.".into(),
        (true, _) => "Aucun dossier connu n'est sauvegardé dans OneDrive.".into(),
        _ => format!("Sauvegardés : {}. Non sauvegardés : {}.", ok.join(", "), ko.join(", ")),
    };
    e
}

fn image_systeme() -> SvEtat {
    let mut e = SvEtat { id: "image", titre: "Image système / disque externe", statut: "rouge",
        detail: "Aucune image système (WindowsImageBackup) trouvée sur les lecteurs connectés.".into(),
        age_jours: None, reglage: Some("ms-settings:backup") };
    let mut meilleur: Option<(u64, String)> = None;
    for l in b'D'..=b'Z' {
        let racine = format!("{}:\\", l as char);
        let dossier = PathBuf::from(&racine).join("WindowsImageBackup");
        if let Some(ts) = mtime(&dossier) {
            if meilleur.as_ref().map_or(true, |(t, _)| ts > *t) {
                meilleur = Some((ts, racine));
            }
        }
    }
    if let Some((ts, r)) = meilleur {
        let j = age(ts);
        e.statut = statut_age(j);
        e.age_jours = Some(j);
        e.detail = format!("Image système sur {r}, modifiée il y a {j} j.");
    }
    e
}

#[tauri::command]
pub async fn sauvegarde_etat() -> Result<Vec<SvEtat>, String> {
    tauri::async_runtime::spawn_blocking(|| vec![onedrive_dossiers(), historique_fichiers(), points_restauration(), image_systeme()])
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn sauvegarde_ouvrir(reglage: String) {
    if (reglage.starts_with("ms-settings:") || reglage == "windowsdefender:") && reglage.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-') {
        let _ = std::process::Command::new("explorer.exe").arg(reglage).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_civiles() {
        assert_eq!(jours_civils(1970, 1, 1), 0);
        assert_eq!(jours_civils(2000, 3, 1), 11_017);
    }

    #[test]
    fn filetime() {
        assert_eq!(filetime_unix(116_444_736_000_000_000), Some(0));
        assert_eq!(filetime_unix(1), None);
    }

    #[test]
    fn balises() {
        assert_eq!(balise("<A><TargetUrl> E:\\ </TargetUrl></A>", "TargetUrl").as_deref(), Some("E:\\"));
        assert_eq!(balise("<A></A>", "TargetUrl"), None);
    }
}
