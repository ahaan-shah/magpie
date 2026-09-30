#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod forms;
mod icons;
mod motion;
mod palette;
mod receipts_cache;
mod theme;
mod toasts;
mod tour;
mod views;
mod widgets;

use magpie_core::Store;
use std::path::PathBuf;

const HELP: &str = "\
magpie — a fast, beautiful personal finance tracker

USAGE:
    magpie [OPTIONS]

OPTIONS:
    --data-dir <PATH>   Use a different data folder
    --demo [N]          Open a throwaway demo workspace (N extra transactions
                        for load testing), leaving your real data untouched
    -V, --version       Print version
    -h, --help          Print this help
";

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut data_dir: Option<PathBuf> = None;
    let mut demo: Option<usize> = None;
    let mut pending: Vec<String> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("magpie {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--demo" => demo = Some(0),
            other => pending.push(other.to_string()),
        }
    }
    if demo.is_some()
        && let Some(n) = pending.first().and_then(|s| s.parse().ok())
    {
        demo = Some(n);
    }

    let dir = data_dir.unwrap_or_else(magpie_core::data_dir);
    let store = if let Some(extra) = demo {
        let demo_dir = std::env::temp_dir().join("magpie-demo");
        let _ = std::fs::remove_dir_all(&demo_dir);
        let mut s = Store::open(&demo_dir).expect("open demo database");
        let started = std::time::Instant::now();
        magpie_core::demo::generate(&mut s, magpie_core::Cur::USD, 24, extra).expect("generate demo data");
        eprintln!(
            "magpie: demo workspace with {} transactions ({:.0?})",
            s.txns().len(),
            started.elapsed()
        );
        s
    } else {
        match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("magpie: couldn't open {}: {e}", dir.display());
                std::process::exit(1);
            }
        }
    };

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Magpie")
            .with_app_id("magpie")
            .with_inner_size([1340.0, 880.0])
            .with_min_inner_size([940.0, 620.0])
            .with_icon(icon),
        multisampling: 0,
        centered: true,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "magpie",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, store)))),
    )
}
