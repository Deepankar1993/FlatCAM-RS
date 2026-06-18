//! Bed-levelling height-map application (port of `ToolLevelling`'s adjust step).
//!
//! After probing produces a set of measured surface samples (see
//! [`crate::levelling`] for generating the probe positions), the recorded
//! `(x, y, z)` triples form a sparse height map of the work surface. To make a
//! tool follow a warped board, every planar `(x, y)` point in a path is given a
//! Z offset interpolated from the nearby probe samples.
//!
//! Interpolation uses inverse-distance weighting (IDW, power 2): each sample
//! contributes in proportion to `1 / dist^2`, so nearer probes dominate. With
//! no samples the offset is zero (flat surface assumed); a query landing on a
//! probe returns that probe's exact Z.

use fc_gcode::Polyline;

/// A sparse map of measured surface heights.
///
/// Each entry is an `(x, y, z)` probe sample. `z` is the deviation (or absolute
/// height) measured at `(x, y)`; the units are whatever the probing used.
#[derive(Clone, Debug, Default)]
pub struct HeightMap {
    /// Probe samples as `(x, y, z)`.
    pub points: Vec<(f64, f64, f64)>,
}

impl HeightMap {
    /// Interpolate the surface height at `(x, y)` via inverse-distance weighting.
    ///
    /// Returns `0.0` when the map is empty. If a probe sample coincides with the
    /// query (within `1e-9`), that sample's `z` is returned exactly, avoiding a
    /// division by zero and honouring measured points.
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }

        let mut weight_sum = 0.0;
        let mut weighted_z = 0.0;
        for &(px, py, pz) in &self.points {
            let dx = px - x;
            let dy = py - y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < 1e-18 {
                // dist < 1e-9: coincident probe, return it exactly.
                return pz;
            }
            let w = 1.0 / dist_sq; // IDW power 2: 1 / dist^2
            weight_sum += w;
            weighted_z += w * pz;
        }
        weighted_z / weight_sum
    }
}

/// Apply a height map to a set of planar tool paths.
///
/// Each `(x, y)` vertex becomes `(x, y, base_z + map.sample(x, y))`, lifting or
/// dropping the cut Z to follow the probed surface.
pub fn apply_paths(
    paths: &[Polyline],
    map: &HeightMap,
    base_z: f64,
) -> Vec<Vec<(f64, f64, f64)>> {
    paths
        .iter()
        .map(|path| {
            path.iter()
                .map(|&(x, y)| (x, y, base_z + map.sample(x, y)))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_samples_zero() {
        let map = HeightMap::default();
        assert_eq!(map.sample(1.0, 2.0), 0.0);
        assert_eq!(map.sample(-5.0, 7.0), 0.0);
    }

    #[test]
    fn empty_map_apply_gives_base_z() {
        let map = HeightMap::default();
        let paths: Vec<Polyline> = vec![vec![(0.0, 0.0), (1.0, 1.0)]];
        let out = apply_paths(&paths, &map, 3.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], vec![(0.0, 0.0, 3.0), (1.0, 1.0, 3.0)]);
    }

    #[test]
    fn flat_map_offsets_by_constant() {
        let map = HeightMap {
            points: vec![
                (0.0, 0.0, 0.5),
                (10.0, 0.0, 0.5),
                (0.0, 10.0, 0.5),
                (10.0, 10.0, 0.5),
            ],
        };
        // Anywhere away from probes, IDW of equal z's is that z.
        assert!((map.sample(3.0, 7.0) - 0.5).abs() < 1e-12);

        let paths: Vec<Polyline> = vec![vec![(3.0, 7.0)]];
        let out = apply_paths(&paths, &map, -1.0);
        let (_, _, z) = out[0][0];
        assert!((z - (-1.0 + 0.5)).abs() < 1e-12);
    }

    #[test]
    fn midpoint_between_two_probes_is_between() {
        let map = HeightMap {
            points: vec![(0.0, 0.0, 0.0), (10.0, 0.0, 2.0)],
        };
        // Equidistant midpoint: equal weights -> average.
        let mid = map.sample(5.0, 0.0);
        assert!((mid - 1.0).abs() < 1e-12, "midpoint should average, got {mid}");

        // Closer to the low probe -> below the average.
        let near_low = map.sample(2.0, 0.0);
        assert!(near_low < 1.0 && near_low > 0.0, "got {near_low}");

        // Closer to the high probe -> above the average.
        let near_high = map.sample(8.0, 0.0);
        assert!(near_high > 1.0 && near_high < 2.0, "got {near_high}");
    }

    #[test]
    fn coincident_probe_returns_exact_z() {
        let map = HeightMap {
            points: vec![(0.0, 0.0, 0.0), (10.0, 0.0, 2.0), (4.0, 9.0, -3.5)],
        };
        assert_eq!(map.sample(4.0, 9.0), -3.5);
        assert_eq!(map.sample(10.0, 0.0), 2.0);
        // Within tolerance also snaps.
        assert_eq!(map.sample(4.0 + 1e-12, 9.0 - 1e-12), -3.5);
    }

    #[test]
    fn apply_preserves_xy_and_path_structure() {
        let map = HeightMap {
            points: vec![(0.0, 0.0, 1.0)],
        };
        let paths: Vec<Polyline> = vec![vec![(2.0, 3.0), (4.0, 5.0)], vec![(6.0, 7.0)]];
        let out = apply_paths(&paths, &map, 0.0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 2);
        assert_eq!(out[1].len(), 1);
        // Single probe at z=1.0 -> every point offset by exactly 1.0.
        for ring in &out {
            for &(_, _, z) in ring {
                assert!((z - 1.0).abs() < 1e-12);
            }
        }
        assert_eq!((out[0][0].0, out[0][0].1), (2.0, 3.0));
    }
}
