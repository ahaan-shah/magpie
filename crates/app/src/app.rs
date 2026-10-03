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
use magpie_core::{Cur, Store, analytics, recurring};
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

    /// Basic mode's pages, in sidebar order.
    pub const BASIC: [Page; 4] = [Page::Dashboard, Page::Ledger, Page::Budgets, Page::Accounts];

    /// The sidebar pages for a mode (Settings sits apart, at the bottom).
    pub fn nav(basic: bool) -> &'static [Page] {
        if basic { &Page::BASIC } else { &Page::NAV }
    }

    /// Whether the page exists in Basic mode.
    pub fn in_basic(self) -> bool {
        self == Page::Settings || Page::BASIC.contains(&self)
    }

    /// The page's name as shown in a mode: Basic calls the dashboard Home.
    pub fn title_in(self, basic: bool) -> &'static str {
        if basic && self == Page::Dashboard {
            "Home"
        } else {
            self.title()
        }
    }

    pub fn icon_in(self, basic: bool) -> &'static str {
        if basic && self == Page::Dashboard {
            ph::HOUSE
        } else {
            self.icon()
        }
    }

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

/// Arrow/Enter/Delete presses this frame, for pages with keyboard
/// selection. Only filled when no text field, modal or palette has focus.
#[derive(Clone, Copy, Default)]
pub struct NavKeys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub edit: bool,
    pub delete: bool,
    pub page_up: bool,
    pub page_down: bool,
    pub home: bool,
    pub end: bool,
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
    pub basic_ledger: views::basic_ledger::State,
    pub dashboard: views::dashboard::State,
    pub budgets: views::budgets::State,
    pub reports: views::reports::State,
    pub accounts: views::accounts::State,
    pub goals: views::goals::State,
    pub settings: views::settings::State,
    pub onboarding: Option<views::onboarding::State>,
    /// When onboarding handed over: the app fades in from the background.
    pub intro_at: Option<f64>,
    /// A mode switch in progress: when it started, and to which mode.
    mode_fade: Option<(f64, bool)>,
    pub receipts: ReceiptCache,
    pub dialogs: crate::dialogs::Dialogs,
    pub updater: crate::updater::Updater,
    pub collapsed: bool,
    fx_rx: Option<mpsc::Receiver<FxResult>>,
    pub fx_busy: bool,
    tour: Option<crate::tour::Tour>,
    pub keys: NavKeys,
    panics: u32,
    last_frame: Option<std::time::Instant>,
}

/// Maximum frames per second while something is animating.
const FRAME_CAP: f32 = 144.0;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Persisted {
    page: Page,
    collapsed: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> App {
        let persisted: Option<Persisted> = cc.storage.and_then(|s| eframe::get_value(s, "magpie"));
        App::with_context(&cc.egui_ctx, persisted, store)
    }

    /// Builds the app against any egui context (the real window, or a
    /// headless one in tests).
    pub(crate) fn with_context(ctx: &egui::Context, persisted: Option<Persisted>, mut store: Store) -> App {
        theme::install_fonts(ctx, &store.settings().font);
        // Magpie owns zoom (so it can be saved); turn off egui's own keys.
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            // egui's default is 40pt per wheel notch, which feels sluggish.
            o.input_options.line_scroll_speed = 100.0;
        });
        let zoom = std::env::var("MAGPIE_ZOOM")
            .ok()
            .and_then(|z| z.parse::<f32>().ok())
            .unwrap_or(store.settings().ui_scale);
        ctx.set_zoom_factor(zoom.clamp(0.5, 3.0));
        widgets::set_scroll_speed(store.settings().scroll_speed);
        let theme = ThemeState::new(ctx, &store.settings().theme);
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
        let page = persisted
            .as_ref()
            .map(|p| p.page)
            .filter(|p| !store.settings().basic || p.in_basic())
            .unwrap_or(Page::Dashboard);
        let mut app = App {
            theme,
            page,
            shown_at: 0.0,
            today,
            toasts,
            modal: None,
            modal_at: 0.0,
            modal_closing: None,
            payees: Memo::default(),
            palette: Palette::default(),
            ledger: views::ledger::State::default(),
            basic_ledger: views::basic_ledger::State::new(today),
            dashboard: views::dashboard::State::default(),
            budgets: views::budgets::State::new(today),
            reports: views::reports::State::default(),
            accounts: views::accounts::State::default(),
            goals: views::goals::State::default(),
            settings: views::settings::State::default(),
            onboarding,
            intro_at: None,
            mode_fade: None,
            receipts: ReceiptCache::default(),
            dialogs: crate::dialogs::Dialogs::default(),
            updater: crate::updater::Updater::default(),
            collapsed: persisted.map(|p| p.collapsed).unwrap_or(false),
            fx_rx: None,
            fx_busy: false,
            tour: crate::tour::Tour::from_env(),
            keys: NavKeys::default(),
            panics: 0,
            last_frame: None,
            store,
        };
        app.maybe_refresh_fx(ctx, false);
        app
    }

    pub fn t(&self) -> Theme {
        self.theme.current
    }

    /// True in Basic mode: fewer pages, simpler forms, plainer words.
    pub fn basic(&self) -> bool {
        self.store.settings().basic
    }

    /// Switches between Basic and Advanced: the whole app fades out, changes
    /// mode while hidden, and fades back in (see `mode_fade`).
    pub fn set_basic(&mut self, ctx: &egui::Context, basic: bool) {
        if self.mode_fade.is_some() || self.basic() == basic {
            return;
        }
        self.mode_fade = Some((ctx.input(|i| i.time), basic));
        ctx.request_repaint();
    }

    /// Fades the app out and back in around a mode switch, so the whole app
    /// visibly becomes the other mode.
    fn mode_fade(&mut self, ctx: &egui::Context, t: &Theme) {
        const OUT: f32 = 0.18;
        const IN: f32 = 0.34;
        let Some((at, basic)) = self.mode_fade else { return };
        let e = (ctx.input(|i| i.time) - at) as f32;
        let cover = if e < OUT {
            motion::ease_in_out(e / OUT)
        } else {
            if self.basic() != basic {
                self.apply_basic(ctx, basic);
            }
            1.0 - motion::ease_out((e - OUT) / IN)
        };
        if e >= OUT + IN {
            self.mode_fade = None;
            return;
        }
        // Above the panels, below modals and toasts.
        ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("mode-fade")))
            .rect_filled(ctx.content_rect(), 0.0, motion::with_alpha(t.bg, cover));
        ctx.request_repaint();
    }

    /// The switch itself, done while the app is hidden: sidebar and page
    /// jump straight to the new mode and the page's cards reveal again.
    fn apply_basic(&mut self, ctx: &egui::Context, basic: bool) {
        crate::diag::crumb(format!("mode {}", if basic { "basic" } else { "advanced" }));
        if self
            .toasts
            .ok(self.store.update_settings(|s| s.basic = basic))
            .is_none()
        {
            return;
        }
        if basic && !self.page.in_basic() {
            self.page = Page::Dashboard;
        }
        self.ledger.clear_selection();
        for p in Page::NAV {
            motion::snap(ctx, Id::new(("nav-vis", p as u8)));
        }
        motion::snap(ctx, Id::new("nav-indicator"));
        self.shown_at = ctx.input(|i| i.time);
        self.toasts.info(if basic {
            concat!("Basic mode. ", shortcut!("Shift T"), " brings everything back.")
        } else {
            concat!("Advanced mode. ", shortcut!("Shift T"), " keeps it simple.")
        });
    }

    pub fn go(&mut self, ctx: &egui::Context, page: Page) {
        // Pages that Basic mode doesn't have land on Home instead.
        let page = if self.basic() && !page.in_basic() {
            Page::Dashboard
        } else {
            page
        };
        if self.page != page {
            crate::diag::crumb(format!("go {page:?}"));
            self.page = page;
            self.shown_at = ctx.input(|i| i.time);
        }
    }

    pub fn open_modal(&mut self, ctx: &egui::Context, m: Modal) {
        crate::diag::crumb(format!("open modal {}", m.name()));
        self.modal = Some(m);
        self.modal_closing = None;
        self.modal_at = ctx.input(|i| i.time);
    }

    pub fn set_theme(&mut self, ctx: &egui::Context, name: &str) {
        crate::diag::crumb(format!("theme {name}"));
        self.theme.switch(ctx, name);
        let n = name.to_string();
        if let Err(e) = self.store.update_settings(|s| s.theme = n) {
            self.toasts.error(e.to_string());
        }
    }

    /// Called after a frame panicked (already logged by the panic hook).
    fn recover(&mut self, ctx: &egui::Context) {
        self.panics += 1;
        self.modal = None;
        self.modal_closing = None;
        self.palette.open = false;
        self.onboarding = None;
        self.intro_at = None;
        motion::zoom_app(ctx, 1.0);
        if self.panics > 1 {
            // Something on this page keeps failing; get somewhere safe.
            self.page = Page::Dashboard;
        }
        self.toasts
            .error("Something went wrong and Magpie recovered. Details were saved to magpie-diagnostics.log.");
        ctx.request_repaint();
    }

    /// Sets and remembers the interface zoom (1.0 = 100%).
    pub fn set_ui_scale(&mut self, ctx: &egui::Context, scale: f32) {
        let scale = (scale * 20.0).round() / 20.0;
        let scale = scale.clamp(0.7, 2.0);
        ctx.set_zoom_factor(scale);
        crate::diag::crumb(format!("ui scale {scale}"));
        let r = self.store.update_settings(|s| s.ui_scale = scale);
        self.toasts.ok(r);
    }

    /// Switches the interface font (live) and remembers it.
    pub fn set_font(&mut self, ctx: &egui::Context, name: &str) {
        theme::install_fonts(ctx, name);
        crate::diag::crumb(format!("font {name}"));
        let n = name.to_string();
        let r = self.store.update_settings(|s| s.font = n);
        self.toasts.ok(r);
    }

    /// Sets and remembers the scroll-wheel speed multiplier.
    pub fn set_scroll_speed(&mut self, v: f32) {
        widgets::set_scroll_speed(v);
        let r = self.store.update_settings(|s| s.scroll_speed = widgets::scroll_speed());
        self.toasts.ok(r);
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
        if self.fx_busy || headless() {
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

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        use crate::dialogs::Purpose;
        if let Some((purpose, Some(path))) = self.dialogs.poll() {
            match purpose {
                Purpose::ImportCsv => match forms::ImportForm::open(&self.store, path) {
                    Ok(f) => self.open_modal(ctx, Modal::Import(Box::new(f))),
                    Err(e) => self.toasts.error(e.to_string()),
                },
                Purpose::ImportFirst(balance) => match forms::ImportForm::open(&self.store, path) {
                    Ok(mut f) => {
                        f.balance_today = balance;
                        self.open_modal(ctx, Modal::Import(Box::new(f)));
                    }
                    Err(e) => self.toasts.error(e.to_string()),
                },
                Purpose::Attach(id) => {
                    let r = magpie_core::receipts::attach(&mut self.store, id, &path);
                    if self.toasts.ok(r).is_some() {
                        self.toasts.success("Receipt attached");
                    }
                }
                Purpose::Backup => match self.store.backup_to(&path) {
                    Ok(()) => self.toasts.success(format!("Backed up to {}", path.display())),
                    Err(e) => self.toasts.error(e.to_string()),
                },
            }
        }
        if let Some(rx) = &self.fx_rx
            && matches!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected))
        {
            // The fetch thread died without answering; don't stay "busy".
            self.fx_rx = None;
            self.fx_busy = false;
        }
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
        use magpie_core::io::Format;
        let cmd = Modifiers::COMMAND;
        let alt = Modifiers::ALT;
        let shift = Modifiers::SHIFT;
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        self.keys = NavKeys::default();

        // Zoom works everywhere, even over modals.
        if pressed(cmd, Key::Plus) || pressed(cmd, Key::Equals) || pressed(cmd | shift, Key::Equals) {
            let z = ctx.zoom_factor() + 0.1;
            self.set_ui_scale(ctx, z);
        }
        if pressed(cmd, Key::Minus) {
            let z = ctx.zoom_factor() - 0.1;
            self.set_ui_scale(ctx, z);
        }
        if pressed(cmd, Key::Num0) {
            self.set_ui_scale(ctx, 1.0);
        }
        if pressed(cmd, Key::K) {
            self.palette.toggle(ctx);
        }
        if self.modal.is_some() || self.palette.open {
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());

        // Before Ctrl+T (New transfer): egui lets Ctrl+T match Ctrl+Shift+T.
        let basic = self.basic();
        if pressed(cmd | shift, Key::T) {
            self.set_basic(ctx, !basic);
            return;
        }

        // Pages: Ctrl/Alt + 1–8, Alt + ↑/↓ and Ctrl + (Shift +) Tab to step.
        let mut pages = Page::nav(basic).to_vec();
        pages.push(Page::Settings);
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
        ];
        for (k, p) in nums.iter().zip(pages.iter().copied()) {
            if pressed(cmd, *k) || pressed(alt, *k) {
                self.go(ctx, p);
            }
        }
        let idx = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        if pressed(alt, Key::ArrowDown) || pressed(cmd, Key::Tab) || pressed(cmd, Key::PageDown) {
            self.go(ctx, pages[(idx + 1) % pages.len()]);
        }
        if pressed(alt, Key::ArrowUp) || pressed(cmd | shift, Key::Tab) || pressed(cmd, Key::PageUp) {
            self.go(ctx, pages[(idx + pages.len() - 1) % pages.len()]);
        }

        // Actions
        if pressed(cmd, Key::N) {
            let m = forms::TxnForm::new(&self.store, self.today);
            self.open_modal(ctx, Modal::Txn(m));
        }
        if pressed(cmd, Key::T) && !basic {
            let m = forms::TxnForm::transfer(&self.store, self.today);
            self.open_modal(ctx, Modal::Txn(m));
        }
        if pressed(cmd | shift, Key::A) {
            let m = forms::AccountForm::new(&self.store);
            self.open_modal(ctx, Modal::Account(m));
        }
        if pressed(cmd, Key::G) && !basic {
            let m = forms::GoalForm::new(&self.store, self.today);
            self.open_modal(ctx, Modal::Goal(m));
        }
        if pressed(cmd, Key::R) && !basic {
            let m = forms::RuleForm::new(&self.store, self.today);
            self.open_modal(ctx, Modal::Rule(m));
        }
        if pressed(cmd, Key::I) {
            self.dialogs.pick(
                ctx,
                crate::dialogs::Purpose::ImportCsv,
                "Import transactions",
                ("Bank statements", magpie_core::statement::EXTENSIONS),
            );
        }
        if pressed(cmd, Key::E) {
            views::settings::export_all(self, Format::Csv);
        }
        if pressed(cmd | shift, Key::Z) || pressed(cmd, Key::Y) {
            self.redo();
        } else if pressed(cmd, Key::Z) {
            self.undo();
        }
        if pressed(cmd, Key::F) {
            self.go(ctx, Page::Ledger);
            if basic {
                self.basic_ledger.focus_search = true;
            } else {
                self.ledger.focus_search = true;
            }
        }
        if pressed(cmd, Key::Comma) {
            self.go(ctx, Page::Settings);
        }
        if pressed(cmd, Key::B) || pressed(cmd, Key::Backslash) {
            self.collapsed = !self.collapsed;
        }
        if pressed(cmd | shift, Key::L) {
            let name = if self.t().dark { "Magpie Light" } else { "Magpie Dark" };
            self.set_theme(ctx, name);
        }
        if !typing
            && (ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1))
                || ctx.input_mut(|i| {
                    i.consume_key(shift, Key::Slash) || i.consume_key(Modifiers::NONE, Key::Questionmark)
                }))
        {
            self.open_modal(ctx, Modal::Help);
        }

        // Plain keys for in-page navigation (not while typing).
        if !typing {
            let none = Modifiers::NONE;
            self.keys = NavKeys {
                up: pressed(none, Key::ArrowUp) || pressed(none, Key::K),
                down: pressed(none, Key::ArrowDown) || pressed(none, Key::J),
                left: pressed(none, Key::ArrowLeft) || pressed(none, Key::H),
                right: pressed(none, Key::ArrowRight) || pressed(none, Key::L),
                enter: pressed(none, Key::Enter) || pressed(none, Key::Space),
                edit: pressed(none, Key::E),
                delete: pressed(none, Key::Delete),
                page_up: pressed(none, Key::PageUp),
                page_down: pressed(none, Key::PageDown),
                home: pressed(none, Key::Home),
                end: pressed(none, Key::End),
            };
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
                let basic = self.basic();
                // Each page folds in or out of the list when the mode
                // changes; `vis` is how much of its slot it occupies.
                let vis: Vec<f32> = Page::NAV
                    .iter()
                    .map(|p| {
                        motion::toggle(
                            &ctx,
                            Id::new(("nav-vis", *p as u8)),
                            !basic || p.in_basic(),
                            motion::EMPHASIS,
                        )
                    })
                    .collect();
                let slot = |v: f32| (item_h + 4.0) * v;
                let total: f32 = vis.iter().map(|v| slot(*v)).sum();
                let (list, _) = ui.allocate_exact_size(vec2(full.width(), total - 4.0), Sense::hover());
                if let Some(i) = Page::NAV.iter().position(|p| *p == self.page) {
                    let target = list.top() + vis[..i].iter().map(|v| slot(*v)).sum::<f32>();
                    let y = motion::tween(&ctx, Id::new("nav-indicator"), target, motion::STANDARD);
                    let r = Rect::from_min_size(pos2(full.left(), y), vec2(full.width(), item_h));
                    ui.painter().rect_filled(r, CornerRadius::same(10), t.accent_soft());
                    let bar = Rect::from_min_size(pos2(full.left() - 12.0, y + 10.0), vec2(3.0, item_h - 20.0));
                    ui.painter().rect_filled(bar, CornerRadius::same(2), t.accent);
                }
                let mut y = list.top();
                let mut n = 0;
                for (p, v) in Page::NAV.iter().zip(&vis) {
                    if *v <= 0.001 {
                        continue;
                    }
                    let h = slot(*v);
                    if *v >= 0.5 {
                        n += 1;
                    }
                    let rect = Rect::from_min_size(pos2(full.left(), y), vec2(full.width(), item_h));
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    c.set_clip_rect(Rect::from_min_size(rect.min, vec2(rect.width(), h)).intersect(ui.clip_rect()));
                    c.set_opacity(*v);
                    let label = NavLabel {
                        title: p.title_in(basic),
                        icon: p.icon_in(basic),
                        n,
                    };
                    if nav_item(&mut c, &t, *p, label, self.page == *p, k, item_h).clicked() && *v > 0.5 {
                        self.go(&ctx, *p);
                    }
                    y += h;
                }

                // Bottom section
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    // Collapse + light/dark toggles. Collapsed, they stack and
                    // centre on the icon column; expanded, they sit in a row
                    // whose first button lines up with the nav icons.
                    let collapsed_now = k > 0.5;
                    let full = ui.max_rect();
                    let btn = 30.0;
                    let strip_h = if collapsed_now { btn * 2.0 + 6.0 } else { btn };
                    let (strip, _) = ui.allocate_exact_size(vec2(full.width(), strip_h), Sense::hover());
                    let icon_cx = motion::lerp(strip.left() + 23.0, strip.center().x, k);
                    let dark = t.dark;
                    let toggle_glyph = if self.collapsed || narrow {
                        ph::CARET_CIRCLE_DOUBLE_RIGHT
                    } else {
                        ph::CARET_CIRCLE_DOUBLE_LEFT
                    };
                    let theme_glyph = if dark { ph::SUN } else { ph::MOON };
                    let (collapse_rect, theme_rect) = if collapsed_now {
                        (
                            Rect::from_center_size(pos2(icon_cx, strip.bottom() - btn / 2.0), vec2(btn, btn)),
                            Rect::from_center_size(pos2(icon_cx, strip.top() + btn / 2.0), vec2(btn, btn)),
                        )
                    } else {
                        (
                            Rect::from_center_size(pos2(icon_cx, strip.center().y), vec2(btn, btn)),
                            Rect::from_center_size(pos2(icon_cx + btn + 8.0, strip.center().y), vec2(btn, btn)),
                        )
                    };
                    let tip = if narrow {
                        "The window is narrow, so the sidebar stays compact"
                    } else if self.collapsed {
                        concat!("Expand sidebar (", shortcut!("B"), ")")
                    } else {
                        concat!("Collapse sidebar (", shortcut!("B"), ")")
                    };
                    let collapse = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(collapse_rect), |ui| {
                            widgets::icon_button(ui, &t, toggle_glyph, tip)
                        })
                        .inner;
                    if collapse.clicked() && !narrow {
                        self.collapsed = !self.collapsed;
                    }
                    let theme_btn = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(theme_rect), |ui| {
                            widgets::icon_button(
                                ui,
                                &t,
                                theme_glyph,
                                concat!("Toggle light / dark (", shortcut!("Shift L"), ")"),
                            )
                        })
                        .inner;
                    if theme_btn.clicked() {
                        let name = if dark { "Magpie Light" } else { "Magpie Dark" };
                        self.set_theme(&ctx, name);
                    }
                    ui.add_space(4.0);
                    let label = NavLabel {
                        title: "Settings",
                        icon: Page::Settings.icon(),
                        n: Page::nav(basic).len() + 1,
                    };
                    if nav_item(ui, &t, Page::Settings, label, self.page == Page::Settings, k, item_h).clicked() {
                        self.go(&ctx, Page::Settings);
                    }
                    // Just above Settings, where an update naturally lives.
                    if self.updater.prompt() {
                        self.update_prompt(ui, &t, k);
                    }
                    if k < 0.3 {
                        ui.add_space(8.0);
                        self.net_worth_card(ui, &t, 1.0 - k / 0.3);
                    }
                });
            });
    }

    /// The sidebar's update / restart prompt: a quiet row in the style of
    /// the nav items, with a small accent dot. Nothing happens without a
    /// click; the launch check only makes it appear.
    fn update_prompt(&mut self, ui: &mut Ui, t: &Theme, collapse: f32) {
        use crate::updater::Phase;
        let ctx = ui.ctx().clone();
        let phase = self.updater.phase.clone();
        let (label, tip) = match &phase {
            Phase::Available(r) => (
                "Update available".to_string(),
                format!("Magpie {} is out. Click to update.", r.version),
            ),
            Phase::Manual(r, why) => (
                "Update available".to_string(),
                format!("Magpie {} is out. {why}.", r.version),
            ),
            Phase::Installing(r) => (
                format!("Updating… {:.0}%", self.updater.progress * 100.0),
                format!("Installing Magpie {}", r.version),
            ),
            Phase::Ready(r) => (
                "Restart to update".to_string(),
                format!("Magpie {} is installed. Click to restart.", r.version),
            ),
            _ => return,
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        let resp = resp.on_hover_text(tip);
        let installing = matches!(phase, Phase::Installing(_));
        let hover = motion::toggle(
            &ctx,
            Id::new("update-row-h"),
            resp.hovered() && !installing,
            motion::MICRO,
        );
        let shown = motion::appear(&ctx, 0.0, 0.0, 0.4);
        let p = ui.painter();
        if hover > 0.0 {
            p.rect_filled(rect, CornerRadius::same(10), motion::with_alpha(t.hover_wash(), hover));
        }
        let fg = motion::with_alpha(motion::lerp_color(t.text3, t.text, hover), shown);
        let icon_x = motion::lerp(rect.left() + 14.0, rect.center().x - 8.0, collapse);
        let icon_c = pos2(icon_x + 8.0, rect.center().y);
        if installing {
            let angle = ui.input(|i| i.time) as f32 * widgets::SPIN_SPEED;
            widgets::paint_circle_arrows(p, icon_c, 6.0, angle, motion::with_alpha(t.accent, shown), 1.5);
            ctx.request_repaint();
        } else {
            let glyph = if matches!(phase, Phase::Ready(_)) {
                ph::ARROW_CLOCKWISE
            } else {
                ph::CLOUD_ARROW_DOWN
            };
            p.text(icon_c, Align2::CENTER_CENTER, glyph, theme::regular(16.0), fg);
            // The one bit of colour: a small dot on the icon.
            p.circle_filled(icon_c + vec2(7.0, -6.0), 3.0, motion::with_alpha(t.accent, shown));
        }
        if collapse < 0.6 {
            let a = (1.0 - collapse / 0.6) * shown;
            p.text(
                pos2(rect.left() + 44.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                theme::medium(12.5),
                motion::with_alpha(fg, a),
            );
        }
        if installing {
            let bar = Rect::from_min_size(
                pos2(rect.left() + 44.0, rect.bottom() - 4.0),
                vec2(rect.width() - 56.0, 2.0),
            );
            if collapse < 0.6 {
                p.rect_filled(bar, CornerRadius::same(1), t.border);
                let done = Rect::from_min_size(bar.min, vec2(bar.width() * self.updater.progress, 2.0));
                p.rect_filled(done, CornerRadius::same(1), t.accent);
            }
        }
        if resp.clicked() {
            self.update_action(&ctx);
        }
        if !installing {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }

    /// What clicking the update prompt does in its current state.
    pub fn update_action(&mut self, ctx: &egui::Context) {
        use crate::updater::Phase;
        match &self.updater.phase {
            Phase::Available(_) => self.updater.install(ctx),
            Phase::Manual(r, _) => open_url(&r.page.clone()),
            Phase::Ready(_) => self.updater.restart(ctx),
            _ => {}
        }
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
                    egui::RichText::new(if self.basic() { "Total balance" } else { "Net worth" })
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
                    egui::RichText::new(self.page.title_in(self.basic()))
                        .font(theme::display(32.0))
                        .color(t.text),
                );
                ui.label(
                    egui::RichText::new(long_date(self.today))
                        .font(theme::medium(15.5))
                        .color(t.text2),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary(ui, &t, Some(ph::PLUS), "Add")
                    .on_hover_text(concat!("New transaction (", shortcut!("N"), ")"))
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
                let basic = self.basic();
                match self.page {
                    Page::Ledger if !basic => views::ledger::show(self, &mut ui),
                    page => {
                        // The scroll area reaches the window edge so its bar sits
                        // in the margin, well clear of the content.
                        let content_w = inner.width();
                        let top = ui.cursor().top();
                        let scroll_rect =
                            Rect::from_min_max(pos2(inner.left(), top), pos2(full.right() - 6.0, full.bottom()));
                        let mut sui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(scroll_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        let keys = self.keys;
                        crate::widgets::scroll_area()
                            .id_salt(("page", page as u8, basic))
                            .auto_shrink([false, false])
                            .show(&mut sui, |ui| {
                                ui.set_max_width(content_w);
                                let step = ui.clip_rect().height() * 0.85;
                                let mut dy = 0.0;
                                if keys.page_down {
                                    dy -= step;
                                }
                                if keys.page_up {
                                    dy += step;
                                }
                                if keys.home {
                                    dy += 1e7;
                                }
                                if keys.end {
                                    dy -= 1e7;
                                }
                                // Pages without list selection scroll with the arrows.
                                if matches!(page, Page::Dashboard | Page::Reports | Page::Settings)
                                    || (basic && page == Page::Ledger)
                                {
                                    if keys.down {
                                        dy -= 80.0;
                                    }
                                    if keys.up {
                                        dy += 80.0;
                                    }
                                }
                                if dy != 0.0 {
                                    ui.scroll_with_delta_animation(
                                        vec2(0.0, dy),
                                        egui::style::ScrollAnimation::duration(0.18),
                                    );
                                }
                                match page {
                                    Page::Dashboard => views::dashboard::show(self, ui),
                                    Page::Budgets => views::budgets::show(self, ui),
                                    Page::Reports => views::reports::show(self, ui),
                                    Page::Accounts => views::accounts::show(self, ui),
                                    Page::Recurring => views::recurring::show(self, ui),
                                    Page::Goals => views::goals::show(self, ui),
                                    Page::Settings => views::settings::show(self, ui),
                                    Page::Ledger => views::basic_ledger::show(self, ui),
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
                "Drop a bank statement to import"
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
            if magpie_core::statement::EXTENSIONS.contains(&ext.as_str()) {
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
        // Frame pacing (vsync is off, see main.rs): while animating, don't
        // render faster than FRAME_CAP. Idle frames aren't affected.
        if let Some(last) = self.last_frame {
            let min = std::time::Duration::from_secs_f32(1.0 / FRAME_CAP);
            let since = last.elapsed();
            if since < min {
                std::thread::sleep(min - since);
            }
        }
        self.last_frame = Some(std::time::Instant::now());
        crate::diag::frame_begin();
        // Crash shield: a bug in one frame must never take the app (and the
        // user's unsaved typing) down. Log it, reset the view, keep going.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frame(ui)));
        crate::diag::frame_end();
        if result.is_err() {
            self.recover(ui.ctx());
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.updater.on_exit();
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

impl App {
    pub(crate) fn frame(&mut self, ui: &mut Ui) {
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
        self.poll_jobs(&ctx);
        match self.updater.poll(&ctx) {
            Some(Ok(msg)) => self.toasts.success(msg),
            Some(Err(msg)) => self.toasts.error(msg),
            None => {}
        }
        if self.onboarding.is_none() && self.tour.is_none() {
            let auto = self.store.settings().auto_update;
            self.updater.launch_check(&ctx, auto);
        }

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
        let t0 = std::time::Instant::now();
        self.sidebar(ui);
        self.content(ui);
        crate::debug_frame(self.page, t0.elapsed());
        forms::show(self, &ctx);
        crate::palette::show(self, &ctx);
        self.drop_overlay(&ctx);
        self.mode_fade(&ctx, &t);
        if let Some(at) = self.intro_at {
            // Onboarding just flew away: the app grows into place from
            // slightly behind as it fades in.
            let zoom = motion::appear(&ctx, at, 0.0, 0.65);
            motion::zoom_app(&ctx, 0.9 + 0.1 * zoom);
            let k = motion::appear(&ctx, at, 0.0, 0.45);
            if zoom >= 1.0 {
                self.intro_at = None;
            } else {
                // Above the panels, below modals and toasts.
                ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("intro")))
                    .rect_filled(ctx.content_rect(), 0.0, motion::with_alpha(t.bg, 1.0 - k));
            }
        }
        if let Some(a) = self.toasts.show(&ctx, &t)
            && a == toasts::Action::Undo
        {
            self.undo();
        }
    }
}

/// What a sidebar row shows, which depends on the mode.
struct NavLabel {
    title: &'static str,
    icon: &'static str,
    /// Its number for Ctrl/Alt + 1…8.
    n: usize,
}

fn nav_item(
    ui: &mut Ui,
    t: &Theme,
    page: Page,
    label: NavLabel,
    active: bool,
    collapse: f32,
    h: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    crate::marks::record(|| format!("nav:{}", label.title), rect);
    let hover = motion::toggle(
        ui.ctx(),
        Id::new(("nav-h", page as u8)),
        resp.hovered() && !active,
        motion::MICRO,
    );
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(10), motion::with_alpha(t.hover_wash(), hover));
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
        label.icon,
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
            label.title,
            font,
            motion::with_alpha(fg, a),
        );
    }
    let n = label.n;
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{}   ·   {} {n}  or  Alt {n}",
            label.title,
            if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" }
        ))
}

/// The Magpie mark: the brand-orange tile with a white bird. Always orange
/// (it's the app icon too), whatever the theme.
pub fn logo(p: &egui::Painter, r: Rect, _t: &Theme) {
    let top = Color32::from_rgb(0xE4, 0x81, 0x4F);
    let bottom = Color32::from_rgb(0xA8, 0x4D, 0x22);
    widgets::rounded_gradient(p, r, r.width() * 0.29, top, bottom);
    p.text(
        r.center() + vec2(0.0, 0.5),
        Align2::CENTER_CENTER,
        ph::BIRD,
        theme::regular(r.width() * 0.59),
        Color32::WHITE,
    );
}

fn search_pill(ui: &mut Ui, t: &Theme) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(260.0, 34.0), Sense::click());
    let hover = motion::toggle(ui.ctx(), resp.id, resp.hovered(), motion::MICRO);
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(10),
        motion::lerp_color(t.card, t.hovered(t.card), hover),
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
        shortcut!("K"),
        theme::medium(10.5),
        t.text2,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// True in tests: no file dialogs, no network, no launching other apps.
pub fn headless() -> bool {
    std::env::var_os("MAGPIE_HEADLESS").is_some()
}

/// Opens a file or folder with the system's default app.
pub fn open_external(path: &std::path::Path) {
    if headless() {
        return;
    }
    #[cfg(target_os = "macos")]
    let cmd = "open";
    // Explorer opens files with their default app and folders in a window,
    // and copes with spaces and unicode in paths (unlike `cmd /c start`).
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(not(any(target_os = "macos", windows)))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        crate::diag::crumb(format!("couldn't run {cmd}: {e}"));
    }
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    if headless() || url.is_empty() {
        return;
    }
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        crate::diag::crumb(format!("couldn't open {url}: {e}"));
    }
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
}
