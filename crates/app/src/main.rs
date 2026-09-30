#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// A shortcut label with the platform's command key: `shortcut!("K")` is
/// "Ctrl K" on Linux and "Cmd K" on macOS.
#[cfg(target_os = "macos")]
macro_rules! shortcut {
    ($k:literal) => {
        concat!("Cmd ", $k)
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! shortcut {
    ($k:literal) => {
        concat!("Ctrl ", $k)
    };
}

mod app;
mod diag;
mod dialogs;
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
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static STARTED: OnceLock<Instant> = OnceLock::new();
static FIRST_FRAME: AtomicBool = AtomicBool::new(true);

/// With `MAGPIE_DEBUG=1`, reports time-to-first-frame and any frame whose
/// UI pass takes longer than 8ms.
pub fn debug_frame(page: app::Page, took: Duration) {
    if std::env::var_os("MAGPIE_DEBUG").is_none() {
        return;
    }
    if FIRST_FRAME.swap(false, Ordering::Relaxed)
        && let Some(s) = STARTED.get()
    {
        eprintln!("magpie: first frame after {:.0?} (ui pass {took:.1?})", s.elapsed());
    } else if took > Duration::from_millis(8) {
        eprintln!("magpie: slow frame on {page:?}: {took:.1?}");
    }
}

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
    STARTED.get_or_init(Instant::now);
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
    diag::init(&dir);
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
        // Don't block in eglSwapBuffers waiting for vblank: on Wayland a hidden
        // window (other workspace) never gets frame callbacks, so a vsync'd
        // swap can hang the UI thread until the compositor says "not
        // responding". Magpie paces its own frames instead (see App::ui).
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: false,
            ..Default::default()
        },
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

#[cfg(test)]
mod monkey;
