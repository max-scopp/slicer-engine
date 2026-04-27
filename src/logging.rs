//! Shared logging infrastructure for the slicing pipeline.
//!
//! The [`ProcessLogger`] trait provides a uniform logging interface that all
//! pipeline entry points (CLI, WebSocket, etc.) share. There are two flavours:
//!
//! - **Global logger** ([`StderrLogger`]): always writes to stderr. Operators
//!   can tail server logs in one place regardless of which interface triggered
//!   the slicing run.
//!
//! - **Request-specific loggers** (in their respective modules):
//!   - `cli::emit::CliLogger` – routes through the CLI [`Emitter`], gating
//!     debug output on the `--verbose` flag. Because the emitter writes to
//!     stderr it naturally mirrors to the global logger.
//!   - `server::ws_session::WsLogger` – relays every message to the global
//!     stderr logger **and** sends a JSON log frame back to the WebSocket
//!     client, giving the browser the same verbosity the CLI enjoys.
//!
//! [`Emitter`]: crate::cli::emit::Emitter

/// Unified logging interface for the slicing pipeline.
///
/// Every implementation must:
/// 1. Write to the **global** stderr sink so server operators see all activity.
/// 2. Optionally relay messages to a request-specific sink (e.g. a WebSocket
///    connection or a CLI emitter with format-aware rendering).
///
/// The three levels map to familiar semantics:
/// - `log_info`  – important results that should always surface (layer count, etc.)
/// - `log_debug` – verbose step details; may be suppressed in non-verbose contexts
/// - `log_warn`  – non-fatal issues the operator should be aware of
pub trait ProcessLogger: Send + Sync {
    /// Emit an informational message that is always visible.
    fn log_info(&self, msg: &str);

    /// Emit a debug message that may be suppressed in non-verbose contexts.
    fn log_debug(&self, msg: &str);

    /// Emit a warning message that is always visible.
    fn log_warn(&self, msg: &str);
}

/// Global stderr logger.
///
/// Writes all messages unconditionally to **stderr**. This is the baseline
/// sink that every request-specific logger should also forward to, ensuring
/// that server operators can observe all slicing activity in one stream.
pub struct StderrLogger;

impl ProcessLogger for StderrLogger {
    fn log_info(&self, msg: &str) {
        eprintln!("[info] {}", msg);
    }

    fn log_debug(&self, msg: &str) {
        eprintln!("[debug] {}", msg);
    }

    fn log_warn(&self, msg: &str) {
        eprintln!("\x1b[33m[warn]\x1b[0m {}", msg);
    }
}

/// No-op logger that silently discards all messages.
///
/// Intended for unit tests where pipeline log output would be distracting or
/// irrelevant to the assertion being tested.
///
/// # Example
/// ```
/// use slicer_engine::logging::{NullLogger, ProcessLogger};
///
/// let logger = NullLogger;
/// logger.log_info("this goes nowhere");
/// logger.log_debug("also discarded");
/// logger.log_warn("silenced");
/// ```
pub struct NullLogger;

impl ProcessLogger for NullLogger {
    fn log_info(&self, _msg: &str) {}
    fn log_debug(&self, _msg: &str) {}
    fn log_warn(&self, _msg: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_logger_does_not_panic() {
        let l = NullLogger;
        l.log_info("info");
        l.log_debug("debug");
        l.log_warn("warn");
    }

    #[test]
    fn stderr_logger_does_not_panic() {
        // StderrLogger writes to stderr; we just verify it doesn't panic.
        let l = StderrLogger;
        l.log_info("info");
        l.log_debug("debug");
        l.log_warn("warn");
    }
}
