//! Cross-hatch fill for diode-laser engraving.
//!
//! A real diode-laser spot is an *ellipse*, not a point: it is wider along one
//! axis than the other. When a region is filled with parallel hatch lines, the
//! amount of energy deposited per unit area depends on the angle between the
//! hatch direction and the beam's long axis — sweeping along the long axis
//! over-burns (the spot smears more material per pass), while sweeping across
//! it under-burns. A single-angle fill therefore leaves a *directional* burn
//! bias visible as banding or uneven darkness.
//!
//! Cross-hatching fills the same region with two or more passes at *different*
//! angles (the classic case being an orthogonal 0°/90° pair, or N angle-stepped
//! passes). Because the directional error of each pass points a different way,
//! summing the passes averages the residual non-uniformity toward zero: where
//! one orientation under-burns, another over-burns, and the visible result is a
//! flatter, more even fill. This module only *generates* the fill geometry;
//! the actual hatching is delegated to [`fc_geo::hatch_lines`].

/// Smallest spacing we will hand to the hatcher; guards against zero/negative
/// spacing collapsing the scanline sweep.
const MIN_SPACING: f64 = 1e-6;

/// Cross-hatch fill: union of straight hatch passes at each angle in `angles`
/// (degrees). Returns all fill polylines concatenated (pass after pass), so a
/// later G-code step burns each orientation in turn. Empty region or empty
/// `angles` yields an empty Vec.
pub fn crosshatch_fill(
    region: &fc_geo::MultiPolygon<f64>,
    spacing: f64,
    angles: &[f64],
) -> Vec<Vec<(f64, f64)>> {
    if region.0.is_empty() || angles.is_empty() {
        return Vec::new();
    }
    // Skip degenerate spacing by clamping to a tiny positive value.
    let spacing = if spacing > 0.0 { spacing } else { MIN_SPACING };

    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    for &angle in angles {
        out.extend(fc_geo::hatch_lines(region, spacing, angle));
    }
    out
}

/// Convenience: a 0/90-style orthogonal cross-hatch around `base_angle`
/// (i.e. angles `base_angle` and `base_angle + 90`).
pub fn crosshatch_orthogonal(
    region: &fc_geo::MultiPolygon<f64>,
    spacing: f64,
    base_angle: f64,
) -> Vec<Vec<(f64, f64)>> {
    crosshatch_fill(region, spacing, &[base_angle, base_angle + 90.0])
}

/// Choose a sensible default cross-hatch for a given beam: orthogonal pair
/// aligned to the beam's mount angle, spacing derived from the beam's short
/// extent (so adjacent lines overlap). Returns the fill polylines.
/// Spacing = `beam.min_extent() * (1 - overlap)`, clamped to a small positive min.
pub fn crosshatch_for_beam(
    region: &fc_geo::MultiPolygon<f64>,
    beam: &crate::beam::BeamShape,
    overlap: f64,
) -> Vec<Vec<(f64, f64)>> {
    let overlap = overlap.clamp(0.0, 0.999);
    let spacing = (beam.min_extent() * (1.0 - overlap)).max(MIN_SPACING);
    crosshatch_orthogonal(region, spacing, beam.angle_deg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beam::BeamShape;

    const EPS: f64 = 1e-6;

    /// A 20x20 square centred at the origin.
    fn square() -> fc_geo::MultiPolygon<f64> {
        fc_geo::MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 20.0, 20.0)])
    }

    fn empty() -> fc_geo::MultiPolygon<f64> {
        fc_geo::MultiPolygon::new(vec![])
    }

    /// End-to-end direction of a polyline in degrees, folded into [0, 180).
    fn line_angle_deg(line: &[(f64, f64)]) -> f64 {
        let (x0, y0) = line[0];
        let (x1, y1) = line[line.len() - 1];
        let mut a = (y1 - y0).atan2(x1 - x0).to_degrees();
        while a < 0.0 {
            a += 180.0;
        }
        while a >= 180.0 {
            a -= 180.0;
        }
        a
    }

    #[test]
    fn orthogonal_yields_more_lines_than_single_pass() {
        let region = square();
        let spacing = 2.0;
        let single = fc_geo::hatch_lines(&region, spacing, 0.0);
        let cross = crosshatch_orthogonal(&region, spacing, 0.0);
        assert!(!single.is_empty());
        // Two passes (0° and 90°) must produce strictly more lines than one.
        assert!(
            cross.len() > single.len(),
            "cross={} single={}",
            cross.len(),
            single.len()
        );
    }

    #[test]
    fn both_orientations_present() {
        let region = square();
        let cross = crosshatch_orthogonal(&region, 2.0, 0.0);

        // Tolerance (degrees) for classifying a line's dominant direction.
        let tol = 5.0;
        let has_horizontal = cross
            .iter()
            .any(|l| line_angle_deg(l) < tol || line_angle_deg(l) > 180.0 - tol);
        let has_vertical = cross
            .iter()
            .any(|l| (line_angle_deg(l) - 90.0).abs() < tol);

        assert!(has_horizontal, "expected at least one ~horizontal line");
        assert!(has_vertical, "expected at least one ~vertical line");
    }

    #[test]
    fn empty_region_or_empty_angles_yield_empty() {
        let region = square();
        assert!(crosshatch_fill(&empty(), 2.0, &[0.0, 90.0]).is_empty());
        assert!(crosshatch_fill(&region, 2.0, &[]).is_empty());
    }

    #[test]
    fn degenerate_spacing_is_clamped_not_panicking() {
        let region = square();
        // spacing <= 0 must not panic and (with the tiny clamp) still hatches.
        let out = crosshatch_fill(&region, 0.0, &[0.0]);
        assert!(!out.is_empty());
        let out_neg = crosshatch_fill(&region, -3.0, &[0.0]);
        assert!(!out_neg.is_empty());
    }

    #[test]
    fn for_beam_nonempty_on_elongated_beam() {
        let region = square();
        // Elongated spot: long along X, short along Y.
        let beam = BeamShape { width_x: 2.0, width_y: 1.0, angle_deg: 0.0 };
        let out = crosshatch_for_beam(&region, &beam, 0.25);
        assert!(!out.is_empty());
    }

    #[test]
    fn for_beam_overlap_clamped() {
        let region = square();
        let beam = BeamShape { width_x: 2.0, width_y: 1.0, angle_deg: 0.0 };
        // overlap > 0.999 would otherwise drive spacing toward zero; the clamp
        // keeps spacing positive so the fill is still produced.
        let out = crosshatch_for_beam(&region, &beam, 5.0);
        assert!(!out.is_empty());
        // overlap = 0 -> spacing == min_extent (1.0), still non-empty.
        let beam2 = BeamShape { width_x: 2.0, width_y: 1.0, angle_deg: 0.0 };
        assert!((beam2.min_extent() - 1.0).abs() < EPS);
        let out0 = crosshatch_for_beam(&region, &beam2, 0.0);
        assert!(!out0.is_empty());
    }
}
