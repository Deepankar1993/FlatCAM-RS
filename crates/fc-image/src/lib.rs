//! `fc-image` — raster image import and tracing for the FlatCAM Rust port.
//!
//! This crate is the Rust analogue of upstream FlatCAM's `ToolImage`: it
//! decodes a raster image (PNG / JPEG / BMP) and converts it into vector
//! geometry suitable for the CAM pipeline.
//!
//! The implementation follows upstream's **raster mode**: the image is reduced
//! to a binary "ink" mask via a luminance threshold, and each ink pixel becomes
//! a unit square. Adjacent squares are merged with [`fc_geo::union_all`] so the
//! result is a small number of clean filled regions rather than thousands of
//! disconnected squares.
//!
//! Coordinates are emitted in pixel units multiplied by [`TraceOptions::scale`].
//! Image rows run top-to-bottom (Y-down); PCB geometry is conventionally Y-up,
//! so by default the output is flipped vertically (see [`TraceOptions::flip_y`]).

use fc_geo::{union_all, Coord, LineString, MultiPolygon, Polygon};

/// Errors that can occur while importing/tracing a raster image.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    /// The bytes could not be decoded as a supported image format.
    #[error("failed to decode image: {0}")]
    Decode(String),
    /// An I/O error occurred while reading the file.
    #[error("io error: {0}")]
    Io(String),
    /// The image contained no ink pixels for the given options.
    #[error("no traceable content (image is empty after thresholding)")]
    Empty,
}

/// Options controlling how a raster image is reduced to geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceOptions {
    /// Luminance cutoff (0..=255). Pixels with luma *strictly less than* this
    /// value are treated as "ink" (dark = ink), unless [`invert`](Self::invert).
    pub threshold: u8,
    /// Invert the selection: treat *light* pixels as ink instead of dark ones.
    pub invert: bool,
    /// Multiplier applied to every output coordinate (pixels → working units).
    pub scale: f64,
    /// Flip the geometry vertically so the output is Y-up (PCB convention).
    pub flip_y: bool,
}

impl Default for TraceOptions {
    fn default() -> Self {
        TraceOptions {
            threshold: 128,
            invert: false,
            scale: 1.0,
            flip_y: true,
        }
    }
}

/// A traced raster image: merged filled regions plus the source dimensions.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageDoc {
    /// The merged ink regions, in working units (pixels × `scale`).
    pub polygons: MultiPolygon<f64>,
    /// Source image width in pixels.
    pub width: u32,
    /// Source image height in pixels.
    pub height: u32,
}

impl Default for ImageDoc {
    fn default() -> Self {
        ImageDoc {
            polygons: MultiPolygon::new(vec![]),
            width: 0,
            height: 0,
        }
    }
}

/// Decode `bytes` and trace them into geometry per `opts`.
pub fn trace_bytes(bytes: &[u8], opts: &TraceOptions) -> Result<ImageDoc, ImageError> {
    let img = image::load_from_memory(bytes).map_err(|e| ImageError::Decode(e.to_string()))?;
    let luma = img.to_luma8();
    let (width, height) = luma.dimensions();

    // Build the per-pixel ink mask. `is_ink` is true where a square is emitted.
    let is_ink = |luma_val: u8| -> bool {
        let dark = luma_val < opts.threshold;
        dark ^ opts.invert
    };

    let mut squares: Vec<Polygon<f64>> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let px = luma.get_pixel(x, y).0[0];
            if !is_ink(px) {
                continue;
            }
            squares.push(pixel_square(x, y, height, opts));
        }
    }

    if squares.is_empty() {
        return Err(ImageError::Empty);
    }

    let polygons = union_all(squares);
    Ok(ImageDoc {
        polygons,
        width,
        height,
    })
}

/// Read `path` from disk then trace it via [`trace_bytes`].
pub fn trace_file(path: &str, opts: &TraceOptions) -> Result<ImageDoc, ImageError> {
    let bytes = std::fs::read(path).map_err(|e| ImageError::Io(e.to_string()))?;
    trace_bytes(&bytes, opts)
}

/// Build the unit square (scaled) for pixel `(x, y)` of an `img_h`-row image.
///
/// Without flipping, pixel `(x, y)` spans `[x, x+1] × [y, y+1]`. With `flip_y`
/// the row is reflected about the image's vertical extent so row 0 (the top of
/// the image) ends up at the top of the Y-up output.
fn pixel_square(x: u32, y: u32, img_h: u32, opts: &TraceOptions) -> Polygon<f64> {
    let s = opts.scale;
    let x0 = x as f64 * s;
    let x1 = (x as f64 + 1.0) * s;

    let (y0, y1) = if opts.flip_y {
        // Reflect: image row y occupies [img_h - (y+1), img_h - y] in Y-up space.
        let yb = (img_h - y - 1) as f64 * s;
        let yt = (img_h - y) as f64 * s;
        (yb, yt)
    } else {
        (y as f64 * s, (y as f64 + 1.0) * s)
    };

    let ring = vec![
        Coord { x: x0, y: y0 },
        Coord { x: x1, y: y0 },
        Coord { x: x1, y: y1 },
        Coord { x: x0, y: y1 },
        Coord { x: x0, y: y0 },
    ];
    Polygon::new(LineString::new(ring), vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, bounds};
    use image::{GrayImage, ImageFormat, Luma};
    use std::io::Cursor;

    /// Encode a `GrayImage` to in-memory PNG bytes.
    fn png_bytes(img: &GrayImage) -> Vec<u8> {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .expect("encode png");
        buf
    }

    /// A `w×h` white image with the given black pixels set.
    fn img_with_black(w: u32, h: u32, black: &[(u32, u32)]) -> GrayImage {
        let mut img = GrayImage::from_pixel(w, h, Luma([255]));
        for &(x, y) in black {
            img.put_pixel(x, y, Luma([0]));
        }
        img
    }

    #[test]
    fn single_black_pixel_makes_unit_square() {
        // 4x4 white image, one black pixel at (1,1).
        let img = img_with_black(4, 4, &[(1, 1)]);
        let bytes = png_bytes(&img);
        let doc = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        assert_eq!(doc.width, 4);
        assert_eq!(doc.height, 4);
        assert_eq!(doc.polygons.0.len(), 1, "one pixel -> one polygon");
        assert!((area(&doc.polygons) - 1.0).abs() < 1e-9, "area was {}", area(&doc.polygons));
    }

    #[test]
    fn solid_black_square_merges_to_one_region() {
        // A contiguous 2x2 black block should union into a single polygon of area 4.
        let img = img_with_black(4, 4, &[(1, 1), (2, 1), (1, 2), (2, 2)]);
        let bytes = png_bytes(&img);
        let doc = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        assert_eq!(doc.polygons.0.len(), 1, "adjacent pixels should merge");
        assert!((area(&doc.polygons) - 4.0).abs() < 1e-9, "area was {}", area(&doc.polygons));
    }

    #[test]
    fn disjoint_pixels_stay_separate() {
        // Two non-touching black pixels (diagonal corners) -> two polygons.
        let img = img_with_black(4, 4, &[(0, 0), (3, 3)]);
        let bytes = png_bytes(&img);
        let doc = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        assert_eq!(doc.polygons.0.len(), 2, "diagonal pixels do not share an edge");
        assert!((area(&doc.polygons) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn all_white_is_empty() {
        let img = GrayImage::from_pixel(8, 8, Luma([255]));
        let bytes = png_bytes(&img);
        let err = trace_bytes(&bytes, &TraceOptions::default()).unwrap_err();
        assert!(matches!(err, ImageError::Empty));
    }

    #[test]
    fn invert_selects_white_pixels() {
        // White background with a single black pixel. With invert=true the
        // selection flips: the 15 white pixels become ink, the black one drops out.
        let img = img_with_black(4, 4, &[(0, 0)]);
        let bytes = png_bytes(&img);
        let opts = TraceOptions {
            invert: true,
            ..TraceOptions::default()
        };
        let doc = trace_bytes(&bytes, &opts).unwrap();
        // 16 pixels - 1 black = 15 white pixels selected.
        assert!((area(&doc.polygons) - 15.0).abs() < 1e-9, "area was {}", area(&doc.polygons));
    }

    #[test]
    fn threshold_controls_selection() {
        // A mid-gray (value 100) pixel. With threshold 128 it counts as ink
        // (100 < 128); with threshold 50 it does not (100 >= 50) -> Empty.
        let mut img = GrayImage::from_pixel(2, 2, Luma([255]));
        img.put_pixel(0, 0, Luma([100]));
        let bytes = png_bytes(&img);

        let inked = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        assert!((area(&inked.polygons) - 1.0).abs() < 1e-9);

        let low = TraceOptions {
            threshold: 50,
            ..TraceOptions::default()
        };
        let err = trace_bytes(&bytes, &low).unwrap_err();
        assert!(matches!(err, ImageError::Empty));
    }

    #[test]
    fn scale_multiplies_coordinates() {
        // One black pixel at (1,1), scale 3 -> area should be 3*3 = 9.
        let img = img_with_black(4, 4, &[(1, 1)]);
        let bytes = png_bytes(&img);
        let opts = TraceOptions {
            scale: 3.0,
            ..TraceOptions::default()
        };
        let doc = trace_bytes(&bytes, &opts).unwrap();
        assert!((area(&doc.polygons) - 9.0).abs() < 1e-9, "area was {}", area(&doc.polygons));
    }

    #[test]
    fn flip_y_reflects_vertically() {
        // Single black pixel in the TOP row (y=0) of a 4-tall image.
        // With flip_y=true (Y-up), the top image row maps to the TOP of output:
        // y in [3,4]. With flip_y=false it maps to y in [0,1].
        let img = img_with_black(4, 4, &[(0, 0)]);
        let bytes = png_bytes(&img);

        let up = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        let (_, miny_up, _, maxy_up) = bounds(&up.polygons).unwrap();
        assert!((miny_up - 3.0).abs() < 1e-9 && (maxy_up - 4.0).abs() < 1e-9,
            "flip_y top row should be at y=[3,4], got [{miny_up},{maxy_up}]");

        let down_opts = TraceOptions {
            flip_y: false,
            ..TraceOptions::default()
        };
        let down = trace_bytes(&bytes, &down_opts).unwrap();
        let (_, miny_dn, _, maxy_dn) = bounds(&down.polygons).unwrap();
        assert!((miny_dn - 0.0).abs() < 1e-9 && (maxy_dn - 1.0).abs() < 1e-9,
            "no-flip top row should be at y=[0,1], got [{miny_dn},{maxy_dn}]");
    }

    #[test]
    fn decode_error_on_garbage() {
        let err = trace_bytes(b"not an image", &TraceOptions::default()).unwrap_err();
        assert!(matches!(err, ImageError::Decode(_)));
    }

    #[test]
    fn cross_shape_merges_and_area_correct() {
        // A plus/cross of 5 connected pixels on an 8x8 grid -> one region, area 5.
        let center = (4u32, 4u32);
        let black = [
            center,
            (center.0, center.1 - 1),
            (center.0, center.1 + 1),
            (center.0 - 1, center.1),
            (center.0 + 1, center.1),
        ];
        let img = img_with_black(8, 8, &black);
        let bytes = png_bytes(&img);
        let doc = trace_bytes(&bytes, &TraceOptions::default()).unwrap();
        assert_eq!(doc.polygons.0.len(), 1, "connected cross should be one region");
        assert!((area(&doc.polygons) - 5.0).abs() < 1e-9, "area was {}", area(&doc.polygons));
    }
}
