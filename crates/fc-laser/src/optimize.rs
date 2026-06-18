//! Fill-angle optimisation for uniform burn — the core of the laser "visual
//! optimization" plugin.
//!
//! When an elliptical diode-laser spot fills a region with parallel hatch lines,
//! the burn intensity (fluence) it deposits depends on the hatch *direction*
//! (see [`crate::beam`]): moves along the spot's long axis dwell longer and
//! over-burn, while moves along the short axis under-burn. The net result is
//! that the same region, filled at different angles, ends up with visibly
//! different fluence *uniformity*.
//!
//! This module quantifies that non-uniformity from a simulated burn map
//! ([`burn_uniformity`]) and sweeps the fill angle to find the most even result
//! ([`optimal_fill_angle`]). It builds on [`crate::simulate`] for the burn
//! simulation and [`fc_geo::hatch_lines`] for the candidate fill patterns.

use crate::beam::BeamShape;
use crate::simulate::{simulate, BurnMap};

/// Coefficient of variation (standard deviation / mean) of the fluence over the
/// **non-zero** cells of a burn map. Lower means a more uniform burn.
///
/// Empty / single-non-zero-cell maps have no meaningful spread, so this returns
/// `0.0` when fewer than two cells carry fluence. The mean is taken over the
/// non-zero cells only, so background (un-burned) area does not dilute the
/// statistic.
pub fn burn_uniformity(map: &BurnMap) -> f64 {
    let mut values: Vec<f64> = Vec::new();
    for &f in &map.fluence {
        if f > 0.0 {
            values.push(f as f64);
        }
    }
    if values.len() < 2 {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    if mean <= 0.0 {
        return 0.0;
    }
    let variance = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt() / mean
}

/// Sweep candidate fill angles and return the one that produces the most
/// uniform simulated burn.
///
/// Candidate angles are `0, 15, 30, … 165` degrees. For each, [`fc_geo::hatch_lines`]
/// produces the fill pattern, [`crate::simulate::simulate`] rasterises the burn,
/// and [`burn_uniformity`] scores it; the angle with the **lowest** coefficient
/// of variation wins.
///
/// Returns `(best_angle_deg, best_cv)`. If no candidate angle produced any fill
/// lines (e.g. an empty region), returns `(0.0, f64::INFINITY)`.
pub fn optimal_fill_angle(
    region: &fc_geo::MultiPolygon<f64>,
    beam: &BeamShape,
    spacing: f64,
    feed: f64,
    power: f64,
) -> (f64, f64) {
    let cell = (spacing / 2.0).max(0.05);

    let mut best_angle = 0.0_f64;
    let mut best_cv = f64::INFINITY;
    let mut any = false;

    let mut angle = 0.0_f64;
    while angle < 180.0 {
        let lines = fc_geo::hatch_lines(region, spacing, angle);
        if !lines.is_empty() {
            any = true;
            let map = simulate(&lines, beam, feed, power, cell);
            let cv = burn_uniformity(&map);
            if cv < best_cv {
                best_cv = cv;
                best_angle = angle;
            }
        }
        angle += 15.0;
    }

    if any {
        (best_angle, best_cv)
    } else {
        (0.0, f64::INFINITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulate::BurnMap;
    use fc_geo::{centered_rect, MultiPolygon};

    /// A burn map with every non-zero cell carrying the same fluence -> CV ~ 0.
    fn uniform_map() -> BurnMap {
        BurnMap {
            cols: 4,
            rows: 1,
            cell: 1.0,
            min_x: 0.0,
            min_y: 0.0,
            fluence: vec![0.0, 2.0, 2.0, 2.0],
        }
    }

    /// A burn map with widely-varied non-zero fluence -> CV clearly > 0.
    fn varied_map() -> BurnMap {
        BurnMap {
            cols: 4,
            rows: 1,
            cell: 1.0,
            min_x: 0.0,
            min_y: 0.0,
            fluence: vec![0.0, 1.0, 5.0, 9.0],
        }
    }

    #[test]
    fn uniform_burn_has_near_zero_cv() {
        let cv = burn_uniformity(&uniform_map());
        assert!(cv.abs() < 1e-9, "uniform CV should be ~0, was {cv}");
    }

    #[test]
    fn varied_burn_has_positive_cv() {
        let cv = burn_uniformity(&varied_map());
        assert!(cv > 0.0, "varied CV should be > 0, was {cv}");
    }

    #[test]
    fn fewer_than_two_nonzero_cells_is_zero() {
        let one = BurnMap {
            cols: 3,
            rows: 1,
            cell: 1.0,
            min_x: 0.0,
            min_y: 0.0,
            fluence: vec![0.0, 7.0, 0.0],
        };
        assert_eq!(burn_uniformity(&one), 0.0);
        let none = BurnMap {
            cols: 2,
            rows: 1,
            cell: 1.0,
            min_x: 0.0,
            min_y: 0.0,
            fluence: vec![0.0, 0.0],
        };
        assert_eq!(burn_uniformity(&none), 0.0);
    }

    #[test]
    fn optimal_fill_angle_on_square_is_finite_and_in_range() {
        // 20x20 centred square region, elongated beam.
        let region: MultiPolygon<f64> =
            MultiPolygon::new(vec![centered_rect(0.0, 0.0, 20.0, 20.0)]);
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let (angle, cv) = optimal_fill_angle(&region, &beam, 0.3, 600.0, 1.0);
        assert!((0.0..180.0).contains(&angle), "angle out of range: {angle}");
        assert!(cv.is_finite() && cv >= 0.0, "cv should be finite and >= 0, was {cv}");
    }

    #[test]
    fn empty_region_returns_infinite_cv() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let beam = BeamShape::circular(0.1);
        let (angle, cv) = optimal_fill_angle(&empty, &beam, 0.3, 600.0, 1.0);
        assert_eq!(angle, 0.0);
        assert!(cv.is_infinite());
    }
}
