//! Ctrl+K command palette: fuzzy-search pages, actions, themes and payees.

use crate::app::{App, Page};
use crate::forms::{self, Modal};
use crate::icons::ph;
use crate::motion;
use crate::theme::{self, THEMES};
use crate::widgets as w;
use egui::{Align2, Color32, CornerRadius, Id, Key, Margin, Rect, Sense, Stroke, pos2, vec2};

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    query: String,
    cursor: usize,
    opened_at: f64,
    focus: bool,
}

impl Palette {
    pub fn toggle(&mut self, ctx: &egui::Context) {
        self.open = !self.open;
        if self.open {
            self.query.clear();
            self.cursor = 0;
            self.opened_at = ctx.input(|i| i.time);
            self.focus = true;
        }
    }
}

impl Palette {
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
    }
}

#[derive(Clone)]
enum Cmd {
    Go(Page),
    NewTxn,
    NewTransfer,
    NewAccount,
    NewGoal,
    NewRule,
    Theme(&'static str),
    Export(magpie_core::io::Format),
    Undo,
    Redo,
    RefreshFx,
    SearchLedger(String),
    Mode(bool),
}

struct Item {
    icon: &'static str,
    label: String,
    hint: &'static str,
    cmd: Cmd,
}

fn items(app: &mut App, q: &str) -> Vec<Item> {
    let mut v = Vec::new();
    let basic = app.basic();
    for p in Page::nav(basic).iter().chain([Page::Settings].iter()) {
        v.push(Item {
            icon: p.icon_in(basic),
            label: format!("Go to {}", p.title_in(basic)),
            hint: "Navigate",
            cmd: Cmd::Go(*p),
        });
    }
    v.push(Item {
        icon: if basic { ph::SLIDERS_HORIZONTAL } else { ph::FEATHER },
        label: if basic {
            "Switch to Advanced mode"
        } else {
            "Switch to Basic mode"
        }
        .into(),
        hint: shortcut!("Shift T"),
        cmd: Cmd::Mode(!basic),
    });
    v.push(Item {
        icon: ph::PLUS,
        label: "New transaction".into(),
        hint: shortcut!("N"),
        cmd: Cmd::NewTxn,
    });
    if !basic {
        v.push(Item {
            icon: ph::ARROWS_LEFT_RIGHT,
            label: "New transfer".into(),
            hint: "Action",
            cmd: Cmd::NewTransfer,
        });
    }
    v.push(Item {
        icon: ph::WALLET,
        label: "New account".into(),
        hint: "Action",
        cmd: Cmd::NewAccount,
    });
    if !basic {
        v.push(Item {
            icon: ph::TARGET,
            label: "New savings goal".into(),
            hint: "Action",
            cmd: Cmd::NewGoal,
        });
        v.push(Item {
            icon: ph::ARROWS_CLOCKWISE,
            label: "New recurring transaction".into(),
            hint: "Action",
            cmd: Cmd::NewRule,
        });
    }
    if let Some(l) = app.store.undo_label() {
        v.push(Item {
            icon: ph::ARROW_COUNTER_CLOCKWISE,
            label: format!("Undo: {l}"),
            hint: shortcut!("Z"),
            cmd: Cmd::Undo,
        });
    }
    if let Some(l) = app.store.redo_label() {
        v.push(Item {
            icon: ph::ARROW_CLOCKWISE,
            label: format!("Redo: {l}"),
            hint: shortcut!("Shift Z"),
            cmd: Cmd::Redo,
        });
    }
    use magpie_core::io::Format;
    for (f, name, icon) in [
        (Format::Csv, "CSV", ph::FILE_CSV),
        (Format::Xlsx, "Excel", ph::FILE_XLS),
        (Format::Json, "JSON", ph::BRACKETS_CURLY),
    ] {
        v.push(Item {
            icon,
            label: format!("Export all transactions as {name}"),
            hint: "~/Downloads",
            cmd: Cmd::Export(f),
        });
    }
    if !basic {
        v.push(Item {
            icon: ph::CURRENCY_CIRCLE_DOLLAR,
            label: "Refresh exchange rates".into(),
            hint: "ECB",
            cmd: Cmd::RefreshFx,
        });
    }
    for th in THEMES {
        v.push(Item {
            icon: ph::PALETTE,
            label: format!("Theme: {}", th.name),
            hint: "Appearance",
            cmd: Cmd::Theme(th.name),
        });
    }
    if !q.is_empty() {
        let payees = app
            .payees
            .get(app.store.version(), || magpie_core::analytics::payee_index(&app.store));
        for p in payees.iter().filter(|p| fuzzy(q, &p.name).is_some()).take(5) {
            v.push(Item {
                icon: ph::MAGNIFYING_GLASS,
                label: format!("Transactions with “{}”", p.name),
                hint: "Search",
                cmd: Cmd::SearchLedger(p.name.clone()),
            });
        }
        v.push(Item {
            icon: ph::MAGNIFYING_GLASS,
            label: format!("Search transactions for “{q}”"),
            hint: "Search",
            cmd: Cmd::SearchLedger(q.to_string()),
        });
    }
    if q.is_empty() {
        return v;
    }
    let mut scored: Vec<(i32, Item)> = v
        .into_iter()
        .filter_map(|i| fuzzy(q, &i.label).map(|s| (s, i)))
        .collect();
    scored.sort_by_key(|(s, _)| -*s);
    // The "search transactions for <query>" catch-all always goes last, so
    // typing "paper" picks the Paper theme rather than a text search.
    let q_lower = q.to_lowercase();
    let is_fallback = |i: &Item| matches!(&i.cmd, Cmd::SearchLedger(x) if x.to_lowercase() == q_lower);
    let (fallback, mut rest): (Vec<_>, Vec<_>) = scored.into_iter().map(|(_, i)| i).partition(is_fallback);
    rest.extend(fallback);
    rest
}

/// Subsequence match with bonuses for word starts and contiguous runs.
fn fuzzy(q: &str, s: &str) -> Option<i32> {
    let s_l = s.to_lowercase();
    let q_l = q.to_lowercase();
    if s_l.contains(&q_l) {
        return Some(1000 - s.len() as i32 + if s_l.starts_with(&q_l) { 200 } else { 0 });
    }
    let chars: Vec<char> = s_l.chars().collect();
    let mut score = 0;
    let mut i = 0;
    let mut prev = usize::MAX - 1;
    for qc in q_l.chars().filter(|c| !c.is_whitespace()) {
        let mut found = false;
        while i < chars.len() {
            if chars[i] == qc {
                score += if i == prev + 1 { 8 } else { 1 };
                if i == 0 || chars[i - 1] == ' ' {
                    score += 6;
                }
                prev = i;
                i += 1;
                found = true;
                break;
            }
            i += 1;
        }
        if !found {
            return None;
        }
    }
    Some(score)
}

fn run(app: &mut App, ctx: &egui::Context, cmd: Cmd) {
    crate::diag::crumb("palette command");
    match cmd {
        Cmd::Go(p) => app.go(ctx, p),
        Cmd::NewTxn => {
            let f = forms::TxnForm::new(&app.store, app.today);
            app.open_modal(ctx, Modal::Txn(f));
        }
        Cmd::NewTransfer => {
            let f = forms::TxnForm::transfer(&app.store, app.today);
            app.open_modal(ctx, Modal::Txn(f));
        }
        Cmd::NewAccount => {
            let f = forms::AccountForm::new(&app.store);
            app.open_modal(ctx, Modal::Account(f));
        }
        Cmd::NewGoal => {
            let f = forms::GoalForm::new(&app.store, app.today);
            app.open_modal(ctx, Modal::Goal(f));
        }
        Cmd::NewRule => {
            let f = forms::RuleForm::new(&app.store, app.today);
            app.open_modal(ctx, Modal::Rule(f));
        }
        Cmd::Theme(name) => app.set_theme(ctx, name),
        Cmd::Export(f) => crate::views::settings::export_all(app, f),
        Cmd::Undo => app.undo(),
        Cmd::Redo => app.redo(),
        Cmd::RefreshFx => app.maybe_refresh_fx(ctx, true),
        Cmd::SearchLedger(q) => {
            app.go(ctx, Page::Ledger);
            if app.basic() {
                app.basic_ledger.set_search(q);
            } else {
                app.ledger.set_search(q);
            }
        }
        Cmd::Mode(basic) => app.set_basic(ctx, basic),
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.palette.open {
        return;
    }
    let t = app.t();
    let p = motion::appear(ctx, app.palette.opened_at, 0.0, 0.2);
    let screen = ctx.content_rect();
    // Backdrop
    let bg = egui::Area::new(Id::new("palette-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let r = ui.allocate_rect(screen, Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha((110.0 * p) as u8));
            r
        })
        .inner;
    if bg.clicked() || ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.palette.open = false;
        return;
    }
    let q = app.palette.query.clone();
    let list = items(app, q.trim());
    let n = list.len();
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        app.palette.cursor = (app.palette.cursor + 1).min(n.saturating_sub(1));
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        app.palette.cursor = app.palette.cursor.saturating_sub(1);
    }
    let enter = ctx.input(|i| i.key_pressed(Key::Enter));
    let width = 580.0f32.min(screen.width() - 40.0);
    let pos = pos2(
        screen.center().x - width / 2.0,
        screen.top() + screen.height() * 0.16 - (1.0 - p) * 12.0,
    );
    let mut chosen = None;
    egui::Area::new(Id::new("palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_opacity(p);
            egui::Frame::new()
                .fill(t.elevated)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(16))
                .shadow(egui::Shadow {
                    offset: [0, 20],
                    blur: 50,
                    spread: 0,
                    color: t.shadow(),
                })
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(ph::MAGNIFYING_GLASS)
                                .font(theme::regular(18.0))
                                .color(t.text3),
                        );
                        let id = Id::new("palette-input");
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut app.palette.query)
                                .id(id)
                                .font(theme::regular(16.0))
                                .hint_text(egui::RichText::new("Type a command, page, theme or payee…").color(t.text3))
                                .frame(egui::Frame::NONE)
                                .desired_width(width - 60.0),
                        );
                        if app.palette.focus {
                            r.request_focus();
                            app.palette.focus = false;
                        }
                        if r.changed() {
                            app.palette.cursor = 0;
                        }
                    });
                    ui.add_space(4.0);
                    ui.painter()
                        .hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, t.border));
                    ui.add_space(6.0);
                    if list.is_empty() {
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| ui.label(w::subtle(&t, "No matches")));
                        ui.add_space(10.0);
                    }
                    crate::widgets::scroll_area().max_height(360.0).show(ui, |ui| {
                        let cur = app.palette.cursor.min(n.saturating_sub(1));
                        for (i, it) in list.iter().enumerate() {
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                                app.palette.cursor = i;
                            }
                            let sel = i == cur;
                            if sel {
                                let y = motion::tween(ui.ctx(), Id::new("palette-sel"), rect.top(), motion::MICRO);
                                let hl = Rect::from_min_size(pos2(rect.left(), y), rect.size());
                                ui.painter().rect_filled(hl, CornerRadius::same(10), t.accent_soft());
                                if enter {
                                    ui.scroll_to_rect(rect, None);
                                }
                                if ui.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) {
                                    ui.scroll_to_rect(rect, None);
                                }
                            }
                            let pnt = ui.painter();
                            pnt.text(
                                pos2(rect.left() + 14.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                it.icon,
                                theme::regular(17.0),
                                if sel { t.accent } else { t.text2 },
                            );
                            pnt.text(
                                pos2(rect.left() + 42.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                &it.label,
                                theme::medium(13.5),
                                t.text,
                            );
                            pnt.text(
                                pos2(rect.right() - 12.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                it.hint,
                                theme::regular(11.5),
                                t.text3,
                            );
                            if resp.clicked() || (sel && enter) {
                                chosen = Some(it.cmd.clone());
                            }
                        }
                    });
                });
        });
    if let Some(cmd) = chosen {
        app.palette.open = false;
        run(app, ctx, cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_ranks_prefix_and_word_starts() {
        assert!(fuzzy("dash", "Go to Dashboard").is_some());
        assert!(fuzzy("gtd", "Go to Dashboard").is_some());
        assert!(fuzzy("xyz", "Go to Dashboard").is_none());
        assert!(fuzzy("theme", "Theme: Nord").unwrap() > fuzzy("theme", "Go to Settings theme").unwrap_or(0) - 1000);
    }
}
