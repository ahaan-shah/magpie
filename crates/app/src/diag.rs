//! Diagnostics: a breadcrumb trail of recent user actions, a watchdog that
//! notices when a frame stalls, and a panic hook. Both write to
//! `<data dir>/magpie-diagnostics.log` so a freeze or crash leaves a trace.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();
static LOG: OnceLock<PathBuf> = OnceLock::new();
static CRUMBS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// Milliseconds since start at which the current frame began (0 = idle).
static FRAME_BEGIN: AtomicU64 = AtomicU64::new(0);

const STALL: Duration = Duration::from_secs(2);

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// Records a user action for the diagnostics trail.
pub fn crumb(s: impl Into<String>) {
    let s = format!("{:>8.1}s  {}", now_ms() as f64 / 1000.0, s.into());
    if let Ok(mut c) = CRUMBS.lock() {
        c.push_back(s);
        while c.len() > 30 {
            c.pop_front();
        }
    }
}

pub fn frame_begin() {
    FRAME_BEGIN.store(now_ms(), Ordering::Relaxed);
}

pub fn frame_end() {
    FRAME_BEGIN.store(0, Ordering::Relaxed);
}

fn write_report(title: &str, detail: &str) {
    let crumbs = CRUMBS
        .lock()
        .map(|c| c.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();
    let report = format!(
        "==== {title} · magpie {} · {} ====\n{detail}\nRecent actions:\n{crumbs}\n\n",
        env!("CARGO_PKG_VERSION"),
        jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S")
    );
    eprint!("{report}");
    if let Some(path) = LOG.get()
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path)
    {
        let _ = f.write_all(report.as_bytes());
    }
}

/// Installs the panic hook and starts the stall watchdog.
pub fn init(data_dir: &std::path::Path) {
    START.get_or_init(Instant::now);
    let _ = std::fs::create_dir_all(data_dir);
    let _ = LOG.set(data_dir.join("magpie-diagnostics.log"));
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_report(
            "PANIC",
            &format!("{info}\n{}", std::backtrace::Backtrace::force_capture()),
        );
        default_hook(info);
    }));
    std::thread::Builder::new()
        .name("magpie-watchdog".into())
        .spawn(|| {
            let mut reported = 0;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let begin = FRAME_BEGIN.load(Ordering::Relaxed);
                if begin != 0 && begin != reported && now_ms().saturating_sub(begin) > STALL.as_millis() as u64 {
                    reported = begin;
                    write_report(
                        "UI STALLED",
                        &format!("A frame has been running for over {}s.", STALL.as_secs()),
                    );
                }
            }
        })
        .ok();
}
