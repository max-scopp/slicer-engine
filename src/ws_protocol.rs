//! WebSocket protocol message types shared between the server and the browser.
//!
//! **All** browser ↔ server communication goes over a single `/ws` endpoint.
//! Messages are JSON objects with a discriminant `"type"` field (snake_case).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::scene::MeshFormat;
use crate::settings::params::SlicingParams;

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

/// Affine transform encoded for the protocol boundary using Euler-XYZ degrees.
///
/// Mirrors the `from_euler_xyz_deg` view of [`crate::scene::Transform`] so
/// payloads stay human-readable JSON. Defaults to the identity transform so
/// callers may omit any field.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TransformDto {
    /// Translation in millimeters.
    #[serde(default = "TransformDto::default_zero3")]
    pub translation: [f32; 3],
    /// Rotation as intrinsic Euler-XYZ angles in **degrees**.
    #[serde(default = "TransformDto::default_zero3")]
    pub euler_xyz_deg: [f32; 3],
    /// Per-axis scale factors.
    #[serde(default = "TransformDto::default_one3")]
    pub scale: [f32; 3],
}

impl TransformDto {
    fn default_zero3() -> [f32; 3] {
        [0.0; 3]
    }
    fn default_one3() -> [f32; 3] {
        [1.0; 3]
    }
}

impl Default for TransformDto {
    fn default() -> Self {
        Self {
            translation: Self::default_zero3(),
            euler_xyz_deg: Self::default_zero3(),
            scale: Self::default_one3(),
        }
    }
}

/// One placed object in a slice request: which uploaded mesh to use, what
/// format it is, and the transform (translation / rotation / scale) the
/// frontend currently has applied to it.
///
/// `file_id` is the upload `request_uuid` returned by `POST /api/upload`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneObjectSliceDto {
    /// Upload identifier (`request_uuid` from `POST /api/upload`).
    pub file_id: String,
    /// Source mesh format on disk.
    #[serde(default = "SceneObjectSliceDto::default_format")]
    pub format: MeshFormat,
    /// Transform to bake into the mesh before slicing.
    #[serde(default)]
    pub transform: TransformDto,
}

impl SceneObjectSliceDto {
    fn default_format() -> MeshFormat {
        MeshFormat::Stl
    }
}

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
    /// Start a slice job.
    ///
    /// Two ways to specify what gets sliced:
    ///
    /// - **`scene`** (preferred): a list of placed objects with their
    ///   transforms. Each entry references a previously-uploaded file by
    ///   `file_id`. The server bakes every object's transform and merges the
    ///   resulting meshes before slicing. This is the path the UI uses so
    ///   user-applied translate/rotate/scale/center/drop-to-floor are honoured.
    /// - **`request_uuid` only**: legacy single-file path that slices the
    ///   uploaded mesh as-is with no transform. Kept for tooling that hasn't
    ///   been updated to the scene-based protocol.
    ///
    /// `request_uuid` is always required because the server uses it to track
    /// the resulting G-code download.
    Slice {
        /// UUID of the uploaded request/session — also the key the resulting
        /// G-code is stored against.
        request_uuid: String,
        /// Optional placed-object scene. When omitted, the server falls back
        /// to slicing `request_uuid`'s upload with the identity transform.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scene: Option<Vec<SceneObjectSliceDto>>,
        settings: Box<SlicingParams>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The slice request must round-trip through serde with an explicit `scene`
    /// list so the server can honour user-applied transforms. The legacy
    /// `request_uuid`-only shape (no `scene`) must still parse for backward
    /// compatibility with tooling that hasn't been updated.
    #[test]
    fn slice_message_with_scene_round_trips() {
        let json = r#"{
            "type": "Slice",
            "request_uuid": "00000000-0000-0000-0000-000000000001",
            "scene": [{
                "file_id": "00000000-0000-0000-0000-000000000001",
                "format": "stl",
                "transform": {
                    "translation": [10.0, 20.0, 0.0],
                    "euler_xyz_deg": [0.0, 0.0, 90.0],
                    "scale": [1.0, 1.0, 1.0]
                }
            }],
            "settings": {}
        }"#;
        let parsed: ClientMessage = serde_json::from_str(json).expect("parse");
        match parsed {
            ClientMessage::Slice {
                scene: Some(objs), ..
            } => {
                assert_eq!(objs.len(), 1);
                assert_eq!(objs[0].transform.translation, [10.0, 20.0, 0.0]);
                assert_eq!(objs[0].transform.euler_xyz_deg, [0.0, 0.0, 90.0]);
            }
            _ => panic!("expected Slice with scene"),
        }
    }

    /// Legacy single-file `Slice` request (no `scene`) must still parse.
    #[test]
    fn slice_message_without_scene_round_trips() {
        let json = r#"{
            "type": "Slice",
            "request_uuid": "00000000-0000-0000-0000-000000000002",
            "settings": {}
        }"#;
        let parsed: ClientMessage = serde_json::from_str(json).expect("parse");
        match parsed {
            ClientMessage::Slice { scene, .. } => assert!(scene.is_none()),
            _ => panic!("expected Slice"),
        }
    }

    /// `infill_density` is a fraction (0.0–1.0) at the wire level — nothing
    /// is divided by 100 server-side. The previous percent-style protocol
    /// silently produced essentially-zero infill when the UI sent fractions.
    #[test]
    fn slicing_params_infill_density_is_a_fraction() {
        let json = r#"{
            "type": "Slice",
            "request_uuid": "00000000-0000-0000-0000-000000000003",
            "settings": { "infill_density": 0.3 }
        }"#;
        let parsed: ClientMessage = serde_json::from_str(json).expect("parse");
        match parsed {
            ClientMessage::Slice { settings, .. } => {
                assert!((settings.infill_density - 0.3).abs() < 1e-9);
            }
            _ => panic!("expected Slice"),
        }
    }
}
