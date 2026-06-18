//! `fc-laser` — beam-shape modelling and compensation for low-cost diode lasers.
//!
//! Diode laser spots are elliptical, not circular, which makes both the cut
//! kerf and the burn intensity depend on the travel direction. This crate
//! models the spot ([`BeamShape`]) and provides the three compensations:
//!
//! * power compensation (equalise fluence across directions) — [`beam`]
//! * anisotropic (elliptical) geometric offset for kerf — [`offset`]
//! * burn simulation + fill-angle optimisation for the visual plugin —
//!   [`simulate`], [`optimize`]
//!
//! plus per-segment laser G-code emission — [`emit`].

pub mod beam;
pub use beam::{segment_angle, BeamShape};
pub mod offset;
pub use offset::anisotropic_offset;
pub mod simulate;
pub use simulate::{simulate, BurnMap};
pub mod optimize;
pub use optimize::{burn_uniformity, optimal_fill_angle};
pub mod emit;
pub use emit::{compensate_power, laser_gcode};
pub mod cam;
pub use cam::laser_isolation;
pub mod astig;
pub use astig::AstigmaticBeam;
pub mod calibration;
pub use calibration::CalParams;
pub mod calfit;
pub use calfit::{fit_astig, KerfMeasurement};
pub mod powercurve;
pub use powercurve::PowerCurve;
pub mod crosshatch;
pub use crosshatch::{crosshatch_fill, crosshatch_for_beam, crosshatch_orthogonal};
pub mod banding;
pub use banding::{apply_scan_offset, compensate_banding, overscan, scan_offset_distance};
pub mod densify;
pub use densify::{densify_for_beam, densify_rings};
pub mod polar;
pub use polar::{polar_samples, PolarSample};
