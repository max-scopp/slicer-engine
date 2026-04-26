//! Central output emitter for CLI operations.
//!
//! The emitter enforces a clear separation between two output channels:
//!
//! - **Logs** (informational / debug messages) → **stderr**
//! - **Results** (typed payloads) → **stdout**
//!
//! In JSON mode every result includes a `$schema` field so downstream
//! consumers can derive the concrete type without additional out-of-band
//! metadata.
//!
//! # Example
//!
//! ```
//! use slicer_engine::cli::emit::Emitter;
//! use slicer_engine::cli::output::OutputFormat;
//!
//! let emitter = Emitter::new(OutputFormat::Human);
//! emitter.log("starting slice operation");
//! ```

use serde_json::{json, Value};

use crate::cli::output::{EmitPayload, OutputFormat};

/// Central emitter that routes log messages to stderr and results to stdout.
pub struct Emitter {
    /// Active output format for this session.
    pub format: OutputFormat,
}

impl Emitter {
    /// Create a new `Emitter` with the given output format.
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    // ── Logging (stderr) ────────────────────────────────────────────────────

    /// Emit an informational log message to **stderr**.
    ///
    /// In JSON mode the entry is a single-line JSON object:
    /// `{"$schema":"slicer-engine/log-v1","level":"info","message":"…"}`.
    pub fn log(&self, message: &str) {
        match self.format {
            OutputFormat::Json => {
                let entry = json!({
                    "$schema": "slicer-engine/log-v1",
                    "level": "info",
                    "message": message,
                });
                eprintln!("{}", entry);
            }
            OutputFormat::Human => eprintln!("[info] {}", message),
        }
    }

    /// Emit a debug log message to **stderr**.
    ///
    /// Call this only when verbose mode is active; the emitter does not
    /// gate on a verbose flag itself so the caller controls visibility.
    pub fn log_debug(&self, message: &str) {
        match self.format {
            OutputFormat::Json => {
                let entry = json!({
                    "$schema": "slicer-engine/log-v1",
                    "level": "debug",
                    "message": message,
                });
                eprintln!("{}", entry);
            }
            OutputFormat::Human => eprintln!("[debug] {}", message),
        }
    }

    // ── Results (stdout) ────────────────────────────────────────────────────

    /// Emit a typed result payload to **stdout**.
    ///
    /// - **Human mode**: calls [`EmitPayload::display_human`] and prints to
    ///   stdout.
    /// - **JSON mode**: serialises via [`EmitPayload::to_json`], injects
    ///   `"$schema"` from [`EmitPayload::schema`], and pretty-prints to
    ///   stdout.
    pub fn emit(&self, payload: &dyn EmitPayload) {
        match self.format {
            OutputFormat::Human => println!("{}", payload.display_human()),
            OutputFormat::Json => {
                let mut value = payload.to_json();
                if let Value::Object(ref mut map) = value {
                    // Insert $schema first so it appears at the top of the object.
                    let schema = Value::String(payload.schema().to_string());
                    // serde_json preserves insertion order via IndexMap; prepend
                    // by rebuilding the map with $schema first.
                    let mut ordered = serde_json::Map::new();
                    ordered.insert("$schema".to_string(), schema);
                    for (k, v) in map.iter() {
                        ordered.insert(k.clone(), v.clone());
                    }
                    value = Value::Object(ordered);
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value)
                        .expect("result value must be serialisable")
                );
            }
        }
    }

    /// Emit an error message to **stderr**.
    ///
    /// - **Human mode**: `✗ Error: <error>` with optional context line.
    /// - **JSON mode**: JSON with `$schema = "slicer-engine/error-v1"`.
    pub fn error(&self, error: &str, context: Option<&str>) {
        match self.format {
            OutputFormat::Json => {
                let output = json!({
                    "$schema": "slicer-engine/error-v1",
                    "status": "error",
                    "error": error,
                    "context": context,
                });
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&output)
                        .expect("error value must be serialisable")
                );
            }
            OutputFormat::Human => {
                eprintln!("✗ Error: {}", error);
                if let Some(ctx) = context {
                    eprintln!("  Context: {}", ctx);
                }
            }
        }
    }
}

// ── Built-in payload types ───────────────────────────────────────────────────

/// Generic success result used when no domain-specific payload is needed.
pub struct SuccessResult {
    /// Short description of what succeeded.
    pub message: String,
    /// Optional additional detail line.
    pub details: Option<String>,
}

impl EmitPayload for SuccessResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/result-v1"
    }

    fn display_human(&self) -> String {
        let mut s = format!("✓ {}", self.message);
        if let Some(d) = &self.details {
            s.push_str(&format!("\n  {}", d));
        }
        s
    }

    fn to_json(&self) -> Value {
        json!({
            "status": "success",
            "message": self.message,
            "details": self.details,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Emitter::log ────────────────────────────────────────────────────────

    #[test]
    fn test_emitter_new() {
        let emitter = Emitter::new(OutputFormat::Human);
        assert_eq!(emitter.format, OutputFormat::Human);
    }

    // ── SuccessResult ────────────────────────────────────────────────────────

    #[test]
    fn test_success_result_schema() {
        let r = SuccessResult {
            message: "done".to_string(),
            details: None,
        };
        assert_eq!(r.schema(), "slicer-engine/result-v1");
    }

    #[test]
    fn test_success_result_human_no_details() {
        let r = SuccessResult {
            message: "done".to_string(),
            details: None,
        };
        assert_eq!(r.display_human(), "✓ done");
    }

    #[test]
    fn test_success_result_human_with_details() {
        let r = SuccessResult {
            message: "done".to_string(),
            details: Some("layer height: 0.2 mm".to_string()),
        };
        assert!(r.display_human().contains("✓ done"));
        assert!(r.display_human().contains("layer height: 0.2 mm"));
    }

    #[test]
    fn test_success_result_json_has_status() {
        let r = SuccessResult {
            message: "done".to_string(),
            details: None,
        };
        let v = r.to_json();
        assert_eq!(v["status"], "success");
        assert_eq!(v["message"], "done");
    }

    // ── emit() injects $schema ───────────────────────────────────────────────

    #[test]
    fn test_emit_injects_schema_in_json_mode() {
        // We can't easily capture stdout in a unit test, so we exercise
        // the JSON value construction logic directly.
        let r = SuccessResult {
            message: "test".to_string(),
            details: None,
        };
        let mut value = r.to_json();
        if let Value::Object(ref mut map) = value {
            let mut ordered = serde_json::Map::new();
            ordered.insert("$schema".to_string(), Value::String(r.schema().to_string()));
            for (k, v) in map.iter() {
                ordered.insert(k.clone(), v.clone());
            }
            value = Value::Object(ordered);
        }
        assert_eq!(value["$schema"], "slicer-engine/result-v1");
        assert_eq!(value["status"], "success");
    }
}
