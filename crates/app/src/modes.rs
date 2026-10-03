//! Basic and Advanced: the words that describe each mode, and a miniature
//! drawing of the app in each, so people can see what they're choosing
//! (onboarding and Settings).

use crate::motion;
use crate::theme::{self, Theme};
use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, pos2, vec2};
use std::f32::consts::{PI, TAU};

pub struct Mode {
    pub name: &'static str,
    pub tagline: &'static str,
    pub detail: &'static str,
    /// What it has beyond Basic, for Settings (empty for Basic itself).
    pub more: &'static str,
}

pub const BASIC: Mode = Mode {
    name: "Basic",
    tagline: "Everyday money, made simple.",
    detail: "Home, transactions, budgets and accounts. Just what you need, nothing you don't.",
    more: "",
};

pub const ADVANCED: Mode = Mode {
    name: "Advanced",
    tagline: "Every insight, total control.",
    detail: "Adds reports, recurring bills, savings goals, transfers, tags and other currencies.",
    more: "Adds reports, recurring bills, goals, transfers, tags and other currencies.",
};

pub fn mode(basic: bool) -> &'static Mode {
    if basic { &BASIC } else { &ADVANCED }
}

/// Category colours for the little charts.
const SWATCH: [u32; 5] = [0x5B8DEF, 0xF2994A, 0x27AE60, 0xE56BAF, 0x9B51E0];

fn rgb(v: u32) -> Color32 {
    theme::rgb(v)
}

/// Draws a tiny Magpie window. `advanced` runs from 0 (Basic) to 1
/// (Advanced); in between, the extra pages fold in and the two layouts
/// crossfade. `round` scales the window's corner radius (onboarding takes it
/// to 0 as the preview grows into the real, full-window app). `inside` fades
/// everything but the window and its sidebar.
pub fn paint_preview(p: &Painter, t: &Theme, rect: Rect, advanced: f32, round: f32, inside: f32) {
    let k = advanced.clamp(0.0, 1.0);
    let s = rect.width() / 420.0; // everything is drawn at 420 wide, then scaled
    let rad = |v: f32| CornerRadius::same((v * s).round().clamp(1.0, 255.0) as u8);
    let corner = |v: f32| (v * s.min(1.5) * round).round().clamp(0.0, 255.0) as u8;
    p.rect(
        rect,
        CornerRadius::same(corner(14.0)),
        t.bg,
        Stroke::new(1.0, t.border),
        egui::StrokeKind::Inside,
    );

    // Sidebar
    // Never wider than the real sidebar, so a full-window preview lines up
    // with the app it turns into.
    let side = Rect::from_min_max(rect.min, pos2(rect.left() + (92.0 * s).min(236.0), rect.bottom()));
    p.rect_filled(
        side.shrink(1.0),
        CornerRadius {
            nw: corner(13.0),
            sw: corner(13.0),
            ne: 0,
            se: 0,
        },
        t.sidebar,
    );
    p.vline(side.right(), side.y_range(), Stroke::new(1.0, t.border));
    if inside <= 0.001 {
        return;
    }
    let mut p = p.clone();
    p.multiply_opacity(inside);
    let p = &p;
    let logo = Rect::from_min_size(side.min + vec2(12.0, 12.0) * s, vec2(16.0, 16.0) * s);
    p.rect_filled(logo, rad(5.0), Color32::from_rgb(0xE4, 0x81, 0x4F));
    bar(
        p,
        pos2(logo.right() + 6.0 * s, logo.center().y),
        36.0 * s,
        4.0 * s,
        t.text2,
    );
    // Pages: Basic has four; Advanced folds three more in between.
    let advanced_only = [false, false, false, true, false, true, true];
    let mut y = logo.bottom() + 16.0 * s;
    for (i, extra) in advanced_only.iter().enumerate() {
        let vis = if *extra { k } else { 1.0 };
        if vis <= 0.01 {
            continue;
        }
        let h = 20.0 * s * vis;
        let row = Rect::from_min_size(pos2(side.left() + 8.0 * s, y), vec2(side.width() - 16.0 * s, 18.0 * s));
        if i == 0 {
            p.rect_filled(row, rad(5.0), t.accent_soft());
        }
        let c = if i == 0 { t.accent } else { t.text3 };
        let a = vis * if i == 0 { 1.0 } else { 0.8 };
        p.rect_filled(
            Rect::from_center_size(pos2(row.left() + 10.0 * s, row.center().y), vec2(7.0, 7.0) * s),
            rad(2.0),
            motion::with_alpha(c, a),
        );
        bar(
            p,
            pos2(row.left() + 20.0 * s, row.center().y),
            [34.0, 44.0, 30.0, 38.0, 36.0, 42.0, 28.0][i] * s,
            3.5 * s,
            motion::with_alpha(if i == 0 { t.text } else { t.text3 }, a),
        );
        y += h;
    }

    // Top bar: page title and the Add button.
    let content = Rect::from_min_max(
        pos2(side.right() + 14.0 * s, rect.top() + 14.0 * s),
        rect.max - vec2(14.0, 14.0) * s,
    );
    bar(
        p,
        pos2(content.left(), content.top() + 6.0 * s),
        64.0 * s,
        8.0 * s,
        t.text,
    );
    bar(
        p,
        pos2(content.left(), content.top() + 19.0 * s),
        44.0 * s,
        4.0 * s,
        t.text3,
    );
    let add = Rect::from_min_size(pos2(content.right() - 34.0 * s, content.top()), vec2(34.0, 14.0) * s);
    p.rect_filled(add, rad(5.0), t.accent);
    let body = Rect::from_min_max(pos2(content.left(), content.top() + 32.0 * s), content.max);
    if k < 0.999 {
        basic_page(p, t, body, s, 1.0 - k);
    }
    if k > 0.001 {
        advanced_page(p, t, body, s, k);
    }
}

fn bar(p: &Painter, left_center: Pos2, w: f32, h: f32, c: Color32) {
    let r = Rect::from_min_size(left_center - vec2(0.0, h / 2.0), vec2(w, h));
    p.rect_filled(r, CornerRadius::same((h / 2.0).round() as u8), c);
}

fn card(p: &Painter, t: &Theme, r: Rect, s: f32, a: f32) {
    p.rect(
        r,
        CornerRadius::same((8.0 * s).round().max(2.0) as u8),
        motion::with_alpha(t.card, a),
        Stroke::new(1.0, motion::with_alpha(t.border, a)),
        egui::StrokeKind::Inside,
    );
}

fn arc(p: &Painter, c: Pos2, r: f32, from: f32, to: f32, width: f32, color: Color32) {
    let n = ((to - from).abs() / TAU * 48.0).ceil().max(2.0) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(PathShape::line(pts, PathStroke::new(width, color)));
}

fn donut(p: &Painter, t: &Theme, c: Pos2, r: f32, width: f32, a: f32) {
    p.circle_stroke(c, r, Stroke::new(width, motion::with_alpha(t.hover, a)));
    let parts = [0.34, 0.24, 0.18, 0.14, 0.1];
    let mut start = -PI / 2.0;
    for (i, f) in parts.iter().enumerate() {
        let end = start + TAU * f;
        arc(
            p,
            c,
            r,
            start + 0.04,
            end - 0.04,
            width,
            motion::with_alpha(rgb(SWATCH[i]), a),
        );
        start = end;
    }
}

/// Home in Basic: this month, where it went, budgets.
fn basic_page(p: &Painter, t: &Theme, body: Rect, s: f32, a: f32) {
    let gap = 8.0 * s;
    let top_h = body.height() * 0.55;
    let left = Rect::from_min_size(body.min, vec2((body.width() - gap) * 0.45, top_h));
    let right = Rect::from_min_max(
        pos2(left.right() + gap, body.top()),
        pos2(body.right(), body.top() + top_h),
    );
    card(p, t, left, s, a);
    let x = left.left() + 10.0 * s;
    bar(
        p,
        pos2(x, left.top() + 12.0 * s),
        40.0 * s,
        4.0 * s,
        motion::with_alpha(t.text2, a),
    );
    bar(
        p,
        pos2(x, left.top() + 30.0 * s),
        70.0 * s,
        11.0 * s,
        motion::with_alpha(t.text, a),
    );
    bar(
        p,
        pos2(x, left.top() + 46.0 * s),
        56.0 * s,
        3.5 * s,
        motion::with_alpha(t.text3, a),
    );
    let flow_y = left.bottom() - 18.0 * s;
    for (i, c) in [t.pos, t.neg].into_iter().enumerate() {
        let fx = x + i as f32 * (left.width() / 2.0 - 4.0 * s);
        p.rect_filled(
            Rect::from_center_size(pos2(fx + 5.0 * s, flow_y), vec2(10.0, 10.0) * s),
            CornerRadius::same((3.0 * s) as u8),
            motion::with_alpha(t.tint(c, 0.35), a),
        );
        bar(
            p,
            pos2(fx + 14.0 * s, flow_y),
            30.0 * s,
            5.0 * s,
            motion::with_alpha(t.text2, a),
        );
    }
    card(p, t, right, s, a);
    let r = (right.height() * 0.32).min(right.width() * 0.2);
    let c = pos2(right.left() + 14.0 * s + r, right.center().y + 4.0 * s);
    donut(p, t, c, r, 7.0 * s, a);
    for (i, sw) in SWATCH.iter().take(4).enumerate() {
        let ly = right.top() + 22.0 * s + i as f32 * 14.0 * s;
        let lx = c.x + r + 18.0 * s;
        p.circle_filled(pos2(lx, ly), 2.5 * s, motion::with_alpha(rgb(*sw), a));
        bar(
            p,
            pos2(lx + 7.0 * s, ly),
            (right.right() - lx - 30.0 * s).max(8.0),
            3.5 * s,
            motion::with_alpha(t.text3, a),
        );
    }
    let bottom = Rect::from_min_max(pos2(body.left(), right.bottom() + gap), body.max);
    card(p, t, bottom, s, a);
    let rows = 3;
    for i in 0..rows {
        let y = bottom.top() + 16.0 * s + i as f32 * (bottom.height() - 24.0 * s) / rows as f32;
        let bx = bottom.left() + 12.0 * s;
        let bw = bottom.width() - 24.0 * s;
        bar(p, pos2(bx, y), 40.0 * s, 3.5 * s, motion::with_alpha(t.text2, a));
        bar(p, pos2(bx, y + 9.0 * s), bw, 4.0 * s, motion::with_alpha(t.hover, a));
        let used = [0.62, 0.38, 0.85][i];
        let c = if used > 0.8 { t.warn } else { t.pos };
        bar(p, pos2(bx, y + 9.0 * s), bw * used, 4.0 * s, motion::with_alpha(c, a));
    }
}

/// The Advanced dashboard: net worth chart, KPIs, cash flow, categories.
fn advanced_page(p: &Painter, t: &Theme, body: Rect, s: f32, a: f32) {
    let gap = 7.0 * s;
    let h1 = body.height() * 0.36;
    let h2 = body.height() * 0.2;
    // Row 1: net worth area chart + this month ring.
    let hero = Rect::from_min_size(body.min, vec2((body.width() - gap) * 0.64, h1));
    let month = Rect::from_min_max(
        pos2(hero.right() + gap, body.top()),
        pos2(body.right(), body.top() + h1),
    );
    card(p, t, hero, s, a);
    bar(
        p,
        pos2(hero.left() + 9.0 * s, hero.top() + 10.0 * s),
        36.0 * s,
        3.5 * s,
        motion::with_alpha(t.text2, a),
    );
    bar(
        p,
        pos2(hero.left() + 9.0 * s, hero.top() + 22.0 * s),
        60.0 * s,
        8.0 * s,
        motion::with_alpha(t.text, a),
    );
    let chart = Rect::from_min_max(
        pos2(hero.left() + 2.0, hero.top() + 32.0 * s),
        hero.max - vec2(2.0, 6.0 * s),
    );
    let ys = [0.7, 0.62, 0.66, 0.5, 0.54, 0.4, 0.44, 0.3, 0.26, 0.14];
    let pts: Vec<Pos2> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| {
            pos2(
                chart.left() + chart.width() * i as f32 / 9.0,
                chart.top() + chart.height() * y,
            )
        })
        .collect();
    let mut fill = pts.clone();
    fill.push(chart.right_bottom());
    fill.push(chart.left_bottom());
    p.add(egui::Shape::convex_polygon(
        fill,
        motion::with_alpha(t.accent, 0.12 * a),
        Stroke::NONE,
    ));
    p.add(PathShape::line(
        pts,
        PathStroke::new(1.5 * s.max(0.7), motion::with_alpha(t.accent, a)),
    ));
    card(p, t, month, s, a);
    let rc = month.center() + vec2(0.0, 3.0 * s);
    let rr = (month.height() * 0.3).min(month.width() * 0.3);
    p.circle_stroke(rc, rr, Stroke::new(5.0 * s, motion::with_alpha(t.hover, a)));
    arc(
        p,
        rc,
        rr,
        -PI / 2.0,
        -PI / 2.0 + TAU * 0.68,
        5.0 * s,
        motion::with_alpha(t.pos, a),
    );
    // Row 2: four KPI tiles with sparklines.
    let y2 = body.top() + h1 + gap;
    let tile_w = (body.width() - 3.0 * gap) / 4.0;
    for i in 0..4 {
        let r = Rect::from_min_size(pos2(body.left() + i as f32 * (tile_w + gap), y2), vec2(tile_w, h2));
        card(p, t, r, s, a);
        bar(
            p,
            pos2(r.left() + 7.0 * s, r.top() + 9.0 * s),
            22.0 * s,
            3.0 * s,
            motion::with_alpha(t.text3, a),
        );
        bar(
            p,
            pos2(r.left() + 7.0 * s, r.top() + 19.0 * s),
            34.0 * s,
            6.0 * s,
            motion::with_alpha(t.text, a),
        );
        let c = [t.neg, t.pos, t.accent, t.warn][i];
        let sp: Vec<Pos2> = [0.6, 0.3, 0.5, 0.2, 0.4]
            .iter()
            .enumerate()
            .map(|(j, y)| {
                pos2(
                    r.right() - 34.0 * s + j as f32 * 7.0 * s,
                    r.bottom() - 6.0 * s - y * 14.0 * s,
                )
            })
            .collect();
        p.add(PathShape::line(sp, PathStroke::new(1.2, motion::with_alpha(c, a))));
    }
    // Row 3: cash flow bars + categories donut.
    let y3 = y2 + h2 + gap;
    let flow = Rect::from_min_max(
        pos2(body.left(), y3),
        pos2(body.left() + (body.width() - gap) * 0.62, body.bottom()),
    );
    let cats = Rect::from_min_max(pos2(flow.right() + gap, y3), body.max);
    card(p, t, flow, s, a);
    let n = 8;
    let slot = (flow.width() - 16.0 * s) / n as f32;
    for i in 0..n {
        let base = flow.bottom() - 7.0 * s;
        let room = flow.height() - 18.0 * s;
        let inc = [0.8, 0.7, 0.85, 0.75, 0.9, 0.8, 0.95, 0.85][i] * room;
        let exp = [0.6, 0.65, 0.5, 0.7, 0.55, 0.6, 0.5, 0.45][i] * room;
        let x = flow.left() + 8.0 * s + i as f32 * slot + slot * 0.2;
        let bw = slot * 0.28;
        p.rect_filled(
            Rect::from_min_max(pos2(x, base - inc), pos2(x + bw, base)),
            CornerRadius::same(1),
            motion::with_alpha(t.pos, 0.85 * a),
        );
        p.rect_filled(
            Rect::from_min_max(pos2(x + bw + 1.0, base - exp), pos2(x + 2.0 * bw + 1.0, base)),
            CornerRadius::same(1),
            motion::with_alpha(t.neg, 0.8 * a),
        );
    }
    card(p, t, cats, s, a);
    let dr = (cats.height() * 0.32).min(cats.width() * 0.28);
    donut(p, t, cats.center(), dr, 5.0 * s, a);
}
