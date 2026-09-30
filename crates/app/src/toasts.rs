//! Bottom-right notifications that spring in, and optionally offer Undo.

use crate::icons::ph;
use crate::motion;
use crate::theme::{self, Theme};
use egui::{Align2, Color32, CornerRadius, Id, LayerId, Order, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Level {
    Info,
    Success,
    Error,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    Undo,
}

struct Toast {
    id: u64,
    text: String,
    level: Level,
    action: Option<Action>,
    born: f64,
    closing: Option<f64>,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next: u64,
    pending: Vec<(String, Level, Option<Action>)>,
}

const LIFE: f64 = 4.2;
const OUT: f64 = 0.22;

impl Toasts {
    pub fn push(&mut self, text: impl Into<String>, level: Level, action: Option<Action>) {
        self.pending.push((text.into(), level, action));
    }
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(text, Level::Info, None);
    }
    pub fn success(&mut self, text: impl Into<String>) {
        self.push(text, Level::Success, None);
    }
    pub fn error(&mut self, text: impl Into<String>) {
        self.push(text, Level::Error, None);
    }
    /// Surfaces an error as a toast and turns the result into an Option.
    pub fn ok<T>(&mut self, r: magpie_core::Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.error(e.to_string());
                None
            }
        }
    }

    pub fn undoable(&mut self, text: impl Into<String>) {
        self.push(text, Level::Success, Some(Action::Undo));
    }

    /// Draws the stack; returns an action the user clicked.
    pub fn show(&mut self, ctx: &egui::Context, t: &Theme) -> Option<Action> {
        let now = ctx.input(|i| i.time);
        for (text, level, action) in self.pending.drain(..) {
            self.next += 1;
            self.items.push(Toast {
                id: self.next,
                text,
                level,
                action,
                born: now,
                closing: None,
            });
        }
        if self.items.len() > 4 {
            let excess = self.items.len() - 4;
            for it in self.items.iter_mut().take(excess) {
                it.closing.get_or_insert(now);
            }
        }
        let mut clicked = None;
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - 20.0;
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("toasts")));
        for it in self.items.iter_mut().rev() {
            if it.closing.is_none() && now - it.born > LIFE {
                it.closing = Some(now);
            }
            let enter = motion::ease_out_back(((now - it.born) / 0.38) as f32);
            let exit = it
                .closing
                .map(|c| motion::ease_out(((now - c) / OUT) as f32))
                .unwrap_or(0.0);
            let alpha = (((now - it.born) / 0.18) as f32).clamp(0.0, 1.0) * (1.0 - exit);
            let (icon, color) = match it.level {
                Level::Info => (ph::INFO, t.accent),
                Level::Success => (ph::CHECK_CIRCLE, t.pos),
                Level::Error => (ph::WARNING, t.neg),
            };
            let text_g = painter.layout(it.text.clone(), theme::medium(13.0), t.text, 320.0);
            let action_w = if it.action.is_some() { 64.0 } else { 0.0 };
            let w = text_g.size().x + 56.0 + action_w;
            let h = text_g.size().y.max(18.0) + 24.0;
            let x = screen.right() - 20.0 - w + (1.0 - enter) * 40.0;
            let rect = Rect::from_min_size(pos2(x, y - h), vec2(w, h));
            let rect = rect.translate(vec2(exit * 30.0, 0.0));
            let a = |c: Color32| motion::with_alpha(c, alpha);
            painter.rect_filled(
                rect.translate(vec2(0.0, 6.0)).expand(3.0),
                CornerRadius::same(14),
                a(t.shadow()),
            );
            painter.rect(
                rect,
                CornerRadius::same(12),
                a(t.elevated),
                Stroke::new(1.0, a(t.border)),
                StrokeKind::Inside,
            );
            painter.text(
                pos2(rect.left() + 16.0, rect.center().y),
                Align2::LEFT_CENTER,
                icon,
                theme::regular(17.0),
                a(color),
            );
            painter.galley(
                pos2(rect.left() + 40.0, rect.center().y - text_g.size().y / 2.0),
                text_g,
                a(t.text),
            );
            if let Some(action) = it.action {
                let br = Rect::from_min_size(
                    pos2(rect.right() - action_w - 6.0, rect.center().y - 14.0),
                    vec2(action_w, 28.0),
                );
                let resp = ctx.read_response(Id::new(("toast", it.id)));
                let hovered = resp.as_ref().is_some_and(|r| r.hovered());
                painter.rect_filled(
                    br,
                    CornerRadius::same(8),
                    a(if hovered { t.accent_soft() } else { t.hover }),
                );
                painter.text(
                    br.center(),
                    Align2::CENTER_CENTER,
                    "Undo",
                    theme::semibold(12.5),
                    a(t.accent),
                );
                let r = egui::Area::new(Id::new(("toast-area", it.id)))
                    .order(Order::Tooltip)
                    .fixed_pos(br.min)
                    .show(ctx, |ui| {
                        ui.interact(
                            Rect::from_min_size(br.min, br.size()),
                            Id::new(("toast", it.id)),
                            Sense::click(),
                        )
                    })
                    .inner;
                if r.clicked() && it.closing.is_none() {
                    clicked = Some(action);
                    it.closing = Some(now);
                }
            }
            y -= (h + 10.0) * (1.0 - exit);
        }
        self.items.retain(|it| it.closing.is_none_or(|c| now - c < OUT));
        if !self.items.is_empty() {
            // Wake up when the next toast should start leaving.
            ctx.request_repaint_after(std::time::Duration::from_millis(
                if self.items.iter().any(|i| now - i.born < 0.5 || i.closing.is_some()) {
                    0
                } else {
                    250
                },
            ));
        }
        clicked
    }
}
