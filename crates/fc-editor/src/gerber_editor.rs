//! `gerber_editor` — GUI-free editable copper-primitive collection.
//!
//! Rust port of the core of FlatCAM's `GerberEditor` (`appEditors/`): an
//! editable list of copper primitives (pads, tracks, regions), nearest-hit
//! selection, in-place translation, deletion, and conversion to a unified
//! `geo` geometry suitable for plotting or further CAM processing.
//!
//! This is a pure data model — no egui, no I/O, no globals. The interactive
//! egui panel in `fc-gui` drives this core.

use fc_geo::{Coord, LineString, MultiPolygon, Polygon};

/// A single editable copper primitive.
#[derive(Clone, Debug)]
pub enum GbrPrim {
    /// A round flash: a filled circle of the given diameter centred at `center`.
    Pad { center: (f64, f64), dia: f64 },
    /// A trace: a round-capped buffered polyline of the given width.
    Track { path: Vec<(f64, f64)>, width: f64 },
    /// A filled region defined by a closed ring of vertices.
    Region { ring: Vec<(f64, f64)> },
}

/// An editable collection of Gerber copper primitives plus a selection cursor.
#[derive(Default, Debug)]
pub struct GerberEditor {
    pub prims: Vec<GbrPrim>,
    pub selected: Option<usize>,
}

impl GerberEditor {
    /// Create an empty editor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a round pad and return its index.
    pub fn add_pad(&mut self, center: (f64, f64), dia: f64) -> usize {
        self.prims.push(GbrPrim::Pad { center, dia });
        self.prims.len() - 1
    }

    /// Add a track (buffered polyline) and return its index.
    pub fn add_track(&mut self, path: Vec<(f64, f64)>, width: f64) -> usize {
        self.prims.push(GbrPrim::Track { path, width });
        self.prims.len() - 1
    }

    /// Add a filled region and return its index.
    pub fn add_region(&mut self, ring: Vec<(f64, f64)>) -> usize {
        self.prims.push(GbrPrim::Region { ring });
        self.prims.len() - 1
    }

    /// Select the primitive nearest to point `p`, within `tol`.
    ///
    /// Distance metric: for a `Pad` it is the distance from `p` to its centre;
    /// for `Track`/`Region` it is the minimum distance from `p` to any vertex.
    /// Returns and stores the index of the closest primitive whose distance is
    /// `<= tol`, or clears the selection and returns `None`.
    pub fn select_at(&mut self, p: (f64, f64), tol: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, prim) in self.prims.iter().enumerate() {
            let d = prim.distance_to(p);
            match best {
                Some((_, bd)) if bd <= d => {}
                _ => best = Some((i, d)),
            }
        }
        match best {
            Some((i, d)) if d <= tol => {
                self.selected = Some(i);
                Some(i)
            }
            _ => {
                self.selected = None;
                None
            }
        }
    }

    /// Translate the currently selected primitive by `(dx, dy)`.
    ///
    /// Returns `true` if a primitive was selected and moved, `false` otherwise.
    pub fn translate_selected(&mut self, dx: f64, dy: f64) -> bool {
        match self.selected {
            Some(i) if i < self.prims.len() => {
                self.prims[i].translate(dx, dy);
                true
            }
            _ => false,
        }
    }

    /// Delete the primitive at `idx`.
    ///
    /// Returns `true` if removed. Adjusts the selection cursor: a deleted
    /// selection clears it; a later selection shifts down by one.
    pub fn delete(&mut self, idx: usize) -> bool {
        if idx >= self.prims.len() {
            return false;
        }
        self.prims.remove(idx);
        self.selected = match self.selected {
            Some(s) if s == idx => None,
            Some(s) if s > idx => Some(s - 1),
            other => other,
        };
        true
    }

    /// Convert the whole collection into one unified `MultiPolygon`.
    ///
    /// * `Pad`    -> filled circle of radius `dia/2`.
    /// * `Track`  -> round-capped buffer of the polyline at radius `width/2`.
    /// * `Region` -> filled closed polygon of the ring.
    ///
    /// All resulting shapes are unioned together so overlapping copper merges.
    pub fn to_geometry(&self, steps: usize) -> MultiPolygon<f64> {
        let mut acc = MultiPolygon::new(vec![]);
        for prim in &self.prims {
            let part = prim.to_geometry(steps);
            acc = fc_geo::union(&acc, &part);
        }
        acc
    }
}

impl GbrPrim {
    /// Distance from point `p` to this primitive, per `select_at`'s metric.
    fn distance_to(&self, p: (f64, f64)) -> f64 {
        match self {
            GbrPrim::Pad { center, .. } => dist(*center, p),
            GbrPrim::Track { path, .. } => min_vertex_dist(path, p),
            GbrPrim::Region { ring } => min_vertex_dist(ring, p),
        }
    }

    /// Shift all of this primitive's coordinates by `(dx, dy)`.
    fn translate(&mut self, dx: f64, dy: f64) {
        match self {
            GbrPrim::Pad { center, .. } => {
                center.0 += dx;
                center.1 += dy;
            }
            GbrPrim::Track { path, .. } => {
                for v in path.iter_mut() {
                    v.0 += dx;
                    v.1 += dy;
                }
            }
            GbrPrim::Region { ring } => {
                for v in ring.iter_mut() {
                    v.0 += dx;
                    v.1 += dy;
                }
            }
        }
    }

    /// Render this single primitive to geometry.
    fn to_geometry(&self, steps: usize) -> MultiPolygon<f64> {
        match self {
            GbrPrim::Pad { center, dia } => {
                MultiPolygon::new(vec![fc_geo::circle(center.0, center.1, dia / 2.0, steps)])
            }
            GbrPrim::Track { path, width } => {
                let coords: Vec<Coord<f64>> =
                    path.iter().map(|&(x, y)| Coord { x, y }).collect();
                fc_geo::buffer_path(&coords, width / 2.0, steps)
            }
            GbrPrim::Region { ring } => {
                if ring.len() < 3 {
                    return MultiPolygon::new(vec![]);
                }
                let mut coords: Vec<Coord<f64>> =
                    ring.iter().map(|&(x, y)| Coord { x, y }).collect();
                // Ensure the ring is closed.
                if coords.first() != coords.last() {
                    coords.push(coords[0]);
                }
                MultiPolygon::new(vec![Polygon::new(LineString::new(coords), vec![])])
            }
        }
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    (dx * dx + dy * dy).sqrt()
}

fn min_vertex_dist(verts: &[(f64, f64)], p: (f64, f64)) -> f64 {
    verts
        .iter()
        .map(|&v| dist(v, p))
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn bounds(mp: &MultiPolygon<f64>) -> Option<(f64, f64, f64, f64)> {
        use geo::BoundingRect;
        mp.bounding_rect()
            .map(|r| (r.min().x, r.min().y, r.max().x, r.max().y))
    }

    #[test]
    fn pad_area_is_approximately_pi() {
        let mut ed = GerberEditor::new();
        ed.add_pad((0.0, 0.0), 2.0); // radius 1 -> area pi
        let a = fc_geo::area(&ed.to_geometry(256));
        assert!((a - PI).abs() < 1e-2, "pad area was {a}");
    }

    #[test]
    fn adding_track_increases_area() {
        let mut ed = GerberEditor::new();
        ed.add_pad((0.0, 0.0), 2.0);
        let a0 = fc_geo::area(&ed.to_geometry(64));
        ed.add_track(vec![(10.0, 0.0), (20.0, 0.0)], 1.0);
        let a1 = fc_geo::area(&ed.to_geometry(64));
        assert!(a1 > a0, "area did not increase: {a0} -> {a1}");
    }

    #[test]
    fn select_at_finds_pad_near_center() {
        let mut ed = GerberEditor::new();
        ed.add_pad((5.0, 5.0), 2.0);
        ed.add_pad((50.0, 50.0), 2.0);
        let sel = ed.select_at((5.1, 4.9), 1.0);
        assert_eq!(sel, Some(0));
        assert_eq!(ed.selected, Some(0));
    }

    #[test]
    fn select_at_misses_when_out_of_tolerance() {
        let mut ed = GerberEditor::new();
        ed.add_pad((5.0, 5.0), 2.0);
        let sel = ed.select_at((100.0, 100.0), 1.0);
        assert_eq!(sel, None);
        assert_eq!(ed.selected, None);
    }

    #[test]
    fn select_at_uses_vertex_for_track() {
        let mut ed = GerberEditor::new();
        ed.add_track(vec![(0.0, 0.0), (10.0, 0.0)], 1.0);
        let sel = ed.select_at((10.2, 0.1), 0.5);
        assert_eq!(sel, Some(0));
    }

    #[test]
    fn translate_selected_shifts_bounds() {
        let mut ed = GerberEditor::new();
        ed.add_pad((0.0, 0.0), 2.0);
        ed.select_at((0.0, 0.0), 1.0);
        let (x0, y0, _, _) = bounds(&ed.to_geometry(64)).unwrap();
        assert!(ed.translate_selected(10.0, 5.0));
        let (x1, y1, _, _) = bounds(&ed.to_geometry(64)).unwrap();
        assert!((x1 - x0 - 10.0).abs() < 1e-6, "x shift wrong: {x0} -> {x1}");
        assert!((y1 - y0 - 5.0).abs() < 1e-6, "y shift wrong: {y0} -> {y1}");
    }

    #[test]
    fn translate_selected_without_selection_is_noop() {
        let mut ed = GerberEditor::new();
        ed.add_pad((0.0, 0.0), 2.0);
        assert!(!ed.translate_selected(1.0, 1.0));
    }

    #[test]
    fn delete_removes_and_adjusts_selection() {
        let mut ed = GerberEditor::new();
        ed.add_pad((0.0, 0.0), 2.0); // 0
        ed.add_pad((10.0, 0.0), 2.0); // 1
        ed.add_pad((20.0, 0.0), 2.0); // 2
        ed.selected = Some(2);
        assert!(ed.delete(0));
        assert_eq!(ed.prims.len(), 2);
        // selection 2 shifts down to 1
        assert_eq!(ed.selected, Some(1));
        // deleting the selected one clears it
        ed.selected = Some(1);
        assert!(ed.delete(1));
        assert_eq!(ed.selected, None);
        // out-of-range delete fails
        assert!(!ed.delete(99));
    }

    #[test]
    fn region_contributes_area() {
        let mut ed = GerberEditor::new();
        ed.add_region(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        let a = fc_geo::area(&ed.to_geometry(64));
        assert!((a - 16.0).abs() < 1e-6, "region area was {a}");
    }

    #[test]
    fn empty_editor_yields_empty_geometry() {
        let ed = GerberEditor::new();
        assert_eq!(ed.to_geometry(64).0.len(), 0);
    }
}
