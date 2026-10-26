//! Commandes locales sans fenêtre et sans appel au fournisseur IA configuré.
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[derive(Debug, PartialEq)]
enum Action { Mensuel(Option<PathBuf>), Scan(String), Aide }

fn analyser(args: &[String]) -> Result<Option<Action>, String> {
    if args.is_empty() { return Ok(None); }
    if args == ["--help"] || args == ["-h"] { return Ok(Some(Action::Aide)); }
    if args[0] == "--rapport-mensuel" {
        return match args.len() {
            1 => Ok(Some(Action::Mensuel(None))),
            3 if args[1] == "--data-dir" && PathBuf::from(&args[2]).is_absolute() => Ok(Some(Action::Mensuel(Some(PathBuf::from(&args[2]))))),
            _ => Err("Usage : disklens --rapport-mensuel [--data-dir <dossier absolu>]".into()),
        };
    }
    if args.len() == 3 && args[0] == "--scan" && args[2] == "--json" {
        let path = std::path::Path::new(&args[1]);
        if !path.is_absolute() || args[1].starts_with("\\\\") || args[1].contains(['\r','\n']) { return Err("Un chemin local absolu est requis pour le scan.".into()); }
        return Ok(Some(Action::Scan(args[1].clone())));
    }
    Err("Arguments inconnus ou incomplets. Utilise disklens --help.".into())
}

pub fn traiter(args: Vec<String>) -> Option<Result<String, String>> {
    let action = match analyser(&args) { Ok(Some(a)) => a, Ok(None) => return None, Err(e) => return Some(Err(e)) };
    Some(match action {
        Action::Aide => Ok("DiskLens\n  Sans argument : interface Windows\n  --rapport-mensuel [--data-dir <dossier absolu>] : rapport local sans fenêtre\n  --scan <racine absolue> --json : analyse en lecture seule sur la sortie standard\nAucun appel IA ni nettoyage depuis ces commandes.".into()),
        Action::Mensuel(dir) => {
            let dir = dir.or_else(|| std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("com.omd.disklens")));
            match dir {
                Some(dir) => match tauri::async_runtime::block_on(crate::rapport_mensuel::generer(&dir)) {
                    Ok(r) => serde_json::to_string(&serde_json::json!({"id":r.id,"ts":r.ts,"dossier":dir.join("rapports_mensuels"),"alertes":r.alertes})).map_err(|e| e.to_string()),
                    Err(e) => {
                        // La tâche Windows a un code de sortie non nul et une trace locale exploitable.
                        if std::fs::create_dir_all(&dir).is_ok() { let _ = std::fs::write(dir.join("rapport_mensuel_erreur.log"), &e); }
                        Err(e)
                    }
                },
                None => Err("APPDATA est absent ; indique --data-dir.".into()),
            }
        },
        Action::Scan(root) => {
            if !std::path::Path::new(&root).is_dir() { return Some(Err("Dossier à analyser introuvable ou inaccessible.".into())); }
            let prog = crate::scan::Progress::default();
            let arena = crate::scan::scan_root(&root, &prog, &AtomicBool::new(false));
            let mut top: Vec<_> = arena.nodes.iter().enumerate().filter(|(_,n)| n.kind != crate::scan::Kind::Small).map(|(i,n)| (arena.path_of(i as u32), n.size, n.files, n.kind)).collect();
            top.sort_by(|a,b| b.1.cmp(&a.1)); top.truncate(20);
            serde_json::to_string(&serde_json::json!({"root":root,"scanned_at":arena.scanned_at,"size":arena.nodes.first().map(|n| n.size).unwrap_or(0),"files":arena.nodes.first().map(|n| n.files).unwrap_or(0),"errors":arena.errors,"top20":top})).map_err(|e| e.to_string())
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> { s.iter().map(|s| s.to_string()).collect() }
    #[test]
    fn gui_uniquement_sans_arguments() {
        assert_eq!(analyser(&[]).unwrap(), None);
        assert!(analyser(&args(&["--inconnu"])).is_err());
    }
    #[test]
    fn options_explicites_et_pas_de_scan_implicite() {
        assert_eq!(analyser(&args(&["--rapport-mensuel"])).unwrap(), Some(Action::Mensuel(None)));
        assert_eq!(analyser(&args(&["--rapport-mensuel", "--data-dir", "C:\\Rapports locaux"])).unwrap(), Some(Action::Mensuel(Some(PathBuf::from("C:\\Rapports locaux")))));
        assert_eq!(analyser(&args(&["--scan", "C:\\Test", "--json"])).unwrap(), Some(Action::Scan("C:\\Test".into())));
        for a in [args(&["--scan"]), args(&["--scan", "C:\\Test"]), args(&["--scan", "C:\\Test", "--json", "--rapport-mensuel"]), args(&["--rapport-mensuel", "--data-dir"])] { assert!(analyser(&a).is_err()); }
    }
}
