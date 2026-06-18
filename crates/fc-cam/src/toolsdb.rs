//! In-memory tool presets (tools database).
//!
//! Provides a small set of built-in [`ToolPreset`] definitions for common
//! milling bits, drill sizes, and V-bits. No serialization or external
//! dependencies; everything is hard-coded so callers always have sane defaults.

/// The mechanical category of a tool.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolType {
    /// A flat end-mill / router bit used for isolation routing and cutouts.
    Milling,
    /// A twist drill used for through-holes.
    Drilling,
    /// A conical V-shaped engraving bit.
    Vbit,
}

/// A single tool preset entry.
#[derive(Clone, Debug)]
pub struct ToolPreset {
    /// Human-readable name of the tool.
    pub name: String,
    /// Cutting diameter in millimeters (for V-bits this is the tip diameter).
    pub diameter: f64,
    /// The kind of tool.
    pub kind: ToolType,
    /// Suggested XY feed rate (mm/min).
    pub feed_xy: f64,
    /// Suggested Z (plunge) feed rate (mm/min).
    pub feed_z: f64,
    /// Suggested spindle speed (RPM).
    pub spindle: f64,
}

/// Built-in milling tool presets, ordered by ascending diameter (mm).
pub fn default_milling_tools() -> Vec<ToolPreset> {
    vec![
        ToolPreset {
            name: "Mill 0.10 mm".to_string(),
            diameter: 0.1,
            kind: ToolType::Milling,
            feed_xy: 60.0,
            feed_z: 30.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "Mill 0.20 mm".to_string(),
            diameter: 0.2,
            kind: ToolType::Milling,
            feed_xy: 90.0,
            feed_z: 40.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "Mill 0.40 mm".to_string(),
            diameter: 0.4,
            kind: ToolType::Milling,
            feed_xy: 150.0,
            feed_z: 60.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "Mill 1.00 mm".to_string(),
            diameter: 1.0,
            kind: ToolType::Milling,
            feed_xy: 300.0,
            feed_z: 100.0,
            spindle: 10000.0,
        },
        ToolPreset {
            name: "Mill 2.00 mm".to_string(),
            diameter: 2.0,
            kind: ToolType::Milling,
            feed_xy: 400.0,
            feed_z: 120.0,
            spindle: 9000.0,
        },
    ]
}

/// Built-in drill tool presets, ordered by ascending diameter (mm).
pub fn default_drill_tools() -> Vec<ToolPreset> {
    vec![
        ToolPreset {
            name: "Drill 0.60 mm".to_string(),
            diameter: 0.6,
            kind: ToolType::Drilling,
            feed_xy: 0.0,
            feed_z: 50.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "Drill 0.80 mm".to_string(),
            diameter: 0.8,
            kind: ToolType::Drilling,
            feed_xy: 0.0,
            feed_z: 60.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "Drill 1.00 mm".to_string(),
            diameter: 1.0,
            kind: ToolType::Drilling,
            feed_xy: 0.0,
            feed_z: 70.0,
            spindle: 11000.0,
        },
        ToolPreset {
            name: "Drill 1.20 mm".to_string(),
            diameter: 1.2,
            kind: ToolType::Drilling,
            feed_xy: 0.0,
            feed_z: 80.0,
            spindle: 11000.0,
        },
    ]
}

/// Built-in V-bit presets. The tip diameter and included angle are encoded in
/// the name (e.g. "V-bit 0.10 mm tip 30deg").
pub fn default_vbits() -> Vec<ToolPreset> {
    vec![
        ToolPreset {
            name: "V-bit 0.10 mm tip 30deg".to_string(),
            diameter: 0.1,
            kind: ToolType::Vbit,
            feed_xy: 120.0,
            feed_z: 40.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "V-bit 0.20 mm tip 60deg".to_string(),
            diameter: 0.2,
            kind: ToolType::Vbit,
            feed_xy: 150.0,
            feed_z: 50.0,
            spindle: 12000.0,
        },
        ToolPreset {
            name: "V-bit 0.30 mm tip 90deg".to_string(),
            diameter: 0.3,
            kind: ToolType::Vbit,
            feed_xy: 180.0,
            feed_z: 60.0,
            spindle: 11000.0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milling_tools_non_empty_and_typed() {
        let tools = default_milling_tools();
        assert!(!tools.is_empty());
        for t in &tools {
            assert_eq!(t.kind, ToolType::Milling);
            assert!(t.diameter > 0.0);
        }
    }

    #[test]
    fn milling_tools_sorted_ascending() {
        let tools = default_milling_tools();
        for w in tools.windows(2) {
            assert!(w[0].diameter <= w[1].diameter);
        }
    }

    #[test]
    fn drill_tools_non_empty_and_typed() {
        let tools = default_drill_tools();
        assert!(!tools.is_empty());
        for t in &tools {
            assert_eq!(t.kind, ToolType::Drilling);
            assert!(t.diameter > 0.0);
        }
    }

    #[test]
    fn drill_tools_sorted_ascending() {
        let tools = default_drill_tools();
        for w in tools.windows(2) {
            assert!(w[0].diameter <= w[1].diameter);
        }
    }

    #[test]
    fn vbits_non_empty_and_typed() {
        let tools = default_vbits();
        assert!(!tools.is_empty());
        for t in &tools {
            assert_eq!(t.kind, ToolType::Vbit);
            assert!(t.diameter > 0.0);
        }
    }

    #[test]
    fn vbit_name_encodes_angle() {
        let tools = default_vbits();
        assert!(tools.iter().any(|t| t.name.contains("30deg")));
    }
}
