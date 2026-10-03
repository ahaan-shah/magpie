//! First-run welcome: pick a currency and a starting account, then start
//! fresh, import a bank statement, or explore with demo data. A second step
//! asks how Magpie should feel (Basic or Advanced), with a preview of each.

use crate::app::App;
use crate::forms::currency_picker;
use crate::icons::ph;
use crate::modes;
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use magpie_core::money::{self, Cur};
use magpie_core::{AccountKind, demo};

pub struct State {
    base: Cur,
    name: String,
    balance: String,
    shown_at: Option<f64>,
    /// What the first step's button asked for, once pressed: the mode
    /// step shows until Begin.
    choice: Option<Choice>,
    basic: bool,
    /// Set by Begin: the welcome fades out, then the app opens.
    leaving_at: Option<f64>,
}

/// Used when the account name is left blank.
const DEFAULT_ACCOUNT: &str = "Current account";

/// How long the welcome takes to fade out after Begin.
const LEAVE: f32 = 0.28;

impl State {
    pub fn new() -> State {
        State {
            base: Cur::USD,
            name: String::new(),
            balance: String::new(),
            shown_at: None,
            choice: None,
            basic: true,
            leaving_at: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Choice {
    Fresh,
    Demo,
    Import,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let Some(st) = app.onboarding.as_mut() else { return };
    let shown = *st.shown_at.get_or_insert(now);
    let leave = st
        .leaving_at
        .map(|at| motion::ease_out(((now - at) as f32 / LEAVE).clamp(0.0, 1.0)))
        .unwrap_or(0.0);
    if st.leaving_at.is_some() {
        ctx.request_repaint();
    }
    // The mode step slides in over the first: 0 = details, 1 = mode.
    let step = motion::toggle(&ctx, Id::new("onb-step"), st.choice.is_some(), motion::EMPHASIS);
    let mut begin = false;
    egui::CentralPanel::no_frame()
        .frame(egui::Frame::new().fill(t.bg))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.set_opacity(1.0 - leave);
            // Soft accent glow behind the card.
            let glow = motion::appear(&ctx, shown, 0.0, 1.2);
            w::radial_glow(
                ui.painter(),
                full.center() - vec2(0.0, 120.0),
                520.0 + 200.0 * step,
                motion::with_alpha(t.accent, 0.16 * glow),
            );
            let p = motion::appear(&ctx, shown, 0.1, 0.6);
            if step < 0.999 {
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(full));
                c.set_opacity(p * (1.0 - step));
                if st.choice.is_some() {
                    c.disable();
                }
                details(&mut c, &t, st, full, (1.0 - p) * 24.0 - step * 40.0);
            }
            if step > 0.001 {
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(full));
                c.set_opacity(step);
                if st.choice.is_none() || st.leaving_at.is_some() {
                    c.disable();
                }
                begin = mode_step(&mut c, &t, st, full, (1.0 - step) * 40.0);
            }
        });

    if begin && let Some(st) = app.onboarding.as_mut() {
        st.leaving_at.get_or_insert(now);
    }
    let done = app
        .onboarding
        .as_ref()
        .and_then(|st| st.leaving_at)
        .is_some_and(|at| now - at >= LEAVE as f64);
    if done {
        finish(app, &ctx);
    }
}

/// Step 1: currency, first account, and how to begin.
fn details(ui: &mut Ui, t: &Theme, st: &mut State, full: Rect, dy: f32) {
    let card_w = 480.0;
    let rect = Rect::from_center_size(full.center() + vec2(0.0, dy), vec2(card_w, 560.0));
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    w::card_frame(t)
        .inner_margin(egui::Margin::same(32))
        .show(&mut ui, |ui| {
            ui.set_width(card_w - 64.0);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                crate::app::logo(ui.painter(), r, t);
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new("Welcome to Magpie")
                        .font(theme::display(28.0))
                        .color(t.text),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("A calm nest for your money.")
                        .font(theme::regular(17.0))
                        .italics()
                        .color(t.text2),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("All your data, on your computer.")
                        .font(theme::regular(13.5))
                        .color(t.text3),
                );
            });
            ui.add_space(22.0);
            let wd = ui.available_width();
            w::field_label(ui, t, "Your main currency");
            currency_picker(ui, "onb-cur", &mut st.base, wd);
            ui.add_space(10.0);
            w::field_label(ui, t, "First account");
            w::text_field(ui, t, Id::new("onb-name"), &mut st.name, DEFAULT_ACCOUNT, wd);
            ui.add_space(10.0);
            w::field_label(ui, t, "Balance today");
            w::text_field(ui, t, Id::new("onb-bal"), &mut st.balance, "0.00", wd);
            ui.add_space(20.0);
            let mut pick = None;
            ui.vertical_centered_justified(|ui| {
                let start = w::primary(ui, t, None, "Get started");
                crate::marks::record(|| "onb:start".into(), start.rect);
                if start.clicked() {
                    pick = Some(Choice::Fresh);
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if w::ghost(ui, t, Some(ph::SPARKLE), "Explore with demo data").clicked() {
                    pick = Some(Choice::Demo);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::ghost(ui, t, Some(ph::UPLOAD_SIMPLE), "Import data")
                        .on_hover_text("A statement from your bank: CSV, Excel or OFX")
                        .clicked()
                    {
                        pick = Some(Choice::Import);
                    }
                });
            });
            if pick.is_some() {
                st.choice = pick;
                ui.memory_mut(|m| m.stop_text_input());
            }
        });
}

/// Step 2: Basic or Advanced, with a live preview. Returns true on Begin.
fn mode_step(ui: &mut Ui, t: &Theme, st: &mut State, full: Rect, dy: f32) -> bool {
    let size = vec2(880.0_f32.min(full.width() - 48.0), 520.0);
    let rect = Rect::from_center_size(full.center() + vec2(0.0, dy), size);
    crate::marks::record(|| "onb:mode".into(), rect);
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let ctx = ui.ctx().clone();
    let mut begin = false;
    // Arrow keys flip the choice and Enter begins.
    if st.choice.is_some() && st.leaving_at.is_none() && ctx.memory(|m| m.focused().is_none()) {
        ctx.input(|i| {
            use egui::Key::*;
            if [ArrowUp, ArrowLeft, K, H].iter().any(|k| i.key_pressed(*k)) {
                st.basic = true;
            }
            if [ArrowDown, ArrowRight, J, L].iter().any(|k| i.key_pressed(*k)) {
                st.basic = false;
            }
            if i.key_pressed(Enter) {
                begin = true;
            }
        });
    }
    let inner = size - vec2(64.0, 64.0);
    w::card_frame(t)
        .inner_margin(egui::Margin::same(32))
        .show(&mut ui, |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            let left_w = 300.0;
            let gap = 36.0;
            let right_w = inner.x - left_w - gap;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(left_w, inner.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(left_w);
                    ui.set_height(inner.y);
                    ui.label(
                        egui::RichText::new("How should Magpie feel?")
                            .font(theme::display(24.0))
                            .color(t.text),
                    );
                    ui.add_space(4.0);
                    ui.label(w::subtle(t, "Pick what suits you. Nothing is lost either way."));
                    ui.add_space(22.0);
                    for basic in [true, false] {
                        if option_row(ui, t, basic, st.basic == basic, left_w).clicked() {
                            st.basic = basic;
                        }
                        ui.add_space(10.0);
                    }
                    ui.add_space(2.0);
                    ui.label(w::faint(
                        t,
                        concat!("Switch any time in Settings, or with ", shortcut!("Shift T"), "."),
                    ));
                    // Back and Begin sit at the bottom of the column.
                    let col = ui.max_rect();
                    let row = Rect::from_min_max(pos2(col.left(), col.bottom() - 36.0), col.max);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                        ui.horizontal(|ui| {
                            if w::ghost(ui, t, Some(ph::ARROW_LEFT), "Back").clicked() {
                                st.choice = None;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let b = w::primary(ui, t, None, "Begin");
                                crate::marks::record(|| "onb:begin".into(), b.rect);
                                if b.clicked() {
                                    begin = true;
                                }
                            });
                        });
                    });
                });
                ui.add_space(gap - ui.spacing().item_spacing.x);
                let (area, _) = ui.allocate_exact_size(vec2(right_w, inner.y), Sense::hover());
                preview(ui, t, area, st.basic);
            });
        });
    begin
}

/// One of the two choices: a radio, the mode's name and a few words.
fn option_row(ui: &mut Ui, t: &Theme, basic: bool, selected: bool, width: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(width, 68.0), Sense::click());
    crate::marks::record(|| format!("onb:{}", modes::mode(basic).name), r);
    let ctx = ui.ctx();
    let h = motion::toggle(ctx, Id::new(("onb-opt-h", basic)), resp.hovered(), motion::MICRO);
    let s = motion::toggle(ctx, Id::new(("onb-opt-s", basic)), selected, motion::STANDARD);
    let p = ui.painter();
    let fill = motion::lerp_color(motion::lerp_color(t.card, t.hover, h), t.accent_soft(), s);
    let edge = motion::lerp_color(t.border, t.accent, s);
    p.rect(
        r,
        CornerRadius::same(12),
        fill,
        Stroke::new(1.0 + s * 0.5, edge),
        egui::StrokeKind::Inside,
    );
    let c = pos2(r.left() + 22.0, r.center().y);
    p.circle_stroke(c, 8.0, Stroke::new(1.5, motion::lerp_color(t.text3, t.accent, s)));
    if s > 0.01 {
        p.circle_filled(c, 4.5 * s, t.accent);
    }
    let m = modes::mode(basic);
    p.text(
        pos2(r.left() + 42.0, r.center().y - 10.0),
        Align2::LEFT_CENTER,
        m.name,
        theme::semibold(15.0),
        t.text,
    );
    p.text(
        pos2(r.left() + 42.0, r.center().y + 11.0),
        Align2::LEFT_CENTER,
        m.short,
        theme::regular(12.5),
        t.text2,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The miniature app plus the chosen mode's tagline, both crossfading as
/// the choice changes.
fn preview(ui: &Ui, t: &Theme, area: Rect, basic: bool) {
    let k = motion::toggle(ui.ctx(), Id::new("onb-preview"), !basic, motion::EMPHASIS);
    let caption_h = 84.0;
    let pw = area.width().min((area.height() - caption_h) * 1.5);
    let pr = Rect::from_min_size(pos2(area.center().x - pw / 2.0, area.top()), vec2(pw, pw / 1.5));
    let p = ui.painter();
    modes::paint_preview(p, t, pr, k);
    let y = pr.bottom() + 20.0;
    // One caption fades out before the other fades in, so they never overlap.
    let out_in = [(1.0 - 2.0 * k).max(0.0), (2.0 * k - 1.0).max(0.0)];
    for (m, a) in [(&modes::BASIC, out_in[0]), (&modes::ADVANCED, out_in[1])] {
        if a < 0.01 {
            continue;
        }
        let dy = (1.0 - a) * 6.0;
        p.text(
            pos2(pr.left(), y + dy),
            Align2::LEFT_TOP,
            m.tagline,
            theme::display(19.0),
            motion::with_alpha(t.text, a),
        );
        let g = p.layout(
            m.detail.to_string(),
            theme::regular(13.5),
            motion::with_alpha(t.text2, a),
            pr.width(),
        );
        p.galley(pos2(pr.left(), y + 30.0 + dy), g, t.text2);
    }
}

/// Begin: sets everything up the way step 1 asked, in the chosen mode, and
/// opens the app (which fades in from here).
fn finish(app: &mut App, ctx: &egui::Context) {
    let Some(mut st) = app.onboarding.take() else { return };
    let Some(choice) = st.choice else {
        st.leaving_at = None;
        app.onboarding = Some(st);
        return;
    };
    let base = st.base;
    let basic = st.basic;
    let result = (|| -> magpie_core::Result<String> {
        match choice {
            Choice::Demo => {
                demo::generate(&mut app.store, base, 24, 0)?;
                app.store.update_settings(|s| s.basic = basic)?;
                Ok("Demo data loaded — poke around!".into())
            }
            Choice::Fresh | Choice::Import => {
                demo::seed_defaults(&mut app.store, base)?;
                if let Some(first) = app.store.accounts().first().cloned() {
                    let mut a = first;
                    a.name = if st.name.trim().is_empty() {
                        DEFAULT_ACCOUNT.into()
                    } else {
                        st.name.trim().into()
                    };
                    a.kind = AccountKind::Checking;
                    a.currency = base;
                    a.opening = money::parse(&st.balance, base).unwrap_or(0);
                    app.store.save_account(a)?;
                }
                app.store.update_settings(|s| {
                    s.onboarded = true;
                    s.basic = basic;
                })?;
                if let Choice::Import = choice {
                    let balance = (!st.balance.trim().is_empty())
                        .then(|| money::parse(&st.balance, base))
                        .flatten();
                    app.dialogs.pick(
                        ctx,
                        crate::dialogs::Purpose::ImportFirst(balance),
                        "Import a bank statement",
                        ("Bank statements", magpie_core::statement::EXTENSIONS),
                    );
                    return Ok("Pick a statement from your bank to bring your history in".into());
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
            st.leaving_at = None;
            app.onboarding = Some(st);
            return;
        }
    }
    let now = ctx.input(|i| i.time);
    app.shown_at = now;
    app.intro_at = Some(now);
    app.page = crate::app::Page::Dashboard;
}
