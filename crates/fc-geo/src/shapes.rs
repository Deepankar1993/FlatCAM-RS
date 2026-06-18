//! Extra shape constructors for the FlatCAM Rust port.
//!
//! These build on the primitive constructors in the crate root (`circle`,
//! `centered_rect`, `buffer_path`, `difference`) to provide the higher-level
//! shapes FlatCAM uses for pad/silkscreen flashes and routing: rounded
//! rectangles, slots (stadiums), stars and annular rings.

use crate::{buffer_path, circle, difference, Coord, LineString, MultiPolygon, Polygon};
use std::f64::consts::PI;

/// Axis-aligned rectangle `w` x `h` centred at `(cx, cy)` with rounded corners
/// of radius `r`. `r` is clamped to `min(w, h) / 2` (so the degenerate case
/// collapses to a stadium/circle rather than self-intersecting). Each corner is
/// approximated by `steps / 4` arc segments; the returned ring is closed.
pub fn rounded_rect(
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    r: f64,
    steps: usize,
) -> Polygon<f64> {
    let r = r.max(0.0).min((w.min(h)) / 2.0);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let per_corner = (steps / 4).max(1);

    // Corner arc centres, ordered CCW starting at the bottom-right corner so the
    // arcs sweep continuously around the perimeter.
    let centers = [
        (cx + hw - r, cy - hh + r, -PI / 2.0), // bottom-right, sweep -90 -> 0
        (cx + hw - r, cy + hh - r, 0.0),       // top-right,    sweep   0 -> 90
        (cx - hw + r, cy + hh - r, PI / 2.0),  // top-left,     sweep  90 -> 180
        (cx - hw + r, cy - hh + r, PI),        // bottom-left,  sweep 180 -> 270
    ];

    let mut coords: Vec<Coord<f64>> = Vec::with_capacity(per_corner * 4 + 1);
    for &(ccx, ccy, start) in &centers {
        for i in 0..=per_corner {
            let a = start + (PI / 2.0) * (i as f64) / (per_corner as f64);
            coords.push(Coord {
                x: ccx + r * a.cos(),
                y: ccy + r * a.sin(),
            });
        }
    }
    coords.push(coords[0]);
    Polygon::new(LineString::new(coords), vec![])
}

/// Slot / stadium between `(x1, y1)` and `(x2, y2)` with the given `width`
/// (rounded ends). Equivalent to buffering the segment by `width / 2`.
pub fn slot(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    width: f64,
    steps: usize,
) -> MultiPolygon<f64> {
    let pts = [Coord { x: x1, y: y1 }, Coord { x: x2, y: y2 }];
    buffer_path(&pts, width / 2.0, steps)
}

/// Star polygon centred at `(cx, cy)` with `points` spikes. Produces
/// `2 * points` vertices alternating between `outer_r` and `inner_r`, ordered
/// CCW. The returned ring is closed.
pub fn star(
    cx: f64,
    cy: f64,
    outer_r: f64,
    inner_r: f64,
    points: usize,
) -> Polygon<f64> {
    let points = points.max(2);
    let n = points * 2;
    let mut coords: Vec<Coord<f64>> = Vec::with_capacity(n + 1);
    for i in 0..n {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let a = PI / 2.0 + 2.0 * PI * (i as f64) / (n as f64);
        coords.push(Coord {
            x: cx + r * a.cos(),
            y: cy + r * a.sin(),
        });
    }
    coords.push(coords[0]);
    Polygon::new(LineString::new(coords), vec![])
}

/// Annular ring (washer) centred at `(cx, cy)`: the outer disk of radius
/// `outer_r` with the inner disk of radius `inner_r` punched out.
pub fn ring(
    cx: f64,
    cy: f64,
    outer_r: f64,
    inner_r: f64,
    steps: usize,
) -> MultiPolygon<f64> {
    let outer = MultiPolygon::new(vec![circle(cx, cy, outer_r, steps)]);
    let inner = MultiPolygon::new(vec![circle(cx, cy, inner_r, steps)]);
    difference(&outer, &inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area;

    fn ring_vertex_count(p: &Polygon<f64>) -> usize {
        // Closed ring: first == last, so subtract the duplicate.
        p.exterior().0.len() - 1
    }

    #[test]
    fn rounded_rect_area_between_inner_and_outer_bounds() {
        let (w, h, r) = (4.0, 3.0, 0.5);
        let p = rounded_rect(0.0, 0.0, w, h, r, 64);
        let mp = MultiPolygon::new(vec![p]);
        let a = area(&mp);
        let lower = (w - 2.0 * r) * (h - 2.0 * r);
        let upper = w * h;
        assert!(
            a > lower && a < upper,
            "rounded_rect area {a} not in ({lower}, {upper})"
        );
    }

    #[test]
    fn rounded_rect_clamps_radius() {
        // r larger than half the short side: must not panic / self-intersect to
        // negative area; area should still be positive and below the bbox.
        let p = rounded_rect(0.0, 0.0, 2.0, 2.0, 5.0, 64);
        let mp = MultiPolygon::new(vec![p]);
        let a = area(&mp);
        assert!(a > 0.0 && a < 4.0, "clamped area was {a}");
    }

    #[test]
    fn slot_has_positive_area() {
        let mp = slot(0.0, 0.0, 5.0, 0.0, 1.0, 32);
        assert!(area(&mp) > 0.0, "slot area was {}", area(&mp));
    }

    #[test]
    fn star_has_two_points_times_vertices() {
        let points = 5;
        let s = star(0.0, 0.0, 2.0, 1.0, points);
        assert_eq!(ring_vertex_count(&s), 2 * points);
    }

    #[test]
    fn ring_area_approximates_annulus() {
        let (outer_r, inner_r) = (3.0, 1.5);
        let mp = ring(0.0, 0.0, outer_r, inner_r, 256);
        let a = area(&mp);
        let expected = PI * (outer_r * outer_r - inner_r * inner_r);
        let rel = (a - expected).abs() / expected;
        assert!(rel < 0.02, "ring area {a} vs expected {expected} (rel {rel})");
    }
}
