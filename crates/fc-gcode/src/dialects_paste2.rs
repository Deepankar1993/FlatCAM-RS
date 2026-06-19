//! Additional solder-paste dispensing G-code preprocessors.
//!
//! This module complements [`crate::dialects_paste`] with the per-controller
//! solder-paste flavours from upstream FlatCAM's `preprocessors/`:
//! `Paste_1` (MACH3-like), `Paste_GRBL` and `Paste_Marlin`. As with
//! [`crate::dialects_paste::SolderPaste`], a paste dispenser is modelled as a
//! [`Preprocessor`] where:
//!
//! - `plunge`  => lower the nozzle and turn the dispenser ON,
//! - `rapid_z` => turn the dispenser OFF and lift,
//! - `linear`  => an ordinary feed move while dispensing.
//!
//! The dispenser ON/OFF signal differs per controller (the distinctive
//! "spindle word"): `Paste_1` and `Paste_GRBL` use `M03`/`M05`, while
//! `Paste_Marlin` uses Marlin's `M3`/`M5` with `;` comments and an `M2`
//! program-end. [`by_name_paste2`] resolves one from a case-insensitive name.

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

/// `Paste_1`: MACH3-like solder-paste dispensing.
///
/// Parenthesised comments and an `M30` program-end (MACH3 default). The
/// dispenser is driven via the spindle output: `M03 S{spindle_rpm}` = dispense
/// ON (the spindle word doubles as the dispense rate), `M05` = OFF.
pub struct Paste1;

impl Preprocessor for Paste1 {
    fn name(&self) -> &str {
        "Paste 1"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Paste_1 solder-paste preprocessor)");
        let _ = writeln!(g, "(MACH3-like: M03 S=dispense ON, M05=OFF)");
        let _ = writeln!(g, "(nozzle dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// `Paste_GRBL`: GRBL-like solder-paste dispensing.
///
/// Parenthesised comments and an `M02` program-end (GRBL convention). The
/// dispenser is driven via the spindle output: `M03 S{spindle_rpm}` = dispense
/// ON, `M05` = OFF.
pub struct PasteGrbl;

impl Preprocessor for PasteGrbl {
    fn name(&self) -> &str {
        "Paste GRBL"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Paste_GRBL solder-paste preprocessor)");
        let _ = writeln!(g, "(GRBL-like: M03 S=dispense ON, M05=OFF)");
        let _ = writeln!(g, "(nozzle dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M02");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// `Paste_Marlin`: Marlin-like solder-paste dispensing.
///
/// `;`-style comments, Marlin motion words (`G0`/`G1`) and an `M2`
/// program-end. The dispenser is driven via Marlin's spindle/extruder output:
/// `M3 S{spindle_rpm}` = dispense ON, `M5` = OFF.
pub struct PasteMarlin;

impl Preprocessor for PasteMarlin {
    fn name(&self) -> &str {
        "Paste Marlin"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Paste_Marlin solder-paste preprocessor");
        let _ = writeln!(g, "; Marlin-like: M3 S=dispense ON, M5=OFF");
        let _ = writeln!(g, "; nozzle dia: {:.4}", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G0 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G0 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G0 X0.0000 Y0.0000");
        let _ = writeln!(g, "M2");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G0 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G0 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 Z{:.4} F{:.0}", z, p.feed_z);
        let _ = writeln!(g, "M3 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Look up a per-controller solder-paste preprocessor by case-insensitive
/// name. Returns None if unknown.
pub fn by_name_paste2(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "paste_1" | "paste1" => Some(Box::new(Paste1)),
        "paste_grbl" => Some(Box::new(PasteGrbl)),
        "paste_marlin" => Some(Box::new(PasteMarlin)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render a paste job's motions through a preprocessor into a String.
    fn render(pp: &dyn Preprocessor) -> String {
        let p = JobParams::default();
        let mut g = String::new();
        pp.header(&mut g, &p);
        pp.rapid_z(&mut g, p.travel_z);
        pp.rapid_xy(&mut g, 0.0, 0.0);
        pp.plunge(&mut g, p.cut_z, &p);
        pp.linear(&mut g, 10.0, 0.0, &p);
        pp.rapid_z(&mut g, p.travel_z);
        pp.footer(&mut g, &p);
        g
    }

    #[test]
    fn paste1_dispenses_on_plunge_off_lift_mach3_end() {
        let g = render(&Paste1);
        assert!(g.contains("G21"));
        assert!(g.contains("M03 S"), "plunge must turn the dispenser ON");
        assert!(g.contains("M05"), "lift must turn the dispenser OFF");
        assert!(g.contains("M30"), "Paste_1 (MACH3-like) program-end is M30");
    }

    #[test]
    fn paste_grbl_uses_m02_end() {
        let g = render(&PasteGrbl);
        assert!(g.contains("M03 S"), "plunge must turn the dispenser ON");
        assert!(g.contains("M05"), "lift must turn the dispenser OFF");
        assert!(g.contains("\nM02\n"), "Paste_GRBL program-end is M02");
    }

    #[test]
    fn paste_marlin_uses_marlin_words_and_m2_end() {
        let g = render(&PasteMarlin);
        assert!(g.contains("; FlatCAM-RS Paste_Marlin"), "must use ; comments");
        assert!(g.contains("M3 S"), "plunge must turn the dispenser ON with M3");
        assert!(g.contains("M5"), "lift must turn the dispenser OFF with M5");
        assert!(g.contains("G1 X"), "Marlin uses G1 motion words");
        assert!(g.contains("\nM2\n"), "Paste_Marlin program-end is M2");
    }

    #[test]
    fn by_name_paste2_resolves_all_keys() {
        assert!(by_name_paste2("paste_1").is_some());
        assert!(by_name_paste2("paste_grbl").is_some());
        assert!(by_name_paste2("paste_marlin").is_some());
        // case-insensitive
        assert!(by_name_paste2("Paste_GRBL").is_some());
    }

    #[test]
    fn by_name_paste2_unknown_is_none() {
        assert!(by_name_paste2("nope").is_none());
    }
}
