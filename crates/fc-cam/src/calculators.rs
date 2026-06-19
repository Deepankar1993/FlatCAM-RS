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
    let denom = 2.0 * half_angle_rad.tan();
    // Degenerate angles (0/180/360°) give a zero/non-finite slope -> no depth.
    if denom.abs() < 1e-12 || !denom.is_finite() {
        return 0.0;
    }
    (width - tip_dia) / denom
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

// ---------------------------------------------------------------------------
// Copper-weight helpers
// ---------------------------------------------------------------------------

/// Copper-foil thickness, in micrometres, for a given foil "weight" in ounces
/// per square foot. The industry definition is 1 oz/ft² ≈ 34.79 µm (35 µm is
/// the common rounded value used on datasheets).
pub const UM_PER_OZ: f64 = 34.79;

/// Convert copper weight (oz/ft²) to a foil thickness in micrometres.
pub fn copper_oz_to_um(oz: f64) -> f64 {
    oz * UM_PER_OZ
}

/// Convert a copper-foil thickness in micrometres to a weight in oz/ft².
pub fn copper_um_to_oz(um: f64) -> f64 {
    if UM_PER_OZ == 0.0 {
        return 0.0;
    }
    um / UM_PER_OZ
}

// ---------------------------------------------------------------------------
// Track (trace) resistance
// ---------------------------------------------------------------------------

/// Bulk electrical resistivity of copper at 20 °C, in ohm-metres.
pub const RHO_CU_OHM_M: f64 = 1.68e-8;

/// Temperature coefficient of resistance for copper, per °C.
pub const ALPHA_CU_PER_C: f64 = 0.00393;

/// DC resistance of a rectangular copper track, in ohms.
///
/// `R = rho * L / A`, where the cross-section `A = width · thickness`. All
/// dimensional inputs are in millimetres; resistivity is converted internally.
///
/// * `length_mm`    — track length along the current path.
/// * `width_mm`     — track width.
/// * `thickness_um` — copper thickness in micrometres (e.g. 35 µm = 1 oz).
///
/// Returns 0 for non-positive cross-sections.
pub fn track_resistance_ohms(length_mm: f64, width_mm: f64, thickness_um: f64) -> f64 {
    let width_m = width_mm * 1e-3;
    let thick_m = thickness_um * 1e-6;
    let area_m2 = width_m * thick_m;
    if area_m2 <= 0.0 || length_mm <= 0.0 {
        return 0.0;
    }
    let length_m = length_mm * 1e-3;
    RHO_CU_OHM_M * length_m / area_m2
}

/// Track resistance adjusted for an operating temperature other than 20 °C,
/// using copper's linear temperature coefficient:
/// `R(T) = R20 · (1 + alpha · (T − 20))`.
pub fn track_resistance_ohms_at(
    length_mm: f64,
    width_mm: f64,
    thickness_um: f64,
    temp_c: f64,
) -> f64 {
    let r20 = track_resistance_ohms(length_mm, width_mm, thickness_um);
    r20 * (1.0 + ALPHA_CU_PER_C * (temp_c - 20.0))
}

// ---------------------------------------------------------------------------
// IPC-2221 trace current / width
// ---------------------------------------------------------------------------

// IPC-2221 constants for the empirical `I = k · ΔT^0.44 · A^0.725` relation,
// where `A` is the cross-section in mil² and `I` is in amperes.
const IPC_K_EXTERNAL: f64 = 0.048;
const IPC_K_INTERNAL: f64 = 0.024;
const IPC_DT_EXP: f64 = 0.44;
const IPC_AREA_EXP: f64 = 0.725;

/// Copper cross-section area (in mil²) for a width and thickness.
/// `width_mm` is the trace width; `thickness_um` is the copper thickness.
fn area_mil2(width_mm: f64, thickness_um: f64) -> f64 {
    let width_mil = width_mm / MM_PER_INCH * 1000.0;
    let thick_mil = (thickness_um * 1e-3) / MM_PER_INCH * 1000.0;
    width_mil * thick_mil
}

/// Maximum continuous current (amperes) a trace can carry for a given allowed
/// temperature rise, per IPC-2221:
///
/// `I = k · ΔT^0.44 · A^0.725`, with `A` the copper cross-section in mil².
///
/// `external` selects the surface-layer constant (`k = 0.048`) vs an inner
/// layer (`k = 0.024`). Inputs in mm / µm / °C.
pub fn trace_current_capacity_a(
    width_mm: f64,
    thickness_um: f64,
    temp_rise_c: f64,
    external: bool,
) -> f64 {
    let area = area_mil2(width_mm, thickness_um);
    if area <= 0.0 || temp_rise_c <= 0.0 {
        return 0.0;
    }
    let k = if external { IPC_K_EXTERNAL } else { IPC_K_INTERNAL };
    k * temp_rise_c.powf(IPC_DT_EXP) * area.powf(IPC_AREA_EXP)
}

/// Minimum trace width (mm) required to carry `current_a` within an allowed
/// temperature rise, per IPC-2221 — the inverse of [`trace_current_capacity_a`].
///
/// Solve `I = k·ΔT^0.44·A^0.725` for the cross-section `A`, then divide by the
/// copper thickness to obtain the width. Inputs in A / µm / °C; result in mm.
pub fn trace_width_required_mm(
    current_a: f64,
    thickness_um: f64,
    temp_rise_c: f64,
    external: bool,
) -> f64 {
    if current_a <= 0.0 || thickness_um <= 0.0 || temp_rise_c <= 0.0 {
        return 0.0;
    }
    let k = if external { IPC_K_EXTERNAL } else { IPC_K_INTERNAL };
    // A = (I / (k·ΔT^0.44))^(1/0.725), in mil².
    let area_mil2 = (current_a / (k * temp_rise_c.powf(IPC_DT_EXP))).powf(1.0 / IPC_AREA_EXP);
    // width_mil = A / thickness_mil.
    let thick_mil = (thickness_um * 1e-3) / MM_PER_INCH * 1000.0;
    if thick_mil <= 0.0 {
        return 0.0;
    }
    let width_mil = area_mil2 / thick_mil;
    width_mil / 1000.0 * MM_PER_INCH
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

    #[test]
    fn copper_weight_round_trip() {
        assert!((copper_oz_to_um(1.0) - 34.79).abs() < 1e-9);
        assert!((copper_um_to_oz(34.79) - 1.0).abs() < 1e-9);
        let v = 2.0;
        assert!((copper_um_to_oz(copper_oz_to_um(v)) - v).abs() < 1e-12);
    }

    #[test]
    fn track_resistance_known_value() {
        // 100 mm long, 1 mm wide, 35 µm thick copper.
        // A = 1e-3 m * 35e-6 m = 3.5e-8 m². L = 0.1 m.
        // R = 1.68e-8 * 0.1 / 3.5e-8 = 0.048 ohm.
        let r = track_resistance_ohms(100.0, 1.0, 35.0);
        assert!((r - 0.048).abs() < 1e-4, "R was {r}");
    }

    #[test]
    fn track_resistance_scales_inverse_with_width() {
        let r1 = track_resistance_ohms(100.0, 1.0, 35.0);
        let r2 = track_resistance_ohms(100.0, 2.0, 35.0);
        assert!((r1 / r2 - 2.0).abs() < 1e-9, "double width => half resistance");
    }

    #[test]
    fn track_resistance_temperature_increases() {
        let r20 = track_resistance_ohms(100.0, 1.0, 35.0);
        let r70 = track_resistance_ohms_at(100.0, 1.0, 35.0, 70.0);
        // +50 °C => +0.00393*50 = +19.65%.
        assert!((r70 / r20 - 1.19650).abs() < 1e-4, "ratio {}", r70 / r20);
    }

    #[test]
    fn track_resistance_guards() {
        assert_eq!(track_resistance_ohms(0.0, 1.0, 35.0), 0.0);
        assert_eq!(track_resistance_ohms(100.0, 0.0, 35.0), 0.0);
    }

    #[test]
    fn ipc_trace_current_known_value() {
        // 100 mil wide, 1 oz (≈34.79 µm) external trace, 10 °C rise.
        // thickness ≈ 1.37 mil => A ≈ 137 mil².
        // I = 0.048 * 10^0.44 * 137^0.725 ≈ 0.048 * 2.754 * 34.9 ≈ 4.6 A.
        let i = trace_current_capacity_a(2.54, 34.79, 10.0, true);
        assert!(i > 3.5 && i < 6.0, "current capacity {i}");
    }

    #[test]
    fn ipc_internal_carries_less_than_external() {
        let ext = trace_current_capacity_a(2.54, 34.79, 10.0, true);
        let int = trace_current_capacity_a(2.54, 34.79, 10.0, false);
        assert!(int < ext, "internal {int} < external {ext}");
        // Internal constant is exactly half the external one.
        assert!((int / ext - 0.5).abs() < 1e-9);
    }

    #[test]
    fn ipc_width_inverts_current() {
        // The required-width solver must invert the capacity formula.
        let width = 1.5;
        let i = trace_current_capacity_a(width, 35.0, 20.0, true);
        let back = trace_width_required_mm(i, 35.0, 20.0, true);
        assert!((back - width).abs() < 1e-6, "round-trip width {back} vs {width}");
    }

    #[test]
    fn ipc_higher_current_needs_wider_trace() {
        let w1 = trace_width_required_mm(1.0, 35.0, 10.0, true);
        let w2 = trace_width_required_mm(5.0, 35.0, 10.0, true);
        assert!(w2 > w1, "more current => wider trace");
    }

    #[test]
    fn ipc_guards() {
        assert_eq!(trace_current_capacity_a(0.0, 35.0, 10.0, true), 0.0);
        assert_eq!(trace_width_required_mm(0.0, 35.0, 10.0, true), 0.0);
    }
}
