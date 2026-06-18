//! Burn / fluence simulation.
//!
//! Rasterises the expected engraving produced by a tool-path under a given
//! [`BeamShape`](crate::beam::BeamShape). The result is a [`BurnMap`] — a regular
//! grid whose cells accumulate *fluence* (delivered energy per unit area). It is
//! the data behind the visual optimisation plugin's burn preview.
//!
//! ## Model
//!
//! The path is walked segment by segment. Each segment is sampled at roughly the
//! grid resolution; at every sample the elliptical beam footprint is stamped onto
//! the grid. A cell receives a contribution proportional to the local dwell:
//!
//! ```text
//! contribution = power * step / feed
//! ```
//!
//! where `step` is the distance advanced between samples. Because the long axis of
//! an elongated spot is sampled more times per unit length *along* that axis (the
//! footprint covers more cells in the travel direction), a move *along* the long
//! axis deposits more total energy than the same-length move across it — exactly
//! the uneven-darkening effect operators observe.

use crate::beam::{segment_angle, BeamShape};

/// A rasterised fluence grid. `fluence` is row-major: index `row * cols + col`.
#[derive(Clone, Debug)]
pub struct BurnMap {
    /// World X of the lower-left corner of cell `(0, 0)`.
    pub min_x: f64,
    /// World Y of the lower-left corner of cell `(0, 0)`.
    pub min_y: f64,
    /// Edge length of a (square) cell, in world units.
    pub cell: f64,
    /// Number of columns (X).
    pub cols: usize,
    /// Number of rows (Y).
    pub rows: usize,
    /// Accumulated fluence per cell, row-major (`row * cols + col`).
    pub fluence: Vec<f32>,
}

impl BurnMap {
    /// Fluence at grid cell `(col, row)`; `0.0` if out of bounds.
    pub fn at(&self, col: usize, row: usize) -> f32 {
        if col >= self.cols || row >= self.rows {
            return 0.0;
        }
        self.fluence[row * self.cols + col]
    }

    /// The peak fluence over the whole grid (`0.0` for an empty grid).
    pub fn max(&self) -> f32 {
        self.fluence.iter().copied().fold(0.0_f32, f32::max)
    }

    /// World coordinate of the centre of cell `(col, row)`.
    fn cell_center(&self, col: usize, row: usize) -> (f64, f64) {
        (
            self.min_x + (col as f64 + 0.5) * self.cell,
            self.min_y + (row as f64 + 0.5) * self.cell,
        )
    }
}

/// Largest grid we are willing to allocate; `cell` is enlarged until we fit.
const MAX_CELLS: usize = 4_000_000;

/// Simulate the burn produced by `paths` with `beam` at the given `feed`, `power`
/// and target grid resolution `cell`.
///
/// `paths` is a list of polylines (each a `Vec` of `(x, y)` points). The grid
/// covers the bounding box of all points padded by `beam.max_extent()`. If the
/// requested `cell` would exceed [`MAX_CELLS`] the cell size is enlarged.
///
/// Returns an empty (all-zero) [`BurnMap`] when there are no usable points.
pub fn simulate(
    paths: &[Vec<(f64, f64)>],
    beam: &BeamShape,
    feed: f64,
    power: f64,
    cell: f64,
) -> BurnMap {
    // ---- bounding box over all points -------------------------------------
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut n_pts = 0usize;
    for path in paths {
        for &(x, y) in path {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            n_pts += 1;
        }
    }

    if n_pts == 0 {
        return BurnMap {
            min_x: 0.0,
            min_y: 0.0,
            cell: cell.max(1e-9),
            cols: 0,
            rows: 0,
            fluence: Vec::new(),
        };
    }

    // ---- pad by the beam's longest extent ---------------------------------
    let pad = beam.max_extent().max(0.0);
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;

    let span_x = (max_x - min_x).max(1e-9);
    let span_y = (max_y - min_y).max(1e-9);

    // ---- choose a cell size that keeps the grid within MAX_CELLS -----------
    let mut cell = cell.max(1e-9);
    loop {
        let cols = (span_x / cell).ceil() as usize + 1;
        let rows = (span_y / cell).ceil() as usize + 1;
        if cols.saturating_mul(rows) <= MAX_CELLS {
            break;
        }
        cell *= 2.0;
    }
    let cols = (span_x / cell).ceil() as usize + 1;
    let rows = (span_y / cell).ceil() as usize + 1;

    let mut map = BurnMap {
        min_x,
        min_y,
        cell,
        cols,
        rows,
        fluence: vec![0.0_f32; cols * rows],
    };

    let inv_feed = 1.0 / feed.max(1e-9);
    // Footprint radius: half the longest axis, plus a cell of slack.
    let foot_r = beam.max_extent() / 2.0 + cell;

    // ---- walk every segment, stamping the footprint along the way ---------
    for path in paths {
        for win in path.windows(2) {
            let a = win[0];
            let b = win[1];
            // `segment_angle` also serves as our zero-length guard; the travel
            // direction itself is implicit in the stamped ellipse geometry.
            if segment_angle(a, b).is_none() {
                continue; // zero-length step
            }
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            // Sample roughly every `cell` along the segment, inclusive of both ends.
            let n_steps = (len / cell).ceil().max(1.0) as usize;
            let step = len / n_steps as f64;
            let contrib = (power * step * inv_feed) as f32;
            let (ux, uy) = (dx / len, dy / len);

            for k in 0..=n_steps {
                let t = k as f64 * step;
                let sx = a.0 + ux * t;
                let sy = a.1 + uy * t;
                stamp(&mut map, sx, sy, beam, foot_r, contrib);
            }
        }
    }

    map
}

/// Add `contrib` to every grid cell whose centre lies inside the beam ellipse
/// centred at `(sx, sy)` for travel direction `dir`.
fn stamp(map: &mut BurnMap, sx: f64, sy: f64, beam: &BeamShape, foot_r: f64, contrib: f32) {
    let cell = map.cell;
    // Column/row window covering the footprint bounding box.
    let c0 = (((sx - foot_r) - map.min_x) / cell).floor();
    let c1 = (((sx + foot_r) - map.min_x) / cell).ceil();
    let r0 = (((sy - foot_r) - map.min_y) / cell).floor();
    let r1 = (((sy + foot_r) - map.min_y) / cell).ceil();

    let c0 = c0.max(0.0) as usize;
    let r0 = r0.max(0.0) as usize;
    let c1 = (c1.max(0.0) as usize).min(map.cols.saturating_sub(1));
    let r1 = (r1.max(0.0) as usize).min(map.rows.saturating_sub(1));

    if map.cols == 0 || map.rows == 0 {
        return;
    }

    for row in r0..=r1 {
        for col in c0..=c1 {
            let (px, py) = map.cell_center(col, row);
            let ddx = px - sx;
            let ddy = py - sy;
            let dist = (ddx * ddx + ddy * ddy).sqrt();
            if dist <= 1e-12 {
                // Centre sample is always inside.
                map.fluence[row * map.cols + col] += contrib;
                continue;
            }
            // Ellipse-boundary radius toward this cell (machine frame).
            let to_deg = ddy.atan2(ddx).to_degrees();
            let r = beam.radius_in_dir(to_deg);
            if dist <= r {
                map.fluence[row * map.cols + col] += contrib;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paths_yield_zero_map() {
        let beam = BeamShape::circular(0.2);
        let m = simulate(&[], &beam, 1000.0, 1.0, 0.05);
        assert_eq!(m.cols, 0);
        assert_eq!(m.rows, 0);
        assert!(m.fluence.is_empty());
        assert_eq!(m.max(), 0.0);
        // Out-of-bounds access is safe.
        assert_eq!(m.at(0, 0), 0.0);
    }

    #[test]
    fn empty_path_with_no_points_yields_zero_map() {
        let beam = BeamShape::circular(0.2);
        let paths = vec![Vec::new()];
        let m = simulate(&paths, &beam, 1000.0, 1.0, 0.05);
        assert_eq!(m.max(), 0.0);
    }

    #[test]
    fn single_segment_burns_along_its_length() {
        // Elongated beam, long axis along X.
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = vec![vec![(0.0, 0.0), (10.0, 0.0)]];
        let m = simulate(&paths, &beam, 600.0, 1.0, 0.05);

        assert!(m.cols > 0 && m.rows > 0);
        assert!(m.max() > 0.0);

        // The row through y = 0 should carry fluence somewhere along the segment.
        let mid_row = ((0.0 - m.min_y) / m.cell) as usize;
        let mut found = false;
        for col in 0..m.cols {
            if m.at(col, mid_row) > 0.0 {
                found = true;
                break;
            }
        }
        assert!(found, "expected non-zero fluence along the burned segment");
    }

    #[test]
    fn long_axis_motion_burns_more_than_short_axis_motion() {
        // Horizontally elongated beam: long axis along X.
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let feed = 600.0;
        let power = 1.0;
        let cell = 0.05;

        // Same length (10), one horizontal (along long axis), one vertical.
        let horiz = vec![vec![(0.0, 0.0), (10.0, 0.0)]];
        let vert = vec![vec![(0.0, 0.0), (0.0, 10.0)]];

        let mh = simulate(&horiz, &beam, feed, power, cell);
        let mv = simulate(&vert, &beam, feed, power, cell);

        let max_h = mh.max();
        let max_v = mv.max();
        assert!(max_h > 0.0 && max_v > 0.0);
        // Moving along the long axis dwells longer => more accumulated fluence.
        assert!(
            max_h > max_v,
            "horizontal (long-axis) max {max_h} should exceed vertical (short-axis) max {max_v}"
        );
    }

    #[test]
    fn cell_is_enlarged_to_respect_cell_cap() {
        // A large area with a tiny requested cell must not allocate beyond cap.
        let beam = BeamShape::circular(0.1);
        let paths = vec![vec![(0.0, 0.0), (5000.0, 5000.0)]];
        let m = simulate(&paths, &beam, 1000.0, 1.0, 0.001);
        assert!(m.cols * m.rows <= MAX_CELLS);
        assert!(m.cell >= 0.001);
    }
}
