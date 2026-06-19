//! `screenshot` — render the FlatCAM-RS GUI to a PNG for visual verification.
//!
//! Usage:
//!     cargo run -p fc-gui --bin screenshot -- <out.png> [file1.gbr file2.gbr ...]
//!
//! ## Why a real eframe window (and not a headless `egui_kittest` harness)
//!
//! The task's preferred approach was `egui_kittest` pinned to egui 0.29. That is
//! not possible: `egui_kittest`'s earliest published release is **0.30.0** — there
//! is no 0.29 version on crates.io, and its `wgpu` render harness must match egui
//! exactly. Bumping the whole GUI to egui 0.30 just for screenshots was out of
//! scope and risky for the working desktop app.
//!
//! So this uses the documented fallback: a normal `eframe` window that, after a
//! few warm-up frames, asks the compositor for a screenshot via
//! `egui::ViewportCommand::Screenshot`, reads the resulting `Event::Screenshot`,
//! writes it with the `image` crate, and closes itself. It needs a working GPU /
//! window (it is NOT display-server-free), but it runs unattended end to end.

use eframe::egui;
use fc_gui::FlatCamApp;

const WIDTH: f32 = 1400.0;
const HEIGHT: f32 = 900.0;
/// Warm-up frames before requesting the screenshot, so layout + the camera
/// "fit-to-contents" pass have settled.
const WARMUP_FRAMES: u32 = 4;

struct ScreenshotApp {
    app: FlatCamApp,
    out_path: String,
    frame: u32,
    requested: bool,
    done: bool,
}

impl eframe::App for ScreenshotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drive the real application UI.
        self.app.ui(ctx);

        // Keep advancing frames until we are finished.
        ctx.request_repaint();
        self.frame += 1;

        // Once warmed up, ask the compositor for a screenshot (once).
        if !self.requested && self.frame >= WARMUP_FRAMES {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            self.requested = true;
            return;
        }

        if self.requested && !self.done {
            // Look for the screenshot reply in this frame's input events.
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let [w, h] = image.size;
                let mut buf: Vec<u8> = Vec::with_capacity(w * h * 4);
                for px in &image.pixels {
                    buf.extend_from_slice(&px.to_array());
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, buf) {
                    Some(rgba) => match rgba.save(&self.out_path) {
                        Ok(()) => {
                            println!("wrote {} ({}x{})", self.out_path, w, h);
                        }
                        Err(e) => eprintln!("failed to write {}: {e}", self.out_path),
                    },
                    None => eprintln!("failed to build image buffer ({w}x{h})"),
                }
                self.done = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let out_path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: screenshot <out.png> [file1 file2 ...]");
            std::process::exit(2);
        }
    };
    let mut files: Vec<String> = args.collect();
    // Optional "--dark" flag anywhere in the file args switches the theme.
    let dark = files.iter().any(|f| f == "--dark");
    files.retain(|f| f != "--dark");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH, HEIGHT])
            // Off-screen + invisible-ish: still a real window, but minimally intrusive.
            .with_visible(true),
        ..Default::default()
    };

    eframe::run_native(
        "FlatCAM-RS (screenshot)",
        native_options,
        Box::new(move |cc| {
            let mut app = FlatCamApp::boot(&cc.egui_ctx);
            app.set_theme(dark);
            for f in &files {
                app.load_path(f);
            }
            // Reproduce the user clicking the loaded object, so the screenshot
            // shows its Operations panel (the "Selected" tab).
            app.focus_selected();
            Ok(Box::new(ScreenshotApp {
                app,
                out_path,
                frame: 0,
                requested: false,
                done: false,
            }))
        }),
    )
}
