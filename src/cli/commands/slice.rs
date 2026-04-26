//! Slice command - performs 3D model slicing

use crate::cli::emit::Emitter;
use crate::cli::output::{EmitPayload, OutputFormat};
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
}

impl EmitPayload for SliceResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/slice-result-v1"
    }

    fn display_human(&self) -> String {
        format!(
            "✓ Sliced {} into layers\n  Layer height: {} mm",
            self.input_name, self.layer_height
        )
    }

    fn to_json(&self) -> Value {
        json!({
            "status": "success",
            "input": self.input_name,
            "layer_height": self.layer_height,
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

        // Load persisted settings and get default layer height
        let settings = load_settings().unwrap_or_else(|_| Default::default());
        let default_layer_height = settings.params.layer_height;

        // Validate input file exists
        if !self.input.exists() {
            return Err(format!("Input file not found: {}", self.input.display()).into());
        }

        if self.verbose {
            emitter.log_debug(&format!("loading mesh: {:?}", self.input));
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
            emitter.log_debug(&format!(
                "layer height: {:.3} mm",
                self.layer_height.unwrap_or(default_layer_height)
            ));
        }

        // TODO: Implement actual slicing logic
        let result = SliceResult {
            input_name: self
                .input
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            layer_height: self.layer_height.unwrap_or(default_layer_height),
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
            verbose: false,
            center: false,
            drop_to_floor: false,
        };
        assert_eq!(cmd.layer_height, Some(0.2));
    }

    #[test]
    fn test_slice_result_schema() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
        };
        assert_eq!(r.schema(), "slicer-engine/slice-result-v1");
    }

    #[test]
    fn test_slice_result_human() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
        };
        let s = r.display_human();
        assert!(s.contains("model.stl"));
        assert!(s.contains("0.2"));
    }

    #[test]
    fn test_slice_result_json_fields() {
        let r = SliceResult {
            input_name: "model.stl".to_string(),
            layer_height: 0.2,
        };
        let v = r.to_json();
        assert_eq!(v["status"], "success");
        assert_eq!(v["input"], "model.stl");
        assert_eq!(v["layer_height"], 0.2);
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
