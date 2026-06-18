//! Laser / air-assist G-code preprocessors.
//!
//! These dialects extend the registries in [`crate::dialects`],
//! [`crate::dialects_extra`], [`crate::dialects_more`] and
//! [`crate::dialects_paste`] with *laser*-style flavours, mirroring the laser
//! variants in FlatCAM's `preprocessors/*.py`.
//!
//! Laser machines have no Z cutting axis in the milling sense: a "plunge" does
//! not lower a tool, it simply turns the laser/beam ON, and a `rapid_z` lift
//! turns it OFF. The abstract [`crate::CncJob`] motion model still calls
//! `plunge`/`rapid_z`, so each laser dialect re-interprets those callbacks:
//!
//! - `plunge`  => laser ON  (no Z word emitted)
//! - `rapid_z` => laser OFF (no Z word emitted)
//! - `linear`  => `G1` cutting move at feed rate
//!
//! [`by_name_laser2`] resolves one of these dialects from a case-insensitive
//! string.

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

/// GRBL laser preprocessor with air-assist control.
///
/// Behaves like a GRBL laser cutter: the spindle commands drive the laser
/// (`M3 S{spindle_rpm}` = beam ON at the given power, `M5` = beam OFF), and
/// there is no Z motion. In addition, this dialect toggles an air-assist
/// (coolant) line: the header emits `M8` (air/coolant ON) before the job and
/// the footer emits `M9` (air/coolant OFF) at the end.
///
/// - `header`  : units/absolute, beam OFF (`M5`), air-assist ON (`M8`).
/// - `plunge`  : beam ON at power (`M3 S{spindle_rpm}`).
/// - `rapid_z` : beam OFF (`M5`).
/// - `linear`  : `G1` cutting move.
/// - `footer`  : beam OFF (`M5`), air-assist OFF (`M9`), program end (`M30`).
pub struct GrblLaserAirAssist;

impl Preprocessor for GrblLaserAirAssist {
    fn name(&self) -> &str {
        "GRBL Laser (Air Assist)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL Laser air-assist preprocessor)");
        let _ = writeln!(g, "(laser: M3 S=beam ON power, M5=OFF; no Z)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Ensure the beam starts OFF, then turn the air assist ON.
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "M8");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        // Beam OFF, air assist OFF, then program end.
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "M9");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // No Z on a laser: a "lift" is just the beam turning OFF.
        let _ = writeln!(g, "M5");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        // No Z on a laser: a "plunge" is just the beam turning ON at power.
        let _ = writeln!(g, "M3 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Clamp `spindle_rpm` (treated as a 0..=1000 GRBL-style power value) and scale
/// it into Marlin's 8-bit FAN/PWM range 0..=255.
fn power_to_fan(spindle_rpm: f64) -> u32 {
    // Treat spindle_rpm as a 0..1000 power request (the common GRBL `$30` max).
    let clamped = spindle_rpm.clamp(0.0, 1000.0);
    let scaled = clamped / 1000.0 * 255.0;
    scaled.round() as u32
}

/// Marlin laser preprocessor driving the laser through the **FAN pin** (PWM).
///
/// Many Marlin laser setups wire the laser PWM enable to a controllable fan
/// output (the part-cooling fan header), because that pin already exposes an
/// 8-bit hardware PWM (`M106 S0..255` / `M107` off). Under this FAN-pin model
/// there is no spindle and no Z motion:
///
/// - `header`  : units/absolute, then `M106` to assert the laser/fan pin
///   (laser enabled, ready for power changes).
/// - `plunge`  : beam ON at scaled power — `M106 S{0..255}`, where the power is
///   derived from `spindle_rpm` clamped to 0..1000 and rescaled to 0..255 (see
///   [`power_to_fan`]).
/// - `rapid_z` : beam OFF — `M107`.
/// - `linear`  : `G1` cutting move.
/// - `footer`  : beam OFF (`M107`) and program end (`M2`).
pub struct MarlinLaserFan;

impl Preprocessor for MarlinLaserFan {
    fn name(&self) -> &str {
        "Marlin Laser (FAN pin)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Marlin Laser (FAN-pin) preprocessor");
        let _ = writeln!(g, "; laser driven via the fan PWM pin: M106 S0..255, M107=off; no Z");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Assert the laser/fan pin (header enable), at zero power.
        let _ = writeln!(g, "M106 S0");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        // Beam OFF via the fan pin, then program end.
        let _ = writeln!(g, "M107");
        let _ = writeln!(g, "M2");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // No Z: "lift" turns the laser OFF.
        let _ = writeln!(g, "M107");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G0 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        // No Z: "plunge" turns the laser ON at the scaled fan PWM power.
        let _ = writeln!(g, "M106 S{}", power_to_fan(p.spindle_rpm));
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Marlin laser preprocessor driving the laser through the **spindle** output.
///
/// This uses Marlin's spindle/laser commands directly: `M3 S{power}` to turn
/// the beam ON at a power value and `M5` to turn it OFF. There is no Z motion.
///
/// - `header`  : units/absolute, beam OFF (`M5`).
/// - `plunge`  : beam ON at power (`M3 S{spindle_rpm}`).
/// - `rapid_z` : beam OFF (`M5`).
/// - `linear`  : `G1` cutting move.
/// - `footer`  : beam OFF (`M5`) and program end (`M2`).
pub struct MarlinLaserSpindle;

impl Preprocessor for MarlinLaserSpindle {
    fn name(&self) -> &str {
        "Marlin Laser (spindle)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Marlin Laser (spindle) preprocessor");
        let _ = writeln!(g, "; laser via spindle output: M3 S=power, M5=off; no Z");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M5");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "M2");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        let _ = writeln!(g, "M5");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G0 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        let _ = writeln!(g, "M3 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Look up a laser/air-assist preprocessor by case-insensitive name.
/// Returns None if unknown.
pub fn by_name_laser2(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "grbl_laser_air" | "laser_air" => Some(Box::new(GrblLaserAirAssist)),
        "marlin_laser_fan" => Some(Box::new(MarlinLaserFan)),
        "marlin_laser_spindle" => Some(Box::new(MarlinLaserSpindle)),
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

    fn footer_string(pp: &dyn Preprocessor) -> String {
        let p = JobParams::default();
        let mut g = String::new();
        pp.footer(&mut g, &p);
        g
    }

    #[test]
    fn each_header_emits_mm_units() {
        assert!(header_string(&GrblLaserAirAssist).contains("G21"));
        assert!(header_string(&MarlinLaserFan).contains("G21"));
        assert!(header_string(&MarlinLaserSpindle).contains("G21"));
    }

    #[test]
    fn grbl_laser_air_header_has_m8_footer_has_m9() {
        assert!(
            header_string(&GrblLaserAirAssist).contains("M8"),
            "air-assist header must turn air ON with M8"
        );
        assert!(
            footer_string(&GrblLaserAirAssist).contains("M9"),
            "air-assist footer must turn air OFF with M9"
        );
    }

    #[test]
    fn marlin_laser_fan_plunge_has_m106() {
        let p = JobParams::default();
        let mut g = String::new();
        MarlinLaserFan.plunge(&mut g, p.cut_z, &p);
        assert!(g.contains("M106"), "fan-pin plunge must turn the laser ON with M106");
    }

    #[test]
    fn power_to_fan_scales_and_clamps() {
        assert_eq!(power_to_fan(0.0), 0);
        assert_eq!(power_to_fan(1000.0), 255);
        assert_eq!(power_to_fan(2000.0), 255, "over-range clamps to 255");
        assert_eq!(power_to_fan(-50.0), 0, "negative clamps to 0");
    }

    #[test]
    fn by_name_laser2_resolves() {
        assert!(by_name_laser2("laser_air").is_some());
        assert!(by_name_laser2("grbl_laser_air").is_some());
        assert!(by_name_laser2("marlin_laser_fan").is_some());
        assert!(by_name_laser2("marlin_laser_spindle").is_some());
    }

    #[test]
    fn by_name_laser2_unknown_is_none() {
        assert!(by_name_laser2("nope").is_none());
    }
}
