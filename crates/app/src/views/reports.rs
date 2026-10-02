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

pub struct State {
    range: usize,
    /// Custom range. `None` for `to` means "today", so it keeps up with the date.
    from: Option<Date>,
    to: Option<Date>,
    memo: Memo<(u64, Date, Date), Rc<Data>>,
}

impl Default for State {
    fn default() -> Self {
        State {
            range: 2,
            from: None,
            to: None,
            memo: Memo::default(),
        }
    }
}

impl State {
    pub fn set_range(&mut self, range: usize) {
        self.range = range.min(RANGES.len() - 1);
    }
}

const RANGES: [&str; 6] = [
    "This month",
    "3 months",
    "6 months",
    "12 months",
    "Year to date",
    "Custom",
];
const CUSTOM: usize = 5;

/// How the charts bucket the period: days for about a month, weeks for a
/// quarter, months beyond that.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grain {
    Day,
    Week,
    Month,
}

struct Data {
    grain: Grain,
    days: i64,
    labels: Vec<String>,
    cashflow: Vec<Totals>,
    total: Totals,
    nw_labels: Vec<String>,
    nw: Vec<i64>,
    trend: analytics::Series,
    cats: Vec<(Option<RowId>, i64, Vec<f32>)>,
    payees: Vec<(String, i64, usize)>,
    heat_start: Date,
    heat: Vec<i64>,
}

impl Data {
    /// Spending is averaged per day for short periods and per month otherwise.
    fn per(&self) -> (&'static str, &'static str, f64) {
        if self.grain == Grain::Day {
            ("Avg. daily spend", "/day", self.days as f64)
        } else {
            ("Avg. monthly spend", "/mo", (self.days as f64 / 30.437).max(1.0))
        }
    }
}

fn day_after(d: Date) -> Date {
    d.tomorrow().unwrap_or(d)
}

fn day_before(d: Date) -> Date {
    d.yesterday().unwrap_or(d)
}

/// The selected period, inclusive.
fn period(st: &State, today: Date) -> (Date, Date) {
    let this = Month::of(today);
    let trailing = |n: i32| (this.add(1 - n).first(), today);
    match st.range {
        0 => trailing(1),
        1 => trailing(3),
        2 => trailing(6),
        3 => trailing(12),
        4 => (Date::new(today.year(), 1, 1).unwrap_or(today), today),
        _ => {
            let (from, to) = custom(st, today);
            (from.min(to), from.max(to))
        }
    }
}

fn custom(st: &State, today: Date) -> (Date, Date) {
    let from = st.from.unwrap_or_else(|| {
        today
            .checked_sub(jiff::Span::new().months(1))
            .map(day_after)
            .unwrap_or(today)
    });
    (from, st.to.unwrap_or(today))
}

fn spans(from: Date, to: Date) -> (Grain, Vec<(Date, Date)>, Vec<String>) {
    let days = (to - from).get_days() as i64 + 1;
    let grain = if days <= 45 {
        Grain::Day
    } else if days <= 140 {
        Grain::Week
    } else {
        Grain::Month
    };
    let mut spans = Vec::new();
    let mut labels = Vec::new();
    let mut d = from;
    let multi_year = from.year() != to.year();
    while d <= to {
        let end = match grain {
            Grain::Day => d,
            Grain::Week => {
                let left = 6 - d.weekday().to_monday_zero_offset() as i64;
                d.checked_add(jiff::Span::new().days(left)).unwrap_or(to)
            }
            Grain::Month => Month::of(d).last(),
        }
        .min(to);
        let m = Month::of(d);
        labels.push(match grain {
            Grain::Day if d.day() == 1 || d == from => format!("{} {}", m.short(), d.day()),
            Grain::Day => d.day().to_string(),
            Grain::Week => format!("{} {}", m.short(), d.day()),
            Grain::Month if multi_year || m.month == 1 => format!("{} {}", m.short(), m.year % 100),
            Grain::Month => m.short().to_string(),
        });
        spans.push((d, end));
        if end >= to {
            break;
        }
        d = day_after(end);
    }
    (grain, spans, labels)
}

fn build(store: &Store, from: Date, to: Date) -> Data {
    let (grain, spans, labels) = spans(from, to);
    let cashflow: Vec<Totals> = spans.iter().map(|(a, b)| analytics::totals(store, *a, *b)).collect();
    let total = analytics::totals(store, from, to);
    let cats = analytics::spending_by_category(store, from, to)
        .into_iter()
        .take(12)
        .map(|(c, v)| {
            let series = spans
                .iter()
                .map(|(a, b)| {
                    store
                        .txns_between(*a, *b)
                        .iter()
                        .filter(|t| t.category == c)
                        .map(|t| analytics::flow(store, t).1)
                        .sum::<i64>() as f32
                })
                .collect();
            (c, v, series)
        })
        .collect();
    // Net worth at the end of each bucket, starting from the day before the
    // period so even a one-bucket range draws a line.
    let mut ends = vec![day_before(from)];
    ends.extend(spans.iter().map(|x| x.1));
    let mut nw_labels = vec![match grain {
        Grain::Month => "Start".to_string(),
        _ => {
            let d = day_before(from);
            format!("{} {}", Month::of(d).short(), d.day())
        }
    }];
    nw_labels.extend(labels.iter().cloned());
    let today = magpie_core::today();
    let heat_end = to.min(today).max(from);
    let heat_start = heat_end.checked_sub(jiff::Span::new().days(364)).unwrap_or(heat_end);
    Data {
        grain,
        days: (to - from).get_days() as i64 + 1,
        cashflow,
        total,
        nw: analytics::net_worth_at(store, &ends),
        nw_labels,
        trend: analytics::category_trend_spans(store, &spans, 6),
        cats,
        payees: analytics::top_payees(store, from, to, 8),
        heat: analytics::daily_spend(store, heat_start, heat_end),
        heat_start,
        labels,
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
    if app.reports.range == CUSTOM {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let (mut from, mut to) = custom(&app.reports, today);
            ui.label(w::faint(&t, "From"));
            if w::date_field(ui, &t, Id::new("reports-from"), &mut from, 150.0) {
                app.reports.from = Some(from);
            }
            ui.add_space(6.0);
            ui.label(w::faint(&t, "Till"));
            if w::date_field(ui, &t, Id::new("reports-to"), &mut to, 150.0) {
                app.reports.to = (to != today).then_some(to);
            }
            if app.reports.to.is_some() && w::ghost(ui, &t, None, "Till today").clicked() {
                app.reports.to = None;
            }
        });
    }
    ui.add_space(14.0);
    let (from, to) = period(&app.reports, today);
    let range = (from, to);
    let d = app
        .reports
        .memo
        .get((app.store.version(), from, to), || Rc::new(build(&app.store, from, to)))
        .clone();
    let store = &app.store;
    let shown = app.shown_at;
    let (avg_label, per, units) = d.per();
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
        (avg_label, (d.total.expense as f64 / units).round() as i64, t.text),
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
                    // Early in a month one big bill dwarfs income; a "-5000%"
                    // rate says nothing useful.
                    let text = if r >= -1.0 {
                        format!("{:.0}% savings rate", r * 100.0)
                    } else {
                        format!("Spent {:.1}× income", 1.0 - r)
                    };
                    ui.label(w::faint(&t, text));
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
                    .zip(&d.labels)
                    .map(|(tt, l)| charts::BarGroup {
                        label: l.clone(),
                        values: vec![tt.income as f32, tt.expense as f32],
                    })
                    .collect();
                let net: Vec<f32> = d.cashflow.iter().map(|x| x.net() as f32).collect();
                let r = ui.available_rect_before_wrap();
                crate::marks::record(|| "rep:cash".into(), r);
                charts::grouped_bars(
                    ui,
                    &t,
                    Id::new(("rep-cash", range)),
                    r,
                    &groups,
                    &[motion::with_alpha(t.pos, 0.9), motion::with_alpha(t.neg, 0.85)],
                    &["Income", "Spending"],
                    // A daily net line just zigzags through every bill.
                    (d.grain != Grain::Day).then_some((&net[..], t.accent)),
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
                    let vals: Vec<f32> = d.nw.iter().map(|x| *x as f32).collect();
                    let labels = &d.nw_labels;
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
                            labels,
                            fmt: &fmt,
                            baseline_zero: false,
                            label_every: 1,
                        },
                    );
                } else {
                    w::card_header(ui, &t, "Spending by category", |_| {});
                    let series: Vec<(String, egui::Color32, Vec<f32>)> = d
                        .trend
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
                    charts::stacked_bars(ui, &t, Id::new(("rep-trend", range)), r, &d.labels, &series, &fmt);
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
                    category_table(ui, &t, store, &d, base, units, per);
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
                    ui.label(w::faint(&t, "12 months to the end of the period"));
                });
                let r = ui.available_rect_before_wrap();
                let f = move |v: i64| w::fmt_money(v, base);
                charts::heatmap(ui, &t, Id::new("rep-heat"), r, d.heat_start, &d.heat, &f);
            })
        })
    });
    let _ = Page::Reports;
}

fn category_table(ui: &mut Ui, t: &Theme, store: &Store, d: &Data, base: Cur, units: f64, per: &str) {
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
            format!("{}{per}", w::fmt_whole((*v as f64 / units).round() as i64, base)),
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
    let (from, to) = period(&app.reports, app.today);
    let (first, last) = (Month::of(from), Month::of(to));
    let months: Vec<Month> = std::iter::successors(Some(first), |m| (*m < last).then(|| m.next())).collect();
    let path = magpie_core::io::export_path(&magpie_core::downloads_dir(), "magpie-summary", "csv");
    let result = magpie_core::io::export_summary(store, &months, &path);
    match result {
        Ok(()) => app.toasts.success(format!("Saved {}", path.display())),
        Err(e) => app.toasts.error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn presets_end_today() {
        let today = d("2026-10-01");
        let mut st = State::default();
        let want = [
            ("2026-10-01", "2026-10-01"),
            ("2026-08-01", "2026-10-01"),
            ("2026-05-01", "2026-10-01"),
            ("2025-11-01", "2026-10-01"),
            ("2026-01-01", "2026-10-01"),
        ];
        for (i, (a, b)) in want.iter().enumerate() {
            st.set_range(i);
            assert_eq!(period(&st, today), (d(a), d(b)), "range {}", RANGES[i]);
        }
    }

    #[test]
    fn custom_defaults_till_today_and_orders_dates() {
        let today = d("2026-10-15");
        let mut st = State::default();
        st.set_range(CUSTOM);
        assert_eq!(period(&st, today), (d("2026-09-16"), today));
        st.from = Some(d("2026-12-01"));
        st.to = Some(d("2026-11-01"));
        assert_eq!(period(&st, today), (d("2026-11-01"), d("2026-12-01")));
    }

    #[test]
    fn buckets_cover_the_period_exactly() {
        for (a, b, grain, n) in [
            ("2026-10-01", "2026-10-01", Grain::Day, 1),
            ("2026-09-01", "2026-09-30", Grain::Day, 30),
            ("2026-08-01", "2026-10-01", Grain::Week, 10),
            ("2025-11-01", "2026-10-15", Grain::Month, 12),
        ] {
            let (g, spans, labels) = spans(d(a), d(b));
            assert!(g == grain, "{a}..{b}");
            assert_eq!(spans.len(), n, "{a}..{b}");
            assert_eq!(labels.len(), n);
            assert_eq!(spans[0].0, d(a));
            assert_eq!(spans[n - 1].1, d(b));
            for w in spans.windows(2) {
                assert_eq!(day_after(w[0].1), w[1].0);
            }
        }
    }
}
