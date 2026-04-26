//! Slice command - performs 3D model slicing

use clap::Parser;
use std::path::PathBuf;
use crate::cli::output::{OutputFormat, SuccessOutput, OutputFormatter};

/// Slice a 3D model into layers
#[derive(Parser, Debug)]
pub struct SliceCommand {
    /// Input model file path (STL, OBJ, etc.)
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

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

impl SliceCommand {
    /// Execute the slice command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let format = OutputFormat::from_str(&self.output_format)
            .map_err(|e| format!("Invalid output format: {}", e))?;

        // Validate input file exists
        if !self.input.exists() {
            return Err(format!("Input file not found: {}", self.input.display()).into());
        }

        if self.verbose {
            eprintln!("[DEBUG] Slicing: {:?}", self.input);
            eprintln!("[DEBUG] Layer height: {:?} mm", self.layer_height.unwrap_or(0.2));
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
        };
        assert_eq!(cmd.layer_height, Some(0.2));
    }
}
