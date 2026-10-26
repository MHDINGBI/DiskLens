//! Recherche exploratoire en lecture seule, indépendante des inscriptions Uninstall disparues.
use super::Logiciel;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::os::windows::fs::MetadataExt;
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct Piste {
    pub nom: String,
    pub chemin: String,
    pub octets: u64,
    pub fichiers: u64,
    pub partiel: bool,
    pub indices: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct Rapport {
    pub dossiers: Vec<Piste>,
    pub dossiers_examines: usize,
    pub limites: Vec<String>,
}

// Ne pas employer compact() : il enlève les années et confondrait deux versions.
fn cle_nom(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric()).map(|s| if s.eq_ignore_ascii_case("rvt") { "revit" } else { s })
        .collect::<String>().to_lowercase()
}

fn annees(s: &str) -> Vec<&str> {
    s.split(|c: char| !c.is_ascii_digit()).filter(|s| s.len() == 4 && s.starts_with("20")).collect()
}

fn nom_associe(a: &str, b: &str) -> bool {
    let (ca, cb) = (cle_nom(a), cle_nom(b));
    if ca.is_empty() || cb.is_empty() { return false; }
    let (aa, ab) = (annees(a), annees(b));
    if !aa.is_empty() && !ab.is_empty() {
        if !aa.iter().any(|y| ab.contains(y)) { return false; }
        let mots = |s: &str| -> Vec<String> { s.split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.eq_ignore_ascii_case("rvt") || (s.len() >= 4 && !s.chars().all(|c| c.is_ascii_digit())))
            .filter(|s| !["autodesk", "software", "professional", "english", "version"].contains(&s.to_lowercase().as_str()))
            .map(cle_nom).collect() };
        let (ma, mb) = (mots(a), mots(b));
        if ma.iter().any(|m| mb.contains(m)) { return true; }
    }
    if ca.len() < 4 || cb.len() < 4 {
        // VLC, R, etc. : correspondance par mot complet, jamais une sous-chaîne arbitraire.
        return ca == cb || a.split(|c: char| !c.is_alphanumeric()).any(|s| cle_nom(s) == cb)
            || b.split(|c: char| !c.is_alphanumeric()).any(|s| cle_nom(s) == ca);
    }
    ca.contains(&cb) || cb.contains(&ca)
}

pub(super) fn editeur_associe(nom: &str, editeur: &str) -> bool {
    let sans_suffixes = editeur.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !["inc", "corp", "corporation", "llc", "ltd", "limited", "sas", "sarl", "software"].contains(&s.to_lowercase().as_str()))
        .collect::<Vec<_>>().join(" ");
    nom_associe(nom, &sans_suffixes)
}

fn cle_chemin(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn recouvre(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}\\")) || b.starts_with(&format!("{a}\\"))
}

fn ignorer(nom: &str) -> bool {
    let n = nom.to_lowercase();
    super::est_microsoft(nom) || super::motif_partage(nom) || n.contains("onedrive") || matches!(n.as_str(),
        "common files" | "common" | "programs" | "packages" | "package cache" | "packagemanagement" |
        "application data" | "temp" | "tmp" | "cache" | ".cache" | "caches" | "logs" | "history" |
        "crashdumps" | "crashreports" | "d3dscache" | "connecteddevicesplatform" | "comms" |
        "credentials" | "gnupg" | "crypto" | "protect" | "vault" | "wallet" | "bittorrenthelper" |
        "npm-cache" | "pip" | "uv" | "go-build" | "msbuild" | "reference assemblies" | "dotnet" |
        "android" | "node" | "node-gyp" | "deno" | "nuget" | "electron" | "electron-builder" |
        "ms-playwright" | "ms-playwright-go" | "ms-playwright-mcp" |
        "installshield installation information" | "uninstall information" | "downloaded installations")
}

fn accessible(p: &Path) -> bool {
    // Pas de jonction, de fichier cloud ou d'ancêtre redirigé, même au niveau des racines.
    p.ancestors().all(|a| std::fs::symlink_metadata(a)
        .map(|m| m.file_attributes() & (0x400 | 0x1000 | 0x400000 | 0x40000) == 0).unwrap_or(false))
}

fn enfants(p: &Path, limites: &mut Vec<String>, fin: Instant) -> Vec<PathBuf> {
    let rd = match std::fs::read_dir(p) {
        Ok(rd) => rd,
        Err(_) => { limites.push(format!("Dossier non lisible : {}", p.display())); return Vec::new(); }
    };
    let mut out = Vec::new();
    for (i, e) in rd.enumerate() {
        if i == 4096 || Instant::now() >= fin { limites.push(format!("Liste interrompue (durée ou 4 096 entrées) : {}", p.display())); break; }
        match e {
            Ok(e) if e.file_type().map(|t| t.is_dir()).unwrap_or(false) && accessible(&e.path()) => out.push(e.path()),
            Err(_) => limites.push(format!("Entrée non lisible dans {}", p.display())),
            _ => {}
        }
    }
    out.sort();
    out
}

fn mesurer(p: &Path, max: usize, fin: Instant) -> Piste {
    let mut r = Piste { nom: super::nom_dossier(p), chemin: p.to_string_lossy().into_owned(), octets: 0, fichiers: 0, partiel: false,
        indices: vec!["Aucune inscription de logiciel installé associée au nom ou au chemin dans l'inventaire consulté.".into()] };
    let mut pile = vec![p.to_path_buf()];
    let mut entrees = 0;
    let mut executable = false;
    let fin = std::cmp::min(fin, Instant::now() + Duration::from_secs(2));
    'parcours: while let Some(d) = pile.pop() {
        let Ok(rd) = std::fs::read_dir(d) else { r.partiel = true; continue };
        for e in rd {
            if entrees >= max || Instant::now() >= fin { r.partiel = true; break 'parcours; }
            entrees += 1;
            let Ok(e) = e else { r.partiel = true; continue };
            let Ok(md) = std::fs::symlink_metadata(e.path()) else { r.partiel = true; continue };
            if md.file_attributes() & (0x400 | 0x1000 | 0x400000 | 0x40000) != 0 { r.partiel = true; continue; }
            if md.is_dir() { pile.push(e.path()); }
            else {
                r.octets += md.len(); r.fichiers += 1;
                executable |= e.path().extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false);
            }
        }
    }
    if executable { r.indices.push("Exécutable présent : peut être une application portable ou encore utilisable.".into()); }
    r.indices.push("La désinstallation reste à confirmer ; des réglages, plugins ou données utiles peuvent subsister.".into());
    r
}

pub(super) fn analyser(racines: &[PathBuf], logiciels: &[Logiciel], actifs: &[PathBuf], deja: &HashSet<String>, max_entrees: usize) -> Rapport {
    let mut r = Rapport::default();
    let fin = Instant::now() + Duration::from_secs(25);
    let installes: Vec<_> = logiciels.iter().filter(|l| l.etat == "installe").collect();
    let mut chemins: Vec<String> = actifs.iter().map(|p| cle_chemin(p)).collect();
    for l in &installes {
        let p = Path::new(l.install_location.trim_matches('"'));
        if p.is_absolute() { chemins.push(cle_chemin(p)); }
        let exe = super::extraire_exe(&l.desinstalleur);
        if let Some(p) = Path::new(&exe).parent().filter(|p| p.is_absolute() && p.components().count() > 2) {
            chemins.push(cle_chemin(p));
        }
    }
    let connu = |p: &Path| {
        let c = cle_chemin(p);
        chemins.iter().chain(deja.iter()).any(|q| recouvre(&c, &q.replace('/', "\\").to_lowercase()))
            || installes.iter().any(|l| nom_associe(&super::nom_dossier(p), &l.nom))
            || actifs.iter().filter_map(|a| a.file_stem().and_then(|s| s.to_str()))
                .filter(|n| !["node", "python", "pythonw", "java", "electron", "update", "setup", "uninstall"].contains(&n.to_lowercase().as_str()))
                .any(|n| nom_associe(&super::nom_dossier(p), n))
    };
    let mut vus = HashSet::new();
    let mut candidats = Vec::new();
    'collecte: for racine in racines {
        if Instant::now() >= fin { r.limites.push("Collecte interrompue à sa limite de durée ; liste incomplète.".into()); break; }
        if !accessible(racine) { r.limites.push(format!("Emplacement absent, inaccessible ou redirigé : {}", racine.display())); continue; }
        for p in enfants(racine, &mut r.limites, fin) {
            if candidats.len() >= 512 || Instant::now() >= fin {
                r.limites.push("Collecte limitée en durée ou à 512 candidats ; liste incomplète.".into()); break 'collecte;
            }
            let nom = super::nom_dossier(&p);
            if ignorer(&nom) { continue; }
            let editeur_actif = installes.iter().any(|l| editeur_associe(&nom, &l.editeur));
            if editeur_actif {
                // Un éditeur encore utilisé peut conserver un ancien produit millésimé.
                // Les sous-dossiers non millésimés et les ressources partagées restent exclus.
                for enfant in enfants(&p, &mut r.limites, fin) {
                    if candidats.len() >= 512 || Instant::now() >= fin {
                        r.limites.push("Collecte limitée en durée ou à 512 candidats ; liste incomplète.".into()); break 'collecte;
                    }
                    let n = super::nom_dossier(&enfant);
                    let millesime = n.split(|c: char| !c.is_ascii_digit()).any(|s| s.len() == 4 && s.starts_with("20"));
                    if millesime && !ignorer(&n) && !connu(&enfant) && vus.insert(cle_chemin(&enfant)) { candidats.push(enfant); }
                }
            } else if !connu(&p) && vus.insert(cle_chemin(&p)) { candidats.push(p); }
        }
    }
    // Les racines peuvent se recouvrir (Local et Local/Programs).
    candidats.sort();
    let mut retenus: Vec<PathBuf> = Vec::new();
    for p in candidats {
        if retenus.iter().any(|q| recouvre(&cle_chemin(q), &cle_chemin(&p))) { continue; }
        retenus.push(p);
    }
    for p in retenus {
        if Instant::now() >= fin || r.dossiers_examines >= 256 {
            r.limites.push("Recherche interrompue à sa limite de durée ou de nombre de dossiers ; liste incomplète.".into()); break;
        }
        r.dossiers_examines += 1;
        let piste = mesurer(&p, max_entrees, fin);
        if piste.fichiers > 0 || piste.partiel { r.dossiers.push(piste); }
    }
    if r.dossiers.iter().any(|d| d.partiel) { r.limites.push("Certaines tailles sont partielles : plafond de mesure, fichiers cloud/liens ignorés ou accès refusé.".into()); }
    r.dossiers.sort_by(|a, b| b.octets.cmp(&a.octets).then_with(|| a.chemin.cmp(&b.chemin)));
    r.limites.sort(); r.limites.dedup();
    r
}

pub(super) fn scanner(logiciels: &[Logiciel], residus: &[super::Residu]) -> Rapport {
    let mut racines: Vec<PathBuf> = ["LOCALAPPDATA", "APPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"]
        .iter().filter_map(|k| std::env::var_os(k).map(PathBuf::from)).collect();
    if let Some(p) = std::env::var_os("LOCALAPPDATA") { racines.push(PathBuf::from(p).join("Programs")); }
    if let Some(p) = std::env::var_os("USERPROFILE") { racines.push(PathBuf::from(p).join("AppData/LocalLow")); }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true,
        sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always));
    let actifs: Vec<PathBuf> = sys.processes().values().filter_map(|p| p.exe().map(Path::to_path_buf)).collect();
    let deja = residus.iter().map(|r| r.chemin.to_lowercase()).collect();
    analyser(&racines, logiciels, &actifs, &deja, 20000)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(nom: &str) -> Self {
            let p = std::env::temp_dir().join(format!("disklens-decouverte-{}-{nom}", std::process::id()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn fichier(&self, relatif: &str, contenu: &[u8]) -> PathBuf {
            let p = self.0.join(relatif);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, contenu).unwrap();
            p
        }
        fn scan(&self, logiciels: &[Logiciel], actifs: &[PathBuf], deja: &HashSet<String>, max: usize) -> Rapport {
            analyser(&[self.0.clone(), self.0.clone()], logiciels, actifs, deja, max)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    fn logiciel(nom: &str, editeur: &str) -> Logiciel {
        serde_json::from_value(serde_json::json!({
            "id":"fixture", "nom":nom, "editeur":editeur, "version":"", "install_location":"",
            "desinstalleur":"", "cle":"", "etat":"installe", "raison":""
        })).unwrap()
    }

    #[test]
    fn trouve_sans_inscription_et_ne_modifie_pas_les_fichiers() {
        let f = Fixture::new("absent");
        let p = f.fichier("FlowLauncher/Settings/config.json", b"mes reglages");
        let r = f.scan(&[], &[], &HashSet::new(), 20000);
        assert_eq!(r.dossiers.len(), 1);
        assert_eq!(r.dossiers[0].nom, "FlowLauncher");
        assert_eq!(r.dossiers[0].octets, 12);
        assert_eq!(r.dossiers[0].fichiers, 1);
        assert!(!r.dossiers[0].partiel);
        assert!(r.dossiers[0].indices.iter().any(|i| i.contains("inscription")));
        assert_eq!(std::fs::read(p).unwrap(), b"mes reglages");
    }

    #[test]
    fn ecarte_installes_editeurs_processus_et_residus_deja_connus() {
        let f = Fixture::new("actifs");
        for n in ["Discord", "Acme", "OutilPortable", "NomSansRapport", "AncienConnu", "WindowsApps", "Package Cache", "Microsoft"] {
            f.fichier(&format!("{n}/data.bin"), b"abc");
        }
        let discord = logiciel("Discord", "Discord Inc.");
        let acme = logiciel("Un produit actif", "Acme Inc.");
        let mut autre = logiciel("Autre produit", "Autre");
        autre.install_location = f.0.join("NomSansRapport").to_string_lossy().into_owned();
        let deja = HashSet::from([f.0.join("AncienConnu").to_string_lossy().to_lowercase()]);
        let r = f.scan(&[discord, acme, autre], &[f.0.join("OutilPortable/app.exe")], &deja, 20000);
        assert!(r.dossiers.is_empty());
    }

    #[test]
    fn trouve_ancienne_version_chez_editeur_encore_installe() {
        let f = Fixture::new("versions");
        f.fichier("Autodesk/AutoCAD 2025/prefs.dat", b"ancien");
        f.fichier("Autodesk/AutoCAD 2026/prefs.dat", b"actuel");
        f.fichier("Autodesk/Shared/data.bin", b"commun");
        let r = f.scan(&[logiciel("AutoCAD 2026 - English", "Autodesk, Inc.")], &[], &HashSet::new(), 20000);
        assert_eq!(r.dossiers.len(), 1);
        assert_eq!(r.dossiers[0].nom, "AutoCAD 2025");
        assert_eq!(r.dossiers[0].octets, 6);
    }

    #[test]
    fn indique_mesure_partielle_sans_presenter_zero_comme_taille_complete() {
        let f = Fixture::new("budget");
        f.fichier("Ancien/a.dat", b"abc");
        f.fichier("Ancien/b.dat", b"def");
        let r = f.scan(&[], &[], &HashSet::new(), 1);
        assert_eq!(r.dossiers.len(), 1);
        assert!(r.dossiers[0].partiel);
        assert!(!r.limites.is_empty());
    }

    #[test]
    fn ecarte_nom_court_installe_et_donnees_application_portable_active() {
        let f = Fixture::new("alias-actifs");
        f.fichier("vlc/config.ini", b"garder");
        f.fichier("FlowLauncher/Settings.json", b"garder");
        let r = f.scan(&[logiciel("VLC media player", "VideoLAN")],
            &[PathBuf::from(r"D:\Portable\FlowLauncher\Flow.Launcher.exe")], &HashSet::new(), 20000);
        assert!(r.dossiers.is_empty());
    }

    #[test]
    fn ecarte_contenus_version_active_et_editeur_avec_suffixe_juridique() {
        let f = Fixture::new("contenus");
        for n in ["Autodesk/RVT 2027", "Autodesk/Revit Steel Connections 2027", "Bluebeam Software", "dotnet"] {
            f.fichier(&format!("{n}/data.bin"), b"garder");
        }
        let r = f.scan(&[logiciel("Autodesk Revit 2027", "Autodesk, Inc."), logiciel("Bluebeam Revu 21", "Bluebeam, Inc.")],
            &[], &HashSet::new(), 20000);
        assert!(r.dossiers.is_empty());
    }

    /// Contrôle explicite en lecture seule ; aucune action sur les logiciels ni le registre.
    #[test]
    #[ignore = "inventaire réel du profil et mesure des dossiers, sur demande uniquement"]
    fn decouverte_reelle() {
        let logiciels = super::super::inventaire();
        let r = scanner(&logiciels, &[]);
        println!("DECOUVERTE_JSON={}", serde_json::to_string(&r).unwrap());
    }
}
