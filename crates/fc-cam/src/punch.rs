//! Punch holes into Gerber pads (port of `ToolPunchGerber`'s core).
//!
//! Punching turns solid copper pads into annular rings by subtracting a small
//! centred circle ("hole") from every pad. This is what FlatCAM's *Punch Gerber*
//! tool does so that, e.g., a SMD pad becomes a ring you can then drill or so a
//! plated-through pad exposes its hole.
//!
//! Three diameter modes mirror the upstream UI:
//! * [`PunchMode::Fixed`] — every pad gets a hole of the same diameter.
//! * [`PunchMode::Proportional`] — the hole diameter is a fraction of the pad's
//!   own size (the smaller of its bounding-box width/height).
//! * [`PunchMode::ExcellonDerived`] — explicit `(point, diameter)` drills; each
//!   hole is punched at the given point with the given diameter (the pad
//!   containing/nearest that point gets the ring).
//!
//! Each pad polygon has its hole subtracted independently; the result is the
//! union of all the resulting rings as a single [`MultiPolygon`].

use fc_geo::{circle, difference, union_all, MultiPolygon, Polygon};
use geo::{BoundingRect, Centroid};

/// Number of segments used to approximate each punched hole.
const HOLE_STEPS: usize = 32;

/// How the diameter of each punched hole is determined.
#[derive(Clone, Debug)]
pub enum PunchMode {
    /// Every pad gets a hole of this fixed diameter.
    Fixed(f64),
    /// Hole diameter = `factor` × the pad's smaller bounding-box dimension.
    /// `factor` is clamped to `0.0..1.0`.
    Proportional(f64),
    /// Explicit drills: punch a hole of `diameter` centred at `point`.
    ExcellonDerived(Vec<((f64, f64), f64)>),
}

/// Parameters controlling the punch operation.
#[derive(Clone, Debug)]
pub struct PunchParams {
    /// Diameter mode (fixed / proportional / excellon-derived).
    pub mode: PunchMode,
}

impl Default for PunchParams {
    fn default() -> Self {
        PunchParams {
            mode: PunchMode::Fixed(0.5),
        }
    }
}

/// Centre point of a pad polygon (its centroid).
fn pad_center(poly: &Polygon<f64>) -> Option<(f64, f64)> {
    poly.centroid().map(|p| (p.x(), p.y()))
}

/// Smaller of a pad's bounding-box width/height (its characteristic "size").
fn pad_size(poly: &Polygon<f64>) -> f64 {
    match poly.bounding_rect() {
        Some(r) => (r.width()).min(r.height()),
        None => 0.0,
    }
}

/// Punch holes into `pads`, turning each pad into an annular ring.
///
/// Returns the union of the resulting rings. Pads whose computed hole diameter
/// is non-positive are left solid. For [`PunchMode::ExcellonDerived`] the holes
/// are punched at the supplied points regardless of pad membership (a hole that
/// misses every pad simply has no effect).
pub fn punch_gerber(pads: &MultiPolygon<f64>, params: &PunchParams) -> MultiPolygon<f64> {
    if pads.0.is_empty() {
        return MultiPolygon::new(vec![]);
    }

    match &params.mode {
        PunchMode::Fixed(d) => punch_uniform(pads, |_| *d),
        PunchMode::Proportional(factor) => {
            let f = factor.clamp(0.0, 1.0);
            punch_uniform(pads, |poly| pad_size(poly) * f)
        }
        PunchMode::ExcellonDerived(drills) => {
            // Build one cutter circle per drill, then subtract from all pads.
            let cutters: Vec<Polygon<f64>> = drills
                .iter()
                .filter(|(_, d)| *d > 0.0)
                .map(|(pt, d)| circle(pt.0, pt.1, d / 2.0, HOLE_STEPS))
                .collect();
            if cutters.is_empty() {
                return pads.clone();
            }
            let cutter = union_all(cutters);
            difference(pads, &cutter)
        }
    }
}

/// Punch one centred hole per pad, sizing each hole via `dia_of`.
fn punch_uniform<F>(pads: &MultiPolygon<f64>, dia_of: F) -> MultiPolygon<f64>
where
    F: Fn(&Polygon<f64>) -> f64,
{
    let mut cutters: Vec<Polygon<f64>> = Vec::new();
    for poly in &pads.0 {
        let d = dia_of(poly);
        if d <= 0.0 {
            continue;
        }
        if let Some((cx, cy)) = pad_center(poly) {
            cutters.push(circle(cx, cy, d / 2.0, HOLE_STEPS));
        }
    }
    if cutters.is_empty() {
        return pads.clone();
    }
    let cutter = union_all(cutters);
    difference(pads, &cutter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, MultiPolygon};

    fn pad(cx: f64, cy: f64, side: f64) -> Polygon<f64> {
        centered_rect(cx, cy, side, side)
    }

    #[test]
    fn fixed_mode_removes_circular_hole() {
        let pads = MultiPolygon::new(vec![pad(0.0, 0.0, 4.0)]);
        let before = area(&pads);
        let result = punch_gerber(&pads, &PunchParams { mode: PunchMode::Fixed(2.0) });

        // Area must drop by roughly the hole area (pi r^2 with r=1 => ~pi).
        let after = area(&result);
        assert!(after < before, "punch must reduce area: {after} < {before}");
        let hole_area = std::f64::consts::PI * 1.0 * 1.0;
        assert!(
            (before - after - hole_area).abs() < 0.05,
            "removed ~{} expected ~{hole_area}",
            before - after
        );

        // The pad became an annular ring => its polygon has an interior.
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0].interiors().len(), 1, "ring should have a hole");
    }

    #[test]
    fn proportional_scales_with_pad() {
        let small = MultiPolygon::new(vec![pad(0.0, 0.0, 2.0)]);
        let large = MultiPolygon::new(vec![pad(0.0, 0.0, 6.0)]);
        let p = PunchParams { mode: PunchMode::Proportional(0.5) };

        // small pad size 2 => hole dia 1 => hole area ~ pi*0.25.
        let removed_small = area(&small) - area(&punch_gerber(&small, &p));
        // large pad size 6 => hole dia 3 => hole area ~ pi*2.25.
        let removed_large = area(&large) - area(&punch_gerber(&large, &p));

        assert!(removed_small > 0.0 && removed_large > 0.0);
        assert!(
            removed_large > removed_small * 3.0,
            "larger pad punches proportionally larger: {removed_large} vs {removed_small}"
        );
    }

    #[test]
    fn excellon_derived_punches_at_points() {
        // Two pads; only one has a matching drill point.
        let pads = MultiPolygon::new(vec![pad(0.0, 0.0, 4.0), pad(20.0, 0.0, 4.0)]);
        let drills = vec![((0.0, 0.0), 2.0)];
        let result = punch_gerber(
            &pads,
            &PunchParams { mode: PunchMode::ExcellonDerived(drills) },
        );

        assert_eq!(result.0.len(), 2, "still two pads");
        // Exactly one pad should now have an interior ring.
        let with_holes: usize = result.0.iter().filter(|p| !p.interiors().is_empty()).count();
        assert_eq!(with_holes, 1, "only the drilled pad becomes a ring");
    }

    #[test]
    fn empty_input_is_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let result = punch_gerber(&empty, &PunchParams::default());
        assert!(result.0.is_empty());
    }
}
