#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(result) = disklens_lib::traiter_cli(std::env::args().skip(1).collect()) {
        match result {
            Ok(text) => { println!("{text}"); return; },
            Err(e) => { eprintln!("DiskLens : {e}"); std::process::exit(1); },
        }
    }
    disklens_lib::run()
}
