//! Slice command - performs 3D model slicing

use crate::cli::output::{OutputFormat, OutputFormatter, SuccessOutput};
use crate::mesh::analysis::{calculate_aabb, calculate_surface_area, calculate_volume};
use crate::mesh::io::read_stl;
use crate::mesh::transforms::{center_mesh, drop_to_floor};
use clap::Parser;
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

    /// Output format (json, human, csv)
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

impl SliceCommand {
    /// Execute the slice command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let format = self
            .output_format
            .parse::<OutputFormat>()
            .map_err(|e| format!("Invalid output format: {}", e))?;

        // Validate input file exists
        if !self.input.exists() {
            return Err(format!("Input file not found: {}", self.input.display()).into());
        }

        if self.verbose {
            eprintln!("[DEBUG] Loading mesh: {:?}", self.input);
        }

        // Load the STL mesh
        let mut mesh = read_stl(&self.input)
            .map_err(|e| format!("Failed to load mesh '{}': {}", self.input.display(), e))?;

        // Apply optional transforms
        if self.center {
            mesh = center_mesh(&mesh);
            if self.verbose {
                eprintln!("[DEBUG] Applied center transform");
            }
        }
        if self.drop_to_floor {
            mesh = drop_to_floor(&mesh);
            if self.verbose {
                eprintln!("[DEBUG] Applied drop-to-floor transform");
            }
        }

        if self.verbose {
            // Compute and log geometry
            let aabb = calculate_aabb(&mesh);
            eprintln!(
                "[DEBUG] AABB: ({:.3}, {:.3}, {:.3}) → ({:.3}, {:.3}, {:.3})",
                aabb.min.x, aabb.min.y, aabb.min.z, aabb.max.x, aabb.max.y, aabb.max.z
            );
            eprintln!(
                "[DEBUG] Dimensions: {:.3} × {:.3} × {:.3} mm",
                aabb.width(),
                aabb.depth(),
                aabb.height()
            );

            match calculate_volume(&mesh) {
                Ok(vol) => eprintln!("[DEBUG] Volume: {:.3} mm³", vol),
                Err(e) => eprintln!("[DEBUG] Volume: {}", e),
            }

            let area = calculate_surface_area(&mesh);
            eprintln!("[DEBUG] Surface area: {:.3} mm²", area);

            eprintln!(
                "[DEBUG] Faces: {}, Vertices: {}",
                mesh.faces.len(),
                mesh.vertices.len()
            );
            eprintln!(
                "[DEBUG] Layer height: {:.3} mm",
                self.layer_height.unwrap_or(0.2)
            );
        }

        // TODO: Implement actual slicing logic
        let output = SuccessOutput {
            message: format!(
                "Sliced {} into layers",
                self.input.file_name().unwrap_or_default().to_string_lossy()
            ),
            details: Some(format!(
                "Layer height: {} mm",
                self.layer_height.unwrap_or(0.2)
            )),
        };

        output.print(format);
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
}
