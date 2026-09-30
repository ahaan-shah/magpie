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
        Modal::Goal(_) | Modal::Rule(_) | Modal::Txn(_) => 520.0,
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
            }
        });
    let close = resp.should_close() || resp.inner == Outcome::Close;
    if app.modal.is_some() {
        // A form opened another modal (e.g. confirm delete) — let it replace us.
        return;
    }
    if close && closing.is_none() {
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
        .map(|a| format!("{}  {}", icons::account_kind(a.kind), a.name))
        .unwrap_or_else(|| "Choose account".into());
    w::dropdown(ui, id, label, width, |ui| {
        for a in store.active_accounts() {
            let text = format!("{}  {}  ·  {}", icons::account_kind(a.kind), a.name, a.currency);
            ui.selectable_value(sel, a.id, text);
        }
    });
}

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
    w::dropdown(ui, id, label, width, |ui| {
        ui.selectable_value(sel, None, egui::RichText::new("Uncategorized").color(t.text2));
        for k in [CategoryKind::Expense, CategoryKind::Income] {
            if kind.is_some_and(|x| x != k) {
                continue;
            }
            ui.add_space(4.0);
            ui.label(w::faint(
                t,
                if k == CategoryKind::Expense {
                    "EXPENSES"
                } else {
                    "INCOME"
                },
            ));
            for c in store.categories().iter().filter(|c| c.kind == k && !c.archived) {
                let text = egui::RichText::new(format!("{}  {}", icons::glyph(&c.icon), c.name))
                    .color(w::readable(t, w::cat_color(c.color)));
                ui.selectable_value(sel, Some(c.id), text);
            }
        }
    });
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
    category: Option<RowId>,
    note: String,
    tags: String,
    cleared: bool,
    error: Option<String>,
    first_frame: bool,
    auto_category: bool,
}

impl TxnForm {
    pub fn new(store: &Store, today: Date) -> TxnForm {
        let account = store.default_account().unwrap_or(0);
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
            category: None,
            note: String::new(),
            tags: String::new(),
            cleared: true,
            error: None,
            first_frame: true,
            auto_category: true,
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
        if self.editing.is_none() || self.mode != 2 {
            let mut m = self.mode;
            let opts: &[&str] = if self.editing.is_some() {
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
            w::field_label(ui, t, "Payee");
            let payee_id = Id::new("txn-payee");
            let r = w::text_field(ui, t, payee_id, &mut self.payee, "Who was it?", ui.available_width());
            let payees = app
                .payees
                .get(app.store.version(), || analytics::payee_index(&app.store))
                .clone();
            let q = self.payee.trim().to_lowercase();
            if r.has_focus() && !q.is_empty() {
                let matches: Vec<_> = payees
                    .iter()
                    .filter(|p| p.name.to_lowercase().contains(&q) && p.name.to_lowercase() != q)
                    .take(6)
                    .collect();
                if !matches.is_empty() {
                    egui::Popup::new(payee_id.with("ac"), ctx.clone(), r.rect, ui.layer_id())
                        .open(true)
                        .width(r.rect.width())
                        .frame(w::popup_frame(t))
                        .show(|ui| {
                            for p in matches {
                                let cat = app.store.category_name(p.category).to_string();
                                let resp = ui.add(
                                    egui::Button::selectable(false, format!("{}   ·  {cat}", p.name))
                                        .min_size(vec2(ui.available_width(), 28.0)),
                                );
                                if resp.clicked() || resp.is_pointer_button_down_on() {
                                    self.payee = p.name.clone();
                                    if self.auto_category {
                                        self.category = p.category;
                                    }
                                }
                            }
                        });
                }
            }
            if r.changed() && self.auto_category {
                if let Some(p) = payees.iter().find(|p| p.name.to_lowercase() == q) {
                    self.category = p.category;
                }
            }
            ui.add_space(8.0);
            let store = &app.store;
            let before = self.category;
            let kind = if self.mode == 1 {
                CategoryKind::Income
            } else {
                CategoryKind::Expense
            };
            two(
                ui,
                |ui, wd| {
                    w::field_label(ui, t, "Category");
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
        error_line(ui, t, &self.error);

        let mut delete = false;
        let mut duplicate = false;
        let editing = self.editing.is_some();
        let (ok, cancel) = footer(ui, t, if editing { "Save" } else { "Add" }, |ui| {
            if editing {
                delete = w::danger(ui, t, Some(ph::TRASH), "Delete").clicked();
                if self.mode != 2 {
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
                archived: false,
                sort: 0,
            },
            opening: String::new(),
            error: None,
        }
    }

    pub fn edit(a: &Account) -> AccountForm {
        AccountForm {
            acc: a.clone(),
            opening: money::to_input(a.opening, a.currency),
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.acc.id != 0;
        title(
            ui,
            t,
            if editing { "Edit account" } else { "New account" },
            "Checking, savings, cards, cash or investments.",
        );
        w::field_label(ui, t, "Name");
        w::text_field(
            ui,
            t,
            Id::new("acc-name"),
            &mut self.acc.name,
            "e.g. Everyday checking",
            ui.available_width(),
        );
        ui.add_space(8.0);
        let acc = &mut self.acc;
        two(
            ui,
            |ui, wd| {
                w::field_label(ui, t, "Type");
                w::dropdown(
                    ui,
                    "acc-kind",
                    format!("{}  {}", icons::account_kind(acc.kind), acc.kind.label()),
                    wd,
                    |ui| {
                        for k in AccountKind::ALL {
                            ui.selectable_value(&mut acc.kind, k, format!("{}  {}", icons::account_kind(k), k.label()));
                        }
                    },
                );
            },
            |ui, wd| {
                w::field_label(ui, t, "Currency");
                currency_picker(ui, "acc-cur", &mut acc.currency, wd);
            },
        );
        ui.add_space(8.0);
        w::field_label(ui, t, "Starting balance");
        w::text_field(
            ui,
            t,
            Id::new("acc-open"),
            &mut self.opening,
            "0.00 (negative for a card balance owed)",
            ui.available_width(),
        );
        ui.add_space(8.0);
        w::field_label(ui, t, "Colour");
        color_row(ui, t, &mut self.acc.color);
        if editing {
            ui.add_space(8.0);
            w::toggle_row(
                ui,
                t,
                &mut self.acc.archived,
                "Archived (hidden from pickers and net worth)",
            );
        }
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
    w::dropdown(ui, id, label, width, |ui| {
        for info in money::CURRENCIES {
            if let Some(c) = Cur::new(info.code) {
                ui.selectable_value(cur, c, format!("{}  {}  {}", info.code, info.symbol.trim(), info.name));
            }
        }
    });
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
                t.hover
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
                    ui.selectable_value(fr, f, f.unit(iv));
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
        }
    }
    pub fn edit(c: &Category) -> CategoryForm {
        CategoryForm {
            cat: c.clone(),
            error: None,
        }
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let editing = self.cat.id != 0;
        title(ui, t, if editing { "Edit category" } else { "New category" }, "");
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
        if app
            .store
            .categories()
            .iter()
            .any(|c| c.id != self.cat.id && c.name.eq_ignore_ascii_case(&name))
        {
            self.error = Some("A category with that name already exists".into());
            return Outcome::Keep;
        }
        self.cat.name = name;
        if app.toasts.ok(app.store.save_category(self.cat.clone())).is_some() {
            app.toasts.success("Category saved");
        }
        Outcome::Close
    }
}

// ============================================================== Import

pub struct ImportForm {
    path: PathBuf,
    preview: CsvPreview,
    map: CsvMapping,
    account: RowId,
    error: Option<String>,
}

impl ImportForm {
    pub fn open(store: &Store, path: PathBuf) -> magpie_core::Result<ImportForm> {
        let preview = magpie_core::io::preview_csv(&path)?;
        let map = preview.guess.clone();
        Ok(ImportForm {
            path,
            preview,
            map,
            account: store.default_account().unwrap_or(0),
            error: None,
        })
    }

    fn col_picker(ui: &mut Ui, id: &str, headers: &[String], sel: &mut Option<usize>, width: f32) {
        let label = sel.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "—".into());
        w::dropdown(ui, id, label, width, |ui| {
            ui.selectable_value(sel, None, "—");
            for (i, h) in headers.iter().enumerate() {
                ui.selectable_value(sel, Some(i), h);
            }
        });
    }

    fn ui(&mut self, app: &mut App, ui: &mut Ui, t: &Theme) -> Outcome {
        let name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file.csv")
            .to_string();
        title(
            ui,
            t,
            "Import CSV",
            &format!(
                "{name} · {} rows found. Match the columns and pick an account.",
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
                    ui.selectable_value(df, f, f.label());
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
                            let amt = if self.map.amount.is_some() {
                                money::parse(&get(r, self.map.amount), cur)
                                    .map(|v| if self.map.invert { -v } else { v })
                            } else {
                                let d = money::parse(&get(r, self.map.debit), cur).unwrap_or(0).abs();
                                let c = money::parse(&get(r, self.map.credit), cur).unwrap_or(0).abs();
                                (d != 0 || c != 0).then_some(c - d)
                            };
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
        match magpie_core::io::read_csv(&app.store, &self.path, &self.map, self.account) {
            Ok(res) => {
                let skipped = res.skipped;
                if res.txns.is_empty() {
                    self.error = Some("No rows could be read with this mapping".into());
                    return Outcome::Keep;
                }
                match magpie_core::io::commit_import(&mut app.store, res, &format!("Import {name}")) {
                    Ok(n) => {
                        let extra = if skipped > 0 {
                            format!(" ({skipped} skipped)")
                        } else {
                            String::new()
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

// ============================================================= Confirm

pub enum ConfirmAction {
    DeleteTxns(Vec<RowId>),
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
            ConfirmAction::DeleteTxns(ids) => {
                if let Some(n) = app.toasts.ok(app.store.delete_txns(ids)) {
                    app.toasts
                        .undoable(format!("Deleted {n} transaction{}", if n == 1 { "" } else { "s" }));
                    app.ledger.clear_selection();
                }
            }
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
