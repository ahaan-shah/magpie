//! Headless "monkey" stress test: drives the real app through thousands of
//! frames of random clicks, keys, typing, scrolling and window sizes on every
//! page, and fails on any panic. Run with `cargo test --release monkey`.

use crate::app::{App, Page};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn f(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }
    fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

const KEYS: &[Key] = &[
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Enter,
    Key::Escape,
    Key::Tab,
    Key::Space,
    Key::Delete,
    Key::Backspace,
    Key::PageUp,
    Key::PageDown,
    Key::Home,
    Key::End,
    Key::A,
    Key::B,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::J,
    Key::K,
    Key::L,
    Key::N,
    Key::R,
    Key::T,
    Key::Y,
    Key::Z,
    Key::Slash,
    Key::Questionmark,
    Key::Comma,
    Key::Minus,
    Key::Plus,
    Key::Equals,
    Key::Num0,
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::F1,
];

const TEXT: &[&str] = &[
    "coffee 4.50 #treats yesterday",
    "+3200 @Acme salary",
    "12",
    "-0.01",
    "999999999999999",
    "abc",
    "₹€¥ 日本 🐦",
    "1,234.56",
    "(3.50)",
    "2026-02-30",
    "sep 31",
    "",
    " ",
    "#",
    "@",
    "/x",
    "~",
    "0",
    ".",
];

fn key_event(key: Key, modifiers: Modifiers, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

fn run(seed: u64, frames: usize) {
    // SAFETY: tests in this binary don't read these env vars concurrently.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var(
            "MAGPIE_EXPORT_DIR",
            std::env::temp_dir().join(format!("magpie-monkey-exports-{seed}")),
        );
    }
    let _ = std::fs::create_dir_all(std::env::temp_dir().join(format!("magpie-monkey-exports-{seed}")));
    let dir = std::env::temp_dir().join(format!("magpie-monkey-{seed}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = magpie_core::Store::open(&dir).expect("open store");
    magpie_core::demo::generate(&mut store, magpie_core::Cur::USD, 24, 0).expect("demo");

    let ctx = egui::Context::default();
    let mut app = App::with_context(&ctx, None, store);
    let mut rng = Rng(seed | 1);
    let mut size = vec2(1400.0, 900.0);
    let mut pointer;
    let mut time = 0.0f64;
    let pages = [
        Page::Dashboard,
        Page::Ledger,
        Page::Budgets,
        Page::Reports,
        Page::Accounts,
        Page::Recurring,
        Page::Goals,
        Page::Settings,
    ];

    for frame in 0..frames {
        time += 1.0 / 60.0;
        let mut events = Vec::new();
        if rng.chance(0.004) {
            size = match rng.below(4) {
                0 => vec2(700.0, 500.0),
                1 => vec2(2560.0, 1440.0),
                2 => vec2(1100.0, 700.0),
                _ => vec2(1400.0, 900.0),
            };
        }
        // Visit every page regularly so coverage doesn't depend on luck.
        if frame % 250 == 0 {
            app.go(&ctx, pages[(frame / 250) % pages.len()]);
        }
        let action = rng.below(100);
        let mods = match rng.below(10) {
            0 => Modifiers::COMMAND,
            1 => Modifiers::ALT,
            2 => Modifiers::SHIFT,
            3 => Modifiers::COMMAND | Modifiers::SHIFT,
            _ => Modifiers::NONE,
        };
        if action < 30 {
            pointer = pos2(rng.f() * size.x, rng.f() * size.y);
            events.push(Event::PointerMoved(pointer));
        } else if action < 50 {
            pointer = pos2(rng.f() * size.x, rng.f() * size.y);
            events.push(Event::PointerMoved(pointer));
            for pressed in [true, false] {
                events.push(Event::PointerButton {
                    pos: pointer,
                    button: if rng.chance(0.9) {
                        PointerButton::Primary
                    } else {
                        PointerButton::Secondary
                    },
                    pressed,
                    modifiers: mods,
                });
            }
        } else if action < 62 {
            events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, (rng.f() - 0.5) * 12.0),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
        } else if action < 85 {
            let key = KEYS[rng.below(KEYS.len() as u64) as usize];
            // Avoid quitting-style shortcuts; everything else is fair game.
            events.push(key_event(key, mods, true));
            events.push(key_event(key, mods, false));
        } else if action < 95 {
            events.push(Event::Text(TEXT[rng.below(TEXT.len() as u64) as usize].to_string()));
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        // A real window would upload these to the GPU; we just discard them.
        out.textures_delta.clear();
    }

    eprintln!(
        "monkey seed {seed}: {frames} frames, store changed {} times, {} transactions, undo: {:?}",
        app.store.version(),
        app.store.txns().len(),
        app.store.undo_label()
    );
    // The data must still be consistent and reload cleanly.
    let t = app.store.txns();
    assert!(
        t.windows(2).all(|w| (w[0].date, w[0].id) <= (w[1].date, w[1].id)),
        "transactions out of order"
    );
    drop(app);
    let reopened = magpie_core::Store::open(&dir).expect("reopen");
    assert!(!reopened.accounts().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn monkey_many_seeds() {
    let frames: usize = std::env::var("MONKEY_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    for seed in [1, 7, 42, 1337, 9001] {
        run(seed, frames);
    }
}
