//! WebSocket protocol message types shared between the server and the browser.
//!
//! **All** browser ↔ server communication goes over a single `/ws` endpoint.
//! Messages are JSON objects with a discriminant `"type"` field (snake_case).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Slicing parameters sent from the browser with a `slice` request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WsSlicingParams {
    /// Layer height in mm (e.g. 0.2).
    pub layer_height: f64,
    /// Print speed in mm/s.
    pub print_speed: f64,
    /// Nozzle temperature in °C.
    pub nozzle_temp: f64,
    /// Heated-bed temperature in °C.
    pub bed_temp: f64,
    /// G-code dialect (`"marlin"` or `"klipper"`).
    pub gcode_flavor: String,
    /// Infill density as a percentage (0–100). Converted to a fraction before
    /// being forwarded to the slicing pipeline (e.g. 20 → 0.2).
    #[serde(default = "WsSlicingParams::default_infill_density")]
    pub infill_density: f64,
    /// Infill pattern name (`"rectilinear"`, `"grid"`, `"honeycomb"`, `"gyroid"`).
    #[serde(default = "WsSlicingParams::default_infill_pattern")]
    pub infill_pattern: String,
    /// Base angle in degrees for infill lines (default 45°). Alternating layers
    /// rotate by +90° on top of this base angle.
    #[serde(default = "WsSlicingParams::default_infill_angle")]
    pub infill_angle: f64,
}

impl WsSlicingParams {
    fn default_infill_density() -> f64 {
        20.0
    }

    fn default_infill_pattern() -> String {
        "rectilinear".to_string()
    }

    fn default_infill_angle() -> f64 {
        45.0
    }
}

/// Summary of a completed slicing session for history/re-download.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    /// Unique request identifier
    pub request_uuid: String,
    /// Original uploaded filename
    pub original_filename: Option<String>,
    /// Number of layers in the sliced G-code
    pub layer_count: Option<usize>,
    /// Session creation timestamp (RFC3339)
    pub created_at: String,
    /// URL to download the G-code file
    pub download_url: String,
}

/// Messages sent **from the browser to the server**.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Start a slice job. The STL file must be uploaded first via HTTP POST /api/upload,
    /// which returns a `request_uuid`. This message initiates slicing on that uploaded file.
    Slice {
        /// UUID of the uploaded request/session
        request_uuid: String,
        settings: WsSlicingParams,
    },
    /// Request a list of previously completed slicing sessions.
    ListSessions,
    /// Abort / reset the current state.
    Reset,
}

/// Messages sent **from the server to the browser**.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Sent once immediately after the WebSocket handshake completes.
    Connected { version: String },
    /// A log line for the status panel.
    Log { level: String, message: String },
    /// A performance timing marker for a pipeline phase.
    ///
    /// Emitted at the start and end of each major processing step so the
    /// browser can display elapsed times in the status panel.
    PhaseMarker {
        /// Pipeline phase name (see `slicer_engine::logging::phases`).
        phase: String,
        /// `"start"` when the phase begins; `"end"` when it completes.
        event: String,
        /// Elapsed time in milliseconds. Only present when `event` is `"end"`.
        #[serde(skip_serializing_if = "Option::is_none")]
        elapsed_ms: Option<u64>,
    },
    /// Incremental slicing progress.
    Progress {
        current_layer: usize,
        total_layers: usize,
    },
    /// Slice finished successfully. Download the G-code from the provided URL.
    SliceComplete {
        layer_count: usize,
        /// HTTP GET this URL to download the generated G-code file
        download_url: String,
    },
    /// List of previously completed slicing sessions.
    SessionsList { sessions: Vec<SessionSummary> },
    /// A fatal error occurred during processing.
    Error { message: String },
}

impl ServerMessage {
    /// Convenience constructor for an `info`-level log message.
    pub fn log_info(message: impl Into<String>) -> Self {
        Self::Log {
            level: "info".to_string(),
            message: message.into(),
        }
    }

    /// Convenience constructor for an `error` message.
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}
