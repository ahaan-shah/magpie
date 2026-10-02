//! Renders the README demo film, frame-perfect, straight from the real app.
//!
//! The app runs headlessly on a virtual clock with a scripted timeline of
//! pointer moves, clicks, typing and shortcuts. Each frame is rendered by
//! wgpu at 2160p, then a virtual camera (smooth zooms and pans) crops and
//! supersamples it down to 1080p, and the frames are piped into ffmpeg.
//!
//! ```text
//! cargo test --profile fast -p magpie-finance film -- --ignored --nocapture
//! ```
//! Writes `docs/magpie-demo.mp4` (override with MAGPIE_FILM_OUT).

use crate::app::{App, logo};
use crate::motion::{ease_in_out, ease_out, ease_out_back};
use crate::theme;
use egui::{
    Align2, Color32, Context, CornerRadius, Event, FontId, Id, Key, LayerId, Modifiers, Order, PointerButton, Pos2,
    RawInput, Rect, Shape, Stroke, Vec2, ViewportId, ViewportInfo, pos2, vec2,
};
use egui_kittest::TestRenderer;
use egui_kittest::wgpu::WgpuTestRenderer;
use std::io::Write;
use std::process::{Command, Stdio};

/// Logical size of the "screen" the app is laid out in.
const W: f32 = 1600.0;
const H: f32 = 900.0;
/// Render scale: 1600x900 points at 2.4 = 3840x2160 pixels, so camera zooms
/// up to 2x stay sharp at 1080p.
const PPP: f32 = 2.4;
const OUT_W: usize = 1920;
const OUT_H: usize = 1080;
const FPS: f32 = 60.0;
const DURATION: f32 = 45.0;

const INTRO_END: f32 = 3.0;
const OUTRO_START: f32 = 41.8;

#[derive(Clone)]
enum Target {
    Mark(String),
    Widget(Id),
    Point(Pos2),
}

impl Target {
    fn rect(&self, ctx: &Context) -> Option<Rect> {
        match self {
            Target::Mark(k) => crate::marks::get(k),
            Target::Widget(id) => ctx.read_response(*id).map(|r| r.rect),
            Target::Point(p) => Some(Rect::from_center_size(*p, Vec2::ZERO)),
        }
    }
}

fn mark(k: &str) -> Target {
    Target::Mark(k.to_string())
}

#[derive(Clone)]
enum Act {
    /// Glide the pointer to a target (optionally offset) over `dur` seconds.
    Move(Target, Vec2, f32),
    Click,
    Type(&'static str, f32),
    Key(Key, Modifiers),
}

struct Beat {
    t: f32,
    act: Act,
}

/// Camera keyframe: zoom level and what to centre on.
#[derive(Clone)]
struct Cam {
    t: f32,
    zoom: f32,
    focus: Target,
    offset: Vec2,
}

fn cam(t: f32, zoom: f32, focus: Target) -> Cam {
    Cam {
        t,
        zoom,
        focus,
        offset: Vec2::ZERO,
    }
}

struct Caption {
    t0: f32,
    t1: f32,
    icon: &'static str,
    text: &'static str,
}

fn script() -> (Vec<Beat>, Vec<Cam>, Vec<Caption>) {
    use crate::icons::ph;
    let screen = Target::Point(pos2(W / 2.0, H / 2.0));
    let b = |t: f32, act: Act| Beat { t, act };
    let mv = |t: f32, target: Target, dur: f32| Beat {
        t,
        act: Act::Move(target, Vec2::ZERO, dur),
    };
    let mv_off = |t: f32, target: Target, off: Vec2, dur: f32| Beat {
        t,
        act: Act::Move(target, off, dur),
    };
    let cmd = Modifiers::COMMAND;
    let cash = format!("chart:{:?}", Id::new("dash-cashflow"));
    let rep = format!("chart:{:?}", Id::new(("rep-cash", 0usize)));

    let beats = vec![
        // Dashboard
        b(3.1, Act::Move(Target::Point(pos2(1180.0, 760.0)), Vec2::ZERO, 0.01)),
        mv_off(3.6, mark("dash:hero"), vec2(120.0, 10.0), 1.3),
        mv_off(6.4, Target::Mark(cash.clone()), vec2(-260.0, 20.0), 1.0),
        mv_off(7.5, Target::Mark(cash.clone()), vec2(40.0, 10.0), 1.0),
        mv_off(8.6, Target::Mark(cash.clone()), vec2(250.0, 30.0), 0.7),
        // Quick add
        mv(9.4, mark("nav:Transactions"), 0.6),
        b(10.1, Act::Click),
        mv_off(10.5, Target::Widget(Id::new("quick-add")), vec2(-180.0, 0.0), 0.7),
        b(11.3, Act::Click),
        b(11.6, Act::Type("lunch at Sushi Zen 18.40 /dining #date-night", 15.0)),
        b(15.2, Act::Key(Key::Enter, Modifiers::NONE)),
        // New account
        mv(17.0, mark("nav:Accounts"), 0.7),
        b(17.8, Act::Click),
        mv(18.4, mark("btn:Account"), 0.7),
        b(19.2, Act::Click),
        mv(19.9, Target::Widget(Id::new("acc-name")), 0.6),
        b(20.6, Act::Click),
        b(20.8, Act::Type("Japan travel fund", 14.0)),
        mv(22.2, Target::Widget(Id::new("acc-open")), 0.5),
        b(22.8, Act::Click),
        b(23.0, Act::Type("2,500", 10.0)),
        mv(23.7, mark("btn:Create account"), 0.6),
        b(24.4, Act::Click),
        // Budgets
        mv(26.2, mark("nav:Budgets"), 0.7),
        b(27.0, Act::Click),
        mv_off(28.0, mark("budget:Dining"), vec2(-200.0, 0.0), 0.8),
        mv_off(29.0, mark("budget:Groceries"), vec2(-120.0, 0.0), 0.8),
        mv_off(30.0, mark("budget:Transport"), vec2(0.0, 0.0), 0.7),
        // Reports
        mv(30.9, mark("nav:Reports"), 0.6),
        b(31.6, Act::Click),
        mv_off(32.4, Target::Mark(rep.clone()), vec2(-300.0, 20.0), 0.8),
        mv_off(33.4, Target::Mark(rep.clone()), vec2(-40.0, 10.0), 0.8),
        mv_off(34.4, Target::Mark(rep.clone()), vec2(200.0, 20.0), 0.6),
        // Themes via the command palette
        mv(35.3, mark("nav:Dashboard"), 0.6),
        b(36.0, Act::Click),
        mv_off(36.4, screen.clone(), vec2(260.0, 180.0), 0.5),
        b(36.9, Act::Key(Key::K, cmd)),
        b(37.25, Act::Type("paper", 9.0)),
        b(38.2, Act::Key(Key::Enter, Modifiers::NONE)),
        b(39.3, Act::Key(Key::K, cmd)),
        b(39.6, Act::Type("tokyo", 9.0)),
        b(40.5, Act::Key(Key::Enter, Modifiers::NONE)),
    ];

    let cams = vec![
        cam(0.0, 1.14, screen.clone()),
        cam(2.4, 1.14, screen.clone()),
        cam(4.0, 1.0, screen.clone()),
        cam(4.4, 1.0, screen.clone()),
        Cam {
            t: 5.6,
            zoom: 1.5,
            focus: mark("dash:hero"),
            offset: vec2(0.0, 10.0),
        },
        Cam {
            t: 6.5,
            zoom: 1.5,
            focus: mark("dash:hero"),
            offset: vec2(0.0, 10.0),
        },
        cam(7.8, 1.35, Target::Mark(cash.clone())),
        cam(8.9, 1.35, Target::Mark(cash)),
        cam(9.9, 1.0, screen.clone()),
        cam(10.3, 1.0, screen.clone()),
        Cam {
            t: 11.3,
            zoom: 1.32,
            focus: Target::Widget(Id::new("quick-add")),
            offset: vec2(0.0, 30.0),
        },
        Cam {
            t: 15.1,
            zoom: 1.32,
            focus: Target::Widget(Id::new("quick-add")),
            offset: vec2(0.0, 30.0),
        },
        Cam {
            t: 16.3,
            zoom: 1.15,
            focus: Target::Widget(Id::new("quick-add")),
            offset: vec2(0.0, 250.0),
        },
        Cam {
            t: 16.9,
            zoom: 1.15,
            focus: Target::Widget(Id::new("quick-add")),
            offset: vec2(0.0, 250.0),
        },
        cam(17.8, 1.0, screen.clone()),
        cam(19.3, 1.0, screen.clone()),
        cam(20.1, 1.45, screen.clone()),
        cam(24.5, 1.45, screen.clone()),
        cam(25.4, 1.0, screen.clone()),
        cam(27.4, 1.0, screen.clone()),
        Cam {
            t: 28.4,
            zoom: 1.3,
            focus: mark("budget:Dining"),
            offset: vec2(-80.0, -60.0),
        },
        Cam {
            t: 30.4,
            zoom: 1.3,
            focus: mark("budget:Transport"),
            offset: vec2(-80.0, -40.0),
        },
        cam(31.4, 1.0, screen.clone()),
        cam(32.0, 1.0, screen.clone()),
        cam(33.0, 1.35, Target::Mark(rep.clone())),
        cam(34.9, 1.35, Target::Mark(rep)),
        cam(35.8, 1.0, screen.clone()),
        cam(40.8, 1.0, screen.clone()),
        cam(42.4, 1.12, screen.clone()),
        cam(45.0, 1.12, screen),
    ];

    let captions = vec![
        Caption {
            t0: 3.6,
            t1: 9.0,
            icon: ph::SQUARES_FOUR,
            text: "Your money, at a glance",
        },
        Caption {
            t0: 10.4,
            t1: 16.9,
            icon: ph::SPARKLE,
            text: "Add anything in plain English",
        },
        Caption {
            t0: 18.4,
            t1: 25.9,
            icon: ph::WALLET,
            text: "Every account, any currency",
        },
        Caption {
            t0: 27.6,
            t1: 30.8,
            icon: ph::CHART_PIE_SLICE,
            text: "Budgets that keep pace with you",
        },
        Caption {
            t0: 32.2,
            t1: 35.2,
            icon: ph::CHART_LINE_UP,
            text: "Reports that actually make sense",
        },
        Caption {
            t0: 36.6,
            t1: 41.4,
            icon: ph::PALETTE,
            text: "14 themes · 9 fonts · make it yours",
        },
    ];
    (beats, cams, captions)
}

/// Pointer state machine driven by the beats.
struct Pointer {
    pos: Pos2,
    from: Pos2,
    to: Pos2,
    start: f32,
    dur: f32,
    ripples: Vec<(Pos2, f32)>,
}

impl Pointer {
    fn at(&self, t: f32) -> Pos2 {
        if self.dur <= 0.0 {
            return self.to;
        }
        let k = ease_in_out(((t - self.start) / self.dur).clamp(0.0, 1.0));
        // A gentle arc instead of a dead-straight line feels hand-driven.
        let mid = self.from.lerp(self.to, k);
        let d = self.to - self.from;
        let normal = vec2(-d.y, d.x).normalized() * (d.length() * 0.06);
        mid + normal * (k * std::f32::consts::PI).sin()
    }
}

/// Camera at time t: (zoom, centre) in UI points, eased between keyframes.
fn camera(cams: &[Cam], ctx: &Context, t: f32, cache: &mut Vec<Option<Pos2>>) -> (f32, Pos2) {
    let resolve = |i: usize, cache: &mut Vec<Option<Pos2>>| -> Pos2 {
        if let Some(Some(p)) = cache.get(i) {
            return *p;
        }
        let c = &cams[i];
        c.focus
            .rect(ctx)
            .map(|r| r.center() + c.offset)
            .unwrap_or(pos2(W / 2.0, H / 2.0))
    };
    let i = cams.iter().rposition(|c| c.t <= t).unwrap_or(0);
    let j = (i + 1).min(cams.len() - 1);
    let (a, b) = (&cams[i], &cams[j]);
    let pa = resolve(i, cache);
    let pb = resolve(j, cache);
    // Freeze a keyframe's focus once we're past it, so layout changes on a
    // later page don't yank the camera.
    if cache.len() < cams.len() {
        cache.resize(cams.len(), None);
    }
    if cache[i].is_none() {
        cache[i] = Some(pa);
    }
    let k = if j == i || b.t <= a.t {
        1.0
    } else {
        ease_in_out((t - a.t) / (b.t - a.t))
    };
    let zoom = a.zoom + (b.zoom - a.zoom) * k;
    let mut c = pa.lerp(pb, k);
    // Keep the view inside the screen.
    let half = vec2(W, H) / (2.0 * zoom);
    c.x = c.x.clamp(half.x, W - half.x);
    c.y = c.y.clamp(half.y, H - half.y);
    (zoom, c)
}

/// Draws cursor, ripples, captions and title cards on top of the app.
fn overlay(ctx: &Context, t: f32, zoom: f32, center: Pos2, pointer: &Pointer, captions: &[Caption]) {
    let p = ctx.layer_painter(LayerId::new(Order::Debug, Id::new("film")));
    // Output-space (what the viewer sees, in unzoomed points) -> UI points.
    let to_ui = |q: Pos2| center + (q - pos2(W / 2.0, H / 2.0)) / zoom;
    let s = 1.0 / zoom;

    // Captions: a frosted pill, bottom centre.
    for c in captions {
        if t < c.t0 || t > c.t1 {
            continue;
        }
        let a = ease_out(((t - c.t0) / 0.35).min((c.t1 - t) / 0.35).clamp(0.0, 1.0));
        let text = format!("{}   {}", c.icon, c.text);
        let font = theme::semibold(22.0 * s);
        let g = p.layout_no_wrap(text, font, Color32::WHITE);
        let size = g.size() + vec2(56.0, 26.0) * s;
        let mid = to_ui(pos2(W / 2.0, H - 64.0 + (1.0 - a) * 18.0));
        let r = Rect::from_center_size(mid, size);
        p.rect_filled(
            r.translate(vec2(0.0, 6.0 * s)),
            CornerRadius::same((size.y / 2.0) as u8),
            Color32::from_black_alpha((70.0 * a) as u8),
        );
        p.rect(
            r,
            CornerRadius::same((size.y / 2.0) as u8),
            Color32::from_rgba_unmultiplied(14, 16, 22, (225.0 * a) as u8),
            Stroke::new(1.0 * s, Color32::from_white_alpha((40.0 * a) as u8)),
            egui::StrokeKind::Inside,
        );
        p.galley(r.center() - g.size() / 2.0, g, Color32::WHITE.gamma_multiply(a));
    }

    // Click ripples and the cursor (hidden during title cards).
    if t > INTRO_END && t < OUTRO_START + 0.3 {
        for (at, t0) in &pointer.ripples {
            let k = (t - t0) / 0.45;
            if (0.0..1.0).contains(&k) {
                let r = (8.0 + 26.0 * ease_out(k)) * s;
                p.circle_stroke(
                    *at,
                    r,
                    Stroke::new(3.0 * s, Color32::from_white_alpha((190.0 * (1.0 - k)) as u8)),
                );
                p.circle_filled(*at, r * 0.6, Color32::from_white_alpha((50.0 * (1.0 - k)) as u8));
            }
        }
        let tip = pointer.pos;
        let arrow = [
            (0.0, 0.0),
            (0.0, 17.0),
            (4.2, 13.0),
            (7.2, 19.6),
            (9.9, 18.4),
            (7.0, 12.0),
            (12.6, 12.0),
        ];
        let pts: Vec<Pos2> = arrow.iter().map(|(x, y)| tip + vec2(*x, *y) * 1.25 * s).collect();
        let shadow: Vec<Pos2> = pts.iter().map(|q| *q + vec2(1.2, 2.0) * s).collect();
        p.add(Shape::convex_polygon(
            shadow,
            Color32::from_black_alpha(70),
            Stroke::NONE,
        ));
        p.add(Shape::closed_line(pts.clone(), Stroke::new(2.4 * s, Color32::BLACK)));
        // Fill as two convex pieces (the arrow outline is concave).
        p.add(Shape::convex_polygon(
            vec![pts[0], pts[1], pts[2], pts[5], pts[6]],
            Color32::WHITE,
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(
            vec![pts[2], pts[3], pts[4], pts[5]],
            Color32::WHITE,
            Stroke::NONE,
        ));
        p.add(Shape::closed_line(pts, Stroke::new(1.1 * s, Color32::BLACK)));
    }

    // Title cards.
    let card = |alpha: f32, title_k: f32, outro: bool| {
        if alpha <= 0.0 {
            return;
        }
        let full = Rect::from_min_max(to_ui(pos2(0.0, 0.0)), to_ui(pos2(W, H)));
        let bg_top = Color32::from_rgb(0x15, 0x11, 0x0E);
        let bg_bot = Color32::from_rgb(0x08, 0x08, 0x0B);
        let mut mesh = egui::epaint::Mesh::default();
        let a = |c: Color32| c.gamma_multiply(alpha);
        mesh.colored_vertex(full.left_top(), a(bg_top));
        mesh.colored_vertex(full.right_top(), a(bg_top));
        mesh.colored_vertex(full.right_bottom(), a(bg_bot));
        mesh.colored_vertex(full.left_bottom(), a(bg_bot));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));
        crate::widgets::radial_glow(
            &p,
            to_ui(pos2(W / 2.0, H / 2.0 - 30.0)),
            520.0 * s,
            Color32::from_rgba_unmultiplied(0xD2, 0x60, 0x2A, (60.0 * alpha) as u8),
        );
        // Logo pops in, title and tagline rise.
        let pop = ease_out_back(title_k.clamp(0.0, 1.0));
        let logo_size = 132.0 * s * (0.6 + 0.4 * pop);
        let lc = to_ui(pos2(W / 2.0, H / 2.0 - 92.0));
        if pop > 0.01 {
            logo(
                &p,
                Rect::from_center_size(lc, Vec2::splat(logo_size)),
                &theme::Theme::by_name("Paper"),
            );
        }
        let rise = |k: f32| ease_out(k.clamp(0.0, 1.0));
        let k1 = rise((title_k - 0.35) / 0.6);
        let k2 = rise((title_k - 0.7) / 0.6);
        p.text(
            to_ui(pos2(W / 2.0, H / 2.0 + 40.0 + (1.0 - k1) * 16.0)),
            Align2::CENTER_CENTER,
            "Magpie",
            FontId::new(76.0 * s, egui::FontFamily::Name("display".into())),
            Color32::WHITE.gamma_multiply(alpha * k1),
        );
        let sub = if outro {
            "github.com/ahaan-shah/magpie"
        } else {
            "Personal finance — fast, beautiful, yours."
        };
        p.text(
            to_ui(pos2(W / 2.0, H / 2.0 + 102.0 + (1.0 - k2) * 12.0)),
            Align2::CENTER_CENTER,
            sub,
            theme::medium(25.0 * s),
            Color32::from_rgb(0xD9, 0xC8, 0xBD).gamma_multiply(alpha * k2),
        );
        let k3 = rise((title_k - 1.0) / 0.6);
        p.text(
            to_ui(pos2(W / 2.0, H - 70.0)),
            Align2::CENTER_CENTER,
            if outro {
                "Linux · macOS · Local-first · Open source"
            } else {
                "Built in Rust · Local-first · Linux & macOS"
            },
            theme::regular(17.0 * s),
            Color32::from_rgb(0x8A, 0x80, 0x78).gamma_multiply(alpha * k3),
        );
    };
    if t < INTRO_END + 0.3 {
        let alpha = 1.0 - ease_in_out(((t - 2.35) / 0.75).clamp(0.0, 1.0));
        card(alpha, (t - 0.15) / 0.9, false);
    }
    if t > OUTRO_START {
        let alpha = ease_in_out(((t - OUTRO_START) / 0.6).clamp(0.0, 1.0));
        card(alpha, (t - OUTRO_START - 0.3) / 0.9, true);
    }
}

/// Crops `view` (in source pixels) out of `src` and resamples it to the
/// output size with 2x2 supersampled bilinear filtering (sub-pixel accurate,
/// so slow pans and zooms don't shimmer). Multithreaded by row bands.
fn resample(src: &image::RgbaImage, view: Rect, out: &mut [u8]) {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let data = src.as_raw();
    let sx = view.width() / OUT_W as f32;
    let sy = view.height() / OUT_H as f32;
    let sample = |x: f32, y: f32| -> [f32; 3] {
        let x = x.clamp(0.0, (sw - 1) as f32);
        let y = y.clamp(0.0, (sh - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |xx: usize, yy: usize, c: usize| data[(yy * sw + xx) * 4 + c] as f32;
        let mut o = [0.0; 3];
        for (c, v) in o.iter_mut().enumerate() {
            let top = px(x0, y0, c) * (1.0 - fx) + px(x1, y0, c) * fx;
            let bot = px(x0, y1, c) * (1.0 - fx) + px(x1, y1, c) * fx;
            *v = top * (1.0 - fy) + bot * fy;
        }
        o
    };
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let band = OUT_H.div_ceil(threads);
    std::thread::scope(|scope| {
        for (bi, chunk) in out.chunks_mut(band * OUT_W * 4).enumerate() {
            scope.spawn(move || {
                for (ry, row) in chunk.chunks_mut(OUT_W * 4).enumerate() {
                    let oy = bi * band + ry;
                    for ox in 0..OUT_W {
                        let mut acc = [0.0f32; 3];
                        for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                            let s = sample(
                                view.left() + (ox as f32 + dx) * sx - 0.5,
                                view.top() + (oy as f32 + dy) * sy - 0.5,
                            );
                            for c in 0..3 {
                                acc[c] += s[c];
                            }
                        }
                        let o = &mut row[ox * 4..ox * 4 + 4];
                        o[0] = (acc[0] / 4.0).round() as u8;
                        o[1] = (acc[1] / 4.0).round() as u8;
                        o[2] = (acc[2] / 4.0).round() as u8;
                        o[3] = 255;
                    }
                }
            });
        }
    });
}

#[test]
#[ignore = "renders the demo video (slow); run explicitly"]
fn film() {
    // SAFETY: single-threaded test setup before the app reads these.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var("MAGPIE_TODAY", "2026-09-29");
        std::env::set_var("MAGPIE_EXPORT_DIR", std::env::temp_dir().join("magpie-film-exports"));
    }
    crate::marks::enable();
    let out_path = std::env::var("MAGPIE_FILM_OUT")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/magpie-demo.mp4").to_string());
    let dir = std::env::temp_dir().join("magpie-film");
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = magpie_core::Store::open(&dir).expect("store");
    magpie_core::demo::generate(&mut store, magpie_core::Cur::USD, 24, 0).expect("demo");
    store.update_settings(|s| s.theme = "Midnight".into()).expect("theme");

    let ctx = Context::default();
    let mut app = App::with_context(&ctx, None, store);
    let mut renderer = WgpuTestRenderer::new();
    let (beats, cams, captions) = script();
    let mut cam_cache: Vec<Option<Pos2>> = Vec::new();

    let mut ffmpeg = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &format!("{OUT_W}x{OUT_H}"), "-r", &format!("{FPS}"), "-i", "-"])
        .args([
            "-c:v", "libx264", "-preset", "slow", "-crf", "16", "-pix_fmt", "yuv420p",
        ])
        .args(["-movflags", "+faststart", &out_path])
        .stdin(Stdio::piped())
        .spawn()
        .expect("ffmpeg");
    let mut sink = ffmpeg.stdin.take().expect("stdin");

    let mut pointer = Pointer {
        pos: pos2(W / 2.0, H / 2.0),
        from: pos2(W / 2.0, H / 2.0),
        to: pos2(W / 2.0, H / 2.0),
        start: 0.0,
        dur: 0.0,
        ripples: Vec::new(),
    };
    let mut typing: Vec<(f32, String)> = Vec::new();
    let mut pending: Vec<(f32, Event)> = Vec::new();
    let mut next_beat = 0;
    let frames = (DURATION * FPS) as usize;
    let mut out = vec![0u8; OUT_W * OUT_H * 4];
    let mut started = false;
    let clock = std::time::Instant::now();

    for frame in 0..frames {
        let t = frame as f32 / FPS;
        let mut events = Vec::new();
        while next_beat < beats.len() && beats[next_beat].t <= t {
            let beat = &beats[next_beat];
            match &beat.act {
                Act::Move(target, off, dur) => {
                    let to = target.rect(&ctx).map(|r| r.center() + *off).unwrap_or(pointer.pos);
                    pointer.from = pointer.pos;
                    pointer.to = to;
                    pointer.start = beat.t;
                    pointer.dur = *dur;
                }
                Act::Click => {
                    let at = pointer.pos;
                    pointer.ripples.push((at, t));
                    events.push(Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    });
                    pending.push((
                        t + 0.07,
                        Event::PointerButton {
                            pos: at,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        },
                    ));
                }
                Act::Type(text, cps) => {
                    // Human-ish rhythm: deterministic jitter, pauses after spaces.
                    let mut at = beat.t;
                    let mut seed = 7u32;
                    for ch in text.chars() {
                        typing.push((at, ch.to_string()));
                        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let jitter = ((seed >> 16) % 100) as f32 / 100.0 - 0.5;
                        at += (1.0 / cps) * (1.0 + 0.45 * jitter) + if ch == ' ' { 0.04 } else { 0.0 };
                    }
                }
                Act::Key(key, mods) => {
                    events.push(Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: *mods,
                    });
                    pending.push((
                        t + 0.05,
                        Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: *mods,
                        },
                    ));
                }
            }
            next_beat += 1;
        }
        typing.retain(|(at, ch)| {
            if *at <= t {
                events.push(Event::Text(ch.clone()));
                false
            } else {
                true
            }
        });
        pending.retain(|(at, ev)| {
            if *at <= t {
                events.push(ev.clone());
                false
            } else {
                true
            }
        });
        let new_pos = pointer.at(t);
        if (new_pos - pointer.pos).length() > 0.01 || frame == 0 {
            events.insert(0, Event::PointerMoved(new_pos));
        }
        pointer.pos = new_pos;

        // The app starts as the intro card begins to fade, so the dashboard's
        // entrance animations play on screen.
        let app_on = t >= 2.3;
        if app_on && !started {
            started = true;
            app.shown_at = t as f64;
        }
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
            time: Some(t as f64),
            events: if app_on { events } else { Vec::new() },
            ..Default::default()
        };
        raw.viewports.insert(
            ViewportId::ROOT,
            ViewportInfo {
                native_pixels_per_point: Some(PPP),
                ..Default::default()
            },
        );
        let (zoom, center) = camera(&cams, &ctx, t, &mut cam_cache);
        let mut output = ctx.run_ui(raw, |ui| {
            if app_on {
                app.frame(ui);
            } else {
                ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
            }
            overlay(ui.ctx(), t, zoom, center, &pointer, &captions);
        });
        renderer.handle_delta(&mut output.textures_delta);
        let img = renderer.render(&ctx, &output).expect("render");

        let view_size = vec2(W, H) / zoom * PPP;
        let view = Rect::from_center_size((center.to_vec2() * PPP).to_pos2(), view_size);
        resample(&img, view, &mut out);
        sink.write_all(&out).expect("write frame");
        // MAGPIE_FILM_STILLS=dir:t1,t2,... saves those moments as PNGs.
        if let Ok(spec) = std::env::var("MAGPIE_FILM_STILLS")
            && let Some((d, times)) = spec.split_once(':')
            && times
                .split(',')
                .filter_map(|x| x.parse::<f32>().ok())
                .any(|x| (x - t).abs() < 0.5 / FPS)
        {
            let _ = std::fs::create_dir_all(d);
            if let Some(img) = image::RgbaImage::from_raw(OUT_W as u32, OUT_H as u32, out.clone()) {
                let _ = img.save(format!("{d}/still-{t:05.2}.png"));
            }
        }
        if frame % 300 == 0 {
            eprintln!("film: {:>4.1}s / {DURATION}s  ({:.0?} elapsed)", t, clock.elapsed());
        }
    }
    drop(sink);
    assert!(ffmpeg.wait().expect("ffmpeg").success(), "ffmpeg failed");
    eprintln!("film: wrote {out_path} in {:.0?}", clock.elapsed());
}

/// `MAGPIE_STILL_DIR=<dir> cargo test -p magpie-finance --release stills -- --ignored`
/// renders a few UI states (an open category dropdown in a light and a dark
/// theme) to PNGs, for checking visual details without a display.
#[test]
#[ignore = "renders PNGs for visual review; run explicitly"]
fn stills() {
    // SAFETY: single-threaded test setup before the app reads these.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var("MAGPIE_TODAY", "2026-09-29");
    }
    crate::marks::enable();
    let out = std::env::var("MAGPIE_STILL_DIR")
        .unwrap_or_else(|_| std::env::temp_dir().join("magpie-stills").display().to_string());
    std::fs::create_dir_all(&out).expect("out dir");
    for theme in ["Paper", "Midnight"] {
        let dir = std::env::temp_dir().join(format!("magpie-stills-{theme}"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = magpie_core::Store::open(&dir).expect("store");
        magpie_core::demo::generate(&mut store, magpie_core::Cur::USD, 3, 0).expect("demo");
        store.update_settings(|s| s.theme = theme.into()).expect("theme");
        let ctx = Context::default();
        let mut app = App::with_context(&ctx, None, store);
        let mut renderer = WgpuTestRenderer::new();
        let ppp = 2.0;
        let mut t = 0.0f64;
        let mut step = |app: &mut App, events: Vec<Event>, t: f64| {
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                time: Some(t),
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
            let mut output = ctx.run_ui(raw, |ui| app.frame(ui));
            renderer.handle_delta(&mut output.textures_delta);
            (renderer.render(&ctx, &output).expect("render"), output)
        };
        let click = |pos: Pos2| {
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        let form = crate::forms::TxnForm::new(&app.store, app.today);
        app.open_modal(&ctx, crate::forms::Modal::Txn(form));
        for _ in 0..90 {
            t += 1.0 / 60.0;
            step(&mut app, Vec::new(), t);
        }
        let picker = crate::marks::get("picker:\"txn-cat\"").expect("category picker on screen");
        step(&mut app, click(picker.center()), t + 0.02);
        for _ in 0..40 {
            t += 1.0 / 60.0;
            step(&mut app, Vec::new(), t);
        }
        // Hover the row two below the selection to show both states.
        let hover = picker.center() + vec2(-40.0, picker.height() * 4.2);
        let mut last = None;
        for _ in 0..30 {
            t += 1.0 / 60.0;
            last = Some(step(&mut app, vec![Event::PointerMoved(hover)], t).0);
        }
        let img = last.expect("frame");
        let path = format!("{out}/dropdown-{}.png", theme.to_lowercase());
        img.save(&path).expect("save");
        eprintln!("stills: wrote {path}");

        // The sidebar's update prompt.
        step(
            &mut app,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            t + 0.02,
        );
        app.modal = None;
        app.updater.phase = crate::updater::Phase::Available(magpie_core::update::Release {
            version: "0.1.7".into(),
            page: String::new(),
            notes: String::new(),
            assets: Vec::new(),
        });
        let mut last = None;
        for _ in 0..40 {
            t += 1.0 / 60.0;
            last = Some(step(&mut app, vec![Event::PointerMoved(pos2(900.0, 400.0))], t).0);
        }
        let path = format!("{out}/update-{}.png", theme.to_lowercase());
        last.expect("frame").save(&path).expect("save");
        eprintln!("stills: wrote {path}");

        // First run: a brand-new, empty workspace shows onboarding.
        let dir = std::env::temp_dir().join(format!("magpie-stills-new-{theme}"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = magpie_core::Store::open(&dir).expect("store");
        store.update_settings(|s| s.theme = theme.into()).expect("theme");
        let ctx = Context::default();
        let mut app = App::with_context(&ctx, None, store);
        let mut renderer = WgpuTestRenderer::new();
        let mut img = None;
        for i in 0..90 {
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                time: Some(i as f64 / 60.0),
                ..Default::default()
            };
            raw.viewports.insert(
                ViewportId::ROOT,
                ViewportInfo {
                    native_pixels_per_point: Some(ppp),
                    ..Default::default()
                },
            );
            let mut output = ctx.run_ui(raw, |ui| app.frame(ui));
            renderer.handle_delta(&mut output.textures_delta);
            img = Some(renderer.render(&ctx, &output).expect("render"));
        }
        let path = format!("{out}/onboarding-{}.png", theme.to_lowercase());
        img.expect("frame").save(&path).expect("save");
        eprintln!("stills: wrote {path}");
    }
}
