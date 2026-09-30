//! Accounts: balances, net worth split into assets and liabilities, and
//! per-account 90-day sparklines.

use crate::app::{App, Memo, Page};
use crate::forms::{self, Modal};
use crate::icons::{self, ph};
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets::{self as w, charts};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::{Account, AccountKind, Id as RowId, Store, analytics};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    show_archived: bool,
    sel: Option<usize>,
    memo: Memo<(u64, Date), Rc<Data>>,
}

struct Data {
    balances: HashMap<RowId, i64>,
    series: HashMap<RowId, Vec<f32>>,
    assets: i64,
    debts: i64,
}

fn build(store: &Store, today: Date) -> Data {
    let balances = analytics::balances(store);
    let from = today.checked_sub(jiff::Span::new().days(89)).unwrap_or(today);
    let series = store
        .accounts()
        .iter()
        .map(|a| {
            (
                a.id,
                analytics::account_series(store, a.id, from, today)
                    .into_iter()
                    .map(|v| v as f32)
                    .collect(),
            )
        })
        .collect();
    let (mut assets, mut debts) = (0, 0);
    for a in store.accounts().iter().filter(|a| !a.archived) {
        let b = store.to_base(balances.get(&a.id).copied().unwrap_or(0), a.currency);
        if b >= 0 {
            assets += b;
        } else {
            debts += -b;
        }
    }
    Data {
        balances,
        series,
        assets,
        debts,
    }
}

enum Act {
    Edit(RowId),
    Open(RowId),
    New,
    Transfer,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let today = app.today;
    let d = app
        .accounts
        .memo
        .get((app.store.version(), today), || Rc::new(build(&app.store, today)))
        .clone();
    let store = &app.store;
    let base = store.base();
    let shown = app.shown_at;
    let mut acts = Vec::new();

    // Keyboard: arrows move across the account grid (3 wide), Enter opens
    // its transactions, E edits.
    let groups: [(&str, &[AccountKind]); 3] = [
        (
            "Cash & bank",
            &[AccountKind::Checking, AccountKind::Savings, AccountKind::Cash],
        ),
        ("Credit cards", &[AccountKind::Credit]),
        ("Investments", &[AccountKind::Investment]),
    ];
    let order: Vec<RowId> = groups
        .iter()
        .flat_map(|(_, kinds)| {
            store
                .accounts()
                .iter()
                .filter(|a| kinds.contains(&a.kind) && !a.archived)
                .map(|a| a.id)
        })
        .collect();
    let keys = app.keys;
    let mut moved = false;
    if !order.is_empty() {
        let last = order.len() - 1;
        let cur = app.accounts.sel;
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
            app.accounts.sel = next;
            moved = true;
        }
        if let Some(id) = app.accounts.sel.and_then(|i| order.get(i)) {
            if keys.enter {
                acts.push(Act::Open(*id));
            }
            if keys.edit {
                acts.push(Act::Edit(*id));
            }
        }
    }
    let selected_id = app.accounts.sel.and_then(|i| order.get(i)).copied();

    ui.horizontal(|ui| {
        ui.label(w::subtle(&t, "Everything you own and owe, in one place."));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::primary(ui, &t, Some(ph::PLUS), "Account")
                .on_hover_text(concat!("New account (", shortcut!("Shift A"), ")"))
                .clicked()
            {
                acts.push(Act::New);
            }
            if w::secondary(ui, &t, Some(ph::ARROWS_LEFT_RIGHT), "Transfer")
                .on_hover_text(concat!("Move money between accounts (", shortcut!("T"), ")"))
                .clicked()
            {
                acts.push(Act::Transfer);
            }
        });
    });
    ui.add_space(12.0);

    w::grid_row(ui, 118.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
        w::with_reveal(ui, shown, i, rect, |ui, rect| {
            w::card_in(ui, &t, rect, |ui| {
                let (label, v, c) = match i {
                    0 => ("Net worth", d.assets - d.debts, t.text),
                    1 => ("Assets", d.assets, t.pos),
                    _ => ("Liabilities", d.debts, t.neg),
                };
                ui.label(w::faint(&t, label));
                w::animated_amount(ui, Id::new(("acc-kpi", i)), v, base, theme::display(26.0), c);
                if i == 0 {
                    ui.label(w::faint(&t, format!("across {} accounts", order.len())));
                } else {
                    let total = (d.assets + d.debts).max(1);
                    let frac = v as f32 / total as f32;
                    w::progress(ui, &t, Id::new(("acc-share", i)), frac, None, c, 6.0);
                }
            })
        })
    });

    let mut idx = 3;
    for (title, kinds) in groups {
        let accs: Vec<&Account> = store
            .accounts()
            .iter()
            .filter(|a| kinds.contains(&a.kind) && !a.archived)
            .collect();
        if accs.is_empty() {
            continue;
        }
        let total: i64 = accs
            .iter()
            .map(|a| store.to_base(d.balances.get(&a.id).copied().unwrap_or(0), a.currency))
            .sum();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).font(theme::semibold(15.0)).color(t.text));
            ui.label(w::subtle(&t, w::fmt_money(total, base)));
        });
        ui.add_space(8.0);
        for chunk in accs.chunks(3) {
            w::grid_row(ui, 158.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
                if let Some(a) = chunk.get(i) {
                    idx += 1;
                    w::with_reveal(ui, shown, idx, rect, |ui, rect| {
                        let sel = selected_id == Some(a.id);
                        account_card(ui, &t, store, a, &d, rect, &mut acts, sel, sel && moved)
                    });
                }
            });
        }
    }

    let archived: Vec<&Account> = store.accounts().iter().filter(|a| a.archived).collect();
    if !archived.is_empty() {
        ui.horizontal(|ui| {
            w::toggle_row(
                ui,
                &t,
                &mut app.accounts.show_archived,
                &format!("Show {} archived", archived.len()),
            );
        });
        if app.accounts.show_archived {
            ui.add_space(8.0);
            for chunk in archived.chunks(3) {
                w::grid_row(ui, 158.0, &[1.0, 1.0, 1.0], |i, ui, rect| {
                    if let Some(a) = chunk.get(i) {
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                        c.set_opacity(0.6);
                        account_card(&mut c, &t, store, a, &d, rect, &mut acts, false, false);
                    }
                });
            }
        }
    }
    if store.accounts().is_empty() {
        w::empty_state(
            ui,
            &t,
            ph::WALLET,
            "No accounts",
            "Add your first account to get started.",
        );
    }

    let ctx = ui.ctx().clone();
    for a in acts {
        match a {
            Act::Edit(id) => {
                if let Some(acc) = app.store.account(id) {
                    let f = forms::AccountForm::edit(acc);
                    app.open_modal(&ctx, Modal::Account(f));
                }
            }
            Act::Open(id) => {
                app.ledger.show_account(id);
                app.go(&ctx, Page::Ledger);
            }
            Act::New => {
                let f = forms::AccountForm::new(&app.store);
                app.open_modal(&ctx, Modal::Account(f));
            }
            Act::Transfer => {
                let f = forms::TxnForm::transfer(&app.store, app.today);
                app.open_modal(&ctx, Modal::Txn(f));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn account_card(
    ui: &mut Ui,
    t: &Theme,
    store: &Store,
    a: &Account,
    d: &Data,
    rect: Rect,
    acts: &mut Vec<Act>,
    selected: bool,
    scroll_into_view: bool,
) {
    let resp = ui.interact(rect, Id::new(("acc-card", a.id)), Sense::click());
    if scroll_into_view {
        ui.scroll_to_rect(rect, None);
    }
    // Geometric hover: the edit button sits on top of the card, and using
    // `resp.hovered()` made the card "un-hover" under it — the button then
    // vanished, flickered, and clicks fell through to the card.
    let over = ui.rect_contains_pointer(rect);
    let h = motion::toggle(ui.ctx(), Id::new(("acc-h", a.id)), over || selected, motion::MICRO);
    let lift = rect.translate(vec2(0.0, -2.0 * h));
    let color = w::cat_color(a.color);
    w::card_in(ui, t, lift, |ui| {
        let p = ui.painter();
        if h > 0.0 {
            p.rect_stroke(
                lift,
                CornerRadius::same(theme::RADIUS),
                Stroke::new(1.0, motion::with_alpha(color, 0.6 * h)),
                egui::StrokeKind::Inside,
            );
        }
        ui.horizontal(|ui| {
            w::icon_badge(ui, t, icons::account_kind(a.kind), color, 34.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&a.name).font(theme::semibold(14.0)).color(t.text));
                ui.label(w::faint(t, format!("{} · {}", a.kind.label(), a.currency)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if (over || selected) && w::icon_button(ui, t, ph::PENCIL_SIMPLE, "Edit account (E)").clicked() {
                    acts.push(Act::Edit(a.id));
                }
            });
        });
        let bal = d.balances.get(&a.id).copied().unwrap_or(0);
        ui.add_space(6.0);
        let c = if bal < 0 { t.neg } else { t.text };
        w::animated_amount(ui, Id::new(("acc-bal", a.id)), bal, a.currency, theme::display(22.0), c);
        if a.currency != store.base() {
            ui.label(w::faint(
                t,
                format!("≈ {}", w::fmt_money(store.to_base(bal, a.currency), store.base())),
            ));
        }
        if let Some(s) = d.series.get(&a.id) {
            let spark = Rect::from_min_max(
                pos2(lift.right() - 130.0, lift.bottom() - 58.0),
                pos2(lift.right() - 20.0, lift.bottom() - 22.0),
            );
            charts::sparkline(ui, Id::new(("acc-spark", a.id)), spark, s, color);
            let first = s.first().copied().unwrap_or(0.0);
            let last = s.last().copied().unwrap_or(0.0);
            let delta = (last - first) as i64;
            let c = if delta >= 0 { t.pos } else { t.neg };
            ui.painter().text(
                pos2(spark.right(), spark.top() - 6.0),
                Align2::RIGHT_BOTTOM,
                format!("{} · 90d", w::fmt_signed(delta, a.currency)),
                theme::regular(11.0),
                c,
            );
        }
    });
    if resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("View transactions")
        .clicked()
    {
        acts.push(Act::Open(a.id));
    }
}
