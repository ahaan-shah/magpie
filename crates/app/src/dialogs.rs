//! Native file dialogs run off the UI thread, so the window keeps
//! repainting (and answering the compositor) while a dialog is open.

use magpie_core::Id as RowId;
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
pub enum Purpose {
    ImportCsv,
    Attach(RowId),
    Backup,
}

#[derive(Default)]
pub struct Dialogs {
    pending: Option<(Purpose, mpsc::Receiver<Option<PathBuf>>)>,
}

impl Dialogs {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    fn spawn<F>(&mut self, ctx: &egui::Context, purpose: Purpose, fut: F)
    where
        F: std::future::Future<Output = Option<rfd::FileHandle>> + Send + 'static,
    {
        if self.busy() || crate::app::headless() {
            return;
        }
        crate::diag::crumb(format!("file dialog open: {purpose:?}"));
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = pollster::block_on(fut).map(|h| h.path().to_path_buf());
            let _ = tx.send(path);
            ctx.request_repaint();
        });
        self.pending = Some((purpose, rx));
    }

    /// No dialog when one is already open, or in headless tests (on macOS,
    /// even *creating* a dialog off the main thread panics).
    fn blocked(&self) -> bool {
        self.busy() || crate::app::headless()
    }

    pub fn pick(&mut self, ctx: &egui::Context, purpose: Purpose, title: &str, filter: (&str, &[&str])) {
        if self.blocked() {
            return;
        }
        // The dialog is created on the UI thread (required on macOS) and only
        // awaited on the worker thread.
        let fut = rfd::AsyncFileDialog::new()
            .set_title(title)
            .add_filter(filter.0, filter.1)
            .pick_file();
        self.spawn(ctx, purpose, fut);
    }

    pub fn save(&mut self, ctx: &egui::Context, purpose: Purpose, name: &str, filter: (&str, &[&str])) {
        if self.blocked() {
            return;
        }
        let fut = rfd::AsyncFileDialog::new()
            .set_file_name(name)
            .add_filter(filter.0, filter.1)
            .save_file();
        self.spawn(ctx, purpose, fut);
    }

    /// Returns a finished dialog's result, if any.
    pub fn poll(&mut self) -> Option<(Purpose, Option<PathBuf>)> {
        let (purpose, rx) = self.pending.as_ref()?;
        match rx.try_recv() {
            Ok(path) => {
                let purpose = *purpose;
                self.pending = None;
                crate::diag::crumb(format!("file dialog closed: {purpose:?} -> {path:?}"));
                Some((purpose, path))
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let purpose = *purpose;
                self.pending = None;
                Some((purpose, None))
            }
        }
    }
}
