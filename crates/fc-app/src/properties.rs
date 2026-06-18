//! Per-object property rows for an inspector panel.
//!
//! Rust port of FlatCAM's object properties view (`ToolReport` / the
//! "Properties" inspector). Given a [`crate::ProjectObject`], this produces a
//! flat, ordered list of `(label, value)` rows suitable for display in a
//! read-only table widget.

use crate::{ObjectKind, ProjectObject};

/// Map an [`ObjectKind`] to its lowercase string label (matching FlatCAM's
/// `kind` strings).
pub fn kind_label(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Gerber => "gerber",
        ObjectKind::Excellon => "excellon",
        ObjectKind::Geometry => "geometry",
        ObjectKind::CncJob => "cncjob",
        ObjectKind::Svg => "svg",
        ObjectKind::Document => "document",
    }
}

/// Build the ordered property rows for an object.
///
/// Always includes `Name`, `Kind`, and `Visible`. `Source` and `Parent` rows
/// are included only when present. Each option is then appended as its own
/// `(key, value)` row, in sorted key order (the options map is a `BTreeMap`,
/// so iteration is already sorted).
pub fn properties(obj: &ProjectObject) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();

    rows.push(("Name".to_string(), obj.name.clone()));
    rows.push(("Kind".to_string(), kind_label(obj.kind).to_string()));
    rows.push(("Visible".to_string(), obj.visible.to_string()));

    if let Some(src) = &obj.source_path {
        rows.push(("Source".to_string(), src.clone()));
    }
    if let Some(parent) = &obj.parent {
        rows.push(("Parent".to_string(), parent.clone()));
    }

    for (key, value) in &obj.options {
        rows.push((key.clone(), value.clone()));
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjectKind;

    fn find<'a>(rows: &'a [(String, String)], key: &str) -> Option<&'a str> {
        rows.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn gerber_with_source_and_option() {
        let obj = ProjectObject::new("top.gbr", ObjectKind::Gerber)
            .with_source("/tmp/top.gbr")
            .set("isolation_tool_dia", "0.4");
        let rows = properties(&obj);

        assert_eq!(find(&rows, "Name"), Some("top.gbr"));
        assert_eq!(find(&rows, "Kind"), Some("gerber"));
        assert_eq!(find(&rows, "Visible"), Some("true"));
        assert_eq!(find(&rows, "Source"), Some("/tmp/top.gbr"));
        assert_eq!(find(&rows, "isolation_tool_dia"), Some("0.4"));
    }

    #[test]
    fn no_parent_row_when_parent_none() {
        let obj = ProjectObject::new("plain", ObjectKind::Geometry);
        let rows = properties(&obj);
        assert!(rows.iter().all(|(k, _)| k != "Parent"));
        assert!(rows.iter().all(|(k, _)| k != "Source"));
    }

    #[test]
    fn parent_and_visible_rows() {
        let mut obj = ProjectObject::new("job", ObjectKind::CncJob);
        obj.parent = Some("geo".to_string());
        obj.visible = false;
        let rows = properties(&obj);
        assert_eq!(find(&rows, "Parent"), Some("geo"));
        assert_eq!(find(&rows, "Visible"), Some("false"));
        assert_eq!(find(&rows, "Kind"), Some("cncjob"));
    }

    #[test]
    fn kind_labels() {
        assert_eq!(kind_label(ObjectKind::Gerber), "gerber");
        assert_eq!(kind_label(ObjectKind::Excellon), "excellon");
        assert_eq!(kind_label(ObjectKind::Geometry), "geometry");
        assert_eq!(kind_label(ObjectKind::CncJob), "cncjob");
        assert_eq!(kind_label(ObjectKind::Svg), "svg");
        assert_eq!(kind_label(ObjectKind::Document), "document");
    }

    #[test]
    fn options_sorted_and_appended_after_fixed_rows() {
        let obj = ProjectObject::new("o", ObjectKind::Gerber)
            .set("zzz", "1")
            .set("aaa", "2");
        let rows = properties(&obj);
        // Fixed rows first: Name, Kind, Visible.
        assert_eq!(rows[0].0, "Name");
        assert_eq!(rows[1].0, "Kind");
        assert_eq!(rows[2].0, "Visible");
        // Options sorted by key.
        assert_eq!(rows[3], ("aaa".to_string(), "2".to_string()));
        assert_eq!(rows[4], ("zzz".to_string(), "1".to_string()));
    }
}
