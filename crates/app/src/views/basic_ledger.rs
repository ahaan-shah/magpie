//! Basic mode's Transactions: one month at a time, grouped by day, with a
//! search box. Clicking a row opens it; nothing to select, filter or bulk
//! edit.

use crate::app::{App, Memo};
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::{Id as RowId, Month, Store, Txn, analytics};
use std::collections::HashMap;
use std::rc::Rc;

pub struct State {
    month: Month,
    search: String,
    pub focus_search: bool,
    /// Set from Home's "Where it went": one category (`Some(None)` is
    /// Uncategorized) for the month.
    cat: Option<Option<RowId>>,
    rows: Memo<Key, Rc<Rows>>,
}

impl State {
    pub fn new(today: Date) -> State {
        State {
            month: Month::of(today),
            search: String::new(),
            focus_search: false,
            cat: None,
            rows: Memo::default(),
        }
    }

    pub fn show_category(&mut self, c: Option<RowId>, m: Month) {
        self.search.clear();
        self.cat = Some(c);
        self.month = m;
    }

    pub fn set_search(&mut self, q: String) {
        self.search = q;
        self.cat = None;
    }
}

#[derive(Clone, PartialEq)]
struct Key {
    version: u64,
    month: Month,
    search: String,
    cat: Option<Option<RowId>>,
}

enum Row {
    Day {
        date: Date,
        net: i64,
    },
    /// Index into `store.txns()`.
    Txn(usize),
}

struct Rows {
    rows: Vec<Row>,
    /// For each transfer leg shown, the account on the other side.
    other_leg: HashMap<RowId, RowId>,
    income: i64,
    expense: i64,
    count: usize,
}

fn build(store: &Store, k: &Key) -> Rows {
    // A search looks through every month; otherwise it's one month.
    let span = if k.search.is_empty() {
        store.range_of(k.month.first(), k.month.last())
    } else {
        0..store.txns().len()
    };
    let off = span.start;
    let slice = &store.txns()[span];
    let terms: Vec<String> = k.search.to_lowercase().split_whitespace().map(str::to_owned).collect();
    // Both legs of a transfer share a date, so the other leg is in the slice.
    let mut legs: HashMap<RowId, Vec<(RowId, RowId)>> = HashMap::new();
    for t in slice {
        if let Some(g) = t.transfer {
            legs.entry(g).or_default().push((t.id, t.account));
        }
    }
    let mut other_leg = HashMap::new();
    for pair in legs.values() {
        if let [a, b] = pair.as_slice() {
            other_leg.insert(a.0, b.1);
            other_leg.insert(b.0, a.1);
        }
    }
    let mut rows = Vec::new();
    let (mut income, mut expense, mut count) = (0, 0, 0);
    let mut day: Option<(Date, usize)> = None;
    let mut day_net = 0i64;
    for (i, t) in slice.iter().enumerate().rev() {
        if let Some(c) = k.cat
            && (t.is_transfer() || t.category != c)
        {
            continue;
        }
        if !terms.is_empty() {
            let hay = format!(
                "{} {} {} {} {}",
                t.payee,
                t.note,
                if t.is_transfer() {
                    "transfer moved"
                } else {
                    store.category_name(t.category)
                },
                store.account(t.account).map(|a| a.name.as_str()).unwrap_or(""),
                magpie_core::money::to_input(t.amount.abs(), store.account_cur(t.account)),
            )
            .to_lowercase();
            if !terms.iter().all(|term| hay.contains(term.as_str())) {
                continue;
            }
        }
        if day.map(|d| d.0) != Some(t.date) {
            if let Some((d, at)) = day {
                rows[at] = Row::Day { date: d, net: day_net };
            }
            day = Some((t.date, rows.len()));
            day_net = 0;
            rows.push(Row::Day { date: t.date, net: 0 });
        }
        let (inc, exp) = analytics::flow(store, t);
        income += inc;
        expense += exp;
        day_net += inc - exp;
        count += 1;
        rows.push(Row::Txn(off + i));
    }
    if let Some((d, at)) = day {
        rows[at] = Row::Day { date: d, net: day_net };
    }
    Rows {
        rows,
        other_leg,
        income,
        expense,
        count,
    }
}

const DAY_H: f32 = 34.0;
const ROW_H: f32 = 54.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let ctx = ui.ctx().clone();
    let today = app.today;
    let base = app.store.base();
    let st = &mut app.basic_ledger;
    let m = st.month;

    if app.keys.left && st.search.is_empty() {
        st.month = m.prev();
    }
    if app.keys.right && st.search.is_empty() {
        st.month = m.next();
    }

    // Month switcher on the left (a search covers every month), search on
    // the right.
    let searching = !st.search.trim().is_empty();
    ui.horizontal(|ui| {
        if searching {
            ui.label(
                egui::RichText::new("All months")
                    .font(theme::semibold(17.0))
                    .color(t.text),
            );
        } else {
            if w::icon_button(ui, &t, ph::CARET_LEFT, "Previous month (←)").clicked() {
                st.month = m.prev();
            }
            ui.label(egui::RichText::new(m.label()).font(theme::semibold(17.0)).color(t.text));
            if w::icon_button(ui, &t, ph::CARET_RIGHT, "Next month (→)").clicked() {
                st.month = m.next();
            }
            if m != Month::of(today) && w::ghost(ui, &t, None, "This month").clicked() {
                st.month = Month::of(today);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let id = Id::new("basic-search");
            if std::mem::take(&mut st.focus_search) {
                ctx.memory_mut(|mem| mem.request_focus(id));
            }
            let width = ui.available_width().clamp(160.0, 280.0);
            let r = w::text_field(
                ui,
                &t,
                id,
                &mut st.search,
                &format!("{}  Search", ph::MAGNIFYING_GLASS),
                width,
            );
            if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                st.search.clear();
            }
        });
    });
    ui.add_space(8.0);

    let key = Key {
        version: app.store.version(),
        month: st.month,
        search: st.search.trim().to_string(),
        cat: st.cat,
    };
    let rows = st.rows.get(key.clone(), || Rc::new(build(&app.store, &key))).clone();
    let store = &app.store;

    ui.horizontal(|ui| {
        if let Some(c) = st.cat {
            let cat = c.and_then(|c| store.category(c));
            let (glyph, color) = cat
                .map(|c| (icons::glyph(&c.icon), w::cat_color(c.color)))
                .unwrap_or((ph::TAG, t.text3));
            let chip = w::chip(
                ui,
                &t,
                Some(glyph),
                &format!("{}   {}", store.category_name(c), ph::X),
                w::readable(&t, color),
            );
            if chip
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Show everything")
                .clicked()
            {
                st.cat = None;
            }
            ui.add_space(6.0);
        }
        ui.label(
            egui::RichText::new(format!(
                "{} transaction{}",
                rows.count,
                if rows.count == 1 { "" } else { "s" }
            ))
            .font(theme::medium(12.5))
            .color(t.text2),
        );
        ui.label(w::faint(&t, "·"));
        ui.label(
            egui::RichText::new(format!("{} in", w::fmt_money(rows.income, base)))
                .font(theme::medium(12.5))
                .color(t.pos),
        );
        ui.label(w::faint(&t, "·"));
        ui.label(
            egui::RichText::new(format!("{} out", w::fmt_money(rows.expense, base)))
                .font(theme::medium(12.5))
                .color(t.text2),
        );
    });
    ui.add_space(10.0);

    let mut open: Option<RowId> = None;
    let mut add = false;
    let (alpha, dy) = w::reveal(ui, app.shown_at, 0);
    ui.add_space(dy);
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap()));
    cui.set_opacity(alpha);
    let card = w::card_frame(&t)
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(&mut cui, |ui| {
            ui.set_width(ui.available_width());
            if rows.rows.is_empty() {
                let (title, body) = if !key.search.is_empty() {
                    ("Nothing matches", "Try another word, or clear the search.")
                } else if key.cat.is_some() {
                    ("Nothing in this category", "Pick another month, or show everything.")
                } else {
                    (
                        "No transactions this month",
                        "Add what you spend and earn, and it shows up here.",
                    )
                };
                w::empty_state(ui, &t, ph::RECEIPT, title, body);
                if key.search.is_empty() && key.cat.is_none() {
                    ui.vertical_centered(|ui| {
                        if w::primary(ui, &t, Some(ph::PLUS), "Add transaction").clicked() {
                            add = true;
                        }
                    });
                    ui.add_space(12.0);
                }
                return;
            }
            let multi = store.active_accounts().nth(1).is_some();
            for (i, row) in rows.rows.iter().enumerate() {
                match row {
                    Row::Day { date, net } => {
                        if i > 0 {
                            ui.add_space(4.0);
                        }
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), DAY_H), Sense::hover());
                        if !ui.is_rect_visible(r) {
                            continue;
                        }
                        let p = ui.painter();
                        p.text(
                            pos2(r.left() + 4.0, r.center().y + 3.0),
                            Align2::LEFT_CENTER,
                            w::day_label(*date, today),
                            theme::semibold(12.5),
                            t.text2,
                        );
                        if *net != 0 {
                            p.text(
                                pos2(r.right() - 4.0, r.center().y + 3.0),
                                Align2::RIGHT_CENTER,
                                w::fmt_signed(*net, base),
                                theme::regular(12.0),
                                t.text3,
                            );
                        }
                    }
                    Row::Txn(ix) => {
                        let Some(tx) = store.txns().get(*ix) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                        if !ui.is_rect_visible(r) {
                            continue;
                        }
                        let h = motion::toggle(ui.ctx(), Id::new(("bl-row", tx.id)), resp.hovered(), motion::MICRO);
                        paint_row(ui, &t, store, tx, rows.other_leg.get(&tx.id).copied(), multi, r, h);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open = Some(tx.id);
                        }
                    }
                }
            }
        });
    ui.allocate_rect(card.response.rect, Sense::hover());

    if add {
        let f = forms::TxnForm::new(&app.store, app.today);
        app.open_modal(&ctx, Modal::Txn(f));
    }
    if let Some(id) = open
        && let Some(tx) = app.store.txn(id).cloned()
    {
        let f = forms::TxnForm::edit(&app.store, &tx);
        app.open_modal(&ctx, Modal::Txn(f));
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &Ui,
    t: &Theme,
    store: &Store,
    tx: &Txn,
    other: Option<RowId>,
    multi_account: bool,
    r: Rect,
    hover: f32,
) {
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(r, CornerRadius::same(10), motion::with_alpha(t.hover_wash(), hover));
    }
    let cur = store.account_cur(tx.account);
    let cat = tx.category.and_then(|c| store.category(c));
    let badge = Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 18.0), vec2(36.0, 36.0));
    let account_name = |id: RowId| store.account(id).map(|a| a.name.as_str()).unwrap_or("another account");
    // Transfers read as plain sentences: "Moved to Savings".
    let (glyph, color, title, sub) = if tx.is_transfer() {
        let title = match (tx.amount < 0, other) {
            (true, Some(o)) => format!("Moved to {}", account_name(o)),
            (false, Some(o)) => format!("Moved from {}", account_name(o)),
            (true, None) => "Moved out".to_string(),
            (false, None) => "Moved in".to_string(),
        };
        (
            ph::ARROWS_LEFT_RIGHT,
            t.text3,
            title,
            account_name(tx.account).to_string(),
        )
    } else {
        let title = if tx.payee.is_empty() {
            store.category_name(tx.category).to_string()
        } else {
            tx.payee.clone()
        };
        let mut sub = store.category_name(tx.category).to_string();
        if multi_account {
            sub.push_str(" · ");
            sub.push_str(account_name(tx.account));
        }
        (
            cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::TAG),
            cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
            title,
            sub,
        )
    };
    w::paint_icon_badge(p, t, badge, glyph, color);
    let x = badge.right() + 12.0;
    let text_w = r.right() - x - 150.0;
    w::text_fit(
        p,
        pos2(x, r.center().y - 9.0),
        Align2::LEFT_CENTER,
        title,
        theme::medium(13.5),
        t.text,
        text_w,
    );
    w::text_fit(
        p,
        pos2(x, r.center().y + 10.0),
        Align2::LEFT_CENTER,
        sub,
        theme::regular(12.0),
        t.text3,
        text_w,
    );
    let c = if tx.is_transfer() {
        t.text2
    } else if tx.amount > 0 {
        t.pos
    } else {
        t.text
    };
    p.text(
        pos2(r.right() - 8.0, r.center().y),
        Align2::RIGHT_CENTER,
        w::fmt_signed(tx.amount, cur),
        theme::semibold(14.0),
        c,
    );
}
