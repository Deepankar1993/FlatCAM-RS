//! Object statistics (port of FlatCAM's `ToolReport`).
//!
//! Pure analysis: given a [`MultiPolygon`], compute simple geometric metrics
//! (polygon count, total area, bounding box and its width/height) that the
//! original tool surfaced in its read-only report panel. No G-code, no
//! mutation, no I/O.

use fc_geo::{area, bounds, MultiPolygon};

/// Summary statistics for a multipolygon's geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct GeoReport {
    /// Number of polygons in the multipolygon.
    pub polygons: usize,
    /// Total filled area (sum over all polygons, holes subtracted).
    pub area: f64,
    /// Bounding box as `(minx, miny, maxx, maxy)`, or `None` if empty.
    pub bounds: Option<(f64, f64, f64, f64)>,
    /// Bounding-box width (`maxx - minx`), or `0.0` if there are no bounds.
    pub width: f64,
    /// Bounding-box height (`maxy - miny`), or `0.0` if there are no bounds.
    pub height: f64,
}

/// Compute a [`GeoReport`] for the given geometry.
///
/// Width and height are derived from the bounding box; an empty geometry has
/// no bounds and therefore zero width and height.
pub fn report(mp: &MultiPolygon<f64>) -> GeoReport {
    let b = bounds(mp);
    let (width, height) = match b {
        Some((minx, miny, maxx, maxy)) => (maxx - minx, maxy - miny),
        None => (0.0, 0.0),
    };
    GeoReport {
        polygons: mp.0.len(),
        area: area(mp),
        bounds: b,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{centered_rect, MultiPolygon};

    #[test]
    fn report_of_centered_rect() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 3.0, 4.0)]);
        let r = report(&mp);
        assert_eq!(r.polygons, 1);
        assert!((r.area - 12.0).abs() < 1e-9, "area {} should be ~12", r.area);
        assert!((r.width - 3.0).abs() < 1e-9, "width {} should be ~3", r.width);
        assert!(
            (r.height - 4.0).abs() < 1e-9,
            "height {} should be ~4",
            r.height
        );
        let (minx, miny, maxx, maxy) = r.bounds.expect("non-empty geometry has bounds");
        assert!((minx + 1.5).abs() < 1e-9);
        assert!((miny + 2.0).abs() < 1e-9);
        assert!((maxx - 1.5).abs() < 1e-9);
        assert!((maxy - 2.0).abs() < 1e-9);
    }

    #[test]
    fn report_of_empty_is_zero() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let r = report(&empty);
        assert_eq!(r.polygons, 0);
        assert!((r.area - 0.0).abs() < 1e-12);
        assert!(r.bounds.is_none());
        assert!((r.width - 0.0).abs() < 1e-12);
        assert!((r.height - 0.0).abs() < 1e-12);
    }

    #[test]
    fn report_counts_multiple_polygons() {
        let mp = MultiPolygon::new(vec![
            centered_rect(0.0, 0.0, 2.0, 2.0),
            centered_rect(10.0, 0.0, 2.0, 2.0),
        ]);
        let r = report(&mp);
        assert_eq!(r.polygons, 2);
        assert!((r.area - 8.0).abs() < 1e-9, "area {} should be ~8", r.area);
        // Combined bbox spans x in [-1, 11] => width 12, height 2.
        assert!((r.width - 12.0).abs() < 1e-9);
        assert!((r.height - 2.0).abs() < 1e-9);
    }
}
