//! WebSocket session management and message handling.

use crate::logging::{phases, PhaseTimer, ProcessLogger, StderrLogger};
use crate::ws_protocol::{ClientMessage, ServerMessage};
use futures_util::StreamExt as _;
use std::sync::Arc;
use uuid::Uuid;

/// A [`ProcessLogger`] that relays every message to the global stderr logger
/// *and* sends a JSON [`ServerMessage::Log`] frame to the connected WebSocket
/// client.
///
/// This gives WebSocket clients the same level of pipeline verbosity that the
/// CLI exposes via `--verbose`, without any special-casing inside the slicing
/// pipeline itself.
struct WsLogger {
    global: StderrLogger,
    tx: tokio::sync::mpsc::Sender<String>,
}

impl WsLogger {
    fn new(tx: tokio::sync::mpsc::Sender<String>) -> Self {
        Self {
            global: StderrLogger,
            tx,
        }
    }

    fn send_log(&self, level: &str, msg: &str) {
        let server_msg = ServerMessage::Log {
            level: level.to_string(),
            message: msg.to_string(),
        };
        let json = serde_json::to_string(&server_msg).unwrap_or_else(|_| {
            format!(
                r#"{{"type":"log","level":"{}","message":"<serialization error>"}}"#,
                level
            )
        });
        // `WsLogger` is exclusively constructed and used inside
        // `tokio::task::spawn_blocking`, so `blocking_send` is safe here and
        // will not stall an async executor thread.
        let _ = self.tx.blocking_send(json);
    }
}

impl ProcessLogger for WsLogger {
    fn log_info(&self, msg: &str) {
        self.global.log_info(msg);
        self.send_log("info", msg);
    }

    fn log_debug(&self, msg: &str) {
        self.global.log_debug(msg);
        self.send_log("debug", msg);
    }

    fn log_warn(&self, msg: &str) {
        self.global.log_warn(msg);
        self.send_log("warn", msg);
    }

    fn log_phase_start(&self, phase: &str) {
        self.global.log_phase_start(phase);
        let server_msg = crate::ws_protocol::ServerMessage::PhaseMarker {
            phase: phase.to_string(),
            event: "start".to_string(),
            elapsed_ms: None,
        };
        let json = serde_json::to_string(&server_msg).unwrap_or_else(|_| {
            format!(
                r#"{{"type":"PhaseMarker","phase":"{}","event":"start"}}"#,
                phase
            )
        });
        let _ = self.tx.blocking_send(json);
    }

    fn log_phase_end(&self, phase: &str, elapsed_ms: u64) {
        self.global.log_phase_end(phase, elapsed_ms);
        let server_msg = crate::ws_protocol::ServerMessage::PhaseMarker {
            phase: phase.to_string(),
            event: "end".to_string(),
            elapsed_ms: Some(elapsed_ms),
        };
        let json = serde_json::to_string(&server_msg).unwrap_or_else(|_| {
            format!(
                r#"{{"type":"PhaseMarker","phase":"{}","event":"end","elapsed_ms":{}}}"#,
                phase, elapsed_ms
            )
        });
        let _ = self.tx.blocking_send(json);
    }
}

/// Upgrade an HTTP GET to a WebSocket connection and hand off to the session handler
pub async fn ws_handler(
    req: actix_web::HttpRequest,
    stream: actix_web::web::Payload,
    state: actix_web::web::Data<super::handlers::AppState>,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    let db = state.db.clone();
    let work_dir = state.work_dir.clone();

    actix_web::rt::spawn(handle_ws_session(session, msg_stream, db, work_dir));

    Ok(response)
}

/// Drive a single WebSocket session: send the initial handshake message then
/// dispatch incoming [`ClientMessage`]s until the client disconnects.
async fn handle_ws_session(
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
    db: Arc<crate::db::Database>,
    work_dir: std::path::PathBuf,
) {
    // Aggregate WebSocket continuation frames (limit: 64 MiB per message)
    let mut stream = msg_stream
        .aggregate_continuations()
        .max_continuation_size(64 * 1024 * 1024);

    // Announce server version on connect
    let hello = ServerMessage::Connected {
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if send_msg(&mut session, &hello).await.is_err() {
        return;
    }

    while let Some(Ok(msg)) = stream.next().await {
        use actix_ws::AggregatedMessage;
        match msg {
            AggregatedMessage::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Slice {
                    request_uuid,
                    settings,
                }) => {
                    handle_slice(
                        &mut session,
                        request_uuid,
                        settings,
                        db.clone(),
                        work_dir.clone(),
                    )
                    .await;
                }
                Ok(ClientMessage::ListSessions) => {
                    handle_list_sessions(&mut session, db.clone()).await;
                }
                Ok(ClientMessage::Reset) => {
                    let _ = send_msg(&mut session, &ServerMessage::log_info("Reset.")).await;
                }
                Err(e) => {
                    let _ = send_msg(
                        &mut session,
                        &ServerMessage::error(format!("Unrecognised message: {e}")),
                    )
                    .await;
                }
            },
            AggregatedMessage::Close(_) => break,
            _ => {}
        }
    }

    let _ = session.close(None).await;
}

/// Process a slice request from the browser:
///
/// 1. Retrieve uploaded STL file from disk.
/// 2. Parse the mesh in a blocking thread-pool task.
/// 3. Slice and generate G-code in that same task.
/// 4. Save G-code to disk.
/// 5. Stream log / progress / result messages back over the WebSocket.
async fn handle_slice(
    session: &mut actix_ws::Session,
    request_uuid: String,
    ws_params: crate::ws_protocol::WsSlicingParams,
    db: Arc<crate::db::Database>,
    work_dir: std::path::PathBuf,
) {
    macro_rules! send_or_return {
        ($msg:expr) => {
            if send_msg(session, &$msg).await.is_err() {
                return;
            }
        };
    }

    // Parse request UUID
    let uuid = match Uuid::parse_str(&request_uuid) {
        Ok(u) => u,
        Err(e) => {
            send_or_return!(ServerMessage::error(format!("Invalid request UUID: {e}")));
            return;
        }
    };

    // Fetch session from database
    let session_result = {
        let db = db.clone();
        tokio::task::spawn_blocking(move || db.get_request(uuid)).await
    };

    let session_info = match session_result {
        Ok(Ok(Some(s))) => s,
        Ok(Ok(None)) => {
            send_or_return!(ServerMessage::error("Request not found in database"));
            return;
        }
        Ok(Err(e)) => {
            send_or_return!(ServerMessage::error(format!("Database error: {e}")));
            return;
        }
        Err(e) => {
            send_or_return!(ServerMessage::error(format!("Task error: {e}")));
            return;
        }
    };

    let stl_file_path = match session_info.upload_file_path {
        Some(p) => p,
        None => {
            send_or_return!(ServerMessage::error(
                "No upload file associated with request"
            ));
            return;
        }
    };

    send_or_return!(ServerMessage::log_info(format!(
        "Loading STL from {}: {} bytes…",
        stl_file_path.display(),
        session_info.upload_file_size.unwrap_or(0)
    )));

    use crate::settings::params::SlicingParams;
    let params = SlicingParams {
        layer_height: ws_params.layer_height,
        print_speed: ws_params.print_speed,
        nozzle_temp: ws_params.nozzle_temp,
        bed_temp: ws_params.bed_temp,
        ..SlicingParams::default()
    };

    // Run blocking work (mesh parse + slice + G-code gen) on the thread pool.
    // Messages are forwarded to the WebSocket via an mpsc channel.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let gcode_output_path = work_dir.join(format!("{}.gcode", uuid));
    let gcode_output_path_clone = gcode_output_path.clone();

    tokio::task::spawn_blocking(move || {
        /// Serializes `msg` to JSON; returns a hard-coded error frame on failure.
        fn to_json(msg: &ServerMessage) -> String {
            serde_json::to_string(msg).unwrap_or_else(|_| {
                r#"{"type":"error","message":"Internal error: failed to serialize message"}"#
                    .to_owned()
            })
        }

        // Build the request-specific logger early so it can cover all phases
        // including mesh loading.  Every pipeline message is sent back to the
        // client as a Log/PhaseMarker frame and is also written to stderr.
        let logger = WsLogger::new(tx.clone());

        let stl_bytes = match std::fs::read(&stl_file_path) {
            Ok(b) => b,
            Err(e) => {
                let msg = ServerMessage::error(format!("Failed to read STL file: {e}"));
                let _ = tx.blocking_send(to_json(&msg));
                return;
            }
        };

        let t_load = PhaseTimer::start(phases::MESH_LOAD, &logger);
        let mesh = match crate::mesh::io::read_stl_from_bytes(&stl_bytes) {
            Ok(m) => m,
            Err(e) => {
                let msg = ServerMessage::error(format!(
                    "Failed to parse STL (unsupported format or corrupted file): {e}"
                ));
                let _ = tx.blocking_send(to_json(&msg));
                return;
            }
        };
        t_load.finish();

        let layers = crate::core::process_mesh(&mesh, &params, &logger);
        let layer_count = layers.len();

        let progress = ServerMessage::Progress {
            current_layer: layer_count,
            total_layers: layer_count,
        };
        let _ = tx.blocking_send(to_json(&progress));

        let t_gcode = PhaseTimer::start(phases::GCODE_GENERATION, &logger);
        let gcode = crate::gcode::generate_gcode(&layers, &params);
        t_gcode.finish();

        // Write G-code to disk
        let t_write = PhaseTimer::start(phases::FILE_WRITE, &logger);
        if let Err(e) = std::fs::write(&gcode_output_path_clone, &gcode) {
            let msg = ServerMessage::error(format!("Failed to write G-code file: {e}"));
            let _ = tx.blocking_send(to_json(&msg));
            return;
        }
        t_write.finish();

        let complete = ServerMessage::SliceComplete {
            layer_count,
            download_url: format!("/api/download/{}", uuid),
        };
        let _ = tx.blocking_send(to_json(&complete));
    });

    // Forward channel messages to the WebSocket until the task finishes
    while let Some(msg_str) = rx.recv().await {
        if session.text(msg_str).await.is_err() {
            break;
        }
    }

    // Update database with G-code file info
    if let Ok(file_size) = std::fs::metadata(&gcode_output_path).map(|m| m.len()) {
        let db = db.clone();
        let _ = tokio::task::spawn_blocking(move || {
            db.set_download_file(uuid, &gcode_output_path, file_size)
        })
        .await;
    }
}

/// Fetch and send a list of previously completed slicing sessions.
async fn handle_list_sessions(
    session: &mut actix_ws::Session,
    db: std::sync::Arc<crate::db::Database>,
) {
    // Query database for completed sessions
    let sessions = tokio::task::spawn_blocking(move || db.get_completed_sessions())
        .await
        .ok()
        .and_then(|result| result.ok())
        .unwrap_or_default();

    // Convert RequestSession to SessionSummary
    let summaries = sessions
        .into_iter()
        .map(|session| {
            use crate::ws_protocol::SessionSummary;
            SessionSummary {
                request_uuid: session.request_uuid.to_string(),
                original_filename: session.original_filename,
                layer_count: session.download_file_size.map(|size| size as usize),
                created_at: session.created_at.to_rfc3339(),
                download_url: format!("/api/download/{}", session.request_uuid),
            }
        })
        .collect::<Vec<_>>();

    let msg = ServerMessage::SessionsList {
        sessions: summaries,
    };
    let _ = send_msg(session, &msg).await;
}

/// Serialize a [`ServerMessage`] to JSON and send it as a WebSocket text frame.
///
/// Falls back to a hard-coded error JSON string in the (very unlikely) event
/// that serialization itself fails, ensuring the client always receives valid
/// JSON rather than an empty frame.
async fn send_msg(
    session: &mut actix_ws::Session,
    msg: &ServerMessage,
) -> Result<(), actix_ws::Closed> {
    const SERIALIZATION_ERROR: &str =
        r#"{"type":"error","message":"Internal error: failed to serialize message"}"#;
    let json = serde_json::to_string(msg).unwrap_or_else(|_| SERIALIZATION_ERROR.to_owned());
    session.text(json).await
}
