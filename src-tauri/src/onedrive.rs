//! OneDrive : libérer l'espace local (fichiers « en ligne uniquement », Files On-Demand).
//! Rien n'est supprimé : les fichiers restent dans le cloud et se retéléchargent à l'ouverture.
use serde::Serialize;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tauri::State;
use winreg::enums::*;
use winreg::RegKey;

const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const RECALL_ON_OPEN: u32 = 0x0004_0000;
const OFFLINE: u32 = 0x0000_1000;
const PINNED: u32 = 0x0008_0000;
const UNPINNED: u32 = 0x0010_0000;
const PROFONDEUR_MAX: u32 = 40;

#[derive(Serialize, Clone, Default)]
pub struct OdDossier {
    pub chemin: String,
    pub nom: String,
    /// Octets réellement présents sur le disque (hors fichiers déjà « en ligne uniquement »)
    pub local_octets: u64,
    pub local_fichiers: u64,
    /// Part de local_octets marquée « toujours conserver sur cet appareil »
    pub epingle_octets: u64,
    pub cloud_fichiers: u64,
}

#[derive(Serialize)]
pub struct OdRacine {
    pub chemin: String,
    pub compte: String,
    pub dossiers: Vec<OdDossier>,
    pub local_octets: u64,
    pub epingle_octets: u64,
    pub cloud_fichiers: u64,
}

#[derive(Serialize)]
pub struct OdResult {
    pub chemin: String,
    pub ok: bool,
    pub fichiers: u64,
    pub octets: u64,
    pub detail: String,
}

fn cloud_seul(a: u32) -> bool {
    a & (RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE) != 0
}

/// Racines OneDrive : comptes du registre (UserFolder) + variables d'environnement.
fn racines() -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Microsoft\\OneDrive\\Accounts") {
        for n in k.enum_keys().filter_map(|x| x.ok()) {
            if let Ok(sk) = k.open_subkey(&n) {
                let dossier: String = sk.get_value("UserFolder").unwrap_or_default();
                let nom: String = sk.get_value("UserEmail").unwrap_or_else(|_| n.clone());
                if !dossier.is_empty() {
                    v.push((nom, PathBuf::from(dossier)));
                }
            }
        }
    }
    for k in ["OneDriveCommercial", "OneDriveConsumer", "OneDrive"] {
        if let Ok(p) = std::env::var(k) {
            v.push((k.to_string(), PathBuf::from(p)));
        }
    }
    let mut vus = std::collections::HashSet::new();
    v.retain(|(_, p)| p.is_dir() && vus.insert(p.to_string_lossy().to_lowercase()));
    v
}

/// Parcourt un dossier sans suivre de lien symbolique ; appelle `f(chemin, attributs, taille)` pour chaque fichier.
fn parcourir(racine: &Path, f: &mut dyn FnMut(&Path, u32, u64)) {
    let mut pile = vec![(racine.to_path_buf(), 0u32)];
    while let Some((d, prof)) = pile.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.file_type().is_symlink() {
                continue;
            }
            if md.is_dir() {
                if prof < PROFONDEUR_MAX {
                    pile.push((e.path(), prof + 1));
                }
            } else {
                f(&e.path(), md.file_attributes(), md.len());
            }
        }
    }
}

fn mesurer(chemin: &Path) -> OdDossier {
    let mut d = OdDossier { chemin: chemin.to_string_lossy().to_string(), nom: chemin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), ..Default::default() };
    parcourir(chemin, &mut |_, a, t| {
        if cloud_seul(a) {
            d.cloud_fichiers += 1;
        } else {
            d.local_octets += t;
            d.local_fichiers += 1;
            if a & PINNED != 0 {
                d.epingle_octets += t;
            }
        }
    });
    d
}

fn scanner() -> Vec<OdRacine> {
    racines()
        .into_iter()
        .map(|(compte, p)| {
            let mut dossiers: Vec<OdDossier> = std::fs::read_dir(&p)
                .map(|rd| rd.flatten().filter(|e| e.metadata().map(|m| m.is_dir() && !m.file_type().is_symlink()).unwrap_or(false)).map(|e| mesurer(&e.path())).collect())
                .unwrap_or_default();
            dossiers.sort_by(|a, b| b.local_octets.cmp(&a.local_octets));
            OdRacine {
                chemin: p.to_string_lossy().to_string(),
                compte,
                local_octets: dossiers.iter().map(|d| d.local_octets).sum(),
                epingle_octets: dossiers.iter().map(|d| d.epingle_octets).sum(),
                cloud_fichiers: dossiers.iter().map(|d| d.cloud_fichiers).sum(),
                dossiers,
            }
        })
        .collect()
}

fn sous_une_racine(chemin: &Path, racines: &[(String, PathBuf)]) -> bool {
    let c = chemin.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    !c.contains("\\..")
        && racines.iter().any(|(_, r)| {
            let r = r.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
            c == r || c.starts_with(&format!("{r}\\"))
        })
}

fn poser_non_epingle(chemin: &Path, attrs: u32) -> bool {
    let wide: Vec<u16> = chemin.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let nouveau = (attrs | UNPINNED) & !PINNED;
    // SAFETY : `wide` est une chaîne UTF-16 terminée par NUL valable pendant l'appel.
    unsafe { windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(wide.as_ptr(), nouveau) != 0 }
}

fn liberer(dir: &Path, chemins: &[String], inclure_epingles: bool) -> Vec<OdResult> {
    let rac = racines();
    let mut res = Vec::new();
    for c in chemins {
        let p = PathBuf::from(c);
        let r = if !sous_une_racine(&p, &rac) || !p.is_dir() {
            OdResult { chemin: c.clone(), ok: false, fichiers: 0, octets: 0, detail: "Refusé : ce dossier n'est pas dans un OneDrive".into() }
        } else {
            let (mut n, mut o, mut ko, mut ep) = (0u64, 0u64, 0u64, 0u64);
            let mut cibles: Vec<(PathBuf, u32)> = Vec::new();
            parcourir(&p, &mut |f, a, t| {
                if cloud_seul(a) {
                    return;
                }
                if a & PINNED != 0 && !inclure_epingles {
                    ep += 1;
                    return;
                }
                cibles.push((f.to_path_buf(), a));
                o += t;
            });
            for (f, a) in &cibles {
                if poser_non_epingle(f, *a) {
                    n += 1;
                } else {
                    ko += 1;
                }
            }
            let mut detail = format!("{n} fichier(s) marqué(s) « en ligne uniquement » ; OneDrive libère l'espace en arrière-plan");
            if ep > 0 {
                detail.push_str(&format!(" · {ep} fichier(s) « toujours conserver » ignoré(s)"));
            }
            if ko > 0 {
                detail.push_str(&format!(" · {ko} échec(s)"));
            }
            OdResult { chemin: c.clone(), ok: ko == 0, fichiers: n, octets: o, detail }
        };
        if let Ok(mut h) = std::fs::OpenOptions::new().create(true).append(true).open(crate::store::data_file(dir, "onedrive.log")) {
            let _ = writeln!(h, "{}", serde_json::json!({ "chemin": r.chemin, "ok": r.ok, "fichiers": r.fichiers, "octets": r.octets, "detail": r.detail }));
        }
        res.push(r);
    }
    res
}

#[tauri::command]
pub async fn onedrive_scan(_st: State<'_, crate::AppState>) -> Result<Vec<OdRacine>, String> {
    tauri::async_runtime::spawn_blocking(scanner).await.map_err(|e| e.to_string())
}

/// Marque les fichiers locaux « en ligne uniquement » (ne supprime rien du cloud).
#[tauri::command]
pub async fn onedrive_liberer(st: State<'_, crate::AppState>, chemins: Vec<String>, inclure_epingles: bool) -> Result<Vec<OdResult>, String> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || liberer(&dir, &chemins, inclure_epingles)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_seul_detecte_les_placeholders() {
        assert!(cloud_seul(RECALL_ON_DATA_ACCESS));
        assert!(cloud_seul(OFFLINE | 0x20));
        assert!(!cloud_seul(0x20));
        assert!(!cloud_seul(PINNED));
    }

    #[test]
    fn refuse_hors_onedrive_et_traversee() {
        let r = vec![("x".to_string(), PathBuf::from(r"C:\OneDrive"))];
        assert!(sous_une_racine(Path::new(r"C:\OneDrive\Docs"), &r));
        assert!(!sous_une_racine(Path::new(r"C:\Windows"), &r));
        assert!(!sous_une_racine(Path::new(r"C:\OneDrive\..\Windows"), &r));
        assert!(!sous_une_racine(Path::new(r"C:\OneDriveX"), &r));
    }

    /// Mesure réelle en lecture seule : `cargo test --lib scan_onedrive -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_onedrive() {
        for r in scanner() {
            println!("== {} ({}) local {:.0} Mo, épinglé {:.0} Mo, cloud {} fichiers", r.chemin, r.compte, r.local_octets as f64 / 1048576.0, r.epingle_octets as f64 / 1048576.0, r.cloud_fichiers);
            for d in r.dossiers.iter().take(8) {
                println!("   {:>8.0} Mo  {}", d.local_octets as f64 / 1048576.0, d.nom);
            }
        }
    }
}
