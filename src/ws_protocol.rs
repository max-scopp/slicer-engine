//! WebSocket protocol message types shared between the server and the browser.
//!
//! **All** browser ↔ server communication goes over a single `/ws` endpoint.
//! Messages are JSON objects with a discriminant `"type"` field (snake_case).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::gcode::GcodeFlavor;
use crate::infill::InfillPattern;
use crate::scene::MeshFormat;

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
    pub gcode_flavor: GcodeFlavor,
    /// Infill density as a percentage (0–100). Converted to a fraction before
    /// being forwarded to the slicing pipeline (e.g. 20 → 0.2).
    #[serde(default = "WsSlicingParams::default_infill_density")]
    pub infill_density: f64,
    /// Infill pattern name (`"rectilinear"`, `"grid"`, `"honeycomb"`, `"gyroid"`, `"tpms-d"`).
    #[serde(default)]
    pub infill_pattern: InfillPattern,
    /// Base angle in degrees for infill lines (default 45°). Alternating layers
    /// rotate by +90° on top of this base angle.
    #[serde(default = "WsSlicingParams::default_infill_angle")]
    pub infill_angle: f64,
}

impl WsSlicingParams {
    fn default_infill_density() -> f64 {
        20.0
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

/// Wire-format scene operation. Mirrors [`crate::scene::SceneOp`] but uses
/// Euler-XYZ degrees and a `file_id` reference for `Add` so payloads stay
/// human-readable JSON.
///
/// `file_id` is the upload `request_uuid` returned by `POST /api/upload`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", content = "args", rename_all = "snake_case")]
pub enum SceneOpDto {
    /// Add a mesh by reference to a previously-uploaded file.
    Add {
        name: String,
        format: MeshFormat,
        file_id: String,
    },
    /// Remove an object by id.
    Remove { id: u64 },
    /// Translate by `[x, y, z]` mm.
    Translate { id: u64, delta: [f64; 3] },
    /// Replace the full transform: translation (mm), Euler-XYZ degrees, scale.
    SetTransform {
        id: u64,
        translation: [f32; 3],
        euler_xyz_deg: [f32; 3],
        scale: [f32; 3],
    },
    /// Rotate around `axis` by `degrees`, composed with the existing rotation.
    Rotate {
        id: u64,
        axis: [f32; 3],
        degrees: f32,
    },
    /// Multiply per-axis scale by `factors`.
    Scale { id: u64, factors: [f32; 3] },
    /// Center the object on the bed in XY (preserves Z).
    CenterOnBed { id: u64 },
    /// Drop the object so its lowest Z vertex sits on Z=0.
    DropToFloor { id: u64 },
    /// Rotate so the chosen face's normal points down, then drop to floor.
    AlignFaceToFloor { id: u64, face_index: usize },
}

/// Snapshot of a scene object sent to the client (no mesh data).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneObjectDto {
    pub id: u64,
    pub name: String,
    pub translation: [f32; 3],
    pub euler_xyz_deg: [f32; 3],
    pub scale: [f32; 3],
    /// Number of triangle faces in the mesh.
    pub triangle_count: usize,
    /// World-space AABB after applying the current transform: `[min, max]`.
    pub world_aabb: [[f64; 3]; 2],
}

/// Snapshot of the bed configuration sent to the client.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BedConfigDto {
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub origin_offset_x: f64,
    pub origin_offset_y: f64,
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
    /// Apply one or more scene operations in order. Server replies with
    /// [`ServerMessage::SceneState`] on success.
    Scene { ops: Vec<SceneOpDto> },
    /// Request the current scene snapshot. Server replies with
    /// [`ServerMessage::SceneState`].
    SceneSnapshot,
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
    /// Snapshot of the per-session scene state.
    SceneState {
        objects: Vec<SceneObjectDto>,
        bed: BedConfigDto,
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
