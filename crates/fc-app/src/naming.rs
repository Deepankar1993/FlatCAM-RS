//! Unique naming and rename helpers for [`crate::Project`].
//!
//! Rust port of FlatCAM's auto-naming behaviour (`ObjectCollection`):
//! when an object name collides with an existing one, FlatCAM appends a
//! numeric suffix (`_1`, `_2`, …) until a free name is found. Renaming an
//! object also keeps the project consistent by re-pointing any derived
//! objects (`parent`) and the current selection at the new name.

impl crate::Project {
    /// Return a unique object name based on `base`.
    ///
    /// If `base` is not currently in use, it is returned unchanged. Otherwise
    /// `_1`, `_2`, … is appended until an unused name is produced.
    pub fn unique_name(&self, base: &str) -> String {
        if self.get(base).is_none() {
            return base.to_string();
        }
        let mut n: u32 = 1;
        loop {
            let candidate = format!("{base}_{n}");
            if self.get(&candidate).is_none() {
                return candidate;
            }
            n += 1;
        }
    }

    /// Rename the object `old` to `new`.
    ///
    /// Returns [`crate::AppError::NotFound`] if `old` does not exist or if a
    /// different object is already named `new`. On success the object is
    /// renamed, every other object whose `parent` was `old` is re-pointed to
    /// `new`, and [`crate::Project::selected`] is updated if it referenced
    /// `old`.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), crate::AppError> {
        if self.get(old).is_none() {
            return Err(crate::AppError::NotFound(format!(
                "cannot rename: no object named '{old}'"
            )));
        }
        // Renaming to the same name is a no-op (and must not be rejected as
        // "already taken").
        if old != new && self.get(new).is_some() {
            return Err(crate::AppError::NotFound(format!(
                "cannot rename to '{new}': name already taken"
            )));
        }

        for obj in &mut self.objects {
            if obj.name == old {
                obj.name = new.to_string();
            }
            if obj.parent.as_deref() == Some(old) {
                obj.parent = Some(new.to_string());
            }
        }

        if self.selected.as_deref() == Some(old) {
            self.selected = Some(new.to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{ObjectKind, Project, ProjectObject};

    fn proj() -> Project {
        let mut p = Project::new("mm");
        p.add(ProjectObject::new("board", ObjectKind::Gerber)).unwrap();
        p
    }

    #[test]
    fn unique_name_free() {
        let p = proj();
        assert_eq!(p.unique_name("fresh"), "fresh");
    }

    #[test]
    fn unique_name_taken() {
        let mut p = proj();
        assert_eq!(p.unique_name("board"), "board_1");
        p.add(ProjectObject::new("board_1", ObjectKind::Geometry)).unwrap();
        assert_eq!(p.unique_name("board"), "board_2");
    }

    #[test]
    fn rename_updates_object() {
        let mut p = proj();
        p.rename("board", "main").unwrap();
        assert!(p.get("board").is_none());
        assert_eq!(p.get("main").unwrap().kind, ObjectKind::Gerber);
    }

    #[test]
    fn rename_updates_children_and_selected() {
        let mut p = proj();
        let mut child = ProjectObject::new("board_geo", ObjectKind::Geometry);
        child.parent = Some("board".to_string());
        p.add(child).unwrap();
        p.selected = Some("board".to_string());

        p.rename("board", "main").unwrap();

        assert_eq!(p.get("board_geo").unwrap().parent.as_deref(), Some("main"));
        assert_eq!(p.selected.as_deref(), Some("main"));
    }

    #[test]
    fn rename_to_existing_errors() {
        let mut p = proj();
        p.add(ProjectObject::new("other", ObjectKind::Geometry)).unwrap();
        assert!(p.rename("board", "other").is_err());
        // Original untouched.
        assert!(p.get("board").is_some());
    }

    #[test]
    fn rename_missing_errors() {
        let mut p = proj();
        assert!(p.rename("ghost", "anything").is_err());
    }

    #[test]
    fn rename_same_name_ok() {
        let mut p = proj();
        assert!(p.rename("board", "board").is_ok());
        assert!(p.get("board").is_some());
    }
}
