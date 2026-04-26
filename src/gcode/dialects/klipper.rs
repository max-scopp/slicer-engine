//! Klipper firmware G-code dialect.

use crate::gcode::GcodeDialect;
use crate::settings::params::SlicingParams;

/// Klipper firmware G-code dialect.
///
/// Extends the standard command set with Klipper-specific commands:
/// - [`KlipperDialect::set_velocity_limit`] — runtime velocity/acceleration cap
/// - [`KlipperDialect::set_pressure_advance`] — pressure advance tuning
/// - [`KlipperDialect::call_macro`] — invoke a named Klipper macro
///
/// The start script automatically applies `SET_VELOCITY_LIMIT` based on the
/// configured print speed.
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

    fn start_script(&self, params: &SlicingParams) -> Vec<String> {
        vec![
            "G21 ; millimetres".to_string(),
            "G90 ; absolute positioning".to_string(),
            "M82 ; extruder absolute mode".to_string(),
            format!("M104 S{:.0} ; set nozzle temperature", params.nozzle_temp),
            format!("M140 S{:.0} ; set bed temperature", params.bed_temp),
            "G28 ; home all axes".to_string(),
            format!(
                "M109 S{:.0} ; wait for nozzle temperature",
                params.nozzle_temp
            ),
            format!("M190 S{:.0} ; wait for bed temperature", params.bed_temp),
            "G92 E0 ; reset extruder".to_string(),
            // Klipper-specific: apply velocity limits derived from slicing params
            self.set_velocity_limit(params.print_speed, 3000.0),
        ]
    }

    fn end_script(&self) -> Vec<String> {
        vec![
            "; end of print".to_string(),
            "G91 ; relative positioning".to_string(),
            "G1 E-2 F3000 ; final retract".to_string(),
            "G1 Z5 F3000 ; lift nozzle".to_string(),
            "G90 ; absolute positioning".to_string(),
            "G28 X0 Y0 ; park".to_string(),
            "M104 S0 ; nozzle off".to_string(),
            "M140 S0 ; bed off".to_string(),
            "M84 ; disable motors".to_string(),
        ]
    }
}
