//! Résidus logiciels — étape 2 : registre. Contrat : CONTRAT_RESIDUS.md (ne pas changer les signatures).
use crate::residus::Logiciel;
use serde::Serialize;
use std::collections::HashSet;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::State;
use winreg::enums::*;
use winreg::RegKey;

const NO_WINDOW: u32 = 0x0800_0000;
const CV: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion";

#[derive(Serialize, Clone)]
pub struct CleReg {
    pub id: String,
    pub logiciel_id: String,
    /// Chemin complet, ex. "HKCU\Software\Acme\Tool"
    pub chemin: String,
    /// "software" | "classes" | "app_paths" | "run" | "service" | "installer" | "uninstall"
    pub genre: String,
    /// "sur" | "probable" | "verifier"
    pub confiance: String,
    pub motif: String,
    pub protege: Option<String>,
    pub valeurs: u32,
}

#[derive(Serialize)]
pub struct RegResult {
    pub chemin: String,
    pub ok: bool,
    pub detail: String,
    /// Fichier .reg exporté avant suppression
    pub export: Option<String>,
}

/// Dernier scan conservé : seules ces clés (non protégées) peuvent être nettoyées.
static DERNIER: Mutex<Vec<CleReg>> = Mutex::new(Vec::new());

// ---------- Normalisation / protection (fonctions pures) ----------

/// Nom normalisé : minuscules, sans parenthèses, version, année, « x64 », symboles.
pub fn normaliser(s: &str) -> String {
    let mut sans = String::new();
    let mut prof = 0u32;
    for c in s.chars() {
        match c {
            '(' | '[' => prof += 1,
            ')' | ']' => prof = prof.saturating_sub(1),
            _ if prof == 0 => sans.push(c),
            _ => {}
        }
    }
    let mut mots: Vec<String> = Vec::new();
    for m in sans.split(|c: char| !c.is_alphanumeric()).filter(|m| !m.is_empty()) {
        let m = m.to_lowercase();
        let chiffres = m.chars().all(|c| c.is_ascii_digit());
        let version = m.len() > 1 && m.starts_with('v') && m[1..].chars().all(|c| c.is_ascii_digit());
        let arch = matches!(m.as_str(), "x64" | "x86" | "amd64" | "win64" | "win32" | "64bit" | "32bit" | "bit" | "bits");
        if (chiffres && !mots.is_empty()) || version || arch {
            continue;
        }
        mots.push(m);
    }
    mots.join(" ")
}

fn compact(s: &str) -> String {
    normaliser(s).replace(' ', "")
}

fn egal(a: &str, b: &str) -> bool {
    let (a, b) = (compact(a), compact(b));
    !a.is_empty() && a == b
}

fn partiel(a: &str, b: &str) -> bool {
    let (a, b) = (compact(a), compact(b));
    a.len() >= 5 && b.len() >= 5 && (a.contains(&b) || b.contains(&a))
}

/// Racine nue ou conteneur générique du registre : jamais supprimable.
pub fn racine_nue(chemin: &str) -> bool {
    let c = chemin.trim().trim_end_matches('\\').to_lowercase();
    if c.contains("::") {
        return false; // valeur Run précise
    }
    let segs: Vec<&str> = c.split('\\').filter(|s| !s.is_empty()).collect();
    if segs.len() <= 2 {
        return true;
    }
    const CONTENEURS: [&str; 24] = [
        "software", "system", "classes", "wow6432node", "microsoft", "windows", "currentversion", "currentcontrolset",
        "services", "run", "runonce", "app paths", "uninstall", "installer", "userdata", "products", "components",
        "clsid", "appid", "typelib", "interface", "features", "policies", "explorer",
    ];
    segs.last().map_or(true, |d| CONTENEURS.contains(d))
}

fn motif_partage(s: &str) -> bool {
    let l = s.to_lowercase();
    ["licens", "genuine", "identity", "redistributable", "visual c++", "vcredist", ".net", "webview2", "directx", "runtime"]
        .iter()
        .any(|m| l.contains(m))
}

fn est_microsoft(s: &str) -> bool {
    let n = s.trim().to_lowercase();
    n.starts_with("microsoft") || n.starts_with("windows")
}

/// Nom de la clé « significative » du chemin (1er segment sous SOFTWARE pour `software`, sinon le dernier).
fn nom_cle(chemin: &str, genre: &str) -> String {
    let sans_val = chemin.split("::").next().unwrap_or(chemin);
    let segs: Vec<&str> = sans_val.split('\\').filter(|s| !s.is_empty()).collect();
    if genre == "software" {
        return segs
            .iter()
            .skip(1)
            .find(|s| {
                let l = s.to_lowercase();
                l != "software" && l != "wow6432node"
            })
            .map(|s| s.to_string())
            .unwrap_or_default();
    }
    segs.last().map(|s| s.to_string()).unwrap_or_default()
}

/// Règles de protection indépendantes du logiciel (aussi revérifiées avant suppression).
fn protection_chemin(chemin: &str) -> Option<String> {
    if racine_nue(chemin) {
        return Some("Racine du registre ou conteneur partagé".into());
    }
    let l = chemin.to_lowercase();
    if l.starts_with("hklm\\system") {
        let segs: Vec<&str> = l.split('\\').collect();
        let svc = segs.len() == 5 && segs[2] == "currentcontrolset" && segs[3] == "services";
        if !svc {
            return Some("Clé HKLM\\SYSTEM (hors service)".into());
        }
    }
    None
}

struct Global {
    ed_installes: HashSet<String>,
    noms_installes: HashSet<String>,
}

struct Ctx<'a> {
    l: &'a Logiciel,
    nom: String,
    ed: String,
    /// InstallLocation en minuscules, avec « \ » final ; vide si trop générique
    inst: String,
}

impl<'a> Ctx<'a> {
    fn new(l: &'a Logiciel) -> Self {
        Ctx { l, nom: l.nom.clone(), ed: l.editeur.clone(), inst: inst_normalise(&l.install_location) }
    }

    fn sous_install(&self, cible: &str) -> bool {
        sous_dossier(cible, &self.inst)
    }

    /// Confiance du lien entre un nom de clé et le logiciel.
    fn lien_nom(&self, n: &str) -> Option<&'static str> {
        if egal(n, &self.nom) {
            Some("sur")
        } else if egal(n, &self.ed) {
            Some("probable")
        } else if partiel(n, &self.nom) {
            Some("verifier")
        } else {
            None
        }
    }

    /// Confiance du lien entre une cible (chemin/commande) et le logiciel.
    fn lien_chemin(&self, cible: &str) -> Option<&'static str> {
        if self.sous_install(cible) {
            return Some("sur");
        }
        let c = compact(cible);
        let (n, e) = (compact(&self.nom), compact(&self.ed));
        if n.len() >= 5 && c.contains(&n) {
            Some("probable")
        } else if e.len() >= 5 && c.contains(&e) {
            Some("verifier")
        } else {
            None
        }
    }

    /// Cible inexistante (ou sous InstallLocation) et liée au logiciel -> confiance.
    fn evaluer(&self, cible: &str, nom: &str) -> Option<&'static str> {
        if cible.is_empty() {
            return None;
        }
        let sous = self.sous_install(cible);
        if !sous && Path::new(cible).exists() {
            return None;
        }
        if sous {
            return Some("sur");
        }
        self.lien_chemin(cible).or_else(|| self.lien_nom(nom))
    }
}

fn inst_normalise(loc: &str) -> String {
    let mut p = loc.trim().trim_matches('"').trim_end_matches('\\').to_lowercase();
    let segs = p.split('\\').filter(|s| !s.is_empty()).count();
    if segs < 3 || p.starts_with("c:\\windows") {
        return String::new(); // trop générique : ne prouve rien
    }
    p.push('\\');
    p
}

fn sous_dossier(cible: &str, inst: &str) -> bool {
    !inst.is_empty() && cible.trim().to_lowercase().replace('/', "\\").starts_with(inst)
}

fn etendre_env(s: &str) -> String {
    let mut out = String::new();
    let mut reste = s;
    while let Some(i) = reste.find('%') {
        out.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        match apres.find('%') {
            Some(j) => {
                match std::env::var(&apres[..j]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(&apres[..j]);
                        out.push('%');
                    }
                }
                reste = &apres[j + 1..];
            }
            None => {
                out.push('%');
                reste = apres;
                break;
            }
        }
    }
    out.push_str(reste);
    out
}

/// Extrait l'exécutable d'une ligne de commande (guillemets, arguments, variables d'environnement).
pub fn extraire_exe(cmd: &str) -> String {
    let c = cmd.trim().trim_start_matches("\\??\\");
    let brut = if let Some(r) = c.strip_prefix('"') {
        r.split('"').next().unwrap_or("").to_string()
    } else {
        let low = c.to_ascii_lowercase();
        match low.find(".exe") {
            Some(i) => c[..i + 4].to_string(),
            None => c.split_whitespace().next().unwrap_or("").to_string(),
        }
    };
    let e = etendre_env(&brut);
    // Chemin relatif (ex. system32\drivers\x.sys) ou système : on ne juge pas.
    if !e.contains(':') || e.to_lowercase().starts_with("c:\\windows") {
        return String::new();
    }
    e
}

fn protege_cle(chemin: &str, genre: &str, c: &Ctx, g: &Global, niveau_editeur: bool) -> Option<String> {
    if let Some(p) = protection_chemin(chemin) {
        return Some(p);
    }
    let k = nom_cle(chemin, genre);
    if est_microsoft(&c.ed) || est_microsoft(&k) {
        return Some("Microsoft / Windows".into());
    }
    if motif_partage(&c.nom) || motif_partage(&k) {
        return Some("Composant partagé".into());
    }
    let ed = normaliser(&c.ed);
    if niveau_editeur && !ed.is_empty() && g.ed_installes.contains(&ed) {
        return Some("L'éditeur a encore un logiciel installé".into());
    }
    if g.noms_installes.contains(&compact(&c.nom)) {
        return Some("Même nom qu'un logiciel installé".into());
    }
    None
}

// ---------- Accès registre ----------

fn hive(nom: &str) -> RegKey {
    match nom {
        "HKCU" => RegKey::predef(HKEY_CURRENT_USER),
        "HKCR" => RegKey::predef(HKEY_CLASSES_ROOT),
        _ => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

fn ouvrir(chemin: &str) -> Option<RegKey> {
    let (h, sub) = chemin.split_once('\\')?;
    hive(h).open_subkey_with_flags(sub, KEY_READ).ok()
}

fn sous_cles(k: &RegKey) -> Vec<String> {
    k.enum_keys().filter_map(|r| r.ok()).collect()
}

fn defaut(chemin: &str) -> String {
    ouvrir(chemin).and_then(|k| k.get_value::<String, _>("").ok()).unwrap_or_default()
}

fn nb_valeurs(chemin: &str) -> u32 {
    ouvrir(chemin).map_or(0, |k| k.enum_values().filter_map(|r| r.ok()).count() as u32)
}

fn hash_court(s: &str) -> String {
    blake3::hash(s.to_lowercase().as_bytes()).to_hex()[..12].to_string()
}

// ---------- Inventaire (une seule passe, partagé par tous les fantômes) ----------

#[derive(Default)]
struct Inv {
    /// (chemin clé, nom de la clé, cible)
    app_paths: Vec<(String, String, String)>,
    /// (chemin "clé::Valeur", nom valeur, cible)
    runs: Vec<(String, String, String)>,
    /// (chemin, nom affiché, cible)
    services: Vec<(String, String, String)>,
    classes: Vec<(String, String, String)>,
    /// (chemin, nom du produit)
    installer: Vec<(String, String)>,
}

fn inventaire() -> Inv {
    let mut inv = Inv::default();
    let wow = "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion";
    let bases = [("HKCU".to_string(), CV.to_string()), ("HKLM".to_string(), CV.to_string()), ("HKLM".to_string(), wow.to_string())];
    for (h, cv) in &bases {
        // App Paths
        let ap = format!("{h}\\{cv}\\App Paths");
        if let Some(k) = ouvrir(&ap) {
            for n in sous_cles(&k) {
                let chemin = format!("{ap}\\{n}");
                let cible = extraire_exe(&defaut(&chemin));
                inv.app_paths.push((chemin, n, cible));
            }
        }
        // Run / RunOnce
        for run in ["Run", "RunOnce"] {
            let base = format!("{h}\\{cv}\\{run}");
            if let Some(k) = ouvrir(&base) {
                for (nom, _) in k.enum_values().filter_map(|x| x.ok()) {
                    let brut = k.get_value::<String, _>(&nom).unwrap_or_default();
                    inv.runs.push((format!("{base}::{nom}"), nom, extraire_exe(&brut)));
                }
            }
        }
    }

    // Services
    if let Some(k) = ouvrir("HKLM\\SYSTEM\\CurrentControlSet\\Services") {
        for n in sous_cles(&k) {
            let chemin = format!("HKLM\\SYSTEM\\CurrentControlSet\\Services\\{n}");
            if let Some(sk) = ouvrir(&chemin) {
                let img: String = sk.get_value("ImagePath").unwrap_or_default();
                let aff: String = sk.get_value("DisplayName").unwrap_or_default();
                let cible = extraire_exe(&img);
                if !cible.is_empty() {
                    inv.services.push((chemin, if aff.starts_with('@') || aff.is_empty() { n } else { aff }, cible));
                }
            }
        }
    }

    // Classes : CLSID (serveurs) et ProgID (commande open)
    for base in ["HKLM\\SOFTWARE\\Classes", "HKLM\\SOFTWARE\\Classes\\WOW6432Node", "HKCU\\Software\\Classes"] {
        if let Some(k) = ouvrir(&format!("{base}\\CLSID")) {
            for g in sous_cles(&k) {
                let chemin = format!("{base}\\CLSID\\{g}");
                let cible = ["InprocServer32", "LocalServer32"]
                    .iter()
                    .map(|s| extraire_exe(&defaut(&format!("{chemin}\\{s}"))))
                    .find(|c| !c.is_empty())
                    .unwrap_or_default();
                if !cible.is_empty() {
                    inv.classes.push((chemin.clone(), defaut(&chemin), cible));
                }
            }
        }
    }
    for base in ["HKLM\\SOFTWARE\\Classes", "HKCU\\Software\\Classes"] {
        if let Some(k) = ouvrir(base) {
            for n in sous_cles(&k) {
                if n.starts_with('.') || matches!(n.as_str(), "CLSID" | "Interface" | "TypeLib" | "AppID" | "Installer" | "WOW6432Node") {
                    continue;
                }
                let chemin = format!("{base}\\{n}");
                let cible = extraire_exe(&defaut(&format!("{chemin}\\shell\\open\\command")));
                if !cible.is_empty() {
                    inv.classes.push((chemin, n, cible));
                }
            }
        }
    }

    // Windows Installer
    if let Some(k) = ouvrir(&format!("HKLM\\{CV}\\Installer\\UserData")) {
        for sid in sous_cles(&k) {
            let prod = format!("HKLM\\{CV}\\Installer\\UserData\\{sid}\\Products");
            if let Some(pk) = ouvrir(&prod) {
                for g in sous_cles(&pk) {
                    let ch = format!("{prod}\\{g}");
                    let nom = ouvrir(&format!("{ch}\\InstallProperties"))
                        .and_then(|x| x.get_value::<String, _>("DisplayName").ok())
                        .unwrap_or_default();
                    if !nom.is_empty() {
                        inv.installer.push((ch, nom));
                    }
                }
            }
        }
    }
    if let Some(k) = ouvrir("HKCR\\Installer\\Products") {
        for g in sous_cles(&k) {
            let ch = format!("HKCR\\Installer\\Products\\{g}");
            let nom = ouvrir(&ch).and_then(|x| x.get_value::<String, _>("ProductName").ok()).unwrap_or_default();
            if !nom.is_empty() {
                inv.installer.push((ch, nom));
            }
        }
    }
    inv
}

// ---------- Scan ----------

struct Sortie<'g> {
    v: Vec<CleReg>,
    vus: HashSet<String>,
    g: &'g Global,
}

impl Sortie<'_> {
    #[allow(clippy::too_many_arguments)]
    fn ajouter(&mut self, c: &Ctx, chemin: String, genre: &str, confiance: &str, motif: String, valeurs: u32, niveau_editeur: bool) {
        if !self.vus.insert(chemin.to_lowercase()) {
            return;
        }
        let protege = protege_cle(&chemin, genre, c, self.g, niveau_editeur);
        // Éditeur encore présent ailleurs : on ne garantit plus « sûr ».
        let ed = normaliser(&c.ed);
        let confiance = if confiance == "sur" && !ed.is_empty() && self.g.ed_installes.contains(&ed) { "probable" } else { confiance };
        self.v.push(CleReg {
            id: hash_court(&chemin),
            logiciel_id: c.l.id.clone(),
            chemin,
            genre: genre.into(),
            confiance: confiance.into(),
            motif,
            protege,
            valeurs,
        });
    }
}

fn scanner_software(c: &Ctx, out: &mut Sortie) {
    let (nom_n, ed_n) = (normaliser(&c.nom), normaliser(&c.ed));
    for (h, sub) in [("HKCU", "Software"), ("HKLM", "SOFTWARE"), ("HKLM", "SOFTWARE\\WOW6432Node")] {
        let base = format!("{h}\\{sub}");
        let Some(k) = ouvrir(&base) else { continue };
        for n in sous_cles(&k) {
            if est_microsoft(&n) || n.eq_ignore_ascii_case("classes") || n.eq_ignore_ascii_case("wow6432node") {
                continue;
            }
            let chemin = format!("{base}\\{n}");
            let nn = normaliser(&n);
            if !ed_n.is_empty() && nn == ed_n {
                // Clé éditeur : on descend d'un niveau pour trouver le produit.
                if let Some(ek) = ouvrir(&chemin) {
                    for p in sous_cles(&ek) {
                        if let Some(conf) = c.lien_nom(&p).filter(|x| *x != "probable") {
                            out.ajouter(c, format!("{chemin}\\{p}"), "software", conf, format!("Clé produit « {p} » de l'éditeur « {n} »"), nb_valeurs(&format!("{chemin}\\{p}")), false);
                        }
                    }
                }
                out.ajouter(c, chemin.clone(), "software", "probable", format!("Clé de l'éditeur « {n} »"), nb_valeurs(&chemin), true);
            } else if !nom_n.is_empty() {
                if let Some(conf) = c.lien_nom(&n) {
                    out.ajouter(c, chemin.clone(), "software", conf, format!("Clé au nom du produit « {n} »"), nb_valeurs(&chemin), false);
                }
            }
        }
    }
}

fn scanner_inventaire(c: &Ctx, inv: &Inv, out: &mut Sortie) {
    for (ch, nom, cible) in &inv.app_paths {
        if let Some(conf) = c.evaluer(cible, nom) {
            out.ajouter(c, ch.clone(), "app_paths", conf, format!("App Paths vers « {cible} » (introuvable ou dans le dossier du logiciel)"), nb_valeurs(ch), false);
        }
    }
    for (ch, nom, cible) in &inv.runs {
        if let Some(conf) = c.evaluer(cible, nom) {
            out.ajouter(c, ch.clone(), "run", conf, format!("Démarrage automatique vers « {cible} »"), 1, false);
        }
    }
    for (ch, nom, cible) in &inv.services {
        if let Some(conf) = c.evaluer(cible, nom) {
            out.ajouter(c, ch.clone(), "service", conf, format!("Service dont l'exécutable est « {cible} »"), nb_valeurs(ch), false);
        }
    }
    for (ch, nom, cible) in &inv.classes {
        if let Some(conf) = c.evaluer(cible, nom) {
            out.ajouter(c, ch.clone(), "classes", conf, format!("Classe COM/ProgID dont la cible « {cible} » n'existe plus"), nb_valeurs(ch), false);
        }
    }
    for (ch, nom) in &inv.installer {
        if egal(nom, &c.nom) {
            out.ajouter(c, ch.clone(), "installer", "sur", format!("Produit Windows Installer « {nom} »"), nb_valeurs(ch), false);
        }
    }
}

fn scanner(logiciels: &[Logiciel]) -> Vec<CleReg> {
    let fantomes: Vec<&Logiciel> = logiciels.iter().filter(|l| l.etat == "fantome").collect();
    if fantomes.is_empty() {
        return vec![];
    }
    let g = Global {
        ed_installes: logiciels.iter().filter(|l| l.etat == "installe").map(|l| normaliser(&l.editeur)).filter(|e| !e.is_empty()).collect(),
        noms_installes: logiciels.iter().filter(|l| l.etat == "installe").map(|l| compact(&l.nom)).filter(|e| !e.is_empty()).collect(),
    };
    let inv = inventaire();
    let mut out = Sortie { v: vec![], vus: HashSet::new(), g: &g };
    for l in fantomes {
        let c = Ctx::new(l);
        // Clé Uninstall du logiciel lui-même
        if !l.cle.trim().is_empty() {
            out.ajouter(&c, l.cle.clone(), "uninstall", "sur", "Entrée de désinstallation du logiciel fantôme".into(), nb_valeurs(&l.cle), false);
        }
        if c.nom.trim().is_empty() && c.ed.trim().is_empty() {
            continue;
        }
        scanner_software(&c, &mut out);
        scanner_inventaire(&c, &inv, &mut out);
    }
    out.v
}

/// Lecture seule.
#[tauri::command]
pub async fn registre_scan(_st: State<'_, crate::AppState>, logiciels: Vec<Logiciel>) -> Result<Vec<CleReg>, String> {
    let res = tauri::async_runtime::spawn_blocking(move || scanner(&logiciels)).await.map_err(|e| e.to_string())?;
    if let Ok(mut d) = DERNIER.lock() {
        *d = res.clone();
    }
    Ok(res)
}

// ---------- Point de restauration ----------

/// Crée un point de restauration Windows (admin). Renvoie sa description.
#[tauri::command]
pub async fn registre_point_restauration() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(point_restauration).await.map_err(|e| e.to_string())?
}

fn point_restauration() -> Result<String, String> {
    const DESC: &str = "DiskLens avant nettoyage registre";
    let script = format!("& {{ Checkpoint-Computer -Description '{DESC}' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop }} *>&1");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("powershell introuvable: {e}"))?;
    let txt = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    interpreter_point(out.status.success(), &txt, DESC)
}

/// Interprète la sortie de Checkpoint-Computer (pur, testable).
fn interpreter_point(succes: bool, txt: &str, desc: &str) -> Result<String, String> {
    let l = txt.to_lowercase();
    // Limite Windows : 1 point / 24 h (1440 minutes) -> avertissement, pas une vraie erreur.
    if l.contains("1440") {
        return Ok("Un point de restauration récent existe déjà".into());
    }
    if succes && !l.contains("warning") && !l.contains("avertissement") {
        return Ok(desc.into());
    }
    if l.contains("denied") || l.contains("refus") || l.contains("administrat") || l.contains("privil") || l.contains("elevat") || l.contains("élév") {
        return Err("droits administrateur requis".into());
    }
    Err(format!("Point de restauration non créé: {}", txt.trim().chars().take(300).collect::<String>()))
}

// ---------- Nettoyage ----------

pub fn verifier_confirmation(confirme: bool) -> Result<(), String> {
    if confirme {
        Ok(())
    } else {
        Err("confirmation requise".into())
    }
}

/// Refuse si `confirme` est faux. Exporte chaque clé en .reg (reg export) avant `reg delete`.
#[tauri::command]
pub async fn registre_clean(st: State<'_, crate::AppState>, cles: Vec<String>, confirme: bool) -> Result<Vec<RegResult>, String> {
    verifier_confirmation(confirme)?;
    let dir = st.dir.clone();
    tauri::async_runtime::spawn_blocking(move || nettoyer(&dir, &cles)).await.map_err(|e| e.to_string())
}

pub fn horodatage() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (j, r) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    // jours -> date civile (algorithme de H. Hinnant)
    let z = j + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}_{:02}{:02}{:02}", r / 3600, (r % 3600) / 60, r % 60)
}

/// D:\Quarantaine_Nettoyage si D: a > 20 Go libres, sinon <SystemDrive>\Quarantaine_Nettoyage.
pub fn racine_quarantaine() -> PathBuf {
    let disques = sysinfo::Disks::new_with_refreshed_list();
    let d_ok = disques.list().iter().any(|d| {
        d.mount_point().to_string_lossy().to_uppercase().starts_with("D:") && d.available_space() > 20 * 1024 * 1024 * 1024
    });
    if d_ok {
        PathBuf::from("D:\\Quarantaine_Nettoyage")
    } else {
        let sd = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        PathBuf::from(format!("{sd}\\Quarantaine_Nettoyage"))
    }
}

fn reg(args: &[&str]) -> (bool, String) {
    match Command::new("reg.exe").args(args).creation_flags(NO_WINDOW).output() {
        Ok(o) => {
            let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            (o.status.success(), t.trim().to_string())
        }
        Err(e) => (false, format!("reg.exe introuvable: {e}")),
    }
}

fn acces_refuse(t: &str) -> bool {
    let l = t.to_lowercase();
    l.contains("denied") || l.contains("refus") || l.contains("access")
}

fn nom_fichier(cle: &str) -> String {
    let n = cle.rsplit('\\').next().unwrap_or("cle");
    let s: String = n.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    s.chars().take(60).collect()
}

fn journal(dir: &Path, v: serde_json::Value) {
    let f = crate::store::data_file(dir, "registre.log");
    if let Ok(mut h) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
        let _ = writeln!(h, "{v}");
    }
}

fn nettoyer(dir: &Path, cles: &[String]) -> Vec<RegResult> {
    let ts = horodatage();
    let lot = racine_quarantaine().join(&ts).join("registre");
    let lot_ok = std::fs::create_dir_all(&lot);
    let snapshot: Vec<CleReg> = DERNIER.lock().map(|d| d.clone()).unwrap_or_default();
    let mut res = Vec::new();

    for (i, chemin) in cles.iter().enumerate() {
        let r = |ok: bool, detail: String, export: Option<String>| RegResult { chemin: chemin.clone(), ok, detail, export };
        let fin = |res: &mut Vec<RegResult>, x: RegResult| {
            journal(dir, serde_json::json!({ "ts": ts, "cle": x.chemin, "export": x.export, "ok": x.ok, "detail": x.detail }));
            res.push(x);
        };

        // 1) Revalidation
        let Some(e) = snapshot.iter().find(|e| e.chemin.eq_ignore_ascii_case(chemin)) else {
            fin(&mut res, r(false, "Refusé : clé absente du dernier scan".into(), None));
            continue;
        };
        if let Some(p) = &e.protege {
            fin(&mut res, r(false, format!("Refusé : clé protégée ({p})"), None));
            continue;
        }
        if let Some(p) = protection_chemin(chemin) {
            fin(&mut res, r(false, format!("Refusé : {p}"), None));
            continue;
        }
        if let Err(er) = &lot_ok {
            fin(&mut res, r(false, format!("Dossier d'export impossible : {er}"), None));
            continue;
        }

        // 2) Export .reg AVANT toute suppression (valeur Run : on exporte la clé parente)
        let (cle, valeur) = match chemin.split_once("::") {
            Some((c, v)) => (c.to_string(), Some(v.to_string())),
            None => (chemin.clone(), None),
        };
        let fichier = lot.join(format!("{:02}_{}.reg", i + 1, nom_fichier(&cle)));
        let fstr = fichier.to_string_lossy().to_string();
        let (ok_exp, sortie) = reg(&["export", &cle, &fstr, "/y"]);
        if !ok_exp || !fichier.exists() {
            let d = if acces_refuse(&sortie) { "droits administrateur requis".to_string() } else { format!("Export échoué, rien supprimé : {sortie}") };
            fin(&mut res, r(false, d, None));
            continue;
        }

        // 3) Suppression (jamais /va)
        let (ok_del, sortie) = match &valeur {
            Some(v) => reg(&["delete", &cle, "/v", v, "/f"]),
            None => reg(&["delete", &cle, "/f"]),
        };
        if ok_del {
            fin(&mut res, r(true, "Supprimée (export conservé)".into(), Some(fstr)));
        } else {
            let d = if acces_refuse(&sortie) { "droits administrateur requis".to_string() } else { format!("Suppression échouée : {sortie}") };
            fin(&mut res, r(false, d, Some(fstr)));
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logiciel(nom: &str, ed: &str, etat: &str, loc: &str) -> Logiciel {
        Logiciel {
            id: "l1".into(),
            nom: nom.into(),
            editeur: ed.into(),
            version: "".into(),
            install_location: loc.into(),
            desinstalleur: "".into(),
            cle: "".into(),
            etat: etat.into(),
            raison: "".into(),
            date_installation: None,
            derniere_utilisation_estimee: None,
            confiance_usage: "inconnue".into(),
            indice_usage: String::new(),
            origine_oem: None,
            confiance_oem: "inconnue".into(),
        }
    }

    #[test]
    fn normalisation() {
        assert_eq!(normaliser("Acme Tool 2.3.1 (x64)"), "acme tool");
        assert_eq!(normaliser("Visual Studio 2022"), "visual studio");
        assert_eq!(normaliser("Acme-Tool_v1.2"), "acme tool");
        assert_eq!(normaliser("7-Zip 23.01 (x64 edition)"), "7 zip");
        assert!(egal("Acme Tool 1.0", "ACMETOOL"));
        assert!(!egal("", ""));
    }

    #[test]
    fn racines_nues() {
        for r in [
            "HKLM\\SOFTWARE",
            "HKCU\\Software",
            "HKLM\\SOFTWARE\\WOW6432Node",
            "HKLM\\SOFTWARE\\Classes",
            "HKCU\\Software\\Classes\\",
            "HKLM\\SYSTEM\\CurrentControlSet\\Services",
            "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
            "HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
            "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            "HKLM\\SOFTWARE\\Classes\\CLSID",
            "HKCR\\Installer\\Products",
        ] {
            assert!(racine_nue(r), "{r}");
        }
        assert!(!racine_nue("HKCU\\Software\\Acme"));
        assert!(!racine_nue("HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run::AcmeUpdater"));
        assert!(!racine_nue("HKLM\\SOFTWARE\\Classes\\CLSID\\{1234}"));
    }

    #[test]
    fn protections() {
        let g = Global { ed_installes: ["acme".to_string()].into_iter().collect(), noms_installes: HashSet::new() };
        let l = logiciel("Old Tool", "Acme", "fantome", "");
        let c = Ctx::new(&l);
        assert!(protege_cle("HKLM\\SOFTWARE", "software", &c, &g, false).is_some());
        assert!(protege_cle("HKLM\\SYSTEM\\CurrentControlSet\\Control\\Foo", "service", &c, &g, false).is_some());
        assert!(protege_cle("HKLM\\SYSTEM\\CurrentControlSet\\Services\\OldSvc", "service", &c, &g, false).is_none());
        assert!(protege_cle("HKLM\\SOFTWARE\\Acme", "software", &c, &g, true).is_some()); // éditeur encore installé
        assert!(protege_cle("HKLM\\SOFTWARE\\Acme\\Old Tool", "software", &c, &g, false).is_none());
        assert!(protege_cle("HKLM\\SOFTWARE\\Microsoft\\Foo", "software", &c, &g, false).is_some());
        let ms = logiciel("Thing", "Microsoft Corporation", "fantome", "");
        assert!(protege_cle("HKCU\\Software\\Thing", "software", &Ctx::new(&ms), &g, false).is_some());
        let vc = logiciel("Visual C++ 2015 Redistributable (x64)", "Autre", "fantome", "");
        assert!(protege_cle("HKCU\\Software\\Autre\\Vc", "software", &Ctx::new(&vc), &g, false).is_some());
        assert!(protection_chemin("HKLM\\SYSTEM\\Select").is_some());
    }

    #[test]
    fn refus_sans_confirmation() {
        assert_eq!(verifier_confirmation(false), Err("confirmation requise".to_string()));
        assert!(verifier_confirmation(true).is_ok());
    }

    #[test]
    fn cibles_et_install() {
        assert_eq!(extraire_exe("\"C:\\Program Files\\Acme\\a.exe\" /silent"), "C:\\Program Files\\Acme\\a.exe");
        assert_eq!(extraire_exe("C:\\Acme\\a.exe -k x"), "C:\\Acme\\a.exe");
        assert_eq!(extraire_exe("system32\\drivers\\x.sys"), "");
        let l = logiciel("Old Tool", "Acme", "fantome", "C:\\Program Files\\Acme\\Old Tool");
        let c = Ctx::new(&l);
        assert!(c.sous_install("c:\\program files\\acme\\old tool\\bin\\a.exe"));
        assert!(!c.sous_install("C:\\Program Files\\Acme\\Other\\a.exe"));
        // InstallLocation trop générique : ne prouve rien
        assert!(inst_normalise("C:\\Program Files").is_empty());
        assert!(inst_normalise("C:\\").is_empty());
    }

    #[test]
    fn point_de_restauration() {
        assert_eq!(interpreter_point(true, "", "D"), Ok("D".to_string()));
        assert_eq!(
            interpreter_point(true, "AVERTISSEMENT : ... au cours des dernières 1440 minutes", "D"),
            Ok("Un point de restauration récent existe déjà".to_string())
        );
        assert_eq!(interpreter_point(false, "Access is denied", "D"), Err("droits administrateur requis".to_string()));
    }

    #[test]
    fn horodatage_format() {
        let h = horodatage();
        assert_eq!(h.len(), 15);
        assert_eq!(&h[8..9], "_");
    }
}
