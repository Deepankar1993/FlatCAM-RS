//! `flatcam-gui` — the desktop front-end for the FlatCAM Rust port.
//!
//! Thin binary entry point. The application itself lives in the crate library
//! (`fc_gui`) so it can be reused by the headless `screenshot` binary as well.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use fc_gui::FlatCamApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    let initial = std::env::args().nth(1);
    eframe::run_native(
        "FlatCAM-RS",
        native_options,
        Box::new(move |cc| {
            let mut app = FlatCamApp::boot(&cc.egui_ctx);
            if let Some(path) = initial {
                app.load_path(&path);
            }
            Ok(Box::new(app))
        }),
    )
}
