//! Serve command – starts a local HTTP + WebSocket server to host the Angular UI.

use clap::Parser;

/// Serve the bundled Angular UI over a local HTTP server
#[derive(Parser, Debug)]
pub struct ServeCommand {
    /// Port to listen on
    #[arg(short, long, default_value_t = 4200)]
    pub port: u16,

    /// Directory containing the built Angular app
    /// (defaults to `./ui/dist/slicer-ui/browser`)
    #[arg(long, default_value = "./ui/dist/slicer-ui/browser")]
    pub ui_dir: String,

    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

impl ServeCommand {
    /// Execute the serve command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ui_dir = std::path::PathBuf::from(&self.ui_dir);

        if !ui_dir.exists() {
            return Err(format!(
                "UI directory not found: {}\n\
                 Build the Angular app first:\n\
                 \n  cd ui && npm run build\n",
                ui_dir.display()
            )
            .into());
        }

        let host = self.host.clone();
        let port = self.port;
        let ui_dir = self.ui_dir.clone();

        eprintln!("Serving Slicer Engine UI at http://{}:{}/", host, port);
        eprintln!("WebSocket endpoint:        ws://{}:{}/ws", host, port);
        eprintln!("Serving files from: {}", ui_dir);
        eprintln!("Press Ctrl+C to stop.");

        tokio::runtime::Runtime::new()?.block_on(run_server(host, port, ui_dir))?;
        Ok(())
    }
}

async fn run_server(
    host: String,
    port: u16,
    ui_dir: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use actix_files::Files;
    use actix_web::{web, App, HttpServer};

    HttpServer::new(move || {
        let fallback_dir = ui_dir.clone();
        App::new()
            // WebSocket endpoint – must be registered before the static file handler
            .route("/ws", web::get().to(ws_handler))
            // Serve static assets; fall back to index.html for SPA navigation
            .service(
                Files::new("/", &ui_dir)
                    .index_file("index.html")
                    .default_handler(web::to(move || {
                        let path = format!("{}/index.html", fallback_dir);
                        async move {
                            actix_files::NamedFile::open_async(path)
                                .await
                                .map_err(actix_web::error::ErrorNotFound)
                        }
                    })),
            )
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}

/// Upgrade an HTTP GET to a WebSocket connection and hand off to the session
/// handler.
async fn ws_handler(
    req: actix_web::HttpRequest,
    stream: actix_web::web::Payload,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    actix_web::rt::spawn(handle_ws_session(session, msg_stream));

    Ok(response)
}

/// Drive a single WebSocket session: send the initial handshake message then
/// dispatch incoming [`ClientMessage`]s until the client disconnects.
async fn handle_ws_session(mut session: actix_ws::Session, msg_stream: actix_ws::MessageStream) {
    use crate::ws_protocol::ServerMessage;
    use futures_util::StreamExt as _;

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
                use crate::ws_protocol::ClientMessage;
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Slice { stl_b64, settings }) => {
                        handle_slice(&mut session, stl_b64, settings).await;
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
/// 1. Decode the base64-encoded STL bytes.
/// 2. Parse the mesh in a blocking thread-pool task.
/// 3. Slice and generate G-code in that same task.
/// 4. Stream log / progress / result messages back over the WebSocket.
async fn handle_slice(
    session: &mut actix_ws::Session,
    stl_b64: String,
    ws_params: crate::ws_protocol::WsSlicingParams,
) {
    use crate::ws_protocol::ServerMessage;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    macro_rules! send_or_return {
        ($msg:expr) => {
            if send_msg(session, &$msg).await.is_err() {
                return;
            }
        };
    }

    send_or_return!(ServerMessage::log_info("Decoding STL data…"));

    let stl_bytes = match STANDARD.decode(&stl_b64) {
        Ok(b) => b,
        Err(e) => {
            send_or_return!(ServerMessage::error(format!("Base64 decode failed: {e}")));
            return;
        }
    };

    send_or_return!(ServerMessage::log_info(format!(
        "STL decoded ({} bytes). Parsing mesh…",
        stl_bytes.len()
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

    tokio::task::spawn_blocking(move || {
        let mesh = match crate::mesh::io::read_stl_from_bytes(&stl_bytes) {
            Ok(m) => m,
            Err(e) => {
                let msg = ServerMessage::error(format!("Failed to parse STL: {e}"));
                let _ = tx.blocking_send(serde_json::to_string(&msg).unwrap_or_default());
                return;
            }
        };

        let face_count = mesh.faces.len();
        let log = ServerMessage::log_info(format!("Mesh loaded: {face_count} triangles. Slicing…"));
        let _ = tx.blocking_send(serde_json::to_string(&log).unwrap_or_default());

        let layers = crate::core::slice_mesh(&mesh, params.layer_height);
        let layer_count = layers.len();

        let progress = ServerMessage::Progress {
            current_layer: layer_count,
            total_layers: layer_count,
        };
        let _ = tx.blocking_send(serde_json::to_string(&progress).unwrap_or_default());

        let gcode = crate::gcode::generate_gcode(&layers, &params);
        let complete = ServerMessage::SliceComplete { gcode, layer_count };
        let _ = tx.blocking_send(serde_json::to_string(&complete).unwrap_or_default());
    });

    // Forward channel messages to the WebSocket until the task finishes
    while let Some(msg_str) = rx.recv().await {
        if session.text(msg_str).await.is_err() {
            break;
        }
    }
}

/// Serialize a [`ServerMessage`] to JSON and send it as a WebSocket text frame.
async fn send_msg(
    session: &mut actix_ws::Session,
    msg: &crate::ws_protocol::ServerMessage,
) -> Result<(), actix_ws::Closed> {
    let json = serde_json::to_string(msg).unwrap_or_default();
    session.text(json).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_command_defaults() {
        let cmd = ServeCommand {
            port: 4200,
            ui_dir: "./ui/dist/slicer-ui/browser".to_string(),
            host: "127.0.0.1".to_string(),
        };
        assert_eq!(cmd.port, 4200);
        assert_eq!(cmd.host, "127.0.0.1");
    }

    #[test]
    fn test_serve_command_missing_dir_error() {
        let cmd = ServeCommand {
            port: 4200,
            ui_dir: "/nonexistent/path/that/does/not/exist".to_string(),
            host: "127.0.0.1".to_string(),
        };
        let result = cmd.execute();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("UI directory not found"));
    }
}
