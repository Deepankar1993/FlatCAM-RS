//! `fc-qr` — QR-code → geometry generator for the FlatCAM Rust port.
//!
//! Encodes a string as a QR code and turns each dark module into a square
//! polygon, merging adjacent modules into solid blocks via [`union_all`]. The
//! result is a [`MultiPolygon`] in the document's working units, suitable for
//! direct conversion into copper/silkscreen geometry.
//!
//! The QR grid is laid out with the origin at the bottom-left and the Y axis
//! pointing up (FlatCAM/CAM convention), so the rendered code reads upright.

use fc_geo::{centered_rect, union_all, MultiPolygon, Polygon};
use qrcode::types::Color;
use qrcode::QrCode;

/// Errors produced while generating QR geometry.
#[derive(thiserror::Error, Debug)]
pub enum QrError {
    /// The QR encoder rejected the input (e.g. too much data).
    #[error("qr encode error: {0}")]
    Encode(String),
}

/// A generated QR code as merged module geometry.
#[derive(Debug)]
pub struct QrDoc {
    /// Union of all dark-module squares.
    pub geometry: MultiPolygon<f64>,
    /// Number of modules per side (the QR grid width).
    pub modules: usize,
}

/// Encode `data` as a QR code and build its geometry.
///
/// Each dark module becomes a `module_size`-wide square; the Y axis is flipped
/// so the code is upright (row 0 of the QR ends up at the top). Adjacent dark
/// modules are merged into solid blocks.
pub fn generate(data: &str, module_size: f64) -> Result<QrDoc, QrError> {
    let code = QrCode::new(data.as_bytes()).map_err(|e| QrError::Encode(e.to_string()))?;
    let w = code.width();
    let colors = code.to_colors();

    let mut squares: Vec<Polygon<f64>> = Vec::new();
    for row in 0..w {
        for col in 0..w {
            if colors[row * w + col] == Color::Dark {
                let x_center = (col as f64 + 0.5) * module_size;
                // Flip Y so the QR is upright (origin bottom-left, Y up).
                let y_center = ((w - 1 - row) as f64 + 0.5) * module_size;
                squares.push(centered_rect(x_center, y_center, module_size, module_size));
            }
        }
    }

    let geometry = union_all(squares);
    Ok(QrDoc {
        geometry,
        modules: w,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::area;

    #[test]
    fn hello_produces_valid_grid() {
        let doc = generate("HELLO", 1.0).expect("HELLO should encode");
        // QR codes start at version 1 = 21x21 modules.
        assert!(doc.modules >= 21, "modules was {}", doc.modules);

        let a = area(&doc.geometry);
        let max = (doc.modules * doc.modules) as f64;
        assert!(a > 0.0, "geometry area should be positive, was {a}");
        assert!(
            a <= max,
            "geometry area {a} should not exceed full grid {max}"
        );
    }

    #[test]
    fn module_size_scales_area() {
        let small = generate("HELLO", 1.0).expect("encode");
        let big = generate("HELLO", 2.0).expect("encode");
        assert_eq!(small.modules, big.modules);
        // Same dark-module count, 2x linear => ~4x area.
        let ratio = area(&big.geometry) / area(&small.geometry);
        assert!((ratio - 4.0).abs() < 1e-6, "area ratio was {ratio}");
    }

    #[test]
    fn empty_input_is_handled() {
        // qrcode 0.14 encodes empty input successfully; assert the Ok path.
        match generate("", 1.0) {
            Ok(doc) => {
                assert!(doc.modules >= 21, "modules was {}", doc.modules);
                // An empty payload still yields finder/timing patterns => dark modules.
                assert!(area(&doc.geometry) > 0.0);
            }
            Err(_) => {
                // If a future encoder rejected empty input, that is also acceptable.
            }
        }
    }

    #[test]
    fn deterministic() {
        let a = generate("FlatCAM", 1.0).expect("encode");
        let b = generate("FlatCAM", 1.0).expect("encode");
        assert_eq!(a.modules, b.modules);
        assert!((area(&a.geometry) - area(&b.geometry)).abs() < 1e-12);
    }
}
