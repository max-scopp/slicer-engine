//! HTTP request handlers for upload, download, and configuration operations.

use actix_web::web;
use std::sync::Arc;

/// Handle file upload: save STL and return request UUID
#[derive(serde::Serialize, serde::Deserialize)]
pub struct UploadResponse {
    pub request_uuid: String,
}

pub struct AppState {
    pub db: Arc<crate::db::Database>,
    pub work_dir: std::path::PathBuf,
}

// ── Config handlers ───────────────────────────────────────────────────────────

/// Request body for `GET /api/config`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConfigResponse {
    pub config: crate::config::AppConfig,
}

/// Request body for `PATCH /api/config`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PatchConfigRequest {
    /// Dot-separated key, e.g. `"slicing.layer_height"` or `"server.port"`.
    pub key: String,
    /// New value (JSON-typed).
    pub value: serde_json::Value,
}

/// `GET /api/config` — return the fully-merged runtime configuration.
pub async fn get_config_handler() -> actix_web::HttpResponse {
    match crate::config::load_and_merge_config(None) {
        Ok(config) => actix_web::HttpResponse::Ok().json(ConfigResponse { config }),
        Err(e) => actix_web::HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// `PATCH /api/config` — update a single config key and persist to `slicer.toml`.
pub async fn patch_config_handler(body: web::Json<PatchConfigRequest>) -> actix_web::HttpResponse {
    use crate::cli::commands::config::apply_config_field;
    use crate::config::{config_file, load_config, save_config};

    let toml_path = config_file();

    let mut config = match load_config(&toml_path) {
        Ok(c) => c,
        Err(e) => {
            return actix_web::HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": e.to_string() }));
        }
    };

    if let Err(e) = apply_config_field(&mut config, &body.key, &body.value) {
        return actix_web::HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": e.to_string() }));
    }

    if let Err(e) = save_config(&config, &toml_path) {
        return actix_web::HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": e.to_string() }));
    }

    actix_web::HttpResponse::Ok().json(serde_json::json!({
        "key": body.key,
        "value": body.value,
        "message": "Configuration updated and persisted to slicer.toml",
    }))
}

/// Handle file upload: save STL and return request UUID
pub async fn upload_handler(
    state: web::Data<AppState>,
    mut multipart: actix_multipart::Multipart,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    use futures_util::StreamExt as _;
    use uuid::Uuid;

    // Generate request UUID
    let request_uuid = Uuid::new_v4();

    // Create database record
    let db = state.db.clone();
    let uuid_clone = request_uuid;
    tokio::task::spawn_blocking(move || {
        let _ = db.create_request(uuid_clone);
    })
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    // Save uploaded file
    let file_path = state.work_dir.join(format!("{}.stl", request_uuid));

    const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB limit
    let mut file_size: u64 = 0;
    let mut original_filename: Option<String> = None;

    // Process multipart fields
    while let Some(field_result) = multipart.next().await {
        let mut field = field_result.map_err(actix_web::error::ErrorBadRequest)?;

        // Only process the "file" field
        if field.name() != Some("file") {
            continue;
        }

        // Extract original filename from Content-Disposition header
        if let Some(filename) = field
            .content_disposition()
            .and_then(|cd| cd.get_filename().map(|f| f.to_string()))
        {
            original_filename = Some(filename);
        }

        let mut file = tokio::fs::File::create(&file_path)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;

        // Stream the file field data directly to disk
        while let Some(chunk_result) = field.next().await {
            let chunk = chunk_result.map_err(actix_web::error::ErrorBadRequest)?;
            file_size += chunk.len() as u64;

            if file_size > MAX_FILE_SIZE {
                let _ = tokio::fs::remove_file(&file_path).await;
                return Err(actix_web::error::ErrorPayloadTooLarge(
                    "File exceeds 500 MB limit",
                ));
            }

            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;
        }

        break; // Only process first file field
    }

    if file_size == 0 {
        let _ = tokio::fs::remove_file(&file_path).await;
        return Err(actix_web::error::ErrorBadRequest("No file uploaded"));
    }

    // Update database with file info
    let db = state.db.clone();
    let uuid_clone = request_uuid;
    let file_path_clone = file_path.clone();
    let filename = original_filename.unwrap_or_else(|| "unknown.stl".to_string());
    tokio::task::spawn_blocking(move || {
        let _ = db.set_upload_file(uuid_clone, filename, file_path_clone, file_size);
    })
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(actix_web::HttpResponse::Ok().json(UploadResponse {
        request_uuid: request_uuid.to_string(),
    }))
}

/// Handle file download: stream G-code file to browser
pub async fn download_handler(
    state: web::Data<AppState>,
    request_uuid: web::Path<String>,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let uuid_str = request_uuid.into_inner();
    let uuid = uuid::Uuid::parse_str(&uuid_str)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    // Look up session in database
    let db = state.db.clone();
    let session = tokio::task::spawn_blocking(move || db.get_request(uuid))
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Request not found"))?;

    let download_path = session
        .download_file_path
        .ok_or_else(|| actix_web::error::ErrorNotFound("G-code not ready"))?;

    // Generate download filename: replace .stl with .gcode
    let download_filename = session
        .original_filename
        .as_ref()
        .map(|f| f.replace(".stl", ".gcode").replace(".STL", ".gcode"))
        .unwrap_or_else(|| "output.gcode".to_string());

    // Read file and stream as response
    let content = tokio::fs::read(&download_path)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("G-code file not found"))?;

    Ok(actix_web::HttpResponse::Ok()
        .content_type("text/plain")
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", download_filename),
        ))
        .body(content))
}

/// Response body for `GET /api/request/:request_uuid`.
#[derive(serde::Serialize)]
pub struct RequestMetaResponse {
    pub request_uuid: String,
    pub status: String,
    pub original_filename: Option<String>,
    pub has_stl: bool,
    pub has_gcode: bool,
}

/// `GET /api/request/:request_uuid` — return metadata for a request session.
pub async fn get_request_handler(
    state: web::Data<AppState>,
    request_uuid: web::Path<String>,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let uuid_str = request_uuid.into_inner();
    let uuid = uuid::Uuid::parse_str(&uuid_str)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    let db = state.db.clone();
    let session = tokio::task::spawn_blocking(move || db.get_request(uuid))
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Request not found"))?;

    let has_stl = session
        .upload_file_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);

    let has_gcode = session
        .download_file_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);

    let status_str = format!("{:?}", session.status).to_lowercase();

    Ok(actix_web::HttpResponse::Ok().json(RequestMetaResponse {
        request_uuid: session.request_uuid.to_string(),
        status: status_str,
        original_filename: session.original_filename,
        has_stl,
        has_gcode,
    }))
}

/// `GET /api/stl/:request_uuid` — stream the uploaded STL file back to the browser.
pub async fn download_stl_handler(
    state: web::Data<AppState>,
    request_uuid: web::Path<String>,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let uuid_str = request_uuid.into_inner();
    let uuid = uuid::Uuid::parse_str(&uuid_str)
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid UUID"))?;

    let db = state.db.clone();
    let session = tokio::task::spawn_blocking(move || db.get_request(uuid))
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Request not found"))?;

    let upload_path = session
        .upload_file_path
        .ok_or_else(|| actix_web::error::ErrorNotFound("STL file not available"))?;

    if !upload_path.exists() {
        return Err(actix_web::error::ErrorNotFound(
            "STL file not found on disk",
        ));
    }

    let filename = session
        .original_filename
        .unwrap_or_else(|| "model.stl".to_string());

    let content = tokio::fs::read(&upload_path)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("STL file could not be read"))?;

    Ok(actix_web::HttpResponse::Ok()
        .content_type("application/octet-stream")
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        ))
        .body(content))
}
