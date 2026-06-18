//! Help-menu content bodies for the FlatCAM-RS desktop UI.
//!
//! Pure presentation helpers only — no application state, no I/O, no side
//! effects beyond the [`egui::Ui`] passed in. Targets egui 0.29.x.
//!
//! Each function renders the *body* of a Help dialog; the caller owns the
//! surrounding [`egui::Window`] chrome and its `open` boolean.
//!
//! Contents:
//! * [`about_body`] — the "About" dialog body.
//! * [`shortcuts_body`] — keyboard & mouse reference table.
//! * [`getting_started_body`] — a short numbered quick-start guide.

use eframe::egui;

/// "About" dialog body.
///
/// Shows the app name, that it is a Rust port of FlatCAM Evo, a one-line
/// description of what it does, and a short technology line.
pub fn about_body(ui: &mut egui::Ui) {
    ui.heading("FlatCAM-RS");
    ui.label(
        egui::RichText::new("A pure-Rust port of FlatCAM Evo")
            .strong(),
    );
    ui.add_space(8.0);

    ui.label(
        "PCB CAM: reads Gerber / Excellon / SVG / DXF / PDF and generates \
         isolation routing, paint, NCC (copper clear), cutout, drilling and \
         laser jobs — emitting G-code for GRBL, Marlin and similar controllers.",
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Built with egui / eframe and pure-Rust CAM crates.")
            .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Inspired by").small());
        ui.hyperlink_to(
            egui::RichText::new("FlatCAM").small(),
            "https://bitbucket.org/jpcgt/flatcam",
        );
    });
}

/// Keyboard & mouse reference body.
///
/// Renders a striped two-column table ("Input" / "Action") of the app's
/// mouse and keyboard interactions.
pub fn shortcuts_body(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Mouse")
            .strong(),
    );
    egui::Grid::new("shortcuts_mouse")
        .num_columns(2)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.strong("Input");
            ui.strong("Action");
            ui.end_row();

            ui.label("Left-drag");
            ui.label("Pan");
            ui.end_row();

            ui.label("Left-click");
            ui.label("Select object");
            ui.end_row();

            ui.label("Right-drag");
            ui.label("Box select");
            ui.end_row();

            ui.label("Right-click");
            ui.label("Context menu");
            ui.end_row();

            ui.label("Scroll");
            ui.label("Zoom");
            ui.end_row();
        });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("Keyboard")
            .strong(),
    );
    egui::Grid::new("shortcuts_keys")
        .num_columns(2)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.strong("Input");
            ui.strong("Action");
            ui.end_row();

            ui.label("Delete");
            ui.label("Delete selected (in editor)");
            ui.end_row();

            ui.label("Esc");
            ui.label("Cancel / deselect");
            ui.end_row();

            ui.label("Ctrl + O");
            ui.label("—");
            ui.end_row();

            ui.label("Ctrl + S");
            ui.label("—");
            ui.end_row();
        });
}

/// A short "Getting started" body.
///
/// Renders a concise numbered quick-start guide as plain labels.
pub fn getting_started_body(ui: &mut egui::Ui) {
    ui.heading("Getting started");
    ui.add_space(6.0);

    ui.label("1. Open your Gerber / Excellon files.");
    ui.label("2. Select a layer in the Project tree.");
    ui.label("3. Set the Tool \u{00d8} and number of Passes in Parameters.");
    ui.label("4. Click Isolation (or Paint / NCC / Cutout / Drilling).");
    ui.label("5. View and save the generated G-code.");
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanity() {
        assert_eq!(2 + 2, 4);
    }
}
