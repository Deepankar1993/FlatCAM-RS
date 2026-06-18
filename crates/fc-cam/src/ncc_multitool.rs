//! Multi-tool Non-Copper-Clearing (NCC).
//!
//! Port of `ToolNCC`'s multi-tool workflow: clear all the non-copper area of a
//! board with a sequence of tools (typically large-to-small). For each tool we
//! reuse [`crate::paint`]'s line-fill core over the *non-copper* region, which is
//! the board outline (the copper bounds grown by a boundary margin) minus the
//! copper itself.
//!
//! Each returned entry pairs a tool diameter with the paint tool-paths generated
//! for that tool. The caller decides how to sequence them and whether to skip
//! already-cleared area between tools.

use crate::paint::{paint_region, PaintParams};
use fc_gcode::Polyline;
use fc_geo::{bounds, centered_rect, difference, MultiPolygon};

/// Generate NCC tool-paths for each tool diameter.
///
/// The clearing region is `board_rect − copper`, where `board_rect` is the
/// bounding box of `copper` grown by `boundary_margin` on every side. For each
/// diameter `d` in `tool_dias` a [`PaintParams`] is built (with `add_contour`
/// and the given `overlap`) and the resulting paths are returned alongside `d`.
///
/// Returns an empty `Vec` when `copper` is empty (no bounds, nothing to clear).
pub fn ncc_multitool(
    copper: &MultiPolygon<f64>,
    boundary_margin: f64,
    tool_dias: &[f64],
    overlap: f64,
) -> Vec<(f64, Vec<Polyline>)> {
    let Some((minx, miny, maxx, maxy)) = bounds(copper) else {
        return Vec::new();
    };

    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = (maxx - minx) + 2.0 * boundary_margin;
    let h = (maxy - miny) + 2.0 * boundary_margin;

    let board = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);
    let region = difference(&board, copper);

    let mut out = Vec::with_capacity(tool_dias.len());
    for &d in tool_dias {
        let pp = PaintParams {
            tool_diameter: d,
            overlap,
            margin: 0.0,
            add_contour: true,
            job: Default::default(),
        };
        out.push((d, paint_region(&region, &pp)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    #[test]
    fn empty_copper_yields_empty() {
        let copper = MultiPolygon::new(vec![]);
        let out = ncc_multitool(&copper, 2.0, &[1.0, 0.5], 0.2);
        assert!(out.is_empty());
    }

    #[test]
    fn two_tools_two_nonempty_entries() {
        // Small centered copper square inside a margin -> a ring of non-copper.
        let copper = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 4.0, 4.0)]);
        let dias = [1.5, 0.8];
        let out = ncc_multitool(&copper, 3.0, &dias, 0.2);

        assert_eq!(out.len(), 2, "two tools => two entries");
        for (i, (d, paths)) in out.iter().enumerate() {
            assert!((*d - dias[i]).abs() < 1e-9, "diameter preserved");
            assert!(!paths.is_empty(), "tool {d} should produce paths");
        }
    }
}
