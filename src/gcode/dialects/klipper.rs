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
}
