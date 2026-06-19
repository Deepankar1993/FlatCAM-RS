//! Corner / alignment marker geometry (port of `ToolMarkers`'s core).
//!
//! Markers are small alignment marks placed at the four corners of a board so a
//! camera or operator can register the layers. FlatCAM offers two common
//! shapes: an L-shaped corner bracket whose arms point inward along the board
//! edges, and a + cross. This module builds either at the bounding-box corners
//! of an object, with configurable arm length, thickness and margin.
//!
//! The marks are returned as a single [`MultiPolygon`] (each arm is a thin
//! rectangle, unioned together).

use fc_geo::{centered_rect, union_all, MultiPolygon, Polygon};

/// Shape of each corner marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerShape {
    /// L-shaped bracket whose two arms point inward along the board edges.
    L,
    /// + cross centred on the corner point.
    Cross,
}

/// Parameters controlling marker placement.
#[derive(Clone, Debug)]
pub struct MarkerParams {
    /// Marker shape (L bracket or + cross).
    pub shape: MarkerShape,
    /// Length of each marker arm.
    pub arm: f64,
    /// Thickness (width) of each arm.
    pub thickness: f64,
    /// Inset of the corner mark from the bounding-box corner, on each axis.
    /// Positive values move the mark inward (onto the board).
    pub margin: f64,
}

impl Default for MarkerParams {
    fn default() -> Self {
        MarkerParams {
            shape: MarkerShape::L,
            arm: 3.0,
            thickness: 0.5,
            margin: 1.0,
        }
    }
}

/// Build a single L-bracket at corner `(cx, cy)` whose arms point in directions
/// `(sx, sy)` (each ±1): one horizontal arm and one vertical arm meeting at the
/// corner point.
fn l_marker(cx: f64, cy: f64, sx: f64, sy: f64, arm: f64, thickness: f64) -> Vec<Polygon<f64>> {
    let half = thickness / 2.0;
    // Horizontal arm: spans from the corner inward by `arm` along x.
    let h = centered_rect(cx + sx * arm / 2.0, cy + sy * half, arm + thickness, thickness);
    // Vertical arm: spans from the corner inward by `arm` along y.
    let v = centered_rect(cx + sx * half, cy + sy * arm / 2.0, thickness, arm + thickness);
    vec![h, v]
}

/// Build a + cross centred on `(cx, cy)`.
fn cross_marker(cx: f64, cy: f64, arm: f64, thickness: f64) -> Vec<Polygon<f64>> {
    let span = 2.0 * arm + thickness;
    let h = centered_rect(cx, cy, span, thickness);
    let v = centered_rect(cx, cy, thickness, span);
    vec![h, v]
}

/// Place a marker at each of the four corners of `bounds`.
///
/// `bounds` is `(minx, miny, maxx, maxy)`. Each corner mark is inset toward the
/// board interior by `margin` on each axis. For [`MarkerShape::L`] the arms of
/// each bracket point inward (toward the board centre); for
/// [`MarkerShape::Cross`] a symmetric + is centred on the (inset) corner point.
pub fn corner_markers(bounds: (f64, f64, f64, f64), p: &MarkerParams) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = bounds;
    // Inset corner points and the inward direction at each.
    let corners = [
        (minx + p.margin, miny + p.margin, 1.0, 1.0),   // bottom-left, arms up/right
        (maxx - p.margin, miny + p.margin, -1.0, 1.0),  // bottom-right, arms up/left
        (maxx - p.margin, maxy - p.margin, -1.0, -1.0), // top-right, arms down/left
        (minx + p.margin, maxy - p.margin, 1.0, -1.0),  // top-left, arms down/right
    ];

    let mut polys: Vec<Polygon<f64>> = Vec::new();
    for &(cx, cy, sx, sy) in &corners {
        match p.shape {
            MarkerShape::L => polys.extend(l_marker(cx, cy, sx, sy, p.arm, p.thickness)),
            MarkerShape::Cross => polys.extend(cross_marker(cx, cy, p.arm, p.thickness)),
        }
    }
    union_all(polys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, bounds};

    #[test]
    fn four_l_markers_at_four_corners() {
        let b = (0.0, 0.0, 50.0, 30.0);
        let mp = corner_markers(b, &MarkerParams::default());
        // Four corners, each an L of two arms that meet => four separate marks.
        assert_eq!(mp.0.len(), 4, "one mark per corner");
        assert!(area(&mp) > 0.0);
    }

    #[test]
    fn four_cross_markers_at_four_corners() {
        let b = (0.0, 0.0, 50.0, 30.0);
        let p = MarkerParams { shape: MarkerShape::Cross, ..Default::default() };
        let mp = corner_markers(b, &p);
        assert_eq!(mp.0.len(), 4, "one cross per corner");
    }

    #[test]
    fn arm_length_and_thickness_honored() {
        // A single L arm has area ~ (arm + thickness) * thickness; the bracket is
        // two such arms overlapping at the corner. Increasing arm length must
        // increase total marker area.
        let b = (0.0, 0.0, 100.0, 100.0);
        let small = MarkerParams { arm: 2.0, thickness: 0.5, ..Default::default() };
        let large = MarkerParams { arm: 8.0, thickness: 0.5, ..Default::default() };
        let area_small = area(&corner_markers(b, &small));
        let area_large = area(&corner_markers(b, &large));
        assert!(area_large > area_small, "longer arms => more area");

        // Thicker arms also increase area.
        let thick = MarkerParams { arm: 2.0, thickness: 1.0, ..Default::default() };
        assert!(area(&corner_markers(b, &thick)) > area_small);
    }

    #[test]
    fn markers_lie_within_bounds_with_margin() {
        let b = (0.0, 0.0, 50.0, 30.0);
        let p = MarkerParams { shape: MarkerShape::L, arm: 3.0, thickness: 0.5, margin: 2.0 };
        let mp = corner_markers(b, &p);
        let (minx, miny, maxx, maxy) = bounds(&mp).expect("non-empty");
        // Marks are inset, so they stay inside the board bounds.
        assert!(minx >= b.0 - 1e-9 && miny >= b.1 - 1e-9);
        assert!(maxx <= b.2 + 1e-9 && maxy <= b.3 + 1e-9);
    }
}
