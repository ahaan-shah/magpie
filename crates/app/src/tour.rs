//! `MAGPIE_TOUR=<dir>` visits every page (in a dark and a light theme) and
//! saves a PNG of each, then quits. Used for README screenshots and as a
//! smoke test that every page renders.

use crate::app::{App, Page};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Extra {
    None,
    Quick(&'static str),
    NewTxn,
    Palette(&'static str),
    Help,
    Font(&'static str),
    Range(usize),
}

pub struct Tour {
    dir: PathBuf,
    steps: Vec<(Page, &'static str, Extra)>,
    index: usize,
    arrived: f64,
    waiting: bool,
}

const SETTLE: f64 = 1.6;

impl Tour {
    pub fn from_env() -> Option<Tour> {
        let dir = PathBuf::from(std::env::var_os("MAGPIE_TOUR")?);
        std::fs::create_dir_all(&dir).ok()?;
        let mut steps: Vec<(Page, &'static str, Extra)> =
            Page::NAV.iter().map(|p| (*p, "Midnight", Extra::None)).collect();
        steps.push((Page::Settings, "Midnight", Extra::None));
        steps.push((
            Page::Ledger,
            "Midnight",
            Extra::Quick("coffee 4.50 @Blue Bottle #treats yesterday"),
        ));
        steps.push((Page::Dashboard, "Midnight", Extra::NewTxn));
        steps.push((Page::Dashboard, "Midnight", Extra::Palette("the")));
        steps.push((Page::Dashboard, "Midnight", Extra::Help));
        steps.push((Page::Dashboard, "Daylight", Extra::None));
        steps.push((Page::Ledger, "Daylight", Extra::None));
        steps.push((Page::Budgets, "Paper", Extra::None));
        steps.push((Page::Reports, "Tokyo Night", Extra::None));
        steps.push((Page::Settings, "Paper", Extra::Font("Plus Jakarta Sans")));
        steps.push((Page::Dashboard, "Latte", Extra::Font("JetBrains Mono")));
        steps.push((Page::Reports, "Midnight", Extra::Range(0)));
        steps.push((Page::Reports, "Midnight", Extra::Range(5)));
        Some(Tour {
            dir,
            steps,
            index: 0,
            arrived: -1.0,
            waiting: false,
        })
    }

    pub fn step(&mut self, app: &mut App, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // Collect a finished screenshot.
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let (page, theme, extra) = self.steps[self.index];
            match extra {
                Extra::None => {}
                Extra::Quick(_) => app.ledger.clear_quick(),
                Extra::NewTxn => app.modal = None,
                Extra::Palette(_) => app.palette.open = false,
                Extra::Help => app.modal = None,
                Extra::Font(_) => app.set_font(ctx, "Inter"),
                Extra::Range(_) => app.reports.set_range(2),
            }
            let name = format!(
                "{:02}-{}-{}.png",
                self.index,
                page.title().to_lowercase(),
                theme.to_lowercase().replace(' ', "-")
            );
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(self.dir.join(&name));
                eprintln!("tour: saved {name}");
            }
            self.index += 1;
            self.waiting = false;
            self.arrived = -1.0;
            if self.index >= self.steps.len() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.waiting || self.index >= self.steps.len() {
            ctx.request_repaint();
            return;
        }
        let (page, theme, extra) = self.steps[self.index];
        if self.arrived < 0.0 {
            app.set_theme(ctx, theme);
            app.go(ctx, page);
            app.shown_at = now;
            self.arrived = now;
            match extra {
                Extra::None => {}
                Extra::Quick(text) => app.ledger.set_quick(text),
                Extra::NewTxn => {
                    let f = crate::forms::TxnForm::new(&app.store, app.today);
                    app.open_modal(ctx, crate::forms::Modal::Txn(f));
                }
                Extra::Help => app.open_modal(ctx, crate::forms::Modal::Help),
                Extra::Font(f) => app.set_font(ctx, f),
                Extra::Range(r) => app.reports.set_range(r),
                Extra::Palette(q) => {
                    app.palette.toggle(ctx);
                    app.palette.set_query(q);
                }
            }
        }
        if now - self.arrived > SETTLE {
            self.waiting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint();
    }
}
