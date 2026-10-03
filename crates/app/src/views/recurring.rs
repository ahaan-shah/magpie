//! Recurring bills, subscriptions and income, with a 30-day timeline.

use crate::app::App;
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use magpie_core::{Id as RowId, RecurringRule, Store, recurring};

enum Act {
    Edit(RowId),
    New,
    Post(RowId),
    Skip(RowId),
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let store = &app.store;
    let base = store.base();
    let shown = app.shown_at;
    let mut acts = Vec::new();

    ui.horizontal(|ui| {
        ui.label(w::subtle(
            &t,
            "Bills, subscriptions and income that repeat. Magpie posts them for you.",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::primary(ui, &t, Some(ph::PLUS), "New recurring")
                .on_hover_text(concat!("New recurring (", shortcut!("R"), ")"))
                .clicked()
            {
                acts.push(Act::New);
            }
        });
    });
    ui.add_space(12.0);

    let active: Vec<&RecurringRule> = store.rules().iter().filter(|r| r.active).collect();
    let monthly = |r: &&RecurringRule| store.to_base(recurring::monthly_equivalent(r), store.account_cur(r.account));
    let bills: i64 = -active.iter().filter(|r| r.amount < 0).map(monthly).sum::<i64>();
    let income: i64 = active.iter().filter(|r| r.amount > 0).map(monthly).sum();
    w::grid_row(ui, 112.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, i, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| match i {
                0 => {
                    ui.label(w::faint(&t, "Bills & subscriptions"));
                    w::animated_amount(ui, Id::new("rec-bills"), bills, base, theme::display(24.0), t.text);
                    ui.label(w::faint(
                        &t,
                        format!("per month · {} a year", w::fmt_whole(bills * 12, base)),
                    ));
                }
                1 => {
                    ui.label(w::faint(&t, "Recurring income"));
                    w::animated_amount(ui, Id::new("rec-income"), income, base, theme::display(24.0), t.pos);
                    ui.label(w::faint(&t, "per month"));
                }
                _ => {
                    ui.label(w::faint(&t, "Active rules"));
                    ui.label(
                        egui::RichText::new(active.len().to_string())
                            .font(theme::display(24.0))
                            .color(t.text),
                    );
                    let paused = store.rules().len() - active.len();
                    ui.label(w::faint(
                        &t,
                        if paused > 0 {
                            format!("{paused} paused")
                        } else {
                            "all running".into()
                        },
                    ));
                }
            })
        })
    });

    let waiting = recurring::awaiting_confirmation(store, today);
    if !waiting.is_empty() {
        w::card_frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            w::card_header(ui, &t, "Waiting for you", |_| {});
            for (id, date) in &waiting {
                let Some(r) = store.rule(*id) else { continue };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(ph::CLOCK).color(t.warn));
                    ui.label(egui::RichText::new(&r.payee).font(theme::medium(13.5)).color(t.text));
                    ui.label(w::subtle(
                        &t,
                        format!(
                            "{} · due {}",
                            w::fmt_money(r.amount.abs(), store.account_cur(r.account)),
                            w::day_label(*date, today)
                        ),
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::primary(ui, &t, Some(ph::CHECK), "Post").clicked() {
                            acts.push(Act::Post(*id));
                        }
                        if w::ghost(ui, &t, Some(ph::SKIP_FORWARD), "Skip").clicked() {
                            acts.push(Act::Skip(*id));
                        }
                    });
                });
            }
        });
        ui.add_space(theme::GAP);
    }

    let mut rules: Vec<&RecurringRule> = store.rules().iter().collect();
    rules.sort_by_key(|r| (!r.active, recurring::next_due(r)));
    let list_h = (84.0 + rules.len().max(2) as f32 * 66.0).max(360.0);
    // Keyboard: ↑ ↓ select a rule, Enter / E edit it.
    let sel_id = egui::Id::new("recurring-sel");
    let mut sel: Option<usize> = ui.data(|d| d.get_temp(sel_id));
    let keys = app.keys;
    let mut moved = false;
    if !rules.is_empty() {
        let last = rules.len() - 1;
        if keys.down {
            sel = Some(sel.map_or(0, |i| (i + 1).min(last)));
            moved = true;
        }
        if keys.up {
            sel = Some(sel.map_or(0, |i| i.saturating_sub(1)));
            moved = true;
        }
        if (keys.enter || keys.edit)
            && let Some(r) = sel.and_then(|i| rules.get(i))
        {
            acts.push(Act::Edit(r.id));
        }
    }
    ui.data_mut(|d| d.insert_temp(sel_id, sel));
    w::grid_row(ui, list_h, &[1.5, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 3 + i, rect, |ui, rect| {
            w::card_scroll(ui, &t, ("recurring-lists", i), rect, |ui| {
                if i == 0 {
                    w::card_header(ui, &t, "All recurring", |_| {});
                    if rules.is_empty() {
                        w::empty_state(
                            ui,
                            &t,
                            ph::ARROWS_CLOCKWISE,
                            "Nothing recurring yet",
                            "Add rent, salary or subscriptions and Magpie will post them for you.",
                        );
                    }
                    for (i, r) in rules.iter().enumerate() {
                        let resp = rule_row(ui, &t, store, r, today, sel == Some(i));
                        if sel == Some(i) && moved {
                            ui.scroll_to_rect(resp.rect, None);
                        }
                        if resp.clicked() {
                            acts.push(Act::Edit(r.id));
                        }
                    }
                } else {
                    timeline(ui, &t, store, today);
                }
            })
        })
    });

    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::Edit(id) => {
                if let Some(r) = app.store.rule(id).cloned() {
                    let f = forms::RuleForm::edit(&app.store, &r);
                    app.open_modal(&ctx, Modal::Rule(f));
                }
            }
            Act::New => {
                let f = forms::RuleForm::new(&app.store, app.today);
                app.open_modal(&ctx, Modal::Rule(f));
            }
            Act::Post(id) => {
                if app.toasts.ok(recurring::post_next(&mut app.store, id)).is_some() {
                    app.toasts.undoable("Posted");
                }
            }
            Act::Skip(id) => {
                if app.toasts.ok(recurring::skip_next(&mut app.store, id)).is_some() {
                    app.toasts.info("Skipped");
                }
            }
        }
    }
}

fn rule_row(
    ui: &mut Ui,
    t: &Theme,
    store: &Store,
    r: &RecurringRule,
    today: jiff::civil::Date,
    selected: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
    let h = motion::toggle(
        ui.ctx(),
        Id::new(("rule-row", r.id)),
        resp.hovered() || selected,
        motion::MICRO,
    );
    let p = ui.painter();
    if selected {
        p.rect_stroke(
            rect.expand2(vec2(8.0, 0.0)),
            CornerRadius::same(10),
            egui::Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    if h > 0.0 {
        p.rect_filled(
            rect.expand2(vec2(8.0, 0.0)),
            CornerRadius::same(10),
            motion::with_alpha(t.hover_wash(), h),
        );
    }
    let alpha = if r.active { 1.0 } else { 0.45 };
    let a = |c: egui::Color32| motion::with_alpha(c, alpha);
    let cat = r.category.and_then(|c| store.category(c));
    let color = cat.map(|c| w::cat_color(c.color)).unwrap_or(t.accent);
    let badge = Rect::from_min_size(pos2(rect.left(), rect.center().y - 18.0), vec2(36.0, 36.0));
    w::paint_icon_badge(
        p,
        t,
        badge,
        cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::REPEAT),
        color,
    );
    let x = badge.right() + 12.0;
    let max_w = rect.right() - x - 130.0;
    w::text_fit(
        p,
        pos2(x, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        &r.payee,
        theme::medium(13.5),
        a(t.text),
        max_w,
    );
    let mut sub = format!("{} · {}", recurring::describe(r), store.account_name(r.account));
    if !r.auto_post {
        sub.push_str(" · asks first");
    }
    if !r.active {
        sub.push_str(" · paused");
    }
    w::text_fit(
        p,
        pos2(x, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        sub,
        theme::regular(11.5),
        a(t.text3),
        max_w,
    );
    let cur = store.account_cur(r.account);
    let c = if r.amount > 0 { t.pos } else { t.text };
    p.text(
        pos2(rect.right(), rect.center().y - 9.0),
        Align2::RIGHT_CENTER,
        w::fmt_signed(r.amount, cur),
        theme::semibold(13.5),
        a(c),
    );
    if let Some(next) = recurring::next_due(r) {
        let soon = (next - today).get_days() <= 3;
        p.text(
            pos2(rect.right(), rect.center().y + 9.0),
            Align2::RIGHT_CENTER,
            format!("next {}", w::in_days(next, today)),
            theme::regular(11.5),
            if soon { t.warn } else { t.text3 },
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn timeline(ui: &mut Ui, t: &Theme, store: &Store, today: jiff::civil::Date) {
    w::card_header(ui, t, "Next 30 days", |_| {});
    let to = today.checked_add(jiff::Span::new().days(30)).unwrap_or(today);
    let items = recurring::upcoming(store, today, to);
    if items.is_empty() {
        w::empty_state(
            ui,
            t,
            ph::CALENDAR_CHECK,
            "All quiet",
            "Nothing scheduled in the next month.",
        );
        return;
    }
    let base = store.base();
    let mut running = 0i64;
    let mut last_date = None;
    for (id, date) in items.iter().take(14) {
        let Some(r) = store.rule(*id) else { continue };
        if last_date != Some(*date) {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(w::day_label(*date, today).to_uppercase())
                    .font(theme::semibold(10.5))
                    .color(t.text3),
            );
            last_date = Some(*date);
        }
        running += store.to_base(r.amount, store.account_cur(r.account));
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 3.5, if r.amount > 0 { t.pos } else { t.accent });
            ui.label(egui::RichText::new(&r.payee).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(w::fmt_signed(r.amount, store.account_cur(r.account)))
                        .font(theme::medium(12.5))
                        .color(if r.amount > 0 { t.pos } else { t.text2 }),
                );
            });
        });
    }
    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(w::faint(t, "Net over this period"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(w::fmt_signed(running, base))
                    .font(theme::semibold(13.0))
                    .color(if running >= 0 { t.pos } else { t.text }),
            );
        });
    });
}
