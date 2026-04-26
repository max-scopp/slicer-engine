//! Output format types and the `EmitPayload` trait.

use std::str::FromStr;

/// Supported output formats for CLI operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Structured JSON output (results include a `$schema` field).
    Json,
    /// Human-readable text output.
    #[default]
    Human,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "human" | "text" => Ok(OutputFormat::Human),
            _ => Err(format!("Unknown output format: {}", s)),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Human => write!(f, "human"),
        }
    }
}

/// A typed result that can be emitted by [`crate::cli::emit::Emitter`].
///
/// Implement this trait on each result type so the emitter can render it in
/// both human-readable and JSON formats, and so JSON consumers can derive the
/// concrete type from the `$schema` field.
pub trait EmitPayload {
    /// JSON Schema identifier embedded as `"$schema"` in JSON output.
    ///
    /// Use a URL-style path, e.g. `"slicer-engine/info-result-v1"`.
    fn schema(&self) -> &'static str;

    /// One or more lines of human-readable output for this result.
    fn display_human(&self) -> String;

    /// JSON representation of this result's data fields (without `$schema`).
    fn to_json(&self) -> serde_json::Value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_str("human").unwrap(),
            OutputFormat::Human
        );
        assert_eq!(OutputFormat::from_str("text").unwrap(), OutputFormat::Human);
        assert_eq!(OutputFormat::from_str("JSON").unwrap(), OutputFormat::Json);
    }

    #[test]
    fn test_output_format_parsing_csv_rejected() {
        assert!(OutputFormat::from_str("csv").is_err());
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

    #[test]
    fn test_output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Human);
    }
}
