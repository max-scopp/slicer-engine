//! Slicing parameters: per-print and per-object settings.

use crate::gcode::GcodeFlavor;
use crate::infill::InfillPattern;
pub use crate::mesh::transforms::MeshQuality;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters that control how a model is sliced and printed.
///
/// All dimensional values are in millimeters; speeds in mm/s;
/// temperatures in °C; infill density as a fraction 0.0–1.0.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default)]
pub struct SlicingParams {
    #[schemars(description = "Layer height in mm.

Smaller values produce finer detail but increase print time.
**Typical:** 0.05–0.35 mm.", extend("x-group" = "Layer"))]
    pub layer_height: f64,

    #[schemars(description = "Number of perimeter (wall) beads per layer.

Arachne places up to this many concentric wall paths around each shell polygon.
The innermost bead may have variable width when narrow space remains.
**Typical:** 2–4.", extend("x-group" = "Walls"))]
    #[serde(default = "SlicingParams::default_wall_count")]
    pub wall_count: usize,

    #[schemars(
        description = "Minimum allowed bead width as a fraction of nozzle diameter.

Beads narrower than `wall_line_width_min × nozzle_diameter_mm` are skipped entirely.
**Range:** 0.5–1.0.",
        extend("x-group" = "Walls")
    )]
    #[serde(default = "SlicingParams::default_wall_line_width_min")]
    pub wall_line_width_min: f64,

    #[schemars(
        description = "Maximum allowed bead width as a fraction of nozzle diameter.

Variable-width beads are capped at this multiple to avoid excessive over-extrusion at corners.
**Range:** 1.0–2.0.",
        extend("x-group" = "Walls")
    )]
    #[serde(default = "SlicingParams::default_wall_line_width_max")]
    pub wall_line_width_max: f64,

    #[schemars(
        description = "Minimum wall space (fraction of nozzle diameter) before bead count decreases.

When remaining space is narrower than `wall_transition_threshold × nozzle_diameter_mm`,
the algorithm widens the existing innermost bead instead of adding a new one.
**Typical:** 0.4–0.8.",
        extend("x-group" = "Walls")
    )]
    #[serde(default = "SlicingParams::default_wall_transition_threshold")]
    pub wall_transition_threshold: f64,

    #[schemars(
        description = "Length (mm) over which a bead-count transition is smoothed.

Larger values produce a gradual width ramp at transitions; smaller values create abrupt changes.
**Typical:** 0.5–2.0 mm.",
        extend("x-group" = "Walls")
    )]
    #[serde(default = "SlicingParams::default_wall_transition_length")]
    pub wall_transition_length: f64,

    #[schemars(description = "Number of inner wall beads that absorb width variation.

When space is too narrow for a separate bead, up to this many innermost beads
are widened proportionally to fill the gap.
**Typical:** 1–2.", extend("x-group" = "Walls"))]
    #[serde(default = "SlicingParams::default_wall_distribution_count")]
    pub wall_distribution_count: usize,

    #[schemars(description = "Infill density as a fraction (0.0–1.0).

- `0.0` = completely hollow
- `0.15`–`0.3` = typical range for good strength/speed balance
- `1.0` = fully solid", extend("x-group" = "Infill"))]
    pub infill_density: f64,

    #[schemars(description = "Infill pattern geometry.

Supported values:
- `rectilinear` — alternating straight lines (fastest)
- `grid` — crossed lines forming a grid
- `honeycomb` — hexagonal cells (good strength-to-weight ratio)
- `gyroid` — smooth triply-periodic surface (excellent isotropy)", extend("x-group" = "Infill"))]
    #[serde(default = "SlicingParams::default_infill_pattern")]
    pub infill_pattern: InfillPattern,

    #[schemars(description = "Base angle in degrees for sparse infill lines.

Alternating layers rotate by +90° on top of this base angle to create a crossing pattern.
**Default:** 45°.", extend("x-group" = "Infill"))]
    #[serde(default = "SlicingParams::default_infill_base_angle")]
    pub infill_base_angle: f64,

    #[schemars(description = "Print speed in mm/s.

Slower speeds improve layer adhesion and surface quality; faster speeds reduce print time.
**Typical:** 40–100 mm/s.", extend("x-group" = "Speed"))]
    pub print_speed: f64,

    #[schemars(description = "Nozzle temperature in °C.

Material guidelines:
- **PLA:** 200–210 °C
- **PETG:** 230–250 °C
- **ABS:** 240–260 °C", extend("x-group" = "Temperature"))]
    pub nozzle_temp: f64,

    #[schemars(description = "Heated bed temperature in °C.

Material guidelines:
- **PLA:** 60–80 °C
- **PETG:** 80–100 °C
- **ABS:** 100–120 °C

Set to `0` for an unheated bed.", extend("x-group" = "Temperature"))]
    pub bed_temp: f64,

    #[schemars(
        description = "Number of solid top layers (horizontal surfaces facing up).

More layers improve surface quality and reduce infill show-through.
**Typical:** 4–6 layers at 0.2 mm layer height.",
        extend("x-group" = "Surfaces")
    )]
    #[serde(default = "SlicingParams::default_top_layers")]
    pub top_layers: usize,

    #[schemars(
        description = "Number of solid bottom layers (horizontal surfaces facing down).

More layers improve bottom surface finish and bed adhesion strength.
**Typical:** 3–4 layers.",
        extend("x-group" = "Surfaces")
    )]
    #[serde(default = "SlicingParams::default_bottom_layers")]
    pub bottom_layers: usize,

    #[schemars(
        description = "Angle in degrees for top/bottom solid surface infill lines.

Changing from the default can improve finish on curved or organic models.
**Default:** 45°.",
        extend("x-group" = "Surfaces")
    )]
    #[serde(default = "SlicingParams::default_surface_infill_angle")]
    pub surface_infill_angle: f64,

    #[schemars(description = "Filament diameter in mm.

Used to calculate extrusion volume from feed distance. Standard sizes:
- `1.75 mm` — most common
- `2.85 mm` — some older or larger-format printers", extend("x-group" = "Hardware"))]
    #[serde(default = "SlicingParams::default_filament_diameter_mm")]
    pub filament_diameter_mm: f64,

    #[schemars(description = "Nozzle orifice diameter in mm.

Affects minimum feature resolution and all line-width calculations.
**Standard:** 0.4 mm. Other common sizes: 0.2, 0.6, 0.8 mm.", extend("x-group" = "Hardware"))]
    #[serde(default = "SlicingParams::default_nozzle_diameter_mm")]
    pub nozzle_diameter_mm: f64,

    #[schemars(description = "Non-print (travel) move speed in **mm/min**.

Convert from mm/s by multiplying by 60. Fast travel reduces print time without affecting print quality.
**Example:** 9000 mm/min = 150 mm/s.", extend("x-group" = "Speed"))]
    #[serde(default = "SlicingParams::default_travel_speed_mm_min")]
    pub travel_speed_mm_min: f64,

    #[schemars(description = "Z-hop lift height in mm during travel moves.

Lifts the nozzle before travelling to reduce stringing and nozzle drag across the print.
**Typical:** 0.2–0.5 mm. Set to `0` to disable.", extend("x-group" = "Retraction"))]
    #[serde(default = "SlicingParams::default_z_hop_mm")]
    pub z_hop_mm: f64,

    #[schemars(description = "Retraction distance in mm on travel moves.

Pulls filament back into the nozzle to reduce oozing and stringing.
**Typical:** 0.5–2 mm (direct drive) or 3–7 mm (Bowden).", extend("x-group" = "Retraction"))]
    #[serde(default = "SlicingParams::default_retract_mm")]
    pub retract_mm: f64,

    #[schemars(
        description = "Use a single outer wall on the topmost layer of top surfaces.

Reduces the chance of pillowing and prevents infill patterns from showing through the top surface.
**Recommended:** enabled.",
        extend("x-group" = "Surfaces")
    )]
    #[serde(default = "SlicingParams::default_only_one_wall_top")]
    pub only_one_wall_top: bool,

    #[schemars(description = "Use a single outer wall on the first layer.

Improves bed adhesion and avoids potential issues with multiple perimeters pressing against the bed simultaneously.
**Recommended:** enabled.", extend("x-group" = "Surfaces"))]
    #[serde(default = "SlicingParams::default_only_one_wall_first_layer")]
    pub only_one_wall_first_layer: bool,

    #[schemars(
        description = "Overhang angle threshold in degrees (0–90) for skipping solid surface generation.

Surfaces are skipped when the overhang angle is below this threshold, since shallow overhangs may not need solid fill.
**Default:** 45°. Set to `0` to always generate surfaces.",
        extend("x-group" = "Surfaces")
    )]
    #[serde(default = "SlicingParams::default_support_threshold_angle")]
    pub support_threshold_angle: f64,

    #[schemars(
        description = "Overlap of solid surfaces into perimeter walls for bonding (0.0–1.0).

Ensures surfaces bond to walls without leaving gaps at the perimeter boundary.
**Typical:** 0.25 (25% of a bead width).",
        extend("x-group" = "Infill")
    )]
    #[serde(default = "SlicingParams::default_infill_overlap_percent")]
    pub infill_overlap_percent: f64,

    #[schemars(
        description = "Maximum perpendicular deviation (mm) for path simplification (Ramer–Douglas–Peucker).

Reduces the number of G-code points without visibly affecting print quality.
**Typical:** 0.01–0.1 mm. Set to `0.0` to disable.",
        extend("x-group" = "Output")
    )]
    #[serde(default = "SlicingParams::default_path_tolerance")]
    pub path_tolerance: f64,

    #[schemars(
        description = "G-code firmware flavor for the target printer.\n\nSupported values:\n- `marlin` — Marlin firmware (widely compatible)\n- `klipper` — Klipper firmware (macro-based)",
        extend("x-group" = "Output")
    )]
    #[serde(default = "SlicingParams::default_gcode_flavor")]
    pub gcode_flavor: GcodeFlavor,

    #[schemars(
        description = "Optional mesh decimation applied before slicing.\n\nSupported values:\n- `normal` — no decimation (default)\n- `high-quality` — no decimation, signals maximum fidelity\n- `draft` — aggressive polygon reduction for faster slicing",
        extend("x-group" = "Mesh")
    )]
    #[serde(default = "SlicingParams::default_mesh_quality")]
    pub mesh_quality: MeshQuality,
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
            gcode_flavor: Self::default_gcode_flavor(),
            mesh_quality: Self::default_mesh_quality(),
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

    fn default_infill_pattern() -> InfillPattern {
        InfillPattern::Rectilinear
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

    fn default_gcode_flavor() -> GcodeFlavor {
        GcodeFlavor::Marlin
    }

    fn default_mesh_quality() -> MeshQuality {
        MeshQuality::Normal
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
