//! Monthly envelope budgets with rollover and a pacing line.

use crate::app::{App, Memo};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::budget::{self, BudgetLine};
use magpie_core::money;
use magpie_core::{BudgetPlan, CategoryKind, Id as RowId, Month, Store, analytics};
use std::rc::Rc;

pub struct State {
    month: Month,
    open: Option<RowId>,
    amount: String,
    this_month_only: bool,
    /// Keyboard-selected row.
    sel: Option<usize>,
    memo: Memo<(u64, Month, Date), Rc<Data>>,
}

impl State {
    /// Opens the inline editor for a category (e.g. right after creating it).
    pub fn edit(&mut self, category: RowId, amount: String) {
        self.open = Some(category);
        self.amount = amount;
        self.this_month_only = false;
    }

    pub fn new(today: Date) -> State {
        State {
            month: Month::of(today),
            open: None,
            amount: String::new(),
            this_month_only: false,
            sel: None,
            memo: Memo::default(),
        }
    }
}

struct Data {
    lines: Vec<BudgetLine>,
    unbudgeted: Vec<(RowId, i64)>,
    averages: Vec<(RowId, i64)>,
}

fn build(store: &Store, m: Month) -> Data {
    let lines = budget::month_budget(store, m);
    let spent = analytics::category_spent_map(store, m);
    let mut unbudgeted: Vec<(RowId, i64)> = spent
        .into_iter()
        .filter(|(c, v)| {
            *v > 0
                && store.budget_plan(*c).is_none()
                && store.category(*c).is_some_and(|x| x.kind == CategoryKind::Expense)
        })
        .collect();
    unbudgeted.sort_by_key(|x| std::cmp::Reverse(x.1));
    let averages = store
        .categories()
        .iter()
        .filter(|c| c.kind == CategoryKind::Expense)
        .map(|c| (c.id, budget::average_spend(store, c.id, m, 3)))
        .collect();
    Data {
        lines,
        unbudgeted,
        averages,
    }
}

enum Act {
    Open(RowId),
    Close,
    Save(RowId, i64, bool, bool),
    Remove(RowId),
    Add(RowId, i64),
    FillAverages,
    NewCategory,
    UndoFit,
}

const FIT_LABEL: &str = "Fit budgets to average";

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let base = app.store.base();
    let m = app.budgets.month;
    let d = app
        .budgets
        .memo
        .get((app.store.version(), m, today), || Rc::new(build(&app.store, m)))
        .clone();
    let pace = budget::month_progress(m, today);
    let shown = app.shown_at;
    let mut acts = Vec::new();

    // Keyboard: ← → months, ↑ ↓ rows, Enter/E edit.
    let keys = app.keys;
    if keys.left {
        app.budgets.month = m.prev();
    }
    if keys.right {
        app.budgets.month = m.next();
    }
    let mut moved = false;
    if !d.lines.is_empty() {
        let last = d.lines.len() - 1;
        if keys.down {
            app.budgets.sel = Some(app.budgets.sel.map_or(0, |i| (i + 1).min(last)));
            moved = true;
        }
        if keys.up {
            app.budgets.sel = Some(app.budgets.sel.map_or(0, |i| i.saturating_sub(1)));
            moved = true;
        }
        if (keys.enter || keys.edit)
            && let Some(l) = app.budgets.sel.and_then(|i| d.lines.get(i))
        {
            acts.push(Act::Open(l.category));
        }
    }

    // Month switcher + actions
    ui.horizontal(|ui| {
        if w::icon_button(ui, &t, ph::CARET_LEFT, "Previous month").clicked() {
            app.budgets.month = m.prev();
        }
        ui.label(egui::RichText::new(m.label()).font(theme::semibold(17.0)).color(t.text));
        if w::icon_button(ui, &t, ph::CARET_RIGHT, "Next month").clicked() {
            app.budgets.month = m.next();
        }
        if m != Month::of(today) && w::ghost(ui, &t, None, "This month").clicked() {
            app.budgets.month = Month::of(today);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let store = &app.store;
            let unplanned: Vec<_> = store
                .categories()
                .iter()
                .filter(|c| c.kind == CategoryKind::Expense && !c.archived && store.budget_plan(c.id).is_none())
                .collect();
            w::dropdown(ui, "budget-add", format!("{}  Add budget", ph::PLUS), 180.0, |ui| {
                if ui
                    .selectable_label(
                        false,
                        egui::RichText::new(format!("{}  New category…", ph::PLUS)).color(t.accent),
                    )
                    .clicked()
                {
                    acts.push(Act::NewCategory);
                }
                if !unplanned.is_empty() {
                    ui.separator();
                }
                for c in unplanned {
                    if ui
                        .selectable_label(false, format!("{}  {}", icons::glyph(&c.icon), c.name))
                        .clicked()
                    {
                        let avg = d.averages.iter().find(|x| x.0 == c.id).map(|x| x.1).unwrap_or(0);
                        acts.push(Act::Add(c.id, avg.max(base.from_major(100.0))));
                    }
                }
            });
            if store.undo_label() == Some(FIT_LABEL)
                && w::secondary(ui, &t, Some(ph::ARROW_COUNTER_CLOCKWISE), "Undo fit")
                    .on_hover_text("Put your budgets back the way they were")
                    .clicked()
            {
                acts.push(Act::UndoFit);
            }
            if !d.lines.is_empty()
                && w::ghost(ui, &t, Some(ph::SPARKLE), "Fit to 3-month average")
                    .on_hover_text("Set every budget to your recent average spend")
                    .clicked()
            {
                acts.push(Act::FillAverages);
            }
        });
    });
    ui.add_space(12.0);

    let sum = budget::summarize(&d.lines);
    w::grid_row(ui, 150.0, &[1.0], |_, ui, rect| {
        w::with_reveal(ui, shown, 0, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                ui.horizontal(|ui| {
                    let col = ui.available_width() / 3.0;
                    for (i, (label, v, c)) in [
                        ("Budgeted", sum.available, t.text),
                        ("Spent", sum.spent, t.text),
                        (
                            if sum.remaining() >= 0 { "Left" } else { "Over" },
                            sum.remaining().abs(),
                            if sum.remaining() >= 0 { t.pos } else { t.neg },
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.allocate_ui_with_layout(
                            vec2(col - 8.0, 60.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.label(w::faint(&t, label));
                                w::animated_amount(ui, Id::new(("bsum", i)), v, base, theme::display(24.0), c);
                            },
                        );
                    }
                });
                ui.add_space(10.0);
                let used = if sum.available > 0 {
                    sum.spent as f32 / sum.available as f32
                } else {
                    0.0
                };
                w::progress(
                    ui,
                    &t,
                    Id::new("bsum-bar"),
                    used,
                    if m == Month::of(today) { Some(pace) } else { None },
                    w::usage_color(&t, used, pace),
                    10.0,
                );
                let msg = if d.lines.is_empty() {
                    "Add a budget to start tracking.".to_string()
                } else if m == Month::of(today) {
                    let ahead = used - pace;
                    let mood = if ahead > 0.05 {
                        "a little ahead of pace"
                    } else if ahead < -0.05 {
                        "comfortably under pace"
                    } else {
                        "right on pace"
                    };
                    format!(
                        "{:.0}% through {}, {:.0}% of budget used — {mood}.{}",
                        pace * 100.0,
                        m.short(),
                        used * 100.0,
                        if sum.over_count > 0 {
                            format!(" {} over budget.", sum.over_count)
                        } else {
                            String::new()
                        }
                    )
                } else {
                    format!("{:.0}% of budget used.", used * 100.0)
                };
                ui.label(w::subtle(&t, msg));
            })
        })
    });

    // Lines
    if d.lines.is_empty() {
        w::card_frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            w::empty_state(
                ui,
                &t,
                ph::CHART_PIE_SLICE,
                "No budgets yet",
                "Pick a category from “Add budget” — we'll suggest an amount from your history.",
            );
        });
    } else {
        w::card_frame(&t)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (i, l) in d.lines.iter().enumerate() {
                    let selected = app.budgets.sel == Some(i);
                    budget_row(
                        app,
                        ui,
                        &t,
                        l,
                        m,
                        pace,
                        today,
                        i,
                        &d,
                        &mut acts,
                        selected,
                        selected && moved,
                    );
                }
            });
    }

    if !d.unbudgeted.is_empty() {
        ui.add_space(theme::GAP);
        w::card_frame(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            w::card_header(ui, &t, "Unbudgeted spending", |ui| {
                ui.label(w::faint(&t, m.short()));
            });
            for (c, v) in &d.unbudgeted {
                let Some(cat) = app.store.category(*c) else { continue };
                ui.horizontal(|ui| {
                    w::icon_badge(ui, &t, icons::glyph(&cat.icon), w::cat_color(cat.color), 28.0);
                    ui.label(egui::RichText::new(&cat.name).color(t.text));
                    ui.label(w::subtle(&t, w::fmt_money(*v, base)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::ghost(ui, &t, Some(ph::PLUS), "Budget it").clicked() {
                            let avg = d.averages.iter().find(|x| x.0 == *c).map(|x| x.1).unwrap_or(*v);
                            acts.push(Act::Add(*c, avg.max(*v)));
                        }
                    });
                });
            }
        });
    }

    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::Open(c) => {
                let st = &mut app.budgets;
                if st.open == Some(c) {
                    st.open = None;
                } else {
                    st.open = Some(c);
                    let planned = budget::planned(&app.store, c, m).unwrap_or(0);
                    st.amount = money::to_input(planned, base);
                    st.this_month_only = app.store.budget_override(c, m).is_some();
                }
            }
            Act::Close => app.budgets.open = None,
            Act::Save(c, amount, rollover, only_month) => {
                let r = app.store.edit_budgets("Edit budget", &[c], |s| {
                    if only_month {
                        s.set_budget_override(c, m, Some(amount))?;
                        if let Some(mut p) = s.budget_plan(c).cloned() {
                            p.rollover = rollover;
                            s.save_budget_plan(p)?;
                        }
                        Ok(())
                    } else {
                        s.set_budget_override(c, m, None)?;
                        s.save_budget_plan(BudgetPlan {
                            category: c,
                            amount,
                            rollover,
                        })
                    }
                });
                if app.toasts.ok(r).is_some() {
                    app.toasts.undoable("Budget saved");
                    app.budgets.open = None;
                }
            }
            Act::Remove(c) => {
                let r = app
                    .store
                    .edit_budgets("Remove budget", &[c], |s| s.delete_budget_plan(c));
                if app.toasts.ok(r).is_some() {
                    app.toasts.undoable("Budget removed");
                    app.budgets.open = None;
                }
            }
            Act::Add(c, amount) => {
                let amount = round_up(amount, base.scale() * 10);
                let r = app.store.edit_budgets("Add budget", &[c], |s| {
                    s.save_budget_plan(BudgetPlan {
                        category: c,
                        amount,
                        rollover: false,
                    })
                });
                if app.toasts.ok(r).is_some() {
                    app.budgets.edit(c, money::to_input(amount, base));
                }
            }
            Act::FillAverages => {
                let changes: Vec<BudgetPlan> = d
                    .lines
                    .iter()
                    .filter_map(|l| {
                        let avg = d.averages.iter().find(|x| x.0 == l.category).map(|x| x.1).unwrap_or(0);
                        (avg > 0).then(|| BudgetPlan {
                            category: l.category,
                            amount: round_up(avg, base.scale() * 10),
                            rollover: l.rollover,
                        })
                    })
                    .collect();
                let cats: Vec<RowId> = changes.iter().map(|p| p.category).collect();
                let n = changes.len();
                let r = app.store.edit_budgets(FIT_LABEL, &cats, |s| {
                    for p in changes {
                        s.set_budget_override(p.category, m, None)?;
                        s.save_budget_plan(p)?;
                    }
                    Ok(())
                });
                if app.toasts.ok(r).is_some() {
                    app.toasts
                        .undoable(format!("Updated {n} budgets from your 3-month average"));
                }
            }
            Act::UndoFit => app.undo(),
            Act::NewCategory => {
                let f = crate::forms::CategoryForm::new(&app.store, CategoryKind::Expense).then_budget();
                app.open_modal(&ctx, crate::forms::Modal::Category(f));
            }
        }
    }
}

fn round_up(v: i64, step: i64) -> i64 {
    if step <= 0 {
        v
    } else {
        v.div_euclid(step) * step + if v.rem_euclid(step) > 0 { step } else { 0 }
    }
}

#[allow(clippy::too_many_arguments)]
fn budget_row(
    app: &mut App,
    ui: &mut Ui,
    t: &Theme,
    l: &BudgetLine,
    m: Month,
    pace: f32,
    today: Date,
    i: usize,
    d: &Data,
    acts: &mut Vec<Act>,
    selected: bool,
    scroll_into_view: bool,
) {
    let store = &app.store;
    let base = store.base();
    let Some(cat) = store.category(l.category) else { return };
    let color = w::cat_color(cat.color);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click());
    let (alpha, dy) = w::reveal(ui, app.shown_at, i + 1);
    let rect = rect.translate(vec2(0.0, dy));
    crate::marks::record(|| format!("budget:{}", cat.name), rect);
    let open = app.budgets.open == Some(l.category);
    let h = motion::toggle(
        ui.ctx(),
        Id::new(("brow", l.category)),
        resp.hovered() || open || selected,
        motion::MICRO,
    );
    let p = ui.painter().clone();
    let a = |c: egui::Color32| motion::with_alpha(c, alpha);
    if h > 0.0 {
        p.rect_filled(rect, CornerRadius::same(12), a(motion::with_alpha(t.hover, h)));
    }
    if selected {
        p.rect_stroke(
            rect,
            CornerRadius::same(12),
            egui::Stroke::new(1.5, a(t.accent)),
            egui::StrokeKind::Inside,
        );
    }
    if scroll_into_view {
        ui.scroll_to_rect(rect, None);
    }
    let inner = rect.shrink2(vec2(12.0, 10.0));
    let badge = Rect::from_min_size(pos2(inner.left(), inner.center().y - 18.0), vec2(36.0, 36.0));
    w::paint_icon_badge(&p, t, badge, icons::glyph(&cat.icon), color);
    let x = badge.right() + 14.0;
    let right_w = 150.0;
    let bar = Rect::from_min_max(
        pos2(x, inner.bottom() - 9.0),
        pos2(inner.right() - right_w - 16.0, inner.bottom() - 1.0),
    );
    p.text(
        pos2(x, inner.top() + 8.0),
        Align2::LEFT_CENTER,
        &cat.name,
        theme::medium(14.0),
        a(t.text),
    );
    let mut sub = format!(
        "{} of {}",
        w::fmt_whole(l.spent, base),
        w::fmt_whole(l.available(), base)
    );
    if l.carry != 0 {
        sub.push_str(&format!("  ·  {} rolled over", w::fmt_signed(l.carry, base)));
    }
    if store.budget_override(l.category, m).is_some() {
        sub.push_str("  ·  this month only");
    }
    let name_w = p.layout_no_wrap(cat.name.clone(), theme::medium(14.0), t.text).size().x;
    w::text_fit(
        &p,
        pos2(x + name_w + 10.0, inner.top() + 8.0),
        Align2::LEFT_CENTER,
        sub,
        theme::regular(12.0),
        a(t.text3),
        inner.right() - right_w - 16.0 - (x + name_w + 10.0),
    );
    let used = l.used();
    let is_now = m == Month::of(today);
    w::paint_progress(
        ui,
        t,
        Id::new(("bbar", l.category)),
        bar,
        used,
        is_now.then_some(pace),
        w::usage_color(t, used, pace),
    );
    let rem = l.remaining();
    let (txt, c) = if rem >= 0 {
        (w::fmt_whole(rem, base), t.text)
    } else {
        (format!("−{}", w::fmt_whole(-rem, base)), t.neg)
    };
    p.text(
        pos2(inner.right(), inner.top() + 10.0),
        Align2::RIGHT_CENTER,
        txt,
        theme::semibold(15.0),
        a(c),
    );
    let sub_r = match budget::per_day_left(l, m, today) {
        Some(pd) if rem > 0 => format!("{} / day", w::fmt_whole(pd, base)),
        _ if rem < 0 => "over budget".into(),
        _ => "left".into(),
    };
    p.text(
        pos2(inner.right(), inner.bottom() - 5.0),
        Align2::RIGHT_CENTER,
        sub_r,
        theme::regular(11.5),
        a(t.text3),
    );
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        acts.push(Act::Open(l.category));
    }

    // Inline editor, expanding smoothly.
    let k = motion::toggle(ui.ctx(), Id::new(("bedit", l.category)), open, motion::STANDARD);
    if k > 0.01 {
        // Animate to the editor's real height (measured last frame), so it
        // never clips whatever the zoom level.
        let hid = Id::new(("bedit-h", l.category));
        let full_h: f32 = ui.data(|dd| dd.get_temp(hid)).unwrap_or(140.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), full_h * k), Sense::hover());
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
            r.min + vec2(62.0, 6.0),
            vec2(r.width() - 74.0, full_h.max(400.0)),
        )));
        c.set_clip_rect(r.intersect(ui.clip_rect()));
        c.set_opacity(k);
        let st = &mut app.budgets;
        let rollover0 = store.budget_plan(l.category).map(|p| p.rollover).unwrap_or(false);
        let mut rollover = rollover0;
        c.horizontal(|ui| {
            w::field_label(ui, t, "Monthly amount");
            w::text_field(ui, t, Id::new(("bamt", l.category)), &mut st.amount, "0", 130.0);
            let avg = d.averages.iter().find(|x| x.0 == l.category).map(|x| x.1).unwrap_or(0);
            if avg > 0 && w::ghost(ui, t, None, &format!("3-mo avg {}", w::fmt_whole(avg, base))).clicked() {
                st.amount = money::to_input(avg, base);
            }
        });
        c.horizontal_wrapped(|ui| {
            w::toggle_row(ui, t, &mut st.this_month_only, &format!("Only for {}", m.label()));
            ui.add_space(12.0);
            w::toggle_row(ui, t, &mut rollover, "Roll leftovers into next month");
        });
        c.add_space(4.0);
        c.horizontal(|ui| {
            if w::primary(ui, t, None, "Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                match money::parse(&st.amount, base) {
                    Some(v) if v >= 0 => acts.push(Act::Save(l.category, v, rollover, st.this_month_only)),
                    _ => app.toasts.error("That amount isn't a number"),
                }
            }
            if w::ghost(ui, t, None, "Cancel").clicked() {
                acts.push(Act::Close);
            }
            if w::ghost(ui, t, Some(ph::TRASH), "Remove budget").clicked() {
                acts.push(Act::Remove(l.category));
            }
        });
        let measured = c.min_rect().height() + 18.0;
        if (measured - full_h).abs() > 0.5 {
            ui.data_mut(|dd| dd.insert_temp(hid, measured));
            ui.ctx().request_repaint();
        }
        if rollover != rollover0
            && !app.budgets.this_month_only
            && let Some(mut plan) = store.budget_plan(l.category).cloned()
        {
            plan.rollover = rollover;
            let r = app
                .store
                .edit_budgets("Change rollover", &[l.category], |s| s.save_budget_plan(plan));
            app.toasts.ok(r);
        }
    }
}
