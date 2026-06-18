//! Single-stroke (stick) vector font for engraving labels.
//!
//! A minimal blocky/segment font covering uppercase `A`-`Z`, digits `0`-`9`
//! and space. Each glyph is defined as one or more polylines on a unit grid
//! `0..1` (width) × `0..1` (height); [`text_to_polylines`] scales those by the
//! requested cap height, lays glyphs left to right, and returns the resulting
//! stroke polylines in world coordinates ready to feed into a milling job.

use fc_gcode::Polyline;

/// Nominal glyph width on the unit grid (advance is `glyph_width·height + spacing`).
const GLYPH_WIDTH: f64 = 0.6;

/// Lay `text` out left to right and return all stroke polylines in world coords.
///
/// * `height` — cap height; the unit glyph is uniformly scaled by this.
/// * `char_spacing` — extra gap added between glyph cells, in world units.
/// * `origin` — `(x, y)` of the lower-left of the first glyph cell.
///
/// Each character advances the pen by `GLYPH_WIDTH·height + char_spacing`.
/// Spaces (and any unknown characters) emit no strokes but still advance.
pub fn text_to_polylines(
    text: &str,
    height: f64,
    char_spacing: f64,
    origin: (f64, f64),
) -> Vec<Polyline> {
    let (ox, oy) = origin;
    let advance = GLYPH_WIDTH * height + char_spacing;
    let mut out: Vec<Polyline> = Vec::new();
    let mut pen_x = ox;

    for ch in text.chars() {
        let upper = ch.to_ascii_uppercase();
        for stroke in glyph(upper) {
            let world: Polyline = stroke
                .iter()
                .map(|&(ux, uy)| (pen_x + ux * height, oy + uy * height))
                .collect();
            out.push(world);
        }
        pen_x += advance;
    }

    out
}

/// Glyph stroke data in unit coordinates (`0..GLYPH_WIDTH` × `0..1`).
///
/// Returns one polyline per pen stroke. Space and unknown characters return an
/// empty vector (no strokes).
fn glyph(ch: char) -> Vec<Vec<(f64, f64)>> {
    // Convenience corner coordinates on the unit grid.
    let l = 0.0; // left
    let r = GLYPH_WIDTH; // right
    let m = GLYPH_WIDTH / 2.0; // horizontal middle
    let b = 0.0; // bottom
    let t = 1.0; // top
    let c = 0.5; // vertical middle

    match ch {
        ' ' => vec![],

        'A' => vec![
            vec![(l, b), (l, 0.7), (m, t), (r, 0.7), (r, b)],
            vec![(l, c), (r, c)],
        ],
        'B' => vec![
            vec![(l, b), (l, t), (r, 0.75), (l, c), (r, 0.25), (l, b)],
        ],
        'C' => vec![vec![(r, 0.8), (m, t), (l, 0.7), (l, 0.3), (m, b), (r, 0.2)]],
        'D' => vec![vec![(l, b), (l, t), (m, t), (r, 0.7), (r, 0.3), (m, b), (l, b)]],
        'E' => vec![
            vec![(r, b), (l, b), (l, t), (r, t)],
            vec![(l, c), (0.45, c)],
        ],
        'F' => vec![
            vec![(l, b), (l, t), (r, t)],
            vec![(l, c), (0.45, c)],
        ],
        'G' => vec![
            vec![(r, 0.8), (m, t), (l, 0.7), (l, 0.3), (m, b), (r, 0.2), (r, c), (m, c)],
        ],
        'H' => vec![
            vec![(l, b), (l, t)],
            vec![(r, b), (r, t)],
            vec![(l, c), (r, c)],
        ],
        'I' => vec![
            vec![(l, t), (r, t)],
            vec![(m, t), (m, b)],
            vec![(l, b), (r, b)],
        ],
        'J' => vec![vec![(r, t), (r, 0.25), (m, b), (l, 0.2)]],
        'K' => vec![
            vec![(l, b), (l, t)],
            vec![(r, t), (l, c), (r, b)],
        ],
        'L' => vec![vec![(l, t), (l, b), (r, b)]],
        'M' => vec![vec![(l, b), (l, t), (m, c), (r, t), (r, b)]],
        'N' => vec![vec![(l, b), (l, t), (r, b), (r, t)]],
        'O' => vec![vec![
            (m, b),
            (l, 0.3),
            (l, 0.7),
            (m, t),
            (r, 0.7),
            (r, 0.3),
            (m, b),
        ]],
        'P' => vec![vec![(l, b), (l, t), (r, 0.75), (l, c)]],
        'Q' => vec![
            vec![(m, b), (l, 0.3), (l, 0.7), (m, t), (r, 0.7), (r, 0.3), (m, b)],
            vec![(c, 0.25), (r, b)],
        ],
        'R' => vec![
            vec![(l, b), (l, t), (r, 0.75), (l, c)],
            vec![(c, c), (r, b)],
        ],
        'S' => vec![vec![
            (r, 0.8),
            (m, t),
            (l, 0.8),
            (m, c),
            (r, 0.2),
            (m, b),
            (l, 0.2),
        ]],
        'T' => vec![
            vec![(l, t), (r, t)],
            vec![(m, t), (m, b)],
        ],
        'U' => vec![vec![(l, t), (l, 0.25), (m, b), (r, 0.25), (r, t)]],
        'V' => vec![vec![(l, t), (m, b), (r, t)]],
        'W' => vec![vec![(l, t), (0.15, b), (m, c), (0.45, b), (r, t)]],
        'X' => vec![
            vec![(l, b), (r, t)],
            vec![(l, t), (r, b)],
        ],
        'Y' => vec![
            vec![(l, t), (m, c), (r, t)],
            vec![(m, c), (m, b)],
        ],
        'Z' => vec![vec![(l, t), (r, t), (l, b), (r, b)]],

        '0' => vec![
            vec![(m, b), (l, 0.3), (l, 0.7), (m, t), (r, 0.7), (r, 0.3), (m, b)],
            vec![(l, 0.3), (r, 0.7)],
        ],
        '1' => vec![
            vec![(0.2, 0.8), (m, t), (m, b)],
            vec![(l, b), (r, b)],
        ],
        '2' => vec![vec![(l, 0.8), (m, t), (r, 0.75), (r, c), (l, b), (r, b)]],
        '3' => vec![vec![
            (l, 0.8),
            (m, t),
            (r, 0.75),
            (m, c),
            (r, 0.25),
            (m, b),
            (l, 0.2),
        ]],
        '4' => vec![
            vec![(r, 0.3), (l, 0.3), (0.45, t)],
            vec![(0.45, t), (0.45, b)],
        ],
        '5' => vec![vec![(r, t), (l, t), (l, c), (m, c), (r, 0.25), (m, b), (l, 0.2)]],
        '6' => vec![vec![
            (r, 0.8),
            (m, t),
            (l, 0.6),
            (l, 0.25),
            (m, b),
            (r, 0.25),
            (r, 0.4),
            (m, c),
            (l, 0.4),
        ]],
        '7' => vec![vec![(l, t), (r, t), (m, b)]],
        '8' => vec![
            vec![(m, c), (l, 0.7), (m, t), (r, 0.7), (m, c), (l, 0.3), (m, b), (r, 0.3), (m, c)],
        ],
        '9' => vec![vec![
            (m, c),
            (r, 0.6),
            (r, t),
            (m, t),
            (l, 0.75),
            (l, 0.6),
            (m, c),
            (r, 0.6),
        ]],

        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn y_extent(polys: &[Polyline]) -> f64 {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for p in polys {
            for &(_, y) in p {
                if y < min {
                    min = y;
                }
                if y > max {
                    max = y;
                }
            }
        }
        if max < min {
            0.0
        } else {
            max - min
        }
    }

    #[test]
    fn single_letter_has_strokes() {
        let strokes = text_to_polylines("A", 10.0, 1.0, (0.0, 0.0));
        assert!(!strokes.is_empty(), "A should yield at least one polyline");
    }

    #[test]
    fn two_letters_have_more_strokes() {
        let a = text_to_polylines("A", 10.0, 1.0, (0.0, 0.0));
        let ab = text_to_polylines("AB", 10.0, 1.0, (0.0, 0.0));
        assert!(
            ab.len() > a.len(),
            "AB ({}) should have more strokes than A ({})",
            ab.len(),
            a.len()
        );
    }

    #[test]
    fn spaces_yield_no_strokes() {
        let strokes = text_to_polylines("  ", 10.0, 1.0, (0.0, 0.0));
        assert_eq!(strokes.len(), 0, "spaces should emit no strokes");
    }

    #[test]
    fn doubling_height_doubles_y_extent() {
        let small = text_to_polylines("E", 10.0, 0.0, (0.0, 0.0));
        let big = text_to_polylines("E", 20.0, 0.0, (0.0, 0.0));
        let es = y_extent(&small);
        let eb = y_extent(&big);
        assert!(es > 0.0 && eb > 0.0);
        assert!(
            (eb / es - 2.0).abs() < 1e-9,
            "doubling height should roughly double y-extent: {} vs {}",
            es,
            eb
        );
    }

    #[test]
    fn covers_alphanumerics() {
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars() {
            let s = ch.to_string();
            let strokes = text_to_polylines(&s, 5.0, 0.0, (0.0, 0.0));
            assert!(!strokes.is_empty(), "glyph {} should have strokes", ch);
        }
    }

    #[test]
    fn lowercase_maps_to_uppercase() {
        let lower = text_to_polylines("abc", 8.0, 1.0, (0.0, 0.0));
        let upper = text_to_polylines("ABC", 8.0, 1.0, (0.0, 0.0));
        assert_eq!(lower.len(), upper.len());
    }

    #[test]
    fn origin_offsets_world_coords() {
        let at_origin = text_to_polylines("A", 10.0, 0.0, (0.0, 0.0));
        let shifted = text_to_polylines("A", 10.0, 0.0, (100.0, 50.0));
        let (x0, y0) = at_origin[0][0];
        let (x1, y1) = shifted[0][0];
        assert!((x1 - x0 - 100.0).abs() < 1e-9);
        assert!((y1 - y0 - 50.0).abs() < 1e-9);
    }
}
