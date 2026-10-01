//! Reports: cash flow, net worth, category trends, breakdowns, payees and a
//! spending calendar.

use crate::app::{App, Memo, Page};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets::{self as w, charts};
use egui::{Align2, Id, Rect, Sense, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::analytics::{self, Totals};
use magpie_core::{Cur, Id as RowId, Month, Store};
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    range: usize,
    memo: Memo<(u64, usize, Date), Rc<Data>>,
}

const RANGES: [&str; 4] = ["6 months", "12 months", "Year to date", "2 years"];

struct Data {
    months: Vec<Month>,
    cashflow: Vec<(Month, Totals)>,
    total: Totals,
    nw: Vec<(Month, i64)>,
    trend: (Vec<Month>, analytics::Series),
    cats: Vec<(Option<RowId>, i64, Vec<f32>)>,
    payees: Vec<(String, i64, usize)>,
    heat_start: Date,
    heat: Vec<i64>,
}

fn months_for(range: usize, today: Date) -> usize {
    match range {
        0 => 6,
        1 => 12,
        2 => today.month() as usize,
        _ => 24,
    }
}

fn build(store: &Store, range: usize, today: Date) -> Data {
    let last = Month::of(today);
    let n = months_for(range, today);
    let first = last.add(-(n as i32) + 1);
    let cashflow = analytics::cashflow(store, last, n);
    let total = analytics::totals(store, first.first(), last.last());
    let months: Vec<Month> = cashflow.iter().map(|x| x.0).collect();
    let cats_total = analytics::spending_by_category(store, first.first(), last.last());
    let cats = cats_total
        .into_iter()
        .take(12)
        .map(|(c, v)| {
            let series = months
                .iter()
                .map(|m| {
                    store
                        .txns_in(*m)
                        .iter()
                        .filter(|t| t.category == c)
                        .map(|t| analytics::flow(store, t).1)
                        .sum::<i64>() as f32
                })
                .collect();
            (c, v, series)
        })
        .collect();
    let heat_start = today.checked_sub(jiff::Span::new().days(364)).unwrap_or(today);
    Data {
        cashflow,
        total,
        nw: analytics::net_worth_series(store, last, n.max(2)),
        trend: analytics::category_trend(store, last, n, 6),
        cats,
        payees: analytics::top_payees(store, first.first(), last.last(), 8),
        heat: analytics::daily_spend(store, heat_start, today),
        heat_start,
        months,
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let base = app.store.base();
    ui.horizontal(|ui| {
        if app.keys.left {
            app.reports.range = app.reports.range.saturating_sub(1);
        }
        if app.keys.right {
            app.reports.range = (app.reports.range + 1).min(RANGES.len() - 1);
        }
        w::segmented(ui, &t, Id::new("reports-range"), &mut app.reports.range, &RANGES);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::ghost(ui, &t, Some(ph::DOWNLOAD_SIMPLE), "Export summary")
                .on_hover_text("Monthly income/spending by category as CSV")
                .clicked()
            {
                export_summary(app);
            }
        });
    });
    ui.add_space(14.0);
    let range = app.reports.range;
    let d = app
        .reports
        .memo
        .get((app.store.version(), range, today), || {
            Rc::new(build(&app.store, range, today))
        })
        .clone();
    let store = &app.store;
    let shown = app.shown_at;
    let n = d.months.len().max(1) as i64;
    let fmt = move |v: f32| w::fmt_compact(v as i64, base);

    // KPI strip
    let kpis = [
        ("Income", d.total.income, t.pos),
        ("Spending", d.total.expense, t.text),
        (
            "Saved",
            d.total.net(),
            if d.total.net() >= 0 { t.accent } else { t.neg },
        ),
        ("Avg. monthly spend", d.total.expense / n, t.text),
    ];
    w::grid_row(ui, 104.0, &[1.0, 1.0, 1.0, 1.0], |i, ui, rect| {
        let (label, v, c) = kpis[i];
        w::with_reveal(ui, shown, i, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                ui.label(w::faint(&t, label));
                w::animated_amount(ui, Id::new(("rk", i)), v, base, theme::display(22.0), c);
                if i == 2
                    && let Some(r) = d.total.savings_rate()
                {
                    ui.label(w::faint(&t, format!("{:.0}% savings rate", r * 100.0)));
                }
            })
        });
    });

    // Cash flow
    w::grid_row(ui, 330.0, &[1.0], |_, ui, rect| {
        w::with_reveal(ui, shown, 4, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                w::card_header(ui, &t, "Income vs spending", |_| {});
                let groups: Vec<charts::BarGroup> = d
                    .cashflow
                    .iter()
                    .map(|(m, tt)| charts::BarGroup {
                        label: short_label(*m, n as usize),
                        values: vec![tt.income as f32, tt.expense as f32],
                    })
                    .collect();
                let net: Vec<f32> = d.cashflow.iter().map(|x| x.1.net() as f32).collect();
                let r = ui.available_rect_before_wrap();
                charts::grouped_bars(
                    ui,
                    &t,
                    Id::new(("rep-cash", range)),
                    r,
                    &groups,
                    &[motion::with_alpha(t.pos, 0.9), motion::with_alpha(t.neg, 0.85)],
                    &["Income", "Spending"],
                    Some((&net, t.accent)),
                    &fmt,
                );
            })
        })
    });

    // Net worth + category trend
    w::grid_row(ui, 320.0, &[1.0, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 5 + i, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                if i == 0 {
                    w::card_header(ui, &t, "Net worth", |_| {});
                    let vals: Vec<f32> = d.nw.iter().map(|x| x.1 as f32).collect();
                    let labels: Vec<String> = d.nw.iter().map(|x| short_label(x.0, d.nw.len())).collect();
                    let r = ui.available_rect_before_wrap();
                    charts::area_chart(
                        ui,
                        &t,
                        Id::new(("rep-nw", range)),
                        r,
                        &vals,
                        charts::AreaOpts {
                            color: t.accent,
                            axis: true,
                            labels: &labels,
                            fmt: &fmt,
                            baseline_zero: false,
                            label_every: if labels.len() > 12 { 3 } else { 1 },
                        },
                    );
                } else {
                    w::card_header(ui, &t, "Spending by category", |_| {});
                    let labels: Vec<String> = d.trend.0.iter().map(|m| short_label(*m, d.trend.0.len())).collect();
                    let series: Vec<(String, egui::Color32, Vec<f32>)> = d
                        .trend
                        .1
                        .iter()
                        .map(|(c, v)| {
                            let cat = c.and_then(|c| store.category(c));
                            (
                                cat.map(|c| c.name.clone()).unwrap_or_else(|| "Other".into()),
                                cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
                                v.iter().map(|x| *x as f32).collect(),
                            )
                        })
                        .collect();
                    let r = ui.available_rect_before_wrap();
                    charts::stacked_bars(ui, &t, Id::new(("rep-trend", range)), r, &labels, &series, &fmt);
                }
            })
        })
    });

    // Category table + payees
    // Rows are 38pt plus 8pt item spacing; add the card's padding and header.
    let rows_h = 96.0 + d.cats.len().max(d.payees.len()).max(3) as f32 * 46.0;
    w::grid_row(ui, rows_h, &[1.6, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, 7 + i, rect, |ui, rect| {
            w::card_scroll(ui, &t, ("reports-lists", i), rect, |ui| {
                if i == 0 {
                    category_table(ui, &t, store, &d, base, n);
                } else {
                    payees(ui, &t, &d, base);
                }
            })
        })
    });

    // Heatmap
    w::grid_row(ui, 200.0, &[1.0], |_, ui, rect| {
        w::with_reveal(ui, shown, 9, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                w::card_header(ui, &t, "Spending calendar", |ui| {
                    ui.label(w::faint(&t, "last 12 months"));
                });
                let r = ui.available_rect_before_wrap();
                let f = move |v: i64| w::fmt_money(v, base);
                charts::heatmap(ui, &t, Id::new("rep-heat"), r, d.heat_start, &d.heat, &f);
            })
        })
    });
    let _ = Page::Reports;
}

fn short_label(m: Month, n: usize) -> String {
    if n > 12 || m.month == 1 {
        format!("{} {}", m.short(), m.year % 100)
    } else {
        m.short().to_string()
    }
}

fn category_table(ui: &mut Ui, t: &Theme, store: &Store, d: &Data, base: Cur, n: i64) {
    w::card_header(ui, t, "Categories", |_| {});
    if d.cats.is_empty() {
        w::empty_state(ui, t, ph::CHART_BAR, "No spending in this period", "");
        return;
    }
    let total: i64 = d.cats.iter().map(|x| x.1).sum();
    let wdt = ui.available_width();
    for (c, v, series) in &d.cats {
        let (r, _) = ui.allocate_exact_size(vec2(wdt, 38.0), Sense::hover());
        let cat = c.and_then(|c| store.category(c));
        let color = cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3);
        let p = ui.painter();
        let badge = Rect::from_min_size(pos2(r.left(), r.center().y - 14.0), vec2(28.0, 28.0));
        w::paint_icon_badge(
            p,
            t,
            badge,
            cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::TAG),
            color,
        );
        w::text_fit(
            p,
            pos2(badge.right() + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            store.category_name(*c),
            theme::medium(13.0),
            t.text,
            r.left() + wdt * 0.36 - badge.right() - 18.0,
        );
        let frac = *v as f32 / total.max(1) as f32;
        let bar = Rect::from_min_size(pos2(r.left() + wdt * 0.36, r.center().y - 3.0), vec2(wdt * 0.2, 6.0));
        w::paint_progress(ui, t, Id::new(("cat-share", *c)), bar, frac, None, color);
        let p = ui.painter();
        p.text(
            pos2(bar.right() + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            format!("{:.0}%", frac * 100.0),
            theme::regular(12.0),
            t.text2,
        );
        let spark = Rect::from_min_size(pos2(r.left() + wdt * 0.66, r.center().y - 10.0), vec2(wdt * 0.12, 20.0));
        charts::sparkline(ui, Id::new(("cat-spark", *c)), spark, series, color);
        let p = ui.painter();
        p.text(
            pos2(r.right(), r.center().y - 7.0),
            Align2::RIGHT_CENTER,
            w::fmt_whole(*v, base),
            theme::semibold(13.0),
            t.text,
        );
        p.text(
            pos2(r.right(), r.center().y + 9.0),
            Align2::RIGHT_CENTER,
            format!("{}/mo", w::fmt_whole(*v / n, base)),
            theme::regular(11.0),
            t.text3,
        );
    }
}

fn payees(ui: &mut Ui, t: &Theme, d: &Data, base: Cur) {
    w::card_header(ui, t, "Top payees", |_| {});
    if d.payees.is_empty() {
        w::empty_state(ui, t, ph::STOREFRONT, "No payees yet", "");
        return;
    }
    let max = d.payees.first().map(|x| x.1).unwrap_or(1).max(1);
    for (i, (name, v, count)) in d.payees.iter().enumerate() {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
        let p = ui.painter();
        w::text_fit(
            p,
            pos2(r.left(), r.center().y - 7.0),
            Align2::LEFT_CENTER,
            name,
            theme::medium(13.0),
            t.text,
            r.width() - 110.0,
        );
        p.text(
            pos2(r.left(), r.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("{count} payments"),
            theme::regular(11.0),
            t.text3,
        );
        p.text(
            pos2(r.right(), r.center().y - 7.0),
            Align2::RIGHT_CENTER,
            w::fmt_whole(*v, base),
            theme::semibold(13.0),
            t.text,
        );
        let bar = Rect::from_min_size(pos2(r.right() - 90.0, r.center().y + 7.0), vec2(90.0, 4.0));
        w::paint_progress(
            ui,
            t,
            Id::new(("payee-bar", i)),
            bar,
            *v as f32 / max as f32,
            None,
            t.accent,
        );
    }
}

fn export_summary(app: &mut App) {
    let store = &app.store;
    let today = app.today;
    let n = months_for(app.reports.range, today);
    let last = Month::of(today);
    let months: Vec<Month> = (0..n as i32).rev().map(|k| last.add(-k)).collect();
    let path = magpie_core::io::export_path(&magpie_core::downloads_dir(), "magpie-summary", "csv");
    let result = magpie_core::io::export_summary(store, &months, &path);
    match result {
        Ok(()) => app.toasts.success(format!("Saved {}", path.display())),
        Err(e) => app.toasts.error(e.to_string()),
    }
}
