//! Résidus logiciels — étape 1 : fichiers. Contrat : CONTRAT_RESIDUS.md (ne pas changer les signatures).
use crate::registre::{horodatage, normaliser, racine_quarantaine};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::State;
use winreg::enums::*;
use winreg::RegKey;

mod decouverte;

const NO_WINDOW: u32 = 0x0800_0000;
const REPARSE: u32 = 0x400;
const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const UNINSTALL_WOW: &str = "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const USERASSIST: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\UserAssist";

#[derive(Serialize, Deserialize, Clone)]
pub struct Logiciel {
    pub id: String,
    pub nom: String,
    pub editeur: String,
    pub version: String,
    pub install_location: String,
    pub desinstalleur: String,
    /// Clé Uninstall complète, ex. "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{GUID}"
    pub cle: String,
    /// "installe" | "fantome"
    pub etat: String,
    pub raison: String,
    #[serde(default)]
    pub date_installation: Option<String>,
    #[serde(default)]
    pub derniere_utilisation_estimee: Option<String>,
    #[serde(default)]
    pub confiance_usage: String,
    #[serde(default)]
    pub indice_usage: String,
    #[serde(default)]
    pub origine_oem: Option<String>,
    #[serde(default)]
    pub confiance_oem: String,
}

#[derive(Serialize, Clone)]
pub struct Residu {
    pub id: String,
    pub logiciel_id: String,
    /// "dossier" | "fichier"
    pub genre: String,
    pub chemin: String,
    pub octets: u64,
    pub fichiers: u64,
    /// "sur" | "probable" | "verifier"
    pub confiance: String,
    pub motif: String,
    /// Some(raison) si protégé : jamais sélectionnable côté UI, refusé côté backend
    pub protege: Option<String>,
}

#[derive(Serialize)]
pub struct ResidusReport {
    pub logiciels: Vec<Logiciel>,
    pub residus: Vec<Residu>,
    /// Pistes exploratoires en lecture seule, exclues des actions de nettoyage.
    pub decouverte: Option<decouverte::Rapport>,
}

#[derive(Serialize)]
pub struct QuarResult {
    pub chemin: String,
    pub ok: bool,
    pub detail: String,
    pub lot: String,
}

#[derive(Serialize)]
pub struct Lot {
    pub nom: String,
    pub chemin: String,
    pub age_jours: u32,
    pub octets: u64,
}

/// Dernier scan : seuls ces résidus (non protégés) peuvent être mis en quarantaine.
static DERNIER: Mutex<Vec<Residu>> = Mutex::new(Vec::new());
/// Noms des logiciels installés au dernier scan, années conservées.
static INSTALLES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Emplacements d'installation (minuscules) des logiciels installés.
static EMPLACEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Identité mémorisée avant lancement du désinstalleur, jamais fournie par le client.
static SUIVIS: Mutex<Option<HashMap<String, Logiciel>>> = Mutex::new(None);

// ---------- Fonctions pures ----------

fn compact(s: &str) -> String {
    normaliser(s).replace(' ', "")
}

fn hash_court(s: &str) -> String {
    blake3::hash(s.to_lowercase().as_bytes()).to_hex()[..12].to_string()
}

fn motif_partage(s: &str) -> bool {
    let l = s.to_lowercase();
    ["licens", "genuine", "identity", "redistributable", "visual c++", ".net", "webview2", "directx", "runtime"]
        .iter()
        .any(|m| l.contains(m))
}

fn est_microsoft(s: &str) -> bool {
    let n = s.trim().to_lowercase();
    n.starts_with("microsoft") || n.starts_with("windows")
}

/// Confiance du lien entre un nom de dossier et le logiciel. `ed_libre` : aucun logiciel installé n'a cet éditeur.
fn lien_nom(dossier: &str, l: &Logiciel, ed_libre: bool) -> Option<&'static str> {
    let (d, n, e) = (compact(dossier), compact(&l.nom), compact(&l.editeur));
    if d.is_empty() {
        return None;
    }
    if !n.is_empty() && d == n {
        // Produit millésimé (« Revit 2024 ») : un dossier sans l'année est partagé entre versions
        let annees: Vec<&str> = l.nom.split(|c: char| !c.is_alphanumeric()).filter(|t| t.len() == 4 && t.starts_with("20") && t.chars().all(|c| c.is_ascii_digit())).collect();
        if annees.is_empty() || annees.iter().all(|a| dossier.contains(a)) { Some("sur") } else { Some("probable") }
    } else if ed_libre && !e.is_empty() && d == e {
        Some("probable")
    } else if n.len() >= 5 && d.contains(&n) {
        Some("verifier")
    } else {
        None
    }
}

/// Raison de protection d'un chemin (indépendante du logiciel), None si déplaçable.
fn proteger(chemin: &str, installes: &[String], emplacements: &[String]) -> Option<String> {
    if let Err(er) = crate::clean::validate_action_path(chemin) {
        return Some(format!("Chemin non sûr : {er}"));
    }
    let p = chemin.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let var = |k: &str| std::env::var(k).unwrap_or_default().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let racines = [
        var("ProgramFiles"), var("ProgramFiles(x86)"), var("ProgramData"), var("USERPROFILE"), var("LOCALAPPDATA"), var("APPDATA"),
        var("SystemRoot"), "c:\\".into(),
    ];
    if p.len() <= 3 || racines.iter().any(|r| !r.is_empty() && *r == p) {
        return Some("Dossier racine du système ou du profil".into());
    }
    let sous = |r: &str| !r.is_empty() && (p == r || p.starts_with(&format!("{r}\\")));
    if sous(&var("SystemRoot")) || sous("c:\\windows") {
        return Some("Dossier système Windows".into());
    }
    let segs: Vec<&str> = p.split('\\').collect();
    for s in &segs {
        if matches!(*s, "windowsapps" | "common files" | "microsoft shared" | "windows nt" | "internet explorer" | "windows defender" | "windows kits")
            || *s == "microsoft"
        {
            return Some("Composant Microsoft / partagé".into());
        }
    }
    let nom = segs.last().copied().unwrap_or("");
    if motif_partage(nom) {
        return Some("Composant partagé".into());
    }
    let c = compact(nom);
    if !c.is_empty() && installes.iter().any(|i| meme_produit(nom, i)) {
        return Some("Même nom qu'un logiciel installé".into());
    }
    let prefixe = format!("{p}\\");
    if emplacements.iter().any(|e| e.starts_with(&prefixe) || *e == p) {
        return Some("Contient un logiciel encore installé".into());
    }
    None
}

fn reparse(md: &std::fs::Metadata) -> bool {
    md.file_attributes() & REPARSE != 0
}

/// (octets, fichiers) réels d'un dossier, sans suivre les jonctions.
fn taille(racine: &Path) -> (u64, u64) {
    let (mut o, mut f) = (0u64, 0u64);
    let mut pile = vec![racine.to_path_buf()];
    while let Some(d) = pile.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if reparse(&md) {
                continue;
            }
            if md.is_dir() {
                pile.push(e.path());
            } else {
                o += md.len();
                f += 1;
            }
        }
    }
    (o, f)
}

/// Exécutable d'une ligne de commande (guillemets, arguments, variables d'environnement) ; vide si non pertinent.
fn extraire_exe(cmd: &str) -> String {
    let c = cmd.trim();
    let brut = if let Some(r) = c.strip_prefix('"') {
        r.split('"').next().unwrap_or("").to_string()
    } else {
        match c.to_ascii_lowercase().find(".exe") {
            Some(i) => c[..i + 4].to_string(),
            None => c.split(',').next().unwrap_or("").trim().to_string(),
        }
    };
    let brut = brut.split(',').next().unwrap_or("").trim().to_string();
    let mut out = String::new();
    let mut reste = brut.as_str();
    while let Some(i) = reste.find('%') {
        out.push_str(&reste[..i]);
        match reste[i + 1..].find('%') {
            Some(j) => {
                let v = &reste[i + 1..i + 1 + j];
                out.push_str(&std::env::var(v).unwrap_or_default());
                reste = &reste[i + j + 2..];
            }
            None => {
                reste = &reste[i..];
                break;
            }
        }
    }
    out.push_str(reste);
    let l = out.to_lowercase();
    if !out.contains(':') || l.starts_with("c:\\windows") || l.contains("msiexec") || l.contains("rundll32") {
        return String::new();
    }
    out
}

fn date_installation(brut: &str) -> Option<String> {
    let b = brut.as_bytes();
    if b.len() != 8 || !b.iter().all(u8::is_ascii_digit) { return None; }
    let an: u32 = brut[..4].parse().ok()?;
    let mois: u32 = brut[4..6].parse().ok()?;
    let jour: u32 = brut[6..].parse().ok()?;
    let bissextile = an % 4 == 0 && (an % 100 != 0 || an % 400 == 0);
    let max = match mois { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30, 2 if bissextile => 29, 2 => 28, _ => return None };
    (an >= 1980 && jour >= 1 && jour <= max).then(|| format!("{an:04}-{mois:02}-{jour:02}"))
}

fn rot13(s: &str) -> String {
    s.chars().map(|c| match c {
        'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
        'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
        _ => c,
    }).collect()
}

/// Date UTC du dernier lancement UserAssist Windows 7+ (72 octets, FILETIME à l'offset 60).
fn userassist_date(data: &[u8]) -> Option<String> {
    if data.len() != 72 { return None; }
    let ft = u64::from_le_bytes(data[60..68].try_into().ok()?);
    let secondes = ft.checked_div(10_000_000)?.checked_sub(11_644_473_600)?;
    let aujourd_hui = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    if secondes > aujourd_hui + 86_400 { return None; }
    let jours = i64::try_from(secondes / 86_400).ok()?;
    // Conversion Gregorian civil_from_days, domaine borné par la date FILETIME validée.
    let z = jours + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut an = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let jour = doy - (153 * mp + 2) / 5 + 1;
    let mois = mp + if mp < 10 { 3 } else { -9 };
    if mois <= 2 { an += 1; }
    (an >= 1980).then(|| format!("{an:04}-{mois:02}-{jour:02}"))
}

fn traces_userassist() -> Vec<(String, String)> {
    let mut traces = Vec::new();
    let Ok(base) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(USERASSIST) else { return traces };
    for guid in base.enum_keys().flatten() {
        let Ok(count) = base.open_subkey(format!("{guid}\\Count")) else { continue };
        for (nom, valeur) in count.enum_values().flatten() {
            let chemin = rot13(&nom).replace('/', "\\").to_lowercase();
            if !chemin.ends_with(".exe") || !chemin.contains(":\\") { continue; }
            if let Some(date) = userassist_date(&valeur.bytes) { traces.push((chemin, date)); }
        }
    }
    traces
}

fn oem_fabricant() -> String {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("HARDWARE\\DESCRIPTION\\System\\BIOS")
        .ok().and_then(|k| k.get_value::<String, _>("SystemManufacturer").ok())
        .unwrap_or_default().trim().to_string()
}

fn enrichir_logiciel(l: &mut Logiciel, icone: &str, traces: &[(String, String)], fabricant: &str) {
    l.derniere_utilisation_estimee = None;
    l.confiance_usage = "inconnue".into();
    l.indice_usage = "Aucune trace UserAssist rattachée avec certitude à ce logiciel (utilisateur courant, lancements via l'Explorateur uniquement).".into();
    let dossier = l.install_location.trim_matches('"').replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let icone_exe = extraire_exe(icone).replace('/', "\\").to_lowercase();
    let candidat = traces.iter().filter(|(chemin, _)| {
        let fichier = chemin.rsplit('\\').next().unwrap_or("");
        let app = !["uninstall.exe", "unins000.exe", "setup.exe", "installer.exe", "msiexec.exe"].contains(&fichier);
        app && ((!icone_exe.is_empty() && *chemin == icone_exe)
            || (dossier.len() > 4 && chemin.starts_with(&format!("{dossier}\\"))))
    }).map(|(_, date)| date).max();
    if let Some(date) = candidat {
        l.derniere_utilisation_estimee = Some(date.clone());
        l.confiance_usage = "faible".into();
        l.indice_usage = "Trace UserAssist du profil courant pour un exécutable du logiciel. Elle peut refléter un composant auxiliaire et ne couvre pas tous les modes de lancement.".into();
    }
    l.origine_oem = None;
    l.confiance_oem = "inconnue".into();
    let ed = compact(&l.editeur);
    let fab = compact(fabricant);
    if fab.len() >= 4 && ed.len() >= 4 && (ed == fab || ed.starts_with(&fab) || fab.starts_with(&ed)) {
        l.origine_oem = Some(fabricant.to_string());
        // Même éditeur que le constructeur : indice d'origine, pas preuve de préinstallation.
        l.confiance_oem = "faible".into();
    }
}

// ---------- Inventaire ----------

fn lire_hive(h: &RegKey, hive: &str, base: &str, out: &mut Vec<Logiciel>) {
    let Ok(k) = h.open_subkey(base) else { return };
    for n in k.enum_keys().filter_map(|x| x.ok()) {
        let Ok(sk) = k.open_subkey(&n) else { continue };
        let nom: String = sk.get_value("DisplayName").unwrap_or_default();
        let nom = nom.trim().to_string();
        if nom.is_empty() || sk.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 || sk.get_value::<String, _>("ParentKeyName").is_ok() {
            continue;
        }
        let g = |c: &str| sk.get_value::<String, _>(c).unwrap_or_default().trim().to_string();
        let cle = format!("{hive}\\{base}\\{n}");
        let des = g("UninstallString");
        out.push(Logiciel {
            id: hash_court(&cle),
            nom,
            editeur: g("Publisher"),
            version: g("DisplayVersion"),
            install_location: g("InstallLocation"),
            desinstalleur: des,
            cle,
            etat: "installe".into(),
            raison: String::new(),
            date_installation: date_installation(&g("InstallDate")),
            derniere_utilisation_estimee: None,
            confiance_usage: "inconnue".into(),
            indice_usage: String::new(),
            origine_oem: None,
            confiance_oem: "inconnue".into(),
        });
    }
}

fn racines_programmes() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|k| std::env::var(k).ok()).map(PathBuf::from).collect()
}

/// Un dossier au nom du produit existe-t-il sous Program Files ou Local\Programs ?
fn dossier_produit_existe(l: &Logiciel) -> bool {
    let mut racines = racines_programmes();
    if let Ok(la) = std::env::var("LOCALAPPDATA") {
        racines.push(PathBuf::from(la).join("Programs"));
    }
    racines.iter().any(|r| {
        std::fs::read_dir(r).map(|rd| rd.flatten().any(|e| e.path().is_dir() && lien_nom(&e.file_name().to_string_lossy(), l, false) == Some("sur"))).unwrap_or(false)
    })
}

/// Classe le logiciel : fantôme seulement si au moins un chemin est déclaré et qu'aucun n'existe plus.
fn evaluer_etat(l: &mut Logiciel, icone: &str) {
    let existe = |p: &str| !p.is_empty() && Path::new(p.trim_matches('"')).exists();
    let declares: Vec<String> = [l.install_location.trim_matches('"').to_string(), extraire_exe(icone), extraire_exe(&l.desinstalleur)]
        .into_iter()
        .filter(|p| p.contains(':'))
        .collect();
    let (etat, raison) = if declares.is_empty() {
        ("installe", "Aucun chemin déclaré : impossible de conclure (traité comme installé)")
    } else if declares.iter().any(|p| existe(p)) {
        ("installe", "Un chemin déclaré existe encore")
    } else if dossier_produit_existe(l) {
        ("installe", "Un dossier au nom du produit existe dans Program Files")
    } else {
        ("fantome", "Tous les chemins déclarés (installation, icône, désinstalleur) ont disparu")
    };
    l.etat = etat.into();
    l.raison = raison.into();
}

fn inventaire() -> Vec<Logiciel> {
    let mut v = Vec::new();
    lire_hive(&RegKey::predef(HKEY_LOCAL_MACHINE), "HKLM", UNINSTALL, &mut v);
    lire_hive(&RegKey::predef(HKEY_LOCAL_MACHINE), "HKLM", UNINSTALL_WOW, &mut v);
    lire_hive(&RegKey::predef(HKEY_CURRENT_USER), "HKCU", UNINSTALL, &mut v);
    // Une même appli peut figurer en 64 et 32 bits ; on garde la première par clé complète
    let mut vus = HashSet::new();
    v.retain(|l| vus.insert(l.cle.to_lowercase()));
    let traces = traces_userassist();
    let fabricant = oem_fabricant();
    for l in v.iter_mut() {
        let icone = open_cle(&l.cle).and_then(|k| k.get_value::<String, _>("DisplayIcon").ok()).unwrap_or_default();
        evaluer_etat(l, &icone);
        enrichir_logiciel(l, &icone, &traces, &fabricant);
    }
    v
}

fn open_cle(chemin: &str) -> Option<RegKey> {
    let (h, reste) = chemin.split_once('\\')?;
    let hive = if h.eq_ignore_ascii_case("HKCU") { RegKey::predef(HKEY_CURRENT_USER) } else { RegKey::predef(HKEY_LOCAL_MACHINE) };
    hive.open_subkey(reste).ok()
}

// ---------- Recherche de résidus ----------

fn racines_recherche() -> Vec<(PathBuf, bool)> {
    let var = |k: &str| std::env::var(k).ok().map(PathBuf::from);
    let mut r: Vec<(PathBuf, bool)> = Vec::new(); // (racine, contenu utilisateur => jamais coché par défaut)
    for k in ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "LOCALAPPDATA", "APPDATA", "TEMP"] {
        if let Some(p) = var(k) {
            r.push((p, false));
        }
    }
    if let Some(la) = var("LOCALAPPDATA") {
        r.push((la.join("Programs"), false));
    }
    if let Some(up) = var("USERPROFILE") {
        r.push((up.join("AppData").join("LocalLow"), false));
        r.push((up.join("Documents"), true));
    }
    for k in ["ProgramData", "APPDATA"] {
        if let Some(p) = var(k) {
            r.push((p.join("Microsoft\\Windows\\Start Menu\\Programs"), false));
        }
    }
    r
}

fn sous_dossiers(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.metadata().map(|m| m.is_dir() && !reparse(&m)).unwrap_or(false))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn nom_dossier(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn chercher_residus(l: &Logiciel, ed_libre: bool, installes: &[String], emplacements: &[String], vus: &mut HashSet<String>) -> Vec<Residu> {
    let mut out: Vec<Residu> = Vec::new();
    let mut ajouter = |p: &Path, conf: &str, motif: String, user: bool| {
        let chemin = p.to_string_lossy().to_string();
        if !vus.insert(chemin.to_lowercase()) {
            return;
        }
        let (octets, fichiers) = taille(p);
        let protege = proteger(&chemin, installes, emplacements);
        let confiance = if user && conf == "sur" { "verifier" } else { conf };
        out.push(Residu {
            id: hash_court(&chemin),
            logiciel_id: l.id.clone(),
            genre: "dossier".into(),
            chemin,
            octets,
            fichiers,
            confiance: confiance.into(),
            motif,
            protege,
        });
    };
    for (racine, user) in racines_recherche() {
        for d in sous_dossiers(&racine) {
            let nom = nom_dossier(&d);
            match lien_nom(&nom, l, ed_libre) {
                Some("sur") => ajouter(&d, "sur", format!("Dossier au nom du produit (« {nom} »)"), user),
                Some("probable") => {
                    // Dossier éditeur : on propose plutôt le sous-dossier du produit s'il existe
                    let enfants: Vec<PathBuf> = sous_dossiers(&d).into_iter().filter(|c| lien_nom(&nom_dossier(c), l, false) == Some("sur")).collect();
                    if enfants.is_empty() {
                        ajouter(&d, "probable", format!("Dossier de l'éditeur « {} » (plus aucun logiciel installé)", l.editeur), user);
                    }
                    for c in enfants {
                        ajouter(&c, "sur", format!("Dossier du produit chez l'éditeur « {} »", l.editeur), user);
                    }
                }
                Some(_) => ajouter(&d, "verifier", format!("Nom proche du produit (« {nom} »)"), user),
                None => {
                    // Un autre produit du même éditeur peut rester installé : ne chercher
                    // que les sous-dossiers du produit, sans proposer le dossier partagé.
                    if !l.editeur.is_empty() && decouverte::editeur_associe(&nom, &l.editeur) {
                        for c in sous_dossiers(&d).into_iter().filter(|c| lien_nom(&nom_dossier(c), l, false) == Some("sur")) {
                            ajouter(&c, "sur", format!("Dossier du produit chez l'éditeur « {} » (autres produits conservés)", l.editeur), user);
                        }
                    }
                }
            }
        }
    }
    out
}

fn scanner() -> ResidusReport {
    let mut logiciels = inventaire();
    let installes_noms: Vec<String> = logiciels.iter().filter(|l| l.etat == "installe").map(|l| l.nom.clone()).collect();
    let ed_installes: HashSet<String> = logiciels.iter().filter(|l| l.etat == "installe").map(|l| compact(&l.editeur)).filter(|c| !c.is_empty()).collect();
    let emplacements: Vec<String> = logiciels
        .iter()
        .filter(|l| l.etat == "installe" && l.install_location.contains(':'))
        .map(|l| l.install_location.trim_matches('"').replace('/', "\\").trim_end_matches('\\').to_lowercase())
        .collect();
    let mut residus = Vec::new();
    let mut vus = HashSet::new();
    for l in logiciels.iter().filter(|l| l.etat == "fantome") {
        if est_microsoft(&l.nom) || est_microsoft(&l.editeur) || motif_partage(&l.nom) {
            continue;
        }
        let ed_libre = !ed_installes.contains(&compact(&l.editeur));
        residus.extend(chercher_residus(l, ed_libre, &installes_noms, &emplacements, &mut vus));
    }
    residus.sort_by(|a, b| b.octets.cmp(&a.octets));
    // Fantômes d'abord dans la liste de logiciels (ceux avec résidus en tête)
    logiciels.sort_by_key(|l| (l.etat != "fantome", l.nom.to_lowercase()));
    if let Ok(mut d) = DERNIER.lock() {
        *d = residus.clone();
    }
    if let Ok(mut i) = INSTALLES.lock() {
        *i = installes_noms;
    }
    if let Ok(mut e) = EMPLACEMENTS.lock() {
        *e = emplacements;
    }
    let decouverte = Some(decouverte::scanner(&logiciels, &residus));
    ResidusReport { logiciels, residus, decouverte }
}

/// Recherche manuelle (mode « après désinstallation ») : restes d'un logiciel désigné par l'utilisateur.
fn chercher_manuel(nom: &str, editeur: &str) -> Result<ResidusReport, String> {
    if compact(nom).len() < 4 {
        return Err("Nom trop court (4 caractères minimum)".into());
    }
    chercher_produit(nom, editeur)
}

/// Une identité mémorisée avant désinstallation peut être courte (Git, Go, R).
/// La limite de saisie manuelle ne s'applique pas à cette identité de confiance.
fn chercher_produit(nom: &str, editeur: &str) -> Result<ResidusReport, String> {
    let nom = nom.trim();
    if compact(nom).is_empty() { return Err("Nom de produit absent".into()); }
    if est_microsoft(nom) || est_microsoft(editeur) || motif_partage(nom) {
        return Err("Composant Microsoft ou partagé : recherche refusée".into());
    }
    let installes = inventaire();
    let noms: Vec<String> = installes.iter().filter(|l| l.etat == "installe").map(|l| l.nom.clone()).collect();
    if let Some(l) = installes.iter().find(|l| l.etat == "installe" && meme_produit(nom, &l.nom)) {
        return Err(format!("« {} » est encore installé : désinstalle-le d'abord", l.nom));
    }
    let ed = editeur.trim();
    let ed_libre = ed.is_empty() || !installes.iter().any(|l| l.etat == "installe" && compact(&l.editeur) == compact(ed));
    let emplacements: Vec<String> = installes
        .iter()
        .filter(|l| l.etat == "installe" && l.install_location.contains(':'))
        .map(|l| l.install_location.trim_matches('"').replace('/', "\\").trim_end_matches('\\').to_lowercase())
        .collect();
    let cle =format!("manuel:{}", compact(nom));
    let logiciel = Logiciel {
        id: hash_court(&cle), nom: nom.into(), editeur: ed.into(), version: String::new(), install_location: String::new(),
        desinstalleur: String::new(), cle: String::new(), etat: "fantome".into(), raison: "Recherche manuelle (logiciel déjà désinstallé)".into(),
        date_installation: None, derniere_utilisation_estimee: None, confiance_usage: "inconnue".into(), indice_usage: String::new(), origine_oem: None, confiance_oem: "inconnue".into(),
    };
    let mut vus = HashSet::new();
    let mut residus = chercher_residus(&logiciel, ed_libre, &noms, &emplacements, &mut vus);
    residus.sort_by(|a, b| b.octets.cmp(&a.octets));
    if let Ok(mut d) = DERNIER.lock() { *d = residus.clone(); }
    if let Ok(mut i) = INSTALLES.lock() { *i = noms; }
    if let Ok(mut e) = EMPLACEMENTS.lock() { *e = emplacements; }
    Ok(ResidusReport { logiciels: vec![logiciel], residus, decouverte: None })
}

// ---------- Quarantaine ----------

fn journal(dir: &Path, v: serde_json::Value) {
    let f = crate::store::data_file(dir, "residus.log");
    if let Ok(mut h) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
        let _ = writeln!(h, "{v}");
    }
}

fn lancer(prog: &str, args: &[&str]) -> i32 {
    Command::new(prog).args(args).creation_flags(NO_WINDOW).output().map(|o| o.status.code().unwrap_or(-1)).unwrap_or(-1)
}

fn deplacer(src: &Path, dest: &Path) -> Result<(), String> {
    let (s, d) = (src.to_string_lossy().to_string(), dest.to_string_lossy().to_string());
    let robocopy = || lancer("robocopy.exe", &[&s, &d, "/E", "/MOVE", "/XJ", "/R:1", "/W:1", "/NFL", "/NDL", "/NP", "/NJH", "/NJS"]);
    let mut code = robocopy();
    if code >= 8 || src.exists() {
        // Droits refusés ou fichiers verrouillés : reprise de propriété limitée à ce dossier
        if lancer("takeown.exe", &["/F", &s, "/R", "/A", "/D", "O"]) != 0 {
            lancer("takeown.exe", &["/F", &s, "/R", "/A", "/D", "Y"]);
        }
        lancer("icacls.exe", &[&s, "/grant", "*S-1-5-32-544:(OI)(CI)F", "/T", "/C", "/Q"]);
        code = robocopy();
    }
    if src.exists() {
        let (_, reste) = taille(src);
        return Err(format!("partiel : {reste} fichier(s) verrouillé(s) restés en place (code robocopy {code}). Droits administrateur ou fermeture du logiciel requis"));
    }
    Ok(())
}

fn ajouter_manifeste(lot: &Path, entree: serde_json::Value) -> Result<(), String> {
    let f = lot.join("manifeste.json");
    let mut v: Vec<serde_json::Value> = std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v.push(entree);
    let t = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(f, t).map_err(|e| format!("Écriture du manifeste impossible : {e}"))?;
    let nom = nom_dossier(lot);
    std::fs::write(lot.join(".disklens-lot.json"), serde_json::json!({"schema":"disklens-residus-v1", "lot":nom}).to_string())
        .map_err(|e| format!("Marqueur de lot impossible : {e}"))
}

fn quarantaine(dir: &Path, chemins: &[String]) -> Vec<QuarResult> {
    let ts = horodatage();
    let lot = racine_quarantaine().join(&ts);
    let derniers: Vec<Residu> = DERNIER.lock().map(|d| d.clone()).unwrap_or_default();
    let mut installes: Vec<String> = INSTALLES.lock().map(|d| d.clone()).unwrap_or_default();
    let mut emplacements: Vec<String> = EMPLACEMENTS.lock().map(|d| d.clone()).unwrap_or_default();
    // Une réinstallation peut avoir eu lieu depuis le scan : réinventorier avant tout déplacement.
    for l in inventaire().into_iter().filter(|l| l.etat == "installe") {
        installes.push(l.nom);
        if l.install_location.contains(':') {
            emplacements.push(l.install_location.trim_matches('"').replace('/', "\\").trim_end_matches('\\').to_lowercase());
        }
    }
    let lot_s = lot.to_string_lossy().to_string();
    let mut res = Vec::new();
    for chemin in chemins {
        let r = |ok: bool, detail: String| QuarResult { chemin: chemin.clone(), ok, detail, lot: lot_s.clone() };
        let fin = |res: &mut Vec<QuarResult>, x: QuarResult| {
            journal(dir, serde_json::json!({ "ts": ts, "chemin": x.chemin, "ok": x.ok, "detail": x.detail, "lot": x.lot }));
            res.push(x);
        };
        let Some(e) = derniers.iter().find(|e| e.chemin.eq_ignore_ascii_case(chemin)) else {
            fin(&mut res, r(false, "Refusé : élément absent du dernier scan (relance l'analyse)".into()));
            continue;
        };
        let src_valide = match crate::clean::validate_action_path(chemin) {
            Ok(p) => p,
            Err(er) => { fin(&mut res, r(false, format!("Refusé : {er}"))); continue; }
        };
        if let Some(p) = e.protege.clone().or_else(|| proteger(chemin, &installes, &emplacements)) {
            fin(&mut res, r(false, format!("Refusé : protégé ({p})")));
            continue;
        }
        let src = PathBuf::from(&src_valide);
        if !src.is_dir() || std::fs::symlink_metadata(&src).map(|m| reparse(&m)).unwrap_or(true) {
            fin(&mut res, r(false, "Refusé : dossier introuvable ou point de jonction".into()));
            continue;
        }
        let dest = lot.join(src_valide.replacen(':', "", 1).trim_start_matches('\\'));
        if let Err(er) = crate::clean::validate_action_path(&lot.to_string_lossy())
            .and_then(|_| crate::clean::validate_action_path(&dest.to_string_lossy())) {
            fin(&mut res, r(false, format!("Quarantaine refusée : {er}")));
            continue;
        }
        if let Err(er) = std::fs::create_dir_all(&dest) {
            fin(&mut res, r(false, format!("Quarantaine impossible : {er}")));
            continue;
        }
        match deplacer(&src, &dest) {
            Ok(()) => {
                match ajouter_manifeste(&lot, serde_json::json!({ "chemin": chemin, "octets": e.octets, "fichiers": e.fichiers, "logiciel": e.logiciel_id, "date": ts })) {
                    Ok(()) => fin(&mut res, r(true, "Déplacé en quarantaine".into())),
                    Err(er) => fin(&mut res, r(false, format!("Déplacé, mais {er}. Ne purge pas ce lot avant vérification."))),
                }
            }
            Err(er) => fin(&mut res, r(false, er)),
        }
    }
    if let Ok(mut d) = DERNIER.lock() {
        let faits: HashSet<String> = res.iter().filter(|x| x.ok).map(|x| x.chemin.to_lowercase()).collect();
        d.retain(|e| !faits.contains(&e.chemin.to_lowercase()));
    }
    res
}

fn racines_quarantaine() -> Vec<PathBuf> {
    let sd = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    vec![PathBuf::from("D:\\Quarantaine_Nettoyage"), PathBuf::from(format!("{sd}\\Quarantaine_Nettoyage"))]
}

fn lister_lots() -> Vec<Lot> {
    let mut v = Vec::new();
    for racine in racines_quarantaine() {
        for d in sous_dossiers(&racine) {
            let age = std::fs::metadata(&d).and_then(|m| m.created()).ok().and_then(|c| c.elapsed().ok()).map(|e| (e.as_secs() / 86_400) as u32).unwrap_or(0);
            v.push(Lot { nom: nom_dossier(&d), chemin: d.to_string_lossy().to_string(), age_jours: age, octets: taille(&d).0 });
        }
    }
    v.sort_by(|a, b| b.age_jours.cmp(&a.age_jours));
    v
}

fn lot_valide(nom: &str) -> bool {
    nom.len() == 15 && nom.chars().enumerate().all(|(i, c)| if i == 8 { c == '_' } else { c.is_ascii_digit() })
}

fn verifier_lot(p: &Path, nom: &str) -> Result<(), String> {
    let refus = || "Lot sans manifeste DiskLens valide : purge refusée".to_string();
    crate::clean::validate_action_path(&p.to_string_lossy())?;
    let marqueur = p.join(".disklens-lot.json");
    let manifeste = p.join("manifeste.json");
    for f in [&marqueur, &manifeste] {
        if std::fs::symlink_metadata(f).map(|m| !m.is_file() || reparse(&m)).unwrap_or(true) { return Err(refus()); }
    }
    let marque: serde_json::Value = serde_json::from_slice(&std::fs::read(&marqueur).map_err(|_| refus())?).map_err(|_| refus())?;
    if marque.get("schema").and_then(|x| x.as_str()) != Some("disklens-residus-v1")
        || marque.get("lot").and_then(|x| x.as_str()) != Some(nom) { return Err(refus()); }
    let entrees: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifeste).map_err(|_| refus())?).map_err(|_| refus())?;
    let entrees = entrees.as_array().filter(|a| !a.is_empty()).ok_or_else(refus)?;
    let mut destinations = Vec::new();
    for e in entrees {
        let source = e.get("chemin").and_then(|x| x.as_str()).ok_or_else(refus)?;
        if e.get("date").and_then(|x| x.as_str()) != Some(nom)
            || e.get("logiciel").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).is_none() { return Err(refus()); }
        let source = crate::clean::validate_action_path(source).map_err(|_| refus())?;
        destinations.push(p.join(source.replacen(':', "", 1).trim_start_matches('\\')));
    }
    let mut pile = vec![p.to_path_buf()];
    while let Some(d) = pile.pop() {
        for entree in std::fs::read_dir(&d).map_err(|_| refus())? {
            let entree = entree.map_err(|_| refus())?;
            let chemin = entree.path();
            let md = std::fs::symlink_metadata(&chemin).map_err(|_| refus())?;
            if reparse(&md) { return Err("Point de jonction dans le lot : purge refusée".into()); }
            if md.is_dir() { pile.push(chemin); }
            else if chemin != marqueur && chemin != manifeste
                && !destinations.iter().any(|dest| chemin.starts_with(dest)) { return Err(refus()); }
        }
    }
    Ok(())
}

fn purger(dir: &Path, nom: &str) -> Result<(), String> {
    if !lot_valide(nom) {
        return Err("Nom de lot invalide".into());
    }
    for racine in racines_quarantaine() {
        let p = racine.join(nom);
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if !md.is_dir() || reparse(&md) {
            return Err("Ce n'est pas un lot de quarantaine".into());
        }
        verifier_lot(&p, nom)?;
        std::fs::remove_dir_all(&p).map_err(|e| format!("Suppression impossible : {e}"))?;
        journal(dir, serde_json::json!({ "purge": p.to_string_lossy() }));
        return Ok(());
    }
    Err("Lot introuvable".into())
}

fn arguments_windows(s: &str) -> Option<Vec<String>> {
    let (mut args, mut mot, mut guillemets) = (Vec::new(), String::new(), false);
    for c in s.chars() {
        match c {
            '"' => guillemets = !guillemets,
            c if c.is_whitespace() && !guillemets => { if !mot.is_empty() { args.push(std::mem::take(&mut mot)); } },
            _ => mot.push(c),
        }
    }
    if guillemets { return None; }
    if !mot.is_empty() { args.push(mot); }
    Some(args)
}

fn preparer_desinstallation(commande: &str) -> Result<(String, Vec<String>), String> {
    let s = commande.trim();
    let fin_exe = s.to_ascii_lowercase().find(".exe").ok_or("Désinstalleur non exécutable (.exe requis)")? + 4;
    let exe = s[..fin_exe].trim().trim_matches('"').to_string();
    let args = arguments_windows(s[fin_exe..].trim().trim_start_matches('"').trim())
        .ok_or("Guillemets invalides dans la commande de désinstallation")?;
    let nom = exe.replace('/', "\\").rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
    if nom == "msiexec.exe" {
        let mut iter = args.iter();
        let premier = iter.next().ok_or("Identifiant MSI absent")?;
        let (mode, produit) = if premier.len() > 2 { (&premier[..2], &premier[2..]) }
            else { (premier.as_str(), iter.next().map(String::as_str).unwrap_or("")) };
        if !mode.eq_ignore_ascii_case("/x") && !mode.eq_ignore_ascii_case("/i") {
            return Err("Commande MSI sans identifiant de produit à désinstaller".into());
        }
        let guid = produit.trim_matches(|c| c == '{' || c == '}');
        if guid.len() != 36 || !guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err("Identifiant de produit MSI invalide".into());
        }
        let systeme = std::env::var("SystemRoot").map_err(|_| "Dossier Windows introuvable")?;
        let msiexec = Path::new(&systeme).join("System32").join("msiexec.exe");
        let msiexec = crate::clean::validate_action_path(&msiexec.to_string_lossy())?;
        if !Path::new(&msiexec).is_file() { return Err("Windows Installer introuvable".into()); }
        return Ok((msiexec, vec!["/x".into(), format!("{{{guid}}}")]));
    }
    if ["cmd.exe", "powershell.exe", "pwsh.exe", "wscript.exe", "cscript.exe", "mshta.exe", "rundll32.exe", "regsvr32.exe"]
        .contains(&nom.as_str()) {
        return Err("Désinstallation par interpréteur ou DLL non prise en charge".into());
    }
    if !exe.to_ascii_lowercase().ends_with(".exe") || !Path::new(&exe).is_absolute() {
        return Err("Chemin absolu du désinstalleur requis".into());
    }
    let exe = crate::clean::validate_action_path(&exe)?;
    if !Path::new(&exe).is_file() { return Err("Désinstalleur introuvable".into()); }
    Ok((exe, args))
}

fn meme_produit(a: &str, b: &str) -> bool {
    let annees: Vec<&str> = a.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() == 4 && s.starts_with("20") && s.chars().all(|c| c.is_ascii_digit())).collect();
    compact(a) == compact(b) && annees.iter().all(|annee| b.contains(annee))
}

fn verifier_fin_desinstallation(original: &Logiciel, courants: &[Logiciel]) -> Result<(), String> {
    if courants.iter().any(|l| l.id == original.id || (l.etat == "installe" && meme_produit(&original.nom, &l.nom))) {
        return Err(format!("« {} » est encore enregistré. Termine le désinstalleur, ou redémarre si demandé, puis réessaie. Aucun nettoyage n'a été lancé.", original.nom));
    }
    Ok(())
}

fn chercher_apres_desinstallation(id: &str) -> Result<ResidusReport, String> {
    let original = SUIVIS.lock().map_err(|_| "Suivi indisponible")?.as_ref()
        .and_then(|s| s.get(id)).cloned().ok_or("Aucune désinstallation suivie pour ce logiciel dans cette session")?;
    verifier_fin_desinstallation(&original, &inventaire())?;
    chercher_produit(&original.nom, &original.editeur)
}

fn desinstaller(id: &str) -> Result<String, String> {
    // Le client ne transmet que l'id ; toute la commande provient d'une nouvelle lecture du registre.
    let logiciel = inventaire().into_iter().find(|l| l.id == id && l.etat == "installe")
        .ok_or("Logiciel installé introuvable après réinventaire")?;
    if logiciel.desinstalleur.trim().is_empty() { return Err("Aucun désinstalleur officiel enregistré".into()); }
    let (exe, args) = preparer_desinstallation(&logiciel.desinstalleur)?;
    Command::new(&exe).args(&args).spawn().map_err(|e| format!("Lancement impossible : {e}"))?;
    let message = format!("Désinstalleur officiel lancé pour {}. Termine ses étapes, puis utilise « Vérifier et chercher les restes ».", logiciel.nom);
    SUIVIS.lock().map_err(|_| "Désinstalleur lancé, mais suivi indisponible")?
        .get_or_insert_with(HashMap::new).insert(logiciel.id.clone(), logiciel);
    Ok(message)
}

// ---------- Commandes ----------

#[tauri::command]
pub async fn residus_scan(_st: State<'_, crate::AppState>) -> Result<ResidusReport, String> {
    tauri::async_runtime::spawn_blocking(scanner).await.map_err(|e| e.to_string())
}

/// Recherche manuelle des restes d'un logiciel déjà désinstallé (nom et éditeur saisis par l'utilisateur).
#[tauri::command]
pub async fn residus_chercher(_st: State<'_, crate::AppState>, nom: String, editeur: String) -> Result<ResidusReport, String> {
    tauri::async_runtime::spawn_blocking(move || chercher_manuel(&nom, &editeur)).await.map_err(|e| e.to_string())?
}

/// Déplace en quarantaine (robocopy /MOVE), journalise dans <data>/residus.log. Refuse les chemins protégés.
#[tauri::command]
pub async fn residus_clean(st: State<'_, crate::AppState>, chemins: Vec<String>) -> Result<Vec<QuarResult>, String> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || quarantaine(&dir, &chemins)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn residus_lots(_st: State<'_, crate::AppState>) -> Result<Vec<Lot>, String> {
    tauri::async_runtime::spawn_blocking(lister_lots).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn residus_purge_lot(st: State<'_, crate::AppState>, lot: String) -> Result<(), String> {
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || purger(&dir, &lot)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn logiciel_desinstaller(id: String, confirme: bool) -> Result<String, String> {
    if !confirme { return Err("Confirmation requise".into()); }
    tauri::async_runtime::spawn_blocking(move || desinstaller(&id)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn logiciel_residus_apres(id: String) -> Result<ResidusReport, String> {
    tauri::async_runtime::spawn_blocking(move || chercher_apres_desinstallation(&id)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(nom: &str, ed: &str) -> Logiciel {
        Logiciel { id: "x".into(), nom: nom.into(), editeur: ed.into(), version: String::new(), install_location: String::new(),
                   desinstalleur: String::new(), cle: String::new(), etat: "fantome".into(), raison: String::new(),
                   date_installation: None, derniere_utilisation_estimee: None, confiance_usage: "inconnue".into(), indice_usage: String::new(), origine_oem: None, confiance_oem: "inconnue".into() }
    }

    #[test]
    fn confiance_par_nom() {
        let a = l("Acme Tool 2.1 (x64)", "Acme Corp");
        assert_eq!(lien_nom("Acme Tool", &a, true), Some("sur"));
        assert_eq!(lien_nom("Acme Corp", &a, true), Some("probable"));
        assert_eq!(lien_nom("Acme Corp", &a, false), None);
        assert_eq!(lien_nom("Autre", &a, true), None);
        let r = l("Revit 2024", "Autodesk");
        assert_eq!(lien_nom("Revit", &r, false), Some("probable"));
        assert_eq!(lien_nom("Revit 2024", &r, false), Some("sur"));
    }

    #[test]
    fn suivi_refuse_logiciel_encore_enregistre_meme_fantome() {
        let original = l("Revit 2024", "Autodesk");
        assert!(verifier_fin_desinstallation(&original, &[original.clone()]).is_err());
        assert!(verifier_fin_desinstallation(&original, &[]).is_ok());
        let mut nouvelle_version = l("Revit 2027", "Autodesk");
        nouvelle_version.id = "autre".into();
        nouvelle_version.etat = "installe".into();
        assert!(verifier_fin_desinstallation(&original, &[nouvelle_version]).is_ok());
    }

    #[test]
    fn protection_systeme_et_installes() {
        assert!(proteger("C:\\Windows\\System32", &[], &[]).is_some());
        assert!(proteger("C:\\Program Files", &[], &[]).is_some());
        assert!(proteger("C:\\ProgramData\\Microsoft\\Foo", &[], &[]).is_some());
        assert!(proteger(r"C:\Program Files\Acme Tool", &["acmetool".into()], &[]).is_some());
        assert!(proteger("C:\\Program Files\\Acme Tool", &[], &[]).is_none());
        assert!(proteger(r"C:\Program Files\Acme", &[], &[r"c:\program files\acme\tool".into()]).is_some());
    }

    #[test]
    fn protection_distingue_ancienne_version_et_dossier_commun() {
        let installes = vec!["Revit 2027".into()];
        assert!(proteger(r"C:\DiskLensFixture\Revit 2024", &installes, &[]).is_none());
        assert!(proteger(r"C:\DiskLensFixture\Revit 2027", &installes, &[]).is_some());
        assert!(proteger(r"C:\DiskLensFixture\Revit", &installes, &[]).is_some());
    }

    #[test]
    fn lot_refuse_traversee() {
        assert!(!lot_valide("..\\Windows"));
        assert!(!lot_valide("a/b"));
        assert!(!lot_valide(""));
        assert!(lot_valide("20261020_101500"));
    }

    #[test]
    fn exe_extrait() {
        assert_eq!(extraire_exe("\"D:\\App\\unins000.exe\" /SILENT"), "D:\\App\\unins000.exe");
        assert_eq!(extraire_exe("MsiExec.exe /X{ABC}"), "");
        assert_eq!(extraire_exe("D:\\App\\app.exe,0"), "D:\\App\\app.exe");
    }

    #[test]
    fn dates_et_userassist_sans_heuristique_usage() {
        assert_eq!(date_installation("20240229"), Some("2024-02-29".into()));
        assert_eq!(date_installation("20230229"), None);
        assert_eq!(date_installation("20241301"), None);
        assert_eq!(rot13(r"P:\Cebtenz\ncc.rkr"), r"C:\Program\app.exe");
        let mut b = vec![0u8; 72];
        let ft = (1_577_836_800u64 + 11_644_473_600) * 10_000_000;
        b[60..68].copy_from_slice(&ft.to_le_bytes());
        assert_eq!(userassist_date(&b), Some("2020-01-01".into()));
        assert_eq!(userassist_date(&b[..68]), None);
        b[60..68].fill(0);
        assert_eq!(userassist_date(&b), None);
    }

    #[test]
    fn attribution_usage_et_oem_prudente() {
        let mut logiciel = l("Acme", "Acme");
        logiciel.install_location = r"C:\Program Files\Acme".into();
        logiciel.date_installation = Some("2020-01-01".into());
        enrichir_logiciel(&mut logiciel, "", &[], "Dell Inc.");
        assert_eq!(logiciel.derniere_utilisation_estimee, None);
        assert_eq!(logiciel.origine_oem, None);
        let traces = vec![(r"c:\program files\acme\acme.exe".into(), "2024-06-01".into())];
        enrichir_logiciel(&mut logiciel, "", &traces, "Acme");
        assert_eq!(logiciel.derniere_utilisation_estimee.as_deref(), Some("2024-06-01"));
        assert_eq!(logiciel.confiance_usage, "faible");
        assert_eq!(logiciel.origine_oem.as_deref(), Some("Acme"));
    }

    #[test]
    fn desinstalleur_msi_controle() {
        let guid = "00112233-4455-6677-8899-AABBCCDDEEFF";
        let (exe, args) = preparer_desinstallation(&format!("MsiExec.exe /I{{{guid}}} /quiet")).unwrap();
        assert!(exe.to_ascii_lowercase().ends_with(r"\system32\msiexec.exe"));
        assert_eq!(args, vec!["/x".to_string(), format!("{{{guid}}}")]);
        assert!(preparer_desinstallation("cmd.exe /c del C:\\data").is_err());
        assert!(preparer_desinstallation(r"C:\Windows\System32\cmd.exe /c test").is_err());
        assert!(preparer_desinstallation("MsiExec.exe /x something").is_err());
    }

    #[test]
    fn purge_exige_marqueur_et_manifeste_de_l_application() {
        let nom = "20261026_120000";
        let lot = std::env::temp_dir().join(format!("disklens-test-{}-{}", std::process::id(), nom)).join(nom);
        std::fs::create_dir_all(&lot).unwrap();
        assert!(verifier_lot(&lot, nom).is_err());
        std::fs::write(lot.join(".disklens-lot.json"), serde_json::json!({"schema":"disklens-residus-v1", "lot":nom}).to_string()).unwrap();
        std::fs::write(lot.join("manifeste.json"), serde_json::json!([{"chemin":"C:\\DiskLensFixture\\Acme", "logiciel":"abc", "date":nom}]).to_string()).unwrap();
        assert!(verifier_lot(&lot, nom).is_ok());
        std::fs::write(lot.join("manifeste.json"), serde_json::json!([{"chemin":"C:\\DiskLensFixture\\..\\Windows", "logiciel":"abc", "date":nom}]).to_string()).unwrap();
        assert!(verifier_lot(&lot, nom).is_err());
        std::fs::remove_dir_all(&lot).unwrap();
    }

    /// Scan réel en lecture seule : `cargo test --lib scan_reel -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_reel() {
        let r = scanner();
        let f: Vec<_> = r.logiciels.iter().filter(|l| l.etat == "fantome").collect();
        println!("{} logiciels, {} fantômes, {} résidus", r.logiciels.len(), f.len(), r.residus.len());
        for l in &f {
            println!("FANTOME {} | {} | {}", l.nom, l.editeur, l.cle);
        }
        for x in r.residus.iter().take(40) {
            println!("{} {} {:.1}Mo prot={:?} | {}", x.confiance, x.chemin, x.octets as f64 / 1048576.0, x.protege, x.motif);
        }
    }

    /// Recherche réelle en lecture seule : `cargo test --lib chercher_reel -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn chercher_reel() {
        for (n, e) in [("Revit 2024", "Autodesk"), ("Advance Steel 2025", "Autodesk")] {
            match chercher_manuel(n, e) {
                Ok(r) => {
                    println!("== {n} : {} résidus", r.residus.len());
                    for x in r.residus.iter().take(15) {
                        println!("{} {} {:.1}Mo prot={:?}", x.confiance, x.chemin, x.octets as f64 / 1048576.0, x.protege);
                    }
                }
                Err(er) => println!("== {n} : {er}"),
            }
        }
    }
}
