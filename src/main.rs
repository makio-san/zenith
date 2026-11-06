//! Zenith — a textmode-styled desktop terminal with a pixel cat and live system stats.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pixelcat;
mod input;
mod stats;
mod term;
mod theme;

fn main() -> eframe::Result {
    // Dev helper: ZENITH_SIZE=1000x640 opens the window at that logical size.
    let size = std::env::var("ZENITH_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])))
        .unwrap_or([1240.0, 780.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Zenith")
            .with_inner_size(size)
            .with_min_inner_size([760.0, 440.0]),
        ..Default::default()
    };
    eframe::run_native("Zenith", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
