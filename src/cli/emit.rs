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
use crate::logging::ProcessLogger;

/// Central emitter that routes log messages to stderr and results to stdout.
#[derive(Clone)]
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

    /// Emit a warning log message to **stderr**.
    ///
    /// Use this for non-fatal issues that the user should be aware of, such as
    /// unsupported dialect commands falling back to generic G-code.
    ///
    /// - **Human mode**: `[warn] <message>` (yellow on terminals that support ANSI colours).
    /// - **JSON mode**: JSON with `"level": "warn"`.
    pub fn log_warn(&self, message: &str) {
        match self.format {
            OutputFormat::Json => {
                let entry = json!({
                    "$schema": "slicer-engine/log-v1",
                    "level": "warn",
                    "message": message,
                });
                eprintln!("{}", entry);
            }
            OutputFormat::Human => eprintln!("\x1b[33m[warn]\x1b[0m {}", message),
        }
    }

    /// Emit a phase-start marker to **stderr**.
    ///
    /// - **Human mode**: `[phase] <phase> → start`
    /// - **JSON mode**: JSON with `"level": "phase_start"` and `"phase"` field.
    pub fn log_phase_start(&self, phase: &str) {
        match self.format {
            OutputFormat::Json => {
                let entry = json!({
                    "$schema": "slicer-engine/log-v1",
                    "level": "phase_start",
                    "phase": phase,
                });
                eprintln!("{}", entry);
            }
            OutputFormat::Human => eprintln!("[phase] {} → start", phase),
        }
    }

    /// Emit a phase-end marker with elapsed time to **stderr**.
    ///
    /// - **Human mode**: `[phase] <phase> ✓ <ms> ms`
    /// - **JSON mode**: JSON with `"level": "phase_end"`, `"phase"`, and `"elapsed_ms"`.
    pub fn log_phase_end(&self, phase: &str, elapsed_ms: u64) {
        match self.format {
            OutputFormat::Json => {
                let entry = json!({
                    "$schema": "slicer-engine/log-v1",
                    "level": "phase_end",
                    "phase": phase,
                    "elapsed_ms": elapsed_ms,
                });
                eprintln!("{}", entry);
            }
            OutputFormat::Human => eprintln!("[phase] {} ✓ {} ms", phase, elapsed_ms),
        }
    }

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

// ── CliLogger ────────────────────────────────────────────────────────────────

/// A [`ProcessLogger`] backed by a CLI [`Emitter`].
///
/// This is the request-specific logger for CLI-initiated slicing runs. Because
/// the [`Emitter`] writes to stderr it automatically satisfies the "global
/// logger" requirement – every message reaches the operator's terminal.
///
/// Debug messages are gated on the `verbose` flag so that `--verbose` output
/// from the CLI maps cleanly to the pipeline's `log_debug` calls.
#[derive(Clone)]
pub struct CliLogger {
    emitter: Emitter,
    verbose: bool,
}

impl CliLogger {
    /// Create a new `CliLogger`.
    ///
    /// When `verbose` is `false`, calls to [`ProcessLogger::log_debug`] are
    /// silently suppressed, matching the existing `--verbose` CLI behaviour.
    pub fn new(emitter: Emitter, verbose: bool) -> Self {
        Self { emitter, verbose }
    }
}

impl ProcessLogger for CliLogger {
    fn log_info(&self, msg: &str) {
        self.emitter.log(msg);
    }

    fn log_debug(&self, msg: &str) {
        if self.verbose {
            self.emitter.log_debug(msg);
        }
    }

    fn log_warn(&self, msg: &str) {
        self.emitter.log_warn(msg);
    }

    fn log_phase_start(&self, phase: &str) {
        self.emitter.log_phase_start(phase);
    }

    fn log_phase_end(&self, phase: &str, elapsed_ms: u64) {
        self.emitter.log_phase_end(phase, elapsed_ms);
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

    #[test]
    fn test_emitter_clone() {
        let a = Emitter::new(OutputFormat::Json);
        let b = a.clone();
        assert_eq!(b.format, OutputFormat::Json);
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
