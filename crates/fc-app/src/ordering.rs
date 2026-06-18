//! Visibility, reordering, and duplication operations on a [`Project`].
//!
//! These mirror FlatCAM's `ObjectCollection` tree operations: toggling an
//! object's canvas visibility, moving an object up/down in the project tree,
//! and duplicating an object under a fresh, unique name. The methods are added
//! to [`crate::Project`] via an inherent `impl` block in this module.

impl crate::Project {
    /// Set the visibility flag of the named object.
    ///
    /// Returns `true` if the object existed (and was updated), `false` if no
    /// object with that name is present.
    pub fn set_visible(&mut self, name: &str, vis: bool) -> bool {
        match self.get_mut(name) {
            Some(obj) => {
                obj.visible = vis;
                true
            }
            None => false,
        }
    }

    /// Toggle the visibility flag of the named object.
    ///
    /// Returns the *resulting* visibility state, or `false` if no object with
    /// that name exists. (A `false` return is therefore ambiguous only in the
    /// sense that a now-hidden existing object and a missing object both yield
    /// `false`; callers needing to distinguish should check existence first.)
    pub fn toggle_visible(&mut self, name: &str) -> bool {
        match self.get_mut(name) {
            Some(obj) => {
                obj.visible = !obj.visible;
                obj.visible
            }
            None => false,
        }
    }

    /// Move the named object one position earlier in the project order by
    /// swapping it with its preceding neighbor.
    ///
    /// Returns `true` if a swap occurred, `false` if the object is missing or
    /// already first.
    pub fn move_up(&mut self, name: &str) -> bool {
        match self.objects.iter().position(|o| o.name == name) {
            Some(idx) if idx > 0 => {
                self.objects.swap(idx, idx - 1);
                true
            }
            _ => false,
        }
    }

    /// Move the named object one position later in the project order by
    /// swapping it with its following neighbor.
    ///
    /// Returns `true` if a swap occurred, `false` if the object is missing or
    /// already last.
    pub fn move_down(&mut self, name: &str) -> bool {
        match self.objects.iter().position(|o| o.name == name) {
            Some(idx) if idx + 1 < self.objects.len() => {
                self.objects.swap(idx, idx + 1);
                true
            }
            _ => false,
        }
    }

    /// Duplicate the named object, inserting a clone under a unique name.
    ///
    /// The clone's name is derived as `"<name>_copy"`, and if that is already
    /// taken, a numeric suffix is appended (`"<name>_copy_1"`, `"_2"`, ...)
    /// until a free name is found. The clone is appended to the end of the
    /// project. Returns the new name, or `None` if the source object is absent.
    pub fn duplicate(&mut self, name: &str) -> Option<String> {
        let mut clone = self.get(name)?.clone();

        let base = format!("{name}_copy");
        let new_name = if self.objects.iter().all(|o| o.name != base) {
            base
        } else {
            let mut counter = 1u32;
            loop {
                let candidate = format!("{base}_{counter}");
                if self.objects.iter().all(|o| o.name != candidate) {
                    break candidate;
                }
                counter += 1;
            }
        };

        clone.name = new_name.clone();
        self.objects.push(clone);
        Some(new_name)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ObjectKind, Project, ProjectObject};

    fn sample() -> Project {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("a", ObjectKind::Gerber)).unwrap();
        p.add(ProjectObject::new("b", ObjectKind::Excellon)).unwrap();
        p.add(ProjectObject::new("c", ObjectKind::Geometry)).unwrap();
        p
    }

    #[test]
    fn set_visible_flips_flag() {
        let mut p = sample();
        assert!(p.get("a").unwrap().visible);
        assert!(p.set_visible("a", false));
        assert!(!p.get("a").unwrap().visible);
        assert!(p.set_visible("a", true));
        assert!(p.get("a").unwrap().visible);
        // Missing object reports failure.
        assert!(!p.set_visible("missing", false));
    }

    #[test]
    fn toggle_visible_returns_new_state() {
        let mut p = sample();
        // Starts true -> toggling yields false, then true again.
        assert!(!p.toggle_visible("b"));
        assert!(!p.get("b").unwrap().visible);
        assert!(p.toggle_visible("b"));
        assert!(p.get("b").unwrap().visible);
        // Missing object returns false.
        assert!(!p.toggle_visible("missing"));
    }

    #[test]
    fn move_up_swaps_with_previous() {
        let mut p = sample();
        // Move the second item ("b") up; order becomes b, a, c.
        assert!(p.move_up("b"));
        assert_eq!(p.objects[0].name, "b");
        assert_eq!(p.objects[1].name, "a");
        assert_eq!(p.objects[2].name, "c");
    }

    #[test]
    fn move_up_on_first_returns_false() {
        let mut p = sample();
        assert!(!p.move_up("a"));
        // Order unchanged.
        assert_eq!(p.objects[0].name, "a");
        // Missing object also returns false.
        assert!(!p.move_up("missing"));
    }

    #[test]
    fn move_down_swaps_with_next() {
        let mut p = sample();
        // Move "b" down; order becomes a, c, b.
        assert!(p.move_down("b"));
        assert_eq!(p.objects[0].name, "a");
        assert_eq!(p.objects[1].name, "c");
        assert_eq!(p.objects[2].name, "b");
        // Last item cannot move down.
        assert!(!p.move_down("b"));
    }

    #[test]
    fn duplicate_creates_unique_copy() {
        let mut p = sample();
        let count_before = p.objects.len();

        let n1 = p.duplicate("a").unwrap();
        assert_eq!(n1, "a_copy");
        assert_eq!(p.objects.len(), count_before + 1);
        assert_eq!(p.get("a_copy").unwrap().kind, ObjectKind::Gerber);

        // A second duplicate must pick a distinct, unique name.
        let n2 = p.duplicate("a").unwrap();
        assert_eq!(n2, "a_copy_1");
        assert_eq!(p.objects.len(), count_before + 2);

        // Duplicating a missing object yields None.
        assert!(p.duplicate("missing").is_none());
    }

    #[test]
    fn duplicate_preserves_options() {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("x", ObjectKind::Gerber).set("dia", "0.4"))
            .unwrap();
        let new_name = p.duplicate("x").unwrap();
        assert_eq!(
            p.get(&new_name).unwrap().options.get("dia"),
            Some(&"0.4".to_string())
        );
    }
}
