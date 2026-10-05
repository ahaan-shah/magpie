//! Modal dialogs: create/edit forms and confirmations. Modals slide + fade
//! in, and fade out when closed.

use crate::app::App;
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets::{self as w, fmt_money};
use egui::{Align2, Color32, CornerRadius, Id, Margin, Stroke, Ui, vec2};
use jiff::civil::Date;
use magpie_core::io::{CsvMapping, CsvPreview, DateFormat};
use magpie_core::money::{self, Cur};
use magpie_core::{
    Account, AccountKind, Category, CategoryKind, Contribution, Freq, Goal, Id as RowId, RecurringRule, Store, Txn,
    analytics, recurring,
};
use std::path::PathBuf;

pub enum Modal {
    Txn(TxnForm),
    Account(AccountForm),
    Rule(RuleForm),
    Goal(GoalForm),
    Contribution(ContribForm),
    Category(CategoryForm),
    Import(Box<ImportForm>),
    Confirm(Confirm),
    Help,
    Currency(CurrencyForm),
    WhatsNew,
}

impl Modal {
    pub fn name(&self) -> &'static str {
        match self {
            Modal::Txn(_) => "transaction",
            Modal::Account(_) => "account",
            Modal::Rule(_) => "recurring",
            Modal::Goal(_) => "goal",
            Modal::Contribution(_) => "contribution",
            Modal::Category(_) => "category",
            Modal::Import(_) => "import",
            Modal::Confirm(_) => "confirm",
            Modal::Help => "help",
            Modal::Currency(_) => "currency",
            Modal::WhatsNew => "whats-new",
        }
    }
}

#[derive(PartialEq)]
enum Outcome {
    Keep,
    Close,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut modal) = app.modal.take() else { return };
    let t = app.t();
    let now = ctx.input(|i| i.time);
    let closing = app.modal_closing;
    let p = match closing {
        Some(c) => 1.0 - motion::ease_out(((now - c) / 0.18) as f32),
        None => motion::appear(ctx, app.modal_at, 0.0, 0.28),
    };
    if closing.is_some() {
        if p <= 0.01 {
            app.modal_closing = None;
            return;
        }
        ctx.request_repaint();
    }
    let width = match &modal {
        Modal::Import(_) => 720.0,
        Modal::Confirm(_) => 400.0,
        Modal::Help => 760.0,
        Modal::Goal(_) | Modal::Rule(_) | Modal::Txn(_) => 520.0,
        Modal::Account(_) => 720.0,
        Modal::WhatsNew => 500.0,
        _ => 460.0,
    };
    let area =
        egui::Modal::default_area(Id::new("modal-area")).anchor(Align2::CENTER_CENTER, vec2(0.0, (1.0 - p) * 22.0));
    let frame = egui::Frame::new()
        .fill(t.elevated)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::same(24))
        .shadow(egui::Shadow {
            offset: [0, 18],
            blur: 48,
            spread: 0,
            color: t.shadow(),
        })
        .multiply_with_opacity(p);
    let resp = egui::Modal::new(Id::new("modal"))
        .area(area)
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha((if t.dark { 150.0 } else { 90.0 } * p) as u8))
        .show(ctx, |ui| {
            ui.set_opacity(p);
            ui.set_width(width);
            if closing.is_some() {
                ui.disable();
            }
            match &mut modal {
                Modal::Txn(f) => f.ui(app, ui, &t),
                Modal::Account(f) => f.ui(app, ui, &t),
                Modal::Rule(f) => f.ui(app, ui, &t),
                Modal::Goal(f) => f.ui(app, ui, &t),
                Modal::Contribution(f) => f.ui(app, ui, &t),
                Modal::Category(f) => f.ui(app, ui, &t),
                Modal::Import(f) => f.ui(app, ui, &t),
                Modal::Confirm(f) => f.ui(app, ui, &t),
                Modal::Help => help_ui(ui, &t),
                Modal::Currency(f) => f.ui(app, ui, &t),
                Modal::WhatsNew => {
                    if crate::whatsnew::card(app, ui, &t) {
                        Outcome::Close
                    } else {
                        Outcome::Keep
                    }
                }
            }
        });
    let close = resp.should_close() || resp.inner == Outcome::Close;
    if app.modal.is_some() {
        // A form opened another modal (e.g. confirm delete) — let it replace us.
        return;
    }
    if close && closing.is_none() {
        crate::diag::crumb(format!("close modal {}", modal.name()));
        // Read it (Got it, Esc or a click outside): the sidebar row goes.
        if matches!(modal, Modal::WhatsNew) {
            app.whats_new.dismiss(&mut app.store);
        }
        app.modal_closing = Some(now);
    }
    app.modal = Some(modal);
}

fn title(ui: &mut Ui, t: &Theme, text: &str, sub: &str) {
    ui.label(egui::RichText::new(text).font(theme::display(21.0)).color(t.text));
    if !sub.is_empty() {
        ui.label(w::subtle(t, sub));
    }
    ui.add_space(14.0);
}

/// Two columns of form fields.
fn two<R>(ui: &mut Ui, left: impl FnOnce(&mut Ui, f32) -> R, right: impl FnOnce(&mut Ui, f32) -> R) {
    let wdt = (ui.available_width() - 14.0) / 2.0;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(wdt, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            left(ui, wdt)
        });
        ui.add_space(6.0);
        ui.allocate_ui_with_layout(vec2(wdt, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            right(ui, wdt)
        });
    });
}

fn footer(ui: &mut Ui, t: &Theme, primary: &str, left: impl FnOnce(&mut Ui)) -> (bool, bool) {
    ui.add_space(18.0);
    let mut ok = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        left(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ok = w::primary(ui, t, None, primary).clicked();
            cancel = w::ghost(ui, t, None, "Cancel").clicked();
        });
    });
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    (ok || enter, cancel)
}

fn error_line(ui: &mut Ui, t: &Theme, err: &Option<String>) {
    if let Some(e) = err {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(format!("{}  {e}", ph::WARNING)).color(t.neg));
    }
}

pub fn account_picker(
    ui: &mut Ui,
    store: &Store,
    id: impl std::hash::Hash + std::fmt::Debug,
    sel: &mut RowId,
    width: f32,
) {
    let label = store
        .account(*sel)
        .map(|a| format!("{}  {}", icons::account(a), a.name))
        .unwrap_or_else(|| "Choose account".into());
    w::dropdown(ui, id, label, width, |ui| {
        for a in store.active_accounts() {
            let text = format!("{}  {}  ·  {}", icons::account(a), a.name, a.currency);
            w::option_value(ui, sel, a.id, text);
        }
    });
}

/// What's typed into an open category list, and which match the keyboard is on.
#[derive(Clone, Default)]
struct CatSearch {
    query: String,
    lit: usize,
    /// Arrows were used, so Enter means the lit row even with nothing typed.
    nav: bool,
    started: bool,
    /// The field has had keyboard focus; until then keep asking for it (a
    /// popup's first frame is laid out unseen and drops the request).
    focused: bool,
}

/// How well a category name matches what's typed: lower is better, `None`
/// for no match. Starts of the name beat starts of a word beat anywhere.
fn cat_match(name: &str, query: &str) -> Option<u8> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let n = name.to_lowercase();
    if n.starts_with(&q) {
        Some(0)
    } else if n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) {
        Some(1)
    } else if n.contains(&q) {
        Some(2)
    } else {
        None
    }
}

/// The category dropdown. Typing while it's open (or while it has keyboard
/// focus) narrows the list; Enter takes the top match, arrows move.
pub fn category_picker(
    ui: &mut Ui,
    t: &Theme,
    store: &Store,
    id: impl std::hash::Hash + std::fmt::Debug,
    sel: &mut Option<RowId>,
    width: f32,
    kind: Option<CategoryKind>,
) {
    let label: egui::WidgetText = match sel.and_then(|c| store.category(c)) {
        Some(c) => egui::RichText::new(format!("{}  {}", icons::glyph(&c.icon), c.name))
            .color(w::readable(t, w::cat_color(c.color)))
            .into(),
        None => egui::RichText::new("Uncategorized").color(t.text2).into(),
    };
    let key = format!("picker:{id:?}");
    let ctx = ui.ctx().clone();
    let popup_id = w::dropdown_popup_id(ui, &id);
    let state_id = popup_id.with("search");
    let mut open = false;
    let resp = w::dropdown_ex(ui, id, label, width, true, |ui, _| {
        open = true;
        let mut st: CatSearch = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();
        let field_id = state_id.with("field");
        // The frame it opens, keys belong to whatever opened it (Enter on the field).
        let first = !st.started;
        st.started = true;
        if !st.focused {
            if ui.memory(|m| m.has_focus(field_id)) {
                st.focused = true;
            } else {
                ui.memory_mut(|m| m.request_focus(field_id));
            }
        }

        // The search line: quiet, just a glyph and the text.
        let before = st.query.clone();
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(ph::MAGNIFYING_GLASS)
                    .font(theme::regular(13.0))
                    .color(t.text3),
            );
            ui.add(
                egui::TextEdit::singleline(&mut st.query)
                    .id(field_id)
                    .frame(egui::Frame::NONE)
                    .hint_text(egui::RichText::new("Type to find").color(t.text3))
                    .desired_width(ui.available_width() - 16.0),
            );
        });
        if st.query != before {
            st.lit = 0;
        }
        ui.add_space(4.0);

        // Matches, best first; with nothing typed, the usual grouped list.
        let searching = !st.query.trim().is_empty();
        let mut matches: Vec<(u8, &magpie_core::Category)> = store
            .categories()
            .iter()
            .filter(|c| !c.archived && kind.is_none_or(|k| c.kind == k))
            .filter_map(|c| cat_match(&c.name, &st.query).map(|r| (r, c)))
            .collect();
        matches.sort_by_key(|(r, _)| *r);
        let picks: Vec<Option<RowId>> = if searching {
            matches.iter().map(|(_, c)| Some(c.id)).collect()
        } else {
            std::iter::once(None)
                .chain([CategoryKind::Expense, CategoryKind::Income].into_iter().flat_map(|k| {
                    matches
                        .iter()
                        .filter(move |(_, c)| c.kind == k)
                        .map(|(_, c)| Some(c.id))
                }))
                .collect()
        };

        let (down, up, enter, esc) = ui.input_mut(|i| {
            if first {
                return (false, false, false, false);
            }
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        let moved = down || up;
        st.nav |= moved;
        if !picks.is_empty() {
            if down {
                st.lit = (st.lit + 1).min(picks.len() - 1);
            }
            if up {
                st.lit = st.lit.saturating_sub(1);
            }
            st.lit = st.lit.min(picks.len() - 1);
        }
        let mut chosen = None;
        let engaged = searching || st.nav;
        if enter
            && engaged
            && let Some(p) = picks.get(st.lit)
        {
            chosen = Some(*p);
        }

        let mut row = 0;
        let mut item = |ui: &mut Ui, pick: Option<RowId>, text: egui::RichText, chosen: &mut Option<Option<RowId>>| {
            let lit = engaged && row == st.lit;
            let r = w::option_lit(ui, *sel == pick, lit, text);
            if lit && moved {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                *chosen = Some(pick);
            }
            row += 1;
        };
        if searching {
            if matches.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(w::faint(t, "No category matches"));
                });
            }
            for (_, c) in &matches {
                let text = egui::RichText::new(format!("{}  {}", icons::glyph(&c.icon), c.name))
                    .color(w::readable(t, w::cat_color(c.color)));
                item(ui, Some(c.id), text, &mut chosen);
            }
        } else {
            item(
                ui,
                None,
                egui::RichText::new("Uncategorized").color(t.text2),
                &mut chosen,
            );
            for k in [CategoryKind::Expense, CategoryKind::Income] {
                if kind.is_some_and(|x| x != k) {
                    continue;
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(if k == CategoryKind::Expense {
                            "EXPENSES"
                        } else {
                            "INCOME"
                        })
                        .font(theme::semibold(10.5))
                        .color(t.text3),
                    );
                });
                ui.add_space(2.0);
                for (_, c) in matches.iter().filter(|(_, c)| c.kind == k) {
                    let text = egui::RichText::new(format!("{}  {}", icons::glyph(&c.icon), c.name))
                        .color(w::readable(t, w::cat_color(c.color)));
                    item(ui, Some(c.id), text, &mut chosen);
                }
            }
        }

        if let Some(pick) = chosen {
            *sel = pick;
        }
        if chosen.is_some() || esc || enter {
            egui::Popup::close_id(ui.ctx(), popup_id);
            ui.data_mut(|d| d.remove::<CatSearch>(state_id));
        } else {
            ui.data_mut(|d| d.insert_temp(state_id, st));
        }
    });
    // Typing on the closed field (reached with Tab) opens it with that text.
    if !open && resp.response.has_focus() {
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect()
        });
        if !typed.trim().is_empty() {
            egui::Popup::open_id(&ctx, popup_id);
            let st = CatSearch {
                query: typed,
                ..Default::default()
            };
            ctx.data_mut(|d| d.insert_temp(state_id, st));
        }
    } else if !open {
        // Closed by a click outside: start fresh next time.
        ctx.data_mut(|d| d.remove::<CatSearch>(state_id));
    }
    crate::marks::record(|| key, resp.response.rect);
}

fn amount_field(ui: &mut Ui, t: &Theme, id: Id, value: &mut String, cur: Cur, color: Color32) -> egui::Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let k = motion::toggle(ui.ctx(), id.with("f"), focused, motion::MICRO);
    let mut resp = None;
    egui::Frame::new()
        .fill(t.bg)
        .stroke(Stroke::new(1.0, motion::lerp_color(t.border, t.accent, k)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(cur.symbol().trim())
                        .font(theme::display(26.0))
                        .color(t.text3),
                );
                resp = Some(
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .id(id)
                            .font(theme::display(26.0))
                            .text_color(color)
                            .hint_text(egui::RichText::new("0.00").font(theme::display(26.0)).color(t.text3))
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width() - 60.0),
                    ),
                );
                ui.label(egui::RichText::new(cur.code()).font(theme::medium(12.0)).color(t.text3));
            });
        });
    resp.expect("text edit shown")
}

// ================================================================ Txn

pub struct TxnForm {
    editing: Option<RowId>,
    mode: usize, // 0 expense, 1 income, 2 transfer
    account: RowId,
    to_account: RowId,
    date: Date,
    amount: String,
    to_amount: String,
    payee: String,
    /// The payee suggestion the arrow keys are on.
    payee_lit: Option<usize>,
    category: Option<RowId>,
    note: String,
    tags: String,
    cleared: bool,
    error: Option<String>,
    first_frame: bool,
    auto_category: bool,
    /// Money in that pays you back for something (a friend's share of the
    /// tickets): it's filed under the spending category, so it comes off
    /// that budget instead of counting as income.
    payback: bool,
}

impl TxnForm {
    pub fn new(store: &Store, today: Date) -> TxnForm {
        // Basic mode always starts on the first account people made.
        let account = if store.settings().basic {
            store.active_accounts().map(|a| a.id).min()
        } else {
            store.default_account()
        }
        .unwrap_or(0);
        let to = store
            .active_accounts()
            .map(|a| a.id)
            .find(|id| *id != account)
            .unwrap_or(account);
        TxnForm {
            editing: None,
            mode: 0,
            account,
            to_account: to,
            date: today,
            amount: String::new(),
            to_amount: String::new(),
            payee: String::new(),
            payee_lit: None,
            category: None,
            note: String::new(),
            tags: String::new(),
            cleared: true,
            error: None,
            first_frame: true,
            auto_category: true,
            payback: false,
        }
    }

    /// A new transaction pre-filled from `t` (e.g. quick add → Details).
    pub fn prefilled(store: &Store, t: &Txn) -> TxnForm {
        let mut f = TxnForm::edit(store, t);
        f.editing = None;
        f.auto_category = t.category.is_none();
        if t.amount == 0 {
            f.amount.clear();
            f.mode = 0;
        }
        f
    }

    /// The payee as typed, for the stills.
    #[cfg(test)]
    pub fn payee_text(&self) -> &str {
        &self.payee
    }

    /// The chosen category, for the stills.
    #[cfg(test)]
    pub fn category_id(&self) -> Option<RowId> {
        self.category
    }

    /// A payback filled in, for the stills.
    #[cfg(test)]
    pub fn payback_example(store: &Store, today: Date, category: RowId, amount: &str, payee: &str) -> TxnForm {
        let mut f = TxnForm::new(store, today);
        f.mode = 1;
        f.payback = true;
        f.category = Some(category);
        f.amount = amount.into();
        f.payee = payee.into();
        f.first_frame = false;
        f
    }

    pub fn transfer(store: &Store, today: Date) -> TxnForm {
        let mut f = TxnForm::new(store, today);
        f.mode = 2;
        f
    }

    pub fn edit(store: &Store, t: &Txn) -> TxnForm {
        let cur = store.account_cur(t.account);
        let mut f = TxnForm::new(store, t.date);
        f.editing = Some(t.id);
        f.account = t.account;
        f.amount = money::to_input(t.amount.abs(), cur);
        f.payee = t.payee.clone();
        f.category = t.category;
        f.note = t.note.clone();
        f.tags = t.tags.iter().map(|x| format!("#{x}")).collect::<Vec<_>>().join(" ");
        f.cleared = t.cleared;
        f.auto_category = false;
        f.payback = t.amount > 0
            && !t.is_transfer()
            && t.category.and_then(|c| store.category(c)).map(|c| c.kind) == Some(CategoryKind::Expense);
        f.mode = if t.is_transfer() {
            2
        } else if t.amount > 0 {
            1
        } else {
            0
        };
        if let Some(g) = t.transfer
            && let Some(other) = store.txns().iter().find(|x| x.transfer == Some(g) && x.id != t.id)
        {
            // Always present a transfer from the outgoing leg's perspective.
            let (out, inc) = if t.amount < 0 { (t, other) } else { (other, t) };
            f.editing = Some(out.id);
            f.account = out.account;
            f.to_account = inc.account;
            f.amount = money::to_input(out.amount.abs(), store.account_cur(out.account));
            f.to_amount = money::to_input(inc.amount.abs(), store.account_cur(inc.account));
        }
        f
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let ctx = ui.ctx().clone();
        let heading = match (self.editing.is_some(), self.mode) {
            (false, 2) => "New transfer",
            (true, 2) => "Edit transfer",
            (false, _) => "New transaction",
            (true, _) => "Edit transaction",
        };
        title(ui, t, heading, "");
        let basic = app.basic();
        if self.editing.is_none() || self.mode != 2 {
            let mut m = self.mode;
            let opts: &[&str] = if basic {
                &["Spent", "Received"]
            } else if self.editing.is_some() {
                &["Expense", "Income"]
            } else {
                &["Expense", "Income", "Transfer"]
            };
            if w::segmented(ui, t, Id::new("txn-mode"), &mut m, opts) {
                self.mode = m;
            }
            ui.add_space(12.0);
        }
        let cur = app.store.account_cur(self.account);
        let amount_id = Id::new("txn-amount");
        let color = match self.mode {
            1 => t.pos,
            _ => t.text,
        };
        amount_field(ui, t, amount_id, &mut self.amount, cur, color);
        if self.first_frame {
            ctx.memory_mut(|m| m.request_focus(amount_id));
            self.first_frame = false;
        }
        ui.add_space(10.0);
        if self.mode == 1 {
            let was = self.payback;
            w::toggle_row(ui, t, &mut self.payback, "Someone's paying me back");
            if self.payback != was {
                // The category list changes between income and spending.
                self.category = None;
                self.auto_category = true;
            }
            ui.label(w::faint(
                t,
                if self.payback {
                    "Comes off a budget instead of counting as income."
                } else {
                    "Paid back for something? Take it off a budget instead."
                },
            ));
            ui.add_space(10.0);
        }
        // What the category list offers: a payback goes against spending.
        let kind = if self.mode == 1 && !self.payback {
            CategoryKind::Income
        } else {
            CategoryKind::Expense
        };
        let fits = |c: Option<RowId>| c.and_then(|c| app.store.category(c)).is_none_or(|c| c.kind == kind);

        if self.mode == 2 {
            let store = &app.store;
            two(
                ui,
                |ui, wd| {
                    w::field_label(ui, t, "From");
                    account_picker(ui, store, "tx-from", &mut self.account, wd);
                },
                |ui, wd| {
                    w::field_label(ui, t, "To");
                    account_picker(ui, store, "tx-to", &mut self.to_account, wd);
                },
            );
            let to_cur = store.account_cur(self.to_account);
            if to_cur != cur {
                ui.add_space(8.0);
                w::field_label(ui, t, &format!("Amount received in {to_cur}"));
                let est = money::parse(&self.amount, cur).map(|v| store.convert(v, cur, to_cur));
                let hint = est.map(|v| money::to_input(v, to_cur)).unwrap_or_default();
                w::text_field(
                    ui,
                    t,
                    Id::new("tx-to-amt"),
                    &mut self.to_amount,
                    &format!("≈ {hint} (auto)"),
                    ui.available_width(),
                );
            }
            ui.add_space(8.0);
            let mut d = self.date;
            two(
                ui,
                |ui, wd| {
                    w::field_label(ui, t, "Date");
                    w::date_field(ui, t, Id::new("tx-date"), &mut d, wd);
                },
                |ui, wd| {
                    w::field_label(ui, t, "Note");
                    w::text_field(ui, t, Id::new("tx-note"), &mut self.note, "Optional", wd);
                },
            );
            self.date = d;
        } else {
            // Payee with autocomplete from history.
            let payback = self.mode == 1 && self.payback;
            w::field_label(
                ui,
                t,
                match (payback, basic) {
                    (true, _) => "Who paid you back",
                    (false, true) => "For what",
                    (false, false) => "Payee",
                },
            );
            let payee_id = Id::new("txn-payee");
            let hint = match (payback, basic, self.mode) {
                (true, _, _) => "A friend, for the movie tickets…",
                (false, false, _) => "Who was it?",
                (false, true, 1) => "Salary, a refund, a gift…",
                (false, true, _) => "Coffee, rent, groceries…",
            };
            let payees = app
                .payees
                .get(app.store.version(), || analytics::payee_index(&app.store))
                .clone();
            let suggestions = |text: &str| {
                let q = text.trim().to_lowercase();
                if q.is_empty() {
                    return Vec::new();
                }
                payees
                    .iter()
                    .filter(|p| p.name.to_lowercase().contains(&q) && p.name.to_lowercase() != q)
                    .take(6)
                    .collect::<Vec<_>>()
            };
            // Arrow keys move through the suggestions and Enter takes one.
            // They're read before the field so it doesn't move its cursor
            // or let Enter save the form.
            let before = suggestions(&self.payee);
            let mut picked = None;
            if ui.memory(|m| m.has_focus(payee_id)) && !before.is_empty() {
                let none = egui::Modifiers::NONE;
                let (down, up) = ui.input_mut(|i| {
                    (
                        i.consume_key(none, egui::Key::ArrowDown),
                        i.consume_key(none, egui::Key::ArrowUp),
                    )
                });
                let last = before.len() - 1;
                if down {
                    self.payee_lit = Some(self.payee_lit.map_or(0, |i| (i + 1).min(last)));
                }
                if up {
                    self.payee_lit = self.payee_lit.and_then(|i| i.checked_sub(1));
                }
                if let Some(i) = self.payee_lit
                    && ui.input_mut(|inp| inp.consume_key(none, egui::Key::Enter))
                {
                    picked = before.get(i.min(last)).map(|p| (*p).clone());
                }
            }
            let r = w::text_field(ui, t, payee_id, &mut self.payee, hint, ui.available_width());
            if r.changed() {
                self.payee_lit = None;
            }
            let matches = suggestions(&self.payee);
            if r.has_focus() && !matches.is_empty() && picked.is_none() {
                self.payee_lit = self.payee_lit.map(|i| i.min(matches.len() - 1));
                let lit = self.payee_lit;
                egui::Popup::new(payee_id.with("ac"), ctx.clone(), r.rect, ui.layer_id())
                    .open(true)
                    .width(r.rect.width())
                    .frame(w::popup_frame(t))
                    .show(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (i, p) in matches.iter().enumerate() {
                            let cat = app.store.category_name(p.category).to_string();
                            let resp = w::option_lit(ui, false, lit == Some(i), format!("{}   ·  {cat}", p.name));
                            if resp.clicked() || resp.is_pointer_button_down_on() {
                                picked = Some((*p).clone());
                            }
                        }
                    });
            }
            if let Some(p) = picked {
                self.payee = p.name.clone();
                self.payee_lit = None;
                if self.auto_category && fits(p.category) {
                    self.category = p.category;
                }
            }
            let q = self.payee.trim().to_lowercase();
            if r.changed()
                && self.auto_category
                && let Some(p) = payees.iter().find(|p| p.name.to_lowercase() == q)
                && fits(p.category)
            {
                self.category = p.category;
            }
            ui.add_space(8.0);
            let store = &app.store;
            let before = self.category;
            let cat_label = if payback { "Comes off" } else { "Category" };
            if basic {
                // Category and date; the account only when there's a choice
                // (it defaults to the first one).
                let mut d = self.date;
                two(
                    ui,
                    |ui, wd| {
                        w::field_label(ui, t, cat_label);
                        category_picker(ui, t, store, "txn-cat", &mut self.category, wd, Some(kind));
                    },
                    |ui, wd| {
                        w::field_label(ui, t, "Date");
                        w::date_field(ui, t, Id::new("txn-date"), &mut d, wd);
                    },
                );
                self.date = d;
                if payback {
                    self.payback_note(store, ui, t);
                }
                if store.active_accounts().nth(1).is_some() {
                    ui.add_space(8.0);
                    w::field_label(ui, t, "Account");
                    account_picker(ui, store, "txn-acc", &mut self.account, ui.available_width());
                }
                if self.category != before {
                    self.auto_category = false;
                }
            } else {
                two(
                    ui,
                    |ui, wd| {
                        w::field_label(ui, t, cat_label);
                        category_picker(ui, t, store, "txn-cat", &mut self.category, wd, Some(kind));
                    },
                    |ui, wd| {
                        w::field_label(ui, t, "Account");
                        account_picker(ui, store, "txn-acc", &mut self.account, wd);
                    },
                );
                if self.category != before {
                    self.auto_category = false;
                }
                if payback {
                    self.payback_note(store, ui, t);
                }
                ui.add_space(8.0);
                let mut d = self.date;
                two(
                    ui,
                    |ui, wd| {
                        w::field_label(ui, t, "Date");
                        w::date_field(ui, t, Id::new("txn-date"), &mut d, wd);
                    },
                    |ui, wd| {
                        w::field_label(ui, t, "Tags");
                        w::text_field(ui, t, Id::new("txn-tags"), &mut self.tags, "#groceries #work", wd);
                    },
                );
                self.date = d;
                ui.add_space(8.0);
                w::field_label(ui, t, "Note");
                w::text_area(
                    ui,
                    t,
                    Id::new("txn-note"),
                    &mut self.note,
                    "Anything to remember?",
                    ui.available_width(),
                );
                ui.add_space(6.0);
                w::toggle_row(ui, t, &mut self.cleared, "Cleared");
            }
        }
        error_line(ui, t, &self.error);

        let mut delete = false;
        let mut duplicate = false;
        let editing = self.editing.is_some();
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Add" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
                if self.mode != 2 && !basic {
                    duplicate = w::ghost(ui, t, Some(ph::COPY), "Duplicate").clicked();
                }
            }
        });
        if cancel {
            return Outcome::Close;
        }
        if delete && let Some(id) = self.editing {
            if let Some(n) = app.toasts.ok(app.store.delete_txns(&[id])) {
                app.toasts.undoable(if n > 1 {
                    "Transfer deleted"
                } else {
                    "Transaction deleted"
                });
                app.ledger.clear_selection();
            }
            return Outcome::Close;
        }
        if duplicate {
            self.editing = None;
            self.date = app.today;
        }
        if !ok && !duplicate {
            return Outcome::Keep;
        }
        match self.save(app) {
            Ok(msg) => {
                if duplicate {
                    app.toasts.success("Duplicated");
                } else {
                    app.toasts.undoable(msg);
                }
                Outcome::Close
            }
            Err(e) => {
                self.error = Some(e);
                Outcome::Keep
            }
        }
    }

    /// For a payback: what it does to that category this month, e.g.
    /// "Entertainment spent in Oct: ₹1,000 → ₹500 of ₹2,000".
    fn payback_note(&self, store: &Store, ui: &mut Ui, t: &Theme) {
        let Some(cat) = self.category.and_then(|c| store.category(c)) else {
            return;
        };
        let base = store.base();
        let m = magpie_core::Month::of(self.date);
        let mut spent = analytics::category_spent_map(store, m)
            .get(&cat.id)
            .copied()
            .unwrap_or(0);
        // When editing, leave this transaction's own effect out of "before".
        if let Some(old) = self.editing.and_then(|id| store.txn(id))
            && old.category == Some(cat.id)
            && magpie_core::Month::of(old.date) == m
        {
            spent -= analytics::flow(store, old).1;
        }
        let cur = store.account_cur(self.account);
        let amt = money::parse(&self.amount, cur)
            .map(|v| store.convert(v.abs(), cur, base))
            .unwrap_or(0);
        let budget = magpie_core::budget::month_budget(store, m)
            .into_iter()
            .find(|l| l.category == cat.id)
            .map(|l| l.available());
        let mut text = format!(
            "{} spent in {}: {} → {}",
            cat.name,
            m.short(),
            w::fmt_money(spent.max(0), base),
            w::fmt_money((spent - amt).max(0), base)
        );
        if let Some(b) = budget {
            text.push_str(&format!(" of {}", w::fmt_money(b, base)));
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!("{}  {text}", ph::ARROW_BEND_DOWN_LEFT))
                .font(theme::regular(12.5))
                .color(t.pos),
        );
    }

    fn save(&mut self, app: &mut App) -> Result<&'static str, String> {
        let store = &mut app.store;
        let cur = store.account_cur(self.account);
        let amt = money::parse(&self.amount, cur)
            .filter(|v| *v != 0)
            .ok_or("Enter an amount")?
            .abs();
        if store.account(self.account).is_none() {
            return Err("Choose an account".into());
        }
        if self.mode == 2 {
            if self.account == self.to_account {
                return Err("Pick two different accounts".into());
            }
            let to_cur = store.account_cur(self.to_account);
            let to_amt = if to_cur != cur {
                money::parse(&self.to_amount, to_cur).map(i64::abs)
            } else {
                Some(amt)
            };
            if let Some(id) = self.editing {
                // Delete + recreate keeps both legs perfectly consistent.
                store.delete_txns(&[id]).map_err(|e| e.to_string())?;
            }
            store
                .add_transfer(self.account, self.to_account, self.date, amt, to_amt, &self.note)
                .map_err(|e| e.to_string())?;
            return Ok(if self.editing.is_some() {
                "Transfer updated"
            } else {
                "Transfer added"
            });
        }
        let signed = if self.mode == 1 { amt } else { -amt };
        let tags: Vec<String> = self
            .tags
            .split(|c: char| c.is_whitespace() || c == ',')
            .map(|s| s.trim_start_matches('#').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let mut txn = match self.editing.and_then(|id| store.txn(id).cloned()) {
            Some(t) => t,
            None => Txn::blank(self.account, self.date),
        };
        txn.account = self.account;
        txn.date = self.date;
        txn.amount = signed;
        txn.payee = self.payee.trim().to_string();
        txn.category = self.category;
        txn.note = self.note.trim().to_string();
        txn.tags = tags;
        txn.cleared = self.cleared;
        if self.editing.is_some() {
            store.update_txn(txn).map_err(|e| e.to_string())?;
            Ok("Transaction updated")
        } else {
            store.add_txn(txn).map_err(|e| e.to_string())?;
            Ok("Transaction added")
        }
    }
}

// ============================================================ Account

pub struct AccountForm {
    acc: Account,
    opening: String,
    /// Balance change from transactions so far, so the preview card shows
    /// the real balance as the starting balance is edited.
    activity: i64,
    error: Option<String>,
}

impl AccountForm {
    pub fn new(store: &Store) -> AccountForm {
        let n = store.accounts().len();
        AccountForm {
            acc: Account {
                id: 0,
                name: String::new(),
                kind: AccountKind::Checking,
                currency: store.base(),
                opening: 0,
                color: magpie_core::demo::PALETTE[n % 16],
                icon: String::new(),
                style: Default::default(),
                archived: false,
                sort: 0,
            },
            opening: String::new(),
            activity: 0,
            error: None,
        }
    }

    pub fn edit(store: &Store, a: &Account) -> AccountForm {
        let balance = analytics::balances(store).get(&a.id).copied().unwrap_or(a.opening);
        AccountForm {
            acc: a.clone(),
            opening: money::to_input(a.opening, a.currency),
            activity: balance - a.opening,
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.acc.id != 0;
        let basic = app.basic();
        let base = app.store.base();
        if basic && !editing {
            self.acc.currency = base;
        }
        title(
            ui,
            t,
            if editing { "Edit account" } else { "New account" },
            "Checking, savings, cards, cash or investments. Make it yours.",
        );
        let gap = 28.0;
        let col = (ui.available_width() - gap) / 2.0;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(col);
                w::field_label(ui, t, "Name");
                w::text_field(
                    ui,
                    t,
                    Id::new("acc-name"),
                    &mut self.acc.name,
                    "e.g. Everyday checking",
                    col,
                );
                ui.add_space(8.0);
                let acc = &mut self.acc;
                w::field_label(ui, t, "Type");
                w::dropdown(
                    ui,
                    "acc-kind",
                    format!("{}  {}", icons::account_kind(acc.kind), acc.kind.label()),
                    col,
                    |ui| {
                        for k in AccountKind::ALL {
                            w::option_value(
                                ui,
                                &mut acc.kind,
                                k,
                                format!("{}  {}", icons::account_kind(k), k.label()),
                            );
                        }
                    },
                );
                // Basic keeps everything in the main currency; an account
                // that already uses another one still shows (and keeps) it.
                if !basic || acc.currency != base {
                    ui.add_space(8.0);
                    w::field_label(ui, t, "Currency");
                    currency_picker(ui, "acc-cur", &mut acc.currency, col);
                }
                ui.add_space(8.0);
                w::field_label(ui, t, "Starting balance");
                w::text_field(
                    ui,
                    t,
                    Id::new("acc-open"),
                    &mut self.opening,
                    if basic {
                        "0.00"
                    } else {
                        "0.00 (negative for a card balance owed)"
                    },
                    col,
                );
                if basic {
                    ui.label(w::faint(t, "For a credit card, put what you owe as a negative."));
                }
                if editing {
                    ui.add_space(10.0);
                    w::toggle_row(
                        ui,
                        t,
                        &mut self.acc.archived,
                        if basic {
                            "Archived (hidden everywhere)"
                        } else {
                            "Archived (hidden from pickers and net worth)"
                        },
                    );
                }
            });
            ui.add_space(gap - ui.spacing().item_spacing.x);
            ui.allocate_ui_with_layout(vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(col);
                let (r, _) = ui.allocate_exact_size(vec2(col, 150.0), egui::Sense::hover());
                let opening = money::parse(&self.opening, self.acc.currency).unwrap_or(0);
                let card = crate::views::accounts::Card {
                    balance: opening + self.activity,
                    series: None,
                    show_currency: !basic || self.acc.currency != base,
                    in_base: None,
                    base,
                };
                crate::views::accounts::paint_card(ui, t, &self.acc, &card, r, 0.0);
                ui.add_space(14.0);
                w::field_label(ui, t, "Colour");
                color_row(ui, t, &mut self.acc.color);
                ui.add_space(8.0);
                w::field_label(ui, t, "Icon");
                let color = w::cat_color(self.acc.color);
                account_icon_row(ui, t, &mut self.acc, color);
                ui.add_space(8.0);
                w::field_label(ui, t, "Look");
                style_row(ui, t, &mut self.acc);
            });
        });
        error_line(ui, t, &self.error);
        let mut delete = false;
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Create account" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
            }
        });
        if cancel {
            return Outcome::Close;
        }
        if delete {
            let n = app.store.txns().iter().filter(|x| x.account == self.acc.id).count();
            let ctx = ui.ctx().clone();
            app.open_modal(
                &ctx,
                Modal::Confirm(Confirm {
                    title: format!("Delete “{}”?", self.acc.name),
                    body: format!(
                        "This permanently deletes the account and its {n} transactions. This can't be undone."
                    ),
                    action: ConfirmAction::DeleteAccount(self.acc.id),
                    danger: true,
                }),
            );
            return Outcome::Keep;
        }
        if !ok {
            return Outcome::Keep;
        }
        if self.acc.name.trim().is_empty() {
            self.error = Some("Give the account a name".into());
            return Outcome::Keep;
        }
        self.acc.opening = if self.opening.trim().is_empty() {
            0
        } else {
            match money::parse(&self.opening, self.acc.currency) {
                Some(v) => v,
                None => {
                    self.error = Some("That starting balance isn't a number".into());
                    return Outcome::Keep;
                }
            }
        };
        self.acc.name = self.acc.name.trim().to_string();
        if app.toasts.ok(app.store.save_account(self.acc.clone())).is_some() {
            app.toasts
                .success(if editing { "Account saved" } else { "Account created" });
            let ctx = ui.ctx().clone();
            app.maybe_refresh_fx(&ctx, false);
        }
        Outcome::Close
    }
}

pub fn currency_picker(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, cur: &mut Cur, width: f32) {
    let label = format!("{}  {}", cur.code(), cur.info().map(|i| i.name).unwrap_or(""));
    let key = format!("picker:{id:?}");
    let resp = w::dropdown(ui, id, label, width, |ui| {
        for info in money::CURRENCIES {
            if let Some(c) = Cur::new(info.code) {
                w::option_value(
                    ui,
                    cur,
                    c,
                    format!("{}  {}  {}", info.code, info.symbol.trim(), info.name),
                );
            }
        }
    });
    crate::marks::record(|| key, resp.response.rect);
}

pub fn color_row(ui: &mut Ui, t: &Theme, color: &mut u32) {
    ui.horizontal_wrapped(|ui| {
        for c in magpie_core::demo::PALETTE {
            let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
            let sel = *color == c;
            let k = motion::toggle(ui.ctx(), resp.id, sel || resp.hovered(), motion::MICRO);
            ui.painter().circle_filled(r.center(), 9.0 + 2.0 * k, w::cat_color(c));
            if sel {
                ui.painter().circle_stroke(r.center(), 12.5, Stroke::new(2.0, t.text));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *color = c;
            }
        }
    });
}

fn icon_row(ui: &mut Ui, t: &Theme, icon: &mut String, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (name, glyph) in icons::PICKER {
            let sel = icon == name;
            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::click());
            let bg = if sel {
                t.tint(color, 0.3)
            } else if resp.hovered() {
                t.hover_wash()
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(r, CornerRadius::same(8), bg);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                *glyph,
                theme::regular(17.0),
                if sel { w::readable(t, color) } else { t.text2 },
            );
            if resp.on_hover_text(*name).clicked() {
                *icon = name.to_string();
            }
        }
    });
}

/// Icons for an account card: the first follows the account's type.
fn account_icon_row(ui: &mut Ui, t: &Theme, acc: &mut Account, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let auto = ("", icons::account_kind(acc.kind));
        // The type's own icon is the first choice, so it isn't listed twice.
        let rest = icons::ACCOUNT_PICKER.iter().filter(|(_, g)| *g != auto.1);
        for (name, glyph) in std::iter::once(&auto).chain(rest) {
            let sel = acc.icon == *name || (name.is_empty() && icons::glyph(&acc.icon) == auto.1);
            let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::click());
            let h = motion::toggle(ui.ctx(), resp.id, resp.hovered(), motion::MICRO);
            let bg = if sel {
                t.tint(color, 0.3)
            } else {
                motion::with_alpha(t.hover_wash(), h)
            };
            ui.painter().rect_filled(r, CornerRadius::same(8), bg);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                *glyph,
                theme::regular(16.0),
                if sel { w::readable(t, color) } else { t.text2 },
            );
            let tip = if name.is_empty() {
                "Match the account type"
            } else {
                name
            };
            if resp.on_hover_text(tip).clicked() {
                acc.icon = name.to_string();
            }
        }
    });
}

/// The three card looks, each drawn as a tiny card in the account's colour.
fn style_row(ui: &mut Ui, t: &Theme, acc: &mut Account) {
    let color = w::cat_color(acc.color);
    let gap = 8.0;
    let wdt = (ui.available_width() - 2.0 * gap) / 3.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for style in magpie_core::CardStyle::ALL {
            let (r, resp) = ui.allocate_exact_size(vec2(wdt, 56.0), egui::Sense::click());
            let sel = acc.style == style;
            let h = motion::toggle(ui.ctx(), resp.id, resp.hovered() || sel, motion::MICRO);
            let p = ui.painter();
            let rad = CornerRadius::same(10);
            let ink = match style {
                magpie_core::CardStyle::Plain => {
                    p.rect(r, rad, t.card, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                    t.text
                }
                magpie_core::CardStyle::Tinted => {
                    p.rect(
                        r,
                        rad,
                        t.tint(color, if t.dark { 0.17 } else { 0.11 }),
                        Stroke::new(1.0, motion::with_alpha(color, 0.35)),
                        egui::StrokeKind::Inside,
                    );
                    t.text
                }
                magpie_core::CardStyle::Bold => {
                    let top = motion::lerp_color(color, Color32::WHITE, 0.06);
                    let bottom = motion::lerp_color(color, Color32::BLACK, 0.3);
                    w::rounded_gradient(p, r, 10.0, top, bottom);
                    Color32::WHITE
                }
            };
            let dot = egui::Rect::from_min_size(r.min + vec2(10.0, 10.0), vec2(14.0, 14.0));
            let dot_c = if style == magpie_core::CardStyle::Bold {
                motion::with_alpha(Color32::WHITE, 0.3)
            } else {
                t.tint(color, 0.45)
            };
            p.rect_filled(dot, CornerRadius::same(4), dot_c);
            p.text(
                egui::pos2(r.left() + 10.0, r.bottom() - 13.0),
                Align2::LEFT_CENTER,
                style.label(),
                theme::medium(12.0),
                ink,
            );
            if h > 0.0 {
                let ring = if sel {
                    t.accent
                } else {
                    motion::with_alpha(t.text3, 0.6)
                };
                p.rect_stroke(
                    r.expand(2.5),
                    CornerRadius::same(12),
                    Stroke::new(if sel { 2.0 } else { 1.0 }, motion::with_alpha(ring, h)),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acc.style = style;
            }
        }
    });
}

// ================================================================ Rule

pub struct RuleForm {
    rule: RecurringRule,
    amount: String,
    income: bool,
    has_end: bool,
    end: Date,
    error: Option<String>,
}

impl RuleForm {
    pub fn new(store: &Store, today: Date) -> RuleForm {
        RuleForm {
            rule: RecurringRule {
                id: 0,
                payee: String::new(),
                account: store.default_account().unwrap_or(0),
                category: None,
                amount: 0,
                note: String::new(),
                freq: Freq::Monthly,
                interval: 1,
                start: today,
                end: None,
                posted: 0,
                auto_post: true,
                active: true,
            },
            amount: String::new(),
            income: false,
            has_end: false,
            end: today,
            error: None,
        }
    }

    pub fn from_txn(store: &Store, tx: &Txn, today: Date) -> RuleForm {
        let mut f = RuleForm::new(store, today);
        f.rule.payee = tx.payee.clone();
        f.rule.account = tx.account;
        f.rule.category = tx.category;
        f.rule.note = tx.note.clone();
        f.income = tx.amount > 0;
        f.amount = money::to_input(tx.amount.abs(), store.account_cur(tx.account));
        // Next occurrence one month after the transaction.
        f.rule.start = tx.date.checked_add(jiff::Span::new().months(1)).unwrap_or(today);
        f
    }

    pub fn edit(store: &Store, r: &RecurringRule) -> RuleForm {
        let cur = store.account_cur(r.account);
        RuleForm {
            rule: r.clone(),
            amount: money::to_input(r.amount.abs(), cur),
            income: r.amount > 0,
            has_end: r.end.is_some(),
            end: r.end.unwrap_or(r.start),
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.rule.id != 0;
        title(
            ui,
            t,
            if editing { "Edit recurring" } else { "New recurring" },
            "Bills, subscriptions, salary — posted automatically.",
        );
        let mut m = self.income as usize;
        if w::segmented(ui, t, Id::new("rule-kind"), &mut m, &["Expense", "Income"]) {
            self.income = m == 1;
        }
        ui.add_space(10.0);
        let cur = app.store.account_cur(self.rule.account);
        amount_field(
            ui,
            t,
            Id::new("rule-amt"),
            &mut self.amount,
            cur,
            if self.income { t.pos } else { t.text },
        );
        ui.add_space(8.0);
        w::field_label(ui, t, "Payee");
        w::text_field(
            ui,
            t,
            Id::new("rule-payee"),
            &mut self.rule.payee,
            "e.g. Netflix",
            ui.available_width(),
        );
        ui.add_space(8.0);
        let store = &app.store;
        let rule = &mut self.rule;
        let kind = if self.income {
            CategoryKind::Income
        } else {
            CategoryKind::Expense
        };
        two(
            ui,
            |ui, wd| {
                w::field_label(ui, t, "Category");
                category_picker(ui, t, store, "rule-cat", &mut rule.category, wd, Some(kind));
            },
            |ui, wd| {
                w::field_label(ui, t, "Account");
                account_picker(ui, store, "rule-acc", &mut rule.account, wd);
            },
        );
        ui.add_space(8.0);
        w::field_label(ui, t, "Repeats");
        ui.horizontal(|ui| {
            ui.label(w::subtle(t, "Every"));
            let mut n = self.rule.interval.to_string();
            ui.add(
                egui::TextEdit::singleline(&mut n)
                    .desired_width(36.0)
                    .horizontal_align(egui::Align::Center),
            );
            if let Ok(v) = n.parse::<i64>() {
                self.rule.interval = v.clamp(1, 365);
            }
            let fr = &mut self.rule.freq;
            let iv = self.rule.interval;
            w::dropdown(ui, "rule-freq", fr.unit(iv), 120.0, |ui| {
                for f in Freq::ALL {
                    w::option_value(ui, fr, f, f.unit(iv));
                }
            });
            ui.label(w::subtle(t, "starting"));
            let mut d = self.rule.start;
            if w::date_field(ui, t, Id::new("rule-start"), &mut d, 150.0) {
                self.rule.start = d;
                if editing {
                    self.rule.posted = 0;
                }
            }
        });
        ui.label(w::faint(t, recurring::describe(&self.rule)));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            w::toggle_row(ui, t, &mut self.has_end, "Ends on");
            if self.has_end {
                w::date_field(ui, t, Id::new("rule-end"), &mut self.end, 150.0);
            }
        });
        w::toggle_row(
            ui,
            t,
            &mut self.rule.auto_post,
            "Post automatically (otherwise ask me first)",
        );
        if editing {
            w::toggle_row(ui, t, &mut self.rule.active, "Active");
        }
        error_line(ui, t, &self.error);
        let mut delete = false;
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Create" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
            }
        });
        if cancel {
            return Outcome::Close;
        }
        if delete {
            if app.toasts.ok(app.store.delete_rule(self.rule.id)).is_some() {
                app.toasts.info("Recurring rule deleted (past transactions kept)");
            }
            return Outcome::Close;
        }
        if !ok {
            return Outcome::Keep;
        }
        let Some(v) = money::parse(&self.amount, cur).filter(|v| *v != 0) else {
            self.error = Some("Enter an amount".into());
            return Outcome::Keep;
        };
        if self.rule.payee.trim().is_empty() {
            self.error = Some("Who is this paid to (or from)?".into());
            return Outcome::Keep;
        }
        self.rule.amount = if self.income { v.abs() } else { -v.abs() };
        self.rule.end = self.has_end.then_some(self.end);
        self.rule.payee = self.rule.payee.trim().to_string();
        if app.toasts.ok(app.store.save_rule(self.rule.clone())).is_some() {
            let today = app.today;
            if let Some(n) = app.toasts.ok(recurring::post_due(&mut app.store, today))
                && n > 0
            {
                app.toasts
                    .info(format!("Posted {n} past occurrence{}", if n == 1 { "" } else { "s" }));
            }
            app.toasts.success("Recurring saved");
        }
        Outcome::Close
    }
}

// ================================================================ Goal

pub struct GoalForm {
    goal: Goal,
    target: String,
    has_deadline: bool,
    deadline: Date,
    linked: bool,
    error: Option<String>,
}

impl GoalForm {
    pub fn new(store: &Store, today: Date) -> GoalForm {
        GoalForm {
            goal: Goal {
                id: 0,
                name: String::new(),
                target: 0,
                currency: store.base(),
                deadline: None,
                account: None,
                color: magpie_core::demo::PALETTE[(store.goals().len() + 2) % 16],
                icon: "target".into(),
                created: today,
                archived: false,
            },
            target: String::new(),
            has_deadline: false,
            deadline: today.checked_add(jiff::Span::new().years(1)).unwrap_or(today),
            linked: false,
            error: None,
        }
    }

    pub fn edit(g: &Goal, today: Date) -> GoalForm {
        GoalForm {
            goal: g.clone(),
            target: money::to_input(g.target, g.currency),
            has_deadline: g.deadline.is_some(),
            deadline: g.deadline.unwrap_or(today),
            linked: g.account.is_some(),
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.goal.id != 0;
        title(
            ui,
            t,
            if editing { "Edit goal" } else { "New savings goal" },
            "Something worth saving up for.",
        );
        ui.horizontal(|ui| {
            w::icon_badge(
                ui,
                t,
                icons::glyph(&self.goal.icon),
                w::cat_color(self.goal.color),
                44.0,
            );
            ui.vertical(|ui| {
                w::field_label(ui, t, "Name");
                w::text_field(
                    ui,
                    t,
                    Id::new("goal-name"),
                    &mut self.goal.name,
                    "e.g. Japan trip",
                    ui.available_width(),
                );
            });
        });
        ui.add_space(8.0);
        w::field_label(ui, t, "Target");
        amount_field(
            ui,
            t,
            Id::new("goal-target"),
            &mut self.target,
            self.goal.currency,
            t.text,
        );
        ui.add_space(8.0);
        let g = &mut self.goal;
        two(
            ui,
            |ui, wd| {
                w::field_label(ui, t, "Currency");
                currency_picker(ui, "goal-cur", &mut g.currency, wd);
            },
            |ui, _| {
                w::field_label(ui, t, "Deadline");
                ui.horizontal(|ui| {
                    w::toggle(ui, t, &mut self.has_deadline);
                    if self.has_deadline {
                        w::date_field(ui, t, Id::new("goal-deadline"), &mut self.deadline, 150.0);
                    }
                });
            },
        );
        ui.add_space(8.0);
        w::toggle_row(
            ui,
            t,
            &mut self.linked,
            "Track an account's balance instead of manual contributions",
        );
        if self.linked {
            let mut acc = self
                .goal
                .account
                .unwrap_or_else(|| app.store.default_account().unwrap_or(0));
            account_picker(ui, &app.store, "goal-acc", &mut acc, ui.available_width());
            self.goal.account = Some(acc);
        } else {
            self.goal.account = None;
        }
        ui.add_space(8.0);
        w::field_label(ui, t, "Colour");
        color_row(ui, t, &mut self.goal.color);
        w::field_label(ui, t, "Icon");
        icon_row(ui, t, &mut self.goal.icon, w::cat_color(self.goal.color));
        if editing {
            w::toggle_row(ui, t, &mut self.goal.archived, "Archived");
        }
        error_line(ui, t, &self.error);
        let mut delete = false;
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Create goal" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
            }
        });
        if cancel {
            return Outcome::Close;
        }
        if delete {
            let ctx = ui.ctx().clone();
            app.open_modal(
                &ctx,
                Modal::Confirm(Confirm {
                    title: format!("Delete “{}”?", self.goal.name),
                    body: "The goal and its contribution history will be removed.".into(),
                    action: ConfirmAction::DeleteGoal(self.goal.id),
                    danger: true,
                }),
            );
            return Outcome::Keep;
        }
        if !ok {
            return Outcome::Keep;
        }
        let Some(v) = money::parse(&self.target, self.goal.currency).filter(|v| *v > 0) else {
            self.error = Some("Set a target amount".into());
            return Outcome::Keep;
        };
        if self.goal.name.trim().is_empty() {
            self.error = Some("Name your goal".into());
            return Outcome::Keep;
        }
        self.goal.target = v;
        self.goal.deadline = self.has_deadline.then_some(self.deadline);
        if app.toasts.ok(app.store.save_goal(self.goal.clone())).is_some() {
            app.toasts.success(if editing {
                "Goal saved"
            } else {
                "Goal created — good luck!"
            });
        }
        Outcome::Close
    }
}

pub struct ContribForm {
    goal: RowId,
    amount: String,
    date: Date,
    note: String,
    withdraw: bool,
    error: Option<String>,
}

impl ContribForm {
    pub fn new(goal: RowId, today: Date) -> ContribForm {
        ContribForm {
            goal,
            amount: String::new(),
            date: today,
            note: String::new(),
            withdraw: false,
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let Some(goal) = app.store.goal(self.goal).cloned() else {
            return Outcome::Close;
        };
        title(ui, t, &goal.name, "Add money to this goal, or take some out.");
        let mut m = self.withdraw as usize;
        if w::segmented(ui, t, Id::new("contrib-kind"), &mut m, &["Add", "Withdraw"]) {
            self.withdraw = m == 1;
        }
        ui.add_space(10.0);
        let r = amount_field(
            ui,
            t,
            Id::new("contrib-amt"),
            &mut self.amount,
            goal.currency,
            if self.withdraw { t.neg } else { t.pos },
        );
        if !r.has_focus() && self.amount.is_empty() {
            r.request_focus();
        }
        ui.add_space(8.0);
        let mut d = self.date;
        two(
            ui,
            |ui, wd| {
                w::field_label(ui, t, "Date");
                w::date_field(ui, t, Id::new("contrib-date"), &mut d, wd);
            },
            |ui, wd| {
                w::field_label(ui, t, "Note");
                w::text_field(ui, t, Id::new("contrib-note"), &mut self.note, "Optional", wd);
            },
        );
        self.date = d;
        error_line(ui, t, &self.error);
        let (ok, cancel) = footer(ui, t, if self.withdraw { "Withdraw" } else { "Add" }, |_| {});
        if cancel {
            return Outcome::Close;
        }
        if !ok {
            return Outcome::Keep;
        }
        let Some(v) = money::parse(&self.amount, goal.currency).filter(|v| *v != 0) else {
            self.error = Some("Enter an amount".into());
            return Outcome::Keep;
        };
        let amount = if self.withdraw { -v.abs() } else { v.abs() };
        let c = Contribution {
            id: 0,
            goal: goal.id,
            date: self.date,
            amount,
            note: self.note.trim().to_string(),
        };
        if app.toasts.ok(app.store.add_contribution(c)).is_some() {
            app.toasts.success(format!(
                "{} {}",
                if self.withdraw { "Withdrew" } else { "Added" },
                fmt_money(v.abs(), goal.currency)
            ));
        }
        Outcome::Close
    }
}

// ============================================================ Category

pub struct CategoryForm {
    cat: Category,
    error: Option<String>,
    /// Create a monthly budget for it right away (opened from Budgets).
    budget: bool,
}

impl CategoryForm {
    pub fn new(store: &Store, kind: CategoryKind) -> CategoryForm {
        CategoryForm {
            cat: Category {
                id: 0,
                name: String::new(),
                kind,
                color: magpie_core::demo::PALETTE[store.categories().len() % 16],
                icon: "tag".into(),
                archived: false,
            },
            error: None,
            budget: false,
        }
    }
    pub fn edit(c: &Category) -> CategoryForm {
        CategoryForm {
            cat: c.clone(),
            error: None,
            budget: false,
        }
    }
    pub fn then_budget(mut self) -> CategoryForm {
        self.budget = true;
        self
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.cat.id != 0;
        title(
            ui,
            t,
            if editing { "Edit category" } else { "New category" },
            if self.budget {
                "It gets a monthly budget you can set right after."
            } else {
                ""
            },
        );
        ui.horizontal(|ui| {
            w::icon_badge(ui, t, icons::glyph(&self.cat.icon), w::cat_color(self.cat.color), 44.0);
            ui.vertical(|ui| {
                w::field_label(ui, t, "Name");
                w::text_field(
                    ui,
                    t,
                    Id::new("cat-name"),
                    &mut self.cat.name,
                    "e.g. Coffee",
                    ui.available_width(),
                );
            });
        });
        ui.add_space(8.0);
        let mut k = (self.cat.kind == CategoryKind::Income) as usize;
        if w::segmented(ui, t, Id::new("cat-kind"), &mut k, &["Expense", "Income"]) {
            self.cat.kind = if k == 1 {
                CategoryKind::Income
            } else {
                CategoryKind::Expense
            };
        }
        ui.add_space(8.0);
        w::field_label(ui, t, "Colour");
        color_row(ui, t, &mut self.cat.color);
        w::field_label(ui, t, "Icon");
        icon_row(ui, t, &mut self.cat.icon, w::cat_color(self.cat.color));
        if editing {
            w::toggle_row(ui, t, &mut self.cat.archived, "Archived (hidden from pickers)");
        }
        error_line(ui, t, &self.error);
        let mut delete = false;
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Create" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
            }
        });
        if cancel {
            return Outcome::Close;
        }
        if delete {
            let n = app
                .store
                .txns()
                .iter()
                .filter(|x| x.category == Some(self.cat.id))
                .count();
            let ctx = ui.ctx().clone();
            app.open_modal(
                &ctx,
                Modal::Confirm(Confirm {
                    title: format!("Delete “{}”?", self.cat.name),
                    body: format!("{n} transactions will become uncategorized, and its budget is removed."),
                    action: ConfirmAction::DeleteCategory(self.cat.id),
                    danger: true,
                }),
            );
            return Outcome::Keep;
        }
        if !ok {
            return Outcome::Keep;
        }
        let name = self.cat.name.trim().to_string();
        if name.is_empty() {
            self.error = Some("Name the category".into());
            return Outcome::Keep;
        }
        let lower = name.to_lowercase();
        let existing = app
            .store
            .categories()
            .iter()
            .find(|c| c.id != self.cat.id && c.name.to_lowercase() == lower)
            .cloned();
        if let Some(c) = existing {
            if self.cat.id != 0 {
                // Renaming onto another category would merge two; don't.
                self.error = Some("A category with that name already exists".into());
                return Outcome::Keep;
            }
            // A new one that's already there: use the one there is.
            let mut c = c;
            if c.archived {
                c.archived = false;
                if app.toasts.ok(app.store.save_category(c.clone())).is_none() {
                    return Outcome::Close;
                }
            }
            app.toasts.info(format!("“{}” already exists, so we used that", c.name));
            if self.budget && c.kind == CategoryKind::Expense {
                self.budget_for(app, c.id);
            }
            return Outcome::Close;
        }
        self.cat.name = name;
        if let Some(id) = app.toasts.ok(app.store.save_category(self.cat.clone())) {
            if self.budget && self.cat.kind == CategoryKind::Expense {
                if self.budget_for(app, id) {
                    app.toasts
                        .success(format!("Created “{}” — now set its monthly amount", self.cat.name));
                }
            } else {
                app.toasts.success("Category saved");
            }
        }
        Outcome::Close
    }

    /// Gives the category a budget (if it hasn't one) and opens its editor
    /// on the Budgets page.
    fn budget_for(&self, app: &mut App, id: RowId) -> bool {
        if let Some(plan) = app.store.budget_plan(id) {
            let amount = money::to_input(plan.amount, app.store.base());
            app.budgets.edit(id, amount);
            return true;
        }
        let r = app.store.edit_budgets("Add budget", &[id], |s| {
            s.save_budget_plan(magpie_core::BudgetPlan {
                category: id,
                amount: 0,
                rollover: false,
            })
        });
        if app.toasts.ok(r).is_some() {
            app.budgets.edit(id, String::new());
            return true;
        }
        false
    }
}

// ============================================================== Import

pub struct ImportForm {
    path: PathBuf,
    preview: CsvPreview,
    map: CsvMapping,
    account: RowId,
    error: Option<String>,
    /// From onboarding: the balance the person said the account has today.
    /// After importing, the opening balance is adjusted so the account still
    /// shows exactly that.
    pub balance_today: Option<i64>,
}

impl ImportForm {
    pub fn open(store: &Store, path: PathBuf) -> magpie_core::Result<ImportForm> {
        let preview = magpie_core::statement::preview(&path)?;
        let map = preview.guess.clone();
        Ok(ImportForm {
            path,
            preview,
            map,
            account: store.default_account().unwrap_or(0),
            error: None,
            balance_today: None,
        })
    }

    fn col_picker(ui: &mut Ui, id: &str, headers: &[String], sel: &mut Option<usize>, width: f32) {
        let label = sel.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "—".into());
        w::dropdown(ui, id, label, width, |ui| {
            w::option_value(ui, sel, None, "—");
            for (i, h) in headers.iter().enumerate() {
                w::option_value(ui, sel, Some(i), h);
            }
        });
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("statement")
            .to_string();
        title(
            ui,
            t,
            "Import transactions",
            &format!(
                "{name} · {} rows found. Check the columns and pick an account.",
                self.preview.rows.len()
            ),
        );
        let headers = self.preview.headers.clone();
        let cw = (ui.available_width() - 3.0 * 12.0) / 4.0;
        ui.horizontal(|ui| {
            for (label, _) in [("Date", 0), ("Payee / description", 1), ("Category", 2), ("Note", 3)] {
                ui.allocate_ui(vec2(cw, 20.0), |ui| w::field_label(ui, t, label));
            }
        });
        ui.horizontal(|ui| {
            let mut d = Some(self.map.date);
            Self::col_picker(ui, "imp-date", &headers, &mut d, cw);
            self.map.date = d.unwrap_or(0);
            Self::col_picker(ui, "imp-payee", &headers, &mut self.map.payee, cw);
            Self::col_picker(ui, "imp-cat", &headers, &mut self.map.category, cw);
            Self::col_picker(ui, "imp-note", &headers, &mut self.map.note, cw);
        });
        ui.add_space(8.0);
        let mut split = self.map.amount.is_none() as usize;
        ui.horizontal(|ui| {
            w::segmented(
                ui,
                t,
                Id::new("imp-split"),
                &mut split,
                &["One amount column", "Debit + credit columns"],
            );
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if split == 0 {
                if self.map.amount.is_none() {
                    self.map.amount = self.map.debit.or(Some(0));
                }
                w::field_label(ui, t, "Amount");
                Self::col_picker(ui, "imp-amt", &headers, &mut self.map.amount, cw);
                w::toggle_row(ui, t, &mut self.map.invert, "Spending is positive in this file");
            } else {
                self.map.amount = None;
                w::field_label(ui, t, "Money out");
                Self::col_picker(ui, "imp-debit", &headers, &mut self.map.debit, cw);
                w::field_label(ui, t, "Money in");
                Self::col_picker(ui, "imp-credit", &headers, &mut self.map.credit, cw);
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            w::field_label(ui, t, "Date format");
            let df = &mut self.map.date_format;
            w::dropdown(ui, "imp-df", df.label(), 150.0, |ui| {
                for f in DateFormat::ALL {
                    w::option_value(ui, df, f, f.label());
                }
            });
            ui.add_space(12.0);
            w::field_label(ui, t, "Into account");
            account_picker(ui, &app.store, "imp-acc", &mut self.account, 220.0);
        });
        ui.add_space(10.0);
        // Preview table
        let cur = app.store.account_cur(self.account);
        egui::Frame::new()
            .fill(t.bg)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("imp-preview")
                    .num_columns(4)
                    .spacing(vec2(18.0, 6.0))
                    .show(ui, |ui| {
                        for h in ["Date", "Payee", "Category", "Amount"] {
                            ui.label(w::faint(t, h));
                        }
                        ui.end_row();
                        let get =
                            |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).cloned().unwrap_or_default();
                        for r in self.preview.rows.iter().take(6) {
                            let date = magpie_core::io::parse_date(&get(r, Some(self.map.date)), self.map.date_format);
                            let amt = magpie_core::statement::row_amount(r, &self.map, cur);
                            ui.label(match date {
                                Some(d) => w::subtle(t, w::fmt_date(d)),
                                None => egui::RichText::new("invalid").color(t.neg),
                            });
                            ui.label(egui::RichText::new(get(r, self.map.payee)).color(t.text));
                            ui.label(w::subtle(t, get(r, self.map.category)));
                            ui.label(match amt {
                                Some(v) => {
                                    egui::RichText::new(w::fmt_signed(v, cur)).color(if v > 0 { t.pos } else { t.text })
                                }
                                None => egui::RichText::new("—").color(t.neg),
                            });
                            ui.end_row();
                        }
                    });
            });
        error_line(ui, t, &self.error);
        let (ok, cancel) = footer(ui, t, "Import", |_| {});
        if cancel {
            return Outcome::Close;
        }
        if !ok {
            return Outcome::Keep;
        }
        match magpie_core::statement::read(&app.store, &self.path, &self.map, self.account) {
            Ok(res) => {
                let (skipped, dupes) = (res.skipped, res.duplicates);
                if res.txns.is_empty() {
                    self.error = Some(if dupes > 0 {
                        format!("All {dupes} transactions in this file are already in this account")
                    } else {
                        "No transactions could be read with these columns".into()
                    });
                    return Outcome::Keep;
                }
                match magpie_core::io::commit_import(&mut app.store, res, &format!("Import {name}")) {
                    Ok(n) => {
                        if let Some(target) = self.balance_today {
                            match_balance(&mut app.store, self.account, target);
                        }
                        let mut extra = Vec::new();
                        if dupes > 0 {
                            extra.push(format!("{dupes} already here"));
                        }
                        if skipped > 0 {
                            extra.push(format!("{skipped} other rows skipped"));
                        }
                        let extra = if extra.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", extra.join(", "))
                        };
                        app.toasts.undoable(format!("Imported {n} transactions{extra}"));
                        Outcome::Close
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        Outcome::Keep
                    }
                }
            }
            Err(e) => {
                self.error = Some(e.to_string());
                Outcome::Keep
            }
        }
    }
}

/// Sets the account's opening balance so its balance today is `target`,
/// after a statement brought in its history.
fn match_balance(store: &mut Store, account: RowId, target: i64) {
    let today = magpie_core::today();
    let Some(mut acc) = store.account(account).cloned() else {
        return;
    };
    let moved: i64 = store
        .txns()
        .iter()
        .filter(|t| t.account == account && t.date <= today)
        .map(|t| t.amount)
        .sum();
    acc.opening = target - moved;
    let _ = store.save_account(acc);
}

// ============================================================= Confirm

pub enum ConfirmAction {
    DeleteAccount(RowId),
    DeleteGoal(RowId),
    DeleteCategory(RowId),
    LoadDemo,
}

pub struct Confirm {
    pub title: String,
    pub body: String,
    pub action: ConfirmAction,
    pub danger: bool,
}

impl Confirm {
    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        ui.horizontal(|ui| {
            let c = if self.danger { t.neg } else { t.accent };
            w::icon_badge(ui, t, if self.danger { ph::WARNING } else { ph::INFO }, c, 40.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&self.title)
                        .font(theme::semibold(16.0))
                        .color(t.text),
                );
                ui.label(w::subtle(t, &self.body));
            });
        });
        ui.add_space(18.0);
        let mut ok = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ok = if self.danger {
                    w::danger(ui, t, None, "Delete")
                } else {
                    w::primary(ui, t, None, "Continue")
                }
                .clicked();
                cancel = w::ghost(ui, t, None, "Cancel").clicked();
            });
        });
        if cancel {
            return Outcome::Close;
        }
        if !ok {
            return Outcome::Keep;
        }
        let ctx = ui.ctx().clone();
        match &self.action {
            ConfirmAction::DeleteAccount(id) => {
                if app.toasts.ok(app.store.delete_account(*id)).is_some() {
                    app.toasts.info("Account deleted");
                }
            }
            ConfirmAction::DeleteGoal(id) => {
                if app.toasts.ok(app.store.delete_goal(*id)).is_some() {
                    app.toasts.info("Goal deleted");
                }
            }
            ConfirmAction::DeleteCategory(id) => {
                if app.toasts.ok(app.store.delete_category(*id)).is_some() {
                    app.toasts.info("Category deleted");
                }
            }
            ConfirmAction::LoadDemo => {
                let base = app.store.base();
                if app
                    .toasts
                    .ok(magpie_core::demo::generate(&mut app.store, base, 24, 0))
                    .is_some()
                {
                    app.toasts.success("Demo data loaded");
                    app.go(&ctx, crate::app::Page::Dashboard);
                }
            }
        }
        Outcome::Close
    }
}

// ================================================================ Help

/// Every keyboard shortcut, grouped. Also shown in Settings.
pub const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Move around",
        &[
            (concat!(shortcut!("1"), " … 8"), "Go to a page (also Alt 1 … 8)"),
            ("Alt ↑ / Alt ↓", "Previous / next page"),
            (concat!(shortcut!("K")), "Command palette"),
            (concat!(shortcut!("F")), "Search transactions"),
            (concat!(shortcut!(",")), "Settings"),
            (concat!(shortcut!("B")), "Collapse sidebar"),
            (concat!(shortcut!("Shift T")), "Basic / Advanced mode"),
            ("PgUp · PgDn · Home · End", "Scroll the page"),
            ("? or F1", "This cheat sheet"),
        ],
    ),
    (
        "Create",
        &[
            (concat!(shortcut!("N")), "New transaction"),
            (concat!(shortcut!("T")), "New transfer"),
            (concat!(shortcut!("R")), "New recurring"),
            (concat!(shortcut!("G")), "New goal"),
            (concat!(shortcut!("Shift A")), "New account"),
            (concat!(shortcut!("I")), "Import a bank statement"),
            (concat!(shortcut!("E")), "Export everything as CSV"),
        ],
    ),
    (
        "Lists (Transactions, Budgets, Accounts, Recurring, Goals)",
        &[
            ("↑ ↓ ← →  or  H J K L", "Move the selection"),
            ("Enter / Space", "Open the selected item"),
            ("E", "Edit the selected item"),
            ("Delete", "Delete the selection (Transactions)"),
            (
                "← →",
                "Previous / next month (Transactions, Budgets) or range (Reports)",
            ),
            ("N or /", "Quick add (Transactions)"),
            (concat!(shortcut!("A")), "Select all (Transactions)"),
            ("Esc", "Clear selection · close dialogs"),
        ],
    ),
    (
        "Everything else",
        &[
            (concat!(shortcut!("Z")), "Undo"),
            (concat!(shortcut!("Shift Z")), "Redo"),
            (concat!(shortcut!("+ / −")), "Bigger / smaller interface"),
            (concat!(shortcut!("0")), "Reset interface size"),
            (concat!(shortcut!("Shift L")), "Toggle light / dark"),
            ("Enter", "Save a form"),
        ],
    ),
];

pub fn shortcut_table(ui: &mut Ui, t: &Theme, columns: usize) {
    // 24pt spacer plus item spacing on both sides of it.
    let gap = 24.0 + 2.0 * ui.spacing().item_spacing.x;
    let col_w = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32;
    for row in SHORTCUTS.chunks(columns) {
        ui.horizontal_top(|ui| {
            for (ci, (title, keys)) in row.iter().enumerate() {
                ui.allocate_ui_with_layout(vec2(col_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(col_w);
                    ui.set_max_width(col_w);
                    ui.label(w::faint(t, title.to_uppercase()));
                    ui.add_space(4.0);
                    for (k, d) in keys.iter() {
                        ui.horizontal(|ui| {
                            let g = ui.painter().layout_no_wrap(k.to_string(), theme::medium(11.5), t.text);
                            let kw = g.size().x + 14.0;
                            let (r, _) = ui.allocate_exact_size(vec2(kw.max(28.0), 22.0), egui::Sense::hover());
                            ui.painter().rect(
                                r,
                                CornerRadius::same(6),
                                t.hover,
                                Stroke::new(1.0, t.border),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().galley(r.center() - g.size() / 2.0, g, t.text);
                            ui.label(egui::RichText::new(*d).font(theme::regular(12.5)).color(t.text2));
                        });
                    }
                });
                if ci + 1 < row.len() {
                    ui.add_space(24.0);
                }
            }
        });
        ui.add_space(14.0);
    }
}

fn help_ui(ui: &mut Ui, t: &Theme) -> Outcome {
    title(
        ui,
        t,
        "Keyboard shortcuts",
        "Magpie can be driven entirely from the keyboard.",
    );
    let max_h = ui.ctx().content_rect().height() * 0.62;
    crate::widgets::scroll_area()
        .max_height(max_h)
        .show(ui, |ui| shortcut_table(ui, t, 2));
    let (_, cancel) = footer(ui, t, "Got it", |_| {});
    if cancel || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        return Outcome::Close;
    }
    Outcome::Keep
}

// ============================================================ Currency

pub struct CurrencyForm {
    new: Cur,
    convert_accounts: bool,
}

impl CurrencyForm {
    pub fn new(new: Cur) -> CurrencyForm {
        CurrencyForm {
            new,
            convert_accounts: true,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let old = app.store.base();
        let n_acc = app.store.accounts().iter().filter(|a| a.currency == old).count();
        title(
            ui,
            t,
            &format!("Switch to {}?", self.new),
            &format!("Your main currency is {old} today."),
        );
        let bullet = |ui: &mut Ui, s: String| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(ph::CHECK).color(t.pos));
                ui.label(egui::RichText::new(s).color(t.text));
            });
        };
        bullet(ui, format!("Totals, charts and reports will be shown in {}.", self.new));
        bullet(
            ui,
            format!("Budgets and {old} goals are converted to {} at today's rate.", self.new),
        );
        ui.add_space(10.0);
        if n_acc > 0 {
            w::toggle_row(
                ui,
                t,
                &mut self.convert_accounts,
                &format!(
                    "Also convert my {n_acc} {old} account{} to {}",
                    if n_acc == 1 { "" } else { "s" },
                    self.new
                ),
            );
            ui.label(w::faint(
                t,
                if self.convert_accounts {
                    "Balances and every transaction in those accounts are converted at today's rate."
                } else {
                    "Those accounts stay in their own currency and are converted for totals."
                },
            ));
        }
        if let Some(r) = app.store.rates.rate(old, self.new) {
            ui.add_space(6.0);
            ui.label(w::faint(t, format!("Rate: 1 {old} = {r:.4} {}", self.new)));
        }
        let (ok, cancel) = footer(ui, t, &format!("Switch to {}", self.new), |_| {});
        if cancel {
            return Outcome::Close;
        }
        if !ok {
            return Outcome::Keep;
        }
        if let Some(n) = app.toasts.ok(app.store.change_base(self.new, self.convert_accounts)) {
            let extra = if n > 0 {
                format!(" · {n} account{} converted", if n == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            app.toasts.success(format!("Main currency is now {}{extra}", self.new));
            let ctx = ui.ctx().clone();
            app.maybe_refresh_fx(&ctx, false);
        }
        Outcome::Close
    }
}
