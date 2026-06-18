//! `preferences` — a polished, category-tabbed Preferences/Settings dialog body
//! for the FlatCAM-RS egui desktop app.
//!
//! This module renders the *body* of the preferences window: a category tab bar
//! plus an aligned form exposing every field of [`fc_app::Preferences`], and a
//! footer with the standard Apply / Save / Load / Reset actions.
//!
//! The caller (`main.rs`) owns the window chrome (the [`egui::Window`] or panel)
//! and is responsible for acting on the returned [`PrefsAction`] — performing the
//! file dialog for Save/Load, applying the prefs into the live params model, etc.
//! This keeps the body pure (apart from `ResetDefaults`, which is a pure
//! `*prefs = Default::default()` that we apply directly).
//!
//! The selected tab is persisted across frames in egui's temporary data store,
//! keyed by a stable [`egui::Id`], so the signature can stay
//! `(&mut Ui, &mut Preferences) -> PrefsAction` without the caller owning any
//! extra state.
//!
//! `fc_app::Preferences` fields covered (all of them):
//! `units`, `default_tool_dia`, `default_cut_z`, `default_travel_z`,
//! `default_feed_xy`, `default_feed_z`, `default_spindle`, `default_preproc`,
//! `iso_passes`, `iso_overlap`.

use eframe::egui;

/// Which action the user pressed in the preferences footer (None if just editing).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrefsAction {
    #[default]
    None,
    ApplyToParams,
    Save,
    Load,
    ResetDefaults,
}

/// The preference categories shown as tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum PrefsTab {
    #[default]
    General,
    CamDefaults,
    Isolation,
}

impl PrefsTab {
    const ALL: [PrefsTab; 3] = [PrefsTab::General, PrefsTab::CamDefaults, PrefsTab::Isolation];

    fn title(self) -> &'static str {
        match self {
            PrefsTab::General => "General",
            PrefsTab::CamDefaults => "CAM Defaults",
            PrefsTab::Isolation => "Isolation",
        }
    }
}

/// Known G-code preprocessor / dialect names offered in the preprocessor combo.
/// Kept in sync with the dialects used elsewhere in the GUI.
const PREPROCESSORS: &[&str] = &[
    "grbl",
    "grbl_no_m6",
    "grbl_laser",
    "marlin",
    "default",
    "roland",
    "smoothie",
    "tinyg",
];

/// Working-unit choices. Stored as the short string `fc_app::Preferences` uses.
const UNITS: &[(&str, &str)] = &[("mm", "Millimeters (mm)"), ("in", "Inches (in)")];

/// Render the preferences body into `ui`, editing `prefs` in place. Returns the
/// footer action the user clicked this frame (or [`PrefsAction::None`]). The
/// caller owns the window and is responsible for acting on the returned action.
pub fn preferences_body(ui: &mut egui::Ui, prefs: &mut fc_app::Preferences) -> PrefsAction {
    // Persist the active tab across frames in egui's temp data store.
    let tab_id = egui::Id::new("fc_prefs_active_tab");
    let mut tab: PrefsTab = ui.ctx().data_mut(|d| *d.get_temp_mut_or(tab_id, PrefsTab::default()));

    // ---- Tab bar (top SelectableLabel row) -------------------------------
    ui.horizontal(|ui| {
        for t in PrefsTab::ALL {
            if ui.selectable_label(tab == t, t.title()).clicked() {
                tab = t;
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(tab_id, tab));

    ui.separator();
    ui.add_space(4.0);

    // ---- Tab body --------------------------------------------------------
    // Give the body a scroll area so the dialog stays usable when small.
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .max_height(360.0)
        .show(ui, |ui| match tab {
            PrefsTab::General => general_tab(ui, prefs),
            PrefsTab::CamDefaults => cam_defaults_tab(ui, prefs),
            PrefsTab::Isolation => isolation_tab(ui, prefs),
        });

    // ---- Footer (always visible) ----------------------------------------
    ui.add_space(6.0);
    ui.separator();
    footer(ui, prefs)
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

/// General: working units and the default G-code preprocessor.
fn general_tab(ui: &mut egui::Ui, prefs: &mut fc_app::Preferences) {
    ui.strong("Application");
    ui.add_space(2.0);
    egui::Grid::new("fc_prefs_general_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("Working units");
            let units_text = UNITS
                .iter()
                .find(|(short, _)| *short == prefs.units)
                .map(|(_, long)| *long)
                .unwrap_or(prefs.units.as_str());
            egui::ComboBox::from_id_salt("fc_prefs_units")
                .selected_text(units_text)
                .show_ui(ui, |ui| {
                    for (short, long) in UNITS {
                        ui.selectable_value(&mut prefs.units, (*short).to_string(), *long);
                    }
                });
            ui.end_row();

            ui.label("Default preprocessor");
            egui::ComboBox::from_id_salt("fc_prefs_preproc")
                .selected_text(prefs.default_preproc.clone())
                .show_ui(ui, |ui| {
                    for name in PREPROCESSORS {
                        ui.selectable_value(
                            &mut prefs.default_preproc,
                            (*name).to_string(),
                            *name,
                        );
                    }
                });
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "These defaults seed newly created objects and CAM operations. \
             Use \"Apply to params\" to push them into the current job parameters.",
        )
        .weak()
        .italics(),
    );
}

/// CAM Defaults: tool geometry, cut/travel depths, feeds, spindle.
fn cam_defaults_tab(ui: &mut egui::Ui, prefs: &mut fc_app::Preferences) {
    let len_suffix = length_suffix(prefs);
    let feed_suffix = feed_suffix(prefs);

    ui.strong("Tool & geometry");
    ui.add_space(2.0);
    egui::Grid::new("fc_prefs_geom_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("Tool diameter");
            ui.add(
                egui::DragValue::new(&mut prefs.default_tool_dia)
                    .speed(0.01)
                    .range(0.001..=100.0)
                    .suffix(len_suffix),
            );
            ui.end_row();

            ui.label("Cut Z (depth)");
            ui.add(
                egui::DragValue::new(&mut prefs.default_cut_z)
                    .speed(0.01)
                    .range(-100.0..=0.0)
                    .suffix(len_suffix),
            );
            ui.end_row();

            ui.label("Travel Z (clearance)");
            ui.add(
                egui::DragValue::new(&mut prefs.default_travel_z)
                    .speed(0.05)
                    .range(0.0..=200.0)
                    .suffix(len_suffix),
            );
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.separator();
    ui.strong("Feeds & speeds");
    ui.add_space(2.0);
    egui::Grid::new("fc_prefs_feeds_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("Feed rate XY");
            ui.add(
                egui::DragValue::new(&mut prefs.default_feed_xy)
                    .speed(1.0)
                    .range(0.0..=100_000.0)
                    .suffix(feed_suffix),
            );
            ui.end_row();

            ui.label("Feed rate Z (plunge)");
            ui.add(
                egui::DragValue::new(&mut prefs.default_feed_z)
                    .speed(1.0)
                    .range(0.0..=100_000.0)
                    .suffix(feed_suffix),
            );
            ui.end_row();

            ui.label("Spindle speed");
            ui.add(
                egui::DragValue::new(&mut prefs.default_spindle)
                    .speed(10.0)
                    .range(0.0..=100_000.0)
                    .suffix(" rpm"),
            );
            ui.end_row();
        });
}

/// Isolation: number of passes and pass overlap.
fn isolation_tab(ui: &mut egui::Ui, prefs: &mut fc_app::Preferences) {
    ui.strong("Isolation routing");
    ui.add_space(2.0);
    egui::Grid::new("fc_prefs_iso_grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("Passes");
            ui.add(
                egui::DragValue::new(&mut prefs.iso_passes)
                    .speed(1)
                    .range(1..=50),
            );
            ui.end_row();

            ui.label("Pass overlap");
            ui.add(
                egui::DragValue::new(&mut prefs.iso_overlap)
                    .speed(0.01)
                    .range(0.0..=0.9)
                    .suffix(" \u{00d7} dia"),
            );
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Overlap is a fraction of the tool diameter between adjacent \
             isolation passes (0.0 \u{2013} 0.9).",
        )
        .weak()
        .italics(),
    );
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

fn footer(ui: &mut egui::Ui, prefs: &mut fc_app::Preferences) -> PrefsAction {
    let mut action = PrefsAction::None;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui
            .button("Apply to params")
            .on_hover_text("Push these defaults into the current job parameters")
            .clicked()
        {
            action = PrefsAction::ApplyToParams;
        }
        if ui
            .button("Save\u{2026}")
            .on_hover_text("Save preferences to a JSON file")
            .clicked()
        {
            action = PrefsAction::Save;
        }
        if ui
            .button("Load\u{2026}")
            .on_hover_text("Load preferences from a JSON file")
            .clicked()
        {
            action = PrefsAction::Load;
        }

        // Push the destructive action to the right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Reset defaults")
                .on_hover_text("Restore all preferences to factory defaults")
                .clicked()
            {
                *prefs = fc_app::Preferences::default();
                action = PrefsAction::ResetDefaults;
            }
        });
    });
    action
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Unit suffix for length-valued fields, derived from the current `units`.
fn length_suffix(prefs: &fc_app::Preferences) -> &'static str {
    if prefs.units == "in" {
        " in"
    } else {
        " mm"
    }
}

/// Unit suffix for feed-rate fields, derived from the current `units`.
fn feed_suffix(prefs: &fc_app::Preferences) -> &'static str {
    if prefs.units == "in" {
        " in/min"
    } else {
        " mm/min"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_action_is_none() {
        let _prefs = fc_app::Preferences::default();
        assert_eq!(PrefsAction::default(), PrefsAction::None);
    }

    #[test]
    fn suffixes_track_units() {
        let mut p = fc_app::Preferences::default();
        p.units = "mm".into();
        assert_eq!(length_suffix(&p), " mm");
        assert_eq!(feed_suffix(&p), " mm/min");
        p.units = "in".into();
        assert_eq!(length_suffix(&p), " in");
        assert_eq!(feed_suffix(&p), " in/min");
    }
}
