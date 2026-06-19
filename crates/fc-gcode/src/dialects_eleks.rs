//! `grbl_laser_eleks_drd` preprocessor for the FlatCAM Rust port.
//!
//! This module ports upstream FlatCAM's `GRBL_laser_eleks_drd.py` preprocessor:
//! the GRBL laser dialect used on EleksMaker-class laser engravers that have
//! **no Z axis**. Its defining feature is the drill-mark behaviour: instead of
//! plunging a bit, it etches a *small circle* (a tiny `G2` arc) at every drill
//! point, so the burnt ring later helps centre a manual drill bit (`_drd` =
//! "drill ring drawing").
//!
//! ## Modelling the drill -> arc conversion within a path-based trait
//!
//! The [`Preprocessor`] trait is **path-based**, not drill-aware: it exposes
//! `header`/`footer`/`rapid_z`/`rapid_xy`/`plunge`/`linear` and has no
//! drill-specific hook. Upstream's eleks_drd, by contrast, is invoked from the
//! drill code path and emits one small ring per drill hole.
//!
//! To stay faithful within the existing trait we map the laser flow as the
//! other laser dialects do (`rapid_z` is a no-op/comment, the beam is toggled
//! with `M3/M4 S` + `M5`), and we treat each `plunge` as a **"mark this
//! point"** event. On every `plunge` we therefore emit a small `G2` ring of a
//! fixed small radius about the *current* (last positioned) point, which is the
//! closest faithful approximation of the upstream drill->arc conversion that
//! the path-based trait allows.
//!
//! A fully drill-aware version (one ring per drill hole, with hole-specific
//! radius/feed) would require extending the [`Preprocessor`] trait with a
//! dedicated drill/mark hook; that is intentionally **out of scope** here.

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

/// Radius (in document units) of the small centring ring etched at each mark.
///
/// Kept deliberately small so the ring only marks the drill centre rather than
/// removing material; it mirrors the tiny radius the upstream eleks_drd uses to
/// hint a manual drill bit's location.
const ELEKS_RING_RADIUS: f64 = 0.2;

/// `grbl_laser_eleks_drd`: EleksMaker GRBL laser, drill->ring drawing.
///
/// XY-only (no Z cutting moves): `rapid_z` is a beam-off comment, never a Z
/// motion. The beam is driven through the GRBL laser S-word: `M3 S{power}`
/// (constant power) to turn it ON, `M5` to turn it OFF; `spindle_rpm` is reused
/// as the laser power S-value, matching the other laser dialects. The header
/// sets `G21`/`G90` and idles the laser with `M3 S0`.
///
/// On each `plunge` (the laser model's "mark this point") it etches a small
/// `G2` ring instead of a hole — see the module docs for how the drill->arc
/// conversion is modelled within the path-based trait.
pub struct GrblLaserEleksDrd;

impl GrblLaserEleksDrd {
    /// Etch a small clockwise (`G2`) ring of [`ELEKS_RING_RADIUS`] around the
    /// machine's *current* position (the drill point the caller just rapided
    /// to).
    ///
    /// Because the trait gives `plunge` no coordinate and `&self` cannot hold
    /// the last position, the ring is emitted **incrementally** (`G91`) so it
    /// correctly encircles wherever the machine currently sits, then absolute
    /// mode (`G90`) is restored:
    ///
    /// 1. step out by the ring radius in +X (rim of the circle),
    /// 2. beam ON, sweep one full clockwise circle (`G02 X0 Y0 I-r J0`: end =
    ///    start, centre offset `I = -r` points back to the drill point),
    /// 3. beam OFF, step back to the drill point.
    fn etch_ring(&self, g: &mut String, p: &JobParams) {
        let r = ELEKS_RING_RADIUS;
        let _ = writeln!(g, "(eleks_drd: centring ring r{:.4} about current point)", r);
        // Incremental mode so every offset is relative to the current point.
        let _ = writeln!(g, "G91");
        // Move to the rim with the beam OFF (out +X by the radius).
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X{:.4} Y0.0000", r);
        // Beam ON, then sweep a full clockwise circle back to the rim point.
        // End offset is (0,0) (back to start); I=-r points from the rim to the
        // centre (the drill point), J=0.
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(
            g,
            "G02 X0.0000 Y0.0000 I{:.4} J0.0000 F{:.0}",
            -r, p.feed_xy
        );
        // Beam OFF and step back to the drill point, then restore absolute mode.
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X{:.4} Y0.0000", -r);
        let _ = writeln!(g, "G90");
    }
}

impl Preprocessor for GrblLaserEleksDrd {
    fn name(&self) -> &str {
        "GRBL Laser Eleks (drill ring)"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS grbl_laser_eleks_drd preprocessor)");
        let _ = writeln!(g, "(EleksMaker GRBL laser, no Z: drill points -> G2 centring rings)");
        let _ = writeln!(g, "{}", units_word(p.units));
        let _ = writeln!(g, "G90");
        // Idle the laser: beam mode with zero power so it stays dark.
        let _ = writeln!(g, "M03 S0");
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        // Ensure the beam is OFF and end the program.
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, _z: f64) {
        // No Z axis on this laser: a "lift" only guarantees the beam is OFF.
        let _ = writeln!(g, "M05");
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, _z: f64, p: &JobParams) {
        // The defining eleks_drd behaviour: a "plunge"/mark is rendered as a
        // small G2 centring ring instead of a Z hole.
        //
        // The path-based trait passes no XY argument to `plunge`, and `&self`
        // cannot carry the last-positioned point. The caller has, however, just
        // emitted a `rapid_xy` to the drill point immediately before this call,
        // so the machine is already sitting on the centre. `etch_ring`
        // therefore draws the ring *incrementally* about the current point and
        // returns the machine to it. A drill-aware trait extension could
        // instead pass the true centre; see the module docs.
        self.etch_ring(g, p);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        // A cutting move with the beam ON (XY only).
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Look up the eleks_drd preprocessor by case-insensitive name. Returns None if
/// unknown. The canonical key is the upstream file name `grbl_laser_eleks_drd`;
/// `eleks_drd` and `grbl_laser_eleks` are accepted as aliases.
pub fn by_name_eleks(name: &str) -> Option<Box<dyn Preprocessor>> {
    match name.to_ascii_lowercase().as_str() {
        "grbl_laser_eleks_drd" | "eleks_drd" | "grbl_laser_eleks" => {
            Some(Box::new(GrblLaserEleksDrd))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialects::by_name;

    /// Render a small drill-style job's motions through a preprocessor.
    ///
    /// Mirrors `CncJob::emit_drill`: rapid lift, position over a point, mark
    /// (plunge), lift — repeated for two points.
    fn render_drill(pp: &dyn Preprocessor) -> String {
        let p = JobParams::default();
        let mut g = String::new();
        pp.header(&mut g, &p);
        for &(x, y) in &[(1.0, 1.0), (2.0, 3.0)] {
            pp.rapid_z(&mut g, p.travel_z);
            pp.rapid_xy(&mut g, x, y);
            pp.plunge(&mut g, p.cut_z, &p);
            pp.rapid_z(&mut g, p.travel_z);
        }
        pp.footer(&mut g, &p);
        g
    }

    #[test]
    fn resolves_via_top_level_by_name() {
        let pp = by_name("grbl_laser_eleks_drd").expect("canonical key must resolve");
        assert_eq!(pp.name(), GrblLaserEleksDrd.name());
    }

    #[test]
    fn resolves_aliases_case_insensitively() {
        assert!(by_name("eleks_drd").is_some());
        assert!(by_name("grbl_laser_eleks").is_some());
        assert!(by_name("GRBL_Laser_Eleks_DRD").is_some());
        assert!(by_name_eleks("ELEKS_DRD").is_some());
    }

    #[test]
    fn by_name_eleks_unknown_is_none() {
        assert!(by_name_eleks("nope").is_none());
    }

    #[test]
    fn header_emits_units_and_idle_laser() {
        let g = render_drill(&GrblLaserEleksDrd);
        assert!(g.contains("G21"), "Mm job must emit G21");
        assert!(g.contains("G90"), "must emit absolute positioning");
        assert!(g.contains("M03 S0"), "laser must idle at zero power (M3 S0)");
    }

    #[test]
    fn plunge_emits_g2_arc() {
        let g = render_drill(&GrblLaserEleksDrd);
        assert!(
            g.contains("G02"),
            "each mark must etch a G2 centring ring instead of a hole"
        );
        // The beam must also be toggled around the ring.
        assert!(g.contains("M03 S"), "beam ON for the ring");
        assert!(g.contains("M05"), "beam OFF after the ring");
    }

    #[test]
    fn is_xy_only_no_z_cutting_move() {
        let g = render_drill(&GrblLaserEleksDrd);
        // No G-code motion line may carry a Z word: this laser has no Z axis.
        // (Comment lines such as the header banner are exempt.)
        for line in g.lines() {
            if line.starts_with('G') {
                assert!(
                    !line.contains('Z'),
                    "eleks_drd must be XY-only (no Z motion): {line}"
                );
            }
        }
    }
}
