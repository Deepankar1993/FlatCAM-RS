//! `fc-geo` — geometry foundation for the FlatCAM Rust port.
//!
//! This crate is the Rust analogue of the Shapely usage scattered across
//! FlatCAM's `camlib.py`. It re-exports the `geo` primitive types and provides
//! the handful of operations the CAM engine actually relies on:
//!
//! * `circle` / `regular_polygon` / `rect` — flash-geometry constructors
//! * `buffer_path` — round-capped buffering of a polyline (Shapely
//!   `LineString.buffer(r)` with round caps), used for traces and slots
//! * `union_all` / `difference` — boolean ops (Shapely `unary_union` / diff),
//!   used to merge flashes/traces and to apply clear polarity
//! * `offset` — polygon offsetting (Shapely `Polygon.buffer(±d)`), used to
//!   build isolation tool paths
//!
//! All angles are radians, all coordinates are in the document's working units
//! (inches or millimetres) — unit handling lives in the parser crates.

use geo::BooleanOps;
pub use geo::{
    Coord, Geometry, LineString, MultiPolygon, Point, Polygon, Rect,
};

pub mod geom_utils;
pub use geom_utils::{centroid, contains_point, convex_hull, simplify};
use std::f64::consts::PI;

/// Default number of segments used to approximate a full circle. FlatCAM's
/// equivalent default is `steps_per_circle = 64` for Gerber geometry.
pub const DEFAULT_CIRCLE_STEPS: usize = 64;

/// Build a closed circle polygon centred at `(cx, cy)` with the given radius.
///
/// Mirrors `Point(cx, cy).buffer(radius, steps)` for circular apertures.
pub fn circle(cx: f64, cy: f64, radius: f64, steps: usize) -> Polygon<f64> {
    let steps = steps.max(8);
    let mut coords = Vec::with_capacity(steps + 1);
    for i in 0..steps {
        let a = 2.0 * PI * (i as f64) / (steps as f64);
        coords.push(Coord {
            x: cx + radius * a.cos(),
            y: cy + radius * a.sin(),
        });
    }
    coords.push(coords[0]);
    Polygon::new(LineString::new(coords), vec![])
}

/// Build a regular N-gon (Gerber `P` aperture / macro polygon primitive).
///
/// `diameter` is the circumscribed-circle diameter, `rotation` is in degrees
/// (matching the Gerber convention), measured CCW from +X.
pub fn regular_polygon(
    cx: f64,
    cy: f64,
    diameter: f64,
    n: usize,
    rotation_deg: f64,
) -> Polygon<f64> {
    let n = n.max(3);
    let r = diameter / 2.0;
    let rot = rotation_deg.to_radians();
    let mut coords = Vec::with_capacity(n + 1);
    for i in 0..n {
        let a = rot + 2.0 * PI * (i as f64) / (n as f64);
        coords.push(Coord {
            x: cx + r * a.cos(),
            y: cy + r * a.sin(),
        });
    }
    coords.push(coords[0]);
    Polygon::new(LineString::new(coords), vec![])
}

/// Axis-aligned rectangle centred at `(cx, cy)` (Gerber `R` aperture).
pub fn centered_rect(cx: f64, cy: f64, width: f64, height: f64) -> Polygon<f64> {
    let (hw, hh) = (width / 2.0, height / 2.0);
    Rect::new(
        Coord { x: cx - hw, y: cy - hh },
        Coord { x: cx + hw, y: cy + hh },
    )
    .to_polygon()
}

/// Obround / stadium shape (Gerber `O` aperture): a rectangle with semicircular
/// caps on the shorter pair of sides. Implemented as the union of a centre
/// rectangle and two end circles, matching FlatCAM's convex-hull-of-two-circles
/// construction closely enough for tool-pathing.
pub fn obround(cx: f64, cy: f64, width: f64, height: f64, steps: usize) -> MultiPolygon<f64> {
    let mut parts: Vec<Polygon<f64>> = Vec::new();
    if width > height {
        let r = height / 2.0;
        let off = (width - height) / 2.0;
        parts.push(circle(cx - off, cy, r, steps));
        parts.push(circle(cx + off, cy, r, steps));
        parts.push(centered_rect(cx, cy, width - height, height));
    } else {
        let r = width / 2.0;
        let off = (height - width) / 2.0;
        parts.push(circle(cx, cy - off, r, steps));
        parts.push(circle(cx, cy + off, r, steps));
        parts.push(centered_rect(cx, cy, width, height - width));
    }
    union_all(parts)
}

/// Round-capped buffer of a polyline (Shapely `LineString.buffer(radius)` with
/// the default round cap/join style). Built as the union of one rectangle per
/// segment plus one circle per vertex, which yields correct round caps and
/// joins without depending on a 1-D buffering routine.
pub fn buffer_path(pts: &[Coord<f64>], radius: f64, steps: usize) -> MultiPolygon<f64> {
    if pts.is_empty() || radius <= 0.0 {
        return MultiPolygon::new(vec![]);
    }
    if pts.len() == 1 {
        return MultiPolygon::new(vec![circle(pts[0].x, pts[0].y, radius, steps)]);
    }
    let mut parts: Vec<Polygon<f64>> = Vec::with_capacity(pts.len() * 2);
    // Round caps / joins.
    for p in pts {
        parts.push(circle(p.x, p.y, radius, steps));
    }
    // Segment rectangles.
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-12 {
            continue;
        }
        let (nx, ny) = (-dy / len * radius, dx / len * radius);
        let ring = vec![
            Coord { x: a.x + nx, y: a.y + ny },
            Coord { x: b.x + nx, y: b.y + ny },
            Coord { x: b.x - nx, y: b.y - ny },
            Coord { x: a.x - nx, y: a.y - ny },
            Coord { x: a.x + nx, y: a.y + ny },
        ];
        parts.push(Polygon::new(LineString::new(ring), vec![]));
    }
    union_all(parts)
}

/// Union a collection of polygons into a single (multi)polygon. Mirrors
/// Shapely's `unary_union` over a list of polygons.
pub fn union_all(polys: Vec<Polygon<f64>>) -> MultiPolygon<f64> {
    let mut acc = MultiPolygon::new(vec![]);
    for p in polys {
        let single = MultiPolygon::new(vec![p]);
        acc = acc.union(&single);
    }
    acc
}

/// Union two multipolygons (Shapely `a.union(b)`).
pub fn union(a: &MultiPolygon<f64>, b: &MultiPolygon<f64>) -> MultiPolygon<f64> {
    a.union(b)
}

/// Subtract `b` from `a` (Shapely `a.difference(b)`), used to apply clear
/// (negative) polarity regions.
pub fn difference(a: &MultiPolygon<f64>, b: &MultiPolygon<f64>) -> MultiPolygon<f64> {
    a.difference(b)
}

/// Offset (inflate when `distance > 0`, deflate when `< 0`) a multipolygon.
/// This is the Shapely `Polygon.buffer(distance)` used to derive isolation
/// passes from a copper region.
pub fn offset(mp: &MultiPolygon<f64>, distance: f64) -> MultiPolygon<f64> {
    if distance == 0.0 {
        return mp.clone();
    }
    // `geo-buffer` is orientation-sensitive: a clockwise exterior ring is
    // treated as a hole and buffered the wrong way. Boolean-op output (i_overlay)
    // does not guarantee the CCW-exterior convention, so normalise first.
    use geo::orient::{Direction, Orient};
    let oriented = mp.orient(Direction::Default);
    geo_buffer::buffer_multi_polygon(&oriented, distance)
}

/// Total area of a multipolygon (sum of polygon areas). Convenience used by
/// tests and reporting.
pub fn area(mp: &MultiPolygon<f64>) -> f64 {
    use geo::Area;
    mp.unsigned_area()
}

/// Bounding box of a multipolygon as `(min_x, min_y, max_x, max_y)`.
pub fn bounds(mp: &MultiPolygon<f64>) -> Option<(f64, f64, f64, f64)> {
    use geo::BoundingRect;
    mp.bounding_rect()
        .map(|r| (r.min().x, r.min().y, r.max().x, r.max().y))
}

/// Affine transforms (Shapely `affinity.*` / FlatCAM `ToolTransform`).
/// Angles are in degrees.
pub mod transform {
    use super::MultiPolygon;
    use geo::{AffineOps, AffineTransform, Coord};

    pub fn translate(mp: &MultiPolygon<f64>, dx: f64, dy: f64) -> MultiPolygon<f64> {
        mp.affine_transform(&AffineTransform::translate(dx, dy))
    }

    pub fn scale(mp: &MultiPolygon<f64>, sx: f64, sy: f64, origin: (f64, f64)) -> MultiPolygon<f64> {
        let t = AffineTransform::scale(sx, sy, Coord { x: origin.0, y: origin.1 });
        mp.affine_transform(&t)
    }

    pub fn rotate(mp: &MultiPolygon<f64>, degrees: f64, origin: (f64, f64)) -> MultiPolygon<f64> {
        let t = AffineTransform::rotate(degrees, Coord { x: origin.0, y: origin.1 });
        mp.affine_transform(&t)
    }

    pub fn skew(mp: &MultiPolygon<f64>, xs: f64, ys: f64, origin: (f64, f64)) -> MultiPolygon<f64> {
        let t = AffineTransform::skew(xs, ys, Coord { x: origin.0, y: origin.1 });
        mp.affine_transform(&t)
    }

    /// Mirror across the X axis (`flip_y`) about `y = axis`.
    pub fn mirror_x(mp: &MultiPolygon<f64>, axis: f64) -> MultiPolygon<f64> {
        scale(mp, 1.0, -1.0, (0.0, axis))
    }

    /// Mirror across the Y axis (`flip_x`) about `x = axis`.
    pub fn mirror_y(mp: &MultiPolygon<f64>, axis: f64) -> MultiPolygon<f64> {
        scale(mp, -1.0, 1.0, (axis, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_area_is_approximately_pi_r_squared() {
        let c = circle(0.0, 0.0, 1.0, 256);
        let mp = MultiPolygon::new(vec![c]);
        let a = area(&mp);
        assert!((a - PI).abs() < 1e-2, "circle area was {a}");
    }

    #[test]
    fn buffer_path_makes_nonzero_area() {
        let pts = vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 10.0, y: 0.0 },
        ];
        let mp = buffer_path(&pts, 0.5, 32);
        let a = area(&mp);
        // ~ rectangle 10x1 plus two semicircles r=0.5 => 10 + pi*0.25 ≈ 10.785
        assert!(a > 10.0 && a < 11.5, "trace area was {a}");
    }

    #[test]
    fn union_merges_overlapping_circles() {
        let a = circle(0.0, 0.0, 1.0, 128);
        let b = circle(1.0, 0.0, 1.0, 128);
        let merged = union_all(vec![a, b]);
        // Two overlapping unit circles: area < 2*pi (overlap removed).
        assert!(area(&merged) < 2.0 * PI);
        assert_eq!(merged.0.len(), 1, "overlapping circles should merge to one polygon");
    }

    #[test]
    fn offset_inflates_area() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let bigger = offset(&mp, 0.5);
        assert!(area(&bigger) > area(&mp));
    }

    #[test]
    fn offset_of_circle_nonempty() {
        let mp = MultiPolygon::new(vec![circle(0.0, 0.0, 0.5, 64)]);
        let grown = offset(&mp, 0.1);
        eprintln!("circle area {} grown area {} polys {}", area(&mp), area(&grown), grown.0.len());
        assert!(grown.0.len() >= 1, "offset of circle returned empty");
        assert!(area(&grown) > area(&mp));
    }

    #[test]
    fn transforms_preserve_area_and_move() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let a0 = area(&mp);
        let moved = transform::translate(&mp, 5.0, 3.0);
        assert!((area(&moved) - a0).abs() < 1e-9);
        let (x0, y0, _, _) = bounds(&moved).unwrap();
        assert!((x0 - 4.0).abs() < 1e-9 && (y0 - 2.0).abs() < 1e-9);

        let rot = transform::rotate(&mp, 90.0, (0.0, 0.0));
        assert!((area(&rot) - a0).abs() < 1e-6);

        let scaled = transform::scale(&mp, 2.0, 3.0, (0.0, 0.0));
        assert!((area(&scaled) - a0 * 6.0).abs() < 1e-6);

        let mir = transform::mirror_y(&mp, 0.0);
        assert!((area(&mir) - a0).abs() < 1e-9);
    }

    #[test]
    fn difference_punches_hole() {
        let outer = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 4.0, 4.0)]);
        let inner = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let holed = difference(&outer, &inner);
        assert!((area(&holed) - 12.0).abs() < 1e-6, "area was {}", area(&holed));
    }
}
