//! `panelize` — board panelization and double-sided mirroring.
//!
//! Port of the geometry cores of FlatCAM's `ToolPanelize` and `ToolDblSided`
//! plugins. Panelization tiles a single board's geometry into an `nx`-by-`ny`
//! grid (Shapely `affinity.translate` + `unary_union` in the original);
//! double-sided mirroring flips the geometry about a vertical axis so the
//! bottom-side artwork lines up with the top once the board is flipped over.
//!
//! These functions are GUI-free and operate purely on [`MultiPolygon`]s.

use fc_geo::transform;
use fc_geo::{bounds, union_all, MultiPolygon};

/// Tile `src` into an `nx`-by-`ny` grid.
///
/// `dx` and `dy` are the step (pitch) between adjacent copies' origins —
/// typically the board width plus a gutter and the board height plus a gutter,
/// respectively. Copy `(col, row)` is `src` translated by `(col*dx, row*dy)`
/// for `col` in `0..nx` and `row` in `0..ny`. All copies are merged into a
/// single [`MultiPolygon`] (Shapely `unary_union`).
pub fn panelize(src: &MultiPolygon<f64>, nx: usize, ny: usize, dx: f64, dy: f64) -> MultiPolygon<f64> {
    let mut parts = Vec::new();
    for col in 0..nx {
        for row in 0..ny {
            let copy = transform::translate(src, col as f64 * dx, row as f64 * dy);
            parts.extend(copy.0);
        }
    }
    union_all(parts)
}

/// Auto-pitch panelize: derive the grid pitch from the source bounding box
/// plus a `gutter` of empty space between adjacent copies.
///
/// `dx = width + gutter`, `dy = height + gutter`, where `width`/`height` come
/// from [`bounds`]. An empty source yields an empty panel.
pub fn panelize_auto(src: &MultiPolygon<f64>, nx: usize, ny: usize, gutter: f64) -> MultiPolygon<f64> {
    let (dx, dy) = match bounds(src) {
        Some((minx, miny, maxx, maxy)) => (maxx - minx + gutter, maxy - miny + gutter),
        None => return MultiPolygon::new(vec![]),
    };
    panelize(src, nx, ny, dx, dy)
}

/// Mirror geometry for the bottom side of a double-sided board, about a
/// vertical axis `x = axis_x` (Shapely `affinity.scale(..., xfact=-1)`).
///
/// This is FlatCAM's "Point/Box" mirror for the bottom layer: after physically
/// flipping the board left-to-right, the mirrored artwork registers with the
/// top side.
pub fn mirror_for_bottom(src: &MultiPolygon<f64>, axis_x: f64) -> MultiPolygon<f64> {
    transform::mirror_y(src, axis_x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect};

    fn src() -> MultiPolygon<f64> {
        // Unit-ish board: 2x2 square centred at (1,1), area 4.
        MultiPolygon::new(vec![centered_rect(1.0, 1.0, 2.0, 2.0)])
    }

    #[test]
    fn panelize_tiles_non_overlapping() {
        let s = src();
        let (sx0, sy0, sx1, sy1) = bounds(&s).unwrap();

        let panel = panelize(&s, 2, 2, 10.0, 10.0);
        // 4 non-overlapping copies of area 4.
        assert!((area(&panel) - 16.0).abs() < 1e-6, "area was {}", area(&panel));

        let (px0, py0, px1, py1) = bounds(&panel).unwrap();
        // Bounds span must grow with the grid pitch.
        assert!((px1 - px0) > (sx1 - sx0));
        assert!((py1 - py0) > (sy1 - sy0));
        assert!(px0 <= sx0 + 1e-9 && py0 <= sy0 + 1e-9);
    }

    #[test]
    fn panelize_auto_three_copies() {
        let s = src();
        let panel = panelize_auto(&s, 3, 1, 1.0);
        // 3 copies of area 4, separated by gutter => no overlap.
        assert!((area(&panel) - 12.0).abs() < 1e-6, "area was {}", area(&panel));
        assert_eq!(panel.0.len(), 3, "three separated copies => three polygons");
    }

    #[test]
    fn panelize_empty_source_is_empty() {
        let empty = MultiPolygon::new(vec![]);
        let panel = panelize_auto(&empty, 3, 3, 1.0);
        assert_eq!(panel.0.len(), 0);
    }

    #[test]
    fn mirror_preserves_area_and_flips_x() {
        let s = src(); // bounds x in [0, 2]
        let mirrored = mirror_for_bottom(&s, 0.0);
        assert!((area(&mirrored) - 4.0).abs() < 1e-9);

        let (mx0, _, mx1, _) = bounds(&mirrored).unwrap();
        // Reflected about x=0: [0,2] -> [-2,0].
        assert!((mx0 - (-2.0)).abs() < 1e-9, "min x was {mx0}");
        assert!((mx1 - 0.0).abs() < 1e-9, "max x was {mx1}");
    }
}
