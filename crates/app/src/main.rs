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
mod marks;
mod motion;
mod palette;
mod receipts_cache;
mod theme;
mod toasts;
mod tour;
mod updater;
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
    --licenses          Print the licenses of bundled fonts and libraries
    -V, --version       Print version
    -h, --help          Print this help
";

fn main() -> eframe::Result<()> {
    STARTED.get_or_init(Instant::now);
    #[cfg(windows)]
    if std::env::args().len() > 1 {
        attach_console();
    }
    let mut args = std::env::args().skip(1);
    let mut data_dir: Option<PathBuf> = None;
    let mut demo: Option<usize> = None;
    let mut pending: Vec<String> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print_quietly(HELP);
                return Ok(());
            }
            "--licenses" => {
                print_quietly(include_str!("../../../THIRD-PARTY-LICENSES.txt"));
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

    // A custom or demo workspace keeps its window and UI state with it, so
    // a test instance never touches the real app's saved state.
    let separate = data_dir.is_some() || demo.is_some();
    let dir = data_dir.unwrap_or_else(magpie_core::data_dir);
    diag::init(&dir);
    let demo_dir = std::env::temp_dir().join("magpie-demo");
    let state_dir = separate.then(|| if demo.is_some() { demo_dir.clone() } else { dir.clone() });
    let store = if let Some(extra) = demo {
        let _ = std::fs::remove_dir_all(&demo_dir);
        let mut s = match Store::open(&demo_dir) {
            Ok(s) => s,
            Err(e) => fatal(&format!(
                "Couldn't create the demo workspace in {}:\n{e}",
                demo_dir.display()
            )),
        };
        let started = std::time::Instant::now();
        if let Err(e) = magpie_core::demo::generate(&mut s, magpie_core::Cur::USD, 24, extra) {
            fatal(&format!("Couldn't generate demo data:\n{e}"));
        }
        eprintln!(
            "magpie: demo workspace with {} transactions ({:.0?})",
            s.txns().len(),
            started.elapsed()
        );
        s
    } else {
        match Store::open(&dir) {
            Ok(s) => s,
            Err(e) => fatal(&format!(
                "Couldn't open your Magpie data in {}:\n{e}\n\nYour data hasn't been changed.",
                dir.display()
            )),
        }
    };
    // If a renderer fails to start and we retry with another, the first
    // attempt may already have taken the store; reopen it from disk then.
    let store_dir = if demo.is_some() { demo_dir.clone() } else { dir.clone() };
    let slot = std::rc::Rc::new(std::cell::RefCell::new(Some(store)));

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-512.png")).unwrap_or_default();
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
        persistence_path: state_dir,
        ..Default::default()
    };
    let run = |options: eframe::NativeOptions| {
        let slot = slot.clone();
        let store_dir = store_dir.clone();
        eframe::run_native(
            "magpie",
            options,
            Box::new(move |cc| {
                let store = match slot.borrow_mut().take() {
                    Some(s) => s,
                    None => Store::open(&store_dir)?,
                };
                Ok(Box::new(app::App::new(cc, store)))
            }),
        )
    };
    let result = run_with_fallback(options, run);
    if let Err(e) = &result {
        fatal(&format!(
            "Magpie couldn't open its window:\n{e}\n\nThis usually means the graphics driver is missing or out of date."
        ));
    }
    result
}

/// Starts the UI. On Windows, OpenGL comes first (lightest), and if it can't
/// start (no usable driver: VMs, remote desktop, some older machines) Magpie
/// retries with Direct3D 12, which always has a software fallback.
/// `MAGPIE_RENDERER=glow|wgpu` forces one.
#[cfg(windows)]
fn run_with_fallback(
    options: eframe::NativeOptions,
    run: impl Fn(eframe::NativeOptions) -> eframe::Result<()>,
) -> eframe::Result<()> {
    use eframe::Renderer;
    let wgpu_options = |mut o: eframe::NativeOptions| {
        o.renderer = Renderer::Wgpu;
        o.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
        o
    };
    match std::env::var("MAGPIE_RENDERER").ok().as_deref() {
        Some("wgpu") => return run(wgpu_options(options)),
        Some("glow") => {
            let mut o = options;
            o.renderer = Renderer::Glow;
            return run(o);
        }
        _ => {}
    }
    let mut glow = options.clone();
    glow.renderer = Renderer::Glow;
    match run(glow) {
        Ok(()) => Ok(()),
        Err(e) => {
            diag::note(
                "RENDERER",
                &format!("OpenGL failed to start ({e}); retrying with Direct3D"),
            );
            run(wgpu_options(options))
        }
    }
}

#[cfg(not(windows))]
fn run_with_fallback(
    options: eframe::NativeOptions,
    run: impl Fn(eframe::NativeOptions) -> eframe::Result<()>,
) -> eframe::Result<()> {
    run(options)
}

/// Prints to stdout, ignoring a closed pipe (`magpie --licenses | head`).
fn print_quietly(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

/// Reports an error that stops Magpie from starting, in a dialog as well as
/// on stderr (a Windows app has no console to print to), then exits.
fn fatal(msg: &str) -> ! {
    diag::note("COULDN'T START", msg);
    if !app::headless() {
        let log = diag::log_path()
            .map(|p| format!("\n\nDetails are in {}", p.display()))
            .unwrap_or_default();
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Magpie couldn't start")
            .set_description(format!("{msg}{log}"))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    std::process::exit(1);
}

/// A Windows GUI app has no console, so `magpie --version` from a terminal
/// would print nothing. Attach to the terminal it was started from, if any.
#[cfg(windows)]
fn attach_console() {
    // SAFETY: plain Win32 call with no pointers; failing (no parent console)
    // is harmless.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}

#[cfg(test)]
mod monkey;

#[cfg(test)]
mod film;

#[cfg(test)]
mod promo;
