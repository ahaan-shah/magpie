//! Design tokens, the built-in themes, fonts, and the animated crossfade
//! between themes.

use crate::motion;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, Style, Visuals,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub hover: Color32,
    pub elevated: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub pos: Color32,
    pub neg: Color32,
    pub warn: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const fn rgb(v: u32) -> Color32 {
    hex(v)
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Midnight",
        dark: true,
        bg: hex(0x0E1016),
        sidebar: hex(0x0A0C11),
        card: hex(0x151821),
        hover: hex(0x1B1F2A),
        elevated: hex(0x1C202B),
        border: hex(0x242938),
        text: hex(0xE9EBF1),
        text2: hex(0xA0A6B6),
        text3: hex(0x686F82),
        accent: hex(0x8AA4FF),
        on_accent: hex(0x0B0E1A),
        pos: hex(0x3DD68C),
        neg: hex(0xFF6B81),
        warn: hex(0xFFB547),
    },
    Theme {
        name: "Daylight",
        dark: false,
        bg: hex(0xF5F6FA),
        sidebar: hex(0xECEEF5),
        card: hex(0xFFFFFF),
        hover: hex(0xF4F6FB),
        elevated: hex(0xFFFFFF),
        border: hex(0xE3E6EE),
        text: hex(0x141722),
        text2: hex(0x5A6174),
        text3: hex(0x949AAB),
        accent: hex(0x4C66EE),
        on_accent: hex(0xFFFFFF),
        pos: hex(0x12A367),
        neg: hex(0xE0434C),
        warn: hex(0xD08600),
    },
    Theme {
        name: "Tokyo Night",
        dark: true,
        bg: hex(0x1A1B26),
        sidebar: hex(0x16161E),
        card: hex(0x1F2233),
        hover: hex(0x252A3E),
        elevated: hex(0x262B40),
        border: hex(0x2C3148),
        text: hex(0xC8D1F7),
        text2: hex(0x9AA3CC),
        text3: hex(0x5E6690),
        accent: hex(0x7AA2F7),
        on_accent: hex(0x11131C),
        pos: hex(0x9ECE6A),
        neg: hex(0xF7768E),
        warn: hex(0xE0AF68),
    },
    Theme {
        name: "Mocha",
        dark: true,
        bg: hex(0x1E1E2E),
        sidebar: hex(0x181825),
        card: hex(0x252536),
        hover: hex(0x2D2D41),
        elevated: hex(0x313244),
        border: hex(0x35364A),
        text: hex(0xCDD6F4),
        text2: hex(0xA6ADC8),
        text3: hex(0x6C7086),
        accent: hex(0xCBA6F7),
        on_accent: hex(0x1E1E2E),
        pos: hex(0xA6E3A1),
        neg: hex(0xF38BA8),
        warn: hex(0xF9E2AF),
    },
    Theme {
        name: "Nord",
        dark: true,
        bg: hex(0x2E3440),
        sidebar: hex(0x2A2F3A),
        card: hex(0x353C4A),
        hover: hex(0x3C4353),
        elevated: hex(0x3E4555),
        border: hex(0x444C5C),
        text: hex(0xECEFF4),
        text2: hex(0xC2CAD8),
        text3: hex(0x8690A4),
        accent: hex(0x88C0D0),
        on_accent: hex(0x242933),
        pos: hex(0xA3BE8C),
        neg: hex(0xD08770),
        warn: hex(0xEBCB8B),
    },
    Theme {
        name: "Rosé Pine",
        dark: true,
        bg: hex(0x191724),
        sidebar: hex(0x15131F),
        card: hex(0x1F1D2E),
        hover: hex(0x26233A),
        elevated: hex(0x26233A),
        border: hex(0x2C293F),
        text: hex(0xE0DEF4),
        text2: hex(0xA7A3C2),
        text3: hex(0x6E6A86),
        accent: hex(0xEBBCBA),
        on_accent: hex(0x191724),
        pos: hex(0x9CCFD8),
        neg: hex(0xEB6F92),
        warn: hex(0xF6C177),
    },
    Theme {
        name: "Gruvbox",
        dark: true,
        bg: hex(0x1D2021),
        sidebar: hex(0x191B1C),
        card: hex(0x282828),
        hover: hex(0x302E2C),
        elevated: hex(0x32302F),
        border: hex(0x3C3836),
        text: hex(0xEBDBB2),
        text2: hex(0xBDAE93),
        text3: hex(0x7C6F64),
        accent: hex(0xFABD2F),
        on_accent: hex(0x1D2021),
        pos: hex(0xB8BB26),
        neg: hex(0xFB4934),
        warn: hex(0xFE8019),
    },
    Theme {
        name: "Paper",
        dark: false,
        bg: hex(0xF8F5EF),
        sidebar: hex(0xF0EBE1),
        card: hex(0xFFFEFB),
        hover: hex(0xF7F3EC),
        elevated: hex(0xFFFEFB),
        border: hex(0xE6DFD2),
        text: hex(0x2A2521),
        text2: hex(0x6B6158),
        text3: hex(0xA2988C),
        accent: hex(0xD2602A),
        on_accent: hex(0xFFFFFF),
        pos: hex(0x2E8B57),
        neg: hex(0xC4423A),
        warn: hex(0xC0850F),
    },
];

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        THEMES.iter().copied().find(|t| t.name == name).unwrap_or(THEMES[0])
    }

    pub fn lerp(&self, other: &Theme, t: f32) -> Theme {
        let l = |a: Color32, b: Color32| motion::lerp_color(a, b, t);
        Theme {
            name: other.name,
            dark: if t < 0.5 { self.dark } else { other.dark },
            bg: l(self.bg, other.bg),
            sidebar: l(self.sidebar, other.sidebar),
            card: l(self.card, other.card),
            hover: l(self.hover, other.hover),
            elevated: l(self.elevated, other.elevated),
            border: l(self.border, other.border),
            text: l(self.text, other.text),
            text2: l(self.text2, other.text2),
            text3: l(self.text3, other.text3),
            accent: l(self.accent, other.accent),
            on_accent: l(self.on_accent, other.on_accent),
            pos: l(self.pos, other.pos),
            neg: l(self.neg, other.neg),
            warn: l(self.warn, other.warn),
        }
    }

    /// A translucent tint of `c` over the card colour.
    pub fn tint(&self, c: Color32, amount: f32) -> Color32 {
        motion::lerp_color(self.card, c, amount)
    }

    pub fn accent_soft(&self) -> Color32 {
        self.tint(self.accent, if self.dark { 0.16 } else { 0.12 })
    }

    pub fn shadow(&self) -> Color32 {
        if self.dark {
            Color32::from_black_alpha(90)
        } else {
            Color32::from_rgba_premultiplied(20, 24, 40, 22)
        }
    }
}

// ----------------------------------------------------------------- spacing

pub const RADIUS: u8 = 14;
pub const RADIUS_SM: u8 = 9;
pub const GAP: f32 = 16.0;
pub const PAD: i8 = 20;

// ------------------------------------------------------------------- fonts

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    };
    add(&mut fonts, "inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(
        &mut fonts,
        "inter-medium",
        include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    );
    add(
        &mut fonts,
        "inter-semibold",
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    );
    add(
        &mut fonts,
        "inter-display",
        include_bytes!("../assets/fonts/InterDisplay-SemiBold.ttf"),
    );

    let fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let family = |main: &str| {
        let mut v = vec![main.to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts
        .families
        .insert(FontFamily::Name("medium".into()), family("inter-medium"));
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), family("inter-semibold"));
    fonts
        .families
        .insert(FontFamily::Name("display".into()), family("inter-display"));

    // Phosphor icons as a fallback in every family so icons mix into text.
    let phosphor = egui_phosphor::Variant::Regular.font_bytes();
    add(&mut fonts, "phosphor", phosphor);
    for fam in ["medium", "semibold", "display"] {
        if let Some(v) = fonts.families.get_mut(&FontFamily::Name(fam.into())) {
            v.insert(1, "phosphor".into());
        }
    }
    if let Some(v) = fonts.families.get_mut(&FontFamily::Proportional) {
        v.insert(1, "phosphor".into());
    }
    ctx.set_fonts(fonts);
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}

// ------------------------------------------------------------------- apply

pub fn apply(ctx: &egui::Context, t: &Theme) {
    ctx.all_styles_mut(|s| style(s, t));
}

fn style(s: &mut Style, t: &Theme) {
    use egui::TextStyle::*;
    s.text_styles = [
        (Small, regular(11.5)),
        (Body, regular(13.5)),
        (Button, medium(13.5)),
        (Heading, semibold(22.0)),
        (Monospace, FontId::monospace(13.0)),
    ]
    .into();
    s.spacing.item_spacing = egui::vec2(8.0, 8.0);
    s.spacing.button_padding = egui::vec2(12.0, 6.0);
    s.spacing.interact_size = egui::vec2(24.0, 32.0);
    s.spacing.menu_margin = Margin::same(6);
    s.spacing.window_margin = Margin::same(PAD);
    s.spacing.scroll = egui::style::ScrollStyle::floating();
    s.spacing.scroll.floating_width = 6.0;
    s.spacing.scroll.bar_width = 6.0;
    s.animation_time = 0.18;
    s.interaction.selectable_labels = false;
    s.visuals = visuals(t);
}

fn visuals(t: &Theme) -> Visuals {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(RADIUS_SM);
    v.override_text_color = Some(t.text);
    v.weak_text_color = Some(t.text3);
    v.hyperlink_color = t.accent;
    v.faint_bg_color = t.hover;
    v.extreme_bg_color = t.bg;
    v.text_edit_bg_color = Some(t.bg);
    v.code_bg_color = t.hover;
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.neg;
    v.window_fill = t.elevated;
    v.panel_fill = t.bg;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.menu_corner_radius = CornerRadius::same(12);
    let shadow = Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: t.shadow(),
    };
    v.window_shadow = shadow;
    v.popup_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: t.shadow(),
    };
    v.selection.bg_fill = motion::with_alpha(t.accent, 0.35);
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.text_cursor.stroke = Stroke::new(2.0, t.accent);
    v.button_frame = true;
    v.striped = false;
    v.indent_has_left_vline = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.card;
    w.noninteractive.weak_bg_fill = t.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text2);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = t.hover;
    w.inactive.weak_bg_fill = t.hover;
    w.inactive.bg_stroke = Stroke::new(1.0, t.border);
    w.inactive.fg_stroke = Stroke::new(1.0, t.text);
    w.inactive.corner_radius = r;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = t.tint(t.accent, 0.10).lerp_to_gamma(t.hover, 0.5);
    w.hovered.weak_bg_fill = w.hovered.bg_fill;
    w.hovered.bg_stroke = Stroke::new(1.0, motion::with_alpha(t.accent, 0.5));
    w.hovered.fg_stroke = Stroke::new(1.5, t.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = t.accent_soft();
    w.active.weak_bg_fill = t.accent_soft();
    w.active.bg_stroke = Stroke::new(1.0, t.accent);
    w.active.fg_stroke = Stroke::new(1.5, t.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.active;
    v
}

/// Owns the current theme and animates switches between themes.
pub struct ThemeState {
    pub current: Theme,
    from: Theme,
    to: Theme,
    started: f64,
    animating: bool,
}

const FADE: f64 = 0.35;

impl ThemeState {
    pub fn new(ctx: &egui::Context, name: &str) -> ThemeState {
        let t = Theme::by_name(name);
        ctx.set_theme(if t.dark { egui::Theme::Dark } else { egui::Theme::Light });
        apply(ctx, &t);
        ThemeState {
            current: t,
            from: t,
            to: t,
            started: 0.0,
            animating: false,
        }
    }

    pub fn switch(&mut self, ctx: &egui::Context, name: &str) {
        let target = Theme::by_name(name);
        if target.name == self.to.name && !self.animating {
            return;
        }
        self.from = self.current;
        self.to = target;
        self.started = ctx.input(|i| i.time);
        self.animating = true;
        ctx.request_repaint();
    }

    pub fn target_name(&self) -> &'static str {
        self.to.name
    }

    /// Call once per frame before drawing.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.animating {
            return;
        }
        let now = ctx.input(|i| i.time);
        let p = ((now - self.started) / FADE).clamp(0.0, 1.0) as f32;
        let e = motion::ease_in_out(p);
        self.current = self.from.lerp(&self.to, e);
        if p >= 1.0 {
            self.current = self.to;
            self.animating = false;
            ctx.set_theme(if self.to.dark {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
        } else {
            ctx.request_repaint();
        }
        apply(ctx, &self.current);
    }
}
