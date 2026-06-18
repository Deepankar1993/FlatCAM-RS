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
