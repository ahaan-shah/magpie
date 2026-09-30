//! First-run welcome: pick a currency and a starting account, or explore
//! with demo data, or bring data over from Pear.

use crate::app::App;
use crate::forms::currency_picker;
use crate::icons::ph;
use crate::motion;
use crate::theme;
use crate::widgets as w;
use egui::{Id, Rect, Ui, vec2};
use magpie_core::money::{self, Cur};
use magpie_core::{AccountKind, demo};

pub struct State {
    base: Cur,
    name: String,
    balance: String,
    shown_at: Option<f64>,
    pear: Option<std::path::PathBuf>,
}

impl State {
    pub fn new() -> State {
        State {
            base: Cur::USD,
            name: "Everyday".into(),
            balance: String::new(),
            shown_at: None,
            pear: magpie_core::io::pear_path(),
        }
    }
}

enum Choice {
    Fresh,
    Demo,
    Pear,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let Some(st) = app.onboarding.as_mut() else { return };
    let shown = *st.shown_at.get_or_insert(now);
    let mut choice = None;
    egui::CentralPanel::no_frame()
        .frame(egui::Frame::new().fill(t.bg))
        .show(ui, |ui| {
            let full = ui.max_rect();
            // Soft accent glow behind the card.
            let glow = motion::appear(&ctx, shown, 0.0, 1.2);
            for (i, r) in [420.0, 300.0, 200.0].iter().enumerate() {
                ui.painter().circle_filled(
                    full.center() - vec2(0.0, 120.0),
                    *r,
                    motion::with_alpha(t.accent, 0.035 * glow * (i as f32 + 1.0)),
                );
            }
            let p = motion::appear(&ctx, shown, 0.1, 0.6);
            let card_w = 480.0;
            let rect = Rect::from_center_size(full.center() + vec2(0.0, (1.0 - p) * 24.0), vec2(card_w, 560.0));
            let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            ui.set_opacity(p);
            w::card_frame(&t)
                .inner_margin(egui::Margin::same(32))
                .show(&mut ui, |ui| {
                    ui.set_width(card_w - 64.0);
                    ui.vertical_centered(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), egui::Sense::hover());
                        crate::app::logo(ui.painter(), r, &t);
                        ui.add_space(14.0);
                        ui.label(
                            egui::RichText::new("Welcome to Magpie")
                                .font(theme::display(28.0))
                                .color(t.text),
                        );
                        ui.label(w::subtle(
                            &t,
                            "A calm, fast home for your money. Everything stays on this computer.",
                        ));
                    });
                    ui.add_space(22.0);
                    let wd = ui.available_width();
                    w::field_label(ui, &t, "Your main currency");
                    currency_picker(ui, "onb-cur", &mut st.base, wd);
                    ui.add_space(10.0);
                    w::field_label(ui, &t, "First account");
                    w::text_field(ui, &t, Id::new("onb-name"), &mut st.name, "Everyday", wd);
                    ui.add_space(10.0);
                    w::field_label(ui, &t, "Current balance");
                    w::text_field(ui, &t, Id::new("onb-bal"), &mut st.balance, "0.00", wd);
                    ui.add_space(20.0);
                    ui.vertical_centered_justified(|ui| {
                        if w::primary(ui, &t, Some(ph::ARROW_RIGHT), "Get started").clicked() {
                            choice = Some(Choice::Fresh);
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if w::ghost(ui, &t, Some(ph::SPARKLE), "Explore with demo data").clicked() {
                            choice = Some(Choice::Demo);
                        }
                        if st.pear.is_some() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if w::ghost(ui, &t, Some(ph::UPLOAD_SIMPLE), "Import from Pear").clicked() {
                                    choice = Some(Choice::Pear);
                                }
                            });
                        }
                    });
                });
        });

    let Some(choice) = choice else { return };
    let Some(st) = app.onboarding.take() else { return };
    let base = st.base;
    let result = (|| -> magpie_core::Result<String> {
        match choice {
            Choice::Demo => {
                demo::generate(&mut app.store, base, 24, 0)?;
                Ok("Demo data loaded — poke around!".into())
            }
            Choice::Fresh | Choice::Pear => {
                demo::seed_defaults(&mut app.store, base)?;
                if let Some(first) = app.store.accounts().first().cloned() {
                    let mut a = first;
                    a.name = if st.name.trim().is_empty() {
                        "Everyday".into()
                    } else {
                        st.name.trim().into()
                    };
                    a.kind = AccountKind::Checking;
                    a.currency = base;
                    a.opening = money::parse(&st.balance, base).unwrap_or(0);
                    app.store.save_account(a)?;
                }
                app.store.update_settings(|s| s.onboarded = true)?;
                if let (Choice::Pear, Some(path)) = (choice, st.pear.as_ref()) {
                    let acc = app.store.accounts()[0].id;
                    let n = magpie_core::io::import_pear(&mut app.store, path, acc)?;
                    return Ok(format!("Imported {n} transactions from Pear"));
                }
                Ok(concat!(
                    "You're all set. Press ",
                    shortcut!("N"),
                    " to add your first transaction."
                )
                .into())
            }
        }
    })();
    match result {
        Ok(msg) => app.toasts.success(msg),
        Err(e) => {
            app.toasts.error(e.to_string());
            app.onboarding = Some(st);
            return;
        }
    }
    app.shown_at = ctx.input(|i| i.time);
    app.page = crate::app::Page::Dashboard;
}
