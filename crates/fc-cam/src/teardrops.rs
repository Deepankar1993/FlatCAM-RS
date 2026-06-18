//! Teardrop fillets at pad/trace junctions.
//!
//! A teardrop is a small fillet of copper added where a trace meets a pad,
//! blending the abrupt right-angle transition into a smooth taper. This
//! reinforces the junction so that drill-bit wander or slight registration
//! error does not break the connection at the annular ring — improving
//! drilling robustness on the manufactured board.
//!
//! [`teardrop`] builds a simple approximation: a quadrilateral spanning the
//! two trace-edge points near the pad (offset perpendicular to the
//! pad→trace axis by half the trace width) blended forward to a single point
//! on the pad edge, taken along the pad→trace direction.

use fc_geo::{Coord, LineString, Polygon};

/// Build a teardrop fillet polygon at a pad/trace junction.
///
/// * `pad`         — centre of the pad.
/// * `pad_radius`  — radius of the (circular) pad.
/// * `trace_end`   — point where the trace centreline meets/near the pad.
/// * `trace_width` — width of the trace.
///
/// The returned polygon is a closed quadrilateral: the two trace-edge points
/// at `trace_end` (offset perpendicular to the pad→trace direction by
/// `trace_width / 2`) tapering to the point on the pad edge that lies along
/// the pad→trace direction.
pub fn teardrop(
    pad: (f64, f64),
    pad_radius: f64,
    trace_end: (f64, f64),
    trace_width: f64,
) -> Polygon<f64> {
    // Direction from the pad centre toward the trace end.
    let mut dx = trace_end.0 - pad.0;
    let mut dy = trace_end.1 - pad.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len > 1e-12 {
        dx /= len;
        dy /= len;
    } else {
        // Degenerate: trace_end coincides with pad centre. Pick +x so we
        // still return a valid, non-empty polygon.
        dx = 1.0;
        dy = 0.0;
    }

    // Unit perpendicular to the pad→trace axis.
    let px = -dy;
    let py = dx;

    let half = trace_width / 2.0;

    // Two trace-edge points straddling the trace centreline at trace_end.
    let a = (trace_end.0 + px * half, trace_end.1 + py * half);
    let b = (trace_end.0 - px * half, trace_end.1 - py * half);

    // Point on the pad edge along the pad→trace direction.
    let tip = (pad.0 + dx * pad_radius, pad.1 + dy * pad_radius);

    let coords = vec![
        Coord { x: a.0, y: a.1 },
        Coord { x: b.0, y: b.1 },
        Coord { x: tip.0, y: tip.1 },
        Coord { x: a.0, y: a.1 }, // close the ring
    ];

    Polygon::new(LineString::new(coords), vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, MultiPolygon};

    fn poly_area(p: &Polygon<f64>) -> f64 {
        area(&MultiPolygon::new(vec![p.clone()]))
    }

    #[test]
    fn teardrop_has_positive_area() {
        let td = teardrop((0.0, 0.0), 1.0, (5.0, 0.0), 1.0);
        assert!(poly_area(&td) > 0.0, "teardrop should enclose positive area");
    }

    #[test]
    fn wider_trace_gives_larger_area() {
        let narrow = teardrop((0.0, 0.0), 1.0, (5.0, 0.0), 0.5);
        let wide = teardrop((0.0, 0.0), 1.0, (5.0, 0.0), 2.0);
        assert!(
            poly_area(&wide) > poly_area(&narrow),
            "wider trace_width must yield a larger teardrop"
        );
    }

    #[test]
    fn teardrop_is_closed() {
        let td = teardrop((1.0, 1.0), 0.8, (4.0, 3.0), 1.0);
        let ring = td.exterior();
        let first = ring.0.first().unwrap();
        let last = ring.0.last().unwrap();
        assert!((first.x - last.x).abs() < 1e-12 && (first.y - last.y).abs() < 1e-12);
    }

    #[test]
    fn diagonal_trace_positive_area() {
        let td = teardrop((0.0, 0.0), 1.0, (3.0, 3.0), 1.0);
        assert!(poly_area(&td) > 0.0);
    }

    #[test]
    fn degenerate_trace_end_is_valid() {
        // trace_end at pad centre -> fallback direction, still a real polygon.
        let td = teardrop((2.0, 2.0), 1.0, (2.0, 2.0), 1.0);
        assert!(poly_area(&td) > 0.0);
    }
}
