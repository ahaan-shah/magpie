//! Magpie's widget kit, drawn by hand so every control shares the same
//! radii, colours and motion.

pub mod charts;

use crate::icons::ph;
use crate::motion;
use crate::theme::{self, Theme};
use egui::{
    Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, Vec2, vec2,
};
use jiff::civil::Date;
use magpie_core::money::{self, Cur, FmtOpts};

// ------------------------------------------------------------------ cards

pub fn card_frame(t: &Theme) -> Frame {
    Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(theme::PAD))
        .shadow(Shadow {
            offset: [0, 2],
            blur: if t.dark { 8 } else { 14 },
            spread: 0,
            color: if t.dark {
                Color32::from_black_alpha(40)
            } else {
                Color32::from_rgba_premultiplied(18, 22, 40, 10)
            },
        })
}

/// A card that fills exactly `rect`.
pub fn card_in<R>(ui: &mut Ui, t: &Theme, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = card_frame(t);
    let inner = rect - frame.total_margin();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    ui.painter().add(frame.paint(inner));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child)
}

/// A card that fills `rect` and scrolls its content when it doesn't fit.
/// The scrollbar sits in the card's right padding, clear of the content.
pub fn card_scroll<R>(
    ui: &mut Ui,
    t: &Theme,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    rect: Rect,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = card_frame(t);
    let inner = rect - frame.total_margin();
    ui.painter().add(frame.paint(inner));
    let scroll_rect = Rect::from_min_max(inner.min, egui::pos2(rect.right() - 5.0, inner.bottom()));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(scroll_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let content_w = inner.width();
    let out = scroll_area()
        .id_salt(Id::new(("card-scroll", id_salt)))
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.set_max_width(content_w);
            add(ui)
        });
    // A scrollable card owns the wheel while the pointer is over it: once it
    // hits its top or bottom, leftover scroll must not chain to the page.
    let scrollable = out.content_size.y > out.inner_rect.height() + 0.5;
    if scrollable && ui.rect_contains_pointer(rect) {
        ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
    }
    out.inner
}

/// Extra wheel speed on top of egui's line scroll speed, so touchpads (which
/// report pixels, not lines) also feel snappier.
pub const SCROLL_BOOST: f32 = 1.3;

/// A vertical scroll area with Magpie's scroll speed.
pub fn scroll_area() -> egui::ScrollArea {
    egui::ScrollArea::vertical().wheel_scroll_multiplier(vec2(1.0, SCROLL_BOOST))
}

/// Card title row: title on the left, optional right-hand content.
pub fn card_header(ui: &mut Ui, t: &Theme, title: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(theme::semibold(14.5)).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(6.0);
}

// ----------------------------------------------------------------- layout

/// Lays out `weights.len()` columns of fixed `height` across the available
/// width (stacking vertically when narrow) and calls `add` for each.
pub fn grid_row(ui: &mut Ui, height: f32, weights: &[f32], mut add: impl FnMut(usize, &mut Ui, Rect)) {
    let gap = theme::GAP;
    let width = ui.available_width();
    let total: f32 = weights.iter().sum();
    let min_col = 250.0;
    // Does every column get at least `min_col` in a single row?
    let usable = width - gap * (weights.len() as f32 - 1.0);
    let fits = weights.iter().all(|w| usable * w / total >= min_col) || weights.len() == 1;
    let per_row = if fits {
        weights.len()
    } else {
        (((width + gap) / (min_col + gap)).floor() as usize).clamp(1, weights.len())
    };
    let mut i = 0;
    while i < weights.len() {
        let chunk = &weights[i..(i + per_row).min(weights.len())];
        // When wrapping, give each column in the row an equal share.
        let ws: Vec<f32> = if fits { chunk.to_vec() } else { vec![1.0; chunk.len()] };
        let row_total: f32 = ws.iter().sum();
        let row_usable = width - gap * (per_row as f32 - 1.0);
        let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let mut x = row.left();
        for (k, w) in ws.iter().enumerate() {
            let cw = if fits {
                row_usable * w / row_total
            } else {
                row_usable / per_row as f32
            };
            add(
                i + k,
                ui,
                Rect::from_min_size(egui::pos2(x, row.top()), vec2(cw, height)),
            );
            x += cw + gap;
        }
        i += chunk.len();
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
}

/// Staggered entrance: fades and lifts `rect` in based on `shown_at`.
pub fn reveal(ui: &mut Ui, shown_at: f64, index: usize) -> (f32, f32) {
    let p = motion::appear(ui.ctx(), shown_at, index as f32 * 0.045, 0.38);
    (p, (1.0 - p) * 14.0)
}

pub fn with_reveal<R>(ui: &mut Ui, shown_at: f64, index: usize, rect: Rect, add: impl FnOnce(&mut Ui, Rect) -> R) -> R {
    let (alpha, dy) = reveal(ui, shown_at, index);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
    child.set_opacity(alpha);
    add(&mut child, rect.translate(vec2(0.0, dy)))
}

// ---------------------------------------------------------------- buttons

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, t: &Theme, kind: Kind, icon: Option<&str>, label: &str) -> Response {
    let font = theme::medium(13.5);
    let text = match icon {
        Some(i) if !label.is_empty() => format!("{i}  {label}"),
        Some(i) => i.to_string(),
        None => label.to_string(),
    };
    let fg = match kind {
        Kind::Primary => t.on_accent,
        Kind::Danger => t.neg,
        Kind::Secondary => t.text,
        Kind::Ghost => t.text2,
    };
    let galley = ui.painter().layout_no_wrap(text, font, fg);
    let pad = if label.is_empty() {
        vec2(9.0, 8.0)
    } else {
        vec2(14.0, 8.0)
    };
    let size = vec2(
        galley.size().x + pad.x * 2.0,
        34.0f32.max(galley.size().y + pad.y * 2.0),
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = motion::toggle(ui.ctx(), resp.id.with("h"), resp.hovered(), motion::MICRO);
    let press = motion::toggle(ui.ctx(), resp.id.with("p"), resp.is_pointer_button_down_on(), 0.08);
    let rect = rect.shrink(press * 1.0);
    let (bg, stroke) = match kind {
        Kind::Primary => (
            motion::lerp_color(
                t.accent,
                if t.dark { Color32::WHITE } else { Color32::BLACK },
                0.08 * hover,
            ),
            Stroke::NONE,
        ),
        Kind::Secondary => (
            motion::lerp_color(t.hover, t.tint(t.accent, 0.14), hover),
            Stroke::new(
                1.0,
                motion::lerp_color(t.border, motion::with_alpha(t.accent, 0.6), hover),
            ),
        ),
        Kind::Ghost => (motion::with_alpha(t.hover, hover), Stroke::NONE),
        Kind::Danger => (
            motion::lerp_color(t.tint(t.neg, 0.08), t.tint(t.neg, 0.18), hover),
            Stroke::new(1.0, motion::with_alpha(t.neg, 0.35)),
        ),
    };
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            bg,
            stroke,
            StrokeKind::Inside,
        );
        let fg = if kind == Kind::Ghost {
            motion::lerp_color(t.text2, t.text, hover)
        } else {
            fg
        };
        p.galley(rect.center() - galley.size() / 2.0, galley, fg);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn primary(ui: &mut Ui, t: &Theme, icon: Option<&str>, label: &str) -> Response {
    button(ui, t, Kind::Primary, icon, label)
}
pub fn secondary(ui: &mut Ui, t: &Theme, icon: Option<&str>, label: &str) -> Response {
    button(ui, t, Kind::Secondary, icon, label)
}
pub fn ghost(ui: &mut Ui, t: &Theme, icon: Option<&str>, label: &str) -> Response {
    button(ui, t, Kind::Ghost, icon, label)
}
pub fn danger(ui: &mut Ui, t: &Theme, icon: Option<&str>, label: &str) -> Response {
    button(ui, t, Kind::Danger, icon, label)
}

/// Small round icon button.
pub fn icon_button(ui: &mut Ui, t: &Theme, glyph: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let hover = motion::toggle(ui.ctx(), resp.id, resp.hovered(), motion::MICRO);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(8), motion::with_alpha(t.hover, hover * 1.0));
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            theme::regular(16.0),
            motion::lerp_color(t.text2, t.text, hover),
        );
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

// ------------------------------------------------------------------ chips

/// A rounded pill with a coloured dot or icon.
pub fn chip(ui: &mut Ui, t: &Theme, icon: Option<&str>, text: &str, color: Color32) -> Response {
    let font = theme::medium(12.0);
    let label = match icon {
        Some(i) => format!("{i} {text}"),
        None => text.to_string(),
    };
    let galley = ui.painter().layout_no_wrap(label, font, color);
    let dot = if icon.is_none() { 12.0 } else { 0.0 };
    let size = vec2(galley.size().x + 18.0 + dot, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(
            rect,
            CornerRadius::same(12),
            t.tint(color, if t.dark { 0.16 } else { 0.12 }),
        );
        let mut x = rect.left() + 9.0;
        if icon.is_none() {
            p.circle_filled(egui::pos2(x + 3.0, rect.center().y), 3.5, color);
            x += dot;
        }
        p.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    resp
}

/// Category colour chip: icon + name, tinted with the category colour.
pub fn category_chip(ui: &mut Ui, t: &Theme, name: &str, icon: &str, color: Color32) -> Response {
    let c = readable(t, color);
    chip(ui, t, Some(icon), name, c)
}

/// Nudges a colour so it reads well as text on the current background.
pub fn readable(t: &Theme, c: Color32) -> Color32 {
    if t.dark {
        motion::lerp_color(c, Color32::WHITE, 0.12)
    } else {
        motion::lerp_color(c, Color32::BLACK, 0.22)
    }
}

pub fn cat_color(v: u32) -> Color32 {
    theme::rgb(v)
}

/// A rounded square with an icon, used as a leading avatar in lists.
pub fn icon_badge(ui: &mut Ui, t: &Theme, glyph: &str, color: Color32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon_badge(ui.painter(), t, rect, glyph, color);
    resp
}

pub fn paint_icon_badge(p: &egui::Painter, t: &Theme, rect: Rect, glyph: &str, color: Color32) {
    let size = rect.width();
    p.rect_filled(
        rect,
        CornerRadius::same((size * 0.3) as u8),
        t.tint(color, if t.dark { 0.2 } else { 0.14 }),
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        theme::regular(size * 0.5),
        readable(t, color),
    );
}

// ---------------------------------------------------------------- toggles

pub fn toggle(ui: &mut Ui, t: &Theme, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(38.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let k = motion::toggle(ui.ctx(), resp.id, *on, motion::STANDARD);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = motion::lerp_color(t.border, t.accent, k);
        p.rect_filled(rect, CornerRadius::same(11), bg);
        let x = motion::lerp(rect.left() + 11.0, rect.right() - 11.0, k);
        p.circle_filled(egui::pos2(x, rect.center().y + 0.5), 8.5, Color32::from_black_alpha(30));
        p.circle_filled(
            egui::pos2(x, rect.center().y),
            8.0,
            if *on { t.on_accent } else { t.card },
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Toggle with a label to its right.
pub fn toggle_row(ui: &mut Ui, t: &Theme, on: &mut bool, label: &str) -> Response {
    ui.horizontal(|ui| {
        let r = toggle(ui, t, on);
        ui.label(egui::RichText::new(label).color(t.text));
        r
    })
    .inner
}

/// Segmented control with a sliding highlight.
pub fn segmented(ui: &mut Ui, t: &Theme, id: Id, selected: &mut usize, options: &[&str]) -> bool {
    let font = theme::medium(12.5);
    let galleys: Vec<_> = options
        .iter()
        .map(|o| ui.painter().layout_no_wrap(o.to_string(), font.clone(), t.text))
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 22.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, CornerRadius::same(10), t.hover);
    let mut changed = false;
    let mut x = rect.left() + 3.0;
    let mut rects = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), vec2(*w, rect.height() - 6.0));
        let resp = ui
            .interact(r, id.with(i), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
        rects.push(r);
        x += w;
    }
    let sel = *selected;
    if let Some(target) = rects.get(sel) {
        let l = motion::tween(ui.ctx(), id.with("l"), target.left(), motion::STANDARD);
        let r = motion::tween(ui.ctx(), id.with("r"), target.right(), motion::STANDARD);
        let hl = Rect::from_x_y_ranges(l..=r, target.y_range());
        p.rect_filled(
            hl.translate(vec2(0.0, 1.0)),
            CornerRadius::same(8),
            Color32::from_black_alpha(if t.dark { 40 } else { 12 }),
        );
        p.rect_filled(hl, CornerRadius::same(8), t.card);
    }
    for (i, (g, r)) in galleys.into_iter().zip(&rects).enumerate() {
        let c = if i == sel { t.text } else { t.text2 };
        p.galley(r.center() - g.size() / 2.0, g, c);
    }
    changed
}

// ----------------------------------------------------------------- inputs

pub fn field_frame(t: &Theme, focused: bool) -> Frame {
    Frame::new()
        .fill(t.bg)
        .stroke(Stroke::new(1.0, if focused { t.accent } else { t.border }))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::symmetric(10, 7))
}

/// A styled single-line text input.
pub fn text_field(ui: &mut Ui, t: &Theme, id: Id, value: &mut String, hint: &str, width: f32) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let k = motion::toggle(ui.ctx(), id.with("f"), focused, motion::MICRO);
    let stroke = motion::lerp_color(t.border, t.accent, k);
    Frame::new()
        .fill(t.bg)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .id(id)
                    .hint_text(egui::RichText::new(hint).color(t.text3))
                    .frame(Frame::NONE)
                    .desired_width(width - 22.0)
                    .text_color(t.text),
            )
        })
        .inner
}

/// Multi-line note field.
pub fn text_area(ui: &mut Ui, t: &Theme, id: Id, value: &mut String, hint: &str, width: f32) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    field_frame(t, focused)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(value)
                    .id(id)
                    .hint_text(egui::RichText::new(hint).color(t.text3))
                    .frame(Frame::NONE)
                    .desired_rows(2)
                    .desired_width(width - 22.0)
                    .text_color(t.text),
            )
        })
        .inner
}

/// Label above a form control.
pub fn field_label(ui: &mut Ui, t: &Theme, text: &str) {
    ui.label(egui::RichText::new(text).font(theme::medium(12.0)).color(t.text2));
}

/// Styled combo box.
pub fn dropdown<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: impl Into<egui::WidgetText>,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(width)
        .height(320.0)
        .show_ui(ui, add)
}

// ------------------------------------------------------------ date picker

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub fn fmt_date(d: Date) -> String {
    format!("{} {}, {}", magpie_core::Month::of(d).short(), d.day(), d.year())
}

/// A button showing the date that opens a calendar popup. Returns true when
/// the date changed.
pub fn date_field(ui: &mut Ui, t: &Theme, id: Id, date: &mut Date, width: f32) -> bool {
    let text = format!("{}   {}", ph::CALENDAR_BLANK, fmt_date(*date));
    let galley = ui.painter().layout_no_wrap(text, theme::regular(13.5), t.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(width.max(galley.size().x + 24.0), 34.0), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    let hover = motion::toggle(ui.ctx(), id.with("h"), resp.hovered() || open, motion::MICRO);
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        t.bg,
        Stroke::new(1.0, motion::lerp_color(t.border, t.accent, hover)),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    if resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), id);
    }
    let mut changed = false;
    egui::Popup::new(id, ui.ctx().clone(), rect, ui.layer_id())
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(popup_frame(t))
        .show(|ui| {
            changed = calendar(ui, t, id.with("cal"), date);
            if changed {
                egui::Popup::close_id(ui.ctx(), id);
            }
        });
    changed
}

pub fn popup_frame(t: &Theme) -> Frame {
    Frame::new()
        .fill(t.elevated)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(10))
        .shadow(Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: t.shadow(),
        })
}

/// Month grid calendar. Returns true when a day was picked.
pub fn calendar(ui: &mut Ui, t: &Theme, id: Id, date: &mut Date) -> bool {
    let mut view: magpie_core::Month = ui.data(|d| d.get_temp(id)).unwrap_or(magpie_core::Month::of(*date));
    let mut picked = false;
    ui.set_width(7.0 * 34.0);
    ui.horizontal(|ui| {
        if icon_button(ui, t, ph::CARET_LEFT, "").clicked() {
            view = view.prev();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, t, ph::CARET_RIGHT, "").clicked() {
                view = view.next();
            }
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(view.label())
                        .font(theme::semibold(13.5))
                        .color(t.text),
                );
            });
        });
    });
    let cell = 34.0;
    let (grid, _) = ui.allocate_exact_size(vec2(cell * 7.0, cell * 7.0), Sense::hover());
    let p = ui.painter().clone();
    for (i, w) in WEEKDAYS.iter().enumerate() {
        let c = egui::pos2(grid.left() + cell * (i as f32 + 0.5), grid.top() + cell * 0.5);
        p.text(c, Align2::CENTER_CENTER, *w, theme::medium(11.0), t.text3);
    }
    let first = view.first();
    let offset = first.weekday().to_monday_zero_offset() as usize;
    let today = magpie_core::today();
    for day in 1..=view.days() {
        let idx = offset + day as usize - 1;
        let (row, col) = (idx / 7 + 1, idx % 7);
        let r = Rect::from_min_size(
            egui::pos2(grid.left() + col as f32 * cell, grid.top() + row as f32 * cell),
            Vec2::splat(cell),
        )
        .shrink(2.0);
        let d = Date::new(view.year, view.month, day as i8).unwrap_or(first);
        let resp = ui.interact(r, id.with(("d", day)), Sense::click());
        let sel = d == *date;
        let bg = if sel {
            t.accent
        } else if resp.hovered() {
            t.hover
        } else {
            Color32::TRANSPARENT
        };
        p.rect_filled(r, CornerRadius::same(8), bg);
        if d == today && !sel {
            p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, t.accent), StrokeKind::Inside);
        }
        let fg = if sel {
            t.on_accent
        } else if d > today {
            t.text2
        } else {
            t.text
        };
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            day.to_string(),
            theme::regular(12.5),
            fg,
        );
        if resp.clicked() {
            *date = d;
            picked = true;
        }
    }
    ui.horizontal(|ui| {
        if ghost(ui, t, None, "Today").clicked() {
            *date = today;
            picked = true;
        }
        if ghost(ui, t, None, "Yesterday").clicked() {
            *date = today.yesterday().unwrap_or(today);
            picked = true;
        }
    });
    ui.data_mut(|d| d.insert_temp(id, view));
    picked
}

// ---------------------------------------------------------------- money

pub fn fmt_money(v: i64, cur: Cur) -> String {
    money::fmt(v, cur)
}

pub fn fmt_signed(v: i64, cur: Cur) -> String {
    money::format(
        v,
        cur,
        FmtOpts {
            plus: true,
            ..Default::default()
        },
    )
}

pub fn fmt_whole(v: i64, cur: Cur) -> String {
    money::format(
        v,
        cur,
        FmtOpts {
            trim_zero_cents: true,
            ..Default::default()
        },
    )
}

pub fn fmt_compact(v: i64, cur: Cur) -> String {
    money::format(
        v,
        cur,
        FmtOpts {
            compact: true,
            ..Default::default()
        },
    )
}

/// A large amount that counts up/down smoothly to its new value.
pub fn animated_amount(ui: &mut Ui, id: Id, value: i64, cur: Cur, font: egui::FontId, color: Color32) -> Response {
    let scale = cur.scale() as f32;
    let shown = motion::tween(ui.ctx(), id, value as f32 / scale, motion::EMPHASIS * 1.6);
    let shown = (shown as f64 * scale as f64).round() as i64;
    let v = if (shown - value).abs() as f32 <= scale * 0.01 {
        value
    } else {
        shown
    };
    ui.label(egui::RichText::new(fmt_money(v, cur)).font(font).color(color))
}

// ----------------------------------------------------------- misc pieces

pub fn empty_state(ui: &mut Ui, t: &Theme, glyph: &str, title: &str, body: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(egui::RichText::new(glyph).font(theme::regular(38.0)).color(t.text3));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).font(theme::semibold(15.0)).color(t.text));
        ui.label(egui::RichText::new(body).color(t.text2));
        ui.add_space(12.0);
    });
}

/// Progress bar with an optional "pace" tick showing where spending should
/// be today. Fill animates.
pub fn progress(ui: &mut Ui, t: &Theme, id: Id, frac: f32, pace: Option<f32>, color: Color32, height: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    paint_progress(ui, t, id, rect, frac, pace, color);
    resp
}

pub fn paint_progress(ui: &Ui, t: &Theme, id: Id, rect: Rect, frac: f32, pace: Option<f32>, color: Color32) {
    let f = motion::tween_from(ui.ctx(), id, 0.0, frac.clamp(0.0, 1.0), motion::CHART);
    let p = ui.painter();
    let r = CornerRadius::same((rect.height() / 2.0) as u8);
    p.rect_filled(rect, r, t.hover);
    if f > 0.001 {
        let w = (rect.width() * f).max(rect.height());
        p.rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), r, color);
    }
    if let Some(pace) = pace {
        let x = rect.left() + rect.width() * pace.clamp(0.0, 1.0);
        p.line_segment(
            [egui::pos2(x, rect.top() - 3.0), egui::pos2(x, rect.bottom() + 3.0)],
            Stroke::new(2.0, t.text2),
        );
    }
}

/// Colour for a budget usage fraction (compared to pace).
pub fn usage_color(t: &Theme, used: f32, pace: f32) -> Color32 {
    if used > 1.0 {
        t.neg
    } else if used > pace.max(0.05) * 1.1 && used > 0.75 {
        t.warn
    } else {
        t.pos
    }
}

pub fn subtle(t: &Theme, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(t.text2)
}

pub fn faint(t: &Theme, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(theme::regular(12.0)).color(t.text3)
}

pub fn colored_dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// Relative day label: "Today", "Yesterday", "Tomorrow", "in 3 days", "Mon, Sep 28".
pub fn day_label(d: Date, today: Date) -> String {
    let diff = (d - today).get_days();
    match diff {
        0 => "Today".into(),
        -1 => "Yesterday".into(),
        1 => "Tomorrow".into(),
        _ => {
            let wd = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d.weekday().to_monday_zero_offset() as usize];
            if d.year() == today.year() {
                format!("{wd}, {} {}", magpie_core::Month::of(d).short(), d.day())
            } else {
                format!("{wd}, {} {}, {}", magpie_core::Month::of(d).short(), d.day(), d.year())
            }
        }
    }
}

pub fn in_days(d: Date, today: Date) -> String {
    let n = (d - today).get_days();
    match n {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        n if n < 0 => format!("{} days ago", -n),
        n => format!("in {n} days"),
    }
}

/// Fills a rounded rectangle with a vertical gradient. Built as a fan mesh
/// over the rounded outline, then stroked with a thin anti-aliased edge.
pub fn rounded_gradient(p: &egui::Painter, r: Rect, radius: f32, top: Color32, bottom: Color32) {
    use egui::epaint::{Mesh, PathShape, PathStroke};
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0);
    let corners = [
        (egui::pos2(r.right() - rad, r.top() + rad), -std::f32::consts::FRAC_PI_2),
        (egui::pos2(r.right() - rad, r.bottom() - rad), 0.0),
        (
            egui::pos2(r.left() + rad, r.bottom() - rad),
            std::f32::consts::FRAC_PI_2,
        ),
        (egui::pos2(r.left() + rad, r.top() + rad), std::f32::consts::PI),
    ];
    let mut outline = Vec::with_capacity(40);
    for (c, a0) in corners {
        for i in 0..=8 {
            let a = a0 + std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
            outline.push(c + vec2(a.cos(), a.sin()) * rad);
        }
    }
    let color_at = |y: f32| motion::lerp_color(top, bottom, ((y - r.top()) / r.height()).clamp(0.0, 1.0));
    let mut mesh = Mesh::default();
    mesh.colored_vertex(r.center(), color_at(r.center().y));
    for pt in &outline {
        mesh.colored_vertex(*pt, color_at(pt.y));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(egui::Shape::mesh(mesh));
    let mid = motion::lerp_color(top, bottom, 0.5);
    p.add(PathShape::closed_line(outline, PathStroke::new(1.0, mid)));
}

/// A soft radial glow: `color` at the centre fading to transparent at `radius`.
pub fn radial_glow(p: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    use egui::epaint::Mesh;
    let rings = 24;
    let segs = 64;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for r in 1..=rings {
        let k = r as f32 / rings as f32;
        // Smooth falloff so there's no visible edge.
        let a = (1.0 - k).powf(2.2);
        for s in 0..segs {
            let ang = s as f32 / segs as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                center + vec2(ang.cos(), ang.sin()) * radius * k,
                color.gamma_multiply(a),
            );
        }
    }
    for s in 0..segs as u32 {
        let n = (s + 1) % segs as u32;
        mesh.add_triangle(0, 1 + s, 1 + n);
    }
    for r in 0..(rings - 1) as u32 {
        let a0 = 1 + r * segs as u32;
        let b0 = a0 + segs as u32;
        for s in 0..segs as u32 {
            let n = (s + 1) % segs as u32;
            mesh.add_triangle(a0 + s, b0 + s, b0 + n);
            mesh.add_triangle(a0 + s, b0 + n, a0 + n);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}
