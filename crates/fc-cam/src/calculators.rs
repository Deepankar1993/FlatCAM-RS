//! PCB calculators — port of FlatCAM's `ToolCalculators`.
//!
//! Pure scalar math with no geometry dependencies: unit conversion, V-bit
//! cut-width geometry, and an electroplating time estimate.

/// Millimetres per inch (exact, by definition).
pub const MM_PER_INCH: f64 = 25.4;

/// Convert a length in millimetres to inches.
pub fn mm_to_inch(mm: f64) -> f64 {
    mm / MM_PER_INCH
}

/// Convert a length in inches to millimetres.
pub fn inch_to_mm(inch: f64) -> f64 {
    inch * MM_PER_INCH
}

/// Width of the cut produced by a V-shaped bit at a given cut depth.
///
/// A V-bit has a flat tip of diameter `tip_dia` and flanks that open at the
/// full included `angle_deg`. Cutting to `depth` exposes a trapezoidal cross
/// section whose top width grows by `depth * tan(angle/2)` on each side:
///
/// `width = tip_dia + 2 * depth * tan((angle/2) in radians)`
pub fn v_bit_cut_width(depth: f64, tip_dia: f64, angle_deg: f64) -> f64 {
    let half_angle_rad = (angle_deg / 2.0).to_radians();
    tip_dia + 2.0 * depth * half_angle_rad.tan()
}

/// Inverse of [`v_bit_cut_width`]: the depth required to achieve `width`.
///
/// `depth = (width - tip_dia) / (2 * tan((angle/2) in radians))`
pub fn v_bit_depth_for_width(width: f64, tip_dia: f64, angle_deg: f64) -> f64 {
    let half_angle_rad = (angle_deg / 2.0).to_radians();
    (width - tip_dia) / (2.0 * half_angle_rad.tan())
}

/// Estimate electroplating time, in minutes, to deposit a copper layer of a
/// given thickness over a given area at a given current density.
///
/// Physical model (copper electrodeposition, Faraday's law):
///
/// - The plated mass is `m = rho * area * thickness` where `rho` is copper's
///   density.
/// - Faraday's law: `m = (I * t * M) / (n * F)`, so the time is
///   `t = (m * n * F) / (I * M)`.
/// - The applied current is `I = J * area`, where `J` is the current density.
///   The `area` therefore cancels, leaving a thickness-driven estimate.
///
/// Constants used:
/// - `RHO_CU   = 8.96 g/cm^3`   (density of copper)
/// - `M_CU     = 63.546 g/mol`  (molar mass of copper)
/// - `N_CU     = 2`             (electrons per Cu^2+ ion)
/// - `F        = 96485 C/mol`   (Faraday constant)
///
/// Inputs are in PCB-friendly units: `area_cm2` in cm^2,
/// `current_density_a_dm2` in A/dm^2, `thickness_um` in micrometres.
/// (Area cancels but is kept in the signature to match the tool's UI.)
pub fn electroplating_time_min(
    area_cm2: f64,
    current_density_a_dm2: f64,
    thickness_um: f64,
) -> f64 {
    const RHO_CU: f64 = 8.96; // g/cm^3
    const M_CU: f64 = 63.546; // g/mol
    const N_CU: f64 = 2.0; // electrons per ion
    const F: f64 = 96485.0; // C/mol

    if current_density_a_dm2 <= 0.0 || area_cm2 <= 0.0 {
        return 0.0;
    }

    // Convert thickness from micrometres to centimetres.
    let thickness_cm = thickness_um * 1e-4;
    // Deposited mass (g): rho * area * thickness.
    let mass_g = RHO_CU * area_cm2 * thickness_cm;

    // Current (A): density is A/dm^2; area is cm^2 = 1e-2 dm^2.
    let area_dm2 = area_cm2 * 1e-2;
    let current_a = current_density_a_dm2 * area_dm2;

    // Faraday's law: t (seconds) = m * n * F / (I * M).
    let time_s = mass_g * N_CU * F / (current_a * M_CU);
    time_s / 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mm_inch_round_trip() {
        assert!((inch_to_mm(1.0) - 25.4).abs() < 1e-12);
        assert!((mm_to_inch(25.4) - 1.0).abs() < 1e-12);
        let v = 3.7;
        assert!((mm_to_inch(inch_to_mm(v)) - v).abs() < 1e-12);
    }

    #[test]
    fn cut_width_at_zero_depth_is_tip() {
        let w = v_bit_cut_width(0.0, 0.2, 30.0);
        assert!((w - 0.2).abs() < 1e-12);
    }

    #[test]
    fn cut_width_increases_with_depth() {
        let shallow = v_bit_cut_width(0.05, 0.2, 30.0);
        let deep = v_bit_cut_width(0.20, 0.2, 30.0);
        assert!(deep > shallow);
    }

    #[test]
    fn depth_inverts_cut_width() {
        let tip = 0.1;
        let angle = 45.0;
        let depth = 0.123;
        let width = v_bit_cut_width(depth, tip, angle);
        let back = v_bit_depth_for_width(width, tip, angle);
        assert!((back - depth).abs() < 1e-9);
    }

    #[test]
    fn electroplating_time_is_positive_and_scales_with_thickness() {
        let t1 = electroplating_time_min(100.0, 2.0, 35.0);
        let t2 = electroplating_time_min(100.0, 2.0, 70.0);
        assert!(t1 > 0.0);
        // Double the thickness => double the time.
        assert!((t2 / t1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn electroplating_guards_against_nonsense() {
        assert_eq!(electroplating_time_min(0.0, 2.0, 35.0), 0.0);
        assert_eq!(electroplating_time_min(100.0, 0.0, 35.0), 0.0);
    }
}
