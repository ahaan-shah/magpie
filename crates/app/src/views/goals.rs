//! Savings goals with animated progress rings and projections.

use crate::app::{App, Memo};
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets::{self as w, charts};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::goals::{self, GoalStatus};
use magpie_core::{Goal, Id as RowId, Store};
use std::rc::Rc;

type Goals = Rc<Vec<(Goal, GoalStatus)>>;

#[derive(Default)]
pub struct State {
    show_archived: bool,
    sel: Option<usize>,
    memo: Memo<(u64, Date), Goals>,
}

enum Act {
    New,
    Edit(RowId),
    Add(RowId),
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let all = app
        .goals
        .memo
        .get((app.store.version(), today), || {
            Rc::new(
                app.store
                    .goals()
                    .iter()
                    .map(|g| (g.clone(), goals::status(&app.store, g, today)))
                    .collect(),
            )
        })
        .clone();
    let shown = app.shown_at;
    let mut acts = Vec::new();
    let store = &app.store;
    let base = store.base();

    let active: Vec<&(Goal, GoalStatus)> = all.iter().filter(|(g, _)| !g.archived).collect();
    let saved: i64 = active
        .iter()
        .map(|(g, s)| store.convert(s.saved, g.currency, base))
        .sum();
    let target: i64 = active
        .iter()
        .map(|(g, _)| store.convert(g.target, g.currency, base))
        .sum();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(w::faint(&t, "Saved towards goals"));
            ui.horizontal(|ui| {
                w::animated_amount(ui, Id::new("goals-saved"), saved, base, theme::display(28.0), t.text);
                ui.label(w::subtle(&t, format!("of {}", w::fmt_whole(target, base))));
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::primary(ui, &t, Some(ph::PLUS), "New goal").clicked() {
                acts.push(Act::New);
            }
        });
    });
    ui.add_space(14.0);

    if active.is_empty() {
        w::card_frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            w::empty_state(
                ui,
                &t,
                ph::TARGET,
                "Dream a little",
                "A trip, a rainy-day fund, a new laptop — set a target and watch it fill up.",
            );
            ui.vertical_centered(|ui| {
                if w::primary(ui, &t, Some(ph::PLUS), "Create your first goal").clicked() {
                    acts.push(Act::New);
                }
            });
        });
    }
    // Keyboard: arrows move across the grid, Enter adds money (or edits an
    // account-linked goal), E edits.
    let keys = app.keys;
    let mut moved = false;
    if !active.is_empty() {
        let last = active.len() - 1;
        let cur = app.goals.sel;
        let next = if keys.right {
            Some(cur.map_or(0, |i| (i + 1).min(last)))
        } else if keys.left {
            Some(cur.map_or(0, |i| i.saturating_sub(1)))
        } else if keys.down {
            Some(cur.map_or(0, |i| (i + 3).min(last)))
        } else if keys.up {
            Some(cur.map_or(0, |i| i.saturating_sub(3)))
        } else {
            None
        };
        if next.is_some() {
            app.goals.sel = next;
            moved = true;
        }
        if let Some((g, _)) = app.goals.sel.and_then(|i| active.get(i)) {
            if keys.enter {
                acts.push(if g.account.is_none() {
                    Act::Add(g.id)
                } else {
                    Act::Edit(g.id)
                });
            }
            if keys.edit {
                acts.push(Act::Edit(g.id));
            }
        }
    }
    let sel = app.goals.sel;
    for (row, chunk) in active.chunks(3).enumerate() {
        w::grid_row(ui, 250.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
            if let Some((g, s)) = chunk.get(i) {
                let selected = sel == Some(row * 3 + i);
                if selected && moved {
                    ui.scroll_to_rect(rect, None);
                }
                w::with_reveal(ui, shown, row * 3 + i, rect, |ui, rect| {
                    goal_card(ui, &t, store, g, s, today, rect, &mut acts, selected)
                });
            }
        });
    }
    let archived: Vec<&(Goal, GoalStatus)> = all.iter().filter(|(g, _)| g.archived).collect();
    if !archived.is_empty() {
        w::toggle_row(
            ui,
            &t,
            &mut app.goals.show_archived,
            &format!("Show {} archived", archived.len()),
        );
        if app.goals.show_archived {
            ui.add_space(8.0);
            for chunk in archived.chunks(3) {
                w::grid_row(ui, 250.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
                    if let Some((g, s)) = chunk.get(i) {
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                        c.set_opacity(0.55);
                        goal_card(&mut c, &t, store, g, s, today, rect, &mut acts, false);
                    }
                });
            }
        }
    }

    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::New => {
                let f = forms::GoalForm::new(&app.store, app.today);
                app.open_modal(&ctx, Modal::Goal(f));
            }
            Act::Edit(id) => {
                if let Some(g) = app.store.goal(id) {
                    let f = forms::GoalForm::edit(g, app.today);
                    app.open_modal(&ctx, Modal::Goal(f));
                }
            }
            Act::Add(id) => {
                let f = forms::ContribForm::new(id, app.today);
                app.open_modal(&ctx, Modal::Contribution(f));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn goal_card(
    ui: &mut Ui,
    t: &Theme,
    store: &Store,
    g: &Goal,
    s: &GoalStatus,
    today: Date,
    rect: Rect,
    acts: &mut Vec<Act>,
    selected: bool,
) {
    let color = w::cat_color(g.color);
    let resp = ui.interact(rect, Id::new(("goal-card", g.id)), Sense::hover());
    let h = motion::toggle(
        ui.ctx(),
        Id::new(("goal-h", g.id)),
        resp.hovered() || selected,
        motion::MICRO,
    );
    let rect = rect.translate(vec2(0.0, -2.0 * h));
    w::card_in(ui, t, rect, |ui| {
        if h > 0.0 {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(theme::RADIUS),
                Stroke::new(1.0, motion::with_alpha(color, 0.6 * h)),
                egui::StrokeKind::Inside,
            );
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&g.name).font(theme::semibold(15.0)).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::icon_button(ui, t, ph::PENCIL_SIMPLE, "Edit goal").clicked() {
                    acts.push(Act::Edit(g.id));
                }
            });
        });
        let center = pos2(rect.left() + 78.0, rect.top() + 118.0);
        charts::ring(
            ui,
            Id::new(("goal-ring", g.id)),
            center,
            50.0,
            10.0,
            s.fraction,
            color,
            t.hover,
        );
        let p = ui.painter();
        let done = s.fraction >= 1.0;
        p.text(
            center - vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            if done { ph::TROPHY } else { icons::glyph(&g.icon) },
            theme::regular(20.0),
            w::readable(t, color),
        );
        p.text(
            center + vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            format!("{:.0}%", s.fraction * 100.0),
            theme::display(16.0),
            t.text,
        );

        let x = rect.left() + 150.0;
        let mut y = rect.top() + 72.0;
        let mut line = |label: &str, value: String, c: egui::Color32| {
            p.text(pos2(x, y), Align2::LEFT_TOP, label, theme::regular(11.5), t.text3);
            p.text(pos2(x, y + 15.0), Align2::LEFT_TOP, value, theme::semibold(13.5), c);
            y += 40.0;
        };
        line(
            "Saved",
            format!(
                "{} / {}",
                w::fmt_whole(s.saved, g.currency),
                w::fmt_whole(g.target, g.currency)
            ),
            t.text,
        );
        match (g.deadline, s.needed_per_month) {
            (Some(dl), Some(per)) => line(
                &format!("By {}", w::fmt_date(dl)),
                format!("{} / month needed", w::fmt_whole(per, g.currency)),
                t.text,
            ),
            (Some(dl), None) => line("Deadline", w::fmt_date(dl), t.text),
            _ => line("Deadline", "Whenever".into(), t.text2),
        }
        let (proj, pc) = if done {
            ("Reached — nice work!".to_string(), t.pos)
        } else {
            match s.projected {
                Some(d) => (
                    format!("{} ({})", w::fmt_date(d), w::in_days(d, today)),
                    if s.on_track == Some(false) { t.warn } else { t.text },
                ),
                None => ("Add money to see a forecast".into(), t.text3),
            }
        };
        line("On current pace", proj, pc);

        let btn = Rect::from_min_size(
            pos2(rect.left() + 20.0, rect.bottom() - 52.0),
            vec2(rect.width() - 150.0, 34.0),
        );
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(btn));
        if g.account.is_none() {
            if w::secondary(&mut c, t, Some(ph::PLUS), "Add money").clicked() {
                acts.push(Act::Add(g.id));
            }
        } else {
            c.label(w::faint(
                t,
                format!("{} Tracks {}", ph::WALLET, store.account_name(g.account.unwrap_or(0))),
            ));
        }
        if let Some(true) = s.on_track {
            let chip = Rect::from_min_size(pos2(rect.right() - 110.0, rect.bottom() - 48.0), vec2(90.0, 26.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(chip));
            w::chip(&mut c, t, Some(ph::CHECK), "On track", t.pos);
        } else if let Some(false) = s.on_track {
            let chip = Rect::from_min_size(pos2(rect.right() - 110.0, rect.bottom() - 48.0), vec2(90.0, 26.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(chip));
            w::chip(&mut c, t, Some(ph::WARNING), "Behind", t.warn);
        }
    });
}
