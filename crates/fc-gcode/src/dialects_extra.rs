//! Additional G-code preprocessors for the FlatCAM Rust port.
//!
//! This module supplies more controller-specific dialects, complementing
//! [`crate::Grbl`]/[`crate::Marlin`] and [`crate::dialects`]. As in the Python
//! FlatCAM project's `preprocessors/*.py`, each controller has its own flavour
//! of header/footer and program-end code; the actual motion words are nearly
//! identical generic RS-274 (`G00`/`G01`). Each dialect is a zero-sized unit
//! struct implementing [`Preprocessor`]; [`by_name_extra`] resolves a dialect
//! from a case-insensitive string.
//!
//! Differences between these dialects are largely cosmetic (comment style and
//! program-end word). They are documented per struct below:
//!
//! | dialect          | comment style | program end |
//! |------------------|---------------|-------------|
//! | [`Isel`]         | `( ... )`     | `M30`       |
//! | [`Repetier`]     | `; ...`       | `M2`        |
//! | [`Berta`]        | `( ... )`     | `M2`        |
//! | [`LinuxCnc`]     | `( ... )`     | `M2`        |
//! | [`Mach3`]        | `( ... )`     | `M30`       |
//! | [`ToolchangeProbe`] | `( ... )`  | `M30`       |

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

/// ISEL / iselautomation controller flavour.
///
/// Standard RS-274 motion with parenthesised comments and an `M30`
/// program-end (rewind), as ISEL Remote/ProNC controllers expect.
pub struct Isel;

impl Preprocessor for Isel {
    fn name(&self) -> &str {
        "ISEL"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS ISEL preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
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

/// Repetier firmware flavour (3D-printer-derived CNC/laser firmware).
///
/// Uses semicolon comments (Repetier/Marlin-style) and an `M2` program-end.
pub struct Repetier;

impl Preprocessor for Repetier {
    fn name(&self) -> &str {
        "Repetier"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Repetier preprocessor");
        let _ = writeln!(g, "; tool dia: {:.4}", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M2");
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

/// Berta CNC controller flavour.
///
/// Parenthesised comments and an `M2` program-end.
pub struct Berta;

impl Preprocessor for Berta {
    fn name(&self) -> &str {
        "Berta"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Berta preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M2");
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

/// LinuxCNC (EMC2) flavour.
///
/// Canonical RS-274NGC with parenthesised comments and an `M2` program-end,
/// which is the idiomatic LinuxCNC end-of-program word.
pub struct LinuxCnc;

impl Preprocessor for LinuxCnc {
    fn name(&self) -> &str {
        "LinuxCNC"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS LinuxCNC preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M2");
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

/// Mach3 / Mach4 controller flavour.
///
/// Parenthesised comments and an `M30` program-end (rewind), matching the
/// Mach3 default end-of-program behaviour.
pub struct Mach3;

impl Preprocessor for Mach3 {
    fn name(&self) -> &str {
        "Mach3"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Mach3 preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
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

/// Toolchange-with-probe flavour (e.g. LinuxCNC/GRBL with a tool-length probe).
///
/// Identical motion output to a generic router, but the header includes a
/// straight-probe sequence comment plus a `G38.2` probe move toward the touch
/// plate, so the operator can re-zero Z after a manual toolchange. Program-end
/// is `M30`.
pub struct ToolchangeProbe;

impl Preprocessor for ToolchangeProbe {
    fn name(&self) -> &str {
        "Toolchange Probe"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Toolchange-Probe preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Tool-length probe sequence: probe down toward the touch plate.
        let _ = writeln!(g, "(TOOLCHANGE: probe Z to touch plate)");
        let _ = writeln!(g, "G38.2 Z{:.4} F{:.0}", p.cut_z, p.feed_z);
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

/// Look up an extra preprocessor by case-insensitive name. Returns None if unknown.
pub fn by_name_extra(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "isel" => Some(Box::new(Isel)),
        "repetier" => Some(Box::new(Repetier)),
        "berta" => Some(Box::new(Berta)),
        "linuxcnc" => Some(Box::new(LinuxCnc)),
        "mach3" => Some(Box::new(Mach3)),
        "toolchange_probe" => Some(Box::new(ToolchangeProbe)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render a small mill job's motions through a preprocessor into a String.
    fn render(pp: &dyn Preprocessor) -> String {
        let p = JobParams::default();
        let mut g = String::new();
        pp.header(&mut g, &p);
        pp.rapid_z(&mut g, p.travel_z);
        pp.rapid_xy(&mut g, 0.0, 0.0);
        pp.plunge(&mut g, p.cut_z, &p);
        pp.linear(&mut g, 10.0, 0.0, &p);
        pp.linear(&mut g, 10.0, 10.0, &p);
        pp.rapid_z(&mut g, p.travel_z);
        pp.footer(&mut g, &p);
        g
    }

    #[test]
    fn all_emit_mm_units() {
        for pp in [
            by_name_extra("isel").unwrap(),
            by_name_extra("repetier").unwrap(),
            by_name_extra("berta").unwrap(),
            by_name_extra("linuxcnc").unwrap(),
            by_name_extra("mach3").unwrap(),
            by_name_extra("toolchange_probe").unwrap(),
        ] {
            let g = render(pp.as_ref());
            assert!(g.contains("G21"), "{} must emit G21 for mm", pp.name());
        }
    }

    #[test]
    fn program_end_words_match_docs() {
        assert!(render(&Isel).contains("M30"));
        assert!(render(&Mach3).contains("M30"));
        assert!(render(&ToolchangeProbe).contains("M30"));
        assert!(render(&Repetier).contains("\nM2\n"));
        assert!(render(&Berta).contains("\nM2\n"));
        assert!(render(&LinuxCnc).contains("\nM2\n"));
    }

    #[test]
    fn toolchange_probe_emits_probe_sequence() {
        let g = render(&ToolchangeProbe);
        assert!(g.contains("G38.2"), "probe preprocessor must emit G38.2");
        assert!(g.contains("(TOOLCHANGE"), "probe must comment the toolchange");
    }

    #[test]
    fn by_name_extra_resolves_and_rejects() {
        assert!(by_name_extra("mach3").is_some());
        assert_eq!(by_name_extra("mach3").unwrap().name(), Mach3.name());
        assert!(by_name_extra("nope").is_none());
    }
}
