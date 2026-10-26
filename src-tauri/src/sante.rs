//! Santé du PC (lecture seule) : processus gourmands, classification « connu / à vérifier »,
//! état de la protection Windows. Aucune action sur les processus : on observe et on explique.
use serde::Serialize;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone)]
pub struct Processus {
    pub nom: String,
    pub chemin: String,
    pub instances: u32,
    pub cpu: f32,
    pub ram_octets: u64,
    pub ram_pct: f32,
    pub editeur: String,
    pub signe: bool,
    /// connu = reconnu et à sa place, verifier = chemin ou signature inhabituels, inconnu = non répertorié
    pub classe: &'static str,
    pub explication: String,
    pub raison: String,
}

#[derive(Serialize)]
pub struct ScanProcessus {
    pub seuil: f32,
    pub cpu_total: f32,
    pub ram_totale: u64,
    pub ram_utilisee: u64,
    pub processus: Vec<Processus>,
}

/// Nom d'exécutable (minuscules) -> (explication, dossier attendu en minuscules ou "" si libre).
fn connus() -> HashMap<&'static str, (&'static str, &'static str)> {
    let s32 = "c:\\windows\\system32";
    HashMap::from([
        ("system", ("Noyau Windows.", "")),
        ("registry", ("Registre Windows en mémoire.", "")),
        ("memory compression", ("Compression mémoire de Windows.", "")),
        ("system idle process", ("Temps CPU inutilisé (pas un programme).", "")),
        ("svchost.exe", ("Hôte de services Windows; plusieurs instances sont normales.", s32)),
        ("explorer.exe", ("Explorateur Windows : bureau, barre des tâches, dossiers.", "c:\\windows")),
        ("dwm.exe", ("Gestionnaire de fenêtres du bureau (affichage).", s32)),
        ("csrss.exe", ("Sous-système Windows essentiel.", s32)),
        ("wininit.exe", ("Initialisation de Windows au démarrage.", s32)),
        ("winlogon.exe", ("Ouverture de session Windows.", s32)),
        ("lsass.exe", ("Authentification et sécurité Windows.", s32)),
        ("services.exe", ("Gestionnaire des services Windows.", s32)),
        ("smss.exe", ("Gestionnaire de session Windows.", s32)),
        ("fontdrvhost.exe", ("Pilote de polices Windows.", s32)),
        ("sihost.exe", ("Hôte d'interface Windows (menu Démarrer, notifications).", s32)),
        ("taskhostw.exe", ("Hôte de tâches Windows.", s32)),
        ("ctfmon.exe", ("Saisie de texte et langues du clavier.", s32)),
        ("conhost.exe", ("Fenêtre de console Windows.", s32)),
        ("runtimebroker.exe", ("Gère les permissions des applications du Store.", s32)),
        ("searchindexer.exe", ("Indexation de la recherche Windows.", s32)),
        ("searchhost.exe", ("Recherche Windows.", "")),
        ("startmenuexperiencehost.exe", ("Menu Démarrer.", "")),
        ("shellexperiencehost.exe", ("Éléments d'interface Windows.", "")),
        ("textinputhost.exe", ("Clavier tactile et emoji.", "")),
        ("mpdefendercoreservice.exe", ("Service principal de Microsoft Defender.", "")),
        ("msmpeng.exe", ("Antivirus Microsoft Defender (analyse en temps réel).", "")),
        ("nissrv.exe", ("Protection réseau de Microsoft Defender.", "")),
        ("securityhealthservice.exe", ("Centre de sécurité Windows.", s32)),
        ("spoolsv.exe", ("Gestion des impressions.", s32)),
        ("audiodg.exe", ("Traitement audio Windows.", s32)),
        ("wmiprvse.exe", ("Infrastructure de gestion Windows (WMI).", "c:\\windows")),
        ("tiworker.exe", ("Installation des mises à jour Windows.", "c:\\windows")),
        ("trustedinstaller.exe", ("Service d'installation de Windows.", "c:\\windows")),
        ("onedrive.exe", ("Synchronisation OneDrive.", "")),
        ("msedge.exe", ("Navigateur Microsoft Edge (un processus par onglet/extension).", "")),
        ("msedgewebview2.exe", ("Composant web utilisé par des applications (WebView2).", "")),
        ("chrome.exe", ("Navigateur Google Chrome (un processus par onglet/extension).", "")),
        ("firefox.exe", ("Navigateur Firefox.", "")),
        ("brave.exe", ("Navigateur Brave.", "")),
        ("code.exe", ("Éditeur Visual Studio Code.", "")),
        ("claude.exe", ("Claude (application ou CLI).", "")),
        ("node.exe", ("Environnement JavaScript Node.js (outils de développement).", "")),
        ("python.exe", ("Interpréteur Python.", "")),
        ("powershell.exe", ("Terminal PowerShell.", s32)),
        ("pwsh.exe", ("Terminal PowerShell 7.", "")),
        ("cmd.exe", ("Invite de commandes.", s32)),
        ("windowsterminal.exe", ("Windows Terminal.", "")),
        ("teams.exe", ("Microsoft Teams.", "")),
        ("ms-teams.exe", ("Microsoft Teams.", "")),
        ("outlook.exe", ("Microsoft Outlook.", "")),
        ("excel.exe", ("Microsoft Excel.", "")),
        ("winword.exe", ("Microsoft Word.", "")),
        ("powerpnt.exe", ("Microsoft PowerPoint.", "")),
        ("revit.exe", ("Autodesk Revit.", "")),
        ("disklens.exe", ("Cette application.", "")),
        ("vmmem", ("Mémoire de la machine virtuelle WSL/Hyper-V.", "")),
        ("vmmemwsl", ("Mémoire de WSL.", "")),
    ])
}

fn minuscule(s: &str) -> String {
    s.to_lowercase()
}

/// Chemins où un exécutable inconnu est suspect : dossiers temporaires, téléchargements, Roaming.
fn chemin_suspect(chemin: &str) -> Option<&'static str> {
    let c = minuscule(chemin);
    if c.contains("\\appdata\\local\\temp\\") || c.starts_with("c:\\windows\\temp\\") {
        Some("lancé depuis un dossier temporaire")
    } else if c.contains("\\downloads\\") {
        Some("lancé depuis Téléchargements")
    } else if c.contains("\\appdata\\roaming\\") {
        Some("lancé depuis AppData\\Roaming")
    } else if c.starts_with("c:\\users\\public\\") || c.starts_with("c:\\programdata\\") {
        Some("lancé depuis un dossier partagé")
    } else {
        None
    }
}

/// Nom « aléatoire » : ≥ 8 caractères alphanumériques sans voyelles ou mélange chiffres/lettres dense.
fn nom_aleatoire(nom: &str) -> bool {
    let base = nom.trim_end_matches(".exe");
    if base.len() < 8 || !base.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let voyelles = base.chars().filter(|c| "aeiouy".contains(c.to_ascii_lowercase())).count();
    let chiffres = base.chars().filter(|c| c.is_ascii_digit()).count();
    voyelles == 0 || chiffres * 3 >= base.len()
}

/// Classe un exécutable. `signe` = signature Authenticode valide.
pub fn classer(nom: &str, chemin: &str, signe: bool, editeur: &str) -> (&'static str, String, String) {
    let n = minuscule(nom);
    let c = minuscule(chemin);
    if let Some((expl, dossier)) = connus().get(n.as_str()) {
        if !dossier.is_empty() && !chemin.is_empty() && !c.starts_with(dossier) {
            return ("verifier", expl.to_string(), format!("Le nom est celui d'un programme Windows mais il est dans {chemin} au lieu de {dossier}."));
        }
        return ("connu", expl.to_string(), String::new());
    }
    // Signature valide = éditeur vérifié : on ne le traite pas comme suspect, même hors table.
    if signe && !chemin.is_empty() {
        let expl = if editeur.is_empty() { "Programme signé numériquement.".to_string() } else { format!("Programme signé par {editeur}.") };
        return ("connu", expl, String::new());
    }
    let mut raisons: Vec<String> = Vec::new();
    if chemin.is_empty() {
        raisons.push("chemin illisible (droits insuffisants)".into());
    }
    if let Some(r) = chemin_suspect(chemin) {
        raisons.push(r.into());
    }
    if !chemin.is_empty() {
        raisons.push("non signé".into());
    }
    if nom_aleatoire(&n) {
        raisons.push("nom qui ressemble à du hasard".into());
    }
    if raisons.is_empty() {
        return ("inconnu", "Programme non répertorié.".into(), String::new());
    }
    ("verifier", "Programme non répertorié.".into(), raisons.join(", "))
}

// ---------- Signature et éditeur ----------

fn large(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Signature Authenticode intégrée valide (les exécutables Windows signés par catalogue renvoient false).
pub(crate) fn signature_valide(chemin: &str) -> bool {
    use windows_sys::core::GUID;
    use windows_sys::Win32::Security::WinTrust::*;
    let fichier = large(chemin);
    let mut info: WINTRUST_FILE_INFO = unsafe { std::mem::zeroed() };
    info.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    info.pcwszFilePath = fichier.as_ptr();
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.Anonymous.pFile = &mut info;
    let mut action = GUID { data1: 0x00AAC56B, data2: 0xCD44, data3: 0x11d0, data4: [0x8c, 0xc2, 0x00, 0xc0, 0x4f, 0xc2, 0x95, 0xee] };
    let rc = unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, &mut data as *mut _ as *mut _) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(std::ptr::null_mut(), &mut action, &mut data as *mut _ as *mut _) };
    rc == 0
}

/// Nom de l'éditeur (CompanyName des ressources de version).
pub(crate) fn editeur_fichier(chemin: &str) -> String {
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let p = large(chemin);
    unsafe {
        let taille = GetFileVersionInfoSizeW(p.as_ptr(), std::ptr::null_mut());
        if taille == 0 {
            return String::new();
        }
        let mut buf = vec![0u8; taille as usize];
        if GetFileVersionInfoW(p.as_ptr(), 0, taille, buf.as_mut_ptr() as *mut _) == 0 {
            return String::new();
        }
        let mut tr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut n = 0u32;
        if VerQueryValueW(buf.as_ptr() as *const _, large("\\VarFileInfo\\Translation").as_ptr(), &mut tr, &mut n) == 0 || n < 4 {
            return String::new();
        }
        let t = std::slice::from_raw_parts(tr as *const u16, 2);
        let sous = format!("\\StringFileInfo\\{:04x}{:04x}\\CompanyName", t[0], t[1]);
        let mut v: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(buf.as_ptr() as *const _, large(&sous).as_ptr(), &mut v, &mut len) == 0 || len == 0 {
            return String::new();
        }
        let s = std::slice::from_raw_parts(v as *const u16, len as usize);
        String::from_utf16_lossy(s).trim_end_matches('\0').trim().to_string()
    }
}

// ---------- Scan des processus ----------

fn scanner(seuil: f32) -> ScanProcessus {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::everything();
    sys.refresh_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    sys.refresh_cpu_usage();
    let coeurs = sys.cpus().len().max(1) as f32;
    let ram_totale = sys.total_memory().max(1);

    // Regroupement par exécutable
    let mut groupes: HashMap<String, (String, String, u32, f32, u64)> = HashMap::new();
    for p in sys.processes().values() {
        let nom = p.name().to_string_lossy().to_string();
        let chemin = p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        let cle = if chemin.is_empty() { minuscule(&nom) } else { minuscule(&chemin) };
        let g = groupes.entry(cle).or_insert((nom, chemin, 0, 0.0, 0));
        g.2 += 1;
        g.3 += p.cpu_usage() / coeurs;
        g.4 += p.memory();
    }
    let mut out: Vec<Processus> = Vec::new();
    for (_, (nom, chemin, instances, cpu, ram)) in groupes {
        let ram_pct = ram as f32 * 100.0 / ram_totale as f32;
        if cpu < seuil && ram_pct < seuil {
            continue;
        }
        let (signe, editeur) = if chemin.is_empty() { (false, String::new()) } else { (signature_valide(&chemin), editeur_fichier(&chemin)) };
        let (classe, explication, raison) = classer(&nom, &chemin, signe, &editeur);
        out.push(Processus { nom, chemin, instances, cpu, ram_octets: ram, ram_pct, editeur, signe, classe, explication, raison });
    }
    out.sort_by(|a, b| (b.cpu + b.ram_pct).total_cmp(&(a.cpu + a.ram_pct)));
    ScanProcessus {
        seuil,
        cpu_total: sys.global_cpu_usage(),
        ram_totale,
        ram_utilisee: sys.used_memory(),
        processus: out,
    }
}

#[tauri::command]
pub async fn sante_processus(seuil: f32) -> Result<ScanProcessus, String> {
    let seuil = seuil.clamp(0.5, 50.0);
    tauri::async_runtime::spawn_blocking(move || scanner(seuil)).await.map_err(|e| e.to_string())
}

// ---------- Protection Windows (Defender, pare-feu) ----------

#[derive(Serialize, Clone)]
pub struct Protection {
    pub id: &'static str,
    pub titre: String,
    pub statut: &'static str,
    pub detail: String,
    pub reglage: Option<&'static str>,
}

fn powershell(script: &str) -> String {
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn defender() -> Protection {
    let mut p = Protection { id: "defender", titre: "Microsoft Defender".into(), statut: "gris", detail: "État illisible.".into(), reglage: Some("windowsdefender:") };
    let s = powershell("try{$m=Get-MpComputerStatus -ErrorAction Stop;\"$($m.AntivirusEnabled)|$($m.RealTimeProtectionEnabled)|$($m.AntivirusSignatureAge)\"}catch{'ERR'}");
    let v: Vec<&str> = s.split('|').collect();
    if v.len() == 3 {
        let (av, rt, age) = (v[0] == "True", v[1] == "True", v[2].parse::<u32>().unwrap_or(999));
        p.statut = if av && rt && age <= 7 { "vert" } else if av { "orange" } else { "rouge" };
        p.detail = format!("Antivirus {}, protection en temps réel {}, définitions vieilles de {age} j.",
            if av { "actif" } else { "inactif" }, if rt { "active" } else { "désactivée" });
    } else {
        p.detail = "Defender illisible : un autre antivirus est peut-être actif.".into();
    }
    p
}

fn pare_feu() -> Protection {
    let mut p = Protection { id: "pare-feu", titre: "Pare-feu Windows".into(), statut: "gris", detail: "État illisible.".into(), reglage: Some("windowsdefender:") };
    let s = powershell("try{(Get-NetFirewallProfile -ErrorAction Stop | ForEach-Object {\"$($_.Name)=$($_.Enabled)\"}) -join ','}catch{'ERR'}");
    if s.contains('=') {
        let off: Vec<&str> = s.split(',').filter(|x| x.ends_with("=False")).map(|x| x.split('=').next().unwrap_or("")).collect();
        p.statut = if off.is_empty() { "vert" } else { "rouge" };
        p.detail = if off.is_empty() { "Activé sur tous les profils réseau.".into() } else { format!("Désactivé sur : {}.", off.join(", ")) };
    }
    p
}

fn mises_a_jour() -> Protection {
    let mut p = Protection { id: "wu", titre: "Mises à jour Windows".into(), statut: "gris", detail: "Date de dernière installation illisible.".into(), reglage: Some("ms-settings:windowsupdate") };
    let s = powershell("try{(Get-HotFix | Sort-Object InstalledOn -Descending | Select-Object -First 1).InstalledOn.ToString('yyyy-MM-dd')}catch{'ERR'}");
    if s.len() == 10 && s.as_bytes()[4] == b'-' {
        let (y, m, d): (i64, i64, i64) = (s[0..4].parse().unwrap_or(0), s[5..7].parse().unwrap_or(0), s[8..10].parse().unwrap_or(0));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) / 86_400;
        let j = now - crate::sauvegarde::jours_civils(y, m, d);
        p.statut = if j <= 45 { "vert" } else if j <= 90 { "orange" } else { "rouge" };
        p.detail = format!("Dernier correctif installé il y a {j} j ({s}).");
    }
    p
}

/// Ligne « nom|type|santé|température|usure|heures|erreurs » -> état. Champs de compteurs vides si non lisibles.
pub fn analyser_disque(ligne: &str) -> Option<Protection> {
    let v: Vec<&str> = ligne.split('|').collect();
    if v.len() < 7 || v[0].trim().is_empty() {
        return None;
    }
    let num = |s: &str| s.trim().parse::<u32>().ok();
    let (temp, usure, heures, erreurs) = (num(v[3]), num(v[4]), num(v[5]), num(v[6]));
    let mut notes: Vec<String> = Vec::new();
    let mut statut = match v[2].trim() {
        "Healthy" => "vert",
        "Warning" => "orange",
        "Unhealthy" => "rouge",
        _ => "gris",
    };
    if let Some(t) = temp.filter(|&t| t > 0) {
        notes.push(format!("{t} °C"));
        if t >= 60 && statut == "vert" { statut = "orange"; }
    }
    if let Some(u) = usure {
        notes.push(format!("usure {u} %"));
        if u >= 80 && statut == "vert" { statut = "orange"; }
    }
    if let Some(h) = heures { notes.push(format!("{h} h d'utilisation")); }
    if let Some(e) = erreurs.filter(|&e| e > 0) {
        notes.push(format!("{e} erreurs de lecture non corrigées"));
        if statut == "vert" { statut = "orange"; }
    }
    let mut detail = format!("Santé Windows : {}.", v[2].trim());
    if notes.is_empty() {
        detail.push_str(" Compteurs détaillés illisibles (droits administrateur requis).");
    } else {
        detail.push(' ');
        detail.push_str(&notes.join(" · "));
        detail.push('.');
    }
    Some(Protection { id: "disque", titre: format!("Disque {} ({})", v[0].trim(), v[1].trim()), statut, detail, reglage: None })
}

fn disques() -> Vec<Protection> {
    let s = powershell("try{Get-PhysicalDisk -ErrorAction Stop | ForEach-Object { $r = $_ | Get-StorageReliabilityCounter -ErrorAction SilentlyContinue; \"$($_.FriendlyName)|$($_.MediaType)|$($_.HealthStatus)|$($r.Temperature)|$($r.Wear)|$($r.PowerOnHours)|$($r.ReadErrorsUncorrected)\" }}catch{}");
    s.lines().filter_map(analyser_disque).collect()
}

#[tauri::command]
pub async fn sante_protection() -> Result<Vec<Protection>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut v = vec![defender(), pare_feu(), mises_a_jour()];
        v.extend(disques());
        v
    })
    .await
    .map_err(|e| e.to_string())
}

// ---------- Sécurité : connexions et extensions (aide au repérage, pas un antivirus) ----------

#[derive(Serialize, Clone)]
pub struct Connexion {
    pub processus: String,
    pub chemin: String,
    pub pid: u32,
    pub distant: String,
    pub port: u16,
    pub classe: &'static str,
    pub raison: String,
}

#[derive(Serialize, Clone)]
pub struct Extension {
    pub navigateur: &'static str,
    pub profil: String,
    pub id: String,
    pub nom: String,
    pub hors_store: bool,
    pub permissions_risque: Vec<String>,
    pub raison: String,
}

#[derive(Serialize)]
pub struct ScanSecurite {
    pub connexions: Vec<Connexion>,
    pub extensions: Vec<Extension>,
    pub limites: Vec<String>,
}

fn ipv4(a: u32) -> String {
    let b = a.to_le_bytes();
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

fn adresse_locale(a: u32) -> bool {
    let b = a.to_le_bytes();
    b[0] == 127 || b[0] == 10 || (b[0] == 192 && b[1] == 168) || (b[0] == 172 && (16..32).contains(&b[1])) || (b[0] == 169 && b[1] == 254) || a == 0
}

fn connexions_suspectes() -> Vec<Connexion> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL};
    let mut taille = 0u32;
    unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut taille, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0) };
    if taille == 0 {
        return vec![];
    }
    let mut buf = vec![0u8; taille as usize];
    if unsafe { GetExtendedTcpTable(buf.as_mut_ptr() as *mut _, &mut taille, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0) } != 0 {
        return vec![];
    }
    let n = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    let lignes = unsafe { std::slice::from_raw_parts(buf.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID, n) };

    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::everything());
    let mut cache: HashMap<String, (bool, String)> = HashMap::new();
    let mut out = Vec::new();
    let mut vus: std::collections::HashSet<(String, u32, u16)> = std::collections::HashSet::new();
    for l in lignes {
        if l.dwState != 5 || adresse_locale(l.dwRemoteAddr) {
            continue;
        }
        let Some(p) = sys.process(sysinfo::Pid::from_u32(l.dwOwningPid)) else { continue };
        let nom = p.name().to_string_lossy().to_string();
        let chemin = p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        let (signe, ed) = cache
            .entry(chemin.clone())
            .or_insert_with(|| if chemin.is_empty() { (false, String::new()) } else { (signature_valide(&chemin), editeur_fichier(&chemin)) })
            .clone();
        let (classe, _, raison) = classer(&nom, &chemin, signe, &ed);
        let port = u16::from_be((l.dwRemotePort & 0xFFFF) as u16);
        if classe == "connu" || !vus.insert((chemin.to_lowercase(), l.dwRemoteAddr, port)) {
            continue;
        }
        out.push(Connexion {
            processus: nom,
            chemin,
            pid: l.dwOwningPid,
            distant: ipv4(l.dwRemoteAddr),
            port,
            classe,
            raison,
        });
    }
    out.sort_by(|a, b| a.processus.cmp(&b.processus));
    out
}

const PERMISSIONS_RISQUE: [&str; 9] = ["<all_urls>", "*://*/*", "webrequest", "webrequestblocking", "cookies", "history", "nativemessaging", "debugger", "proxy"];

/// Analyse le JSON « Secure Preferences » d'un profil Chromium.
pub fn analyser_extensions(json: &str, navigateur: &'static str, profil: &str) -> Vec<Extension> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    let Some(map) = v.pointer("/extensions/settings").and_then(|x| x.as_object()) else { return vec![] };
    let mut out = Vec::new();
    for (id, e) in map {
        let loc = e.get("location").and_then(|x| x.as_i64()).unwrap_or(0);
        let Some(m) = e.get("manifest") else { continue };
        if loc == 5 || loc == 10 {
            continue; // composants intégrés au navigateur
        }
        let nom = m.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let nom = if nom.is_empty() || nom.starts_with("__MSG_") { format!("(nom localisé) {id}") } else { nom };
        let hors_store = !e.get("from_webstore").and_then(|x| x.as_bool()).unwrap_or(false);
        let mut perms: Vec<String> = Vec::new();
        for cle in ["permissions", "host_permissions", "optional_permissions"] {
            if let Some(a) = m.get(cle).and_then(|x| x.as_array()) {
                for p in a.iter().filter_map(|x| x.as_str()) {
                    if PERMISSIONS_RISQUE.contains(&p.to_lowercase().as_str()) && !perms.iter().any(|q| q == p) {
                        perms.push(p.to_string());
                    }
                }
            }
        }
        let mut raisons: Vec<String> = Vec::new();
        if loc == 4 {
            raisons.push("chargée manuellement (mode développeur)".into());
        } else if hors_store {
            raisons.push("installée hors du magasin officiel".into());
        }
        if perms.len() >= 3 {
            raisons.push("demande de nombreux accès sensibles".into());
        }
        if raisons.is_empty() {
            continue; // seules les extensions avec une raison de vérifier sont listées
        }
        out.push(Extension { navigateur, profil: profil.to_string(), id: id.clone(), nom, hors_store, permissions_risque: perms, raison: raisons.join(", ") });
    }
    out
}

fn extensions_navigateurs() -> Vec<Extension> {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let mut out = Vec::new();
    for (nav, rel) in [("Chrome", "Google\\Chrome\\User Data"), ("Edge", "Microsoft\\Edge\\User Data"), ("Brave", "BraveSoftware\\Brave-Browser\\User Data")] {
        let base = std::path::PathBuf::from(&local).join(rel);
        let Ok(rd) = std::fs::read_dir(&base) else { continue };
        for d in rd.filter_map(|x| x.ok()) {
            let nom = d.file_name().to_string_lossy().to_string();
            if nom != "Default" && !nom.starts_with("Profile ") {
                continue;
            }
            let mut json = String::new();
            for f in ["Secure Preferences", "Preferences"] {
                if let Ok(t) = std::fs::read_to_string(d.path().join(f)) {
                    if t.contains("\"settings\"") {
                        json = t;
                        break;
                    }
                }
            }
            out.extend(analyser_extensions(&json, nav, &nom));
        }
    }
    out
}

#[tauri::command]
pub async fn sante_securite() -> Result<ScanSecurite, String> {
    tauri::async_runtime::spawn_blocking(|| ScanSecurite {
        connexions: connexions_suspectes(),
        extensions: extensions_navigateurs(),
        limites: vec![
            "Aide au repérage, pas un antivirus : « à vérifier » ne veut pas dire « malveillant ».".into(),
            "Connexions IPv4 uniquement ; certains processus système ne sont visibles qu'en administrateur.".into(),
            "Firefox n'est pas analysé.".into(),
        ],
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_hors_store() {
        let j = r#"{"extensions":{"settings":{
            "aaa":{"location":1,"from_webstore":true,"manifest":{"name":"Propre","permissions":["storage"]}},
            "bbb":{"location":4,"from_webstore":false,"manifest":{"name":"Maison","permissions":["cookies","history","webRequest"]}},
            "ccc":{"location":5,"manifest":{"name":"Interne"}}}}}"#;
        let e = analyser_extensions(j, "Chrome", "Default");
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, "bbb");
        assert!(e[0].raison.contains("développeur") && e[0].raison.contains("nombreux"));
    }

    /// Appels Windows réels : `cargo test fumee -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fumee() {
        let r = scanner(1.0);
        for p in r.processus.iter().take(8) {
            println!("{:<10} {:<28} cpu {:>5.1} ram {:>4.1}% signe={} ed={:?} {}", p.classe, p.nom, p.cpu, p.ram_pct, p.signe, p.editeur, p.raison);
        }
        let c = connexions_suspectes();
        println!("connexions à vérifier: {}", c.len());
        for x in c.iter().take(5) { println!("  {} -> {}:{} [{}]", x.processus, x.distant, x.port, x.classe); }
        println!("extensions à examiner: {}", extensions_navigateurs().len());
        for p in [defender(), pare_feu(), mises_a_jour()] { println!("{} [{}] {}", p.titre, p.statut, p.detail); }
    }

    #[test]
    fn disque_sain_et_use() {
        let a = analyser_disque("Samsung SSD|SSD|Healthy|38|12|4000|0").unwrap();
        assert_eq!(a.statut, "vert");
        let b = analyser_disque("Vieux|SSD|Healthy|40|93|9000|0").unwrap();
        assert_eq!(b.statut, "orange");
        let c = analyser_disque("Mort|HDD|Unhealthy|||| ").unwrap();
        assert_eq!(c.statut, "rouge");
        assert!(analyser_disque("pas|assez").is_none());
    }

    #[test]
    fn adresses_locales() {
        assert!(adresse_locale(u32::from_le_bytes([192, 168, 1, 5])));
        assert!(!adresse_locale(u32::from_le_bytes([8, 8, 8, 8])));
    }

    #[test]
    fn connu_a_sa_place() {
        assert_eq!(classer("svchost.exe", "C:\\Windows\\System32\\svchost.exe", false, "").0, "connu");
    }

    #[test]
    fn nom_windows_au_mauvais_endroit() {
        let (c, _, r) = classer("svchost.exe", "C:\\Users\\a\\AppData\\Local\\Temp\\svchost.exe", false, "");
        assert_eq!(c, "verifier");
        assert!(r.contains("au lieu de"));
    }

    #[test]
    fn inconnu_temp_non_signe() {
        let (c, _, r) = classer("xk3j9q2m.exe", "C:\\Users\\a\\AppData\\Local\\Temp\\xk3j9q2m.exe", false, "");
        assert_eq!(c, "verifier");
        assert!(r.contains("temporaire") && r.contains("non signé") && r.contains("hasard"));
    }

    #[test]
    fn signe_hors_table_est_connu() {
        assert_eq!(classer("zoom.exe", "C:\\Program Files\\Zoom\\zoom.exe", true, "Zoom Video").0, "connu");
    }

    #[test]
    fn aleatoire() {
        assert!(nom_aleatoire("bcdfghjk.exe"));
        assert!(!nom_aleatoire("notepad.exe"));
    }
}
