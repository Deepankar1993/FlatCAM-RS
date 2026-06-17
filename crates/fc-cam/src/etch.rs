//! Etch compensation (port of `ToolEtchCompensation`'s core).
//!
//! Chemical etching laterally undercuts the copper: the etchant eats sideways
//! under the resist, so a trace drawn at its nominal width finishes narrower
//! than intended. To compensate, the copper geometry is grown outward by a
//! lateral `factor` before etching so that, once the undercut is removed, the
//! remaining copper lands at the nominal size.

use fc_geo::{offset, MultiPolygon};

/// Parameters for etch compensation.
#[derive(Clone, Debug)]
pub struct EtchParams {
    /// Lateral compensation in document units. A positive value widens the
    /// copper to counteract the etchant's sideways undercut.
    pub factor: f64,
}

impl Default for EtchParams {
    fn default() -> Self {
        EtchParams { factor: 0.0 }
    }
}

/// Apply etch compensation to a copper geometry.
///
/// Grows (or, for a negative `factor`, shrinks) every copper feature laterally
/// by [`EtchParams::factor`] so the etched result finishes at nominal size.
pub fn compensate(copper: &MultiPolygon<f64>, p: &EtchParams) -> MultiPolygon<f64> {
    offset(copper, p.factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, MultiPolygon};

    fn square_2x2() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)])
    }

    #[test]
    fn positive_factor_widens_copper() {
        let copper = square_2x2();
        let before = area(&copper);
        assert!((before - 4.0).abs() < 1e-9, "2x2 square has area 4");

        let widened = compensate(&copper, &EtchParams { factor: 0.2 });
        let after = area(&widened);
        assert!(
            after > before,
            "positive factor should grow area: {} !> {}",
            after,
            before
        );
    }

    #[test]
    fn zero_factor_leaves_area_unchanged() {
        let copper = square_2x2();
        let before = area(&copper);
        let same = compensate(&copper, &EtchParams { factor: 0.0 });
        let after = area(&same);
        assert!(
            (after - before).abs() < 1e-9,
            "factor 0.0 must not change area: {} vs {}",
            after,
            before
        );
    }
}
