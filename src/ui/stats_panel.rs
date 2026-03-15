/// stats_panel.rs — Animated statistics panels with FRC-style glass cards.
/// Each numeric value animates from 0 to its target (counter-roll effect).

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Rounding, Stroke, Vec2};
use crate::ui::theme::*;
use crate::ui::cluster_map::draw_glass_panel;

// ── Animated counter ─────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct AnimCounter {
    pub target:  f64,
    pub current: f64,
}

impl AnimCounter {
    pub fn set(&mut self, v: f64) { self.target = v; }
    pub fn tick(&mut self, dt: f32) {
        let diff = self.target - self.current;
        if diff.abs() < 0.5 { self.current = self.target; return; }
        self.current += diff * (dt * 8.0) as f64;
    }
    pub fn val(&self) -> f64 { self.current }
}

// ── Volume stats panel ────────────────────────────────────────────────────────

pub struct VolumeStatsPanel {
    pub total_bytes:     AnimCounter,
    pub free_bytes:      AnimCounter,
    pub cluster_size:    u64,
    pub filesystem:      String,
    pub label:           String,
    pub anim_t:          f32,
}

impl Default for VolumeStatsPanel {
    fn default() -> Self {
        Self {
            total_bytes:  AnimCounter::default(),
            free_bytes:   AnimCounter::default(),
            cluster_size: 4096,
            filesystem:   "NTFS".into(),
            label:        "—".into(),
            anim_t:       0.0,
        }
    }
}

impl VolumeStatsPanel {
    pub fn tick(&mut self, dt: f32) {
        self.anim_t = (self.anim_t + dt * 1.5).min(1.0);
        self.total_bytes.tick(dt);
        self.free_bytes.tick(dt);
    }

    pub fn draw(&self, painter: &Painter, rect: Rect) {
        draw_glass_panel(painter, rect, 8.0);
        let t = ease_out_cubic(self.anim_t);

        let items: &[(&str, String, Color32)] = &[
            ("Volume",      self.label.clone(),                            TEXT_PRI),
            ("Filesystem",  self.filesystem.clone(),                       ACCENT),
            ("Cluster",     fmt_bytes(self.cluster_size),                  TEXT_SEC),
            ("Total",       fmt_bytes(self.total_bytes.val() as u64),      TEXT_SEC),
            ("Free",        fmt_bytes(self.free_bytes.val() as u64),       GREEN),
            ("Used",        {
                let u = self.total_bytes.val() - self.free_bytes.val();
                let p = if self.total_bytes.val() > 0.0 { u / self.total_bytes.val() * 100.0 } else { 0.0 };
                format!("{:.1}%", p)
            }, AMBER),
        ];

        let row_h = rect.height() / items.len() as f32;
        for (i, (k, v, vc)) in items.iter().enumerate() {
            let y = rect.min.y + i as f32 * row_h + row_h * 0.5;
            let key_pos = Pos2::new(rect.min.x + 10.0, y);
            let key_color = with_alpha(TEXT_LABEL, (200.0 * t) as u8);
            if *k == "Volume" {
                let font = FontId::new(9.5, FontFamily::Proportional);
                painter.text(key_pos, egui::Align2::LEFT_CENTER, "VOLUME", font.clone(), key_color);
                painter.text(key_pos + Vec2::new(0.6, 0.0), egui::Align2::LEFT_CENTER, "VOLUME", font, key_color);
            } else {
                painter.text(
                    key_pos, egui::Align2::LEFT_CENTER,
                    k, FontId::new(9.5, FontFamily::Proportional),
                    key_color,
                );
            }
            painter.text(
                Pos2::new(rect.max.x - 8.0, y), egui::Align2::RIGHT_CENTER,
                v, FontId::new(10.0, FontFamily::Monospace),
                with_alpha(*vc, (220.0 * t) as u8),
            );
        }
        // Separator lines
        for i in 1..items.len() {
            let y = rect.min.y + i as f32 * row_h;
            painter.line_segment(
                [Pos2::new(rect.min.x + 8.0, y), Pos2::new(rect.max.x - 8.0, y)],
                Stroke::new(0.5, with_alpha(TEXT_LABEL, 40)),
            );
        }
    }
}

// ── Analysis report panel ─────────────────────────────────────────────────────

pub struct AnalysisPanel {
    pub total_files:      AnimCounter,
    pub fragmented_files: AnimCounter,
    pub total_frags:      AnimCounter,
    pub avg_frags:        f64,
    pub worst_file:       String,
    pub worst_count:      AnimCounter,
    pub frag_pct:         f64,
    pub anim_t:           f32,
}

impl Default for AnalysisPanel {
    fn default() -> Self {
        Self {
            total_files:      AnimCounter::default(),
            fragmented_files: AnimCounter::default(),
            total_frags:      AnimCounter::default(),
            avg_frags:        0.0,
            worst_file:       String::new(),
            worst_count:      AnimCounter::default(),
            frag_pct:         0.0,
            anim_t:           0.0,
        }
    }
}

impl AnalysisPanel {
    pub fn tick(&mut self, dt: f32) {
        self.anim_t = (self.anim_t + dt * 1.2).min(1.0);
        self.total_files.tick(dt);
        self.fragmented_files.tick(dt);
        self.total_frags.tick(dt);
        self.worst_count.tick(dt);
    }

    pub fn draw(&self, painter: &Painter, rect: Rect) {
        draw_glass_panel(painter, rect, 8.0);
        let t = ease_out_cubic(self.anim_t);
        let inner = rect.shrink(8.0);

        draw_bold_title(
            painter,
            Pos2::new(inner.min.x, inner.min.y + 10.0),
            "FRAGMENT REPORT",
            with_alpha(TEXT_LABEL, (180.0 * t) as u8),
        );

        let items: &[(&str, String, Color32)] = &[
            ("Total files",      fmt_large(self.total_files.val() as u64),      TEXT_SEC),
            ("Fragmented",       format!("{} ({:.1}%)", fmt_large(self.fragmented_files.val() as u64), self.frag_pct), AMBER),
            ("Total fragments",  fmt_large(self.total_frags.val() as u64),      TEXT_SEC),
            ("Avg frags/file",   format!("{:.3}", self.avg_frags),               TEXT_SEC),
            ("Most fragmented",  self.worst_file.clone(),                        RED),
            ("Max fragments",    fmt_large(self.worst_count.val() as u64),       RED),
        ];

        let start_y = inner.min.y + 24.0;
        let row_h   = (inner.height() - 28.0) / items.len() as f32;

        for (i, (k, v, vc)) in items.iter().enumerate() {
            let delay = i as f32 * 0.06;
            let ti = ease_out_cubic(((self.anim_t - delay) / (1.0 - delay + 0.001)).clamp(0.0, 1.0));
            let y = start_y + i as f32 * row_h + row_h * 0.5;
            painter.text(
                Pos2::new(inner.min.x + 4.0, y), egui::Align2::LEFT_CENTER,
                k, FontId::new(9.5, FontFamily::Proportional),
                with_alpha(TEXT_LABEL, (200.0 * ti) as u8),
            );
            painter.text(
                Pos2::new(inner.max.x - 4.0, y), egui::Align2::RIGHT_CENTER,
                v, FontId::new(9.5, FontFamily::Monospace),
                with_alpha(*vc, (230.0 * ti) as u8),
            );
        }

        for i in 1..items.len() {
            let y = start_y + i as f32 * row_h;
            painter.line_segment(
                [Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)],
                Stroke::new(0.5, with_alpha(TEXT_LABEL, 40)),
            );
        }
    }
}

// ── Defrag results panel ──────────────────────────────────────────────────────

pub struct DefragPanel {
    pub attempted:   AnimCounter,
    pub defragged:   AnimCounter,
    pub skipped:     AnimCounter,
    pub in_use:      AnimCounter,
    pub whitelisted: AnimCounter,
    pub clusters:    AnimCounter,
    pub bytes_moved: AnimCounter,
    pub boot_queued: AnimCounter,
    pub cluster_size:u64,
    pub anim_t:      f32,
}

impl Default for DefragPanel {
    fn default() -> Self {
        Self {
            attempted:    AnimCounter::default(),
            defragged:    AnimCounter::default(),
            skipped:      AnimCounter::default(),
            in_use:       AnimCounter::default(),
            whitelisted:  AnimCounter::default(),
            clusters:     AnimCounter::default(),
            bytes_moved:  AnimCounter::default(),
            boot_queued:  AnimCounter::default(),
            cluster_size: 4096,
            anim_t:       0.0,
        }
    }
}

impl DefragPanel {
    pub fn tick(&mut self, dt: f32) {
        self.anim_t = (self.anim_t + dt * 1.2).min(1.0);
        self.attempted.tick(dt);
        self.defragged.tick(dt);
        self.skipped.tick(dt);
        self.in_use.tick(dt);
        self.whitelisted.tick(dt);
        self.clusters.tick(dt);
        self.bytes_moved.tick(dt);
        self.boot_queued.tick(dt);
    }

    pub fn draw(&self, painter: &Painter, rect: Rect) {
        draw_glass_panel(painter, rect, 8.0);
        let t = ease_out_cubic(self.anim_t);
        let inner = rect.shrink(8.0);

        draw_bold_title(
            painter,
            Pos2::new(inner.min.x, inner.min.y + 10.0),
            "DEFRAGMENTATION RESULTS",
            with_alpha(TEXT_LABEL, (180.0 * t) as u8),
        );

        let items: &[(&str, String, Color32)] = &[
            ("Files attempted",  fmt_large(self.attempted.val() as u64),    TEXT_SEC),
            ("Files defragged",  fmt_large(self.defragged.val() as u64),    GREEN),
            ("Files skipped",    fmt_large(self.skipped.val() as u64),      TEXT_SEC),
            ("Files in use",     fmt_large(self.in_use.val() as u64),       AMBER),
            ("Whitelisted",      fmt_large(self.whitelisted.val() as u64),  TEXT_DIM),
            ("Boot queued",      fmt_large(self.boot_queued.val() as u64),  ACCENT),
            ("Clusters moved",   fmt_large(self.clusters.val() as u64),     GREEN),
            ("Data moved",       fmt_bytes(self.bytes_moved.val() as u64),  GREEN),
        ];

        let start_y = inner.min.y + 24.0;
        let row_h   = (inner.height() - 28.0) / items.len() as f32;

        for (i, (k, v, vc)) in items.iter().enumerate() {
            let delay = i as f32 * 0.04;
            let ti = ease_out_cubic(((self.anim_t - delay) / (1.0 - delay + 0.001)).clamp(0.0, 1.0));
            let y = start_y + i as f32 * row_h + row_h * 0.5;
            painter.text(Pos2::new(inner.min.x+4.0, y), egui::Align2::LEFT_CENTER,
                k, FontId::new(9.5, FontFamily::Proportional), with_alpha(TEXT_LABEL, (200.0*ti) as u8));
            painter.text(Pos2::new(inner.max.x-4.0, y), egui::Align2::RIGHT_CENTER,
                v, FontId::new(9.5, FontFamily::Monospace), with_alpha(*vc, (230.0*ti) as u8));
        }

        for i in 1..items.len() {
            let y = start_y + i as f32 * row_h;
            painter.line_segment(
                [Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)],
                Stroke::new(0.5, with_alpha(TEXT_LABEL, 40)),
            );
        }
    }
}

// ── Metric card (large number + label) ───────────────────────────────────────

pub struct MetricCard {
    pub label:  String,
    pub value:  AnimCounter,
    pub unit:   String,
    pub color:  Color32,
    pub anim_t: f32,
}

impl MetricCard {
    pub fn new(label: &str, color: Color32) -> Self {
        Self {
            label: label.to_string(),
            value: AnimCounter::default(),
            unit: String::new(),
            color,
            anim_t: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.anim_t = (self.anim_t + dt * 1.5).min(1.0);
        self.value.tick(dt);
    }

    pub fn draw(&self, painter: &Painter, rect: Rect) {
        draw_glass_panel(painter, rect, 8.0);
        let t = ease_out_bounce(self.anim_t);

        let cy = rect.center().y;
        let cx = rect.center().x;

        // Glow circle behind the number
        let gr = rect.size().min_elem() * 0.28 * t;
        painter.circle_filled(Pos2::new(cx, cy - 4.0), gr, with_alpha(self.color, 14));

        // Value
        let val_str = fmt_large(self.value.val() as u64);
        painter.text(
            Pos2::new(cx, cy - 2.0), egui::Align2::CENTER_CENTER,
            &val_str, FontId::new(20.0, FontFamily::Monospace),
            with_alpha(self.color, (240.0 * t) as u8),
        );

        // Label
        painter.text(
            Pos2::new(cx, rect.max.y - 10.0), egui::Align2::CENTER_CENTER,
            &self.label, FontId::new(8.5, FontFamily::Proportional),
            with_alpha(TEXT_LABEL, (180.0 * t) as u8),
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_large(v: u64) -> String {
    if v >= 1_000_000 { format!("{:.1}M", v as f64 / 1_000_000.0) }
    else if v >= 1_000 { format!("{:.1}K", v as f64 / 1_000.0) }
    else { v.to_string() }
}

fn draw_bold_title(painter: &Painter, pos: Pos2, text: &str, color: Color32) {
    let font = FontId::new(8.5, FontFamily::Monospace);
    painter.text(pos, egui::Align2::LEFT_TOP, text, font.clone(), color);
    painter.text(pos + Vec2::new(0.7, 0.0), egui::Align2::LEFT_TOP, text, font, color);
}
