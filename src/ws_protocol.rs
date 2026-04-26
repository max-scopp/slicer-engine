//! WebSocket protocol message types shared between the server and the browser.
//!
//! **All** browser ↔ server communication goes over a single `/ws` endpoint.
//! Messages are JSON objects with a discriminant `"type"` field (snake_case).

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

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
