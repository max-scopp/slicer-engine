//! Slicing parameters: per-print and per-object settings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters that control how a model is sliced and printed.
///
/// All dimensional values are in millimeters; speeds in mm/s;
/// temperatures in °C; infill density as a fraction 0.0–1.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicingParams {
    /// Layer height in mm (e.g. 0.2).
    pub layer_height: f64,
    /// Maximum number of perimeter (wall) beads per layer.
    ///
    /// Arachne will place up to this many concentric wall paths around each
    /// shell polygon.  The innermost bead may have a variable extrusion width
    /// when the remaining space is thinner than the nozzle diameter.
    #[serde(default = "SlicingParams::default_wall_count")]
    pub wall_count: usize,
    /// Minimum allowed bead width as a fraction of the nozzle diameter.
    ///
    /// Beads narrower than `wall_line_width_min × nozzle_diameter_mm` are
    /// skipped entirely.  Typical range: 0.5–1.0.
    #[serde(default = "SlicingParams::default_wall_line_width_min")]
    pub wall_line_width_min: f64,
    /// Maximum allowed bead width as a fraction of the nozzle diameter.
    ///
    /// Variable-width beads are capped at `wall_line_width_max × nozzle_diameter_mm`
    /// to avoid excessive over-extrusion at corners.  Typical range: 1.0–2.0.
    #[serde(default = "SlicingParams::default_wall_line_width_max")]
    pub wall_line_width_max: f64,
    /// Minimum wall thickness (as a fraction of nozzle diameter) before the
    /// bead count transitions down by one.
    ///
    /// When the remaining space is narrower than
    /// `wall_transition_threshold × nozzle_diameter_mm`, the algorithm avoids
    /// adding another bead and instead widens the existing innermost bead.
    #[serde(default = "SlicingParams::default_wall_transition_threshold")]
    pub wall_transition_threshold: f64,
    /// Length over which a bead-count transition is smoothed (mm).
    ///
    /// Larger values produce a longer gradual width ramp at bead-count changes;
    /// smaller values produce abrupt transitions.
    #[serde(default = "SlicingParams::default_wall_transition_length")]
    pub wall_transition_length: f64,
    /// Number of inner walls that absorb width variation.
    ///
    /// When the remaining inner space is too narrow for a separate bead,
    /// up to `wall_distribution_count` of the innermost beads are widened
    /// proportionally to fill the gap.
    #[serde(default = "SlicingParams::default_wall_distribution_count")]
    pub wall_distribution_count: usize,
    /// Infill density as a fraction (0.0 = hollow, 1.0 = solid).
    pub infill_density: f64,
    /// Infill pattern type (rectilinear, grid, honeycomb, gyroid).
    /// Defaults to "rectilinear" if not specified.
    #[serde(default = "SlicingParams::default_infill_pattern")]
    pub infill_pattern: String,
    /// Base angle in degrees for sparse infill lines (default 45°).
    /// Alternating layers rotate +90° on top of this base angle.
    #[serde(default = "SlicingParams::default_infill_base_angle")]
    pub infill_base_angle: f64,
    /// Print speed in mm/s.
    pub print_speed: f64,
    /// Nozzle temperature in °C.
    pub nozzle_temp: f64,
    /// Heated bed temperature in °C.
    pub bed_temp: f64,
    /// Number of solid top layers (horizontal surfaces facing up).
    #[serde(default = "SlicingParams::default_top_layers")]
    pub top_layers: usize,
    /// Number of solid bottom layers (horizontal surfaces facing down).
    #[serde(default = "SlicingParams::default_bottom_layers")]
    pub bottom_layers: usize,
    /// Angle in degrees for top/bottom surface infill lines (e.g. 45 for diagonal).
    #[serde(default = "SlicingParams::default_surface_infill_angle")]
    pub surface_infill_angle: f64,
    /// Filament diameter in mm (e.g. 1.75 for standard PLA/PETG).
    #[serde(default = "SlicingParams::default_filament_diameter_mm")]
    pub filament_diameter_mm: f64,
    /// Nozzle diameter in mm (e.g. 0.4 for a standard 0.4 mm nozzle).
    #[serde(default = "SlicingParams::default_nozzle_diameter_mm")]
    pub nozzle_diameter_mm: f64,
    /// Non-print (travel) speed in mm/min (e.g. 9000 = 150 mm/s).
    #[serde(default = "SlicingParams::default_travel_speed_mm_min")]
    pub travel_speed_mm_min: f64,
    /// Z-hop height in mm applied during travel moves to avoid stringing.
    #[serde(default = "SlicingParams::default_z_hop_mm")]
    pub z_hop_mm: f64,
    /// Retraction distance in mm on travel moves.
    #[serde(default = "SlicingParams::default_retract_mm")]
    pub retract_mm: f64,
    /// Use only outer wall on the last layer of top surfaces.
    ///
    /// When true, the final layer of top surface regions will only print the
    /// outermost perimeter, creating a cleaner top finish with less chance of
    /// pillowing or visible infill patterns showing through.
    #[serde(default = "SlicingParams::default_only_one_wall_top")]
    pub only_one_wall_top: bool,
    /// Use only outer wall on the first layer (bottom surface or general first layer).
    ///
    /// When true, the first layer uses only the outermost perimeter for better
    /// bed adhesion and to avoid potential issues with multiple perimeters on
    /// the first layer.
    #[serde(default = "SlicingParams::default_only_one_wall_first_layer")]
    pub only_one_wall_first_layer: bool,
    /// Overhang angle threshold in degrees (0-90) for skipping surface generation.
    ///
    /// When the angle between adjacent layers suggests a shallow overhang
    /// (angle < threshold), skip generating solid top/bottom surfaces since
    /// perimeters alone may suffice. Default 0° means always generate surfaces;
    /// higher values (e.g., 45°) skip surfaces on gentle slopes.
    ///
    /// This is a foundation for future overhang support without implementing
    /// full overhang detection yet.
    #[serde(default = "SlicingParams::default_support_threshold_angle")]
    pub support_threshold_angle: f64,
    /// How much solid surfaces overlap into perimeter walls for bonding (0.0-1.0).
    ///
    /// A small overlap (e.g., 0.25 = 25%) ensures surfaces bond well to walls
    /// without gaps, while keeping walls as the dominant structure. Default 0.25.
    #[serde(default = "SlicingParams::default_infill_overlap_percent")]
    pub infill_overlap_percent: f64,
    /// Maximum allowed perpendicular deviation (mm) when simplifying output
    /// paths with the Ramer-Douglas-Peucker algorithm.
    ///
    /// Set to `0.0` to disable simplification entirely.
    /// Typical values: `0.01`–`0.1` mm; default `0.05` mm.
    #[serde(default = "SlicingParams::default_path_tolerance")]
    pub path_tolerance: f64,
}

impl Default for SlicingParams {
    /// Sensible defaults for a standard PLA print.
    fn default() -> Self {
        Self {
            layer_height: 0.2,
            wall_count: Self::default_wall_count(),
            wall_line_width_min: Self::default_wall_line_width_min(),
            wall_line_width_max: Self::default_wall_line_width_max(),
            wall_transition_threshold: Self::default_wall_transition_threshold(),
            wall_transition_length: Self::default_wall_transition_length(),
            wall_distribution_count: Self::default_wall_distribution_count(),
            infill_density: 0.2,
            infill_pattern: Self::default_infill_pattern(),
            infill_base_angle: Self::default_infill_base_angle(),
            print_speed: 60.0,
            nozzle_temp: 210.0,
            bed_temp: 60.0,
            top_layers: Self::default_top_layers(),
            bottom_layers: Self::default_bottom_layers(),
            surface_infill_angle: Self::default_surface_infill_angle(),
            filament_diameter_mm: Self::default_filament_diameter_mm(),
            nozzle_diameter_mm: Self::default_nozzle_diameter_mm(),
            travel_speed_mm_min: Self::default_travel_speed_mm_min(),
            z_hop_mm: Self::default_z_hop_mm(),
            retract_mm: Self::default_retract_mm(),
            only_one_wall_top: Self::default_only_one_wall_top(),
            only_one_wall_first_layer: Self::default_only_one_wall_first_layer(),
            support_threshold_angle: Self::default_support_threshold_angle(),
            infill_overlap_percent: Self::default_infill_overlap_percent(),
            path_tolerance: Self::default_path_tolerance(),
        }
    }
}

impl SlicingParams {
    fn default_wall_count() -> usize {
        3
    }

    fn default_wall_line_width_min() -> f64 {
        0.85
    }

    fn default_wall_line_width_max() -> f64 {
        1.5
    }

    fn default_wall_transition_threshold() -> f64 {
        0.6
    }

    fn default_wall_transition_length() -> f64 {
        1.0
    }

    fn default_wall_distribution_count() -> usize {
        1
    }

    fn default_infill_pattern() -> String {
        "rectilinear".to_string()
    }

    fn default_infill_base_angle() -> f64 {
        45.0
    }

    fn default_top_layers() -> usize {
        3
    }

    fn default_bottom_layers() -> usize {
        3
    }

    fn default_surface_infill_angle() -> f64 {
        45.0
    }

    fn default_filament_diameter_mm() -> f64 {
        1.75
    }

    fn default_nozzle_diameter_mm() -> f64 {
        0.4
    }

    fn default_travel_speed_mm_min() -> f64 {
        9000.0
    }

    fn default_z_hop_mm() -> f64 {
        0.2
    }

    fn default_retract_mm() -> f64 {
        1.0
    }

    fn default_only_one_wall_top() -> bool {
        true // Single wall on top surface layers for cleaner finish
    }

    fn default_only_one_wall_first_layer() -> bool {
        true // Single wall on first layer for better bed adhesion
    }

    fn default_support_threshold_angle() -> f64 {
        45.0 // Skip supports for angles ≤45° (shallow overhangs)
    }

    fn default_infill_overlap_percent() -> f64 {
        0.25 // 25% overlap for good bonding
    }

    fn default_path_tolerance() -> f64 {
        0.05
    }
}

/// Per-flavor lifecycle marker configuration.
///
/// Controls whether lifecycle markers are emitted in G-code output and allows
/// overriding the default marker strings for each supported annotation.
///
/// Template placeholders supported by marker override strings:
/// - `{z}` → current layer Z coordinate (e.g. `0.200`)
/// - `{height}` → layer height (e.g. `0.200`)
/// - `{type}` → extrusion role type name (e.g. `Perimeter`)
/// - `{width}` → default extrusion width for the role (e.g. `0.40`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleMarkerConfig {
    /// Whether to emit lifecycle markers at all. Default: true.
    #[serde(default = "LifecycleMarkerConfig::default_enabled")]
    pub enabled: bool,
    /// Override for `;LAYER_CHANGE`. Supports `{z}` and `{height}` placeholders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_change: Option<String>,
    /// Override for `;Z:{z}`. Supports `{z}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_marker: Option<String>,
    /// Override for `;HEIGHT:{height}`. Supports `{height}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_marker: Option<String>,
    /// Override for `;BEFORE_LAYER_CHANGE`. Supports `{z}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_layer_change: Option<String>,
    /// Override for `;AFTER_LAYER_CHANGE`. Supports `{z}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_layer_change: Option<String>,
    /// Override for `;TYPE:{type}`. Supports `{type}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
    /// Override for `;WIDTH:{width}mm`. Supports `{width}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_annotation: Option<String>,
}

impl Default for LifecycleMarkerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layer_change: None,
            z_marker: None,
            height_marker: None,
            before_layer_change: None,
            after_layer_change: None,
            type_annotation: None,
            width_annotation: None,
        }
    }
}

impl LifecycleMarkerConfig {
    fn default_enabled() -> bool {
        true
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
    /// Optional custom G-code to emit at the start of every print job.
    ///
    /// When set, this replaces the firmware dialect's built-in start script.
    /// The value may be either a newline-separated block of G-code or a path
    /// to a `.gcode` file — resolved at slice time via
    /// [`crate::gcode::resolve_gcode_source`].
    ///
    /// Override precedence: `--start-print-gcode` CLI arg → this field → dialect default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_print_gcode: Option<String>,
    /// Optional custom G-code to emit at the end of every print job.
    ///
    /// When set, this replaces the firmware dialect's built-in end script.
    /// The value may be either a newline-separated block of G-code or a path
    /// to a `.gcode` file — resolved at slice time via
    /// [`crate::gcode::resolve_gcode_source`].
    ///
    /// Override precedence: `--end-print-gcode` CLI arg → this field → dialect default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_print_gcode: Option<String>,
    /// Per-flavor lifecycle marker configuration.
    ///
    /// Keys are lowercase flavor names (e.g. `"marlin"`, `"klipper"`).
    /// Missing flavors inherit the default [`LifecycleMarkerConfig`] (enabled: true,
    /// all markers at defaults).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub lifecycle_markers: HashMap<String, LifecycleMarkerConfig>,
}

impl GlobalSettings {
    fn default_gcode_flavor() -> String {
        "marlin".to_string()
    }
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            params: SlicingParams::default(),
            gcode_flavor: Self::default_gcode_flavor(),
            start_print_gcode: None,
            end_print_gcode: None,
            lifecycle_markers: HashMap::new(),
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
        assert!(back.lifecycle_markers.is_empty());
    }

    #[test]
    fn test_global_settings_default_gcode_flavor_is_marlin() {
        let gs = GlobalSettings::default();
        assert_eq!(gs.gcode_flavor, "marlin");
    }

    #[test]
    fn test_global_settings_default_lifecycle_markers_is_empty() {
        let gs = GlobalSettings::default();
        assert!(gs.lifecycle_markers.is_empty());
    }

    #[test]
    fn test_global_settings_gcode_flavor_defaults_when_absent() {
        // Simulate a legacy settings JSON that doesn't have the gcode_flavor field.
        // `wall_thickness` is an unknown field in the new schema and is silently ignored.
        let json = r#"{"params":{"layer_height":0.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0}}"#;
        let back: GlobalSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            back.gcode_flavor, "marlin",
            "should default to marlin for legacy files"
        );
        assert!(
            back.lifecycle_markers.is_empty(),
            "should default to empty map for legacy files"
        );
    }

    #[test]
    fn test_global_settings_start_end_gcode_default_none() {
        let gs = GlobalSettings::default();
        assert!(gs.start_print_gcode.is_none());
        assert!(gs.end_print_gcode.is_none());
    }

    #[test]
    fn test_global_settings_start_end_gcode_round_trip() {
        let gs = GlobalSettings {
            start_print_gcode: Some("START_PRINT BED_TEMP=60".to_string()),
            end_print_gcode: Some("END_PRINT".to_string()),
            ..GlobalSettings::default()
        };
        let json = serde_json::to_string(&gs).expect("serialize");
        let back: GlobalSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            back.start_print_gcode.as_deref(),
            Some("START_PRINT BED_TEMP=60")
        );
        assert_eq!(back.end_print_gcode.as_deref(), Some("END_PRINT"));
    }

    #[test]
    fn test_global_settings_start_end_gcode_absent_from_legacy_json() {
        // Legacy JSON without start_print_gcode / end_print_gcode should default to None
        let json = r#"{"params":{"layer_height":0.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0},"gcode_flavor":"klipper"}"#;
        let back: GlobalSettings = serde_json::from_str(json).expect("deserialize");
        assert!(back.start_print_gcode.is_none());
        assert!(back.end_print_gcode.is_none());
    }

    #[test]
    fn test_global_settings_none_fields_omitted_in_json() {
        let gs = GlobalSettings::default();
        let json = serde_json::to_string(&gs).expect("serialize");
        // Optional None fields should be omitted (skip_serializing_if)
        assert!(
            !json.contains("start_print_gcode"),
            "None field should be omitted from JSON"
        );
        assert!(
            !json.contains("end_print_gcode"),
            "None field should be omitted from JSON"
        );
        assert!(
            !json.contains("lifecycle_markers"),
            "Empty HashMap should be omitted from JSON"
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

    // ── LifecycleMarkerConfig tests ──────────────────────────────────────────

    #[test]
    fn test_lifecycle_marker_config_default_enabled() {
        let cfg = LifecycleMarkerConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.layer_change.is_none());
        assert!(cfg.z_marker.is_none());
        assert!(cfg.height_marker.is_none());
        assert!(cfg.before_layer_change.is_none());
        assert!(cfg.after_layer_change.is_none());
        assert!(cfg.type_annotation.is_none());
        assert!(cfg.width_annotation.is_none());
    }

    #[test]
    fn test_lifecycle_marker_config_round_trip() {
        let cfg = LifecycleMarkerConfig {
            enabled: false,
            layer_change: Some("LAYER_CHANGE {z}".to_string()),
            z_marker: Some(";Z:{z}".to_string()),
            ..LifecycleMarkerConfig::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: LifecycleMarkerConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(!back.enabled);
        assert_eq!(back.layer_change.as_deref(), Some("LAYER_CHANGE {z}"));
        assert_eq!(back.z_marker.as_deref(), Some(";Z:{z}"));
    }

    #[test]
    fn test_lifecycle_marker_config_defaults_when_absent() {
        let json = r#"{}"#;
        let cfg: LifecycleMarkerConfig = serde_json::from_str(json).expect("deserialize");
        assert!(cfg.enabled, "enabled should default to true when absent");
    }

    #[test]
    fn test_lifecycle_marker_config_none_fields_omitted() {
        let cfg = LifecycleMarkerConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        assert!(!json.contains("layer_change"), "None field omitted");
        assert!(!json.contains("z_marker"), "None field omitted");
    }

    #[test]
    fn test_global_settings_lifecycle_markers_round_trip() {
        let mut gs = GlobalSettings::default();
        gs.lifecycle_markers.insert(
            "klipper".to_string(),
            LifecycleMarkerConfig {
                enabled: true,
                layer_change: Some(";LAYER_CHANGE".to_string()),
                ..LifecycleMarkerConfig::default()
            },
        );
        let json = serde_json::to_string(&gs).expect("serialize");
        let back: GlobalSettings = serde_json::from_str(&json).expect("deserialize");
        let klipper_cfg = back.lifecycle_markers.get("klipper").unwrap();
        assert!(klipper_cfg.enabled);
        assert_eq!(klipper_cfg.layer_change.as_deref(), Some(";LAYER_CHANGE"));
    }

    #[test]
    fn test_slicing_params_top_bottom_layers_defaults() {
        let params = SlicingParams::default();
        assert_eq!(params.top_layers, 3, "Default top layers should be 3");
        assert_eq!(params.bottom_layers, 3, "Default bottom layers should be 3");
        assert_eq!(
            params.surface_infill_angle, 45.0,
            "Default surface infill angle should be 45°"
        );
    }

    #[test]
    fn test_slicing_params_arachne_defaults() {
        let params = SlicingParams::default();
        assert_eq!(params.wall_count, 3, "Default wall count should be 3");
        assert_eq!(
            params.wall_line_width_min, 0.85,
            "Default wall_line_width_min should be 0.85"
        );
        assert_eq!(
            params.wall_line_width_max, 1.5,
            "Default wall_line_width_max should be 1.5"
        );
        assert_eq!(
            params.wall_transition_threshold, 0.6,
            "Default wall_transition_threshold should be 0.6"
        );
        assert_eq!(
            params.wall_transition_length, 1.0,
            "Default wall_transition_length should be 1.0"
        );
        assert_eq!(
            params.wall_distribution_count, 1,
            "Default wall_distribution_count should be 1"
        );
    }

    #[test]
    fn test_slicing_params_arachne_fields_round_trip() {
        let params = SlicingParams {
            wall_count: 5,
            wall_line_width_min: 0.6,
            wall_line_width_max: 2.0,
            wall_transition_threshold: 0.4,
            wall_transition_length: 0.8,
            wall_distribution_count: 2,
            ..SlicingParams::default()
        };
        let json = serde_json::to_string(&params).expect("serialize");
        let back: SlicingParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.wall_count, 5);
        assert_eq!(back.wall_line_width_min, 0.6);
        assert_eq!(back.wall_line_width_max, 2.0);
        assert_eq!(back.wall_transition_threshold, 0.4);
        assert_eq!(back.wall_transition_length, 0.8);
        assert_eq!(back.wall_distribution_count, 2);
    }

    #[test]
    fn test_slicing_params_top_bottom_layers_serialization() {
        let params = SlicingParams {
            top_layers: 5,
            bottom_layers: 4,
            surface_infill_angle: 60.0,
            ..SlicingParams::default()
        };
        let json = serde_json::to_string(&params).expect("serialize");
        let back: SlicingParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.top_layers, 5);
        assert_eq!(back.bottom_layers, 4);
        assert_eq!(back.surface_infill_angle, 60.0);
    }

    #[test]
    fn test_slicing_params_legacy_json_without_surface_layers() {
        // Test that old JSON without top_layers/bottom_layers/surface_infill_angle still deserializes.
        // Unknown fields such as "wall_thickness" from legacy files are silently ignored.
        let json = r#"{"layer_height":0.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0}"#;
        let params: SlicingParams = serde_json::from_str(json).expect("deserialize");
        assert_eq!(params.top_layers, 3, "Should default to 3 for legacy JSON");
        assert_eq!(
            params.bottom_layers, 3,
            "Should default to 3 for legacy JSON"
        );
        assert_eq!(
            params.surface_infill_angle, 45.0,
            "Should default to 45.0 for legacy JSON"
        );
    }

    #[test]
    fn test_slicing_params_hardware_defaults() {
        let params = SlicingParams::default();
        assert_eq!(params.filament_diameter_mm, 1.75);
        assert_eq!(params.nozzle_diameter_mm, 0.4);
        assert_eq!(params.travel_speed_mm_min, 9000.0);
        assert_eq!(params.z_hop_mm, 0.2);
        assert_eq!(params.retract_mm, 1.0);
        assert_eq!(params.path_tolerance, 0.05);
    }

    #[test]
    fn test_slicing_params_hardware_fields_round_trip() {
        let params = SlicingParams {
            filament_diameter_mm: 2.85,
            nozzle_diameter_mm: 0.6,
            travel_speed_mm_min: 12000.0,
            z_hop_mm: 0.4,
            retract_mm: 2.0,
            ..SlicingParams::default()
        };
        let json = serde_json::to_string(&params).expect("serialize");
        let back: SlicingParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.filament_diameter_mm, 2.85);
        assert_eq!(back.nozzle_diameter_mm, 0.6);
        assert_eq!(back.travel_speed_mm_min, 12000.0);
        assert_eq!(back.z_hop_mm, 0.4);
        assert_eq!(back.retract_mm, 2.0);
    }

    #[test]
    fn test_slicing_params_hardware_fields_default_when_absent() {
        // Legacy JSON without the new fields should still deserialize with defaults
        let json = r#"{"layer_height":0.2,"infill_density":0.2,"print_speed":60.0,"nozzle_temp":210.0,"bed_temp":60.0}"#;
        let params: SlicingParams = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            params.filament_diameter_mm, 1.75,
            "default filament diameter"
        );
        assert_eq!(params.nozzle_diameter_mm, 0.4, "default nozzle diameter");
        assert_eq!(params.travel_speed_mm_min, 9000.0, "default travel speed");
        assert_eq!(params.z_hop_mm, 0.2, "default z-hop");
        assert_eq!(params.retract_mm, 1.0, "default retract");
        assert_eq!(params.path_tolerance, 0.05, "default path tolerance");
    }
}
