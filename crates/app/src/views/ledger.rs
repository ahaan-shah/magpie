//! Transactions: natural-language quick add, filters, a virtualized table
//! grouped by day, multi-select with bulk actions, and a detail panel.

use crate::app::{App, Memo};
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Key, Rect, Sense, Stroke, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::money::Cur;
use magpie_core::quick::{self, QuickEntry};
use magpie_core::{Id as RowId, Month, Store, Txn, analytics, receipts};
use std::collections::BTreeSet;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Range {
    ThisMonth,
    LastMonth,
    Last90,
    ThisYear,
    All,
    Month(Month),
}

impl Range {
    fn label(self) -> String {
        match self {
            Range::ThisMonth => "This month".into(),
            Range::LastMonth => "Last month".into(),
            Range::Last90 => "Last 90 days".into(),
            Range::ThisYear => "This year".into(),
            Range::All => "All time".into(),
            Range::Month(m) => m.label(),
        }
    }
    fn bounds(self, today: Date) -> (Date, Date) {
        let m = Month::of(today);
        match self {
            Range::ThisMonth => (m.first(), m.last()),
            Range::LastMonth => (m.prev().first(), m.prev().last()),
            Range::Last90 => (
                today.checked_sub(jiff::Span::new().days(89)).unwrap_or(today),
                today.checked_add(jiff::Span::new().days(365)).unwrap_or(today),
            ),
            Range::ThisYear => (
                Date::new(today.year(), 1, 1).unwrap_or(today),
                Date::new(today.year(), 12, 31).unwrap_or(today),
            ),
            Range::All => (Date::MIN, Date::MAX),
            Range::Month(m) => (m.first(), m.last()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CatFilter {
    Any,
    Uncategorized,
    Is(RowId),
}

#[derive(Clone, PartialEq, Debug)]
struct Key_ {
    version: u64,
    today: Date,
    search: String,
    range: Range,
    account: Option<RowId>,
    cat: CatFilter,
    kind: usize,
}

enum Row {
    Day { date: Date, net: i64 },
    Txn(usize),
}

struct Rows {
    rows: Vec<Row>,
    /// Visible transaction ids in display order (for shift-select/arrows).
    order: Vec<RowId>,
    income: i64,
    expense: i64,
    count: usize,
}

pub struct State {
    search: String,
    pub focus_search: bool,
    range: Range,
    account: Option<RowId>,
    cat: CatFilter,
    kind: usize,
    quick: String,
    selected: BTreeSet<RowId>,
    anchor: Option<RowId>,
    rows: Memo<Key_, Rc<Rows>>,
    scroll_to: Option<RowId>,
}

impl Default for State {
    fn default() -> Self {
        State {
            search: String::new(),
            focus_search: false,
            range: Range::All,
            account: None,
            cat: CatFilter::Any,
            kind: 0,
            quick: String::new(),
            selected: BTreeSet::new(),
            anchor: None,
            rows: Memo::default(),
            scroll_to: None,
        }
    }
}

impl State {
    pub fn selected_one(&self) -> Option<RowId> {
        if self.selected.len() == 1 {
            self.selected.iter().next().copied()
        } else {
            None
        }
    }
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }
    pub fn set_search(&mut self, q: String) {
        self.search = q;
        self.range = Range::All;
        self.cat = CatFilter::Any;
        self.account = None;
        self.kind = 0;
    }
    pub fn show_category(&mut self, c: Option<RowId>, m: Month) {
        self.search.clear();
        self.cat = match c {
            Some(id) => CatFilter::Is(id),
            None => CatFilter::Uncategorized,
        };
        self.range = Range::Month(m);
        self.account = None;
        self.kind = 0;
    }
    pub fn set_quick(&mut self, text: &str) {
        self.quick = text.to_string();
    }
    pub fn clear_quick(&mut self) {
        self.quick.clear();
    }
    pub fn show_account(&mut self, a: RowId) {
        self.search.clear();
        self.account = Some(a);
        self.range = Range::All;
        self.cat = CatFilter::Any;
        self.kind = 0;
    }
}

fn build(store: &Store, k: &Key_) -> Rows {
    let (from, to) = k.range.bounds(k.today);
    let span = store.range_of(from, to);
    let base_off = span.start;
    let slice = &store.txns()[span];
    let terms: Vec<String> = k.search.to_lowercase().split_whitespace().map(str::to_owned).collect();
    let cat_names: Vec<(RowId, String)> = store
        .categories()
        .iter()
        .map(|c| (c.id, c.name.to_lowercase()))
        .collect();
    let acc_names: Vec<(RowId, String)> = store.accounts().iter().map(|a| (a.id, a.name.to_lowercase())).collect();
    let mut rows = Vec::new();
    let mut order = Vec::new();
    let (mut income, mut expense, mut count) = (0, 0, 0);
    let mut day: Option<Date> = None;
    let mut day_idx = 0;
    let mut day_net = 0i64;
    for (i, t) in slice.iter().enumerate().rev() {
        if let Some(a) = k.account
            && t.account != a
        {
            continue;
        }
        match k.cat {
            CatFilter::Any => {}
            CatFilter::Uncategorized => {
                if t.category.is_some() || t.is_transfer() {
                    continue;
                }
            }
            CatFilter::Is(c) => {
                if t.category != Some(c) {
                    continue;
                }
            }
        }
        match k.kind {
            1 if t.is_transfer() || t.amount >= 0 => continue,
            2 if t.is_transfer() || t.amount <= 0 => continue,
            3 if !t.is_transfer() => continue,
            _ => {}
        }
        if !terms.is_empty() {
            let payee = t.payee.to_lowercase();
            let note = t.note.to_lowercase();
            let cat = t
                .category
                .and_then(|c| cat_names.iter().find(|x| x.0 == c))
                .map(|x| x.1.as_str())
                .unwrap_or("uncategorized");
            let acc = acc_names
                .iter()
                .find(|x| x.0 == t.account)
                .map(|x| x.1.as_str())
                .unwrap_or("");
            let amount = magpie_core::money::to_input(t.amount.abs(), store.account_cur(t.account));
            let ok = terms.iter().all(|term| {
                let tag = term.strip_prefix('#');
                match tag {
                    Some(tg) => t.tags.iter().any(|x| x.starts_with(tg)),
                    None => {
                        payee.contains(term)
                            || note.contains(term)
                            || cat.contains(term)
                            || acc.contains(term)
                            || amount.starts_with(term.as_str())
                            || t.tags.iter().any(|x| x.contains(term.as_str()))
                    }
                }
            });
            if !ok {
                continue;
            }
        }
        if day != Some(t.date) {
            if let Some(d) = day {
                rows[day_idx] = Row::Day { date: d, net: day_net };
            }
            day = Some(t.date);
            day_idx = rows.len();
            day_net = 0;
            rows.push(Row::Day { date: t.date, net: 0 });
        }
        let (inc, exp) = analytics::flow(store, t);
        income += inc;
        expense += exp;
        day_net += inc - exp;
        count += 1;
        rows.push(Row::Txn(base_off + i));
        order.push(t.id);
    }
    if let Some(d) = day {
        rows[day_idx] = Row::Day { date: d, net: day_net };
    }
    Rows {
        rows,
        order,
        income,
        expense,
        count,
    }
}

enum Act {
    Edit(RowId),
    Delete(Vec<RowId>),
    Recategorize(Vec<RowId>, Option<RowId>),
    ToggleCleared(Vec<RowId>),
    Attach(RowId),
    OpenReceipt(std::path::PathBuf),
    RemoveReceipt(RowId),
    MakeRecurring(RowId),
    Duplicate(RowId),
    ImportCsv,
    Export,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let ctx = ui.ctx().clone();
    let mut acts: Vec<Act> = Vec::new();
    // Everything on this page stays within the page width, even if a filter
    // row overflows (which would otherwise widen the table off-screen).
    let page = ui.max_rect();
    ui.set_clip_rect(page.intersect(ui.clip_rect()));

    quick_add(app, ui, &t);
    ui.add_space(12.0);
    filters(app, ui, &t, &mut acts);
    ui.add_space(10.0);

    let key = Key_ {
        version: app.store.version(),
        today: app.today,
        search: app.ledger.search.trim().to_string(),
        range: app.ledger.range,
        account: app.ledger.account,
        cat: app.ledger.cat,
        kind: app.ledger.kind,
    };
    let rows = app
        .ledger
        .rows
        .get(key.clone(), || Rc::new(build(&app.store, &key)))
        .clone();
    // Drop selections that are no longer visible.
    if !app.ledger.selected.is_empty() {
        let visible: BTreeSet<RowId> = rows.order.iter().copied().collect();
        app.ledger.selected.retain(|id| visible.contains(id));
    }

    // Summary
    let base = app.store.base();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} transactions", thousands(rows.count)))
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
    ui.add_space(6.0);

    let detail_open = app.ledger.selected_one().is_some();
    let k = motion::toggle(&ctx, Id::new("ledger-detail"), detail_open, motion::STANDARD);
    let avail = ui.available_rect_before_wrap();
    let full = Rect::from_min_max(avail.min, pos2(page.right().min(avail.right()), avail.bottom()));
    let panel_w = 340.0 * k;
    let table_rect = Rect::from_min_max(
        full.min,
        pos2(
            full.right() - panel_w - if k > 0.0 { 16.0 * k } else { 0.0 },
            full.bottom() - 12.0,
        ),
    );
    let mut table_ui = ui.new_child(egui::UiBuilder::new().max_rect(table_rect));
    table(app, &mut table_ui, &t, &rows, &mut acts);

    if k > 0.01 {
        let rect = Rect::from_min_max(
            pos2(full.right() - 340.0 * k, full.top()),
            pos2(full.right() + (1.0 - k) * 40.0, full.bottom() - 12.0),
        );
        let mut dui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        dui.set_opacity(k);
        if let Some(id) = app.ledger.selected_one().or(app.ledger.anchor)
            && let Some(tx) = app.store.txn(id).cloned()
        {
            detail(app, &mut dui, &t, rect, &tx, &mut acts);
        }
    }
    ui.allocate_rect(full, Sense::hover());

    if app.ledger.selected.len() > 1 {
        bulk_bar(app, &ctx, &t, full, &mut acts);
    }

    keyboard(app, &ctx, &rows, &mut acts);
    apply(app, &ctx, acts);
}

fn thousands(n: usize) -> String {
    magpie_core::money::format(
        n as i64 * 100,
        Cur::USD,
        magpie_core::money::FmtOpts {
            trim_zero_cents: true,
            no_symbol: true,
            ..Default::default()
        },
    )
}

// ------------------------------------------------------------ quick add

fn resolve(store: &Store, q: &QuickEntry, payees: &[analytics::PayeeInfo]) -> (RowId, Option<RowId>) {
    let account = q
        .account
        .as_ref()
        .and_then(|a| {
            let a = a.to_lowercase();
            store
                .active_accounts()
                .find(|x| x.name.to_lowercase().contains(&a))
                .map(|x| x.id)
        })
        .or_else(|| {
            let key = q.payee.to_lowercase();
            payees.iter().find(|p| p.name.to_lowercase() == key).map(|p| p.account)
        })
        .or_else(|| store.default_account())
        .unwrap_or(0);
    let category = match &q.category {
        Some(c) => store
            .find_category(c)
            .or_else(|| {
                store
                    .categories()
                    .iter()
                    .find(|x| x.name.to_lowercase().starts_with(&c.to_lowercase()))
            })
            .map(|c| c.id),
        None => {
            let key = q.payee.to_lowercase();
            payees
                .iter()
                .find(|p| p.name.to_lowercase() == key)
                .and_then(|p| p.category)
                .or_else(|| {
                    // A payee word that names a category ("coffee" -> Coffee).
                    q.payee
                        .split_whitespace()
                        .find_map(|w| store.find_category(w).map(|c| c.id))
                })
        }
    };
    (account, category)
}

fn quick_add(app: &mut App, ui: &mut Ui, t: &Theme) {
    let id = Id::new("quick-add");
    let focused = ui.memory(|m| m.has_focus(id));
    let k = motion::toggle(
        ui.ctx(),
        id.with("f"),
        focused || !app.ledger.quick.is_empty(),
        motion::STANDARD,
    );
    let today = app.today;
    let base_cur = app.store.base();
    let mut submit = false;
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(
            1.0,
            motion::lerp_color(t.border, motion::with_alpha(t.accent, 0.7), k),
        ))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(16, 12))
        .shadow(egui::Shadow {
            offset: [0, 4],
            blur: (16.0 * k) as u8,
            spread: 0,
            color: motion::with_alpha(t.accent, 0.15 * k),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(ph::SPARKLE)
                        .font(theme::regular(18.0))
                        .color(motion::lerp_color(t.text3, t.accent, k)),
                );
                let r = ui.add(
                    egui::TextEdit::singleline(&mut app.ledger.quick)
                        .id(id)
                        .font(theme::regular(15.0))
                        .hint_text(
                            egui::RichText::new(
                                "Quick add — try “coffee 4.50 #treats yesterday” or “+3200 @Acme salary”",
                            )
                            .color(t.text3),
                        )
                        .frame(egui::Frame::NONE)
                        .desired_width(ui.available_width() - 120.0),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
                if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    app.ledger.quick.clear();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::ghost(ui, t, Some(ph::SLIDERS_HORIZONTAL), "Details")
                        .on_hover_text("Open the full form")
                        .clicked()
                    {
                        let mut f = forms::TxnForm::new(&app.store, today);
                        if !app.ledger.quick.trim().is_empty() {
                            f = prefill(app, f);
                        }
                        let ctx = ui.ctx().clone();
                        app.open_modal(&ctx, Modal::Txn(f));
                        app.ledger.quick.clear();
                    }
                });
            });
            // Live parse preview
            if k > 0.01 && !app.ledger.quick.trim().is_empty() {
                let q = quick::parse(&app.ledger.quick, base_cur, today);
                let payees = app
                    .payees
                    .get(app.store.version(), || analytics::payee_index(&app.store))
                    .clone();
                let (acc, cat) = resolve(&app.store, &q, &payees);
                let cur = app.store.account_cur(acc);
                let q = quick::parse(&app.ledger.quick, cur, today);
                ui.add_space(8.0 * k);
                ui.horizontal_wrapped(|ui| {
                    ui.set_opacity(k);
                    match q.amount {
                        Some(a) => {
                            w::chip(
                                ui,
                                t,
                                Some(if a > 0 { ph::ARROW_DOWN_LEFT } else { ph::ARROW_UP_RIGHT }),
                                &w::fmt_signed(a, cur),
                                if a > 0 { t.pos } else { t.text },
                            );
                        }
                        None => {
                            w::chip(ui, t, Some(ph::WARNING), "add an amount", t.warn);
                        }
                    }
                    if !q.payee.is_empty() {
                        w::chip(ui, t, Some(ph::STOREFRONT), &q.payee, t.text2);
                    }
                    match cat.and_then(|c| app.store.category(c)) {
                        Some(c) => {
                            w::category_chip(ui, t, &c.name, icons::glyph(&c.icon), w::cat_color(c.color));
                        }
                        None => {
                            w::chip(ui, t, Some(ph::TAG), "Uncategorized", t.text3);
                        }
                    }
                    w::chip(
                        ui,
                        t,
                        Some(ph::CALENDAR_BLANK),
                        &w::day_label(q.date.unwrap_or(today), today),
                        t.text2,
                    );
                    w::chip(ui, t, Some(ph::WALLET), app.store.account_name(acc), t.text2);
                    for tag in &q.tags {
                        w::chip(ui, t, Some(ph::HASH), tag, t.accent);
                    }
                    if !q.note.is_empty() {
                        w::chip(ui, t, None, &q.note, t.text3);
                    }
                    ui.label(w::faint(t, "  ↵ to add"));
                });
            }
        });
    if submit {
        let payees = app
            .payees
            .get(app.store.version(), || analytics::payee_index(&app.store))
            .clone();
        let q0 = quick::parse(&app.ledger.quick, base_cur, today);
        let (acc, cat) = resolve(&app.store, &q0, &payees);
        let cur = app.store.account_cur(acc);
        let q = quick::parse(&app.ledger.quick, cur, today);
        match q.amount {
            Some(amount) if amount != 0 && acc != 0 => {
                let mut tx = Txn::blank(acc, q.date.unwrap_or(today));
                tx.amount = amount;
                tx.payee = q.payee.clone();
                tx.note = q.note.clone();
                tx.tags = q.tags.clone();
                tx.category = cat;
                if let Some(id) = app.toasts.ok(app.store.add_txn(tx)) {
                    app.toasts.undoable(format!(
                        "Added {} · {}",
                        w::fmt_signed(amount, cur),
                        if q.payee.is_empty() { "transaction" } else { &q.payee }
                    ));
                    app.ledger.quick.clear();
                    app.ledger.scroll_to = Some(id);
                    app.ledger.selected.clear();
                }
                ui.memory_mut(|m| m.request_focus(id));
            }
            _ => {
                app.toasts.error("Include an amount, e.g. “lunch 12.50”");
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
    }
}

fn prefill(app: &mut App, f: forms::TxnForm) -> forms::TxnForm {
    let today = app.today;
    let payees = app
        .payees
        .get(app.store.version(), || analytics::payee_index(&app.store))
        .clone();
    let q = quick::parse(&app.ledger.quick, app.store.base(), today);
    let (acc, cat) = resolve(&app.store, &q, &payees);
    let mut tx = Txn::blank(acc, q.date.unwrap_or(today));
    tx.amount = q.amount.unwrap_or(0);
    tx.payee = q.payee;
    tx.note = q.note;
    tx.tags = q.tags;
    tx.category = cat;
    let _ = f;
    forms::TxnForm::prefilled(&app.store, &tx)
}

// ------------------------------------------------------------ filters

fn filters(app: &mut App, ui: &mut Ui, t: &Theme, acts: &mut Vec<Act>) {
    let store = &app.store;
    let today = app.today;
    let st = &mut app.ledger;
    let sid = Id::new("ledger-search");
    if st.focus_search {
        ui.memory_mut(|m| m.request_focus(sid));
        st.focus_search = false;
    }
    let hint = format!("{}  Search payee, note, #tag, amount…", ph::MAGNIFYING_GLASS);
    // One row when there's room; two deliberate rows otherwise (wrapping
    // mis-measures dropdowns and pushed them off the edge).
    if ui.available_width() >= 1180.0 {
        ui.horizontal(|ui| {
            w::text_field(ui, t, sid, &mut st.search, &hint, 280.0);
            filter_dropdowns(ui, store, st, today);
            filter_actions(ui, t, st, acts);
        });
    } else {
        ui.horizontal(|ui| {
            let w_search = (ui.available_width() - 460.0).clamp(200.0, 420.0);
            w::text_field(ui, t, sid, &mut st.search, &hint, w_search);
            filter_actions(ui, t, st, acts);
        });
        ui.horizontal(|ui| filter_dropdowns(ui, store, st, today));
    }
}

fn filter_dropdowns(ui: &mut Ui, store: &Store, st: &mut State, today: Date) {
    let app_today = today;
    let range_label = format!("{}  {}", ph::CALENDAR_BLANK, st.range.label());
    w::dropdown(ui, "ledger-range", range_label, 150.0, |ui| {
        for r in [
            Range::ThisMonth,
            Range::LastMonth,
            Range::Last90,
            Range::ThisYear,
            Range::All,
        ] {
            w::option_value(ui, &mut st.range, r, r.label());
        }
        ui.separator();
        let now = Month::of(app_today);
        for k in 0..12 {
            let m = now.add(-k);
            w::option_value(ui, &mut st.range, Range::Month(m), m.label());
        }
    });

    let acc_label = st
        .account
        .and_then(|a| store.account(a))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "All accounts".into());
    w::dropdown(ui, "ledger-acc", format!("{}  {acc_label}", ph::WALLET), 160.0, |ui| {
        w::option_value(ui, &mut st.account, None, "All accounts");
        for a in store.accounts() {
            w::option_value(
                ui,
                &mut st.account,
                Some(a.id),
                format!("{}  {}", icons::account(a), a.name),
            );
        }
    });

    let cat_label = match st.cat {
        CatFilter::Any => "All categories".to_string(),
        CatFilter::Uncategorized => "Uncategorized".into(),
        CatFilter::Is(c) => store.category_name(Some(c)).to_string(),
    };
    w::dropdown(ui, "ledger-cat", format!("{}  {cat_label}", ph::TAG), 160.0, |ui| {
        w::option_value(ui, &mut st.cat, CatFilter::Any, "All categories");
        w::option_value(ui, &mut st.cat, CatFilter::Uncategorized, "Uncategorized");
        for c in store.categories() {
            w::option_value(
                ui,
                &mut st.cat,
                CatFilter::Is(c.id),
                format!("{}  {}", icons::glyph(&c.icon), c.name),
            );
        }
    });
}

fn filter_actions(ui: &mut Ui, t: &Theme, st: &mut State, acts: &mut Vec<Act>) {
    w::segmented(
        ui,
        t,
        Id::new("ledger-kind"),
        &mut st.kind,
        &["All", "Out", "In", "Transfers"],
    );
    let filtered = !st.search.is_empty()
        || st.range != Range::All
        || st.account.is_some()
        || st.cat != CatFilter::Any
        || st.kind != 0;
    if filtered && w::ghost(ui, t, Some(ph::X), "Clear").clicked() {
        st.search.clear();
        st.range = Range::All;
        st.account = None;
        st.cat = CatFilter::Any;
        st.kind = 0;
    }
    if w::icon_button(ui, t, ph::UPLOAD_SIMPLE, "Import a bank statement (CSV, Excel, OFX)").clicked() {
        acts.push(Act::ImportCsv);
    }
    if w::icon_button(ui, t, ph::DOWNLOAD_SIMPLE, "Export these transactions (CSV)").clicked() {
        acts.push(Act::Export);
    }
}

// -------------------------------------------------------------- table

const ROW_H: f32 = 44.0;

fn table(app: &mut App, ui: &mut Ui, t: &Theme, rows: &Rows, acts: &mut Vec<Act>) {
    let rect = ui.max_rect();
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS),
        t.card,
        Stroke::new(1.0, t.border),
        egui::StrokeKind::Inside,
    );
    if rows.rows.is_empty() {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)));
        c.add_space(40.0);
        let (title, body) = if app.store.txns().is_empty() {
            (
                "No transactions yet",
                "Type one into quick add above, or import a statement from your bank.",
            )
        } else {
            ("Nothing matches", "Try a different search or clear the filters.")
        };
        w::empty_state(&mut c, t, ph::RECEIPT, title, body);
        return;
    }
    // Header
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 38.0));
    let cols = columns(header.shrink2(vec2(18.0, 0.0)));
    let p = ui.painter();
    for (label, r, right) in [
        ("Payee", cols.payee, false),
        ("Category", cols.cat, false),
        ("Account", cols.acc, false),
        ("Amount", cols.amt, true),
    ] {
        let (pos, align) = if right {
            (pos2(r.right(), r.center().y), Align2::RIGHT_CENTER)
        } else {
            (pos2(r.left(), r.center().y), Align2::LEFT_CENTER)
        };
        p.text(pos, align, label.to_uppercase(), theme::medium(10.5), t.text3);
    }
    p.hline(rect.x_range(), header.bottom(), Stroke::new(1.0, t.border));

    let body = Rect::from_min_max(pos2(rect.left(), header.bottom() + 1.0), rect.max - vec2(0.0, 1.0));
    let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(body));
    bui.set_clip_rect(body);
    let store = &app.store;
    let today = app.today;
    let mut scroll = crate::widgets::scroll_area()
        .id_salt("ledger-scroll")
        .auto_shrink([false, false]);
    if let Some(target) = app.ledger.scroll_to.take()
        && let Some(pos) = rows
            .rows
            .iter()
            .position(|r| matches!(r, Row::Txn(i) if store.txns()[*i].id == target))
    {
        let y = (pos as f32 * ROW_H - body.height() / 2.0).max(0.0);
        scroll = scroll.vertical_scroll_offset(y);
        app.ledger.selected.insert(target);
        app.ledger.anchor = Some(target);
    }
    let sel = &mut app.ledger.selected;
    let anchor = &mut app.ledger.anchor;
    let mut clicked: Option<(RowId, egui::Modifiers)> = None;
    scroll.show_rows(&mut bui, ROW_H, rows.rows.len(), |ui, range| {
        for i in range {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            match &rows.rows[i] {
                Row::Day { date, net } => {
                    let p = ui.painter();
                    let inner = r.shrink2(vec2(18.0, 0.0));
                    p.rect_filled(r, CornerRadius::ZERO, motion::with_alpha(t.bg, 0.45));
                    p.text(
                        pos2(inner.left(), r.center().y + 3.0),
                        Align2::LEFT_CENTER,
                        w::day_label(*date, today).to_uppercase(),
                        theme::semibold(11.0),
                        t.text2,
                    );
                    let c = if *net > 0 { t.pos } else { t.text3 };
                    p.text(
                        pos2(inner.right(), r.center().y + 3.0),
                        Align2::RIGHT_CENTER,
                        w::fmt_signed(*net, store.base()),
                        theme::medium(11.5),
                        c,
                    );
                }
                Row::Txn(idx) => {
                    let tx = &store.txns()[*idx];
                    let is_sel = sel.contains(&tx.id);
                    let hover = motion::toggle(ui.ctx(), Id::new(("lrow", tx.id)), resp.hovered(), motion::MICRO);
                    let selk = motion::toggle(ui.ctx(), Id::new(("lsel", tx.id)), is_sel, motion::MICRO);
                    paint_row(ui, t, store, tx, r, hover, selk);
                    if resp.clicked() {
                        clicked = Some((tx.id, ui.input(|i| i.modifiers)));
                    }
                    if resp.double_clicked() {
                        acts.push(Act::Edit(tx.id));
                    }
                    resp.context_menu(|ui| {
                        if ui.button(format!("{}  Edit", ph::PENCIL_SIMPLE)).clicked() {
                            acts.push(Act::Edit(tx.id));
                        }
                        if !tx.is_transfer() && ui.button(format!("{}  Duplicate", ph::COPY)).clicked() {
                            acts.push(Act::Duplicate(tx.id));
                        }
                        if ui
                            .button(format!(
                                "{}  {}",
                                ph::CHECK_CIRCLE,
                                if tx.cleared { "Mark uncleared" } else { "Mark cleared" }
                            ))
                            .clicked()
                        {
                            acts.push(Act::ToggleCleared(vec![tx.id]));
                        }
                        if !tx.is_transfer() && ui.button(format!("{}  Make recurring", ph::ARROWS_CLOCKWISE)).clicked()
                        {
                            acts.push(Act::MakeRecurring(tx.id));
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new(format!("{}  Delete", ph::TRASH)).color(t.neg))
                            .clicked()
                        {
                            acts.push(Act::Delete(vec![tx.id]));
                        }
                    });
                }
            }
        }
    });
    if let Some((id, m)) = clicked {
        if m.shift
            && let Some(a) = *anchor
        {
            let (ia, ib) = (
                rows.order.iter().position(|x| *x == a),
                rows.order.iter().position(|x| *x == id),
            );
            if let (Some(ia), Some(ib)) = (ia, ib) {
                let (lo, hi) = (ia.min(ib), ia.max(ib));
                sel.extend(rows.order[lo..=hi].iter().copied());
            }
        } else if m.command {
            if !sel.remove(&id) {
                sel.insert(id);
            }
            *anchor = Some(id);
        } else if sel.len() == 1 && sel.contains(&id) {
            sel.clear();
        } else {
            sel.clear();
            sel.insert(id);
            *anchor = Some(id);
        }
    }
}

struct Cols {
    payee: Rect,
    cat: Rect,
    acc: Rect,
    amt: Rect,
}

fn columns(r: Rect) -> Cols {
    let amt_w = 130.0;
    let narrow = r.width() < 640.0;
    let acc_w = if narrow { 0.0 } else { 150.0 };
    let cat_w = 180.0;
    let payee_w = (r.width() - amt_w - acc_w - cat_w - 24.0).max(120.0);
    let mut x = r.left();
    let mut take = |w: f32, gap: f32| {
        let rr = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
        x += w + gap;
        rr
    };
    let payee = take(payee_w, 12.0);
    let cat = take(cat_w, 12.0);
    let acc = take(acc_w, 0.0);
    let amt = Rect::from_min_max(pos2(r.right() - amt_w, r.top()), r.max);
    Cols { payee, cat, acc, amt }
}

fn paint_row(ui: &Ui, t: &Theme, store: &Store, tx: &Txn, r: Rect, hover: f32, sel: f32) {
    let p = ui.painter();
    let bg = motion::lerp_color(motion::with_alpha(t.hover_wash(), hover), t.accent_soft(), sel);
    if hover > 0.0 || sel > 0.0 {
        p.rect_filled(r.shrink2(vec2(6.0, 2.0)), CornerRadius::same(9), bg);
    }
    if sel > 0.0 {
        let bar = Rect::from_min_size(pos2(r.left() + 6.0, r.top() + 12.0), vec2(3.0, r.height() - 24.0));
        p.rect_filled(bar, CornerRadius::same(2), motion::with_alpha(t.accent, sel));
    }
    let cols = columns(r.shrink2(vec2(18.0, 0.0)));
    let cur = store.account_cur(tx.account);
    let cat = tx.category.and_then(|c| store.category(c));
    // Payee + note
    let badge = Rect::from_min_size(pos2(cols.payee.left(), r.center().y - 15.0), vec2(30.0, 30.0));
    let (glyph, color) = if tx.is_transfer() {
        (ph::ARROWS_LEFT_RIGHT, t.text3)
    } else {
        (
            cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::TAG),
            cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
        )
    };
    w::paint_icon_badge(p, t, badge, glyph, color);
    let text_x = badge.right() + 12.0;
    let clip = p.with_clip_rect(Rect::from_min_max(
        pos2(text_x, r.top()),
        pos2(cols.payee.right(), r.bottom()),
    ));
    let payee = if tx.payee.is_empty() { "—" } else { &tx.payee };
    let has_sub = !tx.note.is_empty() || !tx.tags.is_empty();
    let y = if has_sub { r.center().y - 8.0 } else { r.center().y };
    let g = clip.layout_no_wrap(payee.to_string(), theme::medium(13.5), t.text);
    let payee_w = g.size().x;
    clip.galley(pos2(text_x, y - g.size().y / 2.0), g, t.text);
    let mut icons_x = text_x + payee_w + 8.0;
    if !tx.cleared {
        clip.text(
            pos2(icons_x, y),
            Align2::LEFT_CENTER,
            ph::CLOCK,
            theme::regular(12.0),
            t.warn,
        );
        icons_x += 16.0;
    }
    if tx.recurring.is_some() {
        clip.text(
            pos2(icons_x, y),
            Align2::LEFT_CENTER,
            ph::ARROWS_CLOCKWISE,
            theme::regular(12.0),
            t.text3,
        );
        icons_x += 16.0;
    }
    if store.has_receipt(tx.id) {
        clip.text(
            pos2(icons_x, y),
            Align2::LEFT_CENTER,
            ph::PAPERCLIP,
            theme::regular(12.0),
            t.text3,
        );
    }
    if has_sub {
        let mut sub = tx.note.clone();
        for tag in &tx.tags {
            if !sub.is_empty() {
                sub.push_str("  ");
            }
            sub.push('#');
            sub.push_str(tag);
        }
        clip.text(
            pos2(text_x, r.center().y + 9.0),
            Align2::LEFT_CENTER,
            sub,
            theme::regular(11.5),
            t.text3,
        );
    }
    // Category pill
    let cat_clip = p.with_clip_rect(cols.cat);
    let (name, c) = if tx.is_transfer() {
        ("Transfer".to_string(), t.text3)
    } else {
        match cat {
            Some(c) => (c.name.clone(), w::readable(t, w::cat_color(c.color))),
            None => ("Uncategorized".into(), t.text3),
        }
    };
    let g = cat_clip.layout_no_wrap(name, theme::medium(12.0), c);
    let pill = Rect::from_min_size(
        pos2(cols.cat.left(), r.center().y - 12.0),
        vec2((g.size().x + 20.0).min(cols.cat.width()), 24.0),
    );
    cat_clip.rect_filled(pill, CornerRadius::same(12), t.tint(c, if t.dark { 0.14 } else { 0.1 }));
    cat_clip.galley(pos2(pill.left() + 10.0, pill.center().y - g.size().y / 2.0), g, c);
    // Account
    if cols.acc.width() > 0.0 {
        let acc_clip = p.with_clip_rect(cols.acc);
        acc_clip.text(
            pos2(cols.acc.left(), r.center().y),
            Align2::LEFT_CENTER,
            store.account_name(tx.account),
            theme::regular(12.5),
            t.text2,
        );
    }
    // Amount
    let amt_c = if tx.is_transfer() {
        t.text2
    } else if tx.amount > 0 {
        t.pos
    } else {
        t.text
    };
    p.text(
        pos2(cols.amt.right(), r.center().y),
        Align2::RIGHT_CENTER,
        w::fmt_signed(tx.amount, cur),
        theme::semibold(13.5),
        amt_c,
    );
}

// ------------------------------------------------------------- detail

fn detail(app: &mut App, ui: &mut Ui, t: &Theme, rect: Rect, tx: &Txn, acts: &mut Vec<Act>) {
    let store = &app.store;
    let cur = store.account_cur(tx.account);
    let receipts: Vec<magpie_core::Receipt> = store.receipts_for(tx.id).cloned().collect();
    w::card_in(ui, t, rect, |ui| {
        crate::widgets::scroll_area()
            .id_salt("detail-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let cat = tx.category.and_then(|c| store.category(c));
                    let (glyph, color) = if tx.is_transfer() {
                        (ph::ARROWS_LEFT_RIGHT, t.text3)
                    } else {
                        (
                            cat.map(|c| icons::glyph(&c.icon)).unwrap_or(ph::TAG),
                            cat.map(|c| w::cat_color(c.color)).unwrap_or(t.text3),
                        )
                    };
                    w::icon_badge(ui, t, glyph, color, 42.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button(ui, t, ph::X, "Close").clicked() {
                            app.ledger.selected.clear();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(if tx.payee.is_empty() { "—" } else { &tx.payee })
                        .font(theme::semibold(17.0))
                        .color(t.text),
                );
                let c = if tx.amount > 0 { t.pos } else { t.text };
                ui.label(
                    egui::RichText::new(w::fmt_signed(tx.amount, cur))
                        .font(theme::display(30.0))
                        .color(c),
                );
                if cur != store.base() {
                    ui.label(w::faint(
                        t,
                        format!("≈ {}", w::fmt_money(store.txn_base(tx), store.base())),
                    ));
                }
                ui.add_space(12.0);
                let field = |ui: &mut Ui, label: &str, value: egui::RichText| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            vec2(86.0, 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| ui.label(w::faint(t, label)),
                        );
                        ui.label(value);
                    });
                };
                field(ui, "Date", egui::RichText::new(w::fmt_date(tx.date)).color(t.text));
                field(
                    ui,
                    "Account",
                    egui::RichText::new(store.account_name(tx.account)).color(t.text),
                );
                if tx.is_transfer() {
                    let other = store.txns().iter().find(|x| x.transfer == tx.transfer && x.id != tx.id);
                    if let Some(o) = other {
                        field(
                            ui,
                            if tx.amount < 0 { "To" } else { "From" },
                            egui::RichText::new(store.account_name(o.account)).color(t.text),
                        );
                    }
                } else {
                    field(
                        ui,
                        "Category",
                        egui::RichText::new(store.category_name(tx.category)).color(t.text),
                    );
                }
                field(
                    ui,
                    "Status",
                    egui::RichText::new(if tx.cleared { "Cleared" } else { "Pending" }).color(if tx.cleared {
                        t.text
                    } else {
                        t.warn
                    }),
                );
                if let Some(r) = tx.recurring.and_then(|r| store.rule(r)) {
                    field(
                        ui,
                        "Repeats",
                        egui::RichText::new(magpie_core::recurring::describe(r)).color(t.text),
                    );
                }
                if !tx.tags.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui_with_layout(
                            vec2(86.0, 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| ui.label(w::faint(t, "Tags")),
                        );
                        for tag in &tx.tags {
                            w::chip(ui, t, Some(ph::HASH), tag, t.accent);
                        }
                    });
                }
                if !tx.note.is_empty() {
                    ui.add_space(6.0);
                    ui.label(w::faint(t, "Note"));
                    ui.label(egui::RichText::new(&tx.note).color(t.text));
                }
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    if w::secondary(ui, t, Some(ph::PENCIL_SIMPLE), "Edit").clicked() {
                        acts.push(Act::Edit(tx.id));
                    }
                    if !tx.is_transfer() && w::ghost(ui, t, Some(ph::COPY), "").on_hover_text("Duplicate").clicked() {
                        acts.push(Act::Duplicate(tx.id));
                    }
                    if !tx.is_transfer()
                        && tx.recurring.is_none()
                        && w::ghost(ui, t, Some(ph::ARROWS_CLOCKWISE), "")
                            .on_hover_text("Make recurring")
                            .clicked()
                    {
                        acts.push(Act::MakeRecurring(tx.id));
                    }
                    if w::ghost(ui, t, Some(ph::TRASH), "").on_hover_text("Delete").clicked() {
                        acts.push(Act::Delete(vec![tx.id]));
                    }
                });
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Receipts")
                            .font(theme::semibold(13.5))
                            .color(t.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button(ui, t, ph::PAPERCLIP, "Attach a receipt").clicked() {
                            acts.push(Act::Attach(tx.id));
                        }
                    });
                });
                if receipts.is_empty() {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::click());
                    let h = motion::toggle(ui.ctx(), Id::new("drop-hint"), resp.hovered(), motion::MICRO);
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::same(10),
                        Stroke::new(1.0, motion::lerp_color(t.border, t.accent, h)),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        format!("{}  Drop a photo or PDF, or click", ph::UPLOAD_SIMPLE),
                        theme::regular(12.5),
                        t.text3,
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        acts.push(Act::Attach(tx.id));
                    }
                }
                for r in &receipts {
                    let Some(path) = receipts::path_of(&app.store, r) else {
                        continue;
                    };
                    ui.add_space(6.0);
                    if receipts::is_image(r) {
                        match app.receipts.get(ui.ctx(), &r.hash, path.clone()) {
                            Some(tex) => {
                                let wdt = ui.available_width();
                                let size = tex.size_vec2();
                                let h = (wdt * size.y / size.x).min(260.0);
                                let resp = ui.add(
                                    egui::Image::new(&tex)
                                        .fit_to_exact_size(vec2(wdt, h))
                                        .corner_radius(10)
                                        .sense(Sense::click()),
                                );
                                if resp
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text("Open")
                                    .clicked()
                                {
                                    acts.push(Act::OpenReceipt(path.clone()));
                                }
                            }
                            None => {
                                let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                                ui.painter().rect_filled(rr, CornerRadius::same(10), t.hover);
                                ui.painter().text(
                                    rr.center(),
                                    Align2::CENTER_CENTER,
                                    "Loading…",
                                    theme::regular(12.0),
                                    t.text3,
                                );
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        let icon = if receipts::is_image(r) { ph::IMAGE } else { ph::FILE_PDF };
                        if ui.link(format!("{icon}  {}", r.name)).clicked() {
                            acts.push(Act::OpenReceipt(path.clone()));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if w::icon_button(ui, t, ph::TRASH, "Remove receipt").clicked() {
                                acts.push(Act::RemoveReceipt(r.id));
                            }
                        });
                    });
                }
            });
    });
}

fn bulk_bar(app: &mut App, ctx: &egui::Context, t: &Theme, area: Rect, acts: &mut Vec<Act>) {
    let n = app.ledger.selected.len();
    let ids: Vec<RowId> = app.ledger.selected.iter().copied().collect();
    let total: i64 = ids
        .iter()
        .filter_map(|id| app.store.txn(*id))
        .map(|x| app.store.txn_base(x))
        .sum();
    let base = app.store.base();
    let p = motion::toggle(ctx, Id::new("bulk-bar"), true, motion::STANDARD);
    let pos = pos2(area.center().x - 290.0, area.bottom() - 70.0 + (1.0 - p) * 20.0);
    egui::Area::new(Id::new("bulk"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(t.elevated)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .shadow(egui::Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: t.shadow(),
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{n} selected"))
                                .font(theme::semibold(13.5))
                                .color(t.text),
                        );
                        ui.label(w::subtle(t, format!("· {}", w::fmt_signed(total, base))));
                        ui.add_space(8.0);
                        let mut pick: Option<RowId> = None;
                        let mut chosen = false;
                        w::dropdown(ui, "bulk-cat", format!("{}  Categorize", ph::TAG), 150.0, |ui| {
                            if w::option(ui, false, "Uncategorized").clicked() {
                                chosen = true;
                            }
                            for c in app.store.categories().iter().filter(|c| !c.archived) {
                                if w::option(ui, false, format!("{}  {}", icons::glyph(&c.icon), c.name)).clicked() {
                                    pick = Some(c.id);
                                    chosen = true;
                                }
                            }
                        });
                        if chosen {
                            acts.push(Act::Recategorize(ids.clone(), pick));
                        }
                        if w::ghost(ui, t, Some(ph::CHECK_CIRCLE), "Cleared").clicked() {
                            acts.push(Act::ToggleCleared(ids.clone()));
                        }
                        if w::danger(ui, t, Some(ph::TRASH), "Delete").clicked() {
                            acts.push(Act::Delete(ids.clone()));
                        }
                        if w::icon_button(ui, t, ph::X, "Clear selection").clicked() {
                            app.ledger.clear_selection();
                        }
                    });
                });
        });
}

fn keyboard(app: &mut App, ctx: &egui::Context, rows: &Rows, acts: &mut Vec<Act>) {
    if app.modal.is_some() || app.palette.open || ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let k = app.keys;
    let (backspace, esc, all) = ctx.input(|i| {
        (
            i.key_pressed(Key::Backspace),
            i.key_pressed(Key::Escape),
            i.modifiers.command && i.key_pressed(Key::A),
        )
    });
    if ctx.input(|i| (i.key_pressed(Key::N) && !i.modifiers.any()) || i.key_pressed(Key::Slash)) {
        ctx.memory_mut(|m| m.request_focus(Id::new("quick-add")));
        return;
    }
    // ← / → step the date filter a month at a time.
    if k.left || k.right {
        let now = Month::of(app.today);
        let cur = match app.ledger.range {
            Range::Month(m) => m,
            Range::LastMonth => now.prev(),
            _ => now,
        };
        let next = if k.left { cur.prev() } else { cur.next() };
        app.ledger.range = if next > now { Range::All } else { Range::Month(next) };
    }
    let st = &mut app.ledger;
    if all {
        st.selected = rows.order.iter().copied().collect();
    }
    if esc {
        st.clear_selection();
    }
    let n = rows.order.len();
    let cur = st.anchor.and_then(|a| rows.order.iter().position(|x| *x == a));
    let target = if k.down {
        Some(cur.map(|i| i + 1).unwrap_or(0))
    } else if k.up {
        Some(cur.map(|i| i.saturating_sub(1)).unwrap_or(0))
    } else if k.page_down {
        Some(cur.map(|i| i + 12).unwrap_or(0))
    } else if k.page_up {
        Some(cur.map(|i| i.saturating_sub(12)).unwrap_or(0))
    } else if k.home {
        Some(0)
    } else if k.end {
        Some(n.saturating_sub(1))
    } else {
        None
    };
    if let Some(t) = target
        && let Some(id) = rows.order.get(t.min(n.saturating_sub(1)))
    {
        st.selected.clear();
        st.selected.insert(*id);
        st.anchor = Some(*id);
        st.scroll_to = Some(*id);
    }
    if (k.enter || k.edit)
        && let Some(id) = st.selected_one()
    {
        acts.push(Act::Edit(id));
    }
    if (k.delete || backspace) && !st.selected.is_empty() {
        acts.push(Act::Delete(st.selected.iter().copied().collect()));
    }
}

fn apply(app: &mut App, ctx: &egui::Context, acts: Vec<Act>) {
    for a in acts {
        match a {
            Act::Edit(id) => {
                if let Some(tx) = app.store.txn(id).cloned() {
                    let f = forms::TxnForm::edit(&app.store, &tx);
                    app.open_modal(ctx, Modal::Txn(f));
                }
            }
            Act::Duplicate(id) => {
                if let Some(mut tx) = app.store.txn(id).cloned() {
                    tx.id = 0;
                    tx.date = app.today;
                    tx.recurring = None;
                    if let Some(new) = app.toasts.ok(app.store.add_txn(tx)) {
                        app.toasts.undoable("Duplicated to today");
                        app.ledger.scroll_to = Some(new);
                        app.ledger.selected.clear();
                    }
                }
            }
            Act::Delete(ids) => {
                if let Some(n) = app.toasts.ok(app.store.delete_txns(&ids)) {
                    app.toasts
                        .undoable(format!("Deleted {n} transaction{}", if n == 1 { "" } else { "s" }));
                    app.ledger.clear_selection();
                }
            }
            Act::Recategorize(ids, cat) => {
                let n = ids.len();
                let r = app.store.bulk_update(&ids, "Recategorize", |x| {
                    if !x.is_transfer() {
                        x.category = cat;
                    }
                });
                if app.toasts.ok(r).is_some() {
                    app.toasts
                        .undoable(format!("Moved {n} to {}", app.store.category_name(cat)));
                }
            }
            Act::ToggleCleared(ids) => {
                let all_cleared = ids.iter().filter_map(|id| app.store.txn(*id)).all(|x| x.cleared);
                let r = app
                    .store
                    .bulk_update(&ids, "Change status", |x| x.cleared = !all_cleared);
                app.toasts.ok(r);
            }
            Act::Attach(id) => {
                app.dialogs.pick(
                    ctx,
                    crate::dialogs::Purpose::Attach(id),
                    "Attach a receipt",
                    ("Receipts", receipts::ALLOWED),
                );
            }
            Act::OpenReceipt(p) => crate::app::open_external(&p),
            Act::RemoveReceipt(id) => {
                let r = receipts::remove(&mut app.store, id);
                app.toasts.ok(r);
            }
            Act::MakeRecurring(id) => {
                if let Some(tx) = app.store.txn(id).cloned() {
                    let f = forms::RuleForm::from_txn(&app.store, &tx, app.today);
                    app.open_modal(ctx, Modal::Rule(f));
                }
            }
            Act::ImportCsv => {
                app.dialogs.pick(
                    ctx,
                    crate::dialogs::Purpose::ImportCsv,
                    "Import transactions",
                    ("Bank statements", magpie_core::statement::EXTENSIONS),
                );
            }
            Act::Export => {
                let k = &app.ledger;
                let key = Key_ {
                    version: 0,
                    today: app.today,
                    search: k.search.trim().to_string(),
                    range: k.range,
                    account: k.account,
                    cat: k.cat,
                    kind: k.kind,
                };
                let rows = build(&app.store, &key);
                let mut txns: Vec<Txn> = rows
                    .rows
                    .iter()
                    .filter_map(|r| {
                        if let Row::Txn(i) = r {
                            Some(app.store.txns()[*i].clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                txns.reverse();
                let path = magpie_core::io::export_path(&magpie_core::downloads_dir(), "magpie-transactions", "csv");
                match magpie_core::io::export(&app.store, &txns, magpie_core::io::Format::Csv, &path) {
                    Ok(()) => app
                        .toasts
                        .success(format!("Exported {} rows to {}", txns.len(), path.display())),
                    Err(e) => app.toasts.error(e.to_string()),
                }
            }
        }
    }
}
