//! Line-based G-code / text editor model.
//!
//! GUI-free port of FlatCAM's `GCodeEditor` / `TextEditor` cores. This is pure
//! text manipulation with no dependencies beyond `std`. The model holds the
//! document as a `Vec<String>` (one entry per line) and exposes editing,
//! searching and G-code-specific operations (line renumbering, comment
//! stripping).

/// A simple line-based text buffer for G-code / plain text editing.
#[derive(Clone, Debug, Default)]
pub struct GCodeEditor {
    /// The document, stored one line per element (no trailing newlines).
    pub lines: Vec<String>,
}

impl GCodeEditor {
    /// Build an editor from a text blob, splitting on `'\n'`.
    ///
    /// A trailing `'\r'` (from CRLF line endings) is trimmed from each line so
    /// the model is independent of the source platform's line endings.
    pub fn from_text(s: &str) -> Self {
        let lines = s
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        Self { lines }
    }

    /// Serialize the document back to a single string, joining with `'\n'`.
    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Insert `s` as a new line at `idx`.
    ///
    /// `idx` is clamped to `lines.len()`, so an out-of-range index simply
    /// appends to the end.
    pub fn insert_line(&mut self, idx: usize, s: &str) {
        let at = idx.min(self.lines.len());
        self.lines.insert(at, s.to_string());
    }

    /// Delete the line at `idx`. Returns `true` if a line was removed.
    pub fn delete_line(&mut self, idx: usize) -> bool {
        if idx < self.lines.len() {
            self.lines.remove(idx);
            true
        } else {
            false
        }
    }

    /// Replace the line at `idx` with `s`. Returns `true` if the index existed.
    pub fn replace_line(&mut self, idx: usize, s: &str) -> bool {
        if idx < self.lines.len() {
            self.lines[idx] = s.to_string();
            true
        } else {
            false
        }
    }

    /// Return the indices of all lines that contain `needle` as a substring.
    pub fn find(&self, needle: &str) -> Vec<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// Renumber lines, prefixing each non-empty, non-comment line with
    /// `N{num} `.
    ///
    /// Numbering begins at `start` and increases by `step` for each renumbered
    /// line. Any existing leading `N<digits> ` token is stripped first so the
    /// operation is idempotent. Empty lines and comment lines (whose trimmed
    /// form starts with `'('` or `';'`) are skipped and left untouched.
    pub fn renumber(&mut self, start: u32, step: u32) {
        let mut num = start;
        for line in self.lines.iter_mut() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('(') || trimmed.starts_with(';') {
                continue;
            }
            let stripped = strip_line_number(line);
            *line = format!("N{} {}", num, stripped);
            num = num.saturating_add(step);
        }
    }

    /// Remove comment lines: any line whose trimmed form starts with `'('` or
    /// `';'` is dropped entirely.
    pub fn strip_comments(&mut self) {
        self.lines.retain(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with('(') || trimmed.starts_with(';'))
        });
    }
}

/// Strip a leading `N<digits>` token (and the single following space, if any)
/// from `line`, returning the remainder.
fn strip_line_number(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'N') && bytes.first() != Some(&b'n') {
        return line;
    }
    let mut i = 1;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // Require at least one digit to count as a line number.
    if i == 1 {
        return line;
    }
    let rest = &line[i..];
    rest.strip_prefix(' ').unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_to_text_round_trip() {
        let src = "G21\nG90\nM3 S1000\nM5";
        let ed = GCodeEditor::from_text(src);
        assert_eq!(ed.lines.len(), 4);
        assert_eq!(ed.to_text(), src);
    }

    #[test]
    fn from_text_strips_crlf() {
        let ed = GCodeEditor::from_text("G21\r\nG90\r\n");
        assert_eq!(ed.lines, vec!["G21", "G90", ""]);
        assert_eq!(ed.to_text(), "G21\nG90\n");
    }

    #[test]
    fn insert_line_at_index_and_append() {
        let mut ed = GCodeEditor::from_text("a\nc");
        ed.insert_line(1, "b");
        assert_eq!(ed.lines, vec!["a", "b", "c"]);
        // Out-of-range index appends.
        ed.insert_line(99, "d");
        assert_eq!(ed.lines, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn delete_line_returns_status() {
        let mut ed = GCodeEditor::from_text("a\nb\nc");
        assert!(ed.delete_line(1));
        assert_eq!(ed.lines, vec!["a", "c"]);
        assert!(!ed.delete_line(5));
        assert_eq!(ed.lines, vec!["a", "c"]);
    }

    #[test]
    fn replace_line_returns_status() {
        let mut ed = GCodeEditor::from_text("a\nb\nc");
        assert!(ed.replace_line(1, "B"));
        assert_eq!(ed.lines, vec!["a", "B", "c"]);
        assert!(!ed.replace_line(9, "x"));
        assert_eq!(ed.lines, vec!["a", "B", "c"]);
    }

    #[test]
    fn find_returns_correct_indices() {
        let ed = GCodeEditor::from_text("G0 X1\nG1 X2\nG0 X3\nM5");
        assert_eq!(ed.find("G0"), vec![0, 2]);
        assert_eq!(ed.find("X2"), vec![1]);
        assert_eq!(ed.find("ZZZ"), Vec::<usize>::new());
    }

    #[test]
    fn renumber_adds_sequential_numbers() {
        let mut ed = GCodeEditor::from_text("G21\nG90\nM3");
        ed.renumber(10, 10);
        assert_eq!(ed.lines, vec!["N10 G21", "N20 G90", "N30 M3"]);
    }

    #[test]
    fn renumber_skips_empty_and_comments_and_is_idempotent() {
        let mut ed = GCodeEditor::from_text("G21\n\n(comment)\n;semicolon\nG90");
        ed.renumber(10, 10);
        assert_eq!(
            ed.lines,
            vec!["N10 G21", "", "(comment)", ";semicolon", "N20 G90"]
        );
        // Running again must not stack line numbers.
        ed.renumber(10, 10);
        assert_eq!(
            ed.lines,
            vec!["N10 G21", "", "(comment)", ";semicolon", "N20 G90"]
        );
    }

    #[test]
    fn strip_comments_removes_comment_lines() {
        let mut ed = GCodeEditor::from_text("(header)\nG21\n; setup\nG90\n  (indented)\nM5");
        ed.strip_comments();
        assert_eq!(ed.lines, vec!["G21", "G90", "M5"]);
    }

    #[test]
    fn strip_line_number_helper() {
        assert_eq!(strip_line_number("N10 G21"), "G21");
        assert_eq!(strip_line_number("N100G90"), "G90");
        assert_eq!(strip_line_number("G21"), "G21");
        assert_eq!(strip_line_number("Nope"), "Nope");
    }
}
