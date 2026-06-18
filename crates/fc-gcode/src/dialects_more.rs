//! Further G-code preprocessors for the FlatCAM Rust port.
//!
//! This file extends [`crate::dialects`] and [`crate::dialects_extra`] with a
//! handful more controller flavours, mirroring how `preprocessors/*.py` in the
//! Python FlatCAM project supplies many G-code variants. Each dialect is a
//! zero-sized unit struct implementing [`Preprocessor`] in the same style as
//! [`crate::Grbl`]; [`by_name_more`] resolves one from a case-insensitive name.

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

/// Smoothieware milling preprocessor.
///
/// Smoothieboard runs a GRBL-compatible motion dialect; this emits standard
/// `G90`/`G00`/`G01`/`M03`/`M05` G-code with `;`-style comments.
pub struct Smoothie;

impl Preprocessor for Smoothie {
    fn name(&self) -> &str {
        "Smoothie"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Smoothieware preprocessor");
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

/// Synthetos TinyG / g2core milling preprocessor.
///
/// TinyG accepts standard RS-274 G-code; this emits a conventional
/// `G90`/`G00`/`G01`/`M03`/`M05` program.
pub struct TinyG;

impl Preprocessor for TinyG {
    fn name(&self) -> &str {
        "TinyG"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS TinyG preprocessor)");
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

/// LinuxCNC (EMC2) milling preprocessor.
///
/// LinuxCNC speaks full RS-274/NGC; this emits a standard
/// `G90`/`G00`/`G01`/`M03`/`M05` program with `(...)` comments.
pub struct Emc2;

impl Preprocessor for Emc2 {
    fn name(&self) -> &str {
        "EMC2"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS LinuxCNC/EMC2 preprocessor)");
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

/// GRBL dynamic-laser-power preprocessor (`M4` laser mode).
///
/// There is no Z motion in laser mode. GRBL `M4` is dynamic laser power: the
/// controller scales the `S` value with feed rate so corners and start/stop
/// transitions burn evenly. The laser is armed with `M04 S{spindle_rpm}` on
/// plunge (cut start) and disarmed with `M05` on rapid_z (travel). Here
/// `spindle_rpm` is reused as the laser power S-value.
pub struct GrblDynamicLaser;

impl Preprocessor for GrblDynamicLaser {
    fn name(&self) -> &str {
        "GRBL Dynamic Laser (M4)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL Dynamic Laser preprocessor)");
        let _ = writeln!(g, "(M4 dynamic laser power; spindle_rpm reused as S-value)");
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
        // No plunge: arm the laser in dynamic-power mode (M4) at power instead.
        let _ = writeln!(g, "M04 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Look up one of the additional preprocessors by case-insensitive name.
/// Returns None if unknown.
pub fn by_name_more(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "smoothie" => Some(Box::new(Smoothie)),
        "tinyg" => Some(Box::new(TinyG)),
        "emc2" => Some(Box::new(Emc2)),
        "grbl_m4" => Some(Box::new(GrblDynamicLaser)),
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
    fn smoothie_emits_mm_units() {
        let g = render(&Smoothie);
        assert!(g.contains("G21"), "Mm job must emit G21");
    }

    #[test]
    fn tinyg_emits_mm_units() {
        let g = render(&TinyG);
        assert!(g.contains("G21"), "Mm job must emit G21");
    }

    #[test]
    fn emc2_emits_mm_units() {
        let g = render(&Emc2);
        assert!(g.contains("G21"), "Mm job must emit G21");
    }

    #[test]
    fn dynamic_laser_emits_mm_units() {
        let g = render(&GrblDynamicLaser);
        assert!(g.contains("G21"), "Mm job must emit G21");
    }

    #[test]
    fn dynamic_laser_uses_m4_and_m5_without_z() {
        let g = render(&GrblDynamicLaser);
        assert!(g.contains("M04 S"), "dynamic laser arms with M4 on plunge");
        assert!(g.contains("M05"), "dynamic laser disarms with M5 on travel");
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
    fn by_name_more_resolves_tinyg() {
        let pp = by_name_more("tinyg").expect("tinyg must resolve");
        assert_eq!(pp.name(), TinyG.name());
    }

    #[test]
    fn by_name_more_resolves_grbl_m4() {
        let pp = by_name_more("grbl_m4").expect("grbl_m4 must resolve");
        assert_eq!(pp.name(), GrblDynamicLaser.name());
    }

    #[test]
    fn by_name_more_unknown_is_none() {
        assert!(by_name_more("nope").is_none());
    }
}
