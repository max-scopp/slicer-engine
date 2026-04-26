//! Slicing parameters: per-print and per-object settings.

use serde::{Deserialize, Serialize};

/// Parameters that control how a model is sliced and printed.
///
/// All dimensional values are in millimeters; speeds in mm/s;
/// temperatures in °C; infill density as a fraction 0.0–1.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicingParams {
    /// Layer height in mm (e.g. 0.2).
    pub layer_height: f64,
    /// Wall / perimeter thickness in mm.
    pub wall_thickness: f64,
    /// Infill density as a fraction (0.0 = hollow, 1.0 = solid).
    pub infill_density: f64,
    /// Print speed in mm/s.
    pub print_speed: f64,
    /// Nozzle temperature in °C.
    pub nozzle_temp: f64,
    /// Heated bed temperature in °C.
    pub bed_temp: f64,
}

impl Default for SlicingParams {
    /// Sensible defaults for a standard PLA print.
    fn default() -> Self {
        Self {
            layer_height: 0.2,
            wall_thickness: 1.2,
            infill_density: 0.2,
            print_speed: 60.0,
            nozzle_temp: 210.0,
            bed_temp: 60.0,
        }
    }
}

/// Global (print-level) settings that apply to the entire print job.
///
/// These act as the baseline from which per-object overrides are applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    /// Base slicing parameters for the whole print.
    pub params: SlicingParams,
    /// Preferred G-code firmware flavor (e.g. `"marlin"`, `"klipper"`).
    ///
    /// Used as the default when the `slice` command is invoked without an
    /// explicit `--gcode-flavor` flag.  Must be a valid [`crate::gcode::GcodeFlavor`]
    /// string; defaults to `"marlin"` for new or migrated settings files.
    #[serde(default = "GlobalSettings::default_gcode_flavor")]
    pub gcode_flavor: String,
    /// Emit layer lifecycle markers in G-code output.
    ///
    /// When `true` the generator adds `;LAYER_CHANGE`, `;Z:`, `;HEIGHT:`,
    /// `;BEFORE_LAYER_CHANGE`, `;AFTER_LAYER_CHANGE`, `;TYPE:`, and `;WIDTH:`
    /// annotations compatible with OrcaSlicer / Klipper post-processing hooks.
    /// Defaults to `true` for new or migrated settings files.
    #[serde(default = "GlobalSettings::default_emit_lifecycle_markers")]
    pub emit_lifecycle_markers: bool,
}

impl GlobalSettings {
    fn default_gcode_flavor() -> String {
        "marlin".to_string()
    }

    fn default_emit_lifecycle_markers() -> bool {
        true
    }
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            params: SlicingParams::default(),
            gcode_flavor: Self::default_gcode_flavor(),
            emit_lifecycle_markers: Self::default_emit_lifecycle_markers(),
        }
    }
}

/// Per-object settings that may selectively override the global defaults.
///
/// `overrides` is `None` when no object-level customisation is applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSettings {
    /// Name of the object this settings block applies to.
    pub object_name: String,
    /// Optional parameter overrides for this object.
    /// `None` means the global settings apply without modification.
    pub overrides: Option<SlicingParams>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_settings_round_trip() {
        let gs = GlobalSettings::default();
        let json = serde_json::to_string(&gs).expect("serialize");
        let back: GlobalSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.params.layer_height, gs.params.layer_height);
        assert_eq!(back.params.nozzle_temp, gs.params.nozzle_temp);
        assert_eq!(back.gcode_flavor, gs.gcode_flavor);
        assert_eq!(back.emit_lifecycle_markers, gs.emit_lifecycle_markers);
    }

    #[test]
    fn test_global_settings_default_gcode_flavor_is_marlin() {
        let gs = GlobalSettings::default();
        assert_eq!(gs.gcode_flavor, "marlin");
    }

    #[test]
    fn test_global_settings_default_emit_lifecycle_markers_is_true() {
        let gs = GlobalSettings::default();
        assert!(gs.emit_lifecycle_markers);
    }

    #[test]
    fn test_global_settings_gcode_flavor_defaults_when_absent() {
        // Simulate a legacy settings JSON that doesn't have the gcode_flavor field
        let json = r#"{"params":{"layer_height":0.2,"wall_thickness":1.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0}}"#;
        let back: GlobalSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            back.gcode_flavor, "marlin",
            "should default to marlin for legacy files"
        );
        assert!(
            back.emit_lifecycle_markers,
            "should default to true for legacy files"
        );
    }

    #[test]
    fn test_global_settings_lifecycle_markers_defaults_when_absent() {
        // Simulate a settings JSON without emit_lifecycle_markers field
        let json = r#"{"params":{"layer_height":0.2,"wall_thickness":1.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0},"gcode_flavor":"marlin"}"#;
        let back: GlobalSettings = serde_json::from_str(json).expect("deserialize");
        assert!(
            back.emit_lifecycle_markers,
            "should default to true when absent"
        );
    }

    #[test]
    fn test_object_settings_with_overrides_round_trip() {
        let os = ObjectSettings {
            object_name: "part_a".to_string(),
            overrides: Some(SlicingParams {
                layer_height: 0.1,
                ..SlicingParams::default()
            }),
        };
        let json = serde_json::to_string(&os).expect("serialize");
        let back: ObjectSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.object_name, "part_a");
        assert_eq!(back.overrides.unwrap().layer_height, 0.1);
    }

    #[test]
    fn test_object_settings_without_overrides_round_trip() {
        let os = ObjectSettings {
            object_name: "part_b".to_string(),
            overrides: None,
        };
        let json = serde_json::to_string(&os).expect("serialize");
        let back: ObjectSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(back.overrides.is_none());
    }
}
