//! Klipper firmware G-code dialect.

use crate::gcode::GcodeDialect;
use crate::settings::params::SlicingParams;

/// Klipper firmware G-code dialect.
///
/// Targets Klipper firmware's macro-based print workflow.  The default start
/// and end scripts delegate to the user-defined `START_PRINT` / `END_PRINT`
/// macros (OrcaSlicer-style), passing print parameters as macro arguments.
///
/// Printer-specific setup (homing, bed levelling, purge lines, etc.) is
/// handled inside those Klipper macros, keeping the slicer output clean and
/// portable across different Klipper printer configurations.
///
/// Extra Klipper-specific commands are available as helper methods:
/// - [`KlipperDialect::set_velocity_limit`] — runtime velocity/acceleration cap
/// - [`KlipperDialect::set_pressure_advance`] — pressure advance tuning
/// - [`KlipperDialect::call_macro`] — invoke a named Klipper macro
pub struct KlipperDialect;

impl KlipperDialect {
    /// Emit a `SET_VELOCITY_LIMIT` command.
    ///
    /// Klipper uses this to configure the printer's motion system at runtime,
    /// which is more flexible than compile-time Marlin firmware limits.
    pub fn set_velocity_limit(&self, velocity: f64, accel: f64) -> String {
        format!(
            "SET_VELOCITY_LIMIT VELOCITY={:.0} ACCEL={:.0}",
            velocity, accel
        )
    }

    /// Emit a `SET_PRESSURE_ADVANCE` command.
    ///
    /// Pressure advance compensates for filament compression in the hotend,
    /// improving corner quality at high speeds.
    pub fn set_pressure_advance(&self, value: f64) -> String {
        format!("SET_PRESSURE_ADVANCE ADVANCE={:.4}", value)
    }

    /// Invoke a named Klipper macro (e.g. `PRINT_START`, `PRINT_END`).
    ///
    /// The name is upper-cased to match Klipper macro naming conventions.
    pub fn call_macro(&self, name: &str) -> String {
        name.to_uppercase()
    }

    /// Return the Klipper fan name for a given fan index.
    ///
    /// Klipper uses named fans rather than P-indexed M106 commands:
    /// - P0 → `fan` (part-cooling, the default Klipper fan object)
    /// - P1 → `fan_hotend`
    /// - P2 → `fan_chamber`
    /// - P3 and above → `fan_aux`
    ///
    /// All indices beyond 3 map to `fan_aux` on the assumption that a printer
    /// with more than 4 fans would require custom start/end scripts rather than
    /// generic indexed fan commands.
    pub fn fan_name_for_index(fan_index: u8) -> &'static str {
        match fan_index {
            0 => "fan",
            1 => "fan_hotend",
            2 => "fan_chamber",
            _ => "fan_aux",
        }
    }
}

impl GcodeDialect for KlipperDialect {
    fn flavor_name(&self) -> &'static str {
        "Klipper"
    }

    /// Default Klipper start script: delegates to the `START_PRINT` macro.
    ///
    /// Print temperatures are forwarded as macro arguments so the user's
    /// Klipper `START_PRINT` macro can use them for pre-heat, bed levelling,
    /// purge routines, etc.  This follows the OrcaSlicer / SuperSlicer
    /// convention for Klipper start G-code.
    fn start_script(&self, params: &SlicingParams) -> Vec<String> {
        vec![format!(
            "START_PRINT BED_TEMP={:.0} EXTRUDER_TEMP={:.0}",
            params.bed_temp, params.nozzle_temp
        )]
    }

    /// Default Klipper end script: delegates to the `END_PRINT` macro.
    fn end_script(&self) -> Vec<String> {
        vec!["END_PRINT".to_string()]
    }

    /// Klipper uses `SET_FAN_SPEED fan=<name> speed=<0.0–1.0>` instead of
    /// Marlin's `M106 P<n> S<0–255>`.
    ///
    /// The fan name is resolved from `name_hint` first (custom printer config),
    /// then falls back to the default name derived from `fan_index`.
    fn set_fan_speed_indexed(&self, fan_index: u8, name_hint: Option<&str>, speed: f64) -> String {
        let name = name_hint.unwrap_or_else(|| Self::fan_name_for_index(fan_index));
        let s = speed.clamp(0.0, 1.0);
        format!("SET_FAN_SPEED fan={} speed={:.4}", name, s)
    }
}
