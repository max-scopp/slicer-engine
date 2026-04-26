//! Marlin firmware G-code dialect.

use crate::gcode::GcodeDialect;
use crate::settings::params::SlicingParams;

/// Marlin firmware G-code dialect.
///
/// Targets maximum compatibility with consumer FDM printers.  Uses the
/// standard RepRap M-command set without any firmware-specific extensions.
pub struct MarlinDialect;

impl GcodeDialect for MarlinDialect {
    fn flavor_name(&self) -> &'static str {
        "Marlin"
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
