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

use fc_geo::{bounds, centered_rect, circle, difference, offset, union_all, MultiPolygon, Polygon};

/// Fill pattern used to occupy the empty board area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillPattern {
    /// A grid of round dots (the original behaviour).
    #[default]
    Dots,
    /// A grid of small squares.
    Squares,
    /// Parallel horizontal stripes.
    Lines,
    /// A single solid fill covering the whole board area.
    Solid,
}

/// Parameters controlling the copper-thieving fill.
#[derive(Clone, Debug)]
pub struct ThievingParams {
    /// Diameter of each individual fill dot (for [`FillPattern::Dots`]) or the
    /// side of each square ([`FillPattern::Squares`]) / width of each stripe
    /// ([`FillPattern::Lines`]).
    pub dot_dia: f64,
    /// Grid pitch between adjacent dot centres / stripe centres.
    pub spacing: f64,
    /// Keep-clear distance maintained around existing copper.
    pub clearance: f64,
    /// Amount the board rectangle is grown beyond the copper bounding box.
    pub margin: f64,
    /// Fill pattern to use. Defaults to [`FillPattern::Dots`] to preserve the
    /// original behaviour.
    pub pattern: FillPattern,
    /// Optional robber bar: a frame strip of this width laid around the board
    /// edge. `None` (or a non-positive width) means no robber bar.
    pub robber_bar: Option<f64>,
}

impl Default for ThievingParams {
    fn default() -> Self {
        ThievingParams {
            dot_dia: 1.0,
            spacing: 2.0,
            clearance: 0.5,
            margin: 1.0,
            pattern: FillPattern::Dots,
            robber_bar: None,
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

    // Build the raw fill geometry for the chosen pattern over the board rect.
    let raw = build_pattern(p, (bminx, bminy, bmaxx, bmaxy));

    // Carve out the keepout so the fill stays clear of existing copper.
    let mut fill = if raw.0.is_empty() {
        raw
    } else {
        difference(&raw, &keepout)
    };

    // Optional robber bar: a frame strip around the board edge, also kept clear
    // of existing copper. It is unioned onto the fill.
    if let Some(w) = p.robber_bar {
        if w > 0.0 {
            let bar = robber_bar_geo((bminx, bminy, bmaxx, bmaxy), w);
            let bar = difference(&bar, &keepout);
            fill = fc_geo::union(&fill, &bar);
        }
    }

    fill
}

/// Build the raw (un-clipped) fill geometry for `p.pattern` over the board
/// rectangle `(bminx, bminy, bmaxx, bmaxy)`.
fn build_pattern(p: &ThievingParams, board: (f64, f64, f64, f64)) -> MultiPolygon<f64> {
    let (bminx, bminy, bmaxx, bmaxy) = board;
    let step = if p.spacing > 0.0 { p.spacing } else { p.dot_dia.max(1e-6) };

    match p.pattern {
        FillPattern::Solid => {
            let cx = (bminx + bmaxx) / 2.0;
            let cy = (bminy + bmaxy) / 2.0;
            MultiPolygon::new(vec![centered_rect(cx, cy, bmaxx - bminx, bmaxy - bminy)])
        }
        FillPattern::Lines => {
            // Parallel horizontal stripes of width `dot_dia`, pitched by `step`.
            let w = bmaxx - bminx;
            let cx = (bminx + bmaxx) / 2.0;
            let mut stripes: Vec<Polygon<f64>> = Vec::new();
            let mut y = bminy;
            while y <= bmaxy + 1e-9 {
                stripes.push(centered_rect(cx, y, w, p.dot_dia));
                y += step;
            }
            if stripes.is_empty() {
                return MultiPolygon::new(vec![]);
            }
            union_all(stripes)
        }
        FillPattern::Dots | FillPattern::Squares => {
            let radius = p.dot_dia / 2.0;
            let mut cells: Vec<Polygon<f64>> = Vec::new();
            let mut y = bminy;
            while y <= bmaxy + 1e-9 {
                let mut x = bminx;
                while x <= bmaxx + 1e-9 {
                    let cell = match p.pattern {
                        FillPattern::Squares => centered_rect(x, y, p.dot_dia, p.dot_dia),
                        _ => circle(x, y, radius, DOT_STEPS),
                    };
                    cells.push(cell);
                    x += step;
                }
                y += step;
            }
            if cells.is_empty() {
                return MultiPolygon::new(vec![]);
            }
            union_all(cells)
        }
    }
}

/// Parameters for a pattern-plating mask.
#[derive(Clone, Debug)]
pub struct PlatingMaskParams {
    /// Extra clearance grown around both the existing copper and the thieving
    /// fill so the plating resist comfortably covers (and overlaps) them.
    pub clearance: f64,
    /// Amount the board rectangle is grown beyond the copper/fill bounding box.
    pub margin: f64,
}

impl Default for PlatingMaskParams {
    fn default() -> Self {
        PlatingMaskParams {
            clearance: 0.1,
            margin: 1.0,
        }
    }
}

/// Generate the pattern-plating resist mask for a copper-thieving fill.
///
/// In pattern plating the photoresist mask is the *cover* geometry: it must
/// expose (leave open) every region that will be electroplated — the existing
/// copper plus the thieving fill — and cover everything else on the board. The
/// returned geometry is therefore the board rectangle minus the union of the
/// copper and the fill, each grown by `clearance` so the mask overlaps the
/// plated features by that amount (preventing resist from intruding onto a
/// feature edge).
///
/// `copper` is the existing copper; `fill` is the thieving fill produced by
/// [`thieving`]. The board rectangle is the combined bounding box of the two
/// grown by `margin`. An empty board (no copper and no fill) yields an empty
/// mask.
pub fn plating_mask(
    copper: &MultiPolygon<f64>,
    fill: &MultiPolygon<f64>,
    p: &PlatingMaskParams,
) -> MultiPolygon<f64> {
    // Combined extent of copper + fill.
    let plated = fc_geo::union(copper, fill);
    let (minx, miny, maxx, maxy) = match bounds(&plated) {
        Some(b) => b,
        None => return MultiPolygon::new(vec![]),
    };

    let bminx = minx - p.margin;
    let bminy = miny - p.margin;
    let bmaxx = maxx + p.margin;
    let bmaxy = maxy + p.margin;
    let cx = (bminx + bmaxx) / 2.0;
    let cy = (bminy + bmaxy) / 2.0;
    let board = MultiPolygon::new(vec![centered_rect(
        cx,
        cy,
        bmaxx - bminx,
        bmaxy - bminy,
    )]);

    // The opening (un-masked / to-be-plated) region: everything that will be
    // plated, grown by the clearance so the mask overlaps feature edges.
    let opening = if p.clearance > 0.0 {
        offset(&plated, p.clearance)
    } else {
        plated
    };

    // Mask covers the board everywhere except the openings.
    difference(&board, &opening)
}

/// A robber-bar frame of width `w` around the board rectangle: the board
/// rectangle minus its inward-offset interior, leaving a border strip.
fn robber_bar_geo(board: (f64, f64, f64, f64), w: f64) -> MultiPolygon<f64> {
    let (bminx, bminy, bmaxx, bmaxy) = board;
    let cx = (bminx + bmaxx) / 2.0;
    let cy = (bminy + bmaxy) / 2.0;
    let outer_w = bmaxx - bminx;
    let outer_h = bmaxy - bminy;
    let outer = MultiPolygon::new(vec![centered_rect(cx, cy, outer_w, outer_h)]);
    let inner_w = (outer_w - 2.0 * w).max(0.0);
    let inner_h = (outer_h - 2.0 * w).max(0.0);
    if inner_w <= 0.0 || inner_h <= 0.0 {
        return outer;
    }
    let inner = MultiPolygon::new(vec![centered_rect(cx, cy, inner_w, inner_h)]);
    difference(&outer, &inner)
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

    #[test]
    fn squares_pattern_produces_fill() {
        let copper = square(0.0, 0.0, 4.0);
        let p = ThievingParams {
            margin: 4.0,
            pattern: FillPattern::Squares,
            ..Default::default()
        };
        let fill = thieving(&copper, &p);
        assert!(area(&fill) > 0.0, "squares fill should be non-empty");
    }

    #[test]
    fn lines_pattern_produces_stripes() {
        let copper = square(0.0, 0.0, 4.0);
        let p = ThievingParams {
            margin: 4.0,
            pattern: FillPattern::Lines,
            ..Default::default()
        };
        let fill = thieving(&copper, &p);
        assert!(area(&fill) > 0.0, "lines fill should be non-empty");
    }

    #[test]
    fn solid_pattern_fills_board_minus_copper() {
        // Solid fill over a big margin must cover (nearly) the whole board minus
        // the keepout, so its area exceeds a sparse dot grid's.
        let copper = square(0.0, 0.0, 4.0);
        let p_solid = ThievingParams {
            margin: 4.0,
            pattern: FillPattern::Solid,
            ..Default::default()
        };
        let p_dots = ThievingParams {
            margin: 4.0,
            pattern: FillPattern::Dots,
            ..Default::default()
        };
        let solid = thieving(&copper, &p_solid);
        let dots = thieving(&copper, &p_dots);
        assert!(
            area(&solid) > area(&dots),
            "solid fill {} should exceed dot fill {}",
            area(&solid),
            area(&dots)
        );
    }

    #[test]
    fn plating_mask_covers_board_minus_plated() {
        // Copper square + a thieving fill. The plating mask should be non-empty
        // (it covers the empty board area) and strictly smaller than the full
        // board rectangle (the plated openings are carved out).
        let copper = square(0.0, 0.0, 4.0);
        let tp = ThievingParams { margin: 6.0, spacing: 3.0, ..Default::default() };
        let fill = thieving(&copper, &tp);
        let mp = PlatingMaskParams::default();
        let mask = plating_mask(&copper, &fill, &mp);
        assert!(area(&mask) > 0.0, "mask should cover the empty board area");

        // Compare to the full board rectangle.
        let (minx, miny, maxx, maxy) = bounds(&fc_geo::union(&copper, &fill)).unwrap();
        let bw = (maxx - minx) + 2.0 * mp.margin;
        let bh = (maxy - miny) + 2.0 * mp.margin;
        let board_area = bw * bh;
        assert!(
            area(&mask) < board_area,
            "mask {} must be smaller than board {}",
            area(&mask),
            board_area
        );
    }

    #[test]
    fn plating_mask_clearance_grows_openings() {
        // Larger clearance => larger openings => smaller mask.
        let copper = square(0.0, 0.0, 4.0);
        let tp = ThievingParams { margin: 6.0, spacing: 3.0, ..Default::default() };
        let fill = thieving(&copper, &tp);
        let small = plating_mask(&copper, &fill, &PlatingMaskParams { clearance: 0.1, margin: 2.0 });
        let big = plating_mask(&copper, &fill, &PlatingMaskParams { clearance: 0.5, margin: 2.0 });
        assert!(
            area(&big) < area(&small),
            "more clearance opens more, mask shrinks: {} vs {}",
            area(&big),
            area(&small)
        );
    }

    #[test]
    fn plating_mask_empty_input_is_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let mask = plating_mask(&empty, &empty, &PlatingMaskParams::default());
        assert!(mask.0.is_empty(), "no copper/fill => empty mask");
    }

    #[test]
    fn robber_bar_adds_border_strip() {
        // Adding a robber bar to a solid fill should not shrink area; for a
        // sparse pattern it strictly increases the filled area (the frame).
        let copper = square(0.0, 0.0, 4.0);
        let base = ThievingParams {
            margin: 6.0,
            pattern: FillPattern::Dots,
            spacing: 3.0,
            ..Default::default()
        };
        let mut with_bar = base.clone();
        with_bar.robber_bar = Some(0.6);

        let a0 = area(&thieving(&copper, &base));
        let a1 = area(&thieving(&copper, &with_bar));
        assert!(
            a1 > a0,
            "robber bar must add a border strip: {a1} should be > {a0}"
        );
    }
}
