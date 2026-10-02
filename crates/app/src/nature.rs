//! "A calm nest": the launch film. One continuous 2D take in a living,
//! storybook meadow — trees and grass moving in the wind, clouds, a river,
//! falling leaves. The Magpie icon pops out of a nest, grows into the real
//! app, and the camera dives in Screen Studio-style to show each feature
//! between big kinetic headlines. Themes become seasons: the whole world
//! turns autumn, night and dawn with the app. Nothing fades or cuts; things
//! rise out of mask lines, pop on springs, grow and push.
//!
//! ```text
//! cargo test --profile fast -p magpie-finance nature -- --ignored --nocapture
//! MAGPIE_FILM_STILLS=4.5,10.2 …   # just those moments, as PNGs
//! ```
//! Writes `~/Videos/magpie/magpie-launch.mp4` (override with MAGPIE_FILM_OUT).

use crate::app::Page;
use crate::film::{Act, Beat, Target, encoder_args, mark, paint_cursor};
use crate::icons::ph;
use crate::motion::{ease_in_out, ease_out, ease_out_back};
use crate::promo::{Hook, Pose, Stage, paint_bird, raw, run_app, soft_glow};
use crate::theme;
use egui::epaint::{Mesh, Shadow};
use egui::{Align2, Color32, CornerRadius, Id, Key, Modifiers, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use std::f32::consts::{PI, TAU};
use std::io::Write;
use std::process::{Command, Stdio};

const AW: f32 = 1600.0;
const AH: f32 = 900.0;
const OW: f32 = 1920.0;
const OH: f32 = 1080.0;
const FPS: f32 = 60.0;
const DURATION: f32 = 43.0;
const S0: Pos2 = pos2(OW / 2.0, OH / 2.0);

/// The app window's place in the world (where it sits at zoom 1).
const WIN_AT: Pos2 = pos2(960.0, 590.0);
const WIN_SCALE: f32 = 0.56;
/// The nest on the big tree's branch, where the icon lives.
const NEST: Pos2 = pos2(548.0, 652.0);
const ICON: f32 = 96.0;

// ================================================================ timeline

const T_POP: f32 = 4.5;
const T_MORPH: (f32, f32) = (6.5, 7.6);
const T_WIPE: (f32, f32) = (7.2, 7.8);
const T_UNWIPE: (f32, f32) = (35.6, 36.1);
const T_HOME: (f32, f32) = (36.0, 36.9);
const T_END: f32 = 39.2;

/// Seasons: (time, season) where 0 day, 1 autumn, 2 night, 3 dawn, 4 day.
const SEASONS: &[(f32, f32)] = &[(32.5, 0.0), (33.3, 1.0), (34.1, 2.0), (34.9, 3.0), (35.7, 4.0)];

struct CamKey {
    t: f32,
    /// World point to centre, or an app point (resolved through the window).
    world: Option<Pos2>,
    app: Option<(Target, Vec2)>,
    zoom: f32,
}

fn cw(t: f32, x: f32, y: f32, zoom: f32) -> CamKey {
    CamKey {
        t,
        world: Some(pos2(x, y)),
        app: None,
        zoom,
    }
}

fn ca(t: f32, target: Target, off: Vec2, zoom: f32) -> CamKey {
    CamKey {
        t,
        world: None,
        app: Some((target, off)),
        zoom,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// Centred, top of the frame (over the sky).
    Top,
    /// Centred at a height.
    Center(f32),
    /// Left-aligned at x, vertically centred.
    Right(f32),
}

struct Line {
    t0: f32,
    t1: f32,
    text: &'static str,
    sub: &'static str,
    size: f32,
    place: Place,
    icon: Option<&'static str>,
}

fn line(t0: f32, t1: f32, text: &'static str, size: f32, place: Place) -> Line {
    Line {
        t0,
        t1,
        text,
        sub: "",
        size,
        place,
        icon: None,
    }
}

fn headline(t0: f32, t1: f32, icon: &'static str, text: &'static str, sub: &'static str) -> Line {
    Line {
        t0,
        t1,
        text,
        sub,
        size: 74.0,
        place: Place::Top,
        icon: Some(icon),
    }
}

/// Words that replace each other in place: (time, word).
const FLIP_AT: f32 = 29.9;
const FLIPS: &[(f32, &str)] = &[
    (29.9, "Budgets."),
    (30.35, "Goals."),
    (30.8, "Bills."),
    (31.25, "Accounts."),
    (31.7, "160 currencies."),
    (32.15, "Receipts."),
];
const FLIP_END: f32 = 32.6;

struct Script {
    beats: Vec<Beat>,
    hooks: Vec<(f32, Hook)>,
    cams: Vec<CamKey>,
    lines: Vec<Line>,
}

fn script() -> Script {
    let b = |t: f32, act: Act| Beat { t, act };
    let mv = |t: f32, target: Target, off: Vec2, dur: f32| Beat {
        t,
        act: Act::Move(target, off, dur),
    };
    let hero = mark("dash:hero");
    let quick = Target::Widget(Id::new("quick-add"));
    let rep = mark("rep:cash");
    let modal = Target::Point(pos2(AW / 2.0, AH / 2.0));
    let beats = vec![
        b(10.0, Act::Move(Target::Point(pos2(1150.0, 330.0)), Vec2::ZERO, 0.01)),
        mv(10.9, hero.clone(), vec2(-220.0, 40.0), 0.7),
        mv(11.7, hero.clone(), vec2(280.0, 10.0), 1.1),
        mv(16.0, quick.clone(), vec2(-230.0, 0.0), 0.5),
        b(16.7, Act::Click),
        b(16.85, Act::Type("coffee 4.50 @Blue Bottle #treats", 22.0)),
        b(18.5, Act::Key(Key::Enter, Modifiers::NONE)),
        mv(22.5, mark("btn:Import"), Vec2::ZERO, 0.6),
        b(23.2, Act::Click),
        mv(27.3, rep.clone(), vec2(-300.0, 20.0), 0.5),
        mv(27.9, rep.clone(), vec2(40.0, 0.0), 0.8),
        mv(28.6, rep.clone(), vec2(260.0, 10.0), 0.6),
    ];
    let hooks = vec![
        (13.0, Hook::Go(Page::Ledger)),
        (19.0, Hook::Import),
        (26.6, Hook::Go(Page::Reports)),
        (30.0, Hook::Go(Page::Budgets)),
        (30.45, Hook::Go(Page::Goals)),
        (30.9, Hook::Go(Page::Recurring)),
        (31.35, Hook::Go(Page::Accounts)),
        (31.8, Hook::Go(Page::Ledger)),
        (32.25, Hook::Go(Page::Dashboard)),
        (33.3, Hook::Theme("Flexoki")),
        (34.1, Hook::Theme("Midnight")),
        (34.9, Hook::Theme("Rosé Pine Dawn")),
        (35.7, Hook::Theme("Paper")),
    ];
    let cams = vec![
        cw(0.0, 960.0, 300.0, 1.32),
        cw(0.6, 960.0, 300.0, 1.32),
        cw(4.2, 960.0, 540.0, 1.0),
        cw(5.3, 790.0, 600.0, 1.32),
        cw(6.5, 790.0, 600.0, 1.32),
        cw(7.6, 960.0, 540.0, 1.0),
        cw(10.3, 960.0, 540.0, 1.0),
        ca(11.0, hero.clone(), vec2(0.0, 10.0), 2.7),
        ca(12.8, hero, vec2(0.0, 10.0), 2.7),
        cw(13.5, 960.0, 540.0, 1.0),
        cw(15.8, 960.0, 540.0, 1.0),
        ca(16.5, quick.clone(), vec2(-120.0, 40.0), 2.9),
        ca(18.9, quick, vec2(-120.0, 40.0), 2.9),
        cw(19.6, 960.0, 540.0, 1.0),
        cw(21.7, 960.0, 540.0, 1.0),
        ca(22.4, modal.clone(), Vec2::ZERO, 2.05),
        ca(23.7, modal, Vec2::ZERO, 2.05),
        cw(24.4, 960.0, 540.0, 1.0),
        cw(26.6, 960.0, 540.0, 1.0),
        ca(27.3, rep.clone(), Vec2::ZERO, 2.4),
        ca(29.0, rep, Vec2::ZERO, 2.4),
        cw(29.7, 960.0, 540.0, 1.0),
        cw(36.0, 960.0, 540.0, 1.0),
        cw(36.9, 790.0, 600.0, 1.32),
        cw(38.6, 790.0, 600.0, 1.32),
        cw(39.6, 960.0, 150.0, 1.3),
        cw(DURATION, 960.0, 140.0, 1.32),
    ];
    let lines = vec![
        line(0.35, 2.3, "Life moves *fast*.", 104.0, Place::Center(330.0)),
        line(2.45, 4.35, "Your money doesn't *have to*.", 92.0, Place::Center(330.0)),
        Line {
            t0: 4.8,
            t1: 6.4,
            text: "Meet *Magpie*.",
            sub: "A calm nest for your money.",
            size: 100.0,
            place: Place::Right(800.0),
            icon: None,
        },
        headline(
            7.9,
            10.5,
            ph::SQUARES_FOUR,
            "Everything you have, *at a glance*.",
            "Net worth, cash flow and budgets on one calm dashboard.",
        ),
        headline(
            13.4,
            15.9,
            ph::SPARKLE,
            "Just type it. Magpie *gets it*.",
            "Amounts, payees, tags and dates, understood as you write.",
        ),
        headline(
            19.4,
            21.9,
            ph::UPLOAD_SIMPLE,
            "Bring your bank *along*.",
            "Any statement: CSV, Excel or OFX. The columns map themselves.",
        ),
        headline(
            24.3,
            26.8,
            ph::CHART_BAR,
            "Watch your savings *grow*.",
            "Clear reports for this month, this year, or any range you like.",
        ),
        Line {
            t0: 29.5,
            t1: FLIP_END,
            text: "And everything *else*.",
            sub: "",
            size: 74.0,
            place: Place::Top,
            icon: None,
        },
        headline(
            32.6,
            35.4,
            ph::PALETTE,
            "A look for *every season*.",
            "14 themes and 9 fonts. Make it feel like yours.",
        ),
        Line {
            t0: 36.9,
            t1: 38.9,
            text: "Your data stays\nin *your nest*.",
            sub: "No account. No cloud. No tracking. Ever.",
            size: 84.0,
            place: Place::Right(800.0),
            icon: None,
        },
    ];
    Script {
        beats,
        hooks,
        cams,
        lines,
    }
}

// ================================================================ camera

#[derive(Clone, Copy)]
struct Cam {
    center: Pos2,
    zoom: f32,
}

impl Cam {
    fn apply(&self, w: Pos2) -> Pos2 {
        S0 + (w - self.center) * self.zoom
    }
    /// The camera as seen by a layer at `depth` (0 = sky, 1 = foreground).
    fn layer(&self, depth: f32) -> Cam {
        Cam {
            center: S0.lerp(self.center, depth),
            zoom: 1.0 + (self.zoom - 1.0) * depth,
        }
    }
}

/// Screen Studio-style: the next target glides in a straight line to the
/// centre while zoom changes geometrically, so nothing swings or bounces.
fn cam_at(keys: &[CamKey], world: &[Pos2], t: f32) -> Cam {
    let i = keys.iter().rposition(|k| k.t <= t).unwrap_or(0);
    let j = (i + 1).min(keys.len() - 1);
    let a = Cam {
        center: world[i],
        zoom: keys[i].zoom,
    };
    let b = Cam {
        center: world[j],
        zoom: keys[j].zoom,
    };
    if j == i || keys[j].t <= keys[i].t {
        return a;
    }
    let k = ease_in_out(((t - keys[i].t) / (keys[j].t - keys[i].t)).clamp(0.0, 1.0));
    let start = a.apply(b.center);
    let s = start.lerp(S0, k);
    let zoom = a.zoom * (b.zoom / a.zoom).powf(k);
    Cam {
        center: b.center - (s - S0) / zoom,
        zoom,
    }
}

fn window_world() -> Rect {
    Rect::from_center_size(WIN_AT, vec2(AW, AH) * WIN_SCALE)
}

fn app_to_world(p: Pos2) -> Pos2 {
    window_world().min + p.to_vec2() * WIN_SCALE
}

// ================================================================ seasons

#[derive(Clone, Copy)]
struct Palette {
    sky: [Color32; 3],
    sun: Color32,
    cloud: Color32,
    cloud_shade: Color32,
    hill_far: Color32,
    hill_mid: Color32,
    hill_near: Color32,
    meadow: [Color32; 2],
    grass: [Color32; 2],
    canopy: [Color32; 4],
    trunk: Color32,
    water: [Color32; 2],
    leaf: Color32,
    ink: Color32,
    ink_soft: Color32,
    accent: Color32,
    night: f32,
}

const fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

const DAY: Palette = Palette {
    sky: [rgb(0x86BEE3), rgb(0xBCDCF0), rgb(0xEEF6F1)],
    sun: rgb(0xFFF1CF),
    cloud: rgb(0xFFFFFF),
    cloud_shade: rgb(0xD7E6F0),
    hill_far: rgb(0xA9CCB0),
    hill_mid: rgb(0x8FBE8A),
    hill_near: rgb(0x77AD68),
    meadow: [rgb(0x8EC06A), rgb(0x6A9E50)],
    grass: [rgb(0x4C8A3E), rgb(0x9BCB6E)],
    canopy: [rgb(0x2A5634), rgb(0x3B7242), rgb(0x56924E), rgb(0x8FC26C)],
    trunk: rgb(0x6E5140),
    water: [rgb(0x7DB6DD), rgb(0xE6F4FA)],
    leaf: rgb(0x7DB55E),
    ink: rgb(0x1C2A22),
    ink_soft: rgb(0x48574D),
    accent: rgb(0xD5602A),
    night: 0.0,
};

const AUTUMN: Palette = Palette {
    sky: [rgb(0xEFB98C), rgb(0xF6D6B0), rgb(0xFBEBD5)],
    sun: rgb(0xFFE0B0),
    cloud: rgb(0xFFF6EC),
    cloud_shade: rgb(0xF0D2B6),
    hill_far: rgb(0xD5B28A),
    hill_mid: rgb(0xC69A62),
    hill_near: rgb(0xB0813F),
    meadow: [rgb(0xC9A15A), rgb(0xA77B3A)],
    grass: [rgb(0x8E6A2E), rgb(0xD9B060)],
    canopy: [rgb(0x7D2F17), rgb(0xA94A22), rgb(0xD47A2C), rgb(0xF0B347)],
    trunk: rgb(0x5C3F2E),
    water: [rgb(0xD5A982), rgb(0xFBE7CF)],
    leaf: rgb(0xE07B2C),
    ink: rgb(0x2D2117),
    ink_soft: rgb(0x5E4A3A),
    accent: rgb(0xB5441B),
    night: 0.0,
};

const NIGHT: Palette = Palette {
    sky: [rgb(0x070D22), rgb(0x142047), rgb(0x2A3866)],
    sun: rgb(0xEDEBDF),
    cloud: rgb(0x3A4670),
    cloud_shade: rgb(0x26315A),
    hill_far: rgb(0x1A2945),
    hill_mid: rgb(0x16283A),
    hill_near: rgb(0x133026),
    meadow: [rgb(0x183A2E), rgb(0x0F2A22)],
    grass: [rgb(0x0E2A20), rgb(0x2E5A43)],
    canopy: [rgb(0x0A1D18), rgb(0x123227), rgb(0x1C4734), rgb(0x2E6448)],
    trunk: rgb(0x2B2420),
    water: [rgb(0x22406A), rgb(0x8FA9D6)],
    leaf: rgb(0x3D7556),
    ink: rgb(0xF4F2EA),
    ink_soft: rgb(0xB9C0CF),
    accent: rgb(0xF2A06B),
    night: 1.0,
};

const DAWN: Palette = Palette {
    sky: [rgb(0xBFB0E2), rgb(0xF0C3D2), rgb(0xFCE4DA)],
    sun: rgb(0xFFE3D8),
    cloud: rgb(0xFFF3F4),
    cloud_shade: rgb(0xEACFDF),
    hill_far: rgb(0xBBA9CF),
    hill_mid: rgb(0xA996BF),
    hill_near: rgb(0x8E9E8E),
    meadow: [rgb(0x9DB794), rgb(0x7C9878)],
    grass: [rgb(0x5E7D61), rgb(0xA9C5A0)],
    canopy: [rgb(0x47506E), rgb(0x5F6A8E), rgb(0x8C93B8), rgb(0xD0B9DA)],
    trunk: rgb(0x5E4A52),
    water: [rgb(0xC3B2DE), rgb(0xFBEFF4)],
    leaf: rgb(0xD69BB6),
    ink: rgb(0x33283B),
    ink_soft: rgb(0x655769),
    accent: rgb(0xC1567B),
    night: 0.0,
};

fn mix(a: Color32, b: Color32, k: f32) -> Color32 {
    a.lerp_to_gamma(b, k)
}

fn mix_palette(a: &Palette, b: &Palette, k: f32) -> Palette {
    let m3 = |x: [Color32; 3], y: [Color32; 3]| [mix(x[0], y[0], k), mix(x[1], y[1], k), mix(x[2], y[2], k)];
    let m2 = |x: [Color32; 2], y: [Color32; 2]| [mix(x[0], y[0], k), mix(x[1], y[1], k)];
    Palette {
        sky: m3(a.sky, b.sky),
        sun: mix(a.sun, b.sun, k),
        cloud: mix(a.cloud, b.cloud, k),
        cloud_shade: mix(a.cloud_shade, b.cloud_shade, k),
        hill_far: mix(a.hill_far, b.hill_far, k),
        hill_mid: mix(a.hill_mid, b.hill_mid, k),
        hill_near: mix(a.hill_near, b.hill_near, k),
        meadow: m2(a.meadow, b.meadow),
        grass: m2(a.grass, b.grass),
        canopy: [
            mix(a.canopy[0], b.canopy[0], k),
            mix(a.canopy[1], b.canopy[1], k),
            mix(a.canopy[2], b.canopy[2], k),
            mix(a.canopy[3], b.canopy[3], k),
        ],
        trunk: mix(a.trunk, b.trunk, k),
        water: m2(a.water, b.water),
        leaf: mix(a.leaf, b.leaf, k),
        ink: mix(a.ink, b.ink, k),
        ink_soft: mix(a.ink_soft, b.ink_soft, k),
        accent: mix(a.accent, b.accent, k),
        night: a.night + (b.night - a.night) * k,
    }
}

fn palette_at(t: f32) -> Palette {
    let order = [DAY, AUTUMN, NIGHT, DAWN, DAY];
    let mut s = 0.0;
    for w in SEASONS.windows(2) {
        let ((t0, s0), (t1, s1)) = (w[0], w[1]);
        if t >= t1 {
            s = s1;
        } else if t > t0 {
            s = s0 + (s1 - s0) * ease_in_out((t - t0) / (t1 - t0));
            break;
        }
    }
    if t >= SEASONS[SEASONS.len() - 1].0 {
        s = 4.0;
    }
    let i = (s.floor() as usize).min(3);
    mix_palette(&order[i], &order[i + 1], s - i as f32)
}

// ================================================================ the meadow

/// Deterministic noise in 0..1.
fn rnd(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// The breeze: slow swells with gusts travelling left to right.
fn wind(t: f32, x: f32) -> f32 {
    0.6 * (t * 1.1 - x * 0.0035).sin() + 0.3 * (t * 2.3 - x * 0.009 + 1.3).sin() + 0.12 * (t * 4.1 - x * 0.02).sin()
}

fn sky(p: &egui::Painter, pal: &Palette) {
    let mut m = Mesh::default();
    let stops = [(0.0, pal.sky[0]), (0.55, pal.sky[1]), (1.0, pal.sky[2])];
    for (y, c) in stops {
        m.colored_vertex(pos2(0.0, y * OH), c);
        m.colored_vertex(pos2(OW, y * OH), c);
    }
    for k in 0..2u32 {
        let v = k * 2;
        m.add_triangle(v, v + 1, v + 3);
        m.add_triangle(v, v + 3, v + 2);
    }
    p.add(Shape::mesh(m));
}

fn celestial(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(0.08);
    if pal.night > 0.0 {
        for i in 0..140 {
            let w = pos2(rnd(i, 1) * 2400.0 - 240.0, rnd(i, 2) * 620.0 - 60.0);
            let tw = 0.55 + 0.45 * (t * (1.0 + rnd(i, 3) * 2.0) + i as f32).sin();
            let r = 0.8 + rnd(i, 4) * 1.6;
            p.circle_filled(c.apply(w), r, Color32::from_white_alpha((230.0 * pal.night * tw) as u8));
        }
    }
    let sun = c.apply(pos2(1480.0, 230.0));
    soft_glow(
        p,
        sun,
        300.0,
        Color32::from_rgba_unmultiplied(pal.sun.r(), pal.sun.g(), pal.sun.b(), 110),
    );
    p.circle_filled(sun, 62.0 * c.zoom, pal.sun);
}

fn clouds(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(0.15);
    let shapes: [(f32, f32, f32); 6] = [
        (180.0, 150.0, 1.0),
        (700.0, 90.0, 0.7),
        (1150.0, 190.0, 1.15),
        (1700.0, 110.0, 0.8),
        (2150.0, 230.0, 0.9),
        (-300.0, 260.0, 0.75),
    ];
    for (n, (x0, y, s)) in shapes.into_iter().enumerate() {
        let x = (x0 + t * 9.0 * (0.7 + s * 0.3)).rem_euclid(2700.0) - 400.0;
        let puffs = [
            (-120.0, 22.0, 42.0),
            (-75.0, 2.0, 56.0),
            (-25.0, -22.0, 70.0),
            (35.0, -30.0, 76.0),
            (95.0, -6.0, 60.0),
            (140.0, 18.0, 44.0),
            (10.0, 22.0, 64.0),
        ];
        for (dx, dy, r) in puffs {
            let at = c.apply(pos2(x + dx * s, y + dy * s + 12.0 * s));
            p.circle_filled(at, r * s * c.zoom, pal.cloud_shade.gamma_multiply(0.9));
        }
        for (i, (dx, dy, r)) in puffs.into_iter().enumerate() {
            let bob = (t * 0.4 + n as f32 + i as f32).sin() * 2.0;
            let at = c.apply(pos2(x + dx * s, y + dy * s + bob));
            p.circle_filled(at, r * s * c.zoom, pal.cloud);
        }
    }
}

/// A filled band from a curve y(x) down past the bottom of the frame.
fn band(p: &egui::Painter, cam: &Cam, depth: f32, y: impl Fn(f32) -> f32, color: Color32) {
    let c = cam.layer(depth);
    let mut m = Mesh::default();
    let n = 64;
    for i in 0..=n {
        let x = -400.0 + 2720.0 * i as f32 / n as f32;
        m.colored_vertex(c.apply(pos2(x, y(x))), color);
        m.colored_vertex(c.apply(pos2(x, 1600.0)), color);
    }
    for i in 0..n as u32 {
        let v = i * 2;
        m.add_triangle(v, v + 2, v + 3);
        m.add_triangle(v, v + 3, v + 1);
    }
    p.add(Shape::mesh(m));
}

fn hill_far(x: f32) -> f32 {
    640.0 - 60.0 * (x * 0.0021 + 0.6).sin() - 30.0 * (x * 0.0057).sin()
}
fn hill_mid(x: f32) -> f32 {
    720.0 - 48.0 * (x * 0.0028 + 2.1).sin() - 18.0 * (x * 0.0071).sin()
}
fn hill_near(x: f32) -> f32 {
    790.0 - 30.0 * (x * 0.0032 + 4.0).sin()
}
fn meadow_y(x: f32) -> f32 {
    880.0 - 22.0 * (x * 0.0024 + 1.0).sin()
}

fn landscape(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    band(p, cam, 0.25, hill_far, pal.hill_far);
    // A soft tree line along the far hills.
    let c = cam.layer(0.3);
    for i in 0..90 {
        let x = -300.0 + i as f32 * 28.0 + rnd(i, 9) * 14.0;
        let r = 12.0 + rnd(i, 10) * 16.0;
        let y = hill_far(x) + 8.0 - r * 0.6;
        let sway = wind(t, x) * 1.5;
        p.circle_filled(
            c.apply(pos2(x + sway, y)),
            r * c.zoom,
            mix(pal.hill_far, pal.canopy[1], 0.35),
        );
    }
    band(p, cam, 0.35, hill_mid, pal.hill_mid);
    for i in 0..40 {
        let x = -300.0 + i as f32 * 64.0 + rnd(i, 11) * 30.0;
        let r = 18.0 + rnd(i, 12) * 22.0;
        let y = hill_mid(x) + 6.0 - r * 0.55;
        let c = cam.layer(0.4);
        let sway = wind(t, x) * 2.5;
        p.circle_filled(
            c.apply(pos2(x + sway, y)),
            r * c.zoom,
            mix(pal.hill_mid, pal.canopy[1], 0.55),
        );
        p.circle_filled(
            c.apply(pos2(x + sway + r * 0.25, y - r * 0.3)),
            r * 0.55 * c.zoom,
            mix(pal.hill_mid, pal.canopy[2], 0.5),
        );
    }
    band(p, cam, 0.5, hill_near, pal.hill_near);
    river(p, cam, pal, t);
    band(p, cam, 0.7, meadow_y, pal.meadow[0]);
}

fn river(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(0.55);
    let center = |x: f32| 818.0 + 14.0 * (x * 0.004 + 0.5).sin();
    let width = |x: f32| 16.0 + 44.0 * ((x - 600.0) / 1500.0).clamp(0.0, 1.0);
    let mut m = Mesh::default();
    let n = 48;
    for i in 0..=n {
        let x = 560.0 + 1800.0 * i as f32 / n as f32;
        let (y, w) = (center(x), width(x));
        m.colored_vertex(c.apply(pos2(x, y - w * 0.5)), pal.water[0]);
        m.colored_vertex(c.apply(pos2(x, y + w * 0.5)), mix(pal.water[0], pal.water[1], 0.25));
    }
    for i in 0..n as u32 {
        let v = i * 2;
        m.add_triangle(v, v + 2, v + 3);
        m.add_triangle(v, v + 3, v + 1);
    }
    p.add(Shape::mesh(m));
    // Glints drifting downstream.
    for i in 0..26 {
        let x = 600.0 + (rnd(i, 21) * 1700.0 + t * 26.0).rem_euclid(1700.0);
        let y = center(x) + (rnd(i, 22) - 0.5) * width(x) * 0.6;
        let len = 8.0 + rnd(i, 23) * 18.0;
        let a = (0.5 + 0.5 * (t * 2.0 + i as f32).sin()) * 0.8;
        p.line_segment(
            [c.apply(pos2(x, y)), c.apply(pos2(x + len, y))],
            Stroke::new(2.0 * c.zoom, pal.water[1].gamma_multiply(a)),
        );
    }
}

struct Tree {
    base: Pos2,
    top: Pos2,
    width: f32,
    canopy: Pos2,
    rx: f32,
    ry: f32,
    blobs: u32,
    seed: u32,
    depth: f32,
    birch: bool,
}

fn tree(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32, tr: &Tree) {
    let c = cam.layer(tr.depth);
    let h = tr.base.y - tr.canopy.y;
    let sway_at = |y: f32| wind(t, tr.base.x) * 10.0 * ((tr.base.y - y) / h).clamp(0.0, 1.4);
    let trunk_col = if tr.birch {
        mix(pal.cloud, pal.trunk, 0.18)
    } else {
        pal.trunk
    };
    // Trunk: a tapered, slightly leaning shape.
    let top = tr.top + vec2(sway_at(tr.top.y), 0.0);
    let pts = vec![
        c.apply(tr.base + vec2(-tr.width * 0.5, 0.0)),
        c.apply(tr.base + vec2(tr.width * 0.5, 0.0)),
        c.apply(top + vec2(tr.width * 0.22, 0.0)),
        c.apply(top + vec2(-tr.width * 0.22, 0.0)),
    ];
    p.add(Shape::convex_polygon(pts, trunk_col, Stroke::NONE));
    if tr.birch {
        for i in 0..8 {
            let y = tr.base.y - (i as f32 + 0.5) * (tr.base.y - tr.top.y) / 8.0;
            let x = tr.base.x + (top.x - tr.base.x) * (tr.base.y - y) / (tr.base.y - tr.top.y);
            let w = tr.width * (0.5 - 0.28 * (tr.base.y - y) / (tr.base.y - tr.top.y)) * (0.3 + rnd(i, tr.seed) * 0.5);
            p.line_segment(
                [c.apply(pos2(x - w, y)), c.apply(pos2(x + w * 0.4, y + 2.0))],
                Stroke::new(3.0 * c.zoom, mix(pal.trunk, pal.ink, 0.3)),
            );
        }
    }
    // Branches reaching into the canopy.
    for i in 0..3 {
        let from_y = tr.base.y - h * (0.55 + i as f32 * 0.12);
        let from = pos2(tr.base.x + (top.x - tr.base.x) * 0.6, from_y);
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let to = tr.canopy
            + vec2(
                dir * tr.rx * (0.45 + rnd(i, tr.seed + 3) * 0.2),
                -tr.ry * 0.2 * i as f32,
            );
        let to = to + vec2(sway_at(to.y), 0.0);
        let w = tr.width * 0.18;
        p.add(Shape::convex_polygon(
            vec![
                c.apply(from + vec2(0.0, -w)),
                c.apply(from + vec2(0.0, w)),
                c.apply(to + vec2(0.0, w * 0.3)),
                c.apply(to + vec2(0.0, -w * 0.3)),
            ],
            trunk_col,
            Stroke::NONE,
        ));
    }
    // Canopy: a dark mass, then ever smaller, lighter clumps towards the
    // sunlit upper right, each swaying with the wind and fluttering.
    let layers: [(u32, f32, f32, f32); 4] = [
        (14, 0.30, 0.0, 0.0),
        (44, 0.17, 0.15, 0.1),
        (46, 0.12, 0.35, 0.22),
        (34, 0.07, 0.5, 0.34),
    ];
    for (li, &(n, size, bias_x, bias_y)) in layers.iter().enumerate() {
        let s = tr.seed + li as u32 * 101;
        for i in 0..n {
            let ang = rnd(i, s) * TAU;
            let rad = rnd(i, s + 1).sqrt() * (1.0 - li as f32 * 0.12);
            let off = vec2(
                ang.cos() * tr.rx * rad + bias_x * tr.rx * 0.5,
                ang.sin() * tr.ry * rad - bias_y * tr.ry,
            );
            let at = tr.canopy + off;
            let flutter = vec2((t * 2.6 + i as f32 * 1.7).sin(), (t * 2.1 + i as f32).cos()) * (1.0 + li as f32 * 0.4);
            let pos = at + vec2(sway_at(at.y), 0.0) + flutter;
            let r = tr.rx * size * (0.7 + rnd(i, s + 2) * 0.6);
            let shade = (0.5 - off.y / tr.ry * 0.4 + off.x / tr.rx * 0.2).clamp(0.0, 1.0);
            let col = match li {
                0 => pal.canopy[0],
                1 => mix(pal.canopy[0], pal.canopy[1], 0.4 + shade * 0.6),
                2 => mix(pal.canopy[1], pal.canopy[2], shade),
                _ => mix(pal.canopy[2], pal.canopy[3], shade),
            };
            p.circle_filled(c.apply(pos), r * c.zoom, col);
        }
    }
    // Leaves catching the light: tiny specks that shimmer in the wind.
    for i in 0..tr.blobs * 3 {
        let ang = rnd(i, tr.seed + 7) * TAU;
        let rad = rnd(i, tr.seed + 8).sqrt() * 0.9;
        let at = tr.canopy + vec2(ang.cos() * tr.rx * rad, ang.sin() * tr.ry * rad - tr.ry * 0.1);
        let lit = (t * 5.0 + i as f32 * 0.9 + wind(t, at.x) * 2.0).sin();
        if lit < 0.2 {
            continue;
        }
        let pos = at + vec2(sway_at(at.y), 0.0);
        let col = mix(pal.canopy[2], pal.canopy[3], lit).gamma_multiply(0.9);
        p.circle_filled(c.apply(pos), (2.0 + rnd(i, tr.seed + 9) * 2.5) * c.zoom, col);
    }
}

fn trees() -> [Tree; 4] {
    [
        // Distant, on the hills.
        Tree {
            base: pos2(1540.0, 772.0),
            top: pos2(1546.0, 650.0),
            width: 12.0,
            canopy: pos2(1548.0, 628.0),
            rx: 64.0,
            ry: 70.0,
            blobs: 14,
            seed: 31,
            depth: 0.55,
            birch: false,
        },
        Tree {
            base: pos2(1420.0, 790.0),
            top: pos2(1418.0, 700.0),
            width: 9.0,
            canopy: pos2(1416.0, 684.0),
            rx: 44.0,
            ry: 50.0,
            blobs: 10,
            seed: 47,
            depth: 0.55,
            birch: false,
        },
        // The big tree with the nest.
        Tree {
            base: pos2(250.0, 1010.0),
            top: pos2(300.0, 560.0),
            width: 46.0,
            canopy: pos2(300.0, 430.0),
            rx: 290.0,
            ry: 220.0,
            blobs: 40,
            seed: 7,
            depth: 0.9,
            birch: false,
        },
        // A birch on the right.
        Tree {
            base: pos2(1760.0, 1000.0),
            top: pos2(1740.0, 560.0),
            width: 30.0,
            canopy: pos2(1735.0, 470.0),
            rx: 190.0,
            ry: 200.0,
            blobs: 26,
            seed: 13,
            depth: 0.9,
            birch: true,
        },
    ]
}

/// The nest on its branch.
fn nest(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(0.9);
    let sway = wind(t, 250.0) * 7.0 * 0.8;
    let at = NEST + vec2(sway, 0.0);
    let branch_from = pos2(300.0 + sway * 0.6, 690.0);
    p.add(Shape::convex_polygon(
        vec![
            c.apply(branch_from + vec2(0.0, -8.0)),
            c.apply(branch_from + vec2(0.0, 8.0)),
            c.apply(at + vec2(70.0, 16.0)),
            c.apply(at + vec2(70.0, 10.0)),
        ],
        pal.trunk,
        Stroke::NONE,
    ));
    let bowl: Vec<Pos2> = (0..=16)
        .map(|i| {
            let a = PI * i as f32 / 16.0;
            c.apply(at + vec2(-a.cos() * 58.0, a.sin() * 26.0))
        })
        .collect();
    p.add(Shape::convex_polygon(
        bowl,
        mix(pal.trunk, pal.meadow[1], 0.25),
        Stroke::NONE,
    ));
    for i in 0..9 {
        let y = at.y + 4.0 + i as f32 * 2.4;
        let w = 54.0 - i as f32 * 4.5;
        p.line_segment(
            [
                c.apply(pos2(at.x - w, y + (i % 2) as f32 * 2.0)),
                c.apply(pos2(at.x + w, y)),
            ],
            Stroke::new(2.0 * c.zoom, mix(pal.trunk, pal.cloud, 0.25)),
        );
    }
}

fn foreground(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(1.0);
    // Ground.
    let mut m = Mesh::default();
    for (y, col) in [(900.0, pal.meadow[0]), (1200.0, pal.meadow[1])] {
        m.colored_vertex(c.apply(pos2(-300.0, y)), col);
        m.colored_vertex(c.apply(pos2(2220.0, y)), col);
    }
    m.add_triangle(0, 1, 3);
    m.add_triangle(0, 3, 2);
    p.add(Shape::mesh(m));
    // Flowers and mushrooms in the meadow.
    for i in 0..70 {
        let x = rnd(i, 41) * 2100.0 - 90.0;
        let y = 900.0 + rnd(i, 42) * 110.0;
        let sway = wind(t, x) * 3.0;
        let at = pos2(x + sway, y);
        if i % 9 == 0 {
            // Toadstool: stem, red cap, white spots.
            let s = 0.8 + rnd(i, 43) * 0.6;
            p.add(Shape::convex_polygon(
                vec![
                    c.apply(at + vec2(-4.0, 0.0) * s),
                    c.apply(at + vec2(4.0, 0.0) * s),
                    c.apply(at + vec2(3.0, -16.0) * s),
                    c.apply(at + vec2(-3.0, -16.0) * s),
                ],
                mix(pal.cloud, pal.trunk, 0.15),
                Stroke::NONE,
            ));
            let cap: Vec<Pos2> = (0..=12)
                .map(|k| {
                    let a = PI * k as f32 / 12.0;
                    c.apply(at + vec2(-a.cos() * 13.0, -16.0 - a.sin() * 11.0) * s)
                })
                .collect();
            p.add(Shape::convex_polygon(
                cap,
                mix(rgb(0xD2483A), pal.canopy[0], pal.night * 0.6),
                Stroke::NONE,
            ));
            for (dx, dy) in [(-6.0, -21.0), (2.0, -25.0), (7.0, -19.0)] {
                p.circle_filled(c.apply(at + vec2(dx, dy) * s), 1.8 * s * c.zoom, pal.cloud);
            }
        } else {
            let col = if i % 3 == 0 {
                rgb(0xF5D04E)
            } else {
                mix(pal.cloud, pal.sun, 0.2)
            };
            for k in 0..5 {
                let a = TAU * k as f32 / 5.0 + i as f32;
                p.circle_filled(c.apply(at + vec2(a.cos(), a.sin()) * 3.6), 2.6 * c.zoom, col);
            }
            p.circle_filled(c.apply(at), 1.8 * c.zoom, rgb(0xE9A23B));
        }
    }
    // Grass, two rows of blades bending with the wind.
    for row in 0..2u32 {
        let n = 520;
        for i in 0..n {
            let x = -120.0 + i as f32 * (2160.0 / n as f32) + rnd(i, 50 + row) * 4.0;
            let base_y = if row == 0 { 940.0 } else { 1030.0 } + rnd(i, 52 + row) * 40.0;
            let h = (if row == 0 { 26.0 } else { 46.0 }) * (0.6 + rnd(i, 54 + row) * 0.8);
            let w = 3.0 + rnd(i, 56 + row) * 3.0;
            let bend = wind(t, x) * h * 0.38 + (t * 3.0 + i as f32).sin() * 1.2;
            let col = mix(pal.grass[0], pal.grass[1], rnd(i, 58 + row) * 0.8 + row as f32 * 0.1);
            p.add(Shape::convex_polygon(
                vec![
                    c.apply(pos2(x - w * 0.5, base_y)),
                    c.apply(pos2(x + w * 0.5, base_y)),
                    c.apply(pos2(x + bend, base_y - h)),
                ],
                col,
                Stroke::NONE,
            ));
        }
    }
}

fn falling_leaves(p: &egui::Painter, cam: &Cam, pal: &Palette, t: f32) {
    let c = cam.layer(0.95);
    for i in 0..22 {
        let speed = 26.0 + rnd(i, 61) * 22.0;
        let y = (rnd(i, 62) * 700.0 + t * speed).rem_euclid(760.0) + 300.0;
        let x0 = rnd(i, 63) * 2200.0 - 140.0;
        let x = x0 + t * 22.0 + (t * 0.9 + i as f32).sin() * 40.0 + wind(t, x0) * 30.0;
        let x = (x + 200.0).rem_euclid(2300.0) - 200.0;
        let rot = t * (1.0 + rnd(i, 64)) + i as f32;
        let s = 5.0 + rnd(i, 65) * 4.0;
        let pts: Vec<Pos2> = (0..8)
            .map(|k| {
                let a = TAU * k as f32 / 8.0;
                let v = vec2(a.cos() * s, a.sin() * s * 0.45 * rot.cos().abs().max(0.3));
                let (sn, cs) = rot.sin_cos();
                c.apply(pos2(x, y) + vec2(v.x * cs - v.y * sn, v.x * sn + v.y * cs))
            })
            .collect();
        p.add(Shape::convex_polygon(pts, pal.leaf, Stroke::NONE));
    }
}

// ================================================================ the icon and window

/// The app icon (orange squircle with the bird), drawn at any size.
fn icon(p: &egui::Painter, rect: Rect, bird: f32) {
    if rect.width() < 1.0 {
        return;
    }
    p.add(
        Shadow {
            offset: [0, (rect.height() * 0.08).min(60.0) as i8],
            blur: (rect.height() * 0.25).min(255.0) as u8,
            spread: 0,
            color: Color32::from_black_alpha(60),
        }
        .as_shape(
            rect,
            CornerRadius::same((rect.width().min(rect.height()) * 0.22).min(250.0) as u8),
        ),
    );
    let radius = (rect.width().min(rect.height()) * 0.22).min(rect.height() * 0.5);
    crate::widgets::rounded_gradient(p, rect, radius, rgb(0xE4814F), rgb(0xA84D22));
    if bird > 0.0 {
        let size = rect.width().min(rect.height()) * 0.59 * bird;
        p.text(
            rect.center() + vec2(0.0, size * 0.01),
            Align2::CENTER_CENTER,
            ph::BIRD,
            theme::regular(size),
            Color32::WHITE,
        );
    }
}

/// The app window: a rounded textured mesh, optionally revealed only
/// below `reveal_top` (screen y) for the push-in.
fn window(p: &egui::Painter, pose: &Pose, tex: egui::TextureId, reveal_top: Option<f32>) {
    let painter = match reveal_top {
        Some(y) => p.with_clip_rect(Rect::from_min_max(pos2(-10_000.0, y), pos2(10_000.0, 10_000.0))),
        None => p.clone(),
    };
    crate::promo::paint_window(&painter, pose, tex);
}

// ================================================================ type

/// Lays out `text` as words, with `*accent*` words in the accent colour.
fn words(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut accent = false;
    for (i, part) in text.split('*').enumerate() {
        accent = i % 2 == 1;
        for w in part.split(' ').filter(|w| !w.is_empty()) {
            out.push((w.to_string(), accent));
        }
    }
    let _ = accent;
    // Glue punctuation that followed an accent span onto the previous word.
    let mut merged: Vec<(String, bool)> = Vec::new();
    for (w, a) in out {
        if !merged.is_empty() && w.chars().all(|c| ".,!?;:".contains(c)) {
            let last = merged.last_mut().expect("word");
            last.0.push_str(&w);
        } else {
            merged.push((w, a));
        }
    }
    merged
}

/// A line of words, each rising out of the mask line in turn, then
/// sinking back below it. Returns nothing; draws in screen space.
#[allow(clippy::too_many_arguments)]
fn rising(
    p: &egui::Painter,
    t: f32,
    t0: f32,
    t1: f32,
    text: &str,
    font: egui::FontId,
    at: Pos2,
    centered: bool,
    ink: Color32,
    accent: Color32,
) -> f32 {
    let ws = words(text);
    let space = font.size * 0.27;
    let galleys: Vec<_> = ws
        .iter()
        .map(|(w, a)| p.layout_no_wrap(w.clone(), font.clone(), if *a { accent } else { ink }))
        .collect();
    let total: f32 = galleys.iter().map(|g| g.size().x).sum::<f32>() + space * (ws.len().max(1) - 1) as f32;
    let lh = galleys.first().map(|g| g.size().y).unwrap_or(font.size);
    let mut x = if centered { at.x - total / 2.0 } else { at.x };
    let mask = Rect::from_min_max(pos2(-10.0, at.y - lh * 0.15), pos2(OW + 10.0, at.y + lh * 1.05));
    let mp = p.with_clip_rect(mask);
    for (i, g) in galleys.into_iter().enumerate() {
        let w = g.size().x;
        let rise = ease_out(((t - t0 - i as f32 * 0.075) / 0.6).clamp(0.0, 1.0));
        let sink = ((t - t1 - i as f32 * 0.045) / 0.42).clamp(0.0, 1.0);
        let sink = sink * sink * (3.0 - 2.0 * sink);
        let dy = (1.0 - rise) * lh * 1.1 + sink * lh * 1.1;
        if rise > 0.0 && sink < 1.0 {
            mp.galley(pos2(x, at.y + dy), g, ink);
        }
        x += w + space;
    }
    total
}

fn paint_line(p: &egui::Painter, t: f32, l: &Line, pal: &Palette) {
    if t < l.t0 || t > l.t1 + 0.9 {
        return;
    }
    let font = theme::display(l.size);
    let sub_font = theme::medium(32.0);
    let (anchor, centered) = match l.place {
        Place::Top => (pos2(OW / 2.0, 66.0), true),
        Place::Center(y) => (pos2(OW / 2.0, y - l.size * 0.6), true),
        Place::Right(x) => (
            pos2(
                x,
                OH / 2.0 - l.size * (0.9 + 0.55 * l.text.matches('\n').count() as f32),
            ),
            false,
        ),
    };
    let mut at = anchor;
    if let Some(icon) = l.icon {
        // The feature's icon pops in on a spring just before the words.
        let k = ease_out_back(((t - l.t0) / 0.45).clamp(0.0, 1.0));
        let out = ((t - l.t1) / 0.3).clamp(0.0, 1.0);
        let s = k * (1.0 - out * out);
        if s > 0.0 {
            let c = pos2(OW / 2.0, at.y - 30.0);
            let r = 26.0 * s;
            p.circle_filled(c, r, pal.accent.gamma_multiply(0.16));
            p.text(c, Align2::CENTER_CENTER, icon, theme::regular(28.0 * s), pal.accent);
        }
        at.y += 16.0;
    }
    let rows: Vec<&str> = l.text.split('\n').collect();
    for (i, row) in rows.iter().enumerate() {
        let row_at = at + vec2(0.0, i as f32 * l.size * 1.1);
        let d = i as f32 * 0.15;
        rising(
            p,
            t,
            l.t0 + 0.1 + d,
            l.t1 + d * 0.5,
            row,
            font.clone(),
            row_at,
            centered,
            pal.ink,
            pal.accent,
        );
    }
    if !l.sub.is_empty() {
        let y = at.y + l.size * (1.1 * (rows.len() - 1) as f32 + 1.2);
        let sub_at = pos2(at.x, y);
        rising(
            p,
            t,
            l.t0 + 0.4,
            l.t1 + 0.08,
            l.sub,
            sub_font,
            sub_at,
            centered,
            pal.ink_soft,
            pal.ink_soft,
        );
    }
}

/// "Budgets. → Goals. → …": each word pushes the last one up out of view.
fn paint_flips(p: &egui::Painter, t: f32, pal: &Palette) {
    if !(FLIP_AT..FLIP_END + 0.6).contains(&t) {
        return;
    }
    let font = theme::display(58.0);
    let y = 196.0;
    let lh = 70.0;
    let mask = Rect::from_min_max(pos2(0.0, y - 6.0), pos2(OW, y + lh));
    let mp = p.with_clip_rect(mask);
    for (i, &(t0, word)) in FLIPS.iter().enumerate() {
        let t_next = FLIPS.get(i + 1).map(|f| f.0).unwrap_or(FLIP_END);
        let rise = ease_out(((t - t0) / 0.32).clamp(0.0, 1.0));
        let leave = ease_in_out(((t - t_next) / 0.32).clamp(0.0, 1.0));
        if rise <= 0.0 || leave >= 1.0 {
            continue;
        }
        let dy = (1.0 - rise) * lh - leave * lh;
        let g = p.layout_no_wrap(word.to_string(), font.clone(), pal.accent);
        mp.galley(pos2(OW / 2.0 - g.size().x / 2.0, y + dy), g, pal.accent);
    }
}

fn paint_end(p: &egui::Painter, t: f32, pal: &Palette) {
    if t < 0.0 {
        return;
    }
    let pop = ease_out_back((t / 0.7).clamp(0.0, 1.0));
    let c = pos2(OW / 2.0, 250.0);
    let size = 140.0 * pop;
    if size > 1.0 {
        icon(p, Rect::from_center_size(c, Vec2::splat(size)), 1.0);
    }
    rising(
        p,
        t,
        0.3,
        99.0,
        "Magpie",
        theme::display(112.0),
        pos2(OW / 2.0, 350.0),
        true,
        pal.ink,
        pal.accent,
    );
    rising(
        p,
        t,
        0.65,
        99.0,
        "A calm nest for your money.",
        theme::regular(36.0),
        pos2(OW / 2.0, 488.0),
        true,
        pal.ink_soft,
        pal.ink_soft,
    );
    // Download pill pops on a spring.
    let k = ease_out_back(((t - 1.1) / 0.6).clamp(0.0, 1.0));
    if k > 0.0 {
        let label = p.layout_no_wrap("Download free".into(), theme::semibold(28.0), Color32::WHITE);
        let size = vec2(label.size().x + 84.0, 70.0) * k;
        let r = Rect::from_center_size(pos2(OW / 2.0, 600.0), size);
        p.add(
            Shadow {
                offset: [0, 12],
                blur: 36,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(0xA8, 0x4D, 0x22, 70),
            }
            .as_shape(r, CornerRadius::same((35.0 * k) as u8)),
        );
        crate::widgets::rounded_gradient(p, r, 35.0 * k, rgb(0xE4814F), rgb(0xC0592A));
        if k > 0.6 {
            let mut lp = p.clone();
            lp.set_opacity(((k - 0.6) / 0.4).clamp(0.0, 1.0));
            lp.galley(r.center() - label.size() / 2.0, label, Color32::WHITE);
        }
    }
    rising(
        p,
        t,
        1.5,
        99.0,
        "Linux & macOS  ·  Free & open source  ·  github.com/ahaan-shah/magpie",
        theme::medium(24.0),
        pos2(OW / 2.0, 680.0),
        true,
        pal.ink_soft,
        pal.ink_soft,
    );
}

// ================================================================ the film

#[test]
#[ignore = "renders the launch film (slow); run explicitly"]
fn nature() {
    // SAFETY: single-threaded test setup before the app reads these.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var("MAGPIE_TODAY", "2026-09-29");
    }
    crate::marks::enable();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let out_path =
        std::env::var("MAGPIE_FILM_OUT").unwrap_or_else(|_| format!("{home}/Videos/magpie/magpie-launch.mp4"));
    let stills: Option<Vec<f32>> = std::env::var("MAGPIE_FILM_STILLS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let out_dir = std::path::Path::new(&out_path).parent().expect("out dir").to_path_buf();
    std::fs::create_dir_all(&out_dir).expect("out dir");
    let script = script();
    let input_from = T_WIPE.0;

    // Pass 1: where each camera target is at its key's time.
    let mut world: Vec<Option<Pos2>> = script.cams.iter().map(|k| k.world).collect();
    run_app(
        &script.beats,
        &script.hooks,
        DURATION,
        input_from,
        |t, ctx, _out, _d| {
            for (i, k) in script.cams.iter().enumerate() {
                if world[i].is_none()
                    && k.t <= t + 0.5 / FPS
                    && let Some((target, off)) = &k.app
                {
                    let a = target.rect(ctx).map(|r| r.center()).unwrap_or(pos2(AW / 2.0, AH / 2.0)) + *off;
                    world[i] = Some(app_to_world(a));
                }
            }
        },
    );
    let world: Vec<Pos2> = world.into_iter().map(|p| p.unwrap_or(S0)).collect();

    // Pass 2: film.
    let mut stage = Stage::new();
    let comp = egui::Context::default();
    theme::install_fonts(&comp, "Inter");
    let mut ffmpeg = stills.is_none().then(|| {
        Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", "1920x1080", "-r", &format!("{FPS}"), "-i", "-"])
            .args(encoder_args())
            .args(["-movflags", "+faststart", &out_path])
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg")
    });
    let mut sink = ffmpeg.as_mut().map(|f| f.stdin.take().expect("stdin"));
    let mut frame = vec![0u8; 1920 * 1080 * 4];
    let clock = std::time::Instant::now();
    let mut n = 0usize;
    let trees = trees();
    run_app(
        &script.beats,
        &script.hooks,
        DURATION,
        input_from,
        |t, ctx, out, driver| {
            n += 1;
            let wanted = match &stills {
                Some(list) => list.iter().any(|s| (s - t).abs() < 0.5 / FPS),
                None => true,
            };
            let app_visible = t >= T_WIPE.0 && t <= T_UNWIPE.1;
            if wanted && app_visible {
                stage.render_app(ctx, out);
            } else {
                stage.update_app_textures(out);
                if !wanted {
                    return;
                }
            }
            let cam = cam_at(&script.cams, &world, t);
            let pal = palette_at(t);
            let app_id = stage.app_id;
            let mut comp_out = comp.run_ui(raw(vec2(OW, OH), 1.0, t, Vec::new()), |ui| {
                let p = ui.painter().clone();
                sky(&p, &pal);
                celestial(&p, &cam, &pal, t);
                clouds(&p, &cam, &pal, t);
                landscape(&p, &cam, &pal, t);
                for tr in &trees[..2] {
                    tree(&p, &cam, &pal, t, tr);
                }
                for tr in &trees[2..] {
                    tree(&p, &cam, &pal, t, tr);
                }
                nest(&p, &cam, &pal, t);
                falling_leaves(&p, &cam, &pal, t);

                // Icon ⇄ window.
                let c = cam.layer(1.0);
                let win = window_world();
                let nest_icon = Rect::from_center_size(NEST + vec2(0.0, -46.0), Vec2::splat(ICON));
                let morph = |k: f32| -> Rect {
                    Rect::from_min_max(nest_icon.min.lerp(win.min, k), nest_icon.max.lerp(win.max, k))
                };
                let screen = |r: Rect| Rect::from_min_max(c.apply(r.min), c.apply(r.max));
                let pose = Pose {
                    focus: pos2(AW / 2.0, AH / 2.0),
                    at: c.apply(WIN_AT),
                    scale: WIN_SCALE * c.zoom,
                    tilt: 0.0,
                    alpha: 1.0,
                };
                if t >= T_POP && t < T_MORPH.0 {
                    let s = ease_out_back(((t - T_POP) / 0.6).clamp(0.0, 1.0));
                    let sway = vec2(wind(t, 250.0) * 7.0 * 0.8, 0.0);
                    let r = Rect::from_center_size(nest_icon.center() + sway, nest_icon.size() * s);
                    icon(&p, screen(r), 1.0);
                } else if (T_MORPH.0..T_UNWIPE.0).contains(&t) {
                    let k = ease_in_out(((t - T_MORPH.0) / (T_MORPH.1 - T_MORPH.0)).clamp(0.0, 1.0));
                    let r = screen(morph(k));
                    if t < T_WIPE.1 {
                        icon(&p, r, 1.0);
                    }
                    if t >= T_WIPE.0 {
                        let w = ease_in_out(((t - T_WIPE.0) / (T_WIPE.1 - T_WIPE.0)).clamp(0.0, 1.0));
                        let top = r.bottom() + (r.top() - 2.0 - r.bottom()) * w;
                        window(&p, &pose, app_id, (w < 1.0).then_some(top));
                    }
                } else if (T_UNWIPE.0..T_HOME.1).contains(&t) {
                    let k = ease_in_out(((t - T_HOME.0) / (T_HOME.1 - T_HOME.0)).clamp(0.0, 1.0));
                    let r = screen(morph(1.0 - k));
                    icon(&p, r, 1.0);
                    if t < T_UNWIPE.1 {
                        let w = ease_in_out(((t - T_UNWIPE.0) / (T_UNWIPE.1 - T_UNWIPE.0)).clamp(0.0, 1.0));
                        let top = r.top() + (r.bottom() - r.top()) * w;
                        window(&p, &pose, app_id, Some(top));
                    }
                } else if t >= T_HOME.1 {
                    let land = ((t - T_HOME.1) / 0.35).clamp(0.0, 1.0);
                    let squash = 1.0 + (land * PI).sin() * 0.08;
                    let sway = vec2(wind(t, 250.0) * 7.0 * 0.8, 0.0);
                    let r = Rect::from_center_size(nest_icon.center() + sway, vec2(ICON * squash, ICON / squash));
                    icon(&p, screen(r), 1.0);
                }

                foreground(&p, &cam, &pal, t);
                // A few flocks glide by.
                for (t0, y, dir, size) in [
                    (0.6, 230.0, 1.0, 22.0),
                    (13.8, 120.0, -1.0, 16.0),
                    (38.8, 600.0, 1.0, 20.0),
                ] {
                    let k = (t - t0) / 8.0;
                    if (0.0..=1.0).contains(&k) {
                        let x = if dir > 0.0 {
                            -200.0 + 2300.0 * k
                        } else {
                            2100.0 - 2300.0 * k
                        };
                        for (i, (dx, dy)) in [(0.0, 0.0), (-60.0, 26.0), (-104.0, -14.0)].into_iter().enumerate() {
                            let ph = i as f32 * 1.3 + t0;
                            let beat = ((t * 0.7 + ph).sin() * 0.5 + 0.5).powf(1.5);
                            let flap = (t * 5.0 + ph).sin() * (0.15 + 0.85 * beat);
                            let at = cam
                                .layer(0.3)
                                .apply(pos2(x + dx * dir, y + dy + (t * 1.1 + ph).sin() * 6.0));
                            let col = pal.ink;
                            let mut bp = p.clone();
                            bp.set_opacity(0.55);
                            let _ = col;
                            paint_bird(&bp, at, size * (1.0 - i as f32 * 0.12), flap, 1.0);
                        }
                    }
                }

                // Cursor, while the camera is in close.
                let near = ((cam.zoom - 1.7) / 0.4).clamp(0.0, 1.0);
                if near > 0.0 && app_visible {
                    let s = 1.15 * (cam.zoom / 2.0).sqrt();
                    let to_screen = |a: Pos2| c.apply(app_to_world(a));
                    for (at, t0) in &driver.pointer.ripples {
                        let k = (t - t0) / 0.55;
                        if (0.0..1.0).contains(&k) {
                            let cc = to_screen(*at);
                            let rr = (10.0 + 30.0 * ease_out(k)) * s;
                            p.circle_filled(cc, rr, pal.accent.gamma_multiply(0.2 * (1.0 - k)));
                            p.circle_stroke(cc, rr, Stroke::new(2.0 * s, pal.accent.gamma_multiply(0.7 * (1.0 - k))));
                        }
                    }
                    let mut cp = p.clone();
                    cp.set_opacity(near);
                    paint_cursor(&cp, to_screen(driver.pointer.pos), s);
                }

                for l in &script.lines {
                    paint_line(&p, t, l, &pal);
                }
                paint_flips(&p, t, &pal);
                paint_end(&p, t - T_END, &pal);
            });
            stage.render_comp(&comp, &mut comp_out, &mut frame);
            match &mut sink {
                Some(s) => s.write_all(&frame).expect("write frame"),
                None => {
                    let path = out_dir.join(format!("launch-{t:05.2}.png"));
                    if let Some(png) = image::RgbaImage::from_raw(1920, 1080, frame.clone()) {
                        png.save(&path).expect("save");
                        eprintln!("film: wrote {}", path.display());
                    }
                }
            }
            if n.is_multiple_of(300) {
                eprintln!("film: {t:>4.1}s / {DURATION}s  ({:.0?})", clock.elapsed());
            }
        },
    );
    drop(sink);
    if let Some(mut f) = ffmpeg {
        assert!(f.wait().expect("ffmpeg").success(), "ffmpeg failed");
        eprintln!("film: wrote {out_path} in {:.0?}", clock.elapsed());
    }
}
