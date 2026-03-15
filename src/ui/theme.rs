/// theme.rs — Color palette, easing functions, and animation helpers.
/// Inspired by "Filthy Rich Client" (Haase & Guy):
/// rich depth, glass panels, animated glows.

use egui::Color32;

// ── Cell-state colors ────────────────────────────────────────────────────────

pub const COLOR_FREE:     Color32 = Color32::from_rgb(240, 242, 246);
pub const COLOR_SYSTEM:   Color32 = Color32::from_rgb(235, 200, 70);
pub const COLOR_USED:     Color32 = Color32::from_rgb(88, 145, 210);
pub const COLOR_FRAG:     Color32 = Color32::from_rgb(232, 117, 110);
pub const COLOR_MOVING:   Color32 = Color32::from_rgb(90, 188, 92);
pub const COLOR_DONE:     Color32 = Color32::from_rgb(90, 188, 92);

pub const COLOR_FREE_BRT:   Color32 = Color32::from_rgb(6,   16,  30);
pub const COLOR_USED_BRT:   Color32 = Color32::from_rgb(30, 100, 220);
pub const COLOR_FRAG_BRT:   Color32 = Color32::from_rgb(240, 60,  60);
pub const COLOR_MOVING_BRT: Color32 = Color32::from_rgb(255, 200,  0);
pub const COLOR_DONE_BRT:   Color32 = Color32::from_rgb(40,  210, 80);

pub fn state_color(state: u8) -> Color32 {
    match state {
        0 => COLOR_FREE,
        1 => COLOR_SYSTEM,
        2 => COLOR_USED,
        3 => COLOR_FRAG,
        4 => COLOR_MOVING,
        5 => COLOR_DONE,
        _ => COLOR_FREE,
    }
}

// ── App palette ──────────────────────────────────────────────────────────────

pub const BG_APP:      Color32 = Color32::from_rgb( 7,  16,  30);
pub const BG_PANEL:    Color32 = Color32::from_rgb(10,  22,  44);
pub const BG_SURFACE:  Color32 = Color32::from_rgb(13,  28,  56);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(16,  35,  70);

pub const ACCENT:      Color32 = Color32::from_rgb(40, 120, 240);
pub const ACCENT_DIM:  Color32 = Color32::from_rgb(18,  60, 140);
pub const GREEN:       Color32 = Color32::from_rgb(22, 160,  60);
pub const AMBER:       Color32 = Color32::from_rgb(220, 165,  0);
pub const RED:         Color32 = Color32::from_rgb(200,  40,  40);

pub const TEXT_PRI:    Color32 = Color32::from_rgb(210, 230, 255);
pub const TEXT_SEC:    Color32 = Color32::from_rgb(255, 255, 255);
pub const TEXT_DIM:    Color32 = Color32::from_rgb(255, 255, 255);
pub const TEXT_LABEL:  Color32 = Color32::from_rgb(255, 255, 255);

pub const BORDER:      Color32 = Color32::from_rgba_premultiplied(80, 130, 220, 38);
pub const BORDER_BRT:  Color32 = Color32::from_rgba_premultiplied(80, 140, 255, 80);

// ── Easing functions ─────────────────────────────────────────────────────────

pub fn ease_out_cubic(t: f32) -> f32 { 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3) }
pub fn ease_in_out_sine(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    -(( std::f32::consts::PI * t ).cos() - 1.0) / 2.0
}
pub fn ease_out_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 { N1 * t * t }
    else if t < 2.0 / D1 { let t = t - 1.5 / D1; N1 * t * t + 0.75 }
    else if t < 2.5 / D1 { let t = t - 2.25 / D1; N1 * t * t + 0.9375 }
    else { let t = t - 2.625 / D1; N1 * t * t + 0.984375 }
}

// ── Color lerp ───────────────────────────────────────────────────────────────

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
    )
}
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r() as u32 * a as u32 / 255) as u8,
        (c.g() as u32 * a as u32 / 255) as u8,
        (c.b() as u32 * a as u32 / 255) as u8,
        a,
    )
}

// ── Shimmer value (0..1) for animated progress bars ──────────────────────────

pub fn shimmer(t: f64) -> f32 {
    ((t * 1.2).fract() as f32).clamp(0.0, 1.0)
}

// ── Pulse alpha (0..1) for glow rings ────────────────────────────────────────

pub fn pulse(t: f64, hz: f64) -> f32 {
    ((t * std::f64::consts::TAU * hz).sin() as f32 * 0.35 + 0.65).clamp(0.0, 1.0)
}

// ── Format helpers ───────────────────────────────────────────────────────────

pub fn fmt_bytes(b: u64) -> String {
    match b {
        b if b >= 1 << 40 => format!("{:.2} TB", b as f64 / (1u64 << 40) as f64),
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / (1u64 << 10) as f64),
        b                  => format!("{} B",  b),
    }
}

pub fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 { format!("{:02}:{:02}:{:02}", h, m, s) }
    else      { format!("{:02}:{:02}", m, s) }
}
