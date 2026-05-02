use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;
use slicer_engine::logging::ProcessLogger;
use tauri::{AppHandle, Emitter};

/// [`ProcessLogger`] that emits Tauri events during the slicing pipeline.
///
/// Each log line and phase marker is forwarded to the webview via
/// `app_handle.emit()`, allowing the UI to show real-time progress.
///
/// Event names:
/// - `slice-log`   → `{ level, message }`
/// - `slice-phase` → `{ phase, event: "start" | "end", elapsed_ms? }`
///
/// The `cancel_flag` is checked between pipeline phases via [`is_cancelled`].
/// Set it to `true` via `slice_cancel` to abort an in-progress slice.
///
/// [`is_cancelled`]: ProcessLogger::is_cancelled
pub struct TauriAppLogger {
    app: AppHandle,
    cancel_flag: Arc<AtomicBool>,
}

impl TauriAppLogger {
    pub fn new(app: AppHandle, cancel_flag: Arc<AtomicBool>) -> Self {
        Self { app, cancel_flag }
    }
}

impl ProcessLogger for TauriAppLogger {
    fn log_info(&self, msg: &str) {
        self.app
            .emit("slice-log", json!({ "level": "info", "message": msg }))
            .ok();
    }

    fn log_debug(&self, msg: &str) {
        self.app
            .emit("slice-log", json!({ "level": "debug", "message": msg }))
            .ok();
    }

    fn log_warn(&self, msg: &str) {
        self.app
            .emit("slice-log", json!({ "level": "warn", "message": msg }))
            .ok();
    }

    fn log_phase_start(&self, phase: &str) {
        self.app
            .emit("slice-phase", json!({ "phase": phase, "event": "start" }))
            .ok();
    }

    fn log_phase_end(&self, phase: &str, elapsed_ms: u64) {
        self.app
            .emit(
                "slice-phase",
                json!({ "phase": phase, "event": "end", "elapsed_ms": elapsed_ms }),
            )
            .ok();
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}
