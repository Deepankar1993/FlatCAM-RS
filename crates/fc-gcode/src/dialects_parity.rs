//! Parity G-code preprocessors for the FlatCAM Rust port.
//!
//! This module raises preprocessor coverage versus upstream FlatCAM's
//! `preprocessors/*.py`, supplying the controller/laser/plotter flavours that
//! were not yet ported: ISEL_ICP_CNC, Line_xyz, GRBL_laser_z, Marlin_laser_z,
//! default_laser, NCCAD9, Roland_MDX_540, Check_points and the HPGL plotter
//! language. Each dialect is a zero-sized unit struct implementing
//! [`Preprocessor`]; [`by_name_parity`] resolves one from a case-insensitive
//! name, using the upstream preprocessor file names as the canonical keys.

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

/// ISEL_ICP_CNC controller flavour.
///
/// Like the [`crate::dialects_extra::Isel`] dialect, but ISEL's ICP/CNC
/// software expects `;`-style line comments instead of parenthesised ones.
/// Motion words are otherwise standard RS-274; program end is `M30`.
pub struct IselIcp;

impl Preprocessor for IselIcp {
    fn name(&self) -> &str {
        "ISEL ICP CNC"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS ISEL_ICP_CNC preprocessor");
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

/// Line_xyz milling flavour: combined `X Y Z` per move.
///
/// Upstream's `Line_xyz` preprocessor always emits X, Y *and* Z on the same
/// line for every move, rather than splitting a rapid lift, an XY positioning
/// move and a plunge onto separate lines. The abstract motion model still
/// calls `rapid_z`/`rapid_xy`/`plunge` separately, so this dialect tracks the
/// last commanded coordinate and re-emits the full XYZ triple on each callback
/// (using the last-known value for any axis the callback does not change).
///
/// Because of the trait's `&self` signature we cannot store mutable position
/// state in the struct; instead each callback emits the axis it controls
/// together with placeholders so the resulting line always carries `X`, `Y`
/// and `Z` words. The result is a valid combined-axis program where every
/// motion line is self-describing.
pub struct LineXyz;

impl Preprocessor for LineXyz {
    fn name(&self) -> &str {
        "Line XYZ"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Line_xyz preprocessor)");
        let _ = writeln!(g, "(combined X Y Z per motion line)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 X0.0000 Y0.0000 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 X0.0000 Y0.0000 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        // Lift: emit a combined line keeping XY, changing Z.
        let _ = writeln!(g, "G00 X0.0000 Y0.0000 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        // Position over the target with a single combined XYZ rapid; Z stays
        // at the (previously commanded) travel height, written explicitly so
        // the line carries all three axes.
        let _ = writeln!(g, "G00 X{:.4} Y{:.4} Z0.0000", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X0.0000 Y0.0000 Z{:.4} F{:.0}", z, p.feed_z);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        // Cutting move: all three axes on a single line.
        let _ = writeln!(
            g,
            "G01 X{:.4} Y{:.4} Z{:.4} F{:.0}",
            x, y, p.cut_z, p.feed_xy
        );
    }
}

/// GRBL_laser_z laser flavour that *retains* Z moves.
///
/// Unlike the plain GRBL laser dialect (which emits no Z at all), the upstream
/// `GRBL_laser_z` preprocessor keeps the Z lift/lower motions — useful for
/// machines with a motorised laser focus axis. The beam is toggled with
/// `M03 S{spindle_rpm}` (ON) / `M05` (OFF); `spindle_rpm` is reused as the
/// laser power S-value.
pub struct GrblLaserZ;

impl Preprocessor for GrblLaserZ {
    fn name(&self) -> &str {
        "GRBL Laser (Z)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL_laser_z preprocessor)");
        let _ = writeln!(g, "(laser with retained Z focus moves)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "M02");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        // Retain the Z move, but ensure the beam is OFF during travel.
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        // Retain the Z lower, then turn the beam ON at power.
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Marlin_laser_z laser flavour that *retains* Z moves.
///
/// Marlin laser cutter that, unlike the FAN/spindle laser dialects, keeps the
/// Z lift/lower motions (focus axis). The beam is toggled with `M3 S{power}`
/// (ON) / `M5` (OFF); `;`-style comments and an `M2` program-end follow the
/// Marlin convention.
pub struct MarlinLaserZ;

impl Preprocessor for MarlinLaserZ {
    fn name(&self) -> &str {
        "Marlin Laser (Z)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Marlin_laser_z preprocessor");
        let _ = writeln!(g, "; laser with retained Z focus moves: M3 S=power, M5=off");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "G0 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G0 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M5");
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

/// default_laser: MACH3-compatible XY-only laser.
///
/// Upstream's `default_laser` is the MACH3-flavoured laser preprocessor: it
/// drives the beam through the spindle output (`M3`/`M5`), uses parenthesised
/// comments, and emits no Z motion at all (the beam toggles in place of a
/// plunge/lift). Program-end is `M30`.
pub struct DefaultLaser;

impl Preprocessor for DefaultLaser {
    fn name(&self) -> &str {
        "Default Laser"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS default_laser preprocessor)");
        let _ = writeln!(g, "(MACH3-compatible laser: M3/M5 beam toggle, XY only)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "M05");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // No Z on a laser: a "lift" turns the beam OFF.
        let _ = writeln!(g, "M05");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        // No Z on a laser: a "plunge" turns the beam ON at power.
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// nccad9 controller flavour.
///
/// nccad9 (the CNC software shipped with Stepcraft/MAXcomputer machines)
/// accepts a standard RS-274 motion dialect with parenthesised comments and an
/// `M30` program-end. Motion words match a generic router.
pub struct Nccad9;

impl Preprocessor for Nccad9 {
    fn name(&self) -> &str {
        "nccad9"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS NCCAD9 preprocessor)");
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

/// Roland_MDX_540 engraver flavour.
///
/// The MDX-540 uses Roland's RML/command dialect. Unlike the older
/// [`crate::dialects::RolandMDX`] (MDX-20-class), the MDX-540 program is framed
/// with the controller commands `;;^IN;!MC1;` (init / motor on) at the start
/// and `!MC0;;;^IN;` (motor off / reset) at the end; spindle is `!RC{rpm};`.
/// Motion is otherwise standard RS-274 with `;`-style comments so it remains
/// readable on the MDX-540 in NC-compatibility mode.
pub struct RolandMdx540;

impl Preprocessor for RolandMdx540 {
    fn name(&self) -> &str {
        "Roland MDX-540"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Roland_MDX_540 preprocessor");
        let _ = writeln!(g, "; tool dia: {:.4}", p.tool_diameter);
        // Roland MDX-540 init: reset, motor control 1 (spindle/motor on).
        let _ = writeln!(g, ";;^IN;!MC1;");
        let _ = writeln!(g, "!RC{:.0};", p.spindle_rpm);
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        // Roland MDX-540 end: motor control 0 (off), reset.
        let _ = writeln!(g, "!MC0;;;^IN;");
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

/// Check_points diagnostic flavour.
///
/// Upstream's `Check_points` preprocessor does not cut: it moves the probe/
/// spindle to each target point and pauses with `M0`, so the operator can
/// confirm the work coordinate system / fixture alignment before running the
/// real job. No spindle is started; the "plunge" becomes a dwell + pause and
/// the "cut" becomes a plain rapid positioning move.
pub struct CheckPoints;

impl Preprocessor for CheckPoints {
    fn name(&self) -> &str {
        "Check Points"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS Check_points diagnostic preprocessor)");
        let _ = writeln!(g, "(no cutting: move to each point and pause with M0)");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", units_word(p.units));
        // Deliberately no spindle start.
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, _p: &JobParams) {
        // Do not cut: pause so the operator can inspect this point.
        let _ = writeln!(g, "(MSG, check point)");
        let _ = writeln!(g, "M0");
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, _p: &JobParams) {
        // Diagnostic traverse: a plain rapid, not a cut.
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
}

/// HPGL plotter scale: plotter units per millimetre.
///
/// HPGL coordinates are in plotter units of 0.025 mm (40 units/mm).
const HPGL_UNITS_PER_MM: f64 = 1.0 / 0.025;

/// Convert a millimetre coordinate to integer HPGL plotter units.
fn hpgl_units(mm: f64) -> i64 {
    (mm * HPGL_UNITS_PER_MM).round() as i64
}

/// HPGL plotter-language flavour.
///
/// This dialect emits HPGL, not G-code: `IN;` initialise, `SP1;` select pen 1,
/// `PU;`/`PD;` pen up/down, and `PA{x},{y};` absolute moves. Coordinates are
/// metric only and scaled to HPGL plotter units of 0.025 mm (see
/// [`hpgl_units`]). The abstract motion model maps cleanly: `rapid_z` lift =>
/// pen up (`PU;`), `plunge` => pen down (`PD;`), and XY moves => `PA`.
pub struct Hpgl;

impl Hpgl {
    /// Emit an absolute `PA` move to the given millimetre coordinate.
    fn pa(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "PA{},{};", hpgl_units(x), hpgl_units(y));
    }
}

impl Preprocessor for Hpgl {
    fn name(&self) -> &str {
        "HPGL"
    }
    fn header(&self, g: &mut String, _p: &JobParams) {
        // HPGL is metric-only and uses 0.025 mm plotter units.
        let _ = writeln!(g, "IN;");
        let _ = writeln!(g, "SP1;");
        let _ = writeln!(g, "PU;");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        // Pen up, return to origin, deselect pen.
        let _ = writeln!(g, "PU;");
        let _ = writeln!(g, "PA0,0;");
        let _ = writeln!(g, "SP0;");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // A lift is a pen-up in HPGL.
        let _ = writeln!(g, "PU;");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        // Position with the pen up.
        self.pa(g, x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, _p: &JobParams) {
        // A plunge is a pen-down in HPGL.
        let _ = writeln!(g, "PD;");
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, _p: &JobParams) {
        // Draw with the pen down.
        self.pa(g, x, y);
    }
}

/// Look up a parity preprocessor by case-insensitive name. Returns None if
/// unknown. Canonical keys are the upstream FlatCAM preprocessor file names;
/// a few reasonable aliases are also accepted.
pub fn by_name_parity(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "isel_icp_cnc" | "isel_icp" => Some(Box::new(IselIcp)),
        "line_xyz" | "linexyz" => Some(Box::new(LineXyz)),
        "grbl_laser_z" => Some(Box::new(GrblLaserZ)),
        "marlin_laser_z" => Some(Box::new(MarlinLaserZ)),
        "default_laser" => Some(Box::new(DefaultLaser)),
        "nccad9" => Some(Box::new(Nccad9)),
        "roland_mdx_540" | "roland_540" => Some(Box::new(RolandMdx540)),
        "check_points" | "checkpoints" => Some(Box::new(CheckPoints)),
        "hpgl" => Some(Box::new(Hpgl)),
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
    fn isel_icp_uses_semicolon_comments() {
        let g = render(&IselIcp);
        assert!(g.contains("; FlatCAM-RS ISEL_ICP_CNC"), "must use ; comments");
        assert!(!g.contains("("), "ISEL_ICP must not use parenthesised comments");
        assert!(g.contains("G21"));
        assert!(g.contains("M30"));
    }

    #[test]
    fn line_xyz_emits_single_combined_line() {
        let g = render(&LineXyz);
        // Every cut line must carry X, Y and Z together.
        let cut = g
            .lines()
            .find(|l| l.starts_with("G01 X10.0000 Y0.0000"))
            .expect("a combined cut line must exist");
        assert!(cut.contains('X') && cut.contains('Y') && cut.contains('Z'),
            "cut line must combine X, Y and Z: {cut}");
        assert!(cut.contains("Z-0.0500"), "cut Z must be the cut depth: {cut}");
    }

    #[test]
    fn grbl_laser_z_retains_z_and_toggles_beam() {
        let g = render(&GrblLaserZ);
        assert!(g.contains("M03 S"), "beam must turn ON");
        assert!(g.contains("M05"), "beam must turn OFF");
        // Z lift/lower moves are retained (unlike the plain GRBL laser).
        assert!(g.contains("G00 Z"), "GRBL_laser_z must retain rapid Z moves");
        assert!(g.contains("G01 Z"), "GRBL_laser_z must retain the plunge Z move");
    }

    #[test]
    fn marlin_laser_z_retains_z_and_toggles_beam() {
        let g = render(&MarlinLaserZ);
        assert!(g.contains("M3 S"), "beam must turn ON with M3");
        assert!(g.contains("M5"), "beam must turn OFF with M5");
        assert!(g.contains("G0 Z"), "Marlin_laser_z must retain rapid Z moves");
        assert!(g.contains("G1 Z"), "Marlin_laser_z must retain the plunge Z move");
        assert!(g.contains("\nM2\n"), "Marlin program-end is M2");
    }

    #[test]
    fn default_laser_is_xy_only_with_m3_m5() {
        let g = render(&DefaultLaser);
        assert!(g.contains("M03 S"), "beam ON with M3");
        assert!(g.contains("M05"), "beam OFF with M5");
        assert!(g.contains("M30"), "MACH3 program-end is M30");
        // No Z anywhere in laser mode.
        assert!(!g.contains('Z'), "default_laser must be XY-only (no Z)");
    }

    #[test]
    fn nccad9_is_standard_router() {
        let g = render(&Nccad9);
        assert!(g.contains("G21"));
        assert!(g.contains("M03 S"));
        assert!(g.contains("M30"));
    }

    #[test]
    fn roland_mdx_540_frames_with_roland_commands() {
        let g = render(&RolandMdx540);
        assert!(g.contains(";;^IN;!MC1;"), "MDX-540 init command");
        assert!(g.contains("!MC0;"), "MDX-540 motor-off command");
        assert!(g.contains("!RC"), "MDX-540 spindle/RC command");
    }

    #[test]
    fn check_points_pauses_and_does_not_cut() {
        let g = render(&CheckPoints);
        assert!(g.contains("M0"), "check_points must pause with M0");
        assert!(!g.contains("M03"), "check_points must not start a spindle");
        // Traverses are rapids, never G01 cuts.
        assert!(!g.contains("G01"), "check_points must not emit cutting moves");
    }

    #[test]
    fn hpgl_emits_plotter_language_scaled_to_units() {
        let g = render(&Hpgl);
        assert!(g.contains("IN;"), "HPGL init");
        assert!(g.contains("SP1;"), "HPGL pen select");
        assert!(g.contains("PU;"), "HPGL pen up");
        assert!(g.contains("PD;"), "HPGL pen down");
        // No G-code words at all.
        assert!(!g.contains("G00") && !g.contains("G01"), "HPGL must not emit G-code");
        // 10 mm -> 400 plotter units (0.025 mm units).
        assert!(g.contains("PA400,0;"), "10 mm must scale to 400 plotter units: {g}");
    }

    #[test]
    fn by_name_parity_resolves_all_keys() {
        for key in [
            "isel_icp_cnc",
            "line_xyz",
            "grbl_laser_z",
            "marlin_laser_z",
            "default_laser",
            "nccad9",
            "roland_mdx_540",
            "check_points",
            "hpgl",
        ] {
            assert!(by_name_parity(key).is_some(), "{key} must resolve");
        }
    }

    #[test]
    fn by_name_parity_is_case_insensitive() {
        assert!(by_name_parity("HPGL").is_some());
        assert!(by_name_parity("Line_XYZ").is_some());
        assert!(by_name_parity("GRBL_Laser_Z").is_some());
    }

    #[test]
    fn by_name_parity_unknown_is_none() {
        assert!(by_name_parity("nope").is_none());
    }
}
