//! `preferences` — application-level default settings for the FlatCAM Rust port.
//!
//! Rust analogue of FlatCAM's `factory_defaults` / preferences layer
//! (`defaults.py` + `appGUI/preferences/`). Where [`crate::Project`] holds the
//! per-project object collection, [`Preferences`] holds the app-wide defaults
//! that seed new objects and CAM operations (units, default tool diameter,
//! cut/travel depths, feed rates, spindle speed, preprocessor, isolation
//! parameters).
//!
//! Persistence mirrors [`crate::Project`]: JSON via `serde`, with `to_json` /
//! `from_json` / `save` / `load`, mapping errors to [`crate::AppError`].

use serde::{Deserialize, Serialize};
use std::path::Path;

/// App-wide default settings (the `factory_defaults` analogue).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    /// Working units: "mm" or "in".
    pub units: String,
    /// Default tool diameter.
    pub default_tool_dia: f64,
    /// Default cutting depth (negative = into the material).
    pub default_cut_z: f64,
    /// Default travel (clearance) height.
    pub default_travel_z: f64,
    /// Default XY feed rate.
    pub default_feed_xy: f64,
    /// Default Z (plunge) feed rate.
    pub default_feed_z: f64,
    /// Default spindle speed.
    pub default_spindle: f64,
    /// Default preprocessor (G-code dialect) name.
    pub default_preproc: String,
    /// Default number of isolation passes.
    pub iso_passes: u32,
    /// Default isolation pass overlap (fraction of tool diameter).
    pub iso_overlap: f64,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            units: "mm".into(),
            default_tool_dia: 0.4,
            default_cut_z: -0.05,
            default_travel_z: 2.0,
            default_feed_xy: 120.0,
            default_feed_z: 60.0,
            default_spindle: 1000.0,
            default_preproc: "grbl".into(),
            iso_passes: 1,
            iso_overlap: 0.1,
        }
    }
}

impl Preferences {
    pub fn to_json(&self) -> Result<String, crate::AppError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self, crate::AppError> {
        Ok(serde_json::from_str(s)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), crate::AppError> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, crate::AppError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let p = Preferences::default();
        assert_eq!(p.units, "mm");
        assert_eq!(p.default_tool_dia, 0.4);
        assert_eq!(p.default_cut_z, -0.05);
        assert_eq!(p.default_travel_z, 2.0);
        assert_eq!(p.default_feed_xy, 120.0);
        assert_eq!(p.default_feed_z, 60.0);
        assert_eq!(p.default_spindle, 1000.0);
        assert_eq!(p.default_preproc, "grbl");
        assert_eq!(p.iso_passes, 1);
        assert_eq!(p.iso_overlap, 0.1);
    }

    #[test]
    fn default_roundtrip() {
        let p = Preferences::default();
        let json = p.to_json().unwrap();
        let back = Preferences::from_json(&json).unwrap();
        assert_eq!(p, back);
        assert!(json.contains("units"));
    }

    #[test]
    fn tweaked_roundtrip() {
        let p = Preferences {
            units: "in".into(),
            default_tool_dia: 0.0625,
            default_cut_z: -0.1,
            default_travel_z: 0.1,
            default_feed_xy: 30.0,
            default_feed_z: 15.0,
            default_spindle: 12000.0,
            default_preproc: "marlin".into(),
            iso_passes: 3,
            iso_overlap: 0.25,
        };
        let json = p.to_json().unwrap();
        let back = Preferences::from_json(&json).unwrap();
        assert_eq!(p, back);
    }
}
