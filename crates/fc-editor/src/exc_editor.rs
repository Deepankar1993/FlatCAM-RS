//! `exc_editor` — GUI-free editable drill/slot collection.
//!
//! This is the headless core of FlatCAM's Excellon editor (`appEditors/`,
//! `ExcEditor`): an editable set of tools, each carrying its diameter and a
//! list of drill points and slot segments. It supports the mutating edit
//! operations the interactive panel needs (add/move/delete drills, add slots,
//! drill arrays), hit-testing for picking, and conversion to `geo` geometry
//! for plotting / export.
//!
//! No egui, no I/O, no globals — everything is plain data + methods, so the
//! whole model is unit-testable.

use fc_geo::{self, Coord, MultiPolygon};
use std::collections::BTreeMap;

/// A single Excellon tool: a diameter plus the drills and slots that use it.
#[derive(Clone, Debug, Default)]
pub struct EditTool {
    /// Tool diameter, in working units.
    pub dia: f64,
    /// Drill centres, as `(x, y)` tuples.
    pub drills: Vec<(f64, f64)>,
    /// Slots, each a `(start, end)` pair of `(x, y)` tuples.
    pub slots: Vec<((f64, f64), (f64, f64))>,
}

/// An editable collection of Excellon tools, keyed by tool number.
///
/// Mirrors `ExcEditor`'s in-memory model: tools are addressed by an integer
/// number, and exactly one tool is "active" at a time (the target of
/// add-drill / add-slot operations).
pub struct ExcEditor {
    /// Tools keyed by tool number, ordered for deterministic iteration.
    pub tools: BTreeMap<i32, EditTool>,
    /// The currently active tool number.
    pub active: i32,
}

impl Default for ExcEditor {
    fn default() -> Self {
        ExcEditor {
            tools: BTreeMap::new(),
            active: 1,
        }
    }
}

impl ExcEditor {
    /// Create an empty editor with active tool number `1`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) a tool with the given number and diameter.
    pub fn add_tool(&mut self, num: i32, dia: f64) {
        self.tools.insert(
            num,
            EditTool {
                dia,
                ..EditTool::default()
            },
        );
    }

    /// Set the active tool number.
    pub fn set_active(&mut self, num: i32) {
        self.active = num;
    }

    /// Ensure the active tool exists, creating an empty one if needed, and
    /// return a mutable reference to it.
    fn active_tool_mut(&mut self) -> &mut EditTool {
        self.tools.entry(self.active).or_default()
    }

    /// Add a drill at `p` to the active tool.
    pub fn add_drill(&mut self, p: (f64, f64)) {
        self.active_tool_mut().drills.push(p);
    }

    /// Add a slot from `a` to `b` to the active tool.
    pub fn add_slot(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.active_tool_mut().slots.push((a, b));
    }

    /// Add an `nx` by `ny` grid of drills to the active tool, starting at
    /// `origin` with column spacing `dx` and row spacing `dy`.
    pub fn add_drill_array(
        &mut self,
        origin: (f64, f64),
        dx: f64,
        dy: f64,
        nx: usize,
        ny: usize,
    ) {
        let tool = self.active_tool_mut();
        for iy in 0..ny {
            for ix in 0..nx {
                let x = origin.0 + dx * ix as f64;
                let y = origin.1 + dy * iy as f64;
                tool.drills.push((x, y));
            }
        }
    }

    /// Find the drill nearest to `p` within `tol`, across all tools.
    ///
    /// Returns `(tool_number, drill_index)` of the closest match, or `None`
    /// if no drill lies within `tol`.
    pub fn hit_test_drill(&self, p: (f64, f64), tol: f64) -> Option<(i32, usize)> {
        let tol2 = tol * tol;
        let mut best: Option<(i32, usize, f64)> = None;
        for (&num, tool) in &self.tools {
            for (idx, &(dx0, dy0)) in tool.drills.iter().enumerate() {
                let dx = dx0 - p.0;
                let dy = dy0 - p.1;
                let d2 = dx * dx + dy * dy;
                if d2 <= tol2 && best.map_or(true, |(_, _, b)| d2 < b) {
                    best = Some((num, idx, d2));
                }
            }
        }
        best.map(|(num, idx, _)| (num, idx))
    }

    /// Move drill `idx` of `tool` to `new`. Returns `false` if not found.
    pub fn move_drill(&mut self, tool: i32, idx: usize, new: (f64, f64)) -> bool {
        if let Some(t) = self.tools.get_mut(&tool) {
            if let Some(slot) = t.drills.get_mut(idx) {
                *slot = new;
                return true;
            }
        }
        false
    }

    /// Delete drill `idx` of `tool`. Returns `false` if not found.
    pub fn delete_drill(&mut self, tool: i32, idx: usize) -> bool {
        if let Some(t) = self.tools.get_mut(&tool) {
            if idx < t.drills.len() {
                t.drills.remove(idx);
                return true;
            }
        }
        false
    }

    /// Build the union of all drills (as circles of radius `dia/2`) and slots
    /// (as round-capped buffered paths of radius `dia/2`) across every tool.
    pub fn to_geometry(&self, steps: usize) -> MultiPolygon<f64> {
        let mut acc: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        for tool in self.tools.values() {
            let r = tool.dia / 2.0;
            let mut polys = Vec::new();
            for &(x, y) in &tool.drills {
                polys.push(fc_geo::circle(x, y, r, steps));
            }
            if !polys.is_empty() {
                let mp = fc_geo::union_all(polys);
                acc = fc_geo::union(&acc, &mp);
            }
            for &(a, b) in &tool.slots {
                let path = [
                    Coord { x: a.0, y: a.1 },
                    Coord { x: b.0, y: b.1 },
                ];
                let mp = fc_geo::buffer_path(&path, r, steps);
                acc = fc_geo::union(&acc, &mp);
            }
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn drill_array_populates_active_tool() {
        let mut ed = ExcEditor::new();
        ed.add_tool(1, 0.8);
        ed.add_drill_array((0.0, 0.0), 2.0, 2.0, 3, 3);
        assert_eq!(ed.tools[&1].drills.len(), 9);
    }

    #[test]
    fn hit_test_finds_nearest() {
        let mut ed = ExcEditor::new();
        ed.add_tool(1, 0.8);
        ed.add_drill_array((0.0, 0.0), 2.0, 2.0, 3, 3);
        // First drill of the grid is at the origin.
        assert_eq!(ed.hit_test_drill((0.05, -0.05), 0.5), Some((1, 0)));
        // Nothing within tolerance far away.
        assert_eq!(ed.hit_test_drill((100.0, 100.0), 0.5), None);
    }

    #[test]
    fn geometry_area_matches_drills() {
        let mut ed = ExcEditor::new();
        ed.add_tool(1, 0.8);
        ed.add_drill_array((0.0, 0.0), 2.0, 2.0, 3, 3);
        let geo = ed.to_geometry(256);
        let area = fc_geo::area(&geo);
        // 9 circles of radius 0.4 -> 9 * pi * 0.4^2 = 9 * pi * 0.16
        let expected = 9.0 * PI * 0.16;
        assert!(
            (area - expected).abs() < 0.01,
            "area {area} not close to expected {expected}"
        );
    }

    #[test]
    fn delete_and_move_drill() {
        let mut ed = ExcEditor::new();
        ed.add_tool(1, 0.8);
        ed.add_drill_array((0.0, 0.0), 2.0, 2.0, 3, 3);
        assert!(ed.delete_drill(1, 0));
        assert_eq!(ed.tools[&1].drills.len(), 8);
        assert!(!ed.delete_drill(1, 99));
        assert!(!ed.delete_drill(7, 0));

        assert!(ed.move_drill(1, 0, (5.0, 6.0)));
        assert_eq!(ed.tools[&1].drills[0], (5.0, 6.0));
        assert!(!ed.move_drill(1, 99, (0.0, 0.0)));
    }

    #[test]
    fn slot_geometry_nonempty() {
        let mut ed = ExcEditor::new();
        ed.add_tool(2, 1.0);
        ed.set_active(2);
        ed.add_slot((0.0, 0.0), (5.0, 0.0));
        let geo = ed.to_geometry(64);
        assert!(fc_geo::area(&geo) > 0.0);
    }

    #[test]
    fn ensure_active_tool_created_on_demand() {
        let mut ed = ExcEditor::new();
        // No tools yet; adding a drill should materialize active tool 1.
        ed.add_drill((1.0, 1.0));
        assert!(ed.tools.contains_key(&1));
        assert_eq!(ed.tools[&1].drills.len(), 1);
    }
}
