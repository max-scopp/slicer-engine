//! Slice command - performs 3D model slicing

use crate::cli::emit::Emitter;
use crate::cli::output::{EmitPayload, OutputFormat};
use crate::core::slice_mesh;
use crate::gcode::{GcodeGenerator, GcodeFlavor};
use crate::mesh::analysis::{calculate_aabb, calculate_surface_area, calculate_volume};
use crate::mesh::io::read_stl;
use crate::mesh::transforms::{center_mesh, drop_to_floor};
use crate::settings::load_settings;
use clap::Parser;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Slice a 3D model into layers
#[derive(Parser, Debug)]
pub struct SliceCommand {
    /// Input model file path (STL)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Layer height in millimeters
    #[arg(short = 'l', long)]
    pub layer_height: Option<f64>,

    /// Output file path (auto-generated if not specified)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format (json, human)
    #[arg(long, default_value = "human")]
    pub output_format: String,

    /// G-code firmware flavor (marlin, klipper).
    /// When omitted, falls back to the value stored in global settings (default: marlin).
    #[arg(long)]
    pub gcode_flavor: Option<String>,

    /// Enable verbose output (prints AABB, volume, surface area)
    #[arg(short, long)]
    pub verbose: bool,

    /// Center the mesh horizontally before slicing
    #[arg(long)]
    pub center: bool,

    /// Drop the mesh to Z=0 before slicing
    #[arg(long)]
    pub drop_to_floor: bool,
}

/// Result payload emitted by the `slice` command.
struct SliceResult {
    input_name: String,
    layer_height: f64,
    layer_count: usize,
    output_path: Option<PathBuf>,
    gcode_flavor: String,
}

impl EmitPayload for SliceResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/slice-result-v1"
    }

    fn display_human(&self) -> String {
        let mut s = format!(
            "✓ Sliced {} into {} layers\n  Layer height: {} mm\n  G-code flavor: {}",
            self.input_name, self.layer_count, self.layer_height, self.gcode_flavor
        );
        if let Some(path) = &self.output_path {
            s.push_str(&format!("\n  Output: {}", path.display()));
        }
        s
    }

    fn to_json(&self) -> Value {
        json!({
            "status": "success",
            "input": self.input_name,
            "layer_height": self.layer_height,
            "layer_count": self.layer_count,
            "gcode_flavor": self.gcode_flavor,
            "output": self.output_path.as_ref().map(|p| p.display().to_string()),
        })
    }
}

impl SliceCommand {
    /// Execute the slice command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let format = self
            .output_format
            .parse::<OutputFormat>()
            .map_err(|e| format!("Invalid output format: {}", e))?;

        let emitter = Emitter::new(format);

        // Load persisted settings — used for layer height default and gcode flavor default
        let settings = load_settings().unwrap_or_else(|_| Default::default());

        // Resolve gcode flavor: CLI arg → global settings → built-in default (Marlin)
        let flavor_str = self
            .gcode_flavor
            .as_deref()
            .unwrap_or(&settings.gcode_flavor);
        let flavor = flavor_str
            .parse::<GcodeFlavor>()
            .map_err(|e| format!("Invalid G-code flavor: {}", e))?;
        let default_layer_height = settings.params.layer_height;
        let layer_height = self.layer_height.unwrap_or(default_layer_height);

        // Build slicing params (layer height may be overridden by CLI flag)
        let mut slice_params = settings.params.clone();
        slice_params.layer_height = layer_height;

        // Validate input file exists
        if !self.input.exists() {
            return Err(format!("Input file not found: {}", self.input.display()).into());
        }

        if self.verbose {
            emitter.log_debug(&format!("loading mesh: {:?}", self.input));
            emitter.log_debug(&format!("G-code flavor: {}", flavor));
        }

        // Load the STL mesh
        let mut mesh = read_stl(&self.input)
            .map_err(|e| format!("Failed to load mesh '{}': {}", self.input.display(), e))?;

        // Apply optional transforms
        if self.center {
            mesh = center_mesh(&mesh);
            if self.verbose {
                emitter.log_debug("applied center transform");
            }
        }
        if self.drop_to_floor {
            mesh = drop_to_floor(&mesh);
            if self.verbose {
                emitter.log_debug("applied drop-to-floor transform");
            }
        }

        if self.verbose {
            // Compute and log geometry
            let aabb = calculate_aabb(&mesh);
            emitter.log_debug(&format!(
                "AABB: ({:.3}, {:.3}, {:.3}) → ({:.3}, {:.3}, {:.3})",
                aabb.min.x, aabb.min.y, aabb.min.z, aabb.max.x, aabb.max.y, aabb.max.z
            ));
            emitter.log_debug(&format!(
                "dimensions: {:.3} × {:.3} × {:.3} mm",
                aabb.width(),
                aabb.depth(),
                aabb.height()
            ));

            match calculate_volume(&mesh) {
                Ok(vol) => emitter.log_debug(&format!("volume: {:.3} mm³", vol)),
                Err(e) => emitter.log_debug(&format!("volume: {}", e)),
            }

            let area = calculate_surface_area(&mesh);
            emitter.log_debug(&format!("surface area: {:.3} mm²", area));

            emitter.log_debug(&format!(
                "faces: {}, vertices: {}",
                mesh.faces.len(),
                mesh.vertices.len()
            ));
            emitter.log_debug(&format!("layer height: {:.3} mm", layer_height));
        }

        // Slice the mesh into layers
        if self.verbose {
            emitter.log_debug("slicing mesh…");
        }
        let layers = slice_mesh(&mesh, layer_height);

        if self.verbose {
            emitter.log_debug(&format!("sliced into {} layers", layers.len()));
        }

        // Generate G-code using the selected firmware flavor; route dialect
        // warnings through the emitter's warn channel
        let emitter_for_warn = emitter.clone();
        let generator = GcodeGenerator::new(flavor)
            .with_warn_fn(move |msg| emitter_for_warn.log_warn(msg));
        let gcode = generator.generate(&layers, &slice_params);

        // Determine output path
        let output_path = self.output.clone().or_else(|| {
            // Auto-generate from input filename: model.stl → model.gcode
            // Guard against empty stems (e.g. hidden files like ".stl")
            let stem = self.input.file_stem()?;
            if stem.is_empty() {
                return None;
            }
            Some(self.input.with_file_name(stem).with_extension("gcode"))
        });

        // Write G-code to file
        if let Some(ref path) = output_path {
            std::fs::write(path, &gcode)
                .map_err(|e| format!("Failed to write G-code to '{}': {}", path.display(), e))?;
            if self.verbose {
                emitter.log_debug(&format!("wrote G-code to {}", path.display()));
            }
        }

        let input_name = self
            .input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        let result = SliceResult {
            input_name,
            layer_height,
            layer_count: layers.len(),
            output_path,
            gcode_flavor: flavor.to_string(),
        };

        emitter.emit(&result);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_command_creation() {
        let cmd = SliceCommand {
            input: PathBuf::from("test.stl"),
            layer_height: Some(0.2),
            output: None,
            output_format: "human".to_string(),
            gcode_flavor: Some("marlin".to_string()),
            verbose: false,
            center: false,
            drop_to_floor: false,
        };
        assert_eq!(cmd.layer_height, Some(0.2));
        assert_eq!(cmd.gcode_flavor.as_deref(), Some("marlin"));
    }

    #[test]
    fn test_slice_command_no_flavor_uses_none() {
        let cmd = SliceCommand {
            input: PathBuf::from("test.stl"),
            layer_height: Some(0.2),
            output: None,
            output_format: "human".to_string(),
            gcode_flavor: None, // will fall back to settings / marlin
            verbose: false,
            center: false,
            drop_to_floor: false,
        };
        assert!(cmd.gcode_flavor.is_none());
    }

    #[test]
    fn test_slice_command_klipper_flavor() {
        let cmd = SliceCommand {
            input: PathBuf::from("test.stl"),
            layer_height: Some(0.2),
            output: None,
            output_format: "human".to_string(),
            gcode_flavor: Some("klipper".to_string()),
            verbose: false,
            center: false,
            drop_to_floor: false,
        };
        assert_eq!(cmd.gcode_flavor.as_deref(), Some("klipper"));
    }

    #[test]
    fn test_slice_result_schema() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
            layer_count: 5,
            output_path: None,
            gcode_flavor: "marlin".to_string(),
        };
        assert_eq!(r.schema(), "slicer-engine/slice-result-v1");
    }

    #[test]
    fn test_slice_result_human() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
            layer_count: 5,
            output_path: None,
            gcode_flavor: "marlin".to_string(),
        };
        let s = r.display_human();
        assert!(s.contains("model.stl"));
        assert!(s.contains("0.2"));
        assert!(s.contains('5'));
        assert!(s.contains("marlin"));
    }

    #[test]
    fn test_slice_result_human_klipper() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
            layer_count: 5,
            output_path: None,
            gcode_flavor: "klipper".to_string(),
        };
        let s = r.display_human();
        assert!(s.contains("klipper"));
    }

    #[test]
    fn test_slice_result_human_with_output() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
            layer_count: 5,
            output_path: Some(PathBuf::from("/tmp/model.gcode")),
            gcode_flavor: "marlin".to_string(),
        };
        let s = r.display_human();
        assert!(s.contains("model.gcode"));
    }

    #[test]
    fn test_slice_result_json_fields() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
            layer_count: 5,
            output_path: None,
            gcode_flavor: "marlin".to_string(),
        };
        let v = r.to_json();
        assert_eq!(v["status"], "success");
        assert_eq!(v["input"], "model.stl");
        assert_eq!(v["layer_height"], 0.2);
        assert_eq!(v["layer_count"], 5);
        assert_eq!(v["gcode_flavor"], "marlin");
    }

    #[test]
    fn test_success_result_still_compiles() {
        use crate::cli::emit::SuccessResult;
        // Verify the built-in SuccessResult is still usable
        let r = SuccessResult {
            message: "ok".to_string(),
            details: None,
        };
        assert_eq!(r.schema(), "slicer-engine/result-v1");
    }
}
