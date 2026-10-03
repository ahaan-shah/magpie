//! The dashboard: net worth hero, this month, KPIs, cash flow, categories,
//! budgets, upcoming bills, recent activity, goals and the month in review.

use crate::app::{App, Memo, Page};
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets::{self as w, charts};
use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::analytics::{self, Review, Totals};
use magpie_core::budget::{self, BudgetLine, BudgetSummary};
use magpie_core::goals::{self, GoalStatus};
use magpie_core::{Cur, Goal, Id as RowId, Month, Store, Txn, recurring};
use std::rc::Rc;

pub struct Dash {
    base: Cur,
    month: Month,
    net_worth: i64,
    nw_series: Vec<(Month, i64)>,
    this: Totals,
    last: Totals,
    income_trend: Vec<f32>,
    expense_trend: Vec<f32>,
    net_trend: Vec<f32>,
    cashflow: Vec<(Month, Totals)>,
    cats: Vec<(Option<RowId>, i64)>,
    budgets: Vec<BudgetLine>,
    budget_sum: BudgetSummary,
    upcoming: Vec<(RowId, Date)>,
    confirm: Vec<(RowId, Date)>,
    recent: Vec<Txn>,
    goals: Vec<(Goal, GoalStatus)>,
    review: Review,
    pace: f32,
}

fn build(store: &Store, today: Date) -> Dash {
    let m = Month::of(today);
    let this = analytics::month_totals(store, m);
    let cashflow = analytics::cashflow(store, m, 12);
    let six = &cashflow[cashflow.len() - 6..];
    let budgets = budget::month_budget(store, m);
    let budget_sum = budget::summarize(&budgets);
    let horizon = today.checked_add(jiff::Span::new().days(14)).unwrap_or(today);
    let mut upcoming = recurring::upcoming(store, today.tomorrow().unwrap_or(today), horizon);
    upcoming.truncate(6);
    Dash {
        base: store.base(),
        month: m,
        net_worth: analytics::net_worth(store),
        nw_series: analytics::net_worth_series(store, m, 12),
        this,
        last: analytics::month_totals(store, m.prev()),
        income_trend: six.iter().map(|(_, t)| t.income as f32).collect(),
        expense_trend: six.iter().map(|(_, t)| t.expense as f32).collect(),
        net_trend: six.iter().map(|(_, t)| t.net() as f32).collect(),
        cashflow: cashflow.clone(),
        cats: analytics::spending_by_category(store, m.first(), m.last()),
        budgets,
        budget_sum,
        upcoming,
        confirm: recurring::awaiting_confirmation(store, today),
        recent: store
            .txns()
            .iter()
            .rev()
            .filter(|t| t.date <= today)
            .take(7)
            .cloned()
            .collect(),
        goals: store
            .goals()
            .iter()
            .filter(|g| !g.archived)
            .map(|g| (g.clone(), goals::status(store, g, today)))
            .collect(),
        review: analytics::review(store, m.prev()),
        pace: budget::month_progress(m, today),
    }
}

#[derive(Default)]
pub struct State {
    memo: Memo<(u64, Date), Rc<Dash>>,
    nw: Memo<u64, i64>,
}

impl State {
    pub fn net_worth(&mut self, store: &Store) -> i64 {
        *self.nw.get(store.version(), || analytics::net_worth(store))
    }
}

enum Act {
    Go(Page),
    CategoryLedger(Option<RowId>, Month),
    EditTxn(RowId),
    EditRule(RowId),
    PostRule(RowId),
    SkipRule(RowId),
    Contribute(RowId),
    NewGoal,
    NewBudget,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.basic() {
        return show_basic(app, ui);
    }
    let t = app.t();
    let today = app.today;
    let d = app
        .dashboard
        .memo
        .get((app.store.version(), today), || Rc::new(build(&app.store, today)))
        .clone();
    let shown = app.shown_at;
    let mut acts: Vec<Act> = Vec::new();
    let store = &app.store;
    let base = d.base;

    if !d.confirm.is_empty() {
        confirm_banner(ui, &t, store, &d, &mut acts);
    }

    // Row 1 — hero + this month
    w::grid_row(ui, 236.0, &[2.0, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, i, rect, |ui, rect| match i {
            0 => hero(ui, &t, rect, &d),
            _ => this_month(ui, &t, rect, &d),
        })
    });

    // Row 2 — KPI tiles
    let change = |a: i64, b: i64| if b > 0 { Some((a - b) as f32 / b as f32) } else { None };
    let kpis = [
        (
            "Spent",
            ph::ARROW_UP_RIGHT,
            d.this.expense,
            change(d.this.expense, d.last.expense),
            true,
            d.expense_trend.clone(),
            t.neg,
        ),
        (
            "Income",
            ph::ARROW_DOWN_LEFT,
            d.this.income,
            change(d.this.income, d.last.income),
            false,
            d.income_trend.clone(),
            t.pos,
        ),
        (
            "Saved",
            ph::PIGGY_BANK,
            d.this.net(),
            None,
            false,
            d.net_trend.clone(),
            t.accent,
        ),
        (
            "Left to spend",
            ph::WALLET,
            d.budget_sum.remaining(),
            None,
            false,
            Vec::new(),
            t.warn,
        ),
    ];
    w::grid_row(ui, 128.0, &[1.0, 1.0, 1.0, 1.0], |i, ui, rect| {
        let (label, icon, value, delta, bad_up, trend, color) = &kpis[i];
        w::with_reveal(ui, shown, 2 + i, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                ui.horizontal(|ui| {
                    w::icon_badge(ui, &t, icon, *color, 28.0);
                    ui.label(egui::RichText::new(*label).font(theme::medium(12.5)).color(t.text2));
                });
                ui.add_space(4.0);
                w::animated_amount(ui, Id::new(("kpi", i)), *value, base, theme::display(22.0), t.text);
                let spark = Rect::from_min_max(
                    pos2(rect.right() - 110.0, rect.bottom() - 52.0),
                    pos2(rect.right() - 20.0, rect.bottom() - 20.0),
                );
                if trend.len() > 1 {
                    charts::sparkline(ui, Id::new(("kpi-spark", i)), spark, trend, *color);
                }
                if let Some(dl) = delta {
                    let up = *dl > 0.0;
                    let good = up != *bad_up;
                    let c = if good { t.pos } else { t.neg };
                    let arrow = if up { ph::TREND_UP } else { ph::TREND_DOWN };
                    ui.label(
                        egui::RichText::new(format!("{arrow} {:.0}% vs last month", dl.abs() * 100.0))
                            .font(theme::medium(11.5))
                            .color(c),
                    );
                } else if i == 3 {
                    let per_day = if d.budget_sum.available > 0 {
                        let days_left = (d.month.days() - today.day() as i32 + 1).max(1) as i64;
                        format!(
                            "{} / day for {} day{}",
                            w::fmt_whole(d.budget_sum.remaining().max(0) / days_left, base),
                            days_left,
                            if days_left == 1 { "" } else { "s" }
                        )
                    } else {
                        "Set up budgets to see this".into()
                    };
                    ui.label(w::faint(&t, per_day));
                } else if let Some(r) = d.this.savings_rate() {
                    let text = if r < -1.0 {
                        "more out than in so far".to_string()
                    } else {
                        format!("{:.0}% of income", r * 100.0)
                    };
                    ui.label(w::faint(&t, text));
                }
            })
        });
    });

    // Row 3 — cash flow + categories
    w::grid_row(ui, 348.0, &[1.75, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 6 + i, rect, |ui, rect| match i {
            0 => w::card_in(ui, &t, rect, |ui| {
                w::card_header(ui, &t, "Cash flow", |ui| {
                    legend(ui, &t, &[("Income", t.pos), ("Spending", t.neg), ("Net", t.accent)]);
                });
                let r = ui.available_rect_before_wrap();
                let groups: Vec<charts::BarGroup> = d
                    .cashflow
                    .iter()
                    .map(|(m, tt)| charts::BarGroup {
                        label: m.short().to_string(),
                        values: vec![tt.income as f32, tt.expense as f32],
                    })
                    .collect();
                let net: Vec<f32> = d.cashflow.iter().map(|(_, tt)| tt.net() as f32).collect();
                let fmt = move |v: f32| w::fmt_compact(v as i64, base);
                charts::grouped_bars(
                    ui,
                    &t,
                    Id::new("dash-cashflow"),
                    r,
                    &groups,
                    &[motion::with_alpha(t.pos, 0.9), motion::with_alpha(t.neg, 0.85)],
                    &["Income", "Spending"],
                    Some((&net, t.accent)),
                    &fmt,
                );
            }),
            _ => w::card_in(ui, &t, rect, |ui| categories_card(ui, &t, store, &d, &mut acts)),
        })
    });

    // Row 4 — budgets + upcoming
    w::grid_row(ui, 316.0, &[1.0, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 8 + i, rect, |ui, rect| match i {
            0 => w::card_scroll(ui, &t, "dash-budgets", rect, |ui| {
                budgets_card(ui, &t, store, &d, &mut acts)
            }),
            _ => w::card_scroll(ui, &t, "dash-upcoming", rect, |ui| {
                upcoming_card(ui, &t, store, &d, today, &mut acts)
            }),
        })
    });

    // Row 5 — recent + goals
    w::grid_row(ui, 360.0, &[1.3, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 10 + i, rect, |ui, rect| match i {
            0 => w::card_scroll(ui, &t, "dash-recent", rect, |ui| {
                recent_card(ui, &t, store, &d, today, &mut acts)
            }),
            _ => w::card_scroll(ui, &t, "dash-goals", rect, |ui| {
                goals_card(ui, &t, &d, false, &mut acts)
            }),
        })
    });

    // Month in review
    if d.review.totals.count > 0 {
        w::grid_row(ui, 150.0, &[1.0], |_, ui, rect| {
            w::with_reveal(ui, shown, 12, rect, |ui, rect| {
                w::card_in(ui, &t, rect, |ui| review_card(ui, &t, store, &d))
            })
        });
    }

    apply(app, ui, acts);
}

/// Basic mode's Home: how this month is going, where the money went, and
/// budgets (plus goals, once there are any). Nothing else.
fn show_basic(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let d = app
        .dashboard
        .memo
        .get((app.store.version(), today), || Rc::new(build(&app.store, today)))
        .clone();
    let shown = app.shown_at;
    let mut acts: Vec<Act> = Vec::new();
    let store = &app.store;

    if !d.confirm.is_empty() {
        confirm_banner(ui, &t, store, &d, &mut acts);
    }
    w::grid_row(ui, 336.0, &[1.0, 1.25], |i, ui, rect| {
        w::with_reveal(ui, shown, i, rect, |ui, rect| match i {
            0 => month_basic(ui, &t, rect, &d),
            _ => w::card_in(ui, &t, rect, |ui| categories_card(ui, &t, store, &d, &mut acts)),
        })
    });
    let weights: &[f32] = if d.goals.is_empty() { &[1.0] } else { &[1.0, 1.0] };
    w::grid_row(ui, 316.0, weights, |i, ui, rect| {
        w::with_reveal(ui, shown, 2 + i, rect, |ui, rect| match i {
            0 => w::card_scroll(ui, &t, "home-budgets", rect, |ui| {
                budgets_card(ui, &t, store, &d, &mut acts)
            }),
            _ => w::card_scroll(ui, &t, "home-goals", rect, |ui| goals_card(ui, &t, &d, true, &mut acts)),
        })
    });
    apply(app, ui, acts);
}

/// "This month" in plain words: what's left, what came in, what went out.
fn month_basic(ui: &mut Ui, t: &Theme, rect: Rect, d: &Dash) {
    crate::marks::record(|| "home:month".into(), rect);
    w::card_in(ui, t, rect, |ui| {
        let base = d.base;
        w::card_header(ui, t, "This month", |ui| {
            ui.label(w::faint(t, d.month.label()));
        });
        ui.add_space(4.0);
        let left = d.this.net();
        ui.label(w::faint(t, "Left this month"));
        w::animated_amount(
            ui,
            Id::new("home-left"),
            left,
            base,
            theme::display(36.0),
            if left < 0 { t.neg } else { t.text },
        );
        let note = match d.this.savings_rate() {
            None => "Nothing has come in yet this month.".to_string(),
            Some(_) if left < 0 => "More has gone out than came in.".to_string(),
            Some(r) => format!("You've kept {:.0}% of what came in.", r * 100.0),
        };
        ui.label(w::subtle(t, note));
        ui.add_space(16.0);
        let col = (ui.available_width() - 12.0) / 2.0;
        ui.horizontal(|ui| {
            for (i, (label, icon, value, color)) in [
                ("Money in", ph::ARROW_DOWN_LEFT, d.this.income, t.pos),
                ("Money out", ph::ARROW_UP_RIGHT, d.this.expense, t.neg),
            ]
            .into_iter()
            .enumerate()
            {
                ui.allocate_ui_with_layout(
                    vec2(col, 52.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        w::icon_badge(ui, t, icon, color, 34.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(w::faint(t, label));
                            w::animated_amount(
                                ui,
                                Id::new(("home-flow", i)),
                                value,
                                base,
                                theme::semibold(17.0),
                                if i == 0 { t.pos } else { t.text },
                            );
                        });
                    },
                );
            }
        });
        if d.budget_sum.available > 0 {
            ui.add_space(14.0);
            let used = d.budget_sum.spent as f32 / d.budget_sum.available as f32;
            let c = w::usage_color(t, used, d.pace);
            w::progress(ui, t, Id::new("home-budget"), used, Some(d.pace), c, 8.0);
            ui.label(w::faint(
                t,
                format!(
                    "{} of your {} budget used",
                    w::fmt_whole(d.budget_sum.spent, base),
                    w::fmt_whole(d.budget_sum.available, base)
                ),
            ));
        }
    });
}

fn apply(app: &mut App, ui: &Ui, acts: Vec<Act>) {
    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::Go(p) => app.go(&ctx, p),
            Act::CategoryLedger(c, m) => {
                if app.basic() {
                    app.basic_ledger.show_category(c, m);
                } else {
                    app.ledger.show_category(c, m);
                }
                app.go(&ctx, Page::Ledger);
            }
            Act::EditTxn(id) => {
                if let Some(tx) = app.store.txn(id).cloned() {
                    let f = forms::TxnForm::edit(&app.store, &tx);
                    app.open_modal(&ctx, Modal::Txn(f));
                }
            }
            Act::EditRule(id) => {
                if let Some(r) = app.store.rule(id).cloned() {
                    let f = forms::RuleForm::edit(&app.store, &r);
                    app.open_modal(&ctx, Modal::Rule(f));
                }
            }
            Act::PostRule(id) => {
                if app.toasts.ok(recurring::post_next(&mut app.store, id)).is_some() {
                    app.toasts.undoable("Posted");
                }
            }
            Act::SkipRule(id) => {
                if app.toasts.ok(recurring::skip_next(&mut app.store, id)).is_some() {
                    app.toasts.info("Skipped this one");
                }
            }
            Act::Contribute(id) => {
                let f = forms::ContribForm::new(id, app.today);
                app.open_modal(&ctx, Modal::Contribution(f));
            }
            Act::NewGoal => {
                let f = forms::GoalForm::new(&app.store, app.today);
                app.open_modal(&ctx, Modal::Goal(f));
            }
            Act::NewBudget => app.go(&ctx, Page::Budgets),
        }
    }
}

fn legend(ui: &mut Ui, t: &Theme, items: &[(&str, Color32)]) {
    for (label, c) in items.iter().rev() {
        ui.label(egui::RichText::new(*label).font(theme::regular(11.5)).color(t.text2));
        w::colored_dot(ui, *c);
        ui.add_space(6.0);
    }
}

fn hero(ui: &mut Ui, t: &Theme, rect: Rect, d: &Dash) {
    crate::marks::record(|| "dash:hero".into(), rect);
    w::card_in(ui, t, rect, |ui| {
        let base = d.base;
        let prev = d.nw_series.iter().rev().nth(1).map(|x| x.1).unwrap_or(d.net_worth);
        let delta = d.net_worth - prev;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Net worth")
                    .font(theme::medium(13.0))
                    .color(t.text2),
            );
            ui.add_space(6.0);
            let c = if delta >= 0 { t.pos } else { t.neg };
            let arrow = if delta >= 0 { ph::TREND_UP } else { ph::TREND_DOWN };
            w::chip(
                ui,
                t,
                Some(arrow),
                &format!("{} this month", w::fmt_signed(delta, base)),
                c,
            );
        });
        ui.add_space(2.0);
        w::animated_amount(ui, Id::new("hero-nw"), d.net_worth, base, theme::display(40.0), t.text);
        let chart = Rect::from_min_max(
            pos2(rect.left() + 1.0, ui.cursor().top() + 6.0),
            pos2(rect.right() - 1.0, rect.bottom() - 14.0),
        );
        let vals: Vec<f32> = d.nw_series.iter().map(|x| x.1 as f32).collect();
        let labels: Vec<String> = d.nw_series.iter().map(|x| x.0.label()).collect();
        let fmt = move |v: f32| w::fmt_whole(v as i64, base);
        charts::area_chart(
            ui,
            t,
            Id::new("hero-area"),
            chart,
            &vals,
            charts::AreaOpts {
                color: t.accent,
                axis: false,
                labels: &labels,
                fmt: &fmt,
                baseline_zero: false,
                label_every: 1,
            },
        );
    });
}

fn this_month(ui: &mut Ui, t: &Theme, rect: Rect, d: &Dash) {
    w::card_in(ui, t, rect, |ui| {
        let base = d.base;
        ui.label(
            egui::RichText::new(d.month.label())
                .font(theme::medium(13.0))
                .color(t.text2),
        );
        ui.add_space(2.0);
        let ring_c = pos2(rect.right() - 76.0, rect.top() + 92.0);
        let rate = d.this.savings_rate().unwrap_or(0.0) as f32;
        let ring_color = if rate >= 0.2 {
            t.pos
        } else if rate >= 0.0 {
            t.warn
        } else {
            t.neg
        };
        charts::ring(
            ui,
            Id::new("month-ring"),
            ring_c,
            44.0,
            9.0,
            rate.max(0.0),
            ring_color,
            t.hover,
        );
        let p = ui.painter();
        // Early in a month (little or no income yet) the ratio is meaningless
        // — e.g. "-5853%" — so show something readable instead.
        let (big, small) = match d.this.savings_rate() {
            None => ("—".to_string(), "no income yet"),
            Some(r) if r < -1.0 => ("—".to_string(), "spent > income"),
            Some(r) => (format!("{:.0}%", r * 100.0), "saved"),
        };
        p.text(
            ring_c - vec2(0.0, 6.0),
            Align2::CENTER_CENTER,
            big,
            theme::display(19.0),
            t.text,
        );
        p.text(
            ring_c + vec2(0.0, 13.0),
            Align2::CENTER_CENTER,
            small,
            theme::regular(11.0),
            t.text3,
        );

        ui.label(w::faint(t, "Spent"));
        w::animated_amount(
            ui,
            Id::new("month-spent"),
            d.this.expense,
            base,
            theme::display(26.0),
            t.text,
        );
        ui.add_space(6.0);
        ui.label(w::faint(t, "Income"));
        w::animated_amount(
            ui,
            Id::new("month-income"),
            d.this.income,
            base,
            theme::semibold(17.0),
            t.pos,
        );
        ui.add_space(10.0);
        if d.budget_sum.available > 0 {
            let used = d.budget_sum.spent as f32 / d.budget_sum.available as f32;
            let c = w::usage_color(t, used, d.pace);
            w::progress(ui, t, Id::new("month-budget"), used, Some(d.pace), c, 8.0);
            ui.label(w::faint(
                t,
                format!(
                    "{} of {} budgeted",
                    w::fmt_whole(d.budget_sum.spent, base),
                    w::fmt_whole(d.budget_sum.available, base)
                ),
            ));
        }
    });
}

fn categories_card(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash, acts: &mut Vec<Act>) {
    let base = d.base;
    w::card_header(ui, t, "Where it went", |ui| {
        ui.label(w::faint(t, d.month.short()));
    });
    if d.cats.is_empty() {
        w::empty_state(
            ui,
            t,
            ph::CHART_DONUT,
            "No spending yet",
            "Your categories will show up here.",
        );
        return;
    }
    let area = ui.available_rect_before_wrap();
    let donut_size = (area.height() - 4.0).min(area.width() * 0.5);
    let donut_rect = Rect::from_min_size(area.min, vec2(donut_size, donut_size));
    let top: Vec<_> = d.cats.iter().take(6).collect();
    let rest: i64 = d.cats.iter().skip(6).map(|x| x.1).sum();
    let mut slices: Vec<charts::Slice> = top
        .iter()
        .map(|(c, v)| {
            let cat = c.and_then(|c| store.category(c));
            charts::Slice {
                value: *v as f32,
                color: cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
                label: store.category_name(*c).to_string(),
            }
        })
        .collect();
    if rest > 0 {
        slices.push(charts::Slice {
            value: rest as f32,
            color: t.border,
            label: "Other".into(),
        });
    }
    let hovered = charts::donut(ui, t, Id::new("dash-donut"), donut_rect, &slices, 18.0);
    let total: i64 = d.cats.iter().map(|x| x.1).sum();
    let p = ui.painter();
    let (title, value) = match hovered {
        Some(i) => (slices[i].label.clone(), slices[i].value as i64),
        None => ("Total".into(), total),
    };
    p.text(
        donut_rect.center() - vec2(0.0, 9.0),
        Align2::CENTER_CENTER,
        w::fmt_whole(value, base),
        theme::display(17.0),
        t.text,
    );
    p.text(
        donut_rect.center() + vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        title,
        theme::regular(11.5),
        t.text3,
    );
    if hovered.is_some_and(|i| i < top.len()) && ui.input(|i| i.pointer.primary_clicked()) {
        acts.push(Act::CategoryLedger(top[hovered.unwrap_or(0)].0, d.month));
    }

    // Legend list
    let list = Rect::from_min_max(pos2(donut_rect.right() + 14.0, area.top() + 4.0), area.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
    for (i, s) in slices.iter().enumerate() {
        let (r, resp) = child.allocate_exact_size(vec2(list.width(), 26.0), Sense::click());
        let hl = motion::toggle(
            child.ctx(),
            Id::new(("dash-leg", i)),
            resp.hovered() || hovered == Some(i),
            motion::MICRO,
        );
        let p = child.painter();
        if hl > 0.0 {
            p.rect_filled(
                r.expand2(vec2(4.0, 0.0)),
                CornerRadius::same(6),
                motion::with_alpha(t.hover, hl),
            );
        }
        p.circle_filled(pos2(r.left() + 5.0, r.center().y), 4.0, s.color);
        p.text(
            pos2(r.left() + 16.0, r.center().y),
            Align2::LEFT_CENTER,
            &s.label,
            theme::regular(12.5),
            t.text,
        );
        let pct = s.value / total.max(1) as f32 * 100.0;
        p.text(
            pos2(r.right(), r.center().y),
            Align2::RIGHT_CENTER,
            format!("{pct:.0}%"),
            theme::medium(12.0),
            t.text2,
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && i < top.len() {
            acts.push(Act::CategoryLedger(top[i].0, d.month));
        }
    }
}

fn budgets_card(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash, acts: &mut Vec<Act>) {
    let base = d.base;
    w::card_header(ui, t, "Budgets", |ui| {
        if w::ghost(ui, t, None, "See all").clicked() {
            acts.push(Act::Go(Page::Budgets));
        }
    });
    if d.budgets.is_empty() {
        w::empty_state(
            ui,
            t,
            ph::CHART_PIE_SLICE,
            "No budgets yet",
            "Give each category a monthly limit.",
        );
        ui.vertical_centered(|ui| {
            if w::secondary(ui, t, Some(ph::PLUS), "Create a budget").clicked() {
                acts.push(Act::NewBudget);
            }
        });
        return;
    }
    let mut lines: Vec<&BudgetLine> = d.budgets.iter().collect();
    lines.sort_by(|a, b| b.used().partial_cmp(&a.used()).unwrap_or(std::cmp::Ordering::Equal));
    for (i, l) in lines.iter().take(5).enumerate() {
        let Some(c) = store.category(l.category) else { continue };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{}  {}", icons::glyph(&c.icon), c.name))
                    .font(theme::medium(13.0))
                    .color(t.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rem = l.remaining();
                let (txt, col) = if rem >= 0 {
                    (format!("{} left", w::fmt_whole(rem, base)), t.text2)
                } else {
                    (format!("{} over", w::fmt_whole(-rem, base)), t.neg)
                };
                ui.label(egui::RichText::new(txt).font(theme::regular(12.0)).color(col));
            });
        });
        let used = l.used();
        w::progress(
            ui,
            t,
            Id::new(("dash-budget", l.category)),
            used,
            Some(d.pace),
            w::usage_color(t, used, d.pace),
            7.0,
        );
        if i < 4 {
            ui.add_space(6.0);
        }
    }
}

fn upcoming_card(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash, today: Date, acts: &mut Vec<Act>) {
    w::card_header(ui, t, "Coming up", |ui| {
        if w::ghost(ui, t, None, "Recurring").clicked() {
            acts.push(Act::Go(Page::Recurring));
        }
    });
    if d.upcoming.is_empty() {
        w::empty_state(
            ui,
            t,
            ph::CALENDAR_CHECK,
            "Nothing due soon",
            "Recurring bills and income show here.",
        );
        return;
    }
    for (id, date) in &d.upcoming {
        let Some(r) = store.rule(*id) else { continue };
        let cur = store.account_cur(r.account);
        let cat = r.category.and_then(|c| store.category(c));
        let row = list_row(ui, t, Id::new(("up", *id, date.to_string())), |ui, rect| {
            let p = ui.painter();
            let badge = Rect::from_min_size(pos2(rect.left(), rect.center().y - 17.0), vec2(34.0, 34.0));
            let color = cat.map(|c| w::cat_color(c.color)).unwrap_or(t.accent);
            w::paint_icon_badge(
                p,
                t,
                badge,
                cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::REPEAT),
                color,
            );
            w::text_fit(
                p,
                pos2(badge.right() + 12.0, rect.center().y - 8.0),
                Align2::LEFT_CENTER,
                &r.payee,
                theme::medium(13.0),
                t.text,
                rect.right() - (badge.right() + 12.0) - 110.0,
            );
            w::text_fit(
                p,
                pos2(badge.right() + 12.0, rect.center().y + 9.0),
                Align2::LEFT_CENTER,
                {
                    let label = w::day_label(*date, today);
                    let rel = w::in_days(*date, today);
                    if label.eq_ignore_ascii_case(&rel) {
                        label
                    } else {
                        format!("{label} · {rel}")
                    }
                },
                theme::regular(11.5),
                t.text3,
                rect.right() - (badge.right() + 12.0) - 110.0,
            );
            let c = if r.amount > 0 { t.pos } else { t.text };
            p.text(
                pos2(rect.right(), rect.center().y),
                Align2::RIGHT_CENTER,
                w::fmt_signed(r.amount, cur),
                theme::semibold(13.0),
                c,
            );
        });
        if row.clicked() {
            acts.push(Act::EditRule(*id));
        }
    }
}

fn recent_card(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash, today: Date, acts: &mut Vec<Act>) {
    w::card_header(ui, t, "Recent activity", |ui| {
        if w::ghost(ui, t, None, "All transactions").clicked() {
            acts.push(Act::Go(Page::Ledger));
        }
    });
    if d.recent.is_empty() {
        w::empty_state(
            ui,
            t,
            ph::RECEIPT,
            "No transactions yet",
            concat!("Press ", shortcut!("N"), " to add one."),
        );
        return;
    }
    for tx in &d.recent {
        let row = txn_row(ui, t, store, tx, today);
        if row.clicked() {
            acts.push(Act::EditTxn(tx.id));
        }
    }
}

/// A compact transaction row used on the dashboard and account pages.
pub fn txn_row(ui: &mut Ui, t: &Theme, store: &Store, tx: &Txn, today: Date) -> egui::Response {
    let cur = store.account_cur(tx.account);
    let cat = tx.category.and_then(|c| store.category(c));
    list_row(ui, t, Id::new(("recent", tx.id)), |ui, rect| {
        let p = ui.painter();
        let badge = Rect::from_min_size(pos2(rect.left(), rect.center().y - 17.0), vec2(34.0, 34.0));
        let (glyph, color) = if tx.is_transfer() {
            (ph::ARROWS_LEFT_RIGHT, t.text3)
        } else {
            (
                cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::TAG),
                cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
            )
        };
        w::paint_icon_badge(p, t, badge, glyph, color);
        let payee = if tx.payee.is_empty() { "—" } else { &tx.payee };
        w::text_fit(
            p,
            pos2(badge.right() + 12.0, rect.center().y - 8.0),
            Align2::LEFT_CENTER,
            payee,
            theme::medium(13.0),
            t.text,
            rect.right() - (badge.right() + 12.0) - 110.0,
        );
        let sub = format!(
            "{} · {}",
            if tx.is_transfer() {
                "Transfer"
            } else {
                store.category_name(tx.category)
            },
            w::day_label(tx.date, today)
        );
        w::text_fit(
            p,
            pos2(badge.right() + 12.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            sub,
            theme::regular(11.5),
            t.text3,
            rect.right() - (badge.right() + 12.0) - 110.0,
        );
        let c = if tx.amount > 0 { t.pos } else { t.text };
        p.text(
            pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            w::fmt_signed(tx.amount, cur),
            theme::semibold(13.0),
            c,
        );
    })
}

pub fn list_row(ui: &mut Ui, t: &Theme, id: Id, paint: impl FnOnce(&mut Ui, Rect)) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let h = motion::toggle(ui.ctx(), id, resp.hovered(), motion::MICRO);
    if h > 0.0 {
        ui.painter().rect_filled(
            rect.expand2(vec2(8.0, 0.0)),
            CornerRadius::same(10),
            motion::with_alpha(t.hover, h),
        );
    }
    paint(ui, rect);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn goals_card(ui: &mut Ui, t: &Theme, d: &Dash, basic: bool, acts: &mut Vec<Act>) {
    w::card_header(ui, t, "Goals", |ui| {
        // Basic mode has no Goals page; the card is the whole feature.
        if !basic && w::ghost(ui, t, None, "See all").clicked() {
            acts.push(Act::Go(Page::Goals));
        }
    });
    if d.goals.is_empty() {
        w::empty_state(ui, t, ph::TARGET, "No goals yet", "Save up for something that matters.");
        ui.vertical_centered(|ui| {
            if w::secondary(ui, t, Some(ph::PLUS), "New goal").clicked() {
                acts.push(Act::NewGoal);
            }
        });
        return;
    }
    for (g, s) in d.goals.iter().take(4) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
        let h = motion::toggle(ui.ctx(), Id::new(("goal-row", g.id)), resp.hovered(), motion::MICRO);
        if h > 0.0 {
            ui.painter().rect_filled(
                rect.expand2(vec2(8.0, 0.0)),
                CornerRadius::same(10),
                motion::with_alpha(t.hover, h),
            );
        }
        let color = w::cat_color(g.color);
        let c = pos2(rect.left() + 24.0, rect.center().y);
        charts::ring(
            ui,
            Id::new(("goal-ring-dash", g.id)),
            c,
            20.0,
            5.0,
            s.fraction,
            color,
            t.hover,
        );
        let p = ui.painter();
        p.text(
            c,
            Align2::CENTER_CENTER,
            icons::glyph(&g.icon),
            theme::regular(14.0),
            w::readable(t, color),
        );
        w::text_fit(
            p,
            pos2(rect.left() + 58.0, rect.center().y - 9.0),
            Align2::LEFT_CENTER,
            &g.name,
            theme::medium(13.0),
            t.text,
            rect.right() - (rect.left() + 58.0) - 110.0,
        );
        w::text_fit(
            p,
            pos2(rect.left() + 58.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!(
                "{} of {}",
                w::fmt_whole(s.saved, g.currency),
                w::fmt_whole(g.target, g.currency)
            ),
            theme::regular(11.5),
            t.text3,
            rect.right() - (rect.left() + 58.0) - 110.0,
        );
        p.text(
            pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{:.0}%", s.fraction * 100.0),
            theme::semibold(14.0),
            w::readable(t, color),
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && g.account.is_none() {
            acts.push(Act::Contribute(g.id));
        }
    }
}

fn review_card(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash) {
    let r = &d.review;
    let base = d.base;
    let Some(m) = r.month else { return };
    ui.horizontal(|ui| {
        w::icon_badge(ui, t, ph::SPARKLE, t.accent, 30.0);
        ui.label(
            egui::RichText::new(format!("{} in review", m.label()))
                .font(theme::semibold(14.5))
                .color(t.text),
        );
    });
    ui.add_space(10.0);
    let spent_change = if r.prev.expense > 0 {
        Some((r.totals.expense - r.prev.expense) as f32 / r.prev.expense as f32)
    } else {
        None
    };
    let cols = ui.available_width() / 4.0;
    ui.horizontal(|ui| {
        let stat = |ui: &mut Ui, label: &str, value: String, sub: String, color: Color32| {
            ui.allocate_ui_with_layout(
                vec2(cols - 10.0, 80.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(w::faint(t, label));
                    ui.label(egui::RichText::new(value).font(theme::display(19.0)).color(color));
                    ui.label(egui::RichText::new(sub).font(theme::regular(12.0)).color(t.text2));
                },
            );
        };
        stat(
            ui,
            "You saved",
            w::fmt_whole(r.totals.net(), base),
            r.totals
                .savings_rate()
                .filter(|x| *x >= -1.0)
                .map(|x| format!("{:.0}% of income", x * 100.0))
                .unwrap_or_default(),
            if r.totals.net() >= 0 { t.pos } else { t.neg },
        );
        stat(
            ui,
            "Spending",
            w::fmt_whole(r.totals.expense, base),
            spent_change
                .map(|c| {
                    format!(
                        "{} {:.0}% vs {}",
                        if c > 0.0 { "up" } else { "down" },
                        c.abs() * 100.0,
                        m.prev().short()
                    )
                })
                .unwrap_or_default(),
            t.text,
        );
        if let Some((cat, delta)) = &r.biggest_change {
            stat(
                ui,
                "Biggest change",
                store.category_name(*cat).to_string(),
                format!("{} vs {}", w::fmt_signed(*delta, base), m.prev().short()),
                t.text,
            );
        }
        stat(
            ui,
            "No-spend days",
            format!("{}", r.no_spend_days),
            r.top_payee
                .as_ref()
                .map(|(n, v)| format!("Top payee: {n} ({})", w::fmt_whole(*v, base)))
                .unwrap_or_default(),
            t.accent,
        );
    });
}

fn confirm_banner(ui: &mut Ui, t: &Theme, store: &Store, d: &Dash, acts: &mut Vec<Act>) {
    egui::Frame::new()
        .fill(t.tint(t.warn, 0.12))
        .stroke(egui::Stroke::new(1.0, motion::with_alpha(t.warn, 0.4)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (id, date) in &d.confirm {
                let Some(r) = store.rule(*id) else { continue };
                let cur = store.account_cur(r.account);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(ph::CLOCK).font(theme::regular(17.0)).color(t.warn));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} ({}) was due {}",
                            r.payee,
                            w::fmt_money(r.amount.abs(), cur),
                            w::day_label(*date, magpie_core::today()).to_lowercase()
                        ))
                        .color(t.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::primary(ui, t, Some(ph::CHECK), "Post").clicked() {
                            acts.push(Act::PostRule(*id));
                        }
                        if w::ghost(ui, t, None, "Skip").clicked() {
                            acts.push(Act::SkipRule(*id));
                        }
                    });
                });
            }
        });
    ui.add_space(theme::GAP);
}
