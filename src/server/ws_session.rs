//! WebSocket session management and message handling.

use crate::ws_protocol::{ClientMessage, ServerMessage};
use futures_util::StreamExt as _;
use std::sync::Arc;
use uuid::Uuid;

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
            AggregatedMessage::Text(text) => {
                match serde_json::from_str::<ClientMessage>(&text) {
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
                }
            }
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
        tokio::task::spawn_blocking(move || db.get_request(uuid))
            .await
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
            send_or_return!(ServerMessage::error("No upload file associated with request"));
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

        let stl_bytes = match std::fs::read(&stl_file_path) {
            Ok(b) => b,
            Err(e) => {
                let msg = ServerMessage::error(format!("Failed to read STL file: {e}"));
                let _ = tx.blocking_send(to_json(&msg));
                return;
            }
        };

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

        let face_count = mesh.faces.len();
        let log =
            ServerMessage::log_info(format!("Mesh loaded: {face_count} triangles. Slicing…"));
        let _ = tx.blocking_send(to_json(&log));

        let layers = crate::core::slice_mesh(&mesh, params.layer_height);
        let layer_count = layers.len();

        let progress = ServerMessage::Progress {
            current_layer: layer_count,
            total_layers: layer_count,
        };
        let _ = tx.blocking_send(to_json(&progress));

        let gcode = crate::gcode::generate_gcode(&layers, &params);

        // Write G-code to disk
        if let Err(e) = std::fs::write(&gcode_output_path_clone, &gcode) {
            let msg = ServerMessage::error(format!("Failed to write G-code file: {e}"));
            let _ = tx.blocking_send(to_json(&msg));
            return;
        }

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
