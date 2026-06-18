//! Panel break features: mouse-bite perforations and V-score lines.
//!
//! When several boards are panelised together they need controlled break-away
//! tabs. FlatCAM offers two common styles: a row of small drilled holes
//! ("mouse bites") that perforate a tab so it snaps cleanly, and a straight
//! V-groove ("V-score") cut that weakens the panel along a line. This module
//! provides the geometric primitives for both.

use fc_gcode::Polyline;

/// Evenly spaced drill points along the segment `a`..`b` for a mouse-bite tab.
///
/// With `count >= 2` the points include both endpoints `a` and `b` and are
/// spaced uniformly between them. With `count < 2` a single point at the
/// midpoint of the segment is returned. `_drill_dia` is accepted for API
/// symmetry with the drilling tools but does not affect placement.
pub fn mouse_bites(a: (f64, f64), b: (f64, f64), count: usize, _drill_dia: f64) -> Vec<(f64, f64)> {
    if count < 2 {
        return vec![((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)];
    }
    let segments = (count - 1) as f64;
    (0..count)
        .map(|i| {
            let t = i as f64 / segments;
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
        .collect()
}

/// A V-score line is simply the straight cut from `a` to `b`.
pub fn vscore_line(a: (f64, f64), b: (f64, f64)) -> Polyline {
    vec![a, b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_bites_five_points_inclusive_and_even() {
        let pts = mouse_bites((0.0, 0.0), (10.0, 0.0), 5, 0.5);
        assert_eq!(pts.len(), 5);
        // Inclusive of both endpoints.
        assert!((pts[0].0 - 0.0).abs() < 1e-9 && (pts[0].1).abs() < 1e-9);
        assert!((pts[4].0 - 10.0).abs() < 1e-9 && (pts[4].1).abs() < 1e-9);
        // Evenly spaced at 2.5mm.
        for i in 0..pts.len() - 1 {
            let dx = pts[i + 1].0 - pts[i].0;
            assert!((dx - 2.5).abs() < 1e-9, "spacing {dx}");
        }
    }

    #[test]
    fn mouse_bites_count_below_two_is_midpoint() {
        let one = mouse_bites((0.0, 0.0), (4.0, 2.0), 1, 0.5);
        assert_eq!(one.len(), 1);
        assert!((one[0].0 - 2.0).abs() < 1e-9 && (one[0].1 - 1.0).abs() < 1e-9);

        let zero = mouse_bites((2.0, 2.0), (6.0, 6.0), 0, 0.5);
        assert_eq!(zero.len(), 1);
        assert!((zero[0].0 - 4.0).abs() < 1e-9 && (zero[0].1 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn mouse_bites_two_points_are_the_endpoints() {
        let pts = mouse_bites((1.0, 1.0), (3.0, 5.0), 2, 0.3);
        assert_eq!(pts, vec![(1.0, 1.0), (3.0, 5.0)]);
    }

    #[test]
    fn vscore_line_is_two_points() {
        let line = vscore_line((0.0, 0.0), (10.0, 5.0));
        assert_eq!(line.len(), 2);
        assert_eq!(line, vec![(0.0, 0.0), (10.0, 5.0)]);
    }
}
