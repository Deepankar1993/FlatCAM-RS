//! Object dependency relations — the derivation graph between project objects.
//!
//! Objects in a FlatCAM project form a tree by derivation: a CNCJob is derived
//! from a Gerber (isolation), a Geometry may be derived from a CNCJob, and so
//! on. The link is recorded in [`crate::ProjectObject::parent`]. This module
//! adds graph queries (children/descendants) and a cascading delete so that
//! removing a source object also removes everything generated from it — the
//! analogue of FlatCAM's "delete object and its dependents" behaviour.

use std::collections::HashSet;

impl crate::Project {
    /// Direct children of `name`: objects whose `parent` is `Some(name)`.
    ///
    /// Order follows the project's object order (insertion order).
    pub fn children_of(&self, name: &str) -> Vec<&crate::ProjectObject> {
        self.objects
            .iter()
            .filter(|o| o.parent.as_deref() == Some(name))
            .collect()
    }

    /// Names of all transitive descendants of `name` (children, grandchildren,
    /// …), discovered breadth-first with no duplicates. The starting object
    /// itself is not included.
    ///
    /// Self-referential or cyclic `parent` links cannot cause an infinite loop:
    /// a `visited` set guards against revisiting any name.
    pub fn descendants(&self, name: &str) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        // Seed the queue with the root so its own name is never re-added even
        // through a cycle, but it is not pushed into `result`.
        visited.insert(name.to_string());
        let mut queue: Vec<String> = vec![name.to_string()];

        while let Some(current) = queue.first().cloned() {
            queue.remove(0);
            for child in self.children_of(&current) {
                if visited.insert(child.name.clone()) {
                    result.push(child.name.clone());
                    queue.push(child.name.clone());
                }
            }
        }
        result
    }

    /// Remove `name` together with all of its (transitive) descendants.
    ///
    /// Returns the number of objects actually removed. If `name` does not exist
    /// but has descendants somehow attributed to it, only the existing matches
    /// are removed. Clears [`crate::Project::selected`] if the selected object
    /// was among those removed.
    pub fn remove_cascade(&mut self, name: &str) -> usize {
        let mut to_remove: HashSet<String> = self.descendants(name).into_iter().collect();
        to_remove.insert(name.to_string());

        let before = self.objects.len();
        self.objects.retain(|o| !to_remove.contains(&o.name));
        let removed = before - self.objects.len();

        if let Some(sel) = self.selected.as_deref() {
            if to_remove.contains(sel) {
                self.selected = None;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use crate::{ObjectKind, Project, ProjectObject};

    fn graph() -> Project {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("g", ObjectKind::Gerber)).unwrap();

        let mut j = ProjectObject::new("j", ObjectKind::CncJob);
        j.parent = Some("g".to_string());
        p.add(j).unwrap();

        let mut k = ProjectObject::new("k", ObjectKind::Geometry);
        k.parent = Some("j".to_string());
        p.add(k).unwrap();

        p
    }

    #[test]
    fn children_of_returns_direct_child() {
        let p = graph();
        let kids = p.children_of("g");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].name, "j");
    }

    #[test]
    fn descendants_is_transitive() {
        let p = graph();
        let desc = p.descendants("g");
        assert_eq!(desc.len(), 2);
        assert!(desc.contains(&"j".to_string()));
        assert!(desc.contains(&"k".to_string()));
        // BFS order: direct child first, then grandchild.
        assert_eq!(desc, vec!["j".to_string(), "k".to_string()]);
    }

    #[test]
    fn descendants_leaf_is_empty() {
        let p = graph();
        assert!(p.descendants("k").is_empty());
    }

    #[test]
    fn remove_cascade_removes_subtree() {
        let mut p = graph();
        let removed = p.remove_cascade("g");
        assert_eq!(removed, 3);
        assert!(p.objects.is_empty());
        assert!(p.get("g").is_none());
        assert!(p.get("j").is_none());
        assert!(p.get("k").is_none());
    }

    #[test]
    fn remove_cascade_clears_selection() {
        let mut p = graph();
        p.selected = Some("k".to_string());
        let removed = p.remove_cascade("g");
        assert_eq!(removed, 3);
        assert!(p.selected.is_none());
    }

    #[test]
    fn remove_cascade_keeps_unrelated_selection() {
        let mut p = graph();
        p.add(ProjectObject::new("other", ObjectKind::Document)).unwrap();
        p.selected = Some("other".to_string());
        let removed = p.remove_cascade("g");
        assert_eq!(removed, 3);
        assert_eq!(p.selected, Some("other".to_string()));
        assert!(p.get("other").is_some());
    }

    #[test]
    fn remove_cascade_partial_subtree() {
        let mut p = graph();
        // Removing the middle node takes its child but leaves the root.
        let removed = p.remove_cascade("j");
        assert_eq!(removed, 2);
        assert!(p.get("g").is_some());
        assert!(p.get("j").is_none());
        assert!(p.get("k").is_none());
    }

    #[test]
    fn cyclic_links_terminate() {
        let mut p = Project::new("mm");
        let mut a = ProjectObject::new("a", ObjectKind::Geometry);
        a.parent = Some("b".to_string());
        let mut b = ProjectObject::new("b", ObjectKind::Geometry);
        b.parent = Some("a".to_string());
        p.add(a).unwrap();
        p.add(b).unwrap();

        // a -> b -> a; descendants of "a" is just "b" (no infinite loop, no dups).
        let desc = p.descendants("a");
        assert_eq!(desc, vec!["b".to_string()]);
    }
}
