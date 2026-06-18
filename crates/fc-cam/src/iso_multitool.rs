//! Multi-tool isolation routing.
//!
//! Given a copper region and a set of tool diameters, compute one isolation
//! ring set per tool. Each tool grows the copper outward by half its diameter
//! (`offset(copper, d/2)`) so the cutter centre clears the copper, then the
//! boundary rings of the grown geometry become the cut paths. This mirrors
//! FlatCAM's "multi tool isolation" where larger tools rough out clearance and
//! smaller tools follow for fine features.

use fc_gcode::Polyline;
use fc_geo::{offset, MultiPolygon, Polygon};

/// Compute isolation ring polylines for each tool diameter.
///
/// For every diameter `d` in `tool_dias`, the copper is grown by `d / 2.0`
/// and the exterior + interior rings of every resulting polygon are collected
/// as a `Vec<Polyline>`. Returns one `(d, paths)` pair per tool, in input
/// order. `_overlap` is reserved for future multi-pass spacing and is unused.
pub fn iso_multitool(
    copper: &MultiPolygon<f64>,
    tool_dias: &[f64],
    _overlap: f64,
) -> Vec<(f64, Vec<Polyline>)> {
    tool_dias
        .iter()
        .map(|&d| {
            let grown = offset(copper, d / 2.0);
            let mut paths = Vec::new();
            for poly in &grown.0 {
                paths.extend(ring_polylines(poly));
            }
            (d, paths)
        })
        .collect()
}

/// Collect the exterior ring and every interior (hole) ring of a polygon as
/// closed polylines.
fn ring_polylines(poly: &Polygon<f64>) -> Vec<Polyline> {
    let mut out = Vec::new();
    out.push(poly.exterior().coords().map(|c| (c.x, c.y)).collect());
    for hole in poly.interiors() {
        out.push(hole.coords().map(|c| (c.x, c.y)).collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::circle;

    #[test]
    fn two_tools_each_produce_paths() {
        // A single round pad, gerber-like.
        let copper = MultiPolygon::new(vec![circle(0.0, 0.0, 1.0, 32)]);
        let result = iso_multitool(&copper, &[0.2, 0.4], 0.0);
        assert_eq!(result.len(), 2, "two tools => two entries");
        for (d, paths) in &result {
            assert!(*d > 0.0);
            assert!(!paths.is_empty(), "each tool yields at least one path");
            assert!(paths[0].len() > 8, "ring should be a polygon");
        }
    }

    #[test]
    fn diameters_are_preserved_in_order() {
        let copper = MultiPolygon::new(vec![circle(0.0, 0.0, 1.0, 16)]);
        let result = iso_multitool(&copper, &[0.5, 0.1, 0.3], 0.0);
        let dias: Vec<f64> = result.iter().map(|(d, _)| *d).collect();
        assert_eq!(dias, vec![0.5, 0.1, 0.3]);
    }
}
