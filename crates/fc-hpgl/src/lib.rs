//! HPGL/2 plotter file parser.
//!
//! Parses a subset of the HP Graphics Language (HPGL/2) commonly emitted by
//! PCB design tools when exporting copper/silkscreen layers as plotter files.
//!
//! Supported mnemonics (each terminated by `;`):
//! - `IN`        — initialize (resets pen state and absolute mode)
//! - `SP n`      — select pen `n`
//! - `PU [...]`  — pen up (subsequent moves are travels, start a new polyline)
//! - `PD [...]`  — pen down (subsequent moves draw, extending the polyline)
//! - `PA [...]`  — set absolute coordinate mode (with optional coordinate list)
//! - `PR [...]`  — set relative coordinate mode (with optional coordinate list)
//!
//! Coordinate lists may follow `PU`/`PD`/`PA`/`PR` directly, e.g.
//! `PD100,100,200,200`. Each pair `x,y` is a move; when the pen is down the
//! point extends the current polyline, when the pen is up it begins a new one.
//!
//! Coordinates are kept in raw plotter units as `f64`. By convention one
//! plotter unit equals 0.025 mm (40 units/mm), but this parser applies a scale
//! factor of 1.0 and leaves conversion to the caller.

use fc_geo::{Coord, LineString};

/// Errors that can occur while parsing an HPGL document.
#[derive(Debug, thiserror::Error)]
pub enum HpglError {
    /// A coordinate token could not be parsed as a number.
    #[error("invalid coordinate '{0}'")]
    InvalidCoordinate(String),
    /// A coordinate list had an odd number of values (dangling x without y).
    #[error("odd number of coordinates in command '{0}'")]
    OddCoordinateCount(String),
}

/// A parsed HPGL document: a collection of pen-down polylines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HpglDoc {
    /// Each polyline is a contiguous pen-down stroke in plotter units.
    pub polylines: Vec<LineString<f64>>,
}

#[derive(Clone, Copy, PartialEq)]
enum CoordMode {
    Absolute,
    Relative,
}

struct ParserState {
    pen_down: bool,
    mode: CoordMode,
    pos: Coord<f64>,
    has_pos: bool,
    current: Vec<Coord<f64>>,
    polylines: Vec<LineString<f64>>,
}

impl ParserState {
    fn new() -> Self {
        ParserState {
            pen_down: false,
            mode: CoordMode::Absolute,
            pos: Coord { x: 0.0, y: 0.0 },
            has_pos: false,
            current: Vec::new(),
            polylines: Vec::new(),
        }
    }

    /// Flush the in-progress polyline (if it has >= 2 points) into the result.
    fn flush(&mut self) {
        if self.current.len() >= 2 {
            self.polylines
                .push(LineString::new(std::mem::take(&mut self.current)));
        } else {
            self.current.clear();
        }
    }

    /// Apply a single move to (x,y) given the current pen state.
    fn move_to(&mut self, x: f64, y: f64) {
        let target = match self.mode {
            CoordMode::Absolute => Coord { x, y },
            CoordMode::Relative => {
                if self.has_pos {
                    Coord {
                        x: self.pos.x + x,
                        y: self.pos.y + y,
                    }
                } else {
                    // No prior position: treat relative as from origin.
                    Coord { x, y }
                }
            }
        };

        if self.pen_down {
            // Begin a new polyline from the current position if needed.
            if self.current.is_empty() {
                if self.has_pos {
                    self.current.push(self.pos);
                } else {
                    self.current.push(target);
                    self.pos = target;
                    self.has_pos = true;
                    return;
                }
            }
            self.current.push(target);
        } else {
            // Pen up: any open stroke ends here.
            self.flush();
        }

        self.pos = target;
        self.has_pos = true;
    }
}

/// Parse HPGL `text` into an [`HpglDoc`].
///
/// Whitespace (including newlines) is insignificant outside coordinate tokens.
/// Unknown commands are ignored gracefully.
pub fn parse(text: &str) -> Result<HpglDoc, HpglError> {
    let mut st = ParserState::new();

    for raw in text.split(';') {
        let cmd = raw.trim();
        if cmd.is_empty() {
            continue;
        }

        // HPGL mnemonics are exactly two letters; the rest is the parameter
        // list. Capping at two ensures e.g. `PDfoo,0` parses as PD + "foo,0"
        // (an invalid coordinate) rather than being mistaken for one mnemonic.
        let alpha_len = cmd
            .char_indices()
            .find(|(_, c)| !c.is_ascii_alphabetic())
            .map(|(i, _)| i)
            .unwrap_or(cmd.len());
        let mnemonic_len = alpha_len.min(2);
        let mnemonic = cmd[..mnemonic_len].to_ascii_uppercase();
        let rest = cmd[mnemonic_len..].trim();

        match mnemonic.as_str() {
            "IN" => {
                st.flush();
                st.pen_down = false;
                st.mode = CoordMode::Absolute;
                st.has_pos = false;
                st.pos = Coord { x: 0.0, y: 0.0 };
            }
            "SP" => {
                // Pen selection does not affect geometry; parameter ignored.
            }
            "PA" => {
                st.mode = CoordMode::Absolute;
                apply_coords(&mut st, rest, cmd)?;
            }
            "PR" => {
                st.mode = CoordMode::Relative;
                apply_coords(&mut st, rest, cmd)?;
            }
            "PU" => {
                // Process any coordinates while still pen-down (HPGL applies the
                // moves with the *previous* pen state, then raises the pen), but
                // practically PU coordinates are travels: flush first, then move
                // with pen raised.
                st.pen_down = false;
                st.flush();
                apply_coords(&mut st, rest, cmd)?;
            }
            "PD" => {
                st.pen_down = true;
                apply_coords(&mut st, rest, cmd)?;
            }
            _ => {
                // Unknown / unsupported command: ignore.
            }
        }
    }

    st.flush();
    Ok(HpglDoc {
        polylines: st.polylines,
    })
}

/// Parse a comma-separated coordinate list and feed each (x,y) pair as a move.
fn apply_coords(st: &mut ParserState, rest: &str, full_cmd: &str) -> Result<(), HpglError> {
    if rest.is_empty() {
        return Ok(());
    }

    let mut nums: Vec<f64> = Vec::new();
    for tok in rest.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let v: f64 = tok
            .parse()
            .map_err(|_| HpglError::InvalidCoordinate(tok.to_string()))?;
        nums.push(v);
    }

    if nums.len() % 2 != 0 {
        return Err(HpglError::OddCoordinateCount(full_cmd.to_string()));
    }

    for pair in nums.chunks_exact(2) {
        st.move_to(pair[0], pair[1]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(ls: &LineString<f64>) -> Vec<(f64, f64)> {
        ls.0.iter().map(|c| (c.x, c.y)).collect()
    }

    #[test]
    fn basic_pen_down_stroke() {
        let doc = parse("IN;SP1;PU100,100;PD200,100,200,200;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]
        );
    }

    #[test]
    fn empty_input_yields_no_polylines() {
        let doc = parse("").unwrap();
        assert_eq!(doc.polylines.len(), 0);
    }

    #[test]
    fn whitespace_only_yields_no_polylines() {
        let doc = parse("   \n  \t ").unwrap();
        assert_eq!(doc.polylines.len(), 0);
    }

    #[test]
    fn two_separate_strokes() {
        let doc = parse("IN;PU0,0;PD10,0,10,10;PU50,50;PD60,50,60,60;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 2);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]
        );
        assert_eq!(
            pts(&doc.polylines[1]),
            vec![(50.0, 50.0), (60.0, 50.0), (60.0, 60.0)]
        );
    }

    #[test]
    fn relative_mode_accumulates() {
        // PU to (100,100); switch to relative; PD draws +10,+0 then +0,+10.
        let doc = parse("IN;PU100,100;PR;PD10,0,0,10;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(100.0, 100.0), (110.0, 100.0), (110.0, 110.0)]
        );
    }

    #[test]
    fn pa_with_inline_coordinates_pen_down() {
        let doc = parse("IN;PD;PA0,0,5,5,10,0;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)]
        );
    }

    #[test]
    fn single_point_stroke_is_dropped() {
        // Pen down but only one coordinate -> no 2-point polyline produced.
        let doc = parse("IN;PD100,100;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 0);
    }

    #[test]
    fn negative_and_fractional_coords() {
        let doc = parse("IN;PU-5.5,-2.25;PD-1.0,3.5;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(-5.5, -2.25), (-1.0, 3.5)]
        );
    }

    #[test]
    fn odd_coordinate_count_errors() {
        let err = parse("IN;PD0,0,5;PU;").unwrap_err();
        assert!(matches!(err, HpglError::OddCoordinateCount(_)));
    }

    #[test]
    fn invalid_coordinate_errors() {
        let err = parse("IN;PDfoo,0;").unwrap_err();
        assert!(matches!(err, HpglError::InvalidCoordinate(_)));
    }

    #[test]
    fn unknown_commands_ignored() {
        let doc = parse("IN;LT2;WU0.3;PU0,0;PD10,10;XYZ;PU;").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(pts(&doc.polylines[0]), vec![(0.0, 0.0), (10.0, 10.0)]);
    }

    #[test]
    fn newlines_between_commands() {
        let doc = parse("IN;\nPU0,0;\nPD10,0,\n10,10;\nPU;\n").unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(
            pts(&doc.polylines[0]),
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]
        );
    }
}
