//! Solder-paste and manual-toolchange G-code preprocessors.
//!
//! This module extends the dialect registries (see [`crate::dialects`],
//! [`crate::dialects_extra`], [`crate::dialects_more`]) with two more
//! variants, mirroring how `preprocessors/*.py` in the Python FlatCAM
//! project supplies many G-code flavours. Each dialect is a zero-sized
//! unit struct implementing [`Preprocessor`] in the [`crate::Grbl`] style;
//! [`by_name_paste`] resolves one from a case-insensitive string.

use crate::{JobParams, Preprocessor, Units};
use std::fmt::Write as _;

/// Emit the units word (`G21` for millimetres, `G20` for inches).
fn units_word(units: Units) -> &'static str {
    if units == Units::Mm {
        "G21"
    } else {
        "G20"
    }
}

/// Solder-paste dispenser preprocessor.
///
/// This models a paste *extruder* (a syringe/auger dispenser), not a milling
/// spindle. There is conceptually no cutting: the dispenser is toggled on and
/// off as the nozzle plunges to the board and lifts back to travel height.
///
/// The dispense ON signal reuses `M3 S{spindle_rpm}` — on a paste machine the
/// spindle output drives the dispenser/auger motor, so its speed word doubles
/// as the dispense rate. Dispense OFF is `M5`. This keeps the output usable on
/// the same GRBL-class controllers that drive a normal mill.
///
/// - `header`  : sets up absolute/units and turns the dispenser OFF (`M5`).
/// - `plunge`  : lowers to `z` then turns the dispenser ON (`M3 S{rpm}`).
/// - `rapid_z` : turns the dispenser OFF (`M5`) before lifting/travelling.
/// - `linear`  : ordinary `G1` move while dispensing.
/// - `footer`  : dispenser OFF (`M5`) and program end (`M30`).
pub struct SolderPaste;

impl Preprocessor for SolderPaste {
    fn name(&self) -> &str {
        "Solder Paste"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Solder Paste dispenser preprocessor)");
        let _ = writeln!(g, "(models a paste extruder: M3 S=dispense ON, M5=OFF)");
        let _ = writeln!(g, "(nozzle dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Ensure the dispenser starts OFF.
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        // Dispenser OFF, then program end.
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        // Stop dispensing before lifting / travelling.
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        // Lower the nozzle, then begin dispensing. M3 S{rpm} is the dispense
        // ON signal (the spindle word drives the dispenser motor here).
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
        let _ = writeln!(g, "M3 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Generic milling preprocessor with an explicit manual tool change.
///
/// Behaves like [`crate::Grbl`] for all motion, but the header issues an
/// explicit `M6` toolchange followed by a `(MSG, change tool)` operator
/// prompt and an `M0` program pause, so the operator can swap the tool by
/// hand before the job runs.
pub struct ToolchangeManual;

impl Preprocessor for ToolchangeManual {
    fn name(&self) -> &str {
        "Manual Toolchange"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Manual Toolchange preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Explicit manual tool change: M6, operator message, then pause.
        let _ = writeln!(g, "M6");
        let _ = writeln!(g, "(MSG, change tool)");
        let _ = writeln!(g, "M0");
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Look up a paste/manual preprocessor by case-insensitive name.
/// Returns None if unknown.
pub fn by_name_paste(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "solderpaste" | "paste" => Some(Box::new(SolderPaste)),
        "toolchange_manual" | "manual" => Some(Box::new(ToolchangeManual)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_string(pp: &dyn Preprocessor) -> String {
        let p = JobParams::default();
        let mut g = String::new();
        pp.header(&mut g, &p);
        g
    }

    #[test]
    fn solder_paste_header_emits_mm_units() {
        assert!(header_string(&SolderPaste).contains("G21"), "Mm job must emit G21");
    }

    #[test]
    fn toolchange_manual_header_emits_mm_units() {
        let g = header_string(&ToolchangeManual);
        assert!(g.contains("G21"), "Mm job must emit G21");
        assert!(g.contains("M6"), "manual toolchange must emit M6");
        assert!(g.contains("M0"), "manual toolchange must pause with M0");
    }

    #[test]
    fn solder_paste_plunge_dispenses_on() {
        let p = JobParams::default();
        let mut g = String::new();
        SolderPaste.plunge(&mut g, p.cut_z, &p);
        assert!(g.contains("M3 S"), "plunge must turn the dispenser ON");
    }

    #[test]
    fn solder_paste_rapid_z_dispenses_off() {
        let mut g = String::new();
        SolderPaste.rapid_z(&mut g, 2.0);
        assert!(g.contains("M5"), "rapid_z must turn the dispenser OFF");
    }

    #[test]
    fn by_name_paste_resolves() {
        assert!(by_name_paste("paste").is_some(), "paste must resolve");
        assert!(by_name_paste("solderpaste").is_some());
        assert!(by_name_paste("manual").is_some());
        assert!(by_name_paste("toolchange_manual").is_some());
    }

    #[test]
    fn by_name_paste_unknown_is_none() {
        assert!(by_name_paste("nope").is_none());
    }
}
