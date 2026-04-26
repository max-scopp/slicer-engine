//! Output formatting for CLI operations

use serde_json::json;

/// Output format types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// JSON format
    Json,
    /// Human-readable format
    #[default]
    Human,
    /// CSV format (future)
    Csv,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "human" | "text" => Ok(OutputFormat::Human),
            "csv" => Ok(OutputFormat::Csv),
            _ => Err(format!("Unknown output format: {}", s)),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Human => write!(f, "human"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
}

/// Output formatter trait
pub trait OutputFormatter {
    /// Format and print output
    fn print(&self, format: OutputFormat);
}

/// Success message output
pub struct SuccessOutput {
    pub message: String,
    pub details: Option<String>,
}

impl OutputFormatter for SuccessOutput {
    fn print(&self, format: OutputFormat) {
        match format {
            OutputFormat::Json => {
                let output = json!({
                    "status": "success",
                    "message": self.message,
                    "details": self.details
                });
                println!("{}", output);
            }
            OutputFormat::Human => {
                println!("✓ {}", self.message);
                if let Some(details) = &self.details {
                    println!("  {}", details);
                }
            }
            OutputFormat::Csv => {
                println!("{}", self.message);
            }
        }
    }
}

/// Error output
pub struct ErrorOutput {
    pub error: String,
    pub context: Option<String>,
}

impl OutputFormatter for ErrorOutput {
    fn print(&self, format: OutputFormat) {
        match format {
            OutputFormat::Json => {
                let output = json!({
                    "status": "error",
                    "error": self.error,
                    "context": self.context
                });
                eprintln!("{}", output);
            }
            OutputFormat::Human => {
                eprintln!("✗ Error: {}", self.error);
                if let Some(context) = &self.context {
                    eprintln!("  Context: {}", context);
                }
            }
            OutputFormat::Csv => {
                eprintln!("{}", self.error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_str("human").unwrap(),
            OutputFormat::Human
        );
        assert_eq!(OutputFormat::from_str("csv").unwrap(), OutputFormat::Csv);
        assert_eq!(OutputFormat::from_str("JSON").unwrap(), OutputFormat::Json);
    }

    #[test]
    fn test_output_format_invalid() {
        assert!(OutputFormat::from_str("invalid").is_err());
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Json.to_string(), "json");
        assert_eq!(OutputFormat::Human.to_string(), "human");
    }
}
