//! Printer hardware profile definition.

use serde::{Deserialize, Serialize};

/// Physical constraints and capabilities of a 3D printer.
///
/// Used to validate that slicing parameters are compatible with the
/// hardware. All dimensional values are in millimeters; speeds in mm/s;
/// acceleration in mm/s².
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterProfile {
    /// Human-readable name for this printer profile.
    pub name: String,
    /// Nozzle diameter in mm (e.g. 0.4).
    pub nozzle_diameter: f64,
    /// Minimum supported layer height in mm.
    pub min_layer_height: f64,
    /// Maximum supported layer height in mm.
    /// Typically ≤ 0.8 × nozzle_diameter.
    pub max_layer_height: f64,
    /// Maximum recommended print speed in mm/s.
    pub max_print_speed: f64,
    /// Maximum supported acceleration in mm/s².
    pub max_acceleration: f64,
}

impl Default for PrinterProfile {
    /// Standard 0.4 mm nozzle FDM printer defaults.
    fn default() -> Self {
        Self {
            name: "Default 0.4mm Nozzle".to_string(),
            nozzle_diameter: 0.4,
            min_layer_height: 0.1,
            max_layer_height: 0.3,
            max_print_speed: 150.0,
            max_acceleration: 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile() {
        let profile = PrinterProfile::default();
        assert_eq!(profile.nozzle_diameter, 0.4);
        assert_eq!(profile.min_layer_height, 0.1);
        assert_eq!(profile.max_layer_height, 0.3);
        assert_eq!(profile.max_print_speed, 150.0);
        assert_eq!(profile.max_acceleration, 1000.0);
    }

    #[test]
    fn test_serialize_round_trip() {
        let profile = PrinterProfile::default();
        let json = serde_json::to_string(&profile).expect("serialize");
        let back: PrinterProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.nozzle_diameter, profile.nozzle_diameter);
        assert_eq!(back.max_layer_height, profile.max_layer_height);
        assert_eq!(back.name, profile.name);
    }
}
