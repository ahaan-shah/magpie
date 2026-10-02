//! Update checks and installs, run off the UI thread. Magpie looks for a new
//! release on launch (unless turned off in Settings) and when asked, but only
//! downloads and installs when the person clicks Update, and only restarts
//! when they click Restart.

use magpie_core::update::{self, Plan, Release};
use std::sync::mpsc;

/// The spinner shows for at least this long, so a fast check still reads as
/// "checked" rather than a flicker.
const MIN_CHECK: f64 = 0.9;
/// Launch check delay: let the first frames and data load settle first.
const LAUNCH_DELAY: f64 = 2.5;

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Can't install in place here; offer the release page instead.
    Manual(Release, String),
    Installing(Release),
    /// Installed; takes effect after a restart.
    Ready(Release),
    Failed(String),
}

enum Msg {
    Checked(Result<Option<Release>, String>),
    Progress(f32),
    Installed(Result<(), String>),
}

pub struct Updater {
    pub phase: Phase,
    pub progress: f32,
    rx: Option<mpsc::Receiver<Msg>>,
    started: f64,
    /// A check result waiting out MIN_CHECK.
    held: Option<Result<Option<Release>, String>>,
    launch_done: bool,
    manual: bool,
    restart_on_exit: bool,
}

impl Default for Updater {
    fn default() -> Self {
        update::cleanup();
        Updater {
            phase: Phase::Idle,
            progress: 0.0,
            rx: None,
            started: 0.0,
            held: None,
            launch_done: false,
            manual: false,
            restart_on_exit: false,
        }
    }
}

impl Updater {
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Installing(_))
    }

    /// Whether the sidebar should show the update / restart prompt.
    pub fn prompt(&self) -> bool {
        matches!(
            self.phase,
            Phase::Available(_) | Phase::Manual(..) | Phase::Installing(_) | Phase::Ready(_)
        )
    }

    /// The once-per-launch background check.
    pub fn launch_check(&mut self, ctx: &egui::Context, enabled: bool) {
        if self.launch_done || crate::app::headless() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now < LAUNCH_DELAY {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(LAUNCH_DELAY - now + 0.05));
            return;
        }
        self.launch_done = true;
        if enabled {
            self.check(ctx, false);
        }
    }

    /// Looks for a newer release. `manual` checks report failures; the
    /// launch check stays quiet if offline.
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.busy() || matches!(self.phase, Phase::Ready(_)) {
            return;
        }
        self.manual = manual;
        self.started = ctx.input(|i| i.time);
        self.held = None;
        if manual {
            self.phase = Phase::Checking;
        }
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let r = update::check().map_err(|e| match e {
                magpie_core::Error::Http(m) if m == update::OFFLINE => {
                    "Couldn't check. Make sure you are connected to the internet.".to_string()
                }
                e => {
                    crate::diag::crumb(format!("update check failed: {e}"));
                    "Couldn't check for updates right now. Try again later.".to_string()
                }
            });
            let _ = tx.send(Msg::Checked(r));
            ctx2.request_repaint();
        });
        self.rx = Some(rx);
        crate::diag::crumb(if manual {
            "update check"
        } else {
            "update check (launch)"
        });
    }

    /// Downloads and installs the available release. Only ever called from
    /// a click.
    pub fn install(&mut self, ctx: &egui::Context) {
        let Phase::Available(release) = self.phase.clone() else {
            return;
        };
        self.phase = Phase::Installing(release.clone());
        self.progress = 0.0;
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let ctx3 = ctx2.clone();
            let r = update::install(&release, &move |p| {
                let _ = tx2.send(Msg::Progress(p));
                ctx3.request_repaint();
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Installed(r));
            ctx2.request_repaint();
        });
        self.rx = Some(rx);
        crate::diag::crumb("update install");
    }

    /// Closes Magpie and starts the new version once this one has exited.
    pub fn restart(&mut self, ctx: &egui::Context) {
        if matches!(self.phase, Phase::Ready(_)) {
            self.restart_on_exit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Called from `App::on_exit`, after everything is saved.
    pub fn on_exit(&self) {
        if !self.restart_on_exit {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        #[cfg(target_os = "macos")]
        {
            // Relaunch the bundle so macOS treats it as the same app.
            if let Some(app) = exe
                .ancestors()
                .nth(3)
                .filter(|a| a.extension().is_some_and(|e| e == "app"))
            {
                let _ = std::process::Command::new("open").arg("-n").arg(app).spawn();
                return;
            }
        }
        let _ = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .spawn();
    }

    /// Applies results from the worker thread. Returns a message for a toast
    /// when something worth saying happened.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Result<String, String>> {
        let mut note = None;
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Checked(r) => self.held = Some(r),
                    Msg::Progress(p) => self.progress = p,
                    Msg::Installed(Ok(())) => {
                        if let Phase::Installing(r) = &self.phase {
                            note = Some(Ok(format!("Magpie {} is installed. Restart to use it.", r.version)));
                            self.phase = Phase::Ready(r.clone());
                        }
                    }
                    Msg::Installed(Err(e)) => {
                        crate::diag::crumb(format!("update failed: {e}"));
                        note = Some(Err(format!("Couldn't update: {e}")));
                        self.phase = Phase::Failed(format!("Couldn't update: {e}"));
                    }
                }
            }
        }
        if let Some(held) = self.held.take() {
            let now = ctx.input(|i| i.time);
            if self.manual && now - self.started < MIN_CHECK {
                self.held = Some(held);
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(MIN_CHECK - (now - self.started)));
            } else {
                self.rx = None;
                self.phase = match held {
                    Ok(Some(r)) => match update::plan() {
                        Plan::Replace { .. } => Phase::Available(r),
                        Plan::Manual(why) => Phase::Manual(r, why),
                    },
                    Ok(None) => {
                        if self.manual {
                            Phase::UpToDate
                        } else {
                            Phase::Idle
                        }
                    }
                    Err(e) => {
                        crate::diag::crumb(format!("update check failed: {e}"));
                        if self.manual { Phase::Failed(e) } else { Phase::Idle }
                    }
                };
            }
        }
        if self.busy() {
            ctx.request_repaint();
        }
        note
    }
}
