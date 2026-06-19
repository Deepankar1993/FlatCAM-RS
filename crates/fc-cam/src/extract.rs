//! Extract drill holes from Gerber pads (port of `ToolExtractDrills`'s core).
//!
//! The *Extract Drills* tool turns a Gerber's copper pads into an Excellon-style
//! drill list: each pad contributes one drill at its centre, with a diameter
//! approximated from the pad's size. A diameter range filters out pads that are
//! too small or too large to be real drills, and the resulting drills can be
//! grouped by diameter into "tools" (one tool per distinct diameter), matching
//! how Excellon files organise drills.

use fc_geo::{MultiPolygon, Polygon};
use geo::{BoundingRect, Centroid};

/// A single extracted drill: a centre point and an approximate diameter.
pub type Drill = ((f64, f64), f64);

/// Parameters controlling drill extraction.
#[derive(Clone, Debug)]
pub struct ExtractParams {
    /// Minimum acceptable drill diameter (inclusive). Pads smaller are dropped.
    pub min_dia: f64,
    /// Maximum acceptable drill diameter (inclusive). Pads larger are dropped.
    pub max_dia: f64,
}

impl Default for ExtractParams {
    fn default() -> Self {
        ExtractParams {
            min_dia: 0.0,
            max_dia: f64::INFINITY,
        }
    }
}

/// Approximate a pad's drill diameter as the smaller of its bounding-box
/// width/height (a round pad's bbox is square, so this is its diameter).
fn pad_diameter(poly: &Polygon<f64>) -> f64 {
    match poly.bounding_rect() {
        Some(r) => r.width().min(r.height()),
        None => 0.0,
    }
}

/// Centre point of a pad (its centroid).
fn pad_center(poly: &Polygon<f64>) -> Option<(f64, f64)> {
    poly.centroid().map(|p| (p.x(), p.y()))
}

/// Extract one drill per pad, filtered by the diameter range.
///
/// Each polygon in `pads` yields a drill at its centroid with a diameter
/// approximated from its bounding box. Drills whose diameter falls outside
/// `[min_dia, max_dia]` are discarded.
pub fn extract_drills(pads: &MultiPolygon<f64>, params: &ExtractParams) -> Vec<Drill> {
    let mut drills = Vec::new();
    for poly in &pads.0 {
        let dia = pad_diameter(poly);
        if dia < params.min_dia || dia > params.max_dia {
            continue;
        }
        if let Some(center) = pad_center(poly) {
            drills.push((center, dia));
        }
    }
    drills
}

/// Group extracted drills by diameter into Excellon-style tools.
///
/// Drills whose diameters differ by less than `tol` are placed in the same
/// bucket. Returns `(diameter, points)` pairs sorted by ascending diameter,
/// where `diameter` is the representative (first-seen) diameter of the bucket.
pub fn group_by_diameter(drills: &[Drill], tol: f64) -> Vec<(f64, Vec<(f64, f64)>)> {
    let mut buckets: Vec<(f64, Vec<(f64, f64)>)> = Vec::new();
    for &(pt, dia) in drills {
        match buckets.iter_mut().find(|(d, _)| (d - dia).abs() <= tol) {
            Some((_, pts)) => pts.push(pt),
            None => buckets.push((dia, vec![pt])),
        }
    }
    buckets.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{circle, MultiPolygon, Polygon};

    fn round_pad(cx: f64, cy: f64, dia: f64) -> Polygon<f64> {
        circle(cx, cy, dia / 2.0, 64)
    }

    #[test]
    fn extracts_one_drill_per_pad() {
        let pads = MultiPolygon::new(vec![
            round_pad(0.0, 0.0, 1.0),
            round_pad(10.0, 0.0, 1.0),
            round_pad(0.0, 10.0, 1.0),
        ]);
        let drills = extract_drills(&pads, &ExtractParams::default());
        assert_eq!(drills.len(), 3, "one drill per isolated pad");
        // Centres should be ~ at the pad centres.
        assert!((drills[0].0 .0).abs() < 1e-6 && (drills[0].0 .1).abs() < 1e-6);
        // Diameter ~ 1.0 (polygonal circle slightly under-estimates).
        assert!((drills[0].1 - 1.0).abs() < 0.01, "dia {}", drills[0].1);
    }

    #[test]
    fn diameter_filter_excludes_out_of_range() {
        let pads = MultiPolygon::new(vec![
            round_pad(0.0, 0.0, 0.3),  // too small
            round_pad(5.0, 0.0, 1.0),  // in range
            round_pad(10.0, 0.0, 5.0), // too large
        ]);
        let params = ExtractParams { min_dia: 0.5, max_dia: 2.0 };
        let drills = extract_drills(&pads, &params);
        assert_eq!(drills.len(), 1, "only the mid-size pad passes");
        assert!((drills[0].1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn grouping_buckets_equal_diameters() {
        let pads = MultiPolygon::new(vec![
            round_pad(0.0, 0.0, 1.0),
            round_pad(5.0, 0.0, 1.0),
            round_pad(10.0, 0.0, 2.0),
        ]);
        let drills = extract_drills(&pads, &ExtractParams::default());
        let tools = group_by_diameter(&drills, 1e-3);
        assert_eq!(tools.len(), 2, "two distinct diameters => two tools");
        // Sorted ascending: first the 1.0 bucket with 2 points, then 2.0 with 1.
        assert_eq!(tools[0].1.len(), 2);
        assert_eq!(tools[1].1.len(), 1);
        assert!(tools[0].0 < tools[1].0);
    }

    #[test]
    fn empty_input_yields_no_drills() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        assert!(extract_drills(&empty, &ExtractParams::default()).is_empty());
        assert!(group_by_diameter(&[], 1e-3).is_empty());
    }
}
