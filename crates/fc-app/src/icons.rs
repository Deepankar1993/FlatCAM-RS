//! Kind display metadata for the project tree.
//!
//! Pure, stateless helpers that map an [`crate::ObjectKind`] to the visual
//! attributes used when rendering the object tree: a human-readable label, a
//! short glyph (icon), an RGB accent color, and a stable sort index.
//!
//! Nothing here touches [`crate::Project`] — these are display lookups only.

use crate::ObjectKind;

/// Human-readable label for a kind, suitable for tree headers / tooltips.
pub fn kind_label(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Gerber => "Gerber",
        ObjectKind::Excellon => "Excellon",
        ObjectKind::Geometry => "Geometry",
        ObjectKind::CncJob => "CNC Job",
        ObjectKind::Svg => "SVG",
        ObjectKind::Document => "Document",
    }
}

/// A short, distinct glyph for each kind.
///
/// These are single emoji glyphs chosen to read well at small sizes in a tree.
/// Each kind returns a different, non-empty string.
pub fn kind_icon(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Gerber => "\u{25A6}",   // ▦ square-with-grid: copper layer
        ObjectKind::Excellon => "\u{25CE}", // ◎ bullseye: drill holes
        ObjectKind::Geometry => "\u{25B3}", // △ triangle: generic geometry
        ObjectKind::CncJob => "\u{2699}",   // ⚙ gear: machining job
        ObjectKind::Svg => "\u{2712}",      // ✒ nib: vector drawing
        ObjectKind::Document => "\u{25A4}", // ▤ square-with-lines: text doc
    }
}

/// A distinct accent RGB color per kind.
///
/// Palette (chosen to be visually distinct and roughly themed):
/// - Gerber   = green   (0x2E, 0x8B, 0x57)  copper / solder-mask green
/// - Excellon = red     (0xC0, 0x39, 0x2B)  drill marks
/// - Geometry = blue    (0x29, 0x80, 0xB9)  CAD outlines
/// - CncJob   = orange  (0xE6, 0x7E, 0x22)  machining / toolpath
/// - Svg      = purple  (0x8E, 0x44, 0xAD)  vector art
/// - Document = gray    (0x7F, 0x8C, 0x8D)  neutral text
pub fn kind_color(kind: ObjectKind) -> (u8, u8, u8) {
    match kind {
        ObjectKind::Gerber => (0x2E, 0x8B, 0x57),
        ObjectKind::Excellon => (0xC0, 0x39, 0x2B),
        ObjectKind::Geometry => (0x29, 0x80, 0xB9),
        ObjectKind::CncJob => (0xE6, 0x7E, 0x22),
        ObjectKind::Svg => (0x8E, 0x44, 0xAD),
        ObjectKind::Document => (0x7F, 0x8C, 0x8D),
    }
}

/// Stable sort index used to order kinds in the tree.
///
/// Gerber=0, Excellon=1, Geometry=2, CncJob=3, Svg=4, Document=5.
pub fn kind_sort_index(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Gerber => 0,
        ObjectKind::Excellon => 1,
        ObjectKind::Geometry => 2,
        ObjectKind::CncJob => 3,
        ObjectKind::Svg => 4,
        ObjectKind::Document => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const ALL: [ObjectKind; 6] = [
        ObjectKind::Gerber,
        ObjectKind::Excellon,
        ObjectKind::Geometry,
        ObjectKind::CncJob,
        ObjectKind::Svg,
        ObjectKind::Document,
    ];

    #[test]
    fn labels_are_distinct_and_nonempty() {
        let mut seen = HashSet::new();
        for &k in &ALL {
            let label = kind_label(k);
            assert!(!label.is_empty(), "label empty for {:?}", k);
            assert!(seen.insert(label), "duplicate label: {label}");
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn icons_are_distinct_and_nonempty() {
        let mut seen = HashSet::new();
        for &k in &ALL {
            let icon = kind_icon(k);
            assert!(!icon.is_empty(), "icon empty for {:?}", k);
            assert!(seen.insert(icon), "duplicate icon: {icon}");
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn colors_are_distinct() {
        let mut seen = HashSet::new();
        for &k in &ALL {
            let color = kind_color(k);
            assert!(seen.insert(color), "duplicate color: {:?}", color);
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn sort_indices_are_zero_to_five_and_unique() {
        let mut seen = HashSet::new();
        for &k in &ALL {
            let idx = kind_sort_index(k);
            assert!(idx <= 5, "index out of range for {:?}: {idx}", k);
            assert!(seen.insert(idx), "duplicate index: {idx}");
        }
        // exactly the set {0,1,2,3,4,5}
        let expected: HashSet<u8> = (0..=5).collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn specific_mappings() {
        assert_eq!(kind_label(ObjectKind::CncJob), "CNC Job");
        assert_eq!(kind_sort_index(ObjectKind::Gerber), 0);
        assert_eq!(kind_sort_index(ObjectKind::Document), 5);
        assert_eq!(kind_color(ObjectKind::Gerber), (0x2E, 0x8B, 0x57));
    }
}
