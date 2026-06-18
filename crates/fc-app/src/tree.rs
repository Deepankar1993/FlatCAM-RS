//! Project-tree view model — a flat-with-grouping render of the project's
//! objects, mirroring FlatCAM's `ObjectCollection` Qt tree.
//!
//! Rows are produced in a stable kind order (Gerber, Excellon, Geometry,
//! CncJob, Svg, Document); within each kind the original insertion order is
//! preserved. A child object (one whose `parent` names another present object)
//! is rendered at `depth == 1`, immediately after its parent, regardless of the
//! child's own kind grouping. Everything else sits at `depth == 0`.

use crate::{ObjectKind, Project, ProjectObject};

/// One displayable row in the project tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub name: String,
    pub kind: ObjectKind,
    pub visible: bool,
    pub selected: bool,
    pub depth: usize,
}

/// The fixed display order of kinds in the tree.
const KIND_ORDER: [ObjectKind; 6] = [
    ObjectKind::Gerber,
    ObjectKind::Excellon,
    ObjectKind::Geometry,
    ObjectKind::CncJob,
    ObjectKind::Svg,
    ObjectKind::Document,
];

impl Project {
    /// Build the display rows for the project tree.
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let selected = self.selected.as_deref();

        let make_row = |obj: &ProjectObject, depth: usize| TreeRow {
            name: obj.name.clone(),
            kind: obj.kind,
            visible: obj.visible,
            selected: selected == Some(obj.name.as_str()),
            depth,
        };

        // A child is an object whose `parent` names another object that exists
        // in this project. Such children are emitted right after their parent
        // rather than in their own kind group.
        let is_child = |obj: &ProjectObject| -> bool {
            match &obj.parent {
                Some(p) => self.objects.iter().any(|o| &o.name == p),
                None => false,
            }
        };

        let mut rows = Vec::with_capacity(self.objects.len());

        for kind in KIND_ORDER {
            for obj in self.objects.iter().filter(|o| o.kind == kind) {
                // Children are listed under their parent, not in their kind group.
                if is_child(obj) {
                    continue;
                }
                rows.push(make_row(obj, 0));
                // Emit this object's children (in insertion order) at depth 1.
                for child in self
                    .objects
                    .iter()
                    .filter(|c| c.parent.as_deref() == Some(obj.name.as_str()))
                {
                    rows.push(make_row(child, 1));
                }
            }
        }

        rows
    }

    /// Select the object named `name`. Returns `true` if it exists (and was
    /// selected), `false` otherwise (selection left unchanged).
    pub fn select(&mut self, name: &str) -> bool {
        if self.objects.iter().any(|o| o.name == name) {
            self.selected = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// The currently selected object, if any.
    pub fn selected_object(&self) -> Option<&ProjectObject> {
        let name = self.selected.as_deref()?;
        self.objects.iter().find(|o| o.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectKind, ProjectObject};

    fn sample() -> Project {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("a.gbr", ObjectKind::Gerber)).unwrap();
        p.add(ProjectObject::new("b.gbr", ObjectKind::Gerber)).unwrap();
        p.add(ProjectObject::new("d.drl", ObjectKind::Excellon)).unwrap();
        p
    }

    #[test]
    fn three_rows_in_kind_order() {
        let p = sample();
        let rows = p.tree_rows();
        assert_eq!(rows.len(), 3);
        // Gerbers first (insertion order), then the Excellon.
        assert_eq!(rows[0].name, "a.gbr");
        assert_eq!(rows[0].kind, ObjectKind::Gerber);
        assert_eq!(rows[1].name, "b.gbr");
        assert_eq!(rows[1].kind, ObjectKind::Gerber);
        assert_eq!(rows[2].name, "d.drl");
        assert_eq!(rows[2].kind, ObjectKind::Excellon);
        assert!(rows.iter().all(|r| r.depth == 0));
        assert!(rows.iter().all(|r| !r.selected));
    }

    #[test]
    fn kind_order_independent_of_insertion() {
        // Insert in a scrambled order; rows must still follow KIND_ORDER.
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("doc", ObjectKind::Document)).unwrap();
        p.add(ProjectObject::new("geo", ObjectKind::Geometry)).unwrap();
        p.add(ProjectObject::new("g.gbr", ObjectKind::Gerber)).unwrap();
        let rows = p.tree_rows();
        let kinds: Vec<ObjectKind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![ObjectKind::Gerber, ObjectKind::Geometry, ObjectKind::Document]
        );
    }

    #[test]
    fn select_sets_selected_and_marks_row() {
        let mut p = sample();
        assert!(p.select("b.gbr"));
        assert_eq!(p.selected.as_deref(), Some("b.gbr"));

        let rows = p.tree_rows();
        let marked: Vec<&TreeRow> = rows.iter().filter(|r| r.selected).collect();
        assert_eq!(marked.len(), 1);
        assert_eq!(marked[0].name, "b.gbr");

        assert_eq!(p.selected_object().unwrap().name, "b.gbr");
    }

    #[test]
    fn select_missing_returns_false() {
        let mut p = sample();
        assert!(!p.select("nope"));
        assert!(p.selected.is_none());
        assert!(p.selected_object().is_none());
    }

    #[test]
    fn child_appears_at_depth_one_after_parent() {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("board.gbr", ObjectKind::Gerber)).unwrap();
        // A CNCJob derived from the gerber: would normally sort into the CncJob
        // group, but as a child it sits right under its parent at depth 1.
        let mut job = ProjectObject::new("board_cnc", ObjectKind::CncJob);
        job.parent = Some("board.gbr".to_string());
        p.add(job).unwrap();
        p.add(ProjectObject::new("other.drl", ObjectKind::Excellon)).unwrap();

        let rows = p.tree_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "board.gbr");
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].name, "board_cnc");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[1].kind, ObjectKind::CncJob);
        // The unrelated Excellon still appears (at depth 0).
        assert_eq!(rows[2].name, "other.drl");
        assert_eq!(rows[2].depth, 0);
    }

    #[test]
    fn dangling_parent_is_treated_as_root() {
        // A parent that does not exist => the object is a normal root row.
        let mut p = Project::new("mm");
        let mut orphan = ProjectObject::new("orphan", ObjectKind::Geometry);
        orphan.parent = Some("missing".to_string());
        p.add(orphan).unwrap();
        let rows = p.tree_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "orphan");
        assert_eq!(rows[0].depth, 0);
    }
}
