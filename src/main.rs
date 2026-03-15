// main.rs — RustDefrag GUI entry point
//
// Author: Arafat BOUCHAFRA <arafat877@gmail.com>
// Repository: https://github.com/arafat877/rust-defrag

// On Windows, suppress the console window in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod defrag_engine;
mod engine;
mod ui;

use app::DefragApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustDefrag — NTFS Defragmentation Utility")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "RustDefrag",
        native_options,
        Box::new(|cc| Ok(Box::new(DefragApp::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    // Embedded 32×32 RGBA icon (blue disk icon)
    let size = 32usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let cx = x as f32 - 16.0;
            let cy = y as f32 - 16.0;
            let r  = (cx * cx + cy * cy).sqrt();
            if r <= 14.0 && r >= 5.0 {
                let alpha = ((14.0 - r) * 30.0).clamp(0.0, 255.0) as u8;
                pixels[idx]     = 30;
                pixels[idx + 1] = 100;
                pixels[idx + 2] = 220;
                pixels[idx + 3] = alpha.max(180);
            } else if r < 5.0 {
                pixels[idx]     = 15;
                pixels[idx + 1] = 50;
                pixels[idx + 2] = 140;
                pixels[idx + 3] = 200;
            }
        }
    }
    egui::IconData { rgba: pixels, width: size as u32, height: size as u32 }
}
