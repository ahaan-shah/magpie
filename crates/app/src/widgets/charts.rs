//! Hand-drawn, animated charts. Each chart grows in on first paint and
//! morphs smoothly to new values when the data changes.

use crate::motion;
use crate::theme::{self, Theme};
use egui::epaint::{Mesh, PathShape, PathStroke, Vertex, WHITE_UV};
use egui::{
    Align2, Color32, CornerRadius, Id, LayerId, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};
use std::f32::consts::{PI, TAU};

// ------------------------------------------------------------ animation

#[derive(Clone)]
struct Morph {
    from: Vec<f32>,
    to: Vec<f32>,
    start: f64,
}

/// Returns `values` animated: the first time from zero, then morphing from
/// whatever was on screen to the new values whenever they change.
pub fn animated(ui: &Ui, id: Id, values: &[f32], dur: f32) -> Vec<f32> {
    let ctx = ui.ctx();
    let now = ctx.input(|i| i.time);
    let key = id.with("morph");
    let m: Option<Morph> = ctx.data(|d| d.get_temp(key));
    let mut m = m.unwrap_or_else(|| Morph {
        from: vec![0.0; values.len()],
        to: values.to_vec(),
        start: now,
    });
    let p = motion::ease_out_quint(((now - m.start) as f32 / dur).clamp(0.0, 1.0));
    let current: Vec<f32> =
        m.to.iter()
            .enumerate()
            .map(|(i, t)| motion::lerp(m.from.get(i).copied().unwrap_or(0.0), *t, p))
            .collect();
    if m.to.as_slice() != values {
        let from = if current.len() == values.len() {
            current.clone()
        } else {
            vec![0.0; values.len()]
        };
        m = Morph {
            from,
            to: values.to_vec(),
            start: now,
        };
        ctx.data_mut(|d| d.insert_temp(key, m));
        ctx.request_repaint();
        return if current.len() == values.len() {
            current
        } else {
            vec![0.0; values.len()]
        };
    }
    ctx.data_mut(|d| d.insert_temp(key, m));
    if p < 1.0 {
        ctx.request_repaint();
    }
    current
}

/// 0→1 sweep the first time a chart id is painted.
fn intro(ui: &Ui, id: Id) -> f32 {
    motion::ease_out(motion::tween_from(ui.ctx(), id.with("intro"), 0.0, 1.0, motion::CHART))
}

// ---------------------------------------------------------------- axes

/// A "nice" axis maximum and step for `max`.
pub fn nice_scale(max: f32, ticks: usize) -> (f32, f32) {
    if max <= 0.0 || !max.is_finite() {
        return (1.0, 1.0 / ticks as f32);
    }
    let raw = max / ticks as f32;
    let mag = 10f32.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    ((max / step).ceil() * step, step)
}

/// Axis bounds `(min, max, step)` covering `lo..=hi` with ~4 nice ticks.
/// The step comes from the whole span, so a tiny max with a huge negative
/// min can't produce millions of grid lines.
pub fn axis(lo: f32, hi: f32) -> (f32, f32, f32) {
    let (lo, hi) = if lo.is_finite() && hi.is_finite() {
        (lo.min(hi), hi.max(lo))
    } else {
        (0.0, 1.0)
    };
    let (_, step) = nice_scale((hi - lo).max(1.0), 4);
    let min = (lo / step).floor() * step;
    let mut max = (hi / step).ceil() * step;
    if max <= min {
        max = min + step;
    }
    (min, max, step)
}

fn tooltip(ui: &Ui, t: &Theme, id: Id, anchor: Pos2, title: &str, lines: &[(Color32, String)]) {
    let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, id.with("tip")));
    let title_g = painter.layout_no_wrap(title.to_string(), theme::medium(11.5), t.text2);
    let line_g: Vec<_> = lines
        .iter()
        .map(|(_, s)| painter.layout_no_wrap(s.clone(), theme::semibold(13.0), t.text))
        .collect();
    let w = line_g
        .iter()
        .map(|g| g.size().x + 16.0)
        .fold(title_g.size().x, f32::max)
        + 20.0;
    let h = 12.0 + title_g.size().y + 4.0 + line_g.iter().map(|g| g.size().y + 3.0).sum::<f32>() + 8.0;
    let screen = ui.ctx().content_rect();
    let mut pos = anchor + vec2(14.0, -h - 10.0);
    if pos.x + w > screen.right() - 8.0 {
        pos.x = anchor.x - w - 14.0;
    }
    if pos.y < screen.top() + 8.0 {
        pos.y = anchor.y + 14.0;
    }
    let rect = Rect::from_min_size(pos, vec2(w, h));
    painter.add(egui::epaint::RectShape::filled(
        rect.translate(vec2(0.0, 4.0)).expand(2.0),
        CornerRadius::same(12),
        t.shadow(),
    ));
    painter.rect(
        rect,
        CornerRadius::same(10),
        t.elevated,
        Stroke::new(1.0, t.border),
        StrokeKind::Inside,
    );
    let mut y = rect.top() + 9.0;
    painter.galley(pos2(rect.left() + 10.0, y), title_g.clone(), t.text2);
    y += title_g.size().y + 4.0;
    for ((c, _), g) in lines.iter().zip(line_g) {
        painter.circle_filled(pos2(rect.left() + 14.0, y + g.size().y / 2.0), 3.5, *c);
        painter.galley(pos2(rect.left() + 24.0, y), g.clone(), t.text);
        y += g.size().y + 3.0;
    }
}

fn grid(ui: &Ui, t: &Theme, plot: Rect, max: f32, step: f32, min: f32, fmt: &dyn Fn(f32) -> String) {
    let p = ui.painter();
    let range = max - min;
    let mut v = min;
    let mut n = 0;
    while v <= max + step * 0.01 && n < 16 {
        n += 1;
        let y = plot.bottom() - (v - min) / range * plot.height();
        let y = p.round_to_pixel_center(y);
        p.hline(
            plot.x_range(),
            y,
            Stroke::new(1.0, motion::with_alpha(t.border, if v == 0.0 { 1.0 } else { 0.6 })),
        );
        p.text(
            pos2(plot.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            fmt(v),
            theme::regular(11.0),
            t.text3,
        );
        v += step;
    }
}

fn x_labels(ui: &Ui, t: &Theme, plot: Rect, labels: &[String], xs: &[f32], every: usize) {
    let p = ui.painter();
    for (i, (l, x)) in labels.iter().zip(xs).enumerate() {
        if i % every.max(1) == 0 || i + 1 == labels.len() {
            p.text(
                pos2(*x, plot.bottom() + 8.0),
                Align2::CENTER_TOP,
                l,
                theme::regular(11.0),
                t.text3,
            );
        }
    }
}

// ---------------------------------------------------------- area chart

/// Monotone cubic interpolation (Fritsch–Carlson): smooth, but never
/// overshoots between points — important so money never looks like it dipped
/// when it didn't.
pub fn smooth(points: &[Pos2], steps: usize) -> Vec<Pos2> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let dx: Vec<f32> = (0..n - 1).map(|i| points[i + 1].x - points[i].x).collect();
    let slope: Vec<f32> = (0..n - 1)
        .map(|i| {
            if dx[i] == 0.0 {
                0.0
            } else {
                (points[i + 1].y - points[i].y) / dx[i]
            }
        })
        .collect();
    let mut m = vec![0.0; n];
    m[0] = slope[0];
    m[n - 1] = slope[n - 2];
    for i in 1..n - 1 {
        m[i] = if slope[i - 1] * slope[i] <= 0.0 {
            0.0
        } else {
            (slope[i - 1] + slope[i]) / 2.0
        };
    }
    for i in 0..n - 1 {
        if slope[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
        } else {
            let a = m[i] / slope[i];
            let b = m[i + 1] / slope[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let tau = 3.0 / s.sqrt();
                m[i] = tau * a * slope[i];
                m[i + 1] = tau * b * slope[i];
            }
        }
    }
    let mut out = Vec::with_capacity((n - 1) * steps + 1);
    for i in 0..n - 1 {
        let (p0, p1, h) = (points[i], points[i + 1], dx[i]);
        for s in 0..steps {
            let u = s as f32 / steps as f32;
            let (h00, h10, h01, h11) = (
                2.0 * u.powi(3) - 3.0 * u.powi(2) + 1.0,
                u.powi(3) - 2.0 * u.powi(2) + u,
                -2.0 * u.powi(3) + 3.0 * u.powi(2),
                u.powi(3) - u.powi(2),
            );
            let y = h00 * p0.y + h10 * h * m[i] + h01 * p1.y + h11 * h * m[i + 1];
            out.push(pos2(p0.x + u * h, y));
        }
    }
    out.push(points[n - 1]);
    out
}

pub struct AreaOpts<'a> {
    pub color: Color32,
    pub axis: bool,
    pub labels: &'a [String],
    pub fmt: &'a dyn Fn(f32) -> String,
    pub baseline_zero: bool,
    pub label_every: usize,
}

/// Smooth line with a gradient fill underneath and a hover crosshair.
pub fn area_chart(ui: &mut Ui, t: &Theme, id: Id, rect: Rect, values: &[f32], o: AreaOpts) {
    if values.len() < 2 {
        return;
    }
    let vals = animated(ui, id, values, motion::CHART);
    let reveal = intro(ui, id);
    let plot = if o.axis {
        Rect::from_min_max(
            pos2(rect.left() + 52.0, rect.top() + 6.0),
            pos2(rect.right() - 6.0, rect.bottom() - 24.0),
        )
    } else {
        rect
    };
    let (mut lo, mut hi) = vals
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    if o.baseline_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    let (min, max, step) = if o.axis {
        axis(lo, hi)
    } else {
        let pad = ((hi - lo) * 0.12).max(1.0);
        (lo - if o.baseline_zero { 0.0 } else { pad }, hi + pad, 1.0)
    };
    let range = (max - min).max(1e-6);
    let n = vals.len();
    let xs: Vec<f32> = (0..n)
        .map(|i| plot.left() + plot.width() * i as f32 / (n - 1) as f32)
        .collect();
    let pts: Vec<Pos2> = vals
        .iter()
        .zip(&xs)
        .map(|(v, x)| pos2(*x, plot.bottom() - (v - min) / range * plot.height()))
        .collect();
    if o.axis {
        grid(ui, t, plot, max, step, min, o.fmt);
        x_labels(ui, t, plot, o.labels, &xs, o.label_every);
    }
    let curve = smooth(&pts, 10);
    let clip = Rect::from_min_max(
        plot.min - vec2(4.0, 8.0),
        pos2(plot.left() + (plot.width() + 8.0) * reveal, plot.bottom() + 4.0),
    );
    let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));

    // Gradient fill: a triangle strip from the curve down to the baseline,
    // fading from 28% to 0% alpha.
    let base_y = if o.baseline_zero && min < 0.0 {
        plot.bottom() - (0.0 - min) / range * plot.height()
    } else {
        plot.bottom()
    };
    let mut mesh = Mesh::default();
    let top_c = motion::with_alpha(o.color, if t.dark { 0.30 } else { 0.22 });
    let bot_c = motion::with_alpha(o.color, 0.0);
    for p in &curve {
        let i = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: *p,
            uv: WHITE_UV,
            color: top_c,
        });
        mesh.vertices.push(Vertex {
            pos: pos2(p.x, base_y),
            uv: WHITE_UV,
            color: bot_c,
        });
        if i >= 2 {
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i + 1, i);
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.add(PathShape::line(curve, PathStroke::new(2.2, o.color)));

    // Hover crosshair.
    let resp = ui.interact(plot, id.with("hover"), Sense::hover());
    if let Some(pos) = resp.hover_pos() {
        let i = (((pos.x - plot.left()) / plot.width()) * (n - 1) as f32)
            .round()
            .clamp(0.0, (n - 1) as f32) as usize;
        let pt = pts[i];
        let p = ui.painter();
        p.vline(pt.x, plot.y_range(), Stroke::new(1.0, motion::with_alpha(t.text3, 0.6)));
        p.circle_filled(pt, 6.0, motion::with_alpha(o.color, 0.25));
        p.circle(pt, 3.8, t.card, Stroke::new(2.0, o.color));
        let label = o.labels.get(i).cloned().unwrap_or_default();
        tooltip(ui, t, id, pt, &label, &[(o.color, (o.fmt)(values[i]))]);
    }
}

/// Minimal sparkline with a soft fill.
pub fn sparkline(ui: &Ui, id: Id, rect: Rect, values: &[f32], color: Color32) {
    if values.len() < 2 {
        return;
    }
    let vals = animated(ui, id, values, motion::CHART);
    let (lo, hi) = vals
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let range = (hi - lo).max(1e-6);
    let n = vals.len();
    let pts: Vec<Pos2> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            pos2(
                rect.left() + rect.width() * i as f32 / (n - 1) as f32,
                rect.bottom() - (v - lo) / range * rect.height(),
            )
        })
        .collect();
    let curve = smooth(&pts, 4);
    let mut mesh = Mesh::default();
    let top_c = motion::with_alpha(color, 0.22);
    for p in &curve {
        let i = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: *p,
            uv: WHITE_UV,
            color: top_c,
        });
        mesh.vertices.push(Vertex {
            pos: pos2(p.x, rect.bottom()),
            uv: WHITE_UV,
            color: Color32::TRANSPARENT,
        });
        if i >= 2 {
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i + 1, i);
        }
    }
    let painter = ui.painter();
    painter.add(Shape::mesh(mesh));
    painter.add(PathShape::line(curve, PathStroke::new(1.6, color)));
    if let Some(last) = pts.last() {
        painter.circle_filled(*last, 2.6, color);
    }
}

// -------------------------------------------------------------- bars

pub struct BarGroup {
    pub label: String,
    pub values: Vec<f32>,
}

/// Grouped bars (e.g. income vs expense per month) with an optional net
/// line drawn through the groups.
#[allow(clippy::too_many_arguments)]
pub fn grouped_bars(
    ui: &mut Ui,
    t: &Theme,
    id: Id,
    rect: Rect,
    groups: &[BarGroup],
    colors: &[Color32],
    names: &[&str],
    net: Option<(&[f32], Color32)>,
    fmt: &dyn Fn(f32) -> String,
) {
    if groups.is_empty() {
        return;
    }
    let series = colors.len();
    let flat: Vec<f32> = groups.iter().flat_map(|g| g.values.iter().copied()).collect();
    let vals = animated(ui, id, &flat, motion::CHART);
    let hi = flat.iter().copied().fold(0.0f32, f32::max);
    let lo = net
        .map(|(n, _)| n.iter().copied().fold(0.0f32, f32::min))
        .unwrap_or(0.0);
    let plot = Rect::from_min_max(
        pos2(rect.left() + 52.0, rect.top() + 6.0),
        pos2(rect.right() - 6.0, rect.bottom() - 24.0),
    );
    crate::marks::record(|| format!("chart:{id:?}"), rect);
    let (min, max, step) = axis(lo.min(0.0), hi.max(0.0));
    let range = max - min;
    grid(ui, t, plot, max, step, min, fmt);
    let zero_y = plot.bottom() - (0.0 - min) / range * plot.height();
    let n = groups.len();
    let slot = plot.width() / n as f32;
    let bar_w = ((slot * 0.62) / series as f32).min(18.0);
    let gap = 3.0;
    let group_w = bar_w * series as f32 + gap * (series as f32 - 1.0);
    let xs: Vec<f32> = (0..n).map(|i| plot.left() + slot * (i as f32 + 0.5)).collect();
    let hover = ui.interact(plot, id.with("hover"), Sense::hover()).hover_pos();
    let hovered = hover.map(|p| (((p.x - plot.left()) / slot).floor() as usize).min(n - 1));
    let p = ui.painter();
    if let Some(h) = hovered {
        let r = Rect::from_center_size(pos2(xs[h], plot.center().y), vec2(slot * 0.92, plot.height() + 8.0));
        p.rect_filled(r, CornerRadius::same(8), motion::with_alpha(t.hover, 0.9));
    }
    for (gi, x) in xs.iter().enumerate() {
        for s in 0..series {
            let v = vals[gi * series + s].max(0.0);
            let h = v / range * plot.height();
            let left = x - group_w / 2.0 + s as f32 * (bar_w + gap);
            let r = Rect::from_min_max(pos2(left, zero_y - h), pos2(left + bar_w, zero_y));
            let dim = hovered.is_some_and(|hh| hh != gi);
            let c = if dim {
                motion::with_alpha(colors[s], 0.45)
            } else {
                colors[s]
            };
            let rr = (bar_w / 2.0).min(5.0) as u8;
            p.rect_filled(
                r,
                CornerRadius {
                    nw: rr,
                    ne: rr,
                    sw: 1,
                    se: 1,
                },
                c,
            );
        }
    }
    if let Some((net, color)) = net {
        let nv = animated(ui, id.with("net"), net, motion::CHART);
        let pts: Vec<Pos2> = nv
            .iter()
            .zip(&xs)
            .map(|(v, x)| pos2(*x, plot.bottom() - (v - min) / range * plot.height()))
            .collect();
        p.add(PathShape::line(smooth(&pts, 8), PathStroke::new(2.0, color)));
        for pt in &pts {
            p.circle(*pt, 3.0, t.card, Stroke::new(1.6, color));
        }
    }
    let labels: Vec<String> = groups.iter().map(|g| g.label.clone()).collect();
    x_labels(
        ui,
        t,
        plot,
        &labels,
        &xs,
        if n > 14 {
            3
        } else if n > 8 {
            2
        } else {
            1
        },
    );
    if let Some(h) = hovered {
        let mut lines: Vec<(Color32, String)> = (0..series)
            .map(|s| (colors[s], format!("{}  {}", names[s], fmt(groups[h].values[s]))))
            .collect();
        if let Some((net, c)) = net {
            lines.push((c, format!("Net  {}", fmt(net[h]))));
        }
        tooltip(
            ui,
            t,
            id,
            pos2(xs[h], hover.map(|p| p.y).unwrap_or(plot.center().y)),
            &groups[h].label,
            &lines,
        );
    }
}

/// Stacked bars — one bar per label, one segment per series.
pub fn stacked_bars(
    ui: &mut Ui,
    t: &Theme,
    id: Id,
    rect: Rect,
    labels: &[String],
    series: &[(String, Color32, Vec<f32>)],
    fmt: &dyn Fn(f32) -> String,
) {
    let n = labels.len();
    if n == 0 || series.is_empty() {
        return;
    }
    let flat: Vec<f32> = series.iter().flat_map(|s| s.2.iter().copied()).collect();
    let vals = animated(ui, id, &flat, motion::CHART);
    let totals: Vec<f32> = (0..n).map(|i| series.iter().map(|s| s.2[i].max(0.0)).sum()).collect();
    let plot = Rect::from_min_max(
        pos2(rect.left() + 52.0, rect.top() + 6.0),
        pos2(rect.right() - 6.0, rect.bottom() - 24.0),
    );
    let (max, step) = nice_scale(totals.iter().copied().fold(1.0, f32::max), 4);
    grid(ui, t, plot, max, step, 0.0, fmt);
    let slot = plot.width() / n as f32;
    let bar_w = (slot * 0.56).min(34.0);
    let xs: Vec<f32> = (0..n).map(|i| plot.left() + slot * (i as f32 + 0.5)).collect();
    let hover = ui.interact(plot, id.with("hover"), Sense::hover()).hover_pos();
    let hovered = hover.map(|p| (((p.x - plot.left()) / slot).floor() as usize).min(n - 1));
    let p = ui.painter();
    if let Some(h) = hovered {
        let r = Rect::from_center_size(pos2(xs[h], plot.center().y), vec2(slot * 0.92, plot.height() + 8.0));
        p.rect_filled(r, CornerRadius::same(8), t.hover);
    }
    for (i, x) in xs.iter().enumerate() {
        let mut y = plot.bottom();
        let top_series = (0..series.len()).rev().find(|s| vals[s * n + i] > 0.0);
        for s in 0..series.len() {
            let v = vals[s * n + i].max(0.0);
            if v <= 0.0 {
                continue;
            }
            let h = v / max * plot.height();
            let r = Rect::from_min_max(pos2(x - bar_w / 2.0, y - h), pos2(x + bar_w / 2.0, y));
            let rr: u8 = if Some(s) == top_series { 5 } else { 0 };
            let c = if hovered.is_some_and(|hh| hh != i) {
                motion::with_alpha(series[s].1, 0.5)
            } else {
                series[s].1
            };
            p.rect_filled(
                r,
                CornerRadius {
                    nw: rr,
                    ne: rr,
                    sw: 0,
                    se: 0,
                },
                c,
            );
            // hairline separator between segments
            p.hline(r.x_range(), r.top(), Stroke::new(1.0, t.card));
            y -= h;
        }
    }
    x_labels(
        ui,
        t,
        plot,
        labels,
        &xs,
        if n > 14 {
            3
        } else if n > 8 {
            2
        } else {
            1
        },
    );
    if let Some(h) = hovered {
        let mut lines: Vec<(Color32, String)> = series
            .iter()
            .rev()
            .filter(|s| s.2[h] > 0.0)
            .map(|s| (s.1, format!("{}  {}", s.0, fmt(s.2[h]))))
            .collect();
        lines.push((t.text3, format!("Total  {}", fmt(totals[h]))));
        tooltip(
            ui,
            t,
            id,
            pos2(xs[h], hover.map(|p| p.y).unwrap_or(0.0)),
            &labels[h],
            &lines,
        );
    }
}

// ------------------------------------------------------------- donut

pub struct Slice {
    pub value: f32,
    pub color: Color32,
    pub label: String,
}

fn arc_points(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let steps = (((a1 - a0).abs() / TAU) * 96.0).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Donut chart; returns the hovered slice index. Slices sweep in and the
/// hovered one thickens.
pub fn donut(ui: &mut Ui, t: &Theme, id: Id, rect: Rect, slices: &[Slice], thickness: f32) -> Option<usize> {
    let total: f32 = slices.iter().map(|s| s.value.max(0.0)).sum();
    let center = rect.center();
    let radius = rect.width().min(rect.height()) / 2.0 - 8.0;
    let r_mid = radius - thickness / 2.0;
    let p = ui.painter();
    p.circle_stroke(center, r_mid, Stroke::new(thickness, t.hover));
    if total <= 0.0 {
        return None;
    }
    let values: Vec<f32> = slices.iter().map(|s| s.value.max(0.0) / total).collect();
    let fracs = animated(ui, id, &values, motion::CHART);
    let sweep = intro(ui, id);
    let resp = ui.interact(rect, id.with("hover"), Sense::hover());
    let hovered = resp.hover_pos().and_then(|pos| {
        let d = pos - center;
        let dist = d.length();
        if dist < radius - thickness - 6.0 || dist > radius + 8.0 {
            return None;
        }
        let mut ang = d.y.atan2(d.x) + PI / 2.0;
        if ang < 0.0 {
            ang += TAU;
        }
        let f = ang / TAU;
        let mut acc = 0.0;
        for (i, v) in fracs.iter().enumerate() {
            acc += v;
            if f <= acc {
                return Some(i);
            }
        }
        None
    });
    let gap = if slices.len() > 1 { 0.018 } else { 0.0 };
    let mut a = -PI / 2.0;
    for (i, (f, s)) in fracs.iter().zip(slices).enumerate() {
        let span = f * TAU * sweep;
        if span <= 0.0005 {
            continue;
        }
        let grow = motion::toggle(ui.ctx(), id.with(("slice", i)), hovered == Some(i), motion::MICRO);
        let th = thickness + 6.0 * grow;
        let (a0, a1) = (a + gap / 2.0, a + span - gap / 2.0);
        if a1 > a0 {
            let dim = hovered.is_some() && hovered != Some(i);
            let c = if dim {
                motion::with_alpha(s.color, 0.55)
            } else {
                s.color
            };
            let pts = arc_points(center, r_mid + 3.0 * grow, a0, a1);
            p.add(PathShape::line(pts, PathStroke::new(th, c)));
        }
        a += span;
    }
    hovered
}

/// Progress ring (goals, savings rate).
#[allow(clippy::too_many_arguments)]
pub fn ring(ui: &Ui, id: Id, center: Pos2, radius: f32, width: f32, frac: f32, color: Color32, track: Color32) {
    let f = motion::tween_from(ui.ctx(), id, 0.0, frac.clamp(0.0, 1.0), motion::CHART * 1.3);
    let p = ui.painter();
    p.circle_stroke(center, radius, Stroke::new(width, track));
    if f > 0.002 {
        let pts = arc_points(center, radius, -PI / 2.0, -PI / 2.0 + TAU * f);
        p.add(PathShape::line(pts.clone(), PathStroke::new(width, color)));
        // round caps
        p.circle_filled(pts[0], width / 2.0, color);
        if let Some(last) = pts.last() {
            p.circle_filled(*last, width / 2.0, color);
        }
    }
}

// ----------------------------------------------------------- heatmap

/// Calendar heatmap (weeks as columns, Monday at top) of daily values
/// starting at `start`.
pub fn heatmap(
    ui: &mut Ui,
    t: &Theme,
    id: Id,
    rect: Rect,
    start: jiff::civil::Date,
    values: &[i64],
    fmt: &dyn Fn(i64) -> String,
) {
    if values.is_empty() {
        return;
    }
    let offset = start.weekday().to_monday_zero_offset() as usize;
    let weeks = (values.len() + offset).div_ceil(7);
    let label_w = 28.0;
    let cell = ((rect.width() - label_w) / weeks as f32)
        .min((rect.height() - 18.0) / 7.0)
        .floor();
    let gap = (cell * 0.18).max(1.5);
    let mut sorted: Vec<i64> = values.iter().copied().filter(|v| *v > 0).collect();
    sorted.sort_unstable();
    let q = |p: f32| {
        sorted
            .get(((sorted.len() as f32 - 1.0) * p) as usize)
            .copied()
            .unwrap_or(1)
            .max(1)
    };
    let (q1, q2, q3) = (q(0.35), q(0.7), q(0.92));
    let level = |v: i64| -> f32 {
        if v <= 0 {
            0.0
        } else if v <= q1 {
            0.28
        } else if v <= q2 {
            0.5
        } else if v <= q3 {
            0.75
        } else {
            1.0
        }
    };
    let reveal = intro(ui, id);
    let p = ui.painter();
    let origin = pos2(rect.left() + label_w, rect.top() + 16.0);
    for (i, d) in ["Mon", "", "Wed", "", "Fri", "", ""].iter().enumerate() {
        if !d.is_empty() {
            p.text(
                pos2(rect.left(), origin.y + i as f32 * cell + cell / 2.0),
                Align2::LEFT_CENTER,
                *d,
                theme::regular(10.0),
                t.text3,
            );
        }
    }
    let mut last_month = 0;
    let hover = ui.interact(rect, id.with("hover"), Sense::hover()).hover_pos();
    let mut tip = None;
    for (i, v) in values.iter().enumerate() {
        let k = i + offset;
        let (col, row) = (k / 7, k % 7);
        let min = pos2(origin.x + col as f32 * cell, origin.y + row as f32 * cell);
        let r = Rect::from_min_size(min, vec2(cell - gap, cell - gap));
        let date = start.checked_add(jiff::Span::new().days(i as i64)).unwrap_or(start);
        if row == 0 || i == 0 {
            let m = date.month();
            if m != last_month && date.day() <= 7 {
                p.text(
                    pos2(min.x, rect.top()),
                    Align2::LEFT_TOP,
                    magpie_core::Month::of(date).short(),
                    theme::regular(10.0),
                    t.text3,
                );
                last_month = m;
            }
        }
        let appear = ((reveal * weeks as f32 * 1.4 - col as f32) / 3.0).clamp(0.0, 1.0);
        let lv = level(*v) * appear;
        let c = if lv <= 0.0 {
            t.hover
        } else {
            motion::lerp_color(t.tint(t.accent, 0.18), t.accent, lv)
        };
        p.rect_filled(r, CornerRadius::same((cell * 0.22) as u8), c);
        if let Some(h) = hover
            && r.expand(gap / 2.0).contains(h)
        {
            p.rect_stroke(
                r,
                CornerRadius::same((cell * 0.22) as u8),
                Stroke::new(1.5, t.text),
                StrokeKind::Outside,
            );
            tip = Some((r.center(), date, *v));
        }
    }
    if let Some((pos, d, v)) = tip {
        tooltip(
            ui,
            t,
            id,
            pos,
            &crate::widgets::fmt_date(d),
            &[(t.accent, if v > 0 { fmt(v) } else { "No spending".into() })],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_step_follows_span() {
        let (min, max, step) = axis(-4_400_000.0, 0.5);
        assert!(((max - min) / step) <= 8.0, "{min} {max} {step}");
        let (min, max, step) = axis(0.0, 0.0);
        assert!(max > min && step > 0.0);
        let (min, max, _) = axis(1200.0, 6300.0);
        assert!(min <= 1200.0 && max >= 6300.0);
    }

    #[test]
    fn smooth_never_overshoots() {
        let pts = [pos2(0.0, 0.0), pos2(1.0, 10.0), pos2(2.0, 10.0), pos2(3.0, 0.0)];
        assert!(smooth(&pts, 10).iter().all(|p| p.y >= -0.001 && p.y <= 10.001));
    }
}
