//! Copper thieving / fill (port of `ToolCopperThieving`'s core).
//!
//! Copper thieving fills the empty area of a board with a grid of small copper
//! dots so that the etching/plating process sees a more uniform copper density.
//! The added dots must stay clear of the existing copper, so a clearance ring is
//! kept around every existing feature and any candidate dot overlapping that
//! keepout is discarded.
//!
//! Strategy:
//! 1. Take the bounding box of the existing copper and grow it by `margin` to
//!    obtain the board rectangle the fill is allowed to occupy.
//! 2. Grow the copper by `clearance` ([`fc_geo::offset`]) to obtain a keepout
//!    region that the dots must avoid.
//! 3. Walk a regular grid over the board at `spacing` and place a circular dot
//!    of diameter `dot_dia` at each grid point.
//! 4. Union every candidate dot into a single fill, then subtract the keepout.
//!
//! The final `difference(grid, keepout)` removes (or clips) any dot that would
//! sit on or too close to existing copper, leaving only the dots that fall in
//! the genuinely empty board area.

use fc_geo::{bounds, circle, difference, offset, union_all, MultiPolygon, Polygon};

/// Parameters controlling the copper-thieving fill.
#[derive(Clone, Debug)]
pub struct ThievingParams {
    /// Diameter of each individual fill dot.
    pub dot_dia: f64,
    /// Grid pitch between adjacent dot centres.
    pub spacing: f64,
    /// Keep-clear distance maintained around existing copper.
    pub clearance: f64,
    /// Amount the board rectangle is grown beyond the copper bounding box.
    pub margin: f64,
}

impl Default for ThievingParams {
    fn default() -> Self {
        ThievingParams {
            dot_dia: 1.0,
            spacing: 2.0,
            clearance: 0.5,
            margin: 1.0,
        }
    }
}

/// Number of segments used to approximate each round fill dot.
const DOT_STEPS: usize = 16;

/// Generate a copper-thieving dot fill for the empty area of a board.
///
/// Returns the geometry of the fill dots that lie within the board rectangle
/// (the copper bounding box grown by `margin`) while avoiding the keepout region
/// (the copper grown by `clearance`). An empty input (no copper, hence no
/// bounds) yields an empty result.
pub fn thieving(copper: &MultiPolygon<f64>, p: &ThievingParams) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = match bounds(copper) {
        Some(b) => b,
        None => return MultiPolygon::new(vec![]),
    };

    // Board rectangle: copper bbox grown by `margin` on every side.
    let bminx = minx - p.margin;
    let bminy = miny - p.margin;
    let bmaxx = maxx + p.margin;
    let bmaxy = maxy + p.margin;

    // Keepout: existing copper grown by the clearance distance.
    let keepout = offset(copper, p.clearance);

    let radius = p.dot_dia / 2.0;
    let step = if p.spacing > 0.0 { p.spacing } else { p.dot_dia.max(1e-6) };

    // Walk a regular grid over the board and collect a dot at each point.
    let mut dots: Vec<Polygon<f64>> = Vec::new();
    let mut y = bminy;
    while y <= bmaxy + 1e-9 {
        let mut x = bminx;
        while x <= bmaxx + 1e-9 {
            dots.push(circle(x, y, radius, DOT_STEPS));
            x += step;
        }
        y += step;
    }

    if dots.is_empty() {
        return MultiPolygon::new(vec![]);
    }

    // Union all candidate dots into the full grid, then carve out the keepout.
    let grid = union_all(dots);
    difference(&grid, &keepout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, union_all, MultiPolygon};

    fn square(cx: f64, cy: f64, side: f64) -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(cx, cy, side, side)])
    }

    /// Rebuild the full (unculled) dot grid the same way `thieving` does, for
    /// comparison against the clearance-aware result.
    fn full_grid(copper: &MultiPolygon<f64>, p: &ThievingParams) -> MultiPolygon<f64> {
        let (minx, miny, maxx, maxy) = bounds(copper).unwrap();
        let bminx = minx - p.margin;
        let bminy = miny - p.margin;
        let bmaxx = maxx + p.margin;
        let bmaxy = maxy + p.margin;
        let radius = p.dot_dia / 2.0;
        let step = p.spacing;
        let mut dots = Vec::new();
        let mut y = bminy;
        while y <= bmaxy + 1e-9 {
            let mut x = bminx;
            while x <= bmaxx + 1e-9 {
                dots.push(circle(x, y, radius, DOT_STEPS));
                x += step;
            }
            y += step;
        }
        union_all(dots)
    }

    #[test]
    fn thieving_produces_nonempty_fill() {
        let copper = square(0.0, 0.0, 4.0);
        let p = ThievingParams::default();
        let fill = thieving(&copper, &p);
        assert!(!fill.0.is_empty(), "fill should contain dots");
        assert!(area(&fill) > 0.0, "fill area should be positive");
    }

    #[test]
    fn thieving_reduced_vs_full_grid() {
        // Dots near the copper square must be removed, so the clearance-aware
        // fill area is strictly less than the full untrimmed dot grid.
        let copper = square(0.0, 0.0, 4.0);
        let p = ThievingParams {
            margin: 4.0,
            ..Default::default()
        };
        let fill = thieving(&copper, &p);
        let grid = full_grid(&copper, &p);

        let fill_area = area(&fill);
        let grid_area = area(&grid);
        assert!(fill_area > 0.0, "fill should be non-empty, got {fill_area}");
        assert!(
            fill_area < grid_area,
            "trimmed fill {fill_area} should be smaller than full grid {grid_area}"
        );
    }

    #[test]
    fn thieving_empty_input_is_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let fill = thieving(&empty, &ThievingParams::default());
        assert!(fill.0.is_empty(), "no copper => no fill");
        assert!((area(&fill) - 0.0).abs() < 1e-12);
    }
}
