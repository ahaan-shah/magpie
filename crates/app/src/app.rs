//! The application shell: sidebar, top bar, page transitions, global
//! shortcuts, background jobs, and the overlays (modals, palette, toasts).

use crate::forms::{self, Modal};
use crate::icons::ph;
use crate::palette::Palette;
use crate::theme::{self, Theme, ThemeState};
use crate::toasts::{self, Toasts};
use crate::views;
use crate::widgets;
use crate::{motion, receipts_cache::ReceiptCache};
use egui::{Align2, Color32, CornerRadius, Id, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use jiff::civil::Date;
use magpie_core::{Cur, Id as RowId, Store, analytics, recurring};
use std::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Page {
    Dashboard,
    Ledger,
    Budgets,
    Reports,
    Accounts,
    Recurring,
    Goals,
    Settings,
}

impl Page {
    pub const NAV: [Page; 7] = [
        Page::Dashboard,
        Page::Ledger,
        Page::Budgets,
        Page::Reports,
        Page::Accounts,
        Page::Recurring,
        Page::Goals,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::Ledger => "Transactions",
            Page::Budgets => "Budgets",
            Page::Reports => "Reports",
            Page::Accounts => "Accounts",
            Page::Recurring => "Recurring",
            Page::Goals => "Goals",
            Page::Settings => "Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Page::Dashboard => ph::SQUARES_FOUR,
            Page::Ledger => ph::LIST_BULLETS,
            Page::Budgets => ph::CHART_PIE_SLICE,
            Page::Reports => ph::CHART_LINE_UP,
            Page::Accounts => ph::WALLET,
            Page::Recurring => ph::ARROWS_CLOCKWISE,
            Page::Goals => ph::TARGET,
            Page::Settings => ph::GEAR_SIX,
        }
    }
}

type FxResult = Result<(Vec<(Cur, f64)>, String), String>;

pub struct App {
    pub store: Store,
    pub theme: ThemeState,
    pub page: Page,
    pub shown_at: f64,
    pub today: Date,
    pub toasts: Toasts,
    pub modal: Option<Modal>,
    pub modal_at: f64,
    pub modal_closing: Option<f64>,
    pub payees: Memo<u64, Vec<analytics::PayeeInfo>>,
    pub palette: Palette,
    pub ledger: views::ledger::State,
    pub dashboard: views::dashboard::State,
    pub budgets: views::budgets::State,
    pub reports: views::reports::State,
    pub accounts: views::accounts::State,
    pub goals: views::goals::State,
    pub settings: views::settings::State,
    pub onboarding: Option<views::onboarding::State>,
    pub receipts: ReceiptCache,
    pub collapsed: bool,
    fx_rx: Option<mpsc::Receiver<FxResult>>,
    pub fx_busy: bool,
    tour: Option<crate::tour::Tour>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Persisted {
    page: Page,
    collapsed: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, mut store: Store) -> App {
        theme::install_fonts(&cc.egui_ctx);
        if let Some(z) = std::env::var("MAGPIE_ZOOM").ok().and_then(|z| z.parse::<f32>().ok()) {
            cc.egui_ctx.set_zoom_factor(z.clamp(0.5, 3.0));
        }
        let theme = ThemeState::new(&cc.egui_ctx, &store.settings().theme);
        let persisted: Option<Persisted> = cc.storage.and_then(|s| eframe::get_value(s, "magpie"));
        let today = magpie_core::today();
        let mut toasts = Toasts::default();
        match recurring::post_due(&mut store, today) {
            Ok(0) => {}
            Ok(n) => toasts.info(format!(
                "Posted {n} recurring transaction{}",
                if n == 1 { "" } else { "s" }
            )),
            Err(e) => toasts.error(format!("Couldn't post recurring transactions: {e}")),
        }
        let onboarding = (store.is_empty() || !store.settings().onboarded).then(views::onboarding::State::new);
        let mut app = App {
            theme,
            page: persisted.as_ref().map(|p| p.page).unwrap_or(Page::Dashboard),
            shown_at: 0.0,
            today,
            toasts,
            modal: None,
            modal_at: 0.0,
            modal_closing: None,
            payees: Memo::default(),
            palette: Palette::default(),
            ledger: views::ledger::State::default(),
            dashboard: views::dashboard::State::default(),
            budgets: views::budgets::State::new(today),
            reports: views::reports::State::default(),
            accounts: views::accounts::State::default(),
            goals: views::goals::State::default(),
            settings: views::settings::State::default(),
            onboarding,
            receipts: ReceiptCache::default(),
            collapsed: persisted.map(|p| p.collapsed).unwrap_or(false),
            fx_rx: None,
            fx_busy: false,
            tour: crate::tour::Tour::from_env(),
            store,
        };
        app.maybe_refresh_fx(&cc.egui_ctx, false);
        app
    }

    pub fn t(&self) -> Theme {
        self.theme.current
    }

    pub fn go(&mut self, ctx: &egui::Context, page: Page) {
        if self.page != page {
            self.page = page;
            self.shown_at = ctx.input(|i| i.time);
        }
    }

    pub fn open_modal(&mut self, ctx: &egui::Context, m: Modal) {
        self.modal = Some(m);
        self.modal_closing = None;
        self.modal_at = ctx.input(|i| i.time);
    }

    pub fn set_theme(&mut self, ctx: &egui::Context, name: &str) {
        self.theme.switch(ctx, name);
        let n = name.to_string();
        if let Err(e) = self.store.update_settings(|s| s.theme = n) {
            self.toasts.error(e.to_string());
        }
    }

    pub fn undo(&mut self) {
        match self.store.undo() {
            Ok(Some(label)) => self.toasts.info(format!("Undid: {label}")),
            Ok(None) => self.toasts.info("Nothing to undo"),
            Err(e) => self.toasts.error(e.to_string()),
        }
    }

    pub fn redo(&mut self) {
        match self.store.redo() {
            Ok(Some(label)) => self.toasts.info(format!("Redid: {label}")),
            Ok(None) => {}
            Err(e) => self.toasts.error(e.to_string()),
        }
    }

    /// Currencies used anywhere other than the base.
    fn uses_foreign_currency(&self) -> bool {
        let base = self.store.base();
        self.store.accounts().iter().any(|a| a.currency != base)
            || self.store.goals().iter().any(|g| g.currency != base)
    }

    pub fn maybe_refresh_fx(&mut self, ctx: &egui::Context, force: bool) {
        if self.fx_busy {
            return;
        }
        let s = self.store.settings();
        let stale = s.fx_updated.as_deref() != Some(&self.today.to_string());
        if !force && (!s.fx_auto || !stale || !self.uses_foreign_currency()) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = magpie_core::fx::fetch_ecb().map_err(|e| e.to_string());
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.fx_rx = Some(rx);
        self.fx_busy = true;
    }

    fn poll_jobs(&mut self) {
        if let Some(rx) = &self.fx_rx
            && let Ok(r) = rx.try_recv()
        {
            self.fx_rx = None;
            self.fx_busy = false;
            match r {
                Ok((rates, date)) => {
                    if let Err(e) = self.store.apply_fetched_rates(&rates, &date) {
                        self.toasts.error(e.to_string());
                    } else if self.page == Page::Settings {
                        self.toasts.success(format!("Exchange rates updated ({date})"));
                    }
                }
                Err(e) => {
                    if self.page == Page::Settings {
                        self.toasts.error(format!("Couldn't fetch rates: {e}"));
                    }
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let pressed = |ctx: &egui::Context, m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(ctx, cmd, Key::K) {
            self.palette.toggle(ctx);
        }
        if self.modal.is_some() || self.palette.open {
            return;
        }
        if pressed(ctx, cmd, Key::N) {
            let m = forms::TxnForm::new(&self.store, self.today);
            self.open_modal(ctx, Modal::Txn(m));
        }
        if pressed(ctx, cmd | Modifiers::SHIFT, Key::Z) || pressed(ctx, cmd, Key::Y) {
            self.redo();
        } else if pressed(ctx, cmd, Key::Z) {
            self.undo();
        }
        if pressed(ctx, cmd, Key::F) {
            self.go(ctx, Page::Ledger);
            self.ledger.focus_search = true;
        }
        if pressed(ctx, cmd, Key::Comma) {
            self.go(ctx, Page::Settings);
        }
        let keys = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
        ];
        for (k, p) in keys.iter().zip(Page::NAV) {
            if pressed(ctx, cmd, *k) {
                self.go(ctx, p);
            }
        }
    }

    // ------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut Ui) {
        let t = self.t();
        let ctx = ui.ctx().clone();
        let narrow = ctx.content_rect().width() < 1100.0;
        let k = motion::toggle(
            &ctx,
            Id::new("sidebar-collapse"),
            self.collapsed || narrow,
            motion::STANDARD,
        );
        let width = motion::lerp(236.0, 72.0, k);
        egui::Panel::left("sidebar")
            .exact_size(width)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(t.sidebar)
                    .inner_margin(egui::Margin::symmetric(12, 16)),
            )
            .show(ui, |ui| {
                let full = ui.max_rect();
                ui.painter().vline(
                    full.right() + 12.0,
                    ui.clip_rect().y_range(),
                    Stroke::new(1.0, t.border),
                );
                // Logo
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                    logo(ui.painter(), r, &t);
                    if k < 0.6 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Magpie")
                                .font(theme::display(19.0))
                                .color(motion::with_alpha(t.text, 1.0 - k / 0.6)),
                        );
                    }
                });
                ui.add_space(20.0);

                let item_h = 40.0;
                let active_idx = Page::NAV.iter().position(|p| *p == self.page);
                let top = ui.cursor().top();
                if let Some(i) = active_idx {
                    let y = motion::tween(
                        &ctx,
                        Id::new("nav-indicator"),
                        top + i as f32 * (item_h + 4.0),
                        motion::STANDARD,
                    );
                    let r = Rect::from_min_size(pos2(full.left(), y), vec2(full.width(), item_h));
                    ui.painter().rect_filled(r, CornerRadius::same(10), t.accent_soft());
                    let bar = Rect::from_min_size(pos2(full.left() - 12.0, y + 10.0), vec2(3.0, item_h - 20.0));
                    ui.painter().rect_filled(bar, CornerRadius::same(2), t.accent);
                }
                for p in Page::NAV {
                    if nav_item(ui, &t, p, self.page == p, k, item_h).clicked() {
                        self.go(&ctx, p);
                    }
                    ui.add_space(4.0 - ui.spacing().item_spacing.y);
                }

                // Bottom section
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        let glyph = if self.collapsed { ph::CARET_RIGHT } else { ph::SIDEBAR };
                        if widgets::icon_button(ui, &t, glyph, if self.collapsed { "Expand" } else { "Collapse" })
                            .clicked()
                        {
                            self.collapsed = !self.collapsed;
                        }
                        if k < 0.5 {
                            let dark = t.dark;
                            let glyph = if dark { ph::SUN } else { ph::MOON };
                            if widgets::icon_button(ui, &t, glyph, "Toggle light / dark").clicked() {
                                let name = if dark { "Daylight" } else { "Midnight" };
                                self.set_theme(&ctx, name);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    if nav_item(ui, &t, Page::Settings, self.page == Page::Settings, k, item_h).clicked() {
                        self.go(&ctx, Page::Settings);
                    }
                    if k < 0.3 {
                        ui.add_space(8.0);
                        self.net_worth_card(ui, &t, 1.0 - k / 0.3);
                    }
                });
            });
    }

    fn net_worth_card(&mut self, ui: &mut Ui, t: &Theme, alpha: f32) {
        let nw = self.dashboard.net_worth(&self.store);
        let base = self.store.base();
        egui::Frame::new()
            .fill(motion::with_alpha(t.card, alpha))
            .stroke(Stroke::new(1.0, motion::with_alpha(t.border, alpha)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new("Net worth")
                        .font(theme::medium(11.5))
                        .color(motion::with_alpha(t.text3, alpha)),
                );
                ui.label(
                    egui::RichText::new(widgets::fmt_whole(nw, base))
                        .font(theme::display(18.0))
                        .color(motion::with_alpha(t.text, alpha)),
                );
            });
    }

    // ------------------------------------------------------------ top bar

    fn top_bar(&mut self, ui: &mut Ui) {
        let t = self.t();
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.page.title())
                        .font(theme::display(26.0))
                        .color(t.text),
                );
                ui.label(widgets::subtle(&t, long_date(self.today)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary(ui, &t, Some(ph::PLUS), "Add")
                    .on_hover_text("New transaction (Ctrl+N)")
                    .clicked()
                {
                    let m = forms::TxnForm::new(&self.store, self.today);
                    self.open_modal(&ctx, Modal::Txn(m));
                }
                ui.add_space(4.0);
                if search_pill(ui, &t).clicked() {
                    self.palette.toggle(&ctx);
                }
            });
        });
        ui.add_space(18.0);
    }

    fn content(&mut self, ui: &mut Ui) {
        let t = self.t();
        let ctx = ui.ctx().clone();
        let p = motion::appear(&ctx, self.shown_at, 0.0, 0.3);
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(t.bg))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let max_w = 1360.0;
                let pad_x = ((full.width() - max_w) / 2.0).max(28.0);
                let inner = Rect::from_min_max(
                    pos2(full.left() + pad_x, full.top() + 22.0),
                    pos2(full.right() - pad_x, full.bottom()),
                );
                let mut ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                self.top_bar(&mut ui);
                ui.set_opacity(p);
                ui.add_space((1.0 - p) * 12.0);
                match self.page {
                    Page::Ledger => views::ledger::show(self, &mut ui),
                    page => {
                        egui::ScrollArea::vertical()
                            .id_salt(("page", page as u8))
                            .auto_shrink([false, false])
                            .show(&mut ui, |ui| {
                                ui.set_max_width(ui.available_width() - 10.0);
                                match page {
                                    Page::Dashboard => views::dashboard::show(self, ui),
                                    Page::Budgets => views::budgets::show(self, ui),
                                    Page::Reports => views::reports::show(self, ui),
                                    Page::Accounts => views::accounts::show(self, ui),
                                    Page::Recurring => views::recurring::show(self, ui),
                                    Page::Goals => views::goals::show(self, ui),
                                    Page::Settings => views::settings::show(self, ui),
                                    Page::Ledger => {}
                                }
                                ui.add_space(40.0);
                            });
                    }
                }
            });
    }

    fn drop_overlay(&mut self, ctx: &egui::Context) {
        let t = self.t();
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let k = motion::toggle(ctx, Id::new("drop-overlay"), hovering, motion::STANDARD);
        if k > 0.01 {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop")));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, motion::with_alpha(t.bg, 0.8 * k));
            let r = screen.shrink(24.0);
            painter.rect_stroke(
                r,
                CornerRadius::same(18),
                Stroke::new(2.0, motion::with_alpha(t.accent, k)),
                egui::StrokeKind::Inside,
            );
            let msg = if self.ledger.selected_one().is_some() && self.page == Page::Ledger {
                "Drop to attach receipt"
            } else {
                "Drop a CSV to import"
            };
            painter.text(
                screen.center() - vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                ph::UPLOAD_SIMPLE,
                theme::regular(40.0),
                motion::with_alpha(t.accent, k),
            );
            painter.text(
                screen.center() + vec2(0.0, 22.0),
                Align2::CENTER_CENTER,
                msg,
                theme::semibold(16.0),
                motion::with_alpha(t.text, k),
            );
        }
        let dropped: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for path in dropped {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "csv" {
                match forms::ImportForm::open(&self.store, path) {
                    Ok(f) => self.open_modal(ctx, Modal::Import(Box::new(f))),
                    Err(e) => self.toasts.error(e.to_string()),
                }
            } else if let (Page::Ledger, Some(id)) = (self.page, self.ledger.selected_one()) {
                let r = magpie_core::receipts::attach(&mut self.store, id, &path);
                if self.toasts.ok(r).is_some() {
                    self.toasts.success("Receipt attached");
                }
            } else {
                self.toasts
                    .info("Select a transaction first, then drop a receipt onto it");
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.theme.tick(&ctx);
        let t = self.t();
        let now_day = magpie_core::today();
        if now_day != self.today {
            self.today = now_day;
            if let Ok(n) = recurring::post_due(&mut self.store, now_day)
                && n > 0
            {
                self.toasts.info(format!(
                    "Posted {n} recurring transaction{}",
                    if n == 1 { "" } else { "s" }
                ));
            }
        }
        // Wake up for the date rollover even when idle.
        ctx.request_repaint_after(std::time::Duration::from_secs(60));
        self.poll_jobs();

        if self.onboarding.is_some() {
            views::onboarding::show(self, ui);
            if let Some(a) = self.toasts.show(&ctx, &t)
                && a == toasts::Action::Undo
            {
                self.undo();
            }
            return;
        }

        if let Some(mut tour) = self.tour.take() {
            tour.step(self, &ctx);
            self.tour = Some(tour);
        }
        self.shortcuts(&ctx);
        self.sidebar(ui);
        self.content(ui);
        forms::show(self, &ctx);
        crate::palette::show(self, &ctx);
        self.drop_overlay(&ctx);
        if let Some(a) = self.toasts.show(&ctx, &t)
            && a == toasts::Action::Undo
        {
            self.undo();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            "magpie",
            &Persisted {
                page: self.page,
                collapsed: self.collapsed,
            },
        );
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.theme.current.bg.to_normalized_gamma_f32()
    }
}

fn nav_item(ui: &mut Ui, t: &Theme, page: Page, active: bool, collapse: f32, h: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let hover = motion::toggle(
        ui.ctx(),
        Id::new(("nav-h", page as u8)),
        resp.hovered() && !active,
        motion::MICRO,
    );
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(10), motion::with_alpha(t.hover, hover));
    }
    let fg = if active {
        t.text
    } else {
        motion::lerp_color(t.text2, t.text, hover)
    };
    let icon_c = if active { t.accent } else { fg };
    let icon_x = motion::lerp(rect.left() + 14.0, rect.center().x - 9.0, collapse);
    p.text(
        pos2(icon_x, rect.center().y),
        Align2::LEFT_CENTER,
        page.icon(),
        theme::regular(18.0),
        icon_c,
    );
    if collapse < 0.6 {
        let a = 1.0 - collapse / 0.6;
        let font = if active {
            theme::semibold(13.5)
        } else {
            theme::medium(13.5)
        };
        p.text(
            pos2(rect.left() + 44.0, rect.center().y),
            Align2::LEFT_CENTER,
            page.title(),
            font,
            motion::with_alpha(fg, a),
        );
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if collapse > 0.5 {
        resp.on_hover_text(page.title())
    } else {
        resp
    }
}

/// The Magpie mark: a rounded tile with an accent gradient and a bird.
pub fn logo(p: &egui::Painter, r: Rect, t: &Theme) {
    let top = motion::lerp_color(t.accent, Color32::WHITE, 0.2);
    let bottom = motion::lerp_color(t.accent, Color32::BLACK, 0.2);
    widgets::rounded_gradient(p, r, 10.0, top, bottom);
    p.text(
        r.center() + vec2(0.0, 0.5),
        Align2::CENTER_CENTER,
        ph::BIRD,
        theme::regular(20.0),
        t.on_accent,
    );
}

fn search_pill(ui: &mut Ui, t: &Theme) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(260.0, 34.0), Sense::click());
    let hover = motion::toggle(ui.ctx(), resp.id, resp.hovered(), motion::MICRO);
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(10),
        motion::lerp_color(t.card, t.hover, hover),
        Stroke::new(
            1.0,
            motion::lerp_color(t.border, motion::with_alpha(t.accent, 0.5), hover),
        ),
        egui::StrokeKind::Inside,
    );
    p.text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        ph::MAGNIFYING_GLASS,
        theme::regular(15.0),
        t.text3,
    );
    p.text(
        pos2(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Search or jump to…",
        theme::regular(13.0),
        t.text3,
    );
    let kbd = Rect::from_min_size(pos2(rect.right() - 52.0, rect.top() + 7.0), vec2(44.0, 20.0));
    p.rect_filled(kbd, CornerRadius::same(6), t.hover);
    p.text(
        kbd.center(),
        Align2::CENTER_CENTER,
        "Ctrl K",
        theme::medium(10.5),
        t.text2,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn long_date(d: Date) -> String {
    use jiff::civil::Weekday::*;
    let wd = match d.weekday() {
        Monday => "Monday",
        Tuesday => "Tuesday",
        Wednesday => "Wednesday",
        Thursday => "Thursday",
        Friday => "Friday",
        Saturday => "Saturday",
        Sunday => "Sunday",
    };
    format!("{wd}, {} {}", magpie_core::MONTH_NAMES[d.month() as usize - 1], d.day())
}

/// Cheap memoization keyed on anything comparable (usually the store
/// version plus view parameters).
pub struct Memo<K, V> {
    key: Option<K>,
    val: Option<V>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Memo { key: None, val: None }
    }
}

impl<K: PartialEq, V> Memo<K, V> {
    pub fn get(&mut self, key: K, compute: impl FnOnce() -> V) -> &V {
        if self.key.as_ref() != Some(&key) || self.val.is_none() {
            self.val = Some(compute());
            self.key = Some(key);
        }
        self.val.as_ref().expect("just set")
    }
    pub fn clear(&mut self) {
        self.key = None;
        self.val = None;
    }
}

pub fn account_balance(store: &Store, id: RowId) -> i64 {
    analytics::balances(store).get(&id).copied().unwrap_or(0)
}
