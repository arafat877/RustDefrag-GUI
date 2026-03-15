/// charts.rs — Animated pie, bar, and line charts.
/// All drawn with egui Painter — no external chart crate.
/// Animations inspired by "Filthy Rich Client" (Haase & Guy):
/// segments sweep in, bars rise with bounce easing, lines draw themselves.

use egui::{Color32, FontId, Painter, Pos2, Rect, Rounding, Stroke, Vec2, FontFamily};
use std::f32::consts::{PI, TAU};
use crate::ui::theme::{
    self, ease_out_cubic, ease_out_bounce, ease_in_out_sine,
    with_alpha, TEXT_PRI, TEXT_SEC, TEXT_DIM, BG_SURFACE, BORDER,
    lerp_color,
};

// ── Pie chart ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PieSlice {
    pub label:  String,
    pub value:  f64,
    pub color:  Color32,
}

pub struct PieChart {
    pub slices:    Vec<PieSlice>,
    pub anim_t:    f32,   // 0..1 entrance animation
    pub hover_idx: Option<usize>,
}

impl PieChart {
    pub fn new(slices: Vec<PieSlice>) -> Self {
        Self { slices, anim_t: 0.0, hover_idx: None }
    }

    pub fn tick(&mut self, dt: f32) {
        self.anim_t = (self.anim_t + dt * 1.2).min(1.0);
    }

    pub fn draw(&self, painter: &Painter, rect: Rect, _time: f64) {
        let t = ease_out_cubic(self.anim_t);
        let center = rect.center();
        let radius = rect.size().min_elem() * 0.42;

        let total: f64 = self.slices.iter().map(|s| s.value).sum();
        if total == 0.0 { return; }

        let mut start_angle: f32 = -PI / 2.0; // top

        for (i, slice) in self.slices.iter().enumerate() {
            let frac   = (slice.value / total) as f32;
            let sweep  = TAU * frac * t;
            let end_a  = start_angle + sweep;
            let hovered = self.hover_idx == Some(i);
            let r = if hovered { radius * 1.06 } else { radius };

            // Fill the sector with many thin wedges (egui has no fill_arc)
            draw_sector(painter, center, r, start_angle, end_a, 64, slice.color, hovered);

            // Label line + text for slices > 5%
            if frac > 0.05 {
                let mid_angle = start_angle + sweep / 2.0;
                let lx = center.x + (r + 14.0) * mid_angle.cos();
                let ly = center.y + (r + 14.0) * mid_angle.sin();
                let pct_txt = format!("{:.1}%", frac * 100.0);
                painter.text(
                    Pos2::new(lx, ly), egui::Align2::CENTER_CENTER,
                    &pct_txt, FontId::new(9.5, FontFamily::Monospace),
                    with_alpha(slice.color, (220.0 * t) as u8),
                );
            }

            start_angle = end_a;
        }

        // Hole (donut)
        painter.circle_filled(center, radius * 0.38, BG_SURFACE);
        painter.circle_stroke(center, radius * 0.38, Stroke::new(1.0, BORDER));

        // Centre label
        painter.text(
            center, egui::Align2::CENTER_CENTER,
            "DISK", FontId::new(8.5, FontFamily::Monospace),
            with_alpha(TEXT_DIM, (180.0 * t) as u8),
        );
    }

    /// Draw legend below the chart
    pub fn draw_legend(&self, painter: &Painter, rect: Rect) {
        let total: f64 = self.slices.iter().map(|s| s.value).sum();
        let t = ease_out_cubic(self.anim_t);
        let mut y = rect.min.y;
        let row_h = 16.0;
        for slice in &self.slices {
            let pct = if total > 0.0 { slice.value / total * 100.0 } else { 0.0 };
            // Color swatch
            let swatch = Rect::from_min_size(Pos2::new(rect.min.x, y + 3.0), Vec2::new(10.0, 10.0));
            painter.rect_filled(swatch, Rounding::same(2.0), with_alpha(slice.color, (240.0*t) as u8));
            painter.text(
                Pos2::new(rect.min.x + 15.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER,
                &format!("{} ({:.1}%)", slice.label, pct),
                FontId::new(10.0, FontFamily::Proportional),
                with_alpha(TEXT_SEC, (200.0*t) as u8),
            );
            y += row_h;
        }
    }
}

fn draw_sector(
    painter: &Painter, center: Pos2, radius: f32,
    start: f32, end: f32, steps: usize,
    color: Color32, bright: bool,
) {
    let steps = steps.max(2);
    let da    = (end - start) / steps as f32;
    for i in 0..steps {
        let a0 = start + da * i as f32;
        let a1 = a0 + da;
        let p0 = Pos2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin());
        let p1 = Pos2::new(center.x + radius * a1.cos(), center.y + radius * a1.sin());
        // Draw triangle: center, p0, p1
        let alpha = if bright { 255u8 } else { 220u8 };
        let c = with_alpha(color, alpha);
        painter.add(egui::Shape::convex_polygon(vec![center, p0, p1], c, Stroke::NONE));

        // Bright top edge on each triangle (FRC light source effect)
        if bright {
            painter.line_segment([p0, p1], Stroke::new(1.0, with_alpha(Color32::WHITE, 60)));
        }
    }
    // Outer ring
    for i in 0..steps {
        let a0 = start + da * i as f32;
        let a1 = a0 + da;
        let p0 = Pos2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin());
        let p1 = Pos2::new(center.x + radius * a1.cos(), center.y + radius * a1.sin());
        painter.line_segment([p0, p1], Stroke::new(1.0, with_alpha(Color32::WHITE, 30)));
    }
}

// ── Bar chart ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BarSeries {
    pub label:  String,
    pub value:  f64,
    pub color:  Color32,
}

pub struct BarChart {
    pub series:  Vec<BarSeries>,
    pub title:   String,
    pub anim_t:  f32,
}

impl BarChart {
    pub fn new(title: impl Into<String>, series: Vec<BarSeries>) -> Self {
        Self { title: title.into(), series, anim_t: 0.0 }
    }

    pub fn tick(&mut self, dt: f32) { self.anim_t = (self.anim_t + dt * 0.9).min(1.0); }

    pub fn draw(&self, painter: &Painter, rect: Rect, _time: f64) {
        if self.series.is_empty() { return; }
        let t = ease_out_bounce(self.anim_t);

        let max_val = self.series.iter().map(|s| s.value).fold(0.0f64, f64::max).max(1.0);
        let n = self.series.len();
        let bar_w  = (rect.width() / (n as f32 * 1.4)).min(40.0);
        let gap    = bar_w * 0.4;
        let total_w = n as f32 * (bar_w + gap) - gap;
        let start_x = rect.center().x - total_w / 2.0;
        let base_y  = rect.max.y - 20.0;
        let max_h   = rect.height() - 36.0;

        // Horizontal grid lines
        for i in 0..=4 {
            let y = base_y - (i as f32 / 4.0) * max_h;
            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                Stroke::new(0.5, with_alpha(TEXT_DIM, 50)),
            );
        }

        for (i, bar) in self.series.iter().enumerate() {
            let h    = (bar.value / max_val) as f32 * max_h * t;
            let x    = start_x + i as f32 * (bar_w + gap);
            let br   = Rect::from_min_size(Pos2::new(x, base_y - h), Vec2::new(bar_w, h));

            // Shadow
            let shadow = br.translate(Vec2::new(2.0, 2.0));
            painter.rect_filled(shadow, Rounding::same(3.0), with_alpha(Color32::BLACK, 60));

            // Bar body
            painter.rect_filled(br, Rounding::same(3.0), bar.color);

            // Top highlight (FRC glass bead)
            let top = Rect::from_min_size(br.min, Vec2::new(br.width(), 4.0));
            painter.rect_filled(top, Rounding::same(3.0), with_alpha(Color32::WHITE, 70));

            // Value label above bar
            if h > 18.0 {
                painter.text(
                    Pos2::new(br.center().x, br.min.y - 10.0),
                    egui::Align2::CENTER_CENTER,
                    &compact_num(bar.value),
                    FontId::new(9.0, FontFamily::Monospace),
                    TEXT_SEC,
                );
            }

            // X-axis label
            painter.text(
                Pos2::new(br.center().x, base_y + 10.0),
                egui::Align2::CENTER_CENTER,
                &bar.label,
                FontId::new(8.5, FontFamily::Proportional),
                TEXT_DIM,
            );
        }
    }
}

// ── Line chart ───────────────────────────────────────────────────────────────

pub struct LineChart {
    pub points:  Vec<f64>,    // y values
    pub label:   String,
    pub color:   Color32,
    pub anim_t:  f32,         // draw animation progress
    pub unit:    String,
}

impl LineChart {
    pub fn new(label: impl Into<String>, color: Color32, unit: impl Into<String>) -> Self {
        Self { points: Vec::new(), label: label.into(), color, anim_t: 0.0, unit: unit.into() }
    }

    pub fn push(&mut self, v: f64) {
        self.points.push(v);
        self.anim_t = 0.0; // re-trigger draw animation on new point
    }

    pub fn tick(&mut self, dt: f32) { self.anim_t = (self.anim_t + dt * 1.8).min(1.0); }

    pub fn draw(&self, painter: &Painter, rect: Rect, _time: f64) {
        if self.points.len() < 2 { return; }
        let t = ease_in_out_sine(self.anim_t);
        let visible = ((self.points.len() as f32 * t) as usize).max(2).min(self.points.len());

        let max_y = self.points.iter().cloned().fold(0.0f64, f64::max).max(1.0);
        let min_y = 0.0f64;

        let to_pos = |i: usize, v: f64| -> Pos2 {
            let x = rect.min.x + (i as f32 / (self.points.len() - 1).max(1) as f32) * rect.width();
            let y = rect.max.y - ((v - min_y) / (max_y - min_y)) as f32 * rect.height();
            Pos2::new(x, y.clamp(rect.min.y, rect.max.y))
        };

        // Area fill (multiple alpha layers for glow)
        let pts: Vec<Pos2> = (0..visible).map(|i| to_pos(i, self.points[i])).collect();
        if pts.len() >= 2 {
            // Fill polygon
            let mut fill_pts = pts.clone();
            fill_pts.push(Pos2::new(pts.last().unwrap().x, rect.max.y));
            fill_pts.push(Pos2::new(pts[0].x, rect.max.y));
            painter.add(egui::Shape::convex_polygon(
                fill_pts, with_alpha(self.color, 28), Stroke::NONE,
            ));

            // Line
            for i in 0..pts.len() - 1 {
                painter.line_segment(
                    [pts[i], pts[i + 1]],
                    Stroke::new(1.8, self.color),
                );
                // Glow duplicate
                painter.line_segment(
                    [pts[i], pts[i + 1]],
                    Stroke::new(4.0, with_alpha(self.color, 30)),
                );
            }

            // Dot at last point
            let last = *pts.last().unwrap();
            painter.circle_filled(last, 3.5, self.color);
            painter.circle_filled(last, 5.5, with_alpha(self.color, 50));

            // Y-axis labels
            for i in 0..=3 {
                let v = min_y + (max_y - min_y) * i as f64 / 3.0;
                let y = rect.max.y - (i as f32 / 3.0) * rect.height();
                painter.text(
                    Pos2::new(rect.min.x - 2.0, y),
                    egui::Align2::RIGHT_CENTER,
                    &compact_num(v),
                    FontId::new(8.5, FontFamily::Monospace),
                    TEXT_DIM,
                );
            }
        }
    }
}

// ── Fragment histogram ───────────────────────────────────────────────────────

pub struct FragHistogram {
    pub data:   Vec<u64>,   // index = fragment count (0..50), value = file count
    pub anim_t: f32,
}

impl FragHistogram {
    pub fn new() -> Self { Self { data: vec![0; 51], anim_t: 0.0 } }
    pub fn tick(&mut self, dt: f32) { self.anim_t = (self.anim_t + dt * 0.8).min(1.0); }

    pub fn draw(&self, painter: &Painter, rect: Rect, _time: f64) {
        let t = ease_out_cubic(self.anim_t);
        let max_v = self.data.iter().cloned().max().unwrap_or(1).max(1) as f64;
        let n = self.data.len();
        let bw = rect.width() / n as f32;
        let base_y = rect.max.y;
        let max_h  = rect.height() - 16.0;

        for (i, &v) in self.data.iter().enumerate() {
            if v == 0 { continue; }
            let h  = (v as f64 / max_v) as f32 * max_h * t;
            let x  = rect.min.x + i as f32 * bw;
            let c  = lerp_color(
                Color32::from_rgb(20, 80, 180),
                Color32::from_rgb(200, 40, 40),
                (i as f32 / n as f32).powf(0.5),
            );
            let br = Rect::from_min_size(Pos2::new(x + 0.5, base_y - h), Vec2::new((bw - 1.0).max(1.0), h));
            painter.rect_filled(br, Rounding::same(1.0), c);
        }

        // X-axis labels (1, 5, 10, 20, 50+)
        for &tick in &[1usize, 5, 10, 20, 50] {
            if tick < n {
                let x = rect.min.x + tick as f32 * bw + bw / 2.0;
                painter.text(
                    Pos2::new(x, base_y + 8.0),
                    egui::Align2::CENTER_CENTER,
                    &tick.to_string(),
                    FontId::new(8.0, FontFamily::Monospace),
                    TEXT_DIM,
                );
            }
        }
    }
}

// ── Utility ──────────────────────────────────────────────────────────────────

fn compact_num(v: f64) -> String {
    if v >= 1_000_000.0 { format!("{:.1}M", v / 1_000_000.0) }
    else if v >= 1_000.0 { format!("{:.1}K", v / 1_000.0) }
    else { format!("{:.0}", v) }
}
