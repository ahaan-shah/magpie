//! Settings: appearance, currency & exchange rates, categories, data.

use crate::app::App;
use crate::forms::{self, ConfirmAction, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, THEMES, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use magpie_core::io::Format;
use magpie_core::money::Cur;
use magpie_core::{CategoryKind, Id as RowId};

#[derive(Default)]
pub struct State {
    rate_edit: Option<(Cur, String)>,
}

enum Act {
    Theme(&'static str),
    Scale(f32),
    Font(&'static str),
    ScrollSpeed(f32),
    Base(Cur),
    FxAuto(bool),
    RefreshFx,
    SetRate(Cur, f64),
    ResetRate(Cur),
    EditCat(RowId),
    NewCat(CategoryKind),
    ImportCsv,
    ImportPear,
    Export(Format),
    Backup,
    OpenFolder,
    Demo,
}

pub fn export_all(app: &mut App, f: Format) {
    let path = magpie_core::io::export_path(&magpie_core::downloads_dir(), "magpie-transactions", f.ext());
    let txns = app.store.txns().to_vec();
    match magpie_core::io::export(&app.store, &txns, f, &path) {
        Ok(()) => app
            .toasts
            .success(format!("Exported {} transactions to {}", txns.len(), path.display())),
        Err(e) => app.toasts.error(e.to_string()),
    }
}

fn section(ui: &mut Ui, t: &Theme, title: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    w::card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).font(theme::semibold(16.0)).color(t.text));
        if !sub.is_empty() {
            ui.label(w::subtle(t, sub));
        }
        ui.add_space(12.0);
        add(ui);
    });
    ui.add_space(theme::GAP);
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let mut acts = Vec::new();
    let current_theme = app.theme.target_name();

    let zoom = ui.ctx().zoom_factor();
    let s_font = app.store.settings().font.clone();
    // ← / → cycle themes on this page.
    if app.keys.left || app.keys.right {
        let i = THEMES.iter().position(|x| x.name == current_theme).unwrap_or(0);
        let n = THEMES.len();
        let j = if app.keys.right { (i + 1) % n } else { (i + n - 1) % n };
        acts.push(Act::Theme(THEMES[j].name));
    }
    section(
        ui,
        &t,
        "Appearance and feel",
        "Pick a look (← → to flip through).",
        |ui| {
            for (label, dark) in [("LIGHT", false), ("DARK", true)] {
                ui.label(w::faint(&t, label));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    for th in THEMES.iter().filter(|th| th.dark == dark) {
                        if theme_tile(ui, &t, th, th.name == current_theme).clicked() {
                            acts.push(Act::Theme(th.name));
                        }
                    }
                });
                ui.add_space(10.0);
            }
            ui.add_space(18.0);
            ui.label(egui::RichText::new("Font").font(theme::semibold(14.0)).color(t.text));
            ui.label(w::subtle(&t, "Every font is bundled with Magpie and works offline."));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let current = s_font.clone();
                let label = egui::RichText::new(&current)
                    .family(theme::preview_family(&current))
                    .size(14.5)
                    .color(t.text);
                w::dropdown(ui, "font-picker", label, 240.0, |ui| {
                    for f in theme::FONTS {
                        let mut text = egui::RichText::new(f.name)
                            .family(theme::preview_family(f.name))
                            .size(15.0);
                        if f.name == current {
                            text = text.color(t.accent);
                        }
                        let resp = w::option(ui, f.name == current, text);
                        let resp = if f.mono {
                            resp.on_hover_text("Monospaced — every character the same width")
                        } else {
                            resp
                        };
                        if resp.clicked() && f.name != current {
                            acts.push(Act::Font(f.name));
                        }
                    }
                });
                if theme::font_by_name(&current).mono {
                    w::chip(ui, &t, None, "monospace", t.text2);
                }
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("Groceries  ·  $1,284.50  ·  Sep 30")
                        .font(theme::regular(14.0))
                        .color(t.text2),
                );
            });
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("Interface size")
                    .font(theme::semibold(14.0))
                    .color(t.text),
            );
            ui.label(w::subtle(
                &t,
                concat!(
                    "Scales text and everything else together. Also ",
                    shortcut!("+"),
                    " / ",
                    shortcut!("−"),
                    " / ",
                    shortcut!("0"),
                    "."
                ),
            ));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                const PRESETS: [(f32, &str); 6] = [
                    (0.85, "Compact"),
                    (1.0, "Default"),
                    (1.1, "Comfortable"),
                    (1.25, "Large"),
                    (1.4, "Larger"),
                    (1.6, "Huge"),
                ];
                let labels: Vec<&str> = PRESETS.iter().map(|p| p.1).collect();
                let mut sel = PRESETS
                    .iter()
                    .position(|p| (p.0 - zoom).abs() < 0.02)
                    .unwrap_or(usize::MAX);
                let before = sel;
                w::segmented(ui, &t, egui::Id::new("ui-scale"), &mut sel, &labels);
                if sel != before && sel < PRESETS.len() {
                    acts.push(Act::Scale(PRESETS[sel].0));
                }
                ui.add_space(8.0);
                if w::icon_button(ui, &t, ph::MINUS, "Smaller").clicked() {
                    acts.push(Act::Scale(zoom - 0.05));
                }
                ui.label(
                    egui::RichText::new(format!("{:.0}%", zoom * 100.0))
                        .font(theme::semibold(13.5))
                        .color(t.text),
                );
                if w::icon_button(ui, &t, ph::PLUS, "Bigger").clicked() {
                    acts.push(Act::Scale(zoom + 0.05));
                }
            });
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("Scroll speed")
                    .font(theme::semibold(14.0))
                    .color(t.text),
            );
            ui.label(w::subtle(
                &t,
                "How far each notch of the scroll wheel (or touchpad swipe) moves.",
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(w::faint(&t, "Slower"));
                let mut v = w::scroll_speed();
                if w::slider(ui, &t, egui::Id::new("scroll-speed"), &mut v, 0.25..=3.0, &[1.0], 320.0).changed() {
                    acts.push(Act::ScrollSpeed(v));
                }
                ui.label(w::faint(&t, "Faster"));
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("{:.2}×", w::scroll_speed()))
                        .font(theme::semibold(13.5))
                        .color(t.text),
                );
                if (w::scroll_speed() - 1.0).abs() > 0.01 && w::ghost(ui, &t, None, "Reset").clicked() {
                    acts.push(Act::ScrollSpeed(1.0));
                }
            });
        },
    );

    let store = &app.store;
    let s = store.settings().clone();
    section(
        ui,
        &t,
        "Currency & exchange rates",
        "Totals, budgets and reports use your main currency. Accounts can each use their own.",
        |ui| {
            ui.horizontal(|ui| {
                w::field_label(ui, &t, "Main currency");
                let mut base = s.base;
                forms::currency_picker(ui, "set-base", &mut base, 260.0);
                if base != s.base {
                    acts.push(Act::Base(base));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let mut auto = s.fx_auto;
                if w::toggle(ui, &t, &mut auto).changed() {
                    acts.push(Act::FxAuto(auto));
                }
                ui.label(egui::RichText::new("Update rates daily from the European Central Bank").color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if app.fx_busy { "Updating…" } else { "Refresh now" };
                    if w::secondary(ui, &t, Some(ph::ARROWS_CLOCKWISE), label).clicked() && !app.fx_busy {
                        acts.push(Act::RefreshFx);
                    }
                });
            });
            ui.label(w::faint(
                &t,
                match &s.fx_updated {
                    Some(d) => format!("Last updated {d} · only fetched when you use more than one currency"),
                    None => "Using built-in estimates until the first refresh".into(),
                },
            ));
            // Rates for currencies actually in use.
            let mut used: Vec<Cur> = store
                .accounts()
                .iter()
                .map(|a| a.currency)
                .chain(store.goals().iter().map(|g| g.currency))
                .filter(|c| *c != s.base)
                .collect();
            used.sort();
            used.dedup();
            if !used.is_empty() {
                ui.add_space(10.0);
                egui::Grid::new("rates")
                    .num_columns(3)
                    .spacing(vec2(18.0, 8.0))
                    .show(ui, |ui| {
                        for c in used {
                            let rate = store.rates.rate(c, s.base).unwrap_or(0.0);
                            ui.label(egui::RichText::new(format!("1 {c}")).color(t.text2));
                            let editing = app.settings.rate_edit.as_ref().is_some_and(|(x, _)| *x == c);
                            if editing {
                                let (_, buf) = app.settings.rate_edit.as_mut().expect("editing");
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(buf).desired_width(100.0));
                                    ui.label(w::subtle(&t, s.base.code()));
                                    if w::primary(ui, &t, None, "Set").clicked() {
                                        match buf.trim().parse::<f64>() {
                                            Ok(v) if v > 0.0 => {
                                                // per_eur(c) = per_eur(base) / v
                                                let per_eur_base = store.rates.get(s.base).unwrap_or(1.0);
                                                acts.push(Act::SetRate(c, per_eur_base / v));
                                            }
                                            _ => {}
                                        }
                                    }
                                });
                            } else {
                                ui.label(
                                    egui::RichText::new(format!("= {} {}", fmt_rate(rate), s.base))
                                        .font(theme::medium(13.5))
                                        .color(t.text),
                                );
                            }
                            ui.horizontal(|ui| {
                                if store.rates.is_manual(c) {
                                    w::chip(ui, &t, None, "manual", t.warn);
                                    if w::ghost(ui, &t, None, "Use ECB").clicked() {
                                        acts.push(Act::ResetRate(c));
                                    }
                                } else if !editing && w::ghost(ui, &t, Some(ph::PENCIL_SIMPLE), "Override").clicked() {
                                    app.settings.rate_edit = Some((c, fmt_rate(rate)));
                                }
                            });
                            ui.end_row();
                        }
                    });
            }
        },
    );

    section(
        ui,
        &t,
        "Categories",
        "Colours and icons show up everywhere — charts, budgets and the ledger.",
        |ui| {
            let wdt = (ui.available_width() - 24.0 - 2.0 * ui.spacing().item_spacing.x) / 2.0;
            ui.horizontal_top(|ui| {
                for kind in [CategoryKind::Expense, CategoryKind::Income] {
                    ui.allocate_ui_with_layout(vec2(wdt, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(w::faint(
                                &t,
                                if kind == CategoryKind::Expense {
                                    "EXPENSES"
                                } else {
                                    "INCOME"
                                },
                            ));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if w::ghost(ui, &t, Some(ph::PLUS), "Add").clicked() {
                                    acts.push(Act::NewCat(kind));
                                }
                            });
                        });
                        for c in store.categories().iter().filter(|c| c.kind == kind) {
                            let (r, resp) = ui.allocate_exact_size(vec2(wdt, 38.0), Sense::click());
                            let h = motion::toggle(ui.ctx(), Id::new(("cat-set", c.id)), resp.hovered(), motion::MICRO);
                            let p = ui.painter();
                            if h > 0.0 {
                                p.rect_filled(r, CornerRadius::same(8), motion::with_alpha(t.hover, h));
                            }
                            let badge =
                                Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 13.0), vec2(26.0, 26.0));
                            w::paint_icon_badge(p, &t, badge, icons::glyph(&c.icon), w::cat_color(c.color));
                            let tc = if c.archived { t.text3 } else { t.text };
                            p.text(
                                pos2(badge.right() + 10.0, r.center().y),
                                Align2::LEFT_CENTER,
                                &c.name,
                                theme::medium(13.0),
                                tc,
                            );
                            if c.archived {
                                p.text(
                                    pos2(r.right() - 30.0, r.center().y),
                                    Align2::RIGHT_CENTER,
                                    "archived",
                                    theme::regular(11.0),
                                    t.text3,
                                );
                            }
                            p.text(
                                pos2(r.right() - 8.0, r.center().y),
                                Align2::RIGHT_CENTER,
                                ph::PENCIL_SIMPLE,
                                theme::regular(14.0),
                                motion::with_alpha(t.text2, h),
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                acts.push(Act::EditCat(c.id));
                            }
                        }
                    });
                    if kind == CategoryKind::Expense {
                        ui.add_space(24.0);
                    }
                }
            });
        },
    );

    let has_pear = magpie_core::io::pear_path().is_some();
    section(
        ui,
        &t,
        "Your data",
        "Everything lives in a single SQLite file on this computer. Exports go to your Downloads folder.",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                if w::secondary(ui, &t, Some(ph::UPLOAD_SIMPLE), "Import bank statement").clicked() {
                    acts.push(Act::ImportCsv);
                }
                if has_pear && w::secondary(ui, &t, Some(ph::UPLOAD_SIMPLE), "Import from Pear").clicked() {
                    acts.push(Act::ImportPear);
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if w::secondary(ui, &t, Some(ph::FILE_CSV), "Export CSV").clicked() {
                    acts.push(Act::Export(Format::Csv));
                }
                if w::secondary(ui, &t, Some(ph::FILE_XLS), "Export Excel").clicked() {
                    acts.push(Act::Export(Format::Xlsx));
                }
                if w::secondary(ui, &t, Some(ph::BRACKETS_CURLY), "Export JSON").clicked() {
                    acts.push(Act::Export(Format::Json));
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if w::secondary(ui, &t, Some(ph::FLOPPY_DISK), "Back up database…").clicked() {
                    acts.push(Act::Backup);
                }
                if w::ghost(ui, &t, Some(ph::FOLDER_OPEN), "Open data folder").clicked() {
                    acts.push(Act::OpenFolder);
                }
                if store.txns().is_empty() && w::ghost(ui, &t, Some(ph::SPARKLE), "Load demo data").clicked() {
                    acts.push(Act::Demo);
                }
            });
            if let Some(dir) = store.dir() {
                ui.add_space(6.0);
                ui.label(w::faint(
                    &t,
                    format!("{}  {}", ph::DATABASE, dir.join("magpie.db").display()),
                ));
            }
        },
    );

    section(ui, &t, "Keyboard", "Press ? anywhere to see this list.", |ui| {
        forms::shortcut_table(ui, &t, 2);
    });

    section(ui, &t, "About", "", |ui| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            crate::app::logo(ui.painter(), r, &t);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(format!("Magpie {}", env!("CARGO_PKG_VERSION"))).font(theme::semibold(14.0)).color(t.text));
                ui.label(w::subtle(&t, "Local-first personal finance. MIT licensed. Inter font by Rasmus Andersson (OFL), icons by Phosphor (MIT)."));
            });
        });
    });

    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::Theme(name) => app.set_theme(&ctx, name),
            Act::Base(c) => app.open_modal(&ctx, Modal::Currency(forms::CurrencyForm::new(c))),
            Act::Scale(z) => app.set_ui_scale(&ctx, z),
            Act::Font(name) => app.set_font(&ctx, name),
            Act::ScrollSpeed(v) => app.set_scroll_speed(v),
            Act::FxAuto(v) => {
                app.toasts.ok(app.store.update_settings(|s| s.fx_auto = v));
            }
            Act::RefreshFx => app.maybe_refresh_fx(&ctx, true),
            Act::SetRate(c, per_eur) => {
                if app.toasts.ok(app.store.set_rate(c, per_eur, true)).is_some() {
                    app.settings.rate_edit = None;
                    app.toasts.success(format!("{c} rate set manually"));
                }
            }
            Act::ResetRate(c) => {
                // Clear the manual flag; the next refresh fills in ECB's rate.
                let v = app.store.rates.get(c).unwrap_or(1.0);
                app.toasts.ok(app.store.set_rate(c, v, false));
                app.maybe_refresh_fx(&ctx, true);
            }
            Act::EditCat(id) => {
                if let Some(c) = app.store.category(id) {
                    let f = forms::CategoryForm::edit(c);
                    app.open_modal(&ctx, Modal::Category(f));
                }
            }
            Act::NewCat(k) => {
                let f = forms::CategoryForm::new(&app.store, k);
                app.open_modal(&ctx, Modal::Category(f));
            }
            Act::ImportCsv => {
                app.dialogs.pick(
                    &ctx,
                    crate::dialogs::Purpose::ImportCsv,
                    "Import transactions",
                    ("Bank statements", magpie_core::statement::EXTENSIONS),
                );
            }
            Act::ImportPear => {
                if let (Some(path), Some(acc)) = (magpie_core::io::pear_path(), app.store.default_account()) {
                    let r = magpie_core::io::import_pear(&mut app.store, &path, acc);
                    if let Some(n) = app.toasts.ok(r) {
                        app.toasts.undoable(format!("Imported {n} transactions from Pear"));
                    }
                }
            }
            Act::Export(f) => export_all(app, f),
            Act::Backup => {
                let name = format!("magpie-backup-{}.db", app.today);
                app.dialogs.save(
                    &ctx,
                    crate::dialogs::Purpose::Backup,
                    &name,
                    ("SQLite database", &["db"]),
                );
            }
            Act::OpenFolder => {
                if let Some(dir) = app.store.dir() {
                    crate::app::open_external(dir);
                }
            }
            Act::Demo => {
                app.open_modal(
                    &ctx,
                    Modal::Confirm(forms::Confirm {
                        title: "Load demo data?".into(),
                        body: "Adds two years of sample accounts, transactions, budgets and goals so you can explore."
                            .into(),
                        action: ConfirmAction::LoadDemo,
                        danger: false,
                    }),
                );
            }
        }
    }
}

fn fmt_rate(r: f64) -> String {
    if r >= 100.0 {
        format!("{r:.2}")
    } else if r >= 1.0 {
        format!("{r:.4}")
    } else {
        format!("{r:.6}")
    }
}

/// A miniature rendering of a theme.
fn theme_tile(ui: &mut Ui, t: &Theme, th: &Theme, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(168.0, 112.0), Sense::click());
    let h = motion::toggle(
        ui.ctx(),
        Id::new(("theme-tile", th.name)),
        resp.hovered(),
        motion::MICRO,
    );
    let s = motion::toggle(ui.ctx(), Id::new(("theme-sel", th.name)), selected, motion::STANDARD);
    let r = rect.shrink(3.0 - 2.0 * h);
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(12), th.bg);
    let side = Rect::from_min_size(r.min, vec2(34.0, r.height()));
    p.rect_filled(
        side,
        CornerRadius {
            nw: 12,
            sw: 12,
            ne: 0,
            se: 0,
        },
        th.sidebar,
    );
    for i in 0..4 {
        let y = r.top() + 16.0 + i as f32 * 12.0;
        let c = if i == 0 { th.accent } else { th.text3 };
        p.rect_filled(
            Rect::from_min_size(pos2(side.left() + 8.0, y), vec2(18.0, 4.0)),
            CornerRadius::same(2),
            c,
        );
    }
    let card = Rect::from_min_size(pos2(side.right() + 10.0, r.top() + 12.0), vec2(r.width() - 54.0, 44.0));
    p.rect(
        card,
        CornerRadius::same(7),
        th.card,
        Stroke::new(1.0, th.border),
        egui::StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_min_size(card.min + vec2(8.0, 8.0), vec2(40.0, 5.0)),
        CornerRadius::same(2),
        th.text2,
    );
    p.rect_filled(
        Rect::from_min_size(card.min + vec2(8.0, 18.0), vec2(64.0, 9.0)),
        CornerRadius::same(3),
        th.text,
    );
    let bars = [0.5, 0.8, 0.35, 0.65];
    for (i, b) in bars.iter().enumerate() {
        let x = card.right() - 44.0 + i as f32 * 9.0;
        let hgt = 26.0 * b;
        let c = if i % 2 == 0 { th.accent } else { th.pos };
        p.rect_filled(
            Rect::from_min_max(pos2(x, card.bottom() - 8.0 - hgt), pos2(x + 6.0, card.bottom() - 8.0)),
            CornerRadius::same(2),
            c,
        );
    }
    p.text(
        pos2(side.right() + 10.0, r.bottom() - 16.0),
        Align2::LEFT_CENTER,
        th.name,
        theme::semibold(12.5),
        th.text,
    );
    let ring = motion::lerp_color(t.border, t.accent, s);
    p.rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0 + 1.5 * s, ring),
        egui::StrokeKind::Inside,
    );
    if s > 0.01 {
        let c = pos2(r.right() - 14.0, r.bottom() - 16.0);
        p.circle_filled(c, 8.0 * s, t.accent);
        p.text(
            c,
            Align2::CENTER_CENTER,
            ph::CHECK,
            theme::regular(10.0 * s.max(0.1)),
            t.on_accent,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
