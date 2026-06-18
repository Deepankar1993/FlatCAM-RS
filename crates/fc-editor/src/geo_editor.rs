//! `geo_editor` — GUI-free core of FlatCAM's Geometry editor (`appEditors/`).
//!
//! This is a pure, unit-testable model of an editable geometry collection: a
//! list of primitive [`Shape`]s (points, polylines, polygon rings), the edit
//! operations the interactive editor performs (add / select / move vertices /
//! translate / delete), simple hit-testing, and conversion to `geo`/`fc-geo`
//! geometry for downstream CAM processing. No egui, no I/O, no globals.
//!
//! Coordinates in the public API are plain `(f64, f64)` tuples. Polygon shapes
//! store a *ring* — the first point need **not** be repeated at the end; closing
//! happens during conversion to a `geo::Polygon`.

use fc_geo::{union_all, Coord, LineString, MultiPolygon, Polygon};

/// A single editable primitive in the geometry collection.
///
/// * `Point` — a lone vertex (a drill / reference mark).
/// * `Line`  — an open polyline (a path / trace centreline).
/// * `Polygon` — a closed region stored as a ring; the first vertex is *not*
///   repeated at the end (closing is implicit).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Point((f64, f64)),
    Line(Vec<(f64, f64)>),
    Polygon(Vec<(f64, f64)>),
}

impl Shape {
    /// All vertices of this shape, in order, as `(x, y)` tuples.
    fn vertices(&self) -> &[(f64, f64)] {
        match self {
            Shape::Point(p) => std::slice::from_ref(p),
            Shape::Line(v) | Shape::Polygon(v) => v.as_slice(),
        }
    }

    /// Mutable access to the `vert_idx`-th vertex of this shape, if it exists.
    fn vertex_mut(&mut self, vert_idx: usize) -> Option<&mut (f64, f64)> {
        match self {
            Shape::Point(p) => {
                if vert_idx == 0 {
                    Some(p)
                } else {
                    None
                }
            }
            Shape::Line(v) | Shape::Polygon(v) => v.get_mut(vert_idx),
        }
    }
}

/// An editable collection of [`Shape`]s plus an optional current selection.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct GeoEditor {
    pub shapes: Vec<Shape>,
    pub selected: Option<usize>,
}

#[inline]
fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

impl GeoEditor {
    /// Create an empty editor.
    pub fn new() -> Self {
        GeoEditor::default()
    }

    /// Append a [`Shape::Point`] and return its index.
    pub fn add_point(&mut self, p: (f64, f64)) -> usize {
        self.shapes.push(Shape::Point(p));
        self.shapes.len() - 1
    }

    /// Append a [`Shape::Line`] polyline and return its index.
    pub fn add_line(&mut self, pts: Vec<(f64, f64)>) -> usize {
        self.shapes.push(Shape::Line(pts));
        self.shapes.len() - 1
    }

    /// Append an axis-aligned rectangle as a [`Shape::Polygon`] ring whose
    /// lower-left corner is `(x, y)`. Returns its index.
    pub fn add_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> usize {
        let ring = vec![
            (x, y),
            (x + w, y),
            (x + w, y + h),
            (x, y + h),
        ];
        self.shapes.push(Shape::Polygon(ring));
        self.shapes.len() - 1
    }

    /// Append a circle sampled into a [`Shape::Polygon`] ring of `steps`
    /// vertices (minimum 8). Returns its index.
    pub fn add_circle(&mut self, cx: f64, cy: f64, r: f64, steps: usize) -> usize {
        let steps = steps.max(8);
        let mut ring = Vec::with_capacity(steps);
        for i in 0..steps {
            let a = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
            ring.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        self.shapes.push(Shape::Polygon(ring));
        self.shapes.len() - 1
    }

    /// Hit-test: find the nearest shape that has *any* vertex within `tol` of
    /// `p`. On a hit, set [`Self::selected`] and return the index; otherwise
    /// leave the selection untouched and return `None`.
    pub fn select_at(&mut self, p: (f64, f64), tol: f64) -> Option<usize> {
        let tol2 = tol * tol;
        let mut best: Option<(usize, f64)> = None;
        for (idx, shape) in self.shapes.iter().enumerate() {
            for v in shape.vertices() {
                let d2 = dist2(*v, p);
                if d2 <= tol2 {
                    match best {
                        Some((_, bd)) if bd <= d2 => {}
                        _ => best = Some((idx, d2)),
                    }
                }
            }
        }
        if let Some((idx, _)) = best {
            self.selected = Some(idx);
            Some(idx)
        } else {
            None
        }
    }

    /// Index of the vertex of shape `shape_idx` nearest to `p`, if the shape
    /// exists and has at least one vertex.
    pub fn nearest_vertex(&self, shape_idx: usize, p: (f64, f64)) -> Option<usize> {
        let shape = self.shapes.get(shape_idx)?;
        let verts = shape.vertices();
        if verts.is_empty() {
            return None;
        }
        let mut best_idx = 0;
        let mut best_d2 = dist2(verts[0], p);
        for (i, v) in verts.iter().enumerate().skip(1) {
            let d2 = dist2(*v, p);
            if d2 < best_d2 {
                best_d2 = d2;
                best_idx = i;
            }
        }
        Some(best_idx)
    }

    /// Move a single vertex to `new`. Returns `false` if either index is out of
    /// range.
    pub fn move_vertex(&mut self, shape_idx: usize, vert_idx: usize, new: (f64, f64)) -> bool {
        match self.shapes.get_mut(shape_idx) {
            Some(shape) => match shape.vertex_mut(vert_idx) {
                Some(v) => {
                    *v = new;
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    /// Translate every vertex of the currently selected shape by `(dx, dy)`.
    /// Returns `false` if there is no valid selection.
    pub fn translate_selected(&mut self, dx: f64, dy: f64) -> bool {
        let idx = match self.selected {
            Some(i) => i,
            None => return false,
        };
        let shape = match self.shapes.get_mut(idx) {
            Some(s) => s,
            None => return false,
        };
        match shape {
            Shape::Point(p) => {
                p.0 += dx;
                p.1 += dy;
            }
            Shape::Line(v) | Shape::Polygon(v) => {
                for p in v.iter_mut() {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
        }
        true
    }

    /// Remove the shape at `idx`. Returns `false` if out of range. Keeps
    /// [`Self::selected`] consistent (cleared if it pointed at `idx`, shifted
    /// down if it pointed past `idx`).
    pub fn delete(&mut self, idx: usize) -> bool {
        if idx >= self.shapes.len() {
            return false;
        }
        self.shapes.remove(idx);
        self.selected = match self.selected {
            Some(s) if s == idx => None,
            Some(s) if s > idx => Some(s - 1),
            other => other,
        };
        true
    }

    /// Convert every [`Shape::Polygon`] to a closed `geo::Polygon` and union
    /// them into a single [`MultiPolygon`]. Points and lines are ignored.
    pub fn to_multipolygon(&self) -> MultiPolygon<f64> {
        let mut polys: Vec<Polygon<f64>> = Vec::new();
        for shape in &self.shapes {
            if let Shape::Polygon(ring) = shape {
                if ring.len() < 3 {
                    continue;
                }
                let mut coords: Vec<Coord<f64>> =
                    ring.iter().map(|&(x, y)| Coord { x, y }).collect();
                // Close the ring explicitly for geo.
                coords.push(coords[0]);
                polys.push(Polygon::new(LineString::new(coords), vec![]));
            }
        }
        if polys.is_empty() {
            return MultiPolygon::new(vec![]);
        }
        union_all(polys)
    }

    /// Collect every [`Shape::Line`] as a `geo::LineString`. Points and polygons
    /// are ignored.
    pub fn to_polylines(&self) -> Vec<LineString<f64>> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Line(v) => Some(LineString::new(
                    v.iter().map(|&(x, y)| Coord { x, y }).collect(),
                )),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::area;

    #[test]
    fn add_rect_to_multipolygon_area_is_w_times_h() {
        let mut ed = GeoEditor::new();
        ed.add_rect(1.0, 2.0, 3.0, 4.0);
        let mp = ed.to_multipolygon();
        assert!((area(&mp) - 12.0).abs() < 1e-9, "area was {}", area(&mp));
    }

    #[test]
    fn select_at_near_corner_sets_selected() {
        let mut ed = GeoEditor::new();
        ed.add_rect(0.0, 0.0, 2.0, 2.0); // index 0, corner at (2,2)
        let hit = ed.select_at((2.05, 1.98), 0.1);
        assert_eq!(hit, Some(0));
        assert_eq!(ed.selected, Some(0));
    }

    #[test]
    fn select_at_miss_returns_none_and_keeps_selection() {
        let mut ed = GeoEditor::new();
        ed.add_rect(0.0, 0.0, 2.0, 2.0);
        ed.selected = Some(0);
        let hit = ed.select_at((100.0, 100.0), 0.1);
        assert_eq!(hit, None);
        assert_eq!(ed.selected, Some(0), "selection must be untouched on a miss");
    }

    #[test]
    fn select_at_picks_nearest_of_several() {
        let mut ed = GeoEditor::new();
        ed.add_rect(0.0, 0.0, 1.0, 1.0); // corners near origin
        ed.add_rect(10.0, 10.0, 1.0, 1.0); // far away
        let hit = ed.select_at((10.0, 10.0), 0.5);
        assert_eq!(hit, Some(1));
    }

    #[test]
    fn nearest_vertex_finds_closest() {
        let mut ed = GeoEditor::new();
        let idx = ed.add_rect(0.0, 0.0, 2.0, 2.0);
        // Ring order: (0,0),(2,0),(2,2),(0,2). Closest to (2.1,2.1) is index 2.
        assert_eq!(ed.nearest_vertex(idx, (2.1, 2.1)), Some(2));
        assert_eq!(ed.nearest_vertex(idx, (-0.1, -0.1)), Some(0));
    }

    #[test]
    fn nearest_vertex_out_of_range_is_none() {
        let ed = GeoEditor::new();
        assert_eq!(ed.nearest_vertex(5, (0.0, 0.0)), None);
    }

    #[test]
    fn move_vertex_changes_area() {
        let mut ed = GeoEditor::new();
        let idx = ed.add_rect(0.0, 0.0, 2.0, 2.0);
        let before = area(&ed.to_multipolygon());
        // Push the (2,2) corner outward.
        assert!(ed.move_vertex(idx, 2, (4.0, 4.0)));
        let after = area(&ed.to_multipolygon());
        assert!(after > before, "before {before}, after {after}");
    }

    #[test]
    fn move_vertex_out_of_range_returns_false() {
        let mut ed = GeoEditor::new();
        let idx = ed.add_rect(0.0, 0.0, 2.0, 2.0);
        assert!(!ed.move_vertex(idx, 99, (0.0, 0.0)));
        assert!(!ed.move_vertex(99, 0, (0.0, 0.0)));
    }

    #[test]
    fn move_point_vertex() {
        let mut ed = GeoEditor::new();
        let idx = ed.add_point((1.0, 1.0));
        assert!(ed.move_vertex(idx, 0, (5.0, 6.0)));
        assert_eq!(ed.shapes[idx], Shape::Point((5.0, 6.0)));
        assert!(!ed.move_vertex(idx, 1, (0.0, 0.0)));
    }

    #[test]
    fn translate_selected_moves_all_vertices() {
        let mut ed = GeoEditor::new();
        ed.add_rect(0.0, 0.0, 2.0, 2.0);
        ed.selected = Some(0);
        assert!(ed.translate_selected(10.0, 5.0));
        let mp = ed.to_multipolygon();
        let (minx, miny, _, _) = fc_geo::bounds(&mp).unwrap();
        assert!((minx - 10.0).abs() < 1e-9 && (miny - 5.0).abs() < 1e-9);
    }

    #[test]
    fn translate_selected_without_selection_returns_false() {
        let mut ed = GeoEditor::new();
        ed.add_rect(0.0, 0.0, 2.0, 2.0);
        assert!(!ed.translate_selected(1.0, 1.0));
    }

    #[test]
    fn delete_removes_and_fixes_selection() {
        let mut ed = GeoEditor::new();
        ed.add_point((0.0, 0.0)); // 0
        ed.add_point((1.0, 0.0)); // 1
        ed.add_point((2.0, 0.0)); // 2
        ed.selected = Some(2);
        assert!(ed.delete(0));
        assert_eq!(ed.shapes.len(), 2);
        // Selection 2 -> 1 after removing earlier element.
        assert_eq!(ed.selected, Some(1));
        // Deleting the selected one clears selection.
        assert!(ed.delete(1));
        assert_eq!(ed.selected, None);
        // Out of range.
        assert!(!ed.delete(99));
    }

    #[test]
    fn to_polylines_counts_only_lines() {
        let mut ed = GeoEditor::new();
        ed.add_point((0.0, 0.0));
        ed.add_line(vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]);
        ed.add_line(vec![(5.0, 5.0), (6.0, 6.0)]);
        ed.add_rect(0.0, 0.0, 1.0, 1.0);
        let lines = ed.to_polylines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0.len(), 3);
        assert_eq!(lines[1].0.len(), 2);
    }

    #[test]
    fn empty_to_multipolygon_is_empty() {
        let ed = GeoEditor::new();
        assert!(ed.to_multipolygon().0.is_empty());
    }

    #[test]
    fn add_circle_area_approximates_pi_r_squared() {
        let mut ed = GeoEditor::new();
        ed.add_circle(0.0, 0.0, 1.0, 256);
        let a = area(&ed.to_multipolygon());
        assert!((a - std::f64::consts::PI).abs() < 1e-2, "circle area {a}");
    }
}
