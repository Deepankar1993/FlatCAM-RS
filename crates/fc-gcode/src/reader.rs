//! `reader` — a minimal G-code reader (text -> [`CncJob`]).
//!
//! This is the inverse of the emission side: it parses a flat G-code program
//! back into the abstract motion model. The interpreter is deliberately small
//! and forgiving — it tracks only what FlatCAM-generated milling programs need:
//! modal motion mode (`G0` rapid / `G1` feed), the current tool position, and
//! whether the tool is currently cutting (Z below the work surface).
//!
//! It mirrors how `camlib.py` re-parses CNC programs: lines with `Z < 0` are
//! "cutting"; consecutive feed moves while cutting build up a milling path, and
//! lifting the tool (Z back to >= 0, or a rapid up) closes the path.

use crate::{CncJob, JobKind, JobParams, Polyline, Units};

/// Modal motion mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Motion {
    /// `G0` — rapid positioning.
    Rapid,
    /// `G1` — linear feed move.
    Feed,
}

/// Parse a G-code program into a milling [`CncJob`].
///
/// The interpreter is line-based and modal:
/// * `G20` selects inches, `G21` millimetres (default [`Units::Mm`]).
/// * `G0`/`G1` set the modal motion mode; bare coordinate lines reuse it.
/// * Axis words (`X`, `Y`, `Z`) update the current position; a missing axis
///   keeps its previous value.
/// * Z `< 0` means the tool is cutting. While cutting, every `G1` XY move
///   extends the current path. When the tool lifts (Z `>= 0`, or a `G0` rapid
///   to a non-cutting height) the path is finished and kept if it has >= 2 pts.
/// * Comment lines starting with `(`, `;` or `%` are ignored.
pub fn parse_gcode(text: &str) -> CncJob {
    let mut units = Units::Mm;
    let mut motion = Motion::Rapid;

    let mut x = 0.0_f64;
    let mut y = 0.0_f64;
    let mut z = 0.0_f64;
    let mut cutting = false;

    let mut paths: Vec<Polyline> = Vec::new();
    let mut current: Polyline = Vec::new();

    // Close out the in-progress path, keeping it only if it has enough points.
    fn finish(current: &mut Polyline, paths: &mut Vec<Polyline>) {
        if current.len() >= 2 {
            paths.push(std::mem::take(current));
        } else {
            current.clear();
        }
    }

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Whole-line comments / program markers.
        let first = line.as_bytes()[0];
        if first == b'(' || first == b';' || first == b'%' {
            continue;
        }

        // First pass: handle G-words that affect units / motion mode.
        for word in line.split_whitespace() {
            let (letter, num) = match split_word(word) {
                Some(v) => v,
                None => continue,
            };
            if letter == 'G' {
                // Round to handle "G0"/"G00"/"G1.0".
                match num.round() as i64 {
                    0 => motion = Motion::Rapid,
                    1 => motion = Motion::Feed,
                    20 => units = Units::Inch,
                    21 => units = Units::Mm,
                    _ => {}
                }
            }
        }

        // Second pass: apply axis words to update position.
        let prev_z = z;
        let mut saw_xy = false;
        for word in line.split_whitespace() {
            let (letter, num) = match split_word(word) {
                Some(v) => v,
                None => continue,
            };
            match letter {
                'X' => {
                    x = num;
                    saw_xy = true;
                }
                'Y' => {
                    y = num;
                    saw_xy = true;
                }
                'Z' => z = num,
                _ => {}
            }
        }

        // Update cutting state when Z changed.
        if z != prev_z {
            let now_cutting = z < 0.0;
            if now_cutting && !cutting {
                // Tool plunged: the plunge point seeds the path with the
                // current XY (the location reached before the plunge).
                current.clear();
                current.push((x, y));
            } else if !now_cutting && cutting {
                // Tool lifted: finish the current path.
                finish(&mut current, &mut paths);
            }
            cutting = now_cutting;
        }

        match motion {
            Motion::Rapid => {
                // A rapid up (when not cutting) ends any active cut path.
                if !cutting {
                    finish(&mut current, &mut paths);
                }
            }
            Motion::Feed => {
                if cutting && saw_xy {
                    current.push((x, y));
                }
            }
        }
    }

    // Flush any path still open at end of program.
    finish(&mut current, &mut paths);

    CncJob {
        params: JobParams {
            units,
            ..JobParams::default()
        },
        kind: JobKind::Mill { paths },
    }
}

/// Split a G-code word like `X12.3` into its letter and numeric value.
/// Returns `None` if the word has no leading letter or an unparseable number.
fn split_word(word: &str) -> Option<(char, f64)> {
    let mut chars = word.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let rest = chars.as_str();
    let num: f64 = rest.parse().ok()?;
    Some((letter, num))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_of(job: &CncJob) -> &Vec<Polyline> {
        match &job.kind {
            JobKind::Mill { paths } => paths,
            JobKind::Drill { .. } => panic!("expected a Mill job"),
        }
    }

    #[test]
    fn parses_grbl_like_program() {
        let src = "\
(generated)
G21
G0 Z2
G0 X0 Y0
G1 Z-0.5 F60
G1 X10 Y0 F120
G1 X10 Y10
G0 Z2
";
        let job = parse_gcode(src);
        assert_eq!(job.params.units, Units::Mm);
        let paths = paths_of(&job);
        assert_eq!(paths.len(), 1, "exactly one milling path");
        assert_eq!(paths[0], vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    }

    #[test]
    fn g20_selects_inch() {
        let job = parse_gcode("G20\nG0 Z1\n");
        assert_eq!(job.params.units, Units::Inch);
    }

    #[test]
    fn empty_input_has_no_paths() {
        let job = parse_gcode("");
        assert_eq!(paths_of(&job).len(), 0);
    }

    #[test]
    fn comments_and_markers_ignored() {
        let src = "\
%
; a comment
(another comment)
G21
G0 X0 Y0
G1 Z-1 F60
G1 X5 Y5
G1 Z1
";
        let job = parse_gcode(src);
        let paths = paths_of(&job);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], vec![(0.0, 0.0), (5.0, 5.0)]);
    }

    #[test]
    fn plunge_with_no_feed_move_discarded() {
        // Plunge then immediately lift: only the seed point, < 2 pts -> dropped.
        let src = "G21\nG0 X3 Y3\nG1 Z-0.5 F60\nG0 Z2\n";
        let job = parse_gcode(src);
        assert_eq!(paths_of(&job).len(), 0);
    }

    #[test]
    fn plunge_point_seeds_path() {
        // Plunge at (0,0), one feed move to (3,3) -> path [(0,0),(3,3)].
        let src = "G21\nG1 Z-0.5 F60\nG1 X3 Y3\nG0 Z2\n";
        let job = parse_gcode(src);
        let paths = paths_of(&job);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], vec![(0.0, 0.0), (3.0, 3.0)]);
    }

    #[test]
    fn two_separate_paths() {
        let src = "\
G21
G0 X0 Y0
G1 Z-0.5 F60
G1 X1 Y0
G1 X1 Y1
G0 Z2
G0 X5 Y5
G1 Z-0.5 F60
G1 X6 Y5
G1 X6 Y6
G0 Z2
";
        let job = parse_gcode(src);
        let paths = paths_of(&job);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert_eq!(paths[1], vec![(5.0, 5.0), (6.0, 5.0), (6.0, 6.0)]);
    }
}
