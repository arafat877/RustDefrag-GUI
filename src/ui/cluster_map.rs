/// cluster_map.rs — Animated disk cluster map.
/// Each cell animates through color states with smooth lerp transitions.
/// Scan line sweeps during analysis; cells glow green on defrag completion.

use egui::{Color32, Painter, Pos2, Rect, Rounding, Stroke, Vec2};
use crate::ui::theme::{
    self, lerp_color, ease_out_cubic, with_alpha, pulse, shimmer, state_color,
    COLOR_FREE, COLOR_USED, COLOR_FRAG, COLOR_DONE, COLOR_MOVING, COLOR_SYSTEM,
    BG_SURFACE, BORDER, TEXT_DIM,
};

pub const MAP_COLS: usize = 100;
pub const MAP_ROWS: usize = 25;

#[derive(Clone)]
pub struct ClusterCell {
    pub state:      u8,         // current logical state
    pub display:    Color32,    // current displayed color (animated)
    pub target:     Color32,    // color we're lerping toward
    pub anim_t:     f32,        // 0..1 animation progress
    pub glow:       f32,        // extra glow intensity (decays)
}

impl Default for ClusterCell {
    fn default() -> Self {
        Self {
            state: 0, display: COLOR_FREE, target: COLOR_FREE,
            anim_t: 1.0, glow: 0.0,
        }
    }
}

pub struct ClusterMap {
    pub cells:     Vec<ClusterCell>, // MAP_COLS × MAP_ROWS, row-major
    pub scan_col:  Option<f32>,      // fractional column for scan-line
    pub phase:     MapPhase,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MapPhase { Idle, Scanning, Defragging, Complete }

impl ClusterMap {
    pub fn new() -> Self {
        Self {
            cells:    vec![ClusterCell::default(); MAP_COLS * MAP_ROWS],
            scan_col: None,
            phase:    MapPhase::Idle,
        }
    }

    /// Update cell states from the bitmap map (row-major Vec<Vec<u8>>).
    pub fn apply_bitmap(&mut self, map: &[Vec<u8>]) {
        for (r, row) in map.iter().enumerate().take(MAP_ROWS) {
            for (c, &state) in row.iter().enumerate().take(MAP_COLS) {
                let idx = r * MAP_COLS + c;
                self.set_state(idx, state);
            }
        }
    }

    /// Apply a list of (lcn, new_state) cluster events from the defrag engine.
    pub fn apply_events(&mut self, events: &[(i64, u8)], total_clusters: i64) {
        if total_clusters <= 0 { return; }
        for &(lcn, state) in events {
            let frac = (lcn as f64 / total_clusters as f64).clamp(0.0, 0.9999);
            let cell_idx = (frac * (MAP_COLS * MAP_ROWS) as f64) as usize;
            if cell_idx < self.cells.len() {
                self.set_state(cell_idx, state);
                self.cells[cell_idx].glow = 1.0; // trigger glow
            }
        }
    }

    /// Set the scan-line position as a fraction 0..1 of scan progress.
    pub fn set_scan_progress(&mut self, frac: f32) {
        self.scan_col = Some(frac);
        // Color cells that the scan has passed
        let passed_cells = (frac * (MAP_COLS * MAP_ROWS) as f32) as usize;
        for (i, cell) in self.cells.iter_mut().enumerate().take(passed_cells) {
            if cell.state == 0 {
                cell.target  = theme::lerp_color(COLOR_FREE, COLOR_USED, 0.40);
                cell.anim_t  = 0.0;
            }
        }
    }

    fn set_state(&mut self, idx: usize, state: u8) {
        if idx >= self.cells.len() { return; }
        let cell = &mut self.cells[idx];
        if cell.state == state { return; }
        cell.state  = state;
        cell.target = state_color(state);
        cell.anim_t = 0.0;
    }

    /// Advance animations by dt seconds.
    pub fn tick(&mut self, dt: f32) {
        let speed = 6.0; // lerp speed (state transitions/sec)
        for cell in &mut self.cells {
            if cell.anim_t < 1.0 {
                cell.anim_t = (cell.anim_t + dt * speed).min(1.0);
                let t = ease_out_cubic(cell.anim_t);
                cell.display = lerp_color(cell.display, cell.target, t);
            } else {
                cell.display = cell.target;
            }
            if cell.glow > 0.0 {
                cell.glow = (cell.glow - dt * 1.5).max(0.0);
            }
        }
    }

    /// Draw the cluster map into the given rect.
    pub fn draw(&self, painter: &Painter, rect: Rect, time: f64) {
        // Glass background
        draw_glass_panel(painter, rect, 8.0);

        let inner = rect.shrink(4.0);
        let cell_w = inner.width()  / MAP_COLS as f32;
        let cell_h = inner.height() / MAP_ROWS as f32;
        let gap    = 0.8f32;

        for row in 0..MAP_ROWS {
            for col in 0..MAP_COLS {
                let idx  = row * MAP_COLS + col;
                let cell = &self.cells[idx];

                let x = inner.min.x + col as f32 * cell_w;
                let y = inner.min.y + row as f32 * cell_h;
                let cr = Rect::from_min_size(
                    Pos2::new(x + gap/2.0, y + gap/2.0),
                    Vec2::new((cell_w - gap).max(1.0), (cell_h - gap).max(1.0)),
                );

                // Base fill
                painter.rect_filled(cr, Rounding::same(1.0), cell.display);

                // Top-edge highlight for used/done cells (FRC glass bead)
                if cell.state >= 2 {
                    let top = Rect::from_min_size(cr.min, Vec2::new(cr.width(), 1.0));
                    painter.rect_filled(top, Rounding::ZERO, with_alpha(Color32::WHITE, 40));
                }

                // Glow ring on recently moved cells
                if cell.glow > 0.01 {
                    let a = (cell.glow * 180.0) as u8;
                    let glow_col = with_alpha(theme::COLOR_DONE_BRT, a);
                    painter.rect_stroke(cr.expand(1.5), Rounding::same(2.0), Stroke::new(1.0, glow_col));
                }
            }
        }

        // Scan line overlay (animated shimmer)
        if let Some(frac) = self.scan_col {
            let sx = inner.min.x + frac * inner.width();
            let alpha = (pulse(time, 2.0) * 200.0) as u8;
            let scan_col = Color32::from_rgba_premultiplied(120, 200, 255, alpha);
            painter.line_segment(
                [Pos2::new(sx, inner.min.y), Pos2::new(sx, inner.max.y)],
                Stroke::new(2.0, scan_col),
            );
            // Glow around scan line
            for dx in 1..=4 {
                let a = (alpha as f32 * (1.0 - dx as f32 / 5.0)) as u8;
                let gc = Color32::from_rgba_premultiplied(80, 160, 255, a);
                painter.line_segment(
                    [Pos2::new(sx + dx as f32, inner.min.y), Pos2::new(sx + dx as f32, inner.max.y)],
                    Stroke::new(1.0, gc),
                );
            }
        }

        // Active defrag shimmer sweep across bottom
        if self.phase == MapPhase::Defragging {
            let s = shimmer(time);
            let sx = inner.min.x + s * inner.width();
            let sw = inner.width() * 0.08;
            let shimmer_rect = Rect::from_min_size(
                Pos2::new(sx - sw / 2.0, inner.max.y - 3.0),
                Vec2::new(sw, 3.0),
            );
            painter.rect_filled(
                shimmer_rect,
                Rounding::same(1.5),
                Color32::from_rgba_premultiplied(40, 200, 100, 80),
            );
        }
    }
}

// ── Glass panel helper ───────────────────────────────────────────────────────

pub fn draw_glass_panel(painter: &Painter, rect: Rect, corner: f32) {
    let rounding = Rounding::same(corner);
    painter.rect_filled(rect, rounding, BG_SURFACE);
    // Border
    painter.rect_stroke(rect, rounding, Stroke::new(1.0, BORDER));
}
