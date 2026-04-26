//! Info command - displays build and library information

use clap::Parser;
use crate::cli::output::OutputFormat;
use serde_json::json;

/// Display build and library information
#[derive(Parser, Debug)]
pub struct InfoCommand {
    /// Output format (json, human, csv)
    #[arg(long, default_value = "human")]
    pub output_format: String,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

impl InfoCommand {
    /// Execute the info command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let format = OutputFormat::from_str(&self.output_format)
            .map_err(|e| format!("Invalid output format: {}", e))?;

        let version = env!("CARGO_PKG_VERSION");

        match format {
            OutputFormat::Json => {
                let info = json!({
                    "name": "slicer-engine",
                    "version": version,
                    "edition": "2021",
                    "verbose": self.verbose,
                });
                println!("{}", serde_json::to_string_pretty(&info)?);
            }
            OutputFormat::Human => {
                println!("Slicer Engine");
                println!("  Version: {}", version);
                println!("  Edition: 2021");
                if self.verbose {
                    println!("  Features: clipper2-based polygon clipping");
                }
            }
            OutputFormat::Csv => {
                println!("field,value");
                println!("name,slicer-engine");
                println!("version,{}", version);
                println!("edition,2021");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_command_creation() {
        let cmd = InfoCommand {
            output_format: "human".to_string(),
            verbose: false,
        };
        assert!(!cmd.verbose);
    }
}
