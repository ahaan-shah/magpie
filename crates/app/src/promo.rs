//! The launch promo: a calm ~30 s film of the real app for social posts.
//!
//! Two passes per frame. The app runs headlessly on a virtual clock (Paper
//! theme, demo data) driven by a scripted cursor, and renders to a texture.
//! A second egui context composites the scene in 1080p: a soft pastel
//! backdrop, magpies drifting behind, the app as a floating window, a camera
//! that eases in on each feature, the cursor, captions and title cards.
//!
//! ```text
//! cargo test --profile fast -p magpie-finance promo -- --ignored --nocapture
//! MAGPIE_PROMO_STILLS=4.5,10.2 …   # only render those moments, as PNGs (fast)
//! ```
//! Writes `~/Videos/magpie/magpie-promo.mp4` (override with MAGPIE_PROMO_OUT).

use crate::app::{App, logo};
use crate::film::{Act, Beat, Driver, Target, encoder_args, gpu_renderer, mark, paint_cursor};
use crate::motion::{ease_in_out, ease_out};
use crate::theme;
use egui::epaint::{RectShape, Shadow};
use egui::{
    Align2, Color32, Context, CornerRadius, Id, Key, Modifiers, Pos2, RawInput, Rect, Shape, Stroke, TextureOptions,
    Vec2, ViewportId, ViewportInfo, pos2, vec2,
};
use egui_kittest::TestRenderer;
use std::f32::consts::TAU;
use std::io::Write;
use std::process::{Command, Stdio};

/// The app's logical screen, and its render scale (3200x1800 px, so zooms
/// up to ~1.6x stay crisp at 1080p).
const AW: f32 = 1600.0;
const AH: f32 = 900.0;
const APP_PPP: f32 = 2.0;
/// The film, in points; rendered at 2x and supersampled down.
const OW: f32 = 1920.0;
const OH: f32 = 1080.0;
const OUT_PPP: f32 = 2.0;
const OUT_W: usize = 1920;
const OUT_H: usize = 1080;
const FPS: f32 = 60.0;
const DURATION: f32 = 30.0;

/// The app window at rest: 80% of the frame, centred.
const WIN_SCALE: f32 = 0.96;

const APP_ON: f32 = 2.6;
const OUTRO: f32 = 26.4;

// Palette: warm, light, quiet.
const INK: Color32 = Color32::from_rgb(0x2B, 0x27, 0x24);
const INK_SOFT: Color32 = Color32::from_rgb(0x6E, 0x66, 0x5F);
const INK_FAINT: Color32 = Color32::from_rgb(0x9C, 0x93, 0x8B);
const ACCENT: Color32 = Color32::from_rgb(0xD2, 0x60, 0x2A);

struct Cam {
    t: f32,
    zoom: f32,
    focus: Option<Target>,
    offset: Vec2,
}

fn cam(t: f32, zoom: f32, focus: Option<Target>, offset: Vec2) -> Cam {
    Cam { t, zoom, focus, offset }
}

struct Caption {
    t0: f32,
    t1: f32,
    icon: &'static str,
    text: &'static str,
}

fn script() -> (Vec<Beat>, Vec<Cam>, Vec<Caption>) {
    use crate::icons::ph;
    let b = |t: f32, act: Act| Beat { t, act };
    let mv = |t: f32, target: Target, off: Vec2, dur: f32| Beat {
        t,
        act: Act::Move(target, off, dur),
    };
    let cmd = Modifiers::COMMAND;
    let cash = mark(&format!("chart:{:?}", Id::new("dash-cashflow")));
    let quick = Target::Widget(Id::new("quick-add"));

    let beats = vec![
        // Dashboard: rest on the net worth, then trace the cash flow chart.
        b(2.7, Act::Move(Target::Point(pos2(1250.0, 760.0)), Vec2::ZERO, 0.01)),
        mv(3.6, mark("dash:hero"), vec2(150.0, 20.0), 1.4),
        mv(6.0, cash.clone(), vec2(-240.0, 20.0), 1.1),
        mv(7.1, cash.clone(), vec2(60.0, 0.0), 1.2),
        // Quick add.
        mv(8.2, mark("nav:Transactions"), Vec2::ZERO, 0.8),
        b(9.1, Act::Click),
        mv(9.4, quick.clone(), vec2(-200.0, 0.0), 0.8),
        b(10.3, Act::Click),
        b(10.5, Act::Type("coffee 4.50 @Blue Bottle #treats", 13.0)),
        b(13.3, Act::Key(Key::Enter, Modifiers::NONE)),
        // Budgets.
        mv(14.1, mark("nav:Budgets"), Vec2::ZERO, 0.8),
        b(15.0, Act::Click),
        mv(15.6, mark("budget:Dining"), vec2(-160.0, 0.0), 1.0),
        mv(16.9, mark("budget:Groceries"), vec2(-60.0, 0.0), 1.0),
        // Reports.
        mv(18.1, mark("nav:Reports"), Vec2::ZERO, 0.8),
        b(19.0, Act::Click),
        mv(19.6, mark("rep:cash"), vec2(-280.0, 10.0), 1.0),
        mv(20.7, mark("rep:cash"), vec2(40.0, 0.0), 1.1),
        mv(21.8, mark("rep:cash"), vec2(260.0, 10.0), 0.9),
        // Make it yours: a theme swap from the command palette.
        mv(22.6, mark("nav:Dashboard"), Vec2::ZERO, 0.7),
        b(23.3, Act::Click),
        b(23.8, Act::Key(Key::K, cmd)),
        b(24.1, Act::Type("dawn", 8.0)),
        b(24.8, Act::Key(Key::Enter, Modifiers::NONE)),
    ];

    let cams = vec![
        cam(0.0, 1.0, None, Vec2::ZERO),
        cam(3.9, 1.0, None, Vec2::ZERO),
        cam(5.0, 1.55, Some(mark("dash:hero")), vec2(0.0, 30.0)),
        cam(6.1, 1.55, Some(mark("dash:hero")), vec2(0.0, 30.0)),
        cam(7.0, 1.4, Some(cash.clone()), Vec2::ZERO),
        cam(8.1, 1.4, Some(cash), Vec2::ZERO),
        cam(9.0, 1.0, None, Vec2::ZERO),
        cam(9.8, 1.0, None, Vec2::ZERO),
        cam(10.6, 1.55, Some(quick.clone()), vec2(-40.0, 40.0)),
        cam(13.4, 1.55, Some(quick.clone()), vec2(-40.0, 40.0)),
        cam(14.2, 1.25, Some(quick), vec2(0.0, 230.0)),
        cam(14.8, 1.0, None, Vec2::ZERO),
        cam(15.4, 1.0, None, Vec2::ZERO),
        cam(16.2, 1.45, Some(mark("budget:Dining")), vec2(-60.0, 40.0)),
        cam(17.6, 1.45, Some(mark("budget:Groceries")), vec2(-60.0, 0.0)),
        cam(18.6, 1.0, None, Vec2::ZERO),
        cam(19.3, 1.0, None, Vec2::ZERO),
        cam(20.1, 1.4, Some(mark("rep:cash")), Vec2::ZERO),
        cam(22.2, 1.4, Some(mark("rep:cash")), Vec2::ZERO),
        cam(23.0, 1.0, None, Vec2::ZERO),
        cam(30.0, 1.0, None, Vec2::ZERO),
    ];

    let captions = vec![
        Caption {
            t0: 4.0,
            t1: 8.2,
            icon: ph::SQUARES_FOUR,
            text: "All your money. One calm view.",
        },
        Caption {
            t0: 9.4,
            t1: 14.2,
            icon: ph::SPARKLE,
            text: "Just type it. Magpie gets it.",
        },
        Caption {
            t0: 15.2,
            t1: 18.2,
            icon: ph::CHART_PIE_SLICE,
            text: "Budgets that keep pace with you.",
        },
        Caption {
            t0: 19.2,
            t1: 22.5,
            icon: ph::CHART_LINE_UP,
            text: "See exactly where it goes.",
        },
        Caption {
            t0: 23.4,
            t1: 26.3,
            icon: ph::PALETTE,
            text: "Make it feel like yours.",
        },
    ];
    (beats, cams, captions)
}

// ------------------------------------------------------------------ camera

/// Zoom and focus (in app points) at time `t`, eased between keyframes.
/// `None` focus means the whole window.
fn camera(cams: &[Cam], ctx: &Context, t: f32, cache: &mut Vec<Option<Pos2>>) -> (f32, Pos2) {
    if cache.len() < cams.len() {
        cache.resize(cams.len(), None);
    }
    let mut resolve = |i: usize| -> Pos2 {
        if let Some(p) = cache[i] {
            return p;
        }
        let c = &cams[i];
        let p = c
            .focus
            .as_ref()
            .and_then(|f| f.rect(ctx))
            .map(|r| r.center() + c.offset)
            .unwrap_or(pos2(AW / 2.0, AH / 2.0));
        // Freeze each keyframe's target once reached, so later layout
        // changes don't yank the camera.
        if c.t <= t {
            cache[i] = Some(p);
        }
        p
    };
    let i = cams.iter().rposition(|c| c.t <= t).unwrap_or(0);
    let j = (i + 1).min(cams.len() - 1);
    let (pa, pb) = (resolve(i), resolve(j));
    let (a, b) = (&cams[i], &cams[j]);
    let k = if j == i || b.t <= a.t {
        1.0
    } else {
        ease_in_out(((t - a.t) / (b.t - a.t)).clamp(0.0, 1.0))
    };
    (a.zoom + (b.zoom - a.zoom) * k, pa.lerp(pb, k))
}

/// Maps between app points, scene points (the frame at rest) and output
/// points (after the camera).
#[derive(Clone, Copy)]
struct View {
    win: Rect,
    zoom: f32,
    focus: Pos2,
}

impl View {
    fn new(zoom: f32, app_focus: Pos2, rise: f32) -> View {
        let size = vec2(AW, AH) * WIN_SCALE;
        let win = Rect::from_center_size(pos2(OW / 2.0, OH / 2.0 + rise), size);
        let scene_focus = win.min + app_focus.to_vec2() * WIN_SCALE;
        // At zoom 1 the camera frames the whole scene; zooming in moves it
        // towards the target, never past the window's edges.
        let k = ((zoom - 1.0) / 0.4).clamp(0.0, 1.0);
        let mut focus = pos2(OW / 2.0, OH / 2.0).lerp(scene_focus, k);
        let half = vec2(OW, OH) / (2.0 * zoom);
        let clamp = |v: f32, lo: f32, hi: f32| if lo > hi { (lo + hi) / 2.0 } else { v.clamp(lo, hi) };
        focus.x = clamp(focus.x, win.left() + half.x - 30.0, win.right() - half.x + 30.0);
        focus.y = clamp(focus.y, win.top() + half.y - 30.0, win.bottom() - half.y + 30.0);
        View { win, zoom, focus }
    }

    fn scene(&self, q: Pos2) -> Pos2 {
        pos2(OW / 2.0, OH / 2.0) + (q - self.focus) * self.zoom
    }

    fn app(&self, a: Pos2) -> Pos2 {
        self.scene(self.win.min + a.to_vec2() * WIN_SCALE)
    }

    /// Background layers move less than the window (gentle parallax).
    fn parallax(&self, q: Pos2, depth: f32) -> Pos2 {
        let z = 1.0 + (self.zoom - 1.0) * depth;
        let f = pos2(OW / 2.0, OH / 2.0).lerp(self.focus, depth);
        pos2(OW / 2.0, OH / 2.0) + (q - f) * z
    }
}

// ------------------------------------------------------------------ painting

fn backdrop(p: &egui::Painter, t: f32, view: &View) {
    let full = Rect::from_min_size(Pos2::ZERO, vec2(OW, OH));
    let top = Color32::from_rgb(0xF8, 0xF3, 0xEC);
    let bottom = Color32::from_rgb(0xEF, 0xEA, 0xE4);
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(full.left_top(), top);
    mesh.colored_vertex(full.right_top(), top);
    mesh.colored_vertex(full.right_bottom(), bottom);
    mesh.colored_vertex(full.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
    // Soft pastel light, drifting very slowly.
    let blobs = [
        (
            pos2(330.0, 250.0),
            520.0,
            Color32::from_rgba_unmultiplied(0xF6, 0xD3, 0xBE, 120),
            0.0,
        ),
        (
            pos2(1600.0, 300.0),
            560.0,
            Color32::from_rgba_unmultiplied(0xDD, 0xD8, 0xF0, 110),
            1.7,
        ),
        (
            pos2(1450.0, 900.0),
            520.0,
            Color32::from_rgba_unmultiplied(0xCF, 0xE6, 0xD8, 110),
            3.1,
        ),
        (
            pos2(420.0, 920.0),
            480.0,
            Color32::from_rgba_unmultiplied(0xD5, 0xE4, 0xF2, 100),
            4.4,
        ),
    ];
    for (c, r, col, ph) in blobs {
        let drift = vec2((t * 0.11 + ph).sin(), (t * 0.09 + ph * 1.3).cos()) * 40.0;
        crate::widgets::radial_glow(
            p,
            view.parallax(c + drift, 0.2),
            r * (1.0 + (view.zoom - 1.0) * 0.2),
            col,
        );
    }
}

/// A soft radial light: many thin rings with a smooth (gaussian-like)
/// falloff, so large pale blobs don't show rings.
fn soft_glow(p: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
    use egui::epaint::Mesh;
    let rings = 72;
    let segs = 96;
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let at = |k: f32| {
        let f = (-4.0 * k * k).exp() * (1.0 - k).max(0.0).powf(0.5);
        Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * f) as u8)
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, at(0.0));
    for ring in 1..=rings {
        let k = ring as f32 / rings as f32;
        for s in 0..segs {
            let ang = TAU * s as f32 / segs as f32;
            mesh.colored_vertex(c + vec2(ang.cos(), ang.sin()) * radius * k, at(k));
        }
    }
    for s in 0..segs as u32 {
        let n = (s + 1) % segs as u32;
        mesh.add_triangle(0, 1 + s, 1 + n);
    }
    for ring in 1..rings as u32 {
        let base = 1 + (ring - 1) * segs as u32;
        let next = base + segs as u32;
        for s in 0..segs as u32 {
            let n = (s + 1) % segs as u32;
            mesh.add_triangle(base + s, next + s, next + n);
            mesh.add_triangle(base + s, next + n, base + n);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// A magpie in flight, facing +x, about `size` points nose to tail tip.
/// `flap` runs -1 (wings down) to 1 (wings up).
fn paint_magpie(p: &egui::Painter, at: Pos2, size: f32, flip: bool, tilt: f32, flap: f32, alpha: f32) {
    let (sin, cos) = tilt.sin_cos();
    let pt = |x: f32, y: f32| {
        let x = if flip { -x } else { x };
        at + vec2(x * cos - y * sin, x * sin + y * cos) * size
    };
    let ink = Color32::from_rgb(0x33, 0x32, 0x3A).gamma_multiply(alpha);
    let sheen = Color32::from_rgb(0x3A, 0x4C, 0x60).gamma_multiply(alpha);
    let white = Color32::from_rgb(0xFF, 0xFD, 0xF8).gamma_multiply(alpha);
    let poly = |pts: Vec<Pos2>, c: Color32| {
        p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
    };
    let oval = |cx: f32, cy: f32, rx: f32, ry: f32, n: usize, c: Color32| {
        poly(
            (0..n)
                .map(|i| {
                    let a = TAU * i as f32 / n as f32;
                    pt(cx + a.cos() * rx, cy + a.sin() * ry)
                })
                .collect(),
            c,
        );
    };
    // A wing: shoulder at (x0..x1, y0), swept back to a tip; up/down with
    // the flap. Built from two convex pieces (inner arm, outer hand).
    let wing = |span: f32, c: Color32, flash: bool| {
        let lift = -flap * span; // negative y is up
        let elbow = (0.02, -0.01 + lift * 0.5);
        let tip = (-0.20, -0.01 + lift);
        poly(
            vec![
                pt(0.13, -0.02),
                pt(elbow.0 + 0.05, elbow.1),
                pt(elbow.0 - 0.07, elbow.1),
                pt(-0.09, -0.02),
            ],
            c,
        );
        poly(
            vec![
                pt(elbow.0 + 0.05, elbow.1),
                pt(tip.0, tip.1),
                pt(elbow.0 - 0.07, elbow.1),
            ],
            c,
        );
        if flash {
            // The magpie's white wing patch, on the outer half.
            let m = |a: (f32, f32), b: (f32, f32), k: f32| (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k);
            let e1 = (elbow.0 + 0.07, elbow.1);
            let e2 = (elbow.0 - 0.10, elbow.1);
            let a1 = m(e1, tip, 0.15);
            let a2 = m(e2, tip, 0.15);
            let b1 = m(e1, tip, 0.6);
            let b2 = m(e2, tip, 0.6);
            poly(
                vec![pt(a1.0, a1.1), pt(b1.0, b1.1), pt(b2.0, b2.1), pt(a2.0, a2.1)],
                white,
            );
        }
    };
    wing(0.40, ink.gamma_multiply(0.6), false); // far wing
    // Long, slightly fanned tail with a blue-green sheen.
    poly(
        vec![
            pt(-0.14, -0.03),
            pt(-0.62, -0.06),
            pt(-0.68, 0.0),
            pt(-0.62, 0.06),
            pt(-0.14, 0.04),
        ],
        sheen,
    );
    oval(0.03, 0.0, 0.21, 0.085, 22, ink); // body
    oval(0.03, 0.025, 0.12, 0.05, 16, white); // belly
    oval(0.25, -0.03, 0.075, 0.068, 16, ink); // head
    poly(vec![pt(0.31, -0.045), pt(0.40, -0.02), pt(0.31, -0.008)], ink); // beak
    wing(0.52, ink, true); // near wing
}

/// A flight path: (start, duration, from, to, size, depth alpha, flap Hz).
type Flight = (f32, f32, (f32, f32), (f32, f32), f32, f32, f32);

const FLIGHTS: &[Flight] = &[
    // Intro: above and below the title, never across it.
    (0.0, 8.0, (-160.0, 190.0), (2080.0, 120.0), 120.0, 0.9, 2.6),
    (0.7, 9.5, (2080.0, 900.0), (-160.0, 820.0), 84.0, 0.6, 3.0),
    (1.6, 10.0, (-160.0, 990.0), (2080.0, 930.0), 66.0, 0.45, 3.3),
    // While the app is up: through the margins now and then.
    (6.5, 10.0, (2080.0, 58.0), (-160.0, 46.0), 70.0, 0.5, 3.1),
    (13.0, 10.0, (-160.0, 1030.0), (2080.0, 1012.0), 72.0, 0.5, 3.1),
    (19.5, 9.0, (2080.0, 62.0), (-160.0, 80.0), 74.0, 0.55, 3.0),
    // Outro: a little flock passes above and below the sign-off.
    (25.4, 7.0, (-160.0, 150.0), (2080.0, 90.0), 116.0, 0.9, 2.6),
    (26.2, 8.0, (2080.0, 930.0), (-160.0, 880.0), 88.0, 0.65, 2.9),
    (26.9, 7.5, (-160.0, 230.0), (2080.0, 200.0), 64.0, 0.45, 3.3),
];

fn birds(p: &egui::Painter, t: f32, view: &View) {
    for (i, &(t0, dur, from, to, size, alpha, hz)) in FLIGHTS.iter().enumerate() {
        let k = (t - t0) / dur;
        if !(0.0..=1.0).contains(&k) {
            continue;
        }
        let ph = i as f32 * 1.7;
        let (a, b) = (pos2(from.0, from.1), pos2(to.0, to.1));
        let bob = (t * 1.6 + ph).sin() * 10.0;
        let pos = a.lerp(b, k) + vec2(0.0, bob);
        let slope = (b.y - a.y) / (b.x - a.x).abs().max(1.0) + (t * 1.6 + ph).cos() * 0.08;
        // Flap in bursts, then glide: calmer than constant flapping.
        let burst = ((t * 0.55 + ph).sin() * 0.5 + 0.5).powf(0.6);
        let flap = (t * hz * TAU + ph).sin() * (0.25 + 0.75 * burst) + 0.15;
        let depth = 0.1 + 0.25 * alpha;
        let at = view.parallax(pos, depth);
        let fade = (k / 0.05).min((1.0 - k) / 0.05).clamp(0.0, 1.0);
        paint_magpie(p, at, size, b.x < a.x, slope.atan() * 0.6, flap, alpha * fade);
    }
}

fn caption(p: &egui::Painter, t: f32, c: &Caption) {
    if t < c.t0 || t > c.t1 {
        return;
    }
    let a = ease_out(((t - c.t0) / 0.6).min((c.t1 - t) / 0.5).clamp(0.0, 1.0));
    let icon = p.layout_no_wrap(c.icon.to_string(), theme::regular(26.0), ACCENT.gamma_multiply(a));
    let text = p.layout_no_wrap(c.text.to_string(), theme::semibold(28.0), INK.gamma_multiply(a));
    let w = icon.size().x + 14.0 + text.size().x;
    let size = vec2(w + 64.0, 68.0);
    let mid = pos2(OW / 2.0, OH - 82.0 + (1.0 - a) * 16.0);
    let r = Rect::from_center_size(mid, size);
    let radius = CornerRadius::same(34);
    p.add(
        Shadow {
            offset: [0, 10],
            blur: 36,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(0x6B, 0x55, 0x40, (40.0 * a) as u8),
        }
        .as_shape(r, radius),
    );
    p.rect(
        r,
        radius,
        Color32::from_rgba_unmultiplied(0xFF, 0xFD, 0xFA, (235.0 * a) as u8),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x2B, 0x27, 0x24, (18.0 * a) as u8)),
        egui::StrokeKind::Inside,
    );
    let x0 = r.left() + 32.0;
    p.galley(pos2(x0, mid.y - icon.size().y / 2.0), icon, ACCENT);
    p.galley(pos2(x0 + w - text.size().x, mid.y - text.size().y / 2.0), text, INK);
}

/// Title card: logo, name, tagline and a small line, each settling in.
fn title_card(p: &egui::Painter, k: f32, alpha: f32, outro: bool) {
    if alpha <= 0.0 {
        return;
    }
    let rise = |d: f32| ease_out(((k - d) / 0.7).clamp(0.0, 1.0));
    let c = pos2(OW / 2.0, OH / 2.0 - 30.0);
    let k0 = rise(0.0);
    let float = (k * 1.4).sin() * 3.0;
    let size = 128.0 * (0.92 + 0.08 * k0);
    if k0 > 0.0 {
        soft_glow(
            p,
            c - vec2(0.0, 80.0),
            340.0,
            Color32::from_rgba_unmultiplied(0xF2, 0xB8, 0x94, (70.0 * alpha * k0) as u8),
        );
        let lr = Rect::from_center_size(c - vec2(0.0, 92.0 - (1.0 - k0) * 14.0 - float), Vec2::splat(size));
        // Fade the logo in by drawing it to a layer at reduced opacity.
        let lp = p.clone().with_layer_id(p.layer_id());
        let mut lp = lp;
        lp.set_opacity(alpha * k0);
        logo(&lp, lr, &theme::Theme::by_name("Paper"));
    }
    let k1 = rise(0.35);
    p.text(
        c + vec2(0.0, 52.0 + (1.0 - k1) * 14.0),
        Align2::CENTER_CENTER,
        "Magpie",
        theme::display(84.0),
        INK.gamma_multiply(alpha * k1),
    );
    let k2 = rise(0.7);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "A calm nest for your money.",
        0.0,
        egui::TextFormat {
            font_id: theme::regular(30.0),
            color: INK_SOFT.gamma_multiply(alpha * k2),
            italics: true,
            ..Default::default()
        },
    );
    let g = p.layout_job(job);
    p.galley(c + vec2(-g.size().x / 2.0, 108.0 + (1.0 - k2) * 10.0), g, INK_SOFT);
    let k3 = rise(1.1);
    let line = if outro {
        "Free & open source · Private by design · Linux & macOS"
    } else {
        "Local-first personal finance"
    };
    p.text(
        c + vec2(0.0, 196.0),
        Align2::CENTER_CENTER,
        line,
        theme::medium(20.0),
        INK_FAINT.gamma_multiply(alpha * k3),
    );
    if outro {
        let k4 = rise(1.5);
        p.text(
            c + vec2(0.0, 236.0),
            Align2::CENTER_CENTER,
            "github.com/ahaan-shah/magpie",
            theme::semibold(22.0),
            ACCENT.gamma_multiply(alpha * k4),
        );
    }
}

/// Supersamples the 2x render down to the output size (2x2 box filter).
fn downsample(src: &image::RgbaImage, out: &mut [u8]) {
    let sw = src.width() as usize;
    let data = src.as_raw();
    for (y, row) in out.chunks_mut(OUT_W * 4).enumerate() {
        for x in 0..OUT_W {
            let mut acc = [0u32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = ((y * 2 + dy) * sw + x * 2 + dx) * 4;
                for c in 0..3 {
                    acc[c] += data[i + c] as u32;
                }
            }
            let o = &mut row[x * 4..x * 4 + 4];
            o[0] = (acc[0] / 4) as u8;
            o[1] = (acc[1] / 4) as u8;
            o[2] = (acc[2] / 4) as u8;
            o[3] = 255;
        }
    }
}

fn raw(size: Vec2, ppp: f32, t: f32, events: Vec<egui::Event>) -> RawInput {
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(t as f64),
        events,
        ..Default::default()
    };
    raw.viewports.insert(
        ViewportId::ROOT,
        ViewportInfo {
            native_pixels_per_point: Some(ppp),
            ..Default::default()
        },
    );
    raw
}

#[test]
#[ignore = "renders the launch promo (slow); run explicitly"]
fn promo() {
    // SAFETY: single-threaded test setup before the app reads these.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var("MAGPIE_TODAY", "2026-09-29");
    }
    crate::marks::enable();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let out_path =
        std::env::var("MAGPIE_PROMO_OUT").unwrap_or_else(|_| format!("{home}/Videos/magpie/magpie-promo.mp4"));
    let stills: Option<Vec<f32>> = std::env::var("MAGPIE_PROMO_STILLS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let out_dir = std::path::Path::new(&out_path).parent().expect("out dir").to_path_buf();
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let dir = std::env::temp_dir().join("magpie-promo");
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = magpie_core::Store::open(&dir).expect("store");
    magpie_core::demo::generate(&mut store, magpie_core::Cur::USD, 24, 0).expect("demo");
    store.update_settings(|s| s.theme = "Paper".into()).expect("theme");

    let app_ctx = Context::default();
    let mut app = App::with_context(&app_ctx, None, store);
    let mut app_renderer = gpu_renderer();
    let comp = Context::default();
    theme::install_fonts(&comp, "Inter");
    let mut comp_renderer = gpu_renderer();
    let mut tex: Option<egui::TextureHandle> = None;

    let (beats, cams, captions) = script();
    let mut driver = Driver::new(beats, pos2(AW / 2.0, AH / 2.0));
    let mut cache = Vec::new();

    let mut ffmpeg = stills.is_none().then(|| {
        Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", &format!("{OUT_W}x{OUT_H}"), "-r", &format!("{FPS}"), "-i", "-"])
            .args(encoder_args())
            .args(["-movflags", "+faststart", &out_path])
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg")
    });
    let mut sink = ffmpeg.as_mut().map(|f| f.stdin.take().expect("stdin"));
    let mut out = vec![0u8; OUT_W * OUT_H * 4];
    let frames = (DURATION * FPS) as usize;
    let clock = std::time::Instant::now();
    let mut started = false;

    for frame in 0..frames {
        let t = frame as f32 / FPS;
        let events = driver.events(&app_ctx, t);
        let on = t >= APP_ON;
        if on && !started {
            started = true;
            app.shown_at = t as f64;
        }
        let mut app_out = app_ctx.run_ui(
            raw(vec2(AW, AH), APP_PPP, t, if on { events } else { Vec::new() }),
            |ui| app.frame(ui),
        );
        let wanted = match &stills {
            Some(list) => list.iter().any(|s| (s - t).abs() < 0.5 / FPS),
            None => true,
        };
        app_renderer.handle_delta(&mut app_out.textures_delta);
        if !wanted {
            continue;
        }
        let shot = app_renderer.render(&app_ctx, &app_out).expect("render app");
        let image =
            egui::ColorImage::from_rgba_unmultiplied([shot.width() as usize, shot.height() as usize], shot.as_raw());
        let opts = TextureOptions::LINEAR;
        match &mut tex {
            Some(h) => h.set(image, opts),
            None => tex = Some(comp.load_texture("app", image, opts)),
        }
        let tex_id = tex.as_ref().expect("texture").id();

        let (zoom, focus) = camera(&cams, &app_ctx, t, &mut cache);
        // The window floats up in at the start and settles away at the end.
        let rise_in = ease_out(((t - APP_ON) / 1.1).clamp(0.0, 1.0));
        let fall_out = ease_in_out(((t - OUTRO) / 0.9).clamp(0.0, 1.0));
        let win_alpha = rise_in * (1.0 - fall_out);
        let rise = (1.0 - rise_in) * 60.0 - fall_out * 24.0;
        let view = View::new(zoom, focus, rise);
        let pointer = &driver.pointer;

        let mut comp_out = comp.run_ui(raw(vec2(OW, OH), OUT_PPP, t, Vec::new()), |ui| {
            let p = ui.painter().clone();
            backdrop(&p, t, &view);
            birds(&p, t, &view);
            if win_alpha > 0.0 {
                let r = Rect::from_min_max(view.app(Pos2::ZERO), view.app(pos2(AW, AH)));
                let radius = CornerRadius::same((18.0 * view.zoom).min(250.0) as u8);
                p.add(
                    Shadow {
                        offset: [0, (26.0 * view.zoom) as i8],
                        blur: (70.0 * view.zoom).min(255.0) as u8,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0x6B, 0x50, 0x3A, (52.0 * win_alpha) as u8),
                    }
                    .as_shape(r, radius),
                );
                let tint = Color32::WHITE.gamma_multiply(win_alpha);
                p.add(
                    RectShape::filled(r, radius, tint)
                        .with_texture(tex_id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
                );
                p.rect_stroke(
                    r,
                    radius,
                    Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(0x2B, 0x27, 0x24, (22.0 * win_alpha) as u8),
                    ),
                    egui::StrokeKind::Outside,
                );
                // Cursor and click ripples, in the window's space.
                if (APP_ON + 0.6..OUTRO + 0.2).contains(&t) {
                    let s = 1.15 * (1.0 + (view.zoom - 1.0) * 0.4);
                    for (at, t0) in &pointer.ripples {
                        let k = (t - t0) / 0.6;
                        if (0.0..1.0).contains(&k) {
                            let c = view.app(*at);
                            let rr = (10.0 + 30.0 * ease_out(k)) * s;
                            p.circle_filled(c, rr, ACCENT.gamma_multiply(0.18 * (1.0 - k)));
                            p.circle_stroke(c, rr, Stroke::new(2.0 * s, ACCENT.gamma_multiply(0.55 * (1.0 - k))));
                        }
                    }
                    paint_cursor(&p, view.app(pointer.pos), s);
                }
            }
            for c in &captions {
                caption(&p, t, c);
            }
            let intro_alpha = 1.0 - ease_in_out(((t - 2.3) / 0.8).clamp(0.0, 1.0));
            title_card(&p, (t - 0.2) / 1.0 * 1.0, intro_alpha, false);
            if t > OUTRO {
                let a = ease_in_out(((t - OUTRO - 0.4) / 0.8).clamp(0.0, 1.0));
                title_card(&p, t - OUTRO - 0.5, a, true);
            }
        });
        comp_renderer.handle_delta(&mut comp_out.textures_delta);
        let img = comp_renderer.render(&comp, &comp_out).expect("render scene");
        downsample(&img, &mut out);
        match &mut sink {
            Some(s) => s.write_all(&out).expect("write frame"),
            None => {
                let path = out_dir.join(format!("promo-{t:05.2}.png"));
                if let Some(png) = image::RgbaImage::from_raw(OUT_W as u32, OUT_H as u32, out.clone()) {
                    png.save(&path).expect("save still");
                    eprintln!("promo: wrote {}", path.display());
                }
            }
        }
        if frame % 300 == 0 {
            eprintln!("promo: {:>4.1}s / {DURATION}s  ({:.0?} elapsed)", t, clock.elapsed());
        }
    }
    drop(sink);
    if let Some(mut f) = ffmpeg {
        assert!(f.wait().expect("ffmpeg").success(), "ffmpeg failed");
        eprintln!("promo: wrote {out_path} in {:.0?}", clock.elapsed());
    }
}
