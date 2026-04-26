//! Slice command - performs 3D model slicing

use clap::Parser;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::cli::emit::Emitter;
use crate::cli::output::{EmitPayload, OutputFormat};

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

    /// Output format (json, human)
    #[arg(long, default_value = "human")]
    pub output_format: String,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
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

        // Validate input file exists
        if !self.input.exists() {
            return Err(format!("Input file not found: {}", self.input.display()).into());
        }

        if self.verbose {
            emitter.log_debug(&format!("slicing: {:?}", self.input));
            emitter.log_debug(&format!(
                "layer height: {} mm",
                self.layer_height.unwrap_or(0.2)
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
            layer_height: self.layer_height.unwrap_or(0.2),
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
