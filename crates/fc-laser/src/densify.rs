//! Ring densification — a geometry pre-pass for anisotropic kerf offsetting.
//!
//! [`crate::offset::anisotropic_offset`] compensates for an elliptical laser
//! spot with an "affine trick": rotate the geometry into the beam's frame,
//! **non-uniformly** scale it by `(1/a, 1/b)` so the ellipse becomes a unit
//! circle, run an ordinary circular (isotropic) offset, then scale and rotate
//! back. This is exact for the offset *distance*, but it has a subtle caveat for
//! *curved* features.
//!
//! Polygons store arcs and circles as straight chords. Non-uniform scaling
//! stretches those chords **unevenly**: a segment lying along the most-scaled
//! axis (the short beam axis, scaled by the large factor `1/b`) is lengthened
//! far more than one along the least-scaled axis. The mid-chord sag (the gap
//! between the chord and the true curve) grows with the segment length, so after
//! the back-scale the reconstructed curve can deviate visibly from the intended
//! offset at high beam aspect ratios (`a/b` large). A coarse octagon that was an
//! acceptable circle approximation before scaling becomes a lumpy, faceted blob
//! afterwards.
//!
//! The fix is purely geometric and applied *before* the affine offset: subdivide
//! every ring segment so that it is short to begin with. Then even multiplied by
//! the worst-case scale factor the post-scale chord stays sub-pixel, and the
//! round-trip preserves curvature. This module is that pre-pass — it only **adds**
//! collinear interpolated points, so the polygon shape (and therefore its bounds
//! and area) is unchanged; it merely raises the vertex density.

use fc_geo::{Coord, LineString, MultiPolygon, Polygon};

/// Densify a single ring (`LineString`) so that no segment exceeds `max_seg_len`.
///
/// Walks consecutive coordinate pairs; for any pair longer than `max_seg_len`,
/// inserts `ceil(len / max_seg_len) - 1` evenly spaced interior points by linear
/// interpolation. The start of each segment is pushed exactly once (the shared
/// endpoint between consecutive segments is not duplicated) and the final ring
/// vertex is appended at the end so a closed ring stays closed.
fn densify_ring(ls: &LineString<f64>, max_seg_len: f64) -> LineString<f64> {
    let pts: Vec<Coord<f64>> = ls.coords().copied().collect();
    if pts.len() < 2 {
        return LineString::from(pts);
    }

    let mut out: Vec<Coord<f64>> = Vec::with_capacity(pts.len());
    for w in pts.windows(2) {
        let a = w[0];
        let b = w[1];
        // Always emit the segment start exactly once (shared endpoints are not
        // duplicated because the next iteration emits b as its own start).
        out.push(a);

        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > max_seg_len {
            let divisions = (len / max_seg_len).ceil() as usize;
            // Insert `divisions - 1` interior points between a and b.
            for i in 1..divisions {
                let t = i as f64 / divisions as f64;
                out.push(Coord {
                    x: a.x + dx * t,
                    y: a.y + dy * t,
                });
            }
        }
    }
    // Append the final ring vertex (last segment's end) to close the ring.
    out.push(*pts.last().unwrap());

    LineString::from(out)
}

/// Densify a single polygon's exterior and all interior rings.
fn densify_polygon(poly: &Polygon<f64>, max_seg_len: f64) -> Polygon<f64> {
    let exterior = densify_ring(poly.exterior(), max_seg_len);
    let interiors: Vec<LineString<f64>> = poly
        .interiors()
        .iter()
        .map(|ring| densify_ring(ring, max_seg_len))
        .collect();
    Polygon::new(exterior, interiors)
}

/// Return a copy of `geom` where every ring segment longer than `max_seg_len`
/// is subdivided into equal sub-segments each `<= max_seg_len`, by linear
/// interpolation between the original vertices. Original vertices are kept
/// (densification only ADDS points; the polygon shape is unchanged). A
/// non-positive or non-finite `max_seg_len` returns the geometry unchanged.
pub fn densify_rings(geom: &MultiPolygon<f64>, max_seg_len: f64) -> MultiPolygon<f64> {
    if !(max_seg_len.is_finite() && max_seg_len > 0.0) {
        return geom.clone();
    }
    let polys: Vec<Polygon<f64>> = geom
        .0
        .iter()
        .map(|poly| densify_polygon(poly, max_seg_len))
        .collect();
    MultiPolygon::new(polys)
}

/// Densify appropriately for a given beam before an anisotropic offset: choose
/// `max_seg_len` as a small fraction of the beam's SHORT extent (so the chord
/// error after the `1/b` scaling stays sub-pixel).
///
/// Uses `max_seg_len = beam.min_extent() * fraction`, with `fraction` clamped to
/// the sane range `(0, 1]` (a non-finite or non-positive `fraction` falls back
/// to the default `0.25`). Returns the densified geometry; if the resulting
/// `max_seg_len` is non-positive (degenerate beam) the geometry is returned
/// unchanged by [`densify_rings`].
pub fn densify_for_beam(
    geom: &MultiPolygon<f64>,
    beam: &crate::beam::BeamShape,
    fraction: f64,
) -> MultiPolygon<f64> {
    let fraction = if fraction.is_finite() && fraction > 0.0 {
        fraction.min(1.0)
    } else {
        0.25
    };
    let max_seg_len = beam.min_extent() * fraction;
    densify_rings(geom, max_seg_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beam::BeamShape;

    /// Count every coordinate across all rings of a multipolygon.
    fn vertex_count(mp: &MultiPolygon<f64>) -> usize {
        mp.0
            .iter()
            .map(|p| {
                p.exterior().coords().count()
                    + p.interiors().iter().map(|r| r.coords().count()).sum::<usize>()
            })
            .sum()
    }

    #[test]
    fn densifies_square_preserving_shape() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let before = vertex_count(&mp);
        let dense = densify_rings(&mp, 1.0);
        let after = vertex_count(&dense);

        // Each 10-long side -> ~10 segments, so the count jumps substantially.
        assert!(
            after > before + 30,
            "expected substantial densification: before={before} after={after}"
        );

        // Bounds unchanged within 1e-9.
        let (b0, b1, b2, b3) = fc_geo::bounds(&mp).unwrap();
        let (d0, d1, d2, d3) = fc_geo::bounds(&dense).unwrap();
        assert!((b0 - d0).abs() < 1e-9);
        assert!((b1 - d1).abs() < 1e-9);
        assert!((b2 - d2).abs() < 1e-9);
        assert!((b3 - d3).abs() < 1e-9);

        // Area unchanged within 1e-6.
        assert!((fc_geo::area(&mp) - fc_geo::area(&dense)).abs() < 1e-6);
    }

    #[test]
    fn densified_octagon_keeps_chord_bounds() {
        // Coarse octagon approximation of a circle r=5.
        let coarse = MultiPolygon::new(vec![fc_geo::circle(0.0, 0.0, 5.0, 8)]);
        let before = vertex_count(&coarse);
        let dense = densify_rings(&coarse, 0.5);
        let after = vertex_count(&dense);

        assert!(
            after > before + 30,
            "expected many more vertices: before={before} after={after}"
        );

        // The new points lie on the original chords, so the densified bounds
        // equal the COARSE polygon's bounds (NOT the true circle's), within 1e-9.
        let (c0, c1, c2, c3) = fc_geo::bounds(&coarse).unwrap();
        let (d0, d1, d2, d3) = fc_geo::bounds(&dense).unwrap();
        assert!((c0 - d0).abs() < 1e-9, "minx {c0} vs {d0}");
        assert!((c1 - d1).abs() < 1e-9, "miny {c1} vs {d1}");
        assert!((c2 - d2).abs() < 1e-9, "maxx {c2} vs {d2}");
        assert!((c3 - d3).abs() < 1e-9, "maxy {c3} vs {d3}");
    }

    #[test]
    fn nonpositive_max_seg_len_is_noop() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let before = vertex_count(&mp);

        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let out = densify_rings(&mp, bad);
            assert_eq!(
                vertex_count(&out),
                before,
                "max_seg_len={bad} should leave vertex count unchanged"
            );
        }
    }

    #[test]
    fn densify_for_beam_subdivides_with_elongated_beam() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let before = vertex_count(&mp);
        // Short extent = 0.1; fraction 0.25 -> max_seg_len = 0.025, so each
        // 10-long side -> ~400 segments.
        let beam = BeamShape {
            width_x: 0.1,
            width_y: 0.5,
            angle_deg: 0.0,
        };
        let dense = densify_for_beam(&mp, &beam, 0.25);
        assert!(
            vertex_count(&dense) > before + 100,
            "elongated-beam densify should add many vertices: before={before} after={}",
            vertex_count(&dense)
        );
    }
}
