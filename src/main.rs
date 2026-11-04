//! Zenith — a textmode-styled desktop terminal with a pixel cat and live system stats.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pixelcat;
mod stats;
mod term;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Zenith")
            .with_inner_size([1240.0, 780.0])
            .with_min_inner_size([760.0, 440.0]),
        ..Default::default()
    };
    eframe::run_native("Zenith", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
