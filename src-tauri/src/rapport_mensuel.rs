//! Rapports locaux déterministes : aucune IA, aucune action de nettoyage.
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeSet, HashMap}, fs, path::{Path, PathBuf}, process::Command, time::{SystemTime, UNIX_EPOCH}};
use std::os::windows::process::CommandExt;

const TACHE: &str = "DiskLens-RapportMensuel";
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Deserialize)]
struct ConfigMensuel {
    active: bool,
    seuil_libre_pct: u8,
}

impl Default for ConfigMensuel {
    fn default() -> Self { Self { active: false, seuil_libre_pct: 10 } }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ResumeMensuel {
    pub id: String,
    pub ts: u64,
    pub markdown: String,
    pub html: String,
    pub alertes: Vec<String>,
}

#[derive(Serialize)]
pub struct EtatMensuel {
    pub active: bool,
    pub derniere_execution: Option<u64>,
    pub rapports: Vec<ResumeMensuel>,
    pub dossier: String,
    pub seuil_libre_pct: u8,
    pub planification: String,
}

#[derive(Serialize, Deserialize)]
struct RapportEnregistre {
    instantane: InstantaneMensuel,
    rapport: ResumeMensuel,
}

fn maintenant() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }

fn date_utc(ts: u64) -> String {
    if ts > 253_402_300_799 { return "date inconnue".into(); }
    let z = (ts / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut an = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let jour = doy - (153 * mp + 2) / 5 + 1;
    let mois = mp + if mp < 10 { 3 } else { -9 };
    if mois <= 2 { an += 1; }
    format!("{an:04}-{mois:02}-{jour:02} {:02}:{:02}:{:02} UTC", ts / 3600 % 24, ts / 60 % 60, ts % 60)
}

fn config(dir: &Path) -> Result<ConfigMensuel, String> {
    match fs::read(dir.join("rapport_mensuel.json")) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("Configuration mensuelle illisible : {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigMensuel::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn ecrire(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Dossier de destination absent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = path.with_extension(format!("{suffix}.tmp"));
    use std::io::Write;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

fn charger_rapports(dir: &Path) -> Result<Vec<RapportEnregistre>, String> {
    let entries = match fs::read_dir(dir.join("rapports_mensuels")) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut files: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json") && p.file_stem().and_then(|x| x.to_str()).map_or(false, id_valide)).collect();
    files.sort_by(|a,b| b.file_stem().cmp(&a.file_stem()));
    let mut rapports = Vec::new();
    for path in files.into_iter().take(24) {
        let bytes = fs::read(&path).map_err(|e| format!("Rapport {} illisible : {e}", path.display()))?;
        let r: RapportEnregistre = serde_json::from_slice(&bytes).map_err(|e| format!("Rapport {} invalide : {e}", path.display()))?;
        if path.file_stem().and_then(|x| x.to_str()) != Some(r.rapport.id.as_str()) { return Err("Identifiant de rapport incohérent".into()); }
        rapports.push(r);
    }
    rapports.sort_by(|a,b| b.instantane.ts.cmp(&a.instantane.ts).then_with(|| b.rapport.id.cmp(&a.rapport.id)));
    Ok(rapports)
}

pub fn etat(dir: &Path) -> Result<EtatMensuel, String> {
    let cfg = config(dir)?;
    let rapports: Vec<_> = charger_rapports(dir)?.into_iter().map(|r| r.rapport).collect();
    let presente = if cfg.active { Some(tache_active()?) } else { None };
    Ok(EtatMensuel {
        active: cfg.active && presente == Some(true),
        derniere_execution: rapports.first().map(|r| r.ts),
        rapports,
        dossier: dir.join("rapports_mensuels").to_string_lossy().into_owned(),
        seuil_libre_pct: cfg.seuil_libre_pct,
        planification: if presente == Some(true) { "Le 1er du mois à 09:00, dans votre session Windows." } else if cfg.active { "La tâche Windows est absente, désactivée ou inaccessible. Réactivez la planification." } else { "Planification désactivée." }.into(),
    })
}

fn xml_tache_active(xml: &str) -> bool {
    let xml = xml.replace('\0', "").to_lowercase();
    let Some(settings) = xml.split("<settings>").nth(1).and_then(|s| s.split("</settings>").next()) else { return false };
    settings.contains("<enabled>true</enabled>")
}

fn tache_active() -> Result<bool, String> {
    let output = Command::new("schtasks.exe").args(["/Query", "/TN", TACHE, "/XML"]).creation_flags(NO_WINDOW).output().map_err(|e| e.to_string())?;
    Ok(output.status.success() && xml_tache_active(&String::from_utf8_lossy(&output.stdout)))
}

fn commande_tache(exe: &Path, dir: &Path) -> Result<String, String> {
    let (exe, dir) = (exe.to_string_lossy(), dir.to_string_lossy());
    if exe.contains(['"', '\r', '\n']) || dir.contains(['"', '\r', '\n']) { return Err("Chemin de planification invalide".into()); }
    Ok(format!("\"{exe}\" --rapport-mensuel --data-dir \"{dir}\""))
}

pub fn planifier(dir: &Path, active: bool, seuil: u8) -> Result<EtatMensuel, String> {
    if !(1..=50).contains(&seuil) { return Err("Le seuil d'espace libre doit être compris entre 1 et 50 %.".into()); }
    let cfg = config(dir)?;
    let mut cmd = Command::new("schtasks.exe");
    if active {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let ligne = commande_tache(&exe, dir)?;
        cmd.args(["/Create", "/TN", TACHE, "/TR", &ligne, "/SC", "MONTHLY", "/D", "1", "/ST", "09:00", "/RL", "LIMITED", "/IT", "/F"]);
    } else if cfg.active || Command::new("schtasks.exe").args(["/Query", "/TN", TACHE]).creation_flags(NO_WINDOW).output().map(|o| o.status.success()).unwrap_or(false) {
        cmd.args(["/Delete", "/TN", TACHE, "/F"]);
    } else {
        ecrire(&dir.join("rapport_mensuel.json"), &serde_json::to_vec_pretty(&ConfigMensuel { active, seuil_libre_pct: seuil }).map_err(|e| e.to_string())?)?;
        return etat(dir);
    }
    let out = cmd.creation_flags(NO_WINDOW).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(format!("Planification Windows refusée : {} {}", String::from_utf8_lossy(&out.stdout).trim(), String::from_utf8_lossy(&out.stderr).trim())); }
    ecrire(&dir.join("rapport_mensuel.json"), &serde_json::to_vec_pretty(&ConfigMensuel { active, seuil_libre_pct: seuil }).map_err(|e| e.to_string())?)?;
    etat(dir)
}

pub fn ouvrir(dir: &Path, id: &str, format: &str) -> Result<(), String> {
    if !id_valide(id) || !["md", "html", "json"].contains(&format) { return Err("Export demandé invalide".into()); }
    let path = dir.join("rapports_mensuels").join(format!("{id}.{format}"));
    if !path.is_file() { return Err("Export introuvable".into()); }
    Command::new("explorer.exe").arg(path).creation_flags(NO_WINDOW).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

async fn collecter(dir: &Path) -> InstantaneMensuel {
    let mut s = InstantaneMensuel { ts: maintenant(), ..Default::default() };
    let disks = sysinfo::Disks::new_with_refreshed_list();
    for d in disks.iter().filter(|d| d.total_space() > 0) {
        let key = d.mount_point().to_string_lossy().trim_end_matches('\\').to_uppercase();
        let (scan_ts, mut top, fichiers) = match fs::read(dir.join(format!("snapshots_{}.json", key.replace(':', "")))) {
            Ok(bytes) => match serde_json::from_slice::<Vec<crate::Snapshot>>(&bytes) {
                Ok(snaps) => snaps.last().map(|last| (Some(last.ts), last.dirs.clone(), last.files.clone())).unwrap_or_default(),
                Err(_) => { s.limites.push(format!("{key} : instantané de stockage illisible.")); (None, vec![], vec![]) }
            },
            Err(e) => { if e.kind() != std::io::ErrorKind::NotFound { s.limites.push(format!("{key} : instantané inaccessible ({e}).")); } (None, vec![], vec![]) }
        };
        top.truncate(20);
        s.disques.push(DisqueMensuel { key, total: d.total_space(), free: d.available_space(), scan_ts, top, fichiers });
    }
    s.disques.sort_by(|a,b| a.key.cmp(&b.key));
    match crate::sante::sante_processus(5.0).await {
        Ok(p) => s.processus = p.processus.into_iter().map(|p| ProcessusMensuel { nom: p.nom, chemin: p.chemin, cpu: p.cpu, ram_pct: p.ram_pct, classe: p.classe.into() }).collect(),
        Err(e) => s.limites.push(format!("Processus non mesurés : {e}")),
    }
    match crate::sante::sante_securite().await {
        Ok(securite) => {
            s.limites.extend(securite.limites);
            s.intrus.extend(securite.extensions.into_iter().map(|e| format!("Extension {} / {} / {} : {}", e.navigateur, e.profil, e.id, e.raison)));
            s.intrus.extend(securite.connexions.into_iter().map(|c| format!("Connexion {} ({}) vers {}:{} : {}", c.processus, c.chemin, c.distant, c.port, c.raison)));
        },
        Err(e) => s.limites.push(format!("Repérage des intrus indisponible : {e}")),
    }
    s.intrus.extend(s.processus.iter().filter(|p| p.classe != "connu").map(|p| format!("Processus {} ({}) : {}", p.nom, p.chemin, p.classe)));
    s.intrus.sort(); s.intrus.dedup();
    s
}

fn rendre(s: &InstantaneMensuel, ancien: Option<&InstantaneMensuel>, id: String, seuil: u8) -> ResumeMensuel {
    let alertes = alertes(s, seuil);
    let mut md = format!("# DiskLens — rapport local\n\nDate : {}.\n\n## Alertes\n", date_utc(s.ts));
    for a in &alertes { md.push_str(&format!("- {a}\n")); }
    if alertes.is_empty() { md.push_str("Aucune alerte d'espace selon le seuil configuré.\n"); }
    md.push_str("\n## Ce qui a changé\n");
    for c in comparer(ancien, s) { md.push_str(&format!("- {c}\n")); }
    md.push_str("\n## Stockage\nTailles logiques issues du dernier scan ; dossiers parents et enfants se recouvrent. Aucune somme de gain récupérable.\n");
    for d in &s.disques {
        md.push_str(&format!("\n### {}\nLibre : {} / {} octets. Dernier scan : {}.\n", d.key, d.free, d.total, d.scan_ts.map(date_utc).unwrap_or_else(|| "aucun".into())));
        for (p, taille) in &d.top { md.push_str(&format!("- {p} : {taille} octets\n")); }
        md.push_str("\nPlus gros fichiers conservés dans le dernier scan :\n");
        for (p, taille) in &d.fichiers { md.push_str(&format!("- {p} : {taille} octets\n")); }
        if d.fichiers.is_empty() { md.push_str("Aucun fichier individuel mémorisé (ancien instantané ou aucun fichier volumineux).\n"); }
    }
    md.push_str("\n## Processus : CPU ou RAM au-dessus de 5 %\nMesure ponctuelle, pas une consommation sur le mois.\n");
    for p in &s.processus { md.push_str(&format!("- {} ({}) : CPU {:.1} %, RAM {:.1} % — {}\n", p.nom, p.chemin, p.cpu, p.ram_pct, p.classe)); }
    md.push_str("\n## Éléments à vérifier\nInconnu ne signifie pas malveillant.\n");
    for i in &s.intrus { md.push_str(&format!("- {i}\n")); }
    if s.intrus.is_empty() { md.push_str("Aucun signalement dans le périmètre analysé.\n"); }
    md.push_str("\n## Limites de l'analyse\n");
    for l in &s.limites { md.push_str(&format!("- {l}\n")); }
    md.push_str("\nRapport local sans IA. Aucun fichier supprimé, aucun processus arrêté, aucun réglage de sécurité modifié.\n");
    let html = format!("<!doctype html><html lang=\"fr\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\"><title>DiskLens — rapport local</title><style>body{{font:16px system-ui;max-width:1100px;margin:40px auto;padding:0 24px;color:#182033}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;line-height:1.6}}</style><body><pre>{}</pre></body></html>", echapper_html(&md));
    ResumeMensuel { id, ts: s.ts, markdown: md, html, alertes }
}

pub async fn generer(dir: &Path) -> Result<ResumeMensuel, String> {
    // Ne pas écraser la référence si l'historique ne peut pas être lu.
    let anciens = charger_rapports(dir)?;
    let cfg = config(dir)?;
    let s = collecter(dir).await;
    let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos().to_string();
    enregistrer(dir, s, anciens.first().map(|r| &r.instantane), id, cfg.seuil_libre_pct)
}

fn enregistrer(dir: &Path, s: InstantaneMensuel, ancien: Option<&InstantaneMensuel>, id: String, seuil: u8) -> Result<ResumeMensuel, String> {
    let rapport = rendre(&s, ancien, id.clone(), seuil);
    let base = dir.join("rapports_mensuels");
    ecrire(&base.join(format!("{id}.md")), rapport.markdown.as_bytes())?;
    ecrire(&base.join(format!("{id}.html")), rapport.html.as_bytes())?;
    let record = RapportEnregistre { instantane: s, rapport: rapport.clone() };
    // JSON écrit en dernier : un rapport incomplet n'entre jamais dans l'historique.
    ecrire(&base.join(format!("{id}.json")), &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?)?;
    Ok(rapport)
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct DisqueMensuel {
    pub key: String,
    pub total: u64,
    pub free: u64,
    pub scan_ts: Option<u64>,
    pub top: Vec<(String, u64)>,
    #[serde(default)]
    pub fichiers: Vec<(String, u64)>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ProcessusMensuel {
    pub nom: String,
    pub chemin: String,
    pub cpu: f32,
    pub ram_pct: f32,
    pub classe: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct InstantaneMensuel {
    pub ts: u64,
    pub disques: Vec<DisqueMensuel>,
    pub processus: Vec<ProcessusMensuel>,
    pub intrus: Vec<String>,
    pub limites: Vec<String>,
}

fn comparer(avant: Option<&InstantaneMensuel>, apres: &InstantaneMensuel) -> Vec<String> {
    let Some(avant) = avant else { return vec!["Premier rapport : aucune tendance ne peut encore être calculée.".into()] };
    let mut out = Vec::new();
    for d in &apres.disques {
        if let Some(a) = avant.disques.iter().find(|a| a.key == d.key) {
            let delta = d.free as i128 - a.free as i128;
            out.push(format!("{} : variation d'espace libre {delta:+} octets.", d.key));
            // Pas de fausse croissance quand les deux rapports réutilisent le même scan.
            if d.scan_ts != a.scan_ts || d.top != a.top {
                let vieux: HashMap<_,_> = a.top.iter().map(|(p,s)| (p.to_lowercase(), *s)).collect();
                for (p,s) in &d.top {
                    if let Some(t) = vieux.get(&p.to_lowercase()) {
                        let delta = *s as i128 - *t as i128;
                        if delta != 0 { out.push(format!("{p} : {delta:+} octets depuis le précédent scan.")); }
                    } else { out.push(format!("{p} : nouvel élément du top 20 ({s} octets), pas nécessairement un nouveau dossier.")); }
                }
                for (p,_) in &a.top { if !d.top.iter().any(|(q,_)| q.eq_ignore_ascii_case(p)) { out.push(format!("{p} : absent du top 20 actuel ; cela ne prouve pas une suppression.")); } }
            }
        } else { out.push(format!("{} : disque nouvellement observé.", d.key)); }
    }
    for d in &avant.disques { if !apres.disques.iter().any(|a| a.key == d.key) { out.push(format!("{} : disque non observé lors de ce rapport.", d.key)); } }
    let a: BTreeSet<_> = avant.intrus.iter().collect();
    let b: BTreeSet<_> = apres.intrus.iter().collect();
    for i in b.difference(&a) { out.push(format!("Nouveau signalement : {i}")); }
    for i in a.difference(&b) { out.push(format!("Signalement non revu (mesure ponctuelle) : {i}")); }
    let cle = |p: &ProcessusMensuel| format!("{}|{}", p.chemin.to_lowercase(), p.nom.to_lowercase());
    let precedents: BTreeSet<_> = avant.processus.iter().map(cle).collect();
    let courants: BTreeSet<_> = apres.processus.iter().map(cle).collect();
    for p in &apres.processus { if !precedents.contains(&cle(p)) { out.push(format!("{} : nouvellement au-dessus du seuil de CPU ou RAM (mesure ponctuelle, pas nécessairement une nouvelle installation).", p.nom)); } }
    for p in &avant.processus { if !courants.contains(&cle(p)) { out.push(format!("{} : non revu au-dessus du seuil de CPU ou RAM.", p.nom)); } }
    out
}

fn echapper_html(text: &str) -> String { text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;") }

fn id_valide(id: &str) -> bool { (10..=30).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()) }

fn alertes(s: &InstantaneMensuel, seuil: u8) -> Vec<String> {
    let mut out = Vec::new();
    for d in &s.disques {
        if d.total > 0 {
            let libre = d.free as f64 / d.total as f64 * 100.;
            if libre < seuil as f64 { out.push(format!("{} : espace libre faible ({libre:.1} %, seuil {seuil} %).", d.key)); }
        }
        match d.scan_ts {
            None => out.push(format!("{} : aucun scan de dossiers disponible ; analyser ce disque dans l'Explorateur.", d.key)),
            Some(t) if s.ts.saturating_sub(t) > 30 * 86_400 => out.push(format!("{} : scan de dossiers ancien (plus de 30 jours).", d.key)),
            _ => (),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instantane() -> InstantaneMensuel {
        InstantaneMensuel { ts: 100, disques: vec![DisqueMensuel { key: "C:".into(), total: 1000, free: 50, scan_ts: Some(1), top: vec![("C:\\Docs".into(), 400)], fichiers: vec![] }], ..Default::default() }
    }
    #[test]
    fn premier_rapport_ne_fabrique_pas_de_tendance() {
        let lignes = comparer(None, &instantane()).join(" ");
        assert!(lignes.contains("Premier rapport"));
    }
    #[test]
    fn comparaison_signale_ajout_disparition_et_croissance() {
        let avant = instantane();
        let mut apres = instantane();
        apres.disques[0].free = 30;
        apres.disques[0].top[0].1 = 450;
        apres.intrus.push("extension inconnue".into());
        let texte = comparer(Some(&avant), &apres).join(" ");
        assert!(texte.contains("C:\\Docs") && texte.contains("+50"));
        assert!(texte.contains("extension inconnue"));
        apres.disques.clear();
        assert!(comparer(Some(&avant), &apres).join(" ").contains("non observé"));
    }
    #[test]
    fn alertes_espace_et_scan_ancien() {
        let mut s = instantane();
        s.ts = 90 * 86_400;
        let a = alertes(&s, 10).join(" ");
        assert!(a.contains("5.0 %") && a.contains("ancien"));
    }
    #[test]
    fn html_et_identifiants_non_fiables() {
        assert_eq!(echapper_html("<script>\"&'"), "&lt;script&gt;&quot;&amp;&#39;");
        assert!(id_valide("1793000000000000000"));
        for id in ["", "../x", "C:\\Windows", "1.json", "<x>"] { assert!(!id_valide(id)); }
    }
    #[test]
    fn exports_reouvrables_et_config_atomique() {
        let dossier = std::env::temp_dir().join(format!("disklens-mensuel-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        assert!(!dossier.exists());
        let r = enregistrer(&dossier, instantane(), None, "1793000000000000000".into(), 10).unwrap();
        assert!(r.html.contains("&lt;") || r.html.contains("<!doctype html>"));
        let historique = charger_rapports(&dossier).unwrap();
        assert_eq!(historique.len(), 1);
        assert_eq!(historique[0].rapport.markdown, r.markdown);
        assert!(dossier.join("rapports_mensuels/1793000000000000000.md").is_file());
        let fichier = dossier.join("rapport_mensuel.json");
        ecrire(&fichier, br#"{"active":false,"seuil_libre_pct":10}"#).unwrap();
        ecrire(&fichier, br#"{"active":false,"seuil_libre_pct":15}"#).unwrap();
        assert_eq!(config(&dossier).unwrap().seuil_libre_pct, 15);
        fs::write(dossier.join("rapports_mensuels/1793000000000000001.json"), "corrompu").unwrap();
        assert!(charger_rapports(&dossier).is_err());
        fs::remove_dir_all(dossier).unwrap();
    }
    #[test]
    fn tache_quote_les_chemins_sans_shell() {
        let cmd = commande_tache(Path::new(r"C:\Apps locales\DiskLens.exe"), Path::new(r"C:\Rapports locaux")).unwrap();
        assert_eq!(cmd, "\"C:\\Apps locales\\DiskLens.exe\" --rapport-mensuel --data-dir \"C:\\Rapports locaux\"");
        assert!(commande_tache(Path::new("C:\\bad\".exe"), Path::new("C:\\Data")).is_err());
    }
    #[test]
    fn tache_desactivee_ne_devient_pas_active_par_sa_presence() {
        assert!(!xml_tache_active("<Task><Settings><Enabled>false</Enabled></Settings></Task>"));
        assert!(xml_tache_active("<Task><Triggers><Enabled>false</Enabled></Triggers><Settings><Enabled>true</Enabled></Settings></Task>"));
        assert!(!xml_tache_active("inaccessible"));
    }
    #[test]
    fn dates_lisibles_en_utc() {
        assert_eq!(date_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(date_utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(date_utc(u64::MAX), "date inconnue");
    }
}
