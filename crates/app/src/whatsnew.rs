//! "What's new" after an update: a quiet row in the sidebar (where the
//! update prompt was) that opens a card with the release's changes in a few
//! plain points. The row goes away as soon as the person moves to another
//! page; Settings → About keeps a link to the full release notes.
//!
//! Every release adds its notes to [`RELEASES`] (see CLAUDE.md → Releasing).

use crate::icons::ph;
use crate::motion;
use crate::theme::{self, Theme};
use crate::widgets as w;
use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use magpie_core::Store;
use magpie_core::update::is_newer;
use std::collections::HashMap;

/// One change, said simply.
pub struct Item {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
    /// A picture of the change, when words aren't enough: a PNG from
    /// `assets/whatsnew/`, about 2:1, via `include_bytes!`.
    pub image: Option<&'static [u8]>,
}

pub struct Notes {
    pub version: &'static str,
    pub items: &'static [Item],
}

/// Newest first. Keep each to 2–4 points in everyday words: what changed
/// and why it helps, not how it works.
pub const RELEASES: &[Notes] = &[
    Notes {
        version: "0.2.5",
        items: &[
            Item {
                icon: ph::TAG,
                title: "A usual category for each payee",
                body: "Change a transaction's category and Magpie offers to use it for everything \
                       from that payee, including new ones and imports.",
                image: None,
            },
            Item {
                icon: ph::GIFT,
                title: "One-offs stay one-offs",
                body: "Bought a gift at your usual grocer? Pick \"Just this one\" and only that \
                       transaction changes.",
                image: None,
            },
        ],
    },
    Notes {
        version: "0.2.4",
        items: &[
            Item {
                icon: ph::KEYBOARD,
                title: "Pick a payee with the arrow keys",
                body: "As you type a payee, use the arrow keys to move through the suggestions \
                       and press Enter to pick one.",
                image: None,
            },
            Item {
                icon: ph::GIFT,
                title: "What's new waits for you",
                body: "The What's new button stays in the sidebar until you've opened it, \
                       even if you move around the app first.",
                image: None,
            },
        ],
    },
    Notes {
        version: "0.2.3",
        items: &[
            Item {
                icon: ph::SPARKLE,
                title: "See what's new after each update",
                body: "When Magpie updates, this card shows what changed in a few words. \
               You can find the full notes any time in Settings.",
                image: None,
            },
            Item {
                icon: ph::KEYBOARD,
                title: "Pick a category by typing",
                body: "Open the category list and type a few letters, like \"gro\" for Groceries. \
               Press Enter to pick it.",
                image: None,
            },
            Item {
                icon: ph::FILE_ARROW_DOWN,
                title: "More bank statements import cleanly",
                body: "Statements with lots of account details above the transactions now \
               import without any fixing up.",
                image: None,
            },
        ],
    },
];

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Set by the updater when it restarts Magpie into a new version.
pub const UPDATED_FROM_ENV: &str = "MAGPIE_UPDATED_FROM";

pub fn release_url(version: &str) -> String {
    format!("https://github.com/ahaan-shah/magpie/releases/tag/v{version}")
}

/// The notes to show on this launch: every release newer than the one this
/// workspace last ran, up to the running one. `updated_from` (from the
/// updater's restart) wins over the saved version. With neither, someone
/// already onboarded has updated from a version before this existed, so they
/// see this release's notes; a brand-new workspace sees nothing.
pub fn to_show(
    releases: &'static [Notes],
    seen: Option<&str>,
    current: &str,
    onboarded: bool,
    updated_from: Option<&str>,
) -> Vec<&'static Notes> {
    match updated_from.or(seen) {
        Some(from) => releases
            .iter()
            .filter(|n| is_newer(n.version, from) && !is_newer(n.version, current))
            .collect(),
        None if onboarded => releases.iter().filter(|n| n.version == current).collect(),
        None => Vec::new(),
    }
}

#[derive(Default)]
pub struct State {
    pub notes: Vec<&'static Notes>,
    /// The sidebar row is showing.
    pub pending: bool,
    images: HashMap<(usize, usize), egui::TextureHandle>,
}

impl State {
    /// Decides what to show, and remembers this version when there's
    /// nothing to say (otherwise once the row is dismissed).
    pub fn new(store: &mut Store) -> State {
        let s = store.settings();
        let updated_from = std::env::var(UPDATED_FROM_ENV).ok();
        let notes = to_show(
            RELEASES,
            s.seen_version.as_deref(),
            VERSION,
            s.onboarded,
            updated_from.as_deref(),
        );
        let state = State {
            pending: !notes.is_empty(),
            notes,
            images: HashMap::new(),
        };
        if !state.pending {
            remember(store);
        }
        state
    }

    /// Takes the row away for good (the person moved on).
    pub fn dismiss(&mut self, store: &mut Store) {
        if self.pending {
            self.pending = false;
            remember(store);
        }
    }
}

fn remember(store: &mut Store) {
    if store.settings().seen_version.as_deref() != Some(VERSION) {
        let _ = store.update_settings(|s| s.seen_version = Some(VERSION.into()));
    }
}

/// The sidebar row, styled like the update prompt it replaces: an icon with
/// a small accent dot, and "What's new".
pub fn row(ui: &mut Ui, t: &Theme, collapse: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    crate::marks::record(|| "whatsnew:row".into(), rect);
    let ctx = ui.ctx().clone();
    let hover = motion::toggle(&ctx, Id::new("whatsnew-row-h"), resp.hovered(), motion::MICRO);
    let shown = motion::appear(&ctx, 0.0, 0.0, 0.4);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(10), motion::with_alpha(t.hover_wash(), hover));
    }
    let fg = motion::with_alpha(motion::lerp_color(t.text2, t.text, hover), shown);
    let icon_x = motion::lerp(rect.left() + 14.0, rect.center().x - 8.0, collapse);
    let icon_c = pos2(icon_x + 8.0, rect.center().y);
    p.text(icon_c, Align2::CENTER_CENTER, ph::GIFT, theme::regular(16.0), fg);
    p.circle_filled(icon_c + vec2(7.0, -6.0), 3.0, motion::with_alpha(t.accent, shown));
    if collapse < 0.6 {
        p.text(
            pos2(rect.left() + 44.0, rect.center().y),
            Align2::LEFT_CENTER,
            "What's new",
            theme::medium(12.5),
            motion::with_alpha(fg, 1.0 - collapse / 0.6),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("See what's new in Magpie {VERSION}"))
}

/// The card. Returns true when it should close.
pub fn card(app: &mut crate::app::App, ui: &mut Ui, t: &Theme) -> bool {
    let ctx = ui.ctx().clone();
    let mut close = false;
    // The newest release being described (normally the running one).
    let version = app.whats_new.notes.first().map_or(VERSION, |n| n.version);
    // Header: a gift in a soft accent tile, then the title.
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(14), t.accent_soft());
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            ph::GIFT,
            theme::regular(23.0),
            t.accent,
        );
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            ui.label(
                egui::RichText::new("What's new")
                    .font(theme::display(23.0))
                    .color(t.text),
            );
            ui.label(w::faint(t, format!("Magpie {version}")));
        });
    });
    ui.add_space(18.0);

    let notes = app.whats_new.notes.clone();
    let several = notes.len() > 1;
    let width = ui.available_width();
    w::scroll_area()
        .id_salt("whatsnew-scroll")
        .max_height(440.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_width(width);
            for (ni, n) in notes.iter().enumerate() {
                if several {
                    if ni > 0 {
                        ui.add_space(6.0);
                    }
                    ui.label(
                        egui::RichText::new(format!("IN {}", n.version))
                            .font(theme::semibold(10.5))
                            .color(t.text3),
                    );
                    ui.add_space(8.0);
                }
                for (ii, item) in n.items.iter().enumerate() {
                    point(app, ui, t, (ni, ii), item, width);
                    ui.add_space(16.0);
                }
            }
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if w::ghost(ui, t, Some(ph::ARROW_SQUARE_OUT), "Full release notes").clicked() {
            crate::app::open_url(&release_url(version));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let ok = w::primary(ui, t, None, "Got it");
            crate::marks::record(|| "whatsnew:ok".into(), ok.rect);
            if ok.clicked() {
                close = true;
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        close = true;
    }
    close
}

/// One point: icon tile, a short title, a sentence, and maybe a picture.
fn point(app: &mut crate::app::App, ui: &mut Ui, t: &Theme, key: (usize, usize), item: &Item, width: f32) {
    let text_x = 50.0;
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(
            r,
            CornerRadius::same(11),
            t.tint(t.accent, if t.dark { 0.16 } else { 0.1 }),
        );
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            item.icon,
            theme::regular(17.0),
            t.accent,
        );
        ui.add_space(text_x - 36.0 - ui.spacing().item_spacing.x);
        ui.vertical(|ui| {
            ui.set_width(width - text_x);
            ui.add_space(1.0);
            ui.label(
                egui::RichText::new(item.title)
                    .font(theme::semibold(15.0))
                    .color(t.text),
            );
            ui.add_space(2.0);
            ui.label(egui::RichText::new(item.body).font(theme::regular(13.5)).color(t.text2));
        });
    });
    let Some(bytes) = item.image else { return };
    let tex = app
        .whats_new
        .images
        .entry(key)
        .or_insert_with(|| {
            let img = image::load_from_memory(bytes).map(|i| i.to_rgba8()).unwrap_or_default();
            let size = [img.width() as usize, img.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            ui.ctx().load_texture(
                format!("whatsnew-{}-{}", key.0, key.1),
                color,
                egui::TextureOptions::LINEAR,
            )
        })
        .clone();
    let [iw, ih] = tex.size();
    if iw == 0 || ih == 0 {
        return;
    }
    ui.add_space(10.0);
    let w_img = width - text_x;
    let size = vec2(w_img, w_img * ih as f32 / iw as f32);
    let (slot, _) = ui.allocate_exact_size(vec2(width, size.y), Sense::hover());
    let rect = Rect::from_min_size(pos2(slot.left() + text_x, slot.top()), size);
    egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
        .corner_radius(CornerRadius::same(12))
        .paint_at(ui, rect);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(12),
        egui::Stroke::new(1.0, t.border),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &[Notes] = &[
        Notes {
            version: "0.2.5",
            items: &[],
        },
        Notes {
            version: "0.2.4",
            items: &[],
        },
        Notes {
            version: "0.2.3",
            items: &[],
        },
    ];

    fn versions(v: Vec<&Notes>) -> Vec<&str> {
        v.iter().map(|n| n.version).collect()
    }

    #[test]
    fn after_an_update_shows_whats_newer() {
        assert_eq!(versions(to_show(FAKE, Some("0.2.3"), "0.2.4", true, None)), ["0.2.4"]);
        // Skipped a version: both, newest first.
        assert_eq!(
            versions(to_show(FAKE, Some("0.2.3"), "0.2.5", true, None)),
            ["0.2.5", "0.2.4"]
        );
        // The updater's restart says where it came from, even when the
        // saved version is missing or already current (a demo workspace).
        assert_eq!(
            versions(to_show(FAKE, Some("0.2.4"), "0.2.4", true, Some("0.2.3"))),
            ["0.2.4"]
        );
    }

    #[test]
    fn nothing_without_an_update() {
        assert!(to_show(FAKE, Some("0.2.4"), "0.2.4", true, None).is_empty());
        // A new workspace: nothing to announce.
        assert!(to_show(FAKE, None, "0.2.4", false, None).is_empty());
        // Running an older build than the notes: never shows future notes.
        assert!(to_show(FAKE, Some("0.2.2"), "0.2.2", true, None).is_empty());
    }

    #[test]
    fn existing_users_from_before_this_feature_see_this_release() {
        assert_eq!(versions(to_show(FAKE, None, "0.2.4", true, None)), ["0.2.4"]);
    }

    /// Every release from 0.2.3 on must say what's new (CLAUDE.md →
    /// Releasing): bumping the version without notes fails here.
    #[test]
    fn this_release_has_notes() {
        if is_newer(VERSION, "0.2.2") {
            let notes = RELEASES.iter().find(|n| n.version == VERSION);
            assert!(
                notes.is_some_and(|n| !n.items.is_empty()),
                "add What's new notes for {VERSION} to whatsnew::RELEASES"
            );
        }
        // Newest first, so a skipped-version card reads top-down.
        assert!(RELEASES.windows(2).all(|w| is_newer(w[0].version, w[1].version)));
    }
}
