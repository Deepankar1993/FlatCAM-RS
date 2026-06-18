//! Additional G-code preprocessors for the FlatCAM Rust port.
//!
//! This file complements the [`crate::Grbl`] and [`crate::Marlin`] dialects
//! defined in the crate root, mirroring how `preprocessors/*.py` in the Python
//! FlatCAM project supplies many G-code variants. Each dialect is a zero-sized
//! unit struct implementing [`Preprocessor`]; [`by_name`] resolves a dialect
//! from a case-insensitive string.

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

/// Generic / MACH3-style milling preprocessor.
///
/// Behaves like a standard GRBL router but issues an explicit `M06 T1`
/// toolchange (with a `(TOOLCHANGE)` comment) in the header.
pub struct GenericDefault;

impl Preprocessor for GenericDefault {
    fn name(&self) -> &str {
        "Default"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Default/MACH3 preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "(TOOLCHANGE)");
        let _ = writeln!(g, "M06 T1");
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

/// Standard GRBL router that never emits an M6 toolchange (single tool).
///
/// Identical to [`crate::Grbl`] in motion output; the distinction is purely
/// that no toolchange line is ever produced, suitable for fixed-spindle
/// machines.
pub struct GrblNoM6;

impl Preprocessor for GrblNoM6 {
    fn name(&self) -> &str {
        "GRBL (no M6)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL no-M6 preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "(single tool: no M6 toolchange emitted)");
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

/// GRBL laser-mode preprocessor.
///
/// There is no Z motion in laser mode: the laser is switched OFF (`M05`)
/// during travel and ON (`M03 S{spindle_rpm}`) while cutting. Note that
/// `spindle_rpm` is reused here as the laser power S-value (GRBL laser mode
/// drives the same `S` word for laser intensity).
pub struct GrblLaser;

impl Preprocessor for GrblLaser {
    fn name(&self) -> &str {
        "GRBL Laser"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL Laser preprocessor)");
        let _ = writeln!(g, "(spindle_rpm reused as laser power S-value)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M05");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "M02");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // No Z motion in laser mode; ensure the laser is OFF during travel.
        let _ = writeln!(g, "M05");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        // No plunge: turn the laser ON at power (S-value) instead.
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Roland MDX engraver flavour (approximation).
///
/// Real Roland controllers use an RML/NC dialect with `;`-style directives
/// (e.g. `;;^IN;!MC1;`). This is a pragmatic approximation that emits `;`
/// comments alongside otherwise standard `G90`/`G00`/`G01`/`M03` G-code, which
/// most Roland MDX machines in NC-compatibility mode will accept.
pub struct RolandMDX;

impl Preprocessor for RolandMDX {
    fn name(&self) -> &str {
        "Roland MDX"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Roland MDX preprocessor (approximation)");
        let _ = writeln!(g, "; tool dia: {:.4}", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "M02");
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

/// Look up a preprocessor by case-insensitive name. Returns None if unknown.
pub fn by_name(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "grbl" => Some(Box::new(crate::Grbl)),
        "marlin" => Some(Box::new(crate::Marlin)),
        "default" | "generic" => Some(Box::new(GenericDefault)),
        "grbl_no_m6" => Some(Box::new(GrblNoM6)),
        "grbl_laser" | "laser" => Some(Box::new(GrblLaser)),
        "roland" | "roland_mdx" => Some(Box::new(RolandMDX)),
        // Fall back to the extended dialect registries.
        other => crate::dialects_extra::by_name_extra(other)
            .or_else(|| crate::dialects_more::by_name_more(other))
            .or_else(|| crate::dialects_paste::by_name_paste(other)),
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
    fn grbl_laser_turns_on_and_off_without_z() {
        let g = render(&GrblLaser);
        assert!(g.contains("M03 S"), "laser must turn on at power");
        assert!(g.contains("M05"), "laser must turn off");
        // No Z axis should appear on cut/linear lines in laser mode.
        for line in g.lines() {
            if line.starts_with("G01") {
                assert!(
                    !line.contains('Z'),
                    "laser cut line must not contain Z: {line}"
                );
            }
        }
    }

    #[test]
    fn generic_default_emits_mm_units() {
        let g = render(&GenericDefault);
        assert!(g.contains("G21"), "Mm job must emit G21");
        assert!(g.contains("M06 T1"), "default emits a toolchange");
    }

    #[test]
    fn by_name_resolves_laser() {
        let pp = by_name("laser").expect("laser must resolve");
        assert_eq!(pp.name(), GrblLaser.name());
    }

    #[test]
    fn by_name_unknown_is_none() {
        assert!(by_name("nope").is_none());
    }
}
