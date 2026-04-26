//! Local SQLite database for tracking upload/download sessions and file metadata.
//!
//! Optimized for tracking large file operations with indexed queries for:
//! - Fast session lookup by requestUuid
//! - Cleanup of old sessions (time-based)
//! - Efficient file metadata storage (no copying file contents)

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

/// Request lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestStatus {
    /// Awaiting file upload.
    AwaitingUpload,
    /// Upload in progress.
    Uploading,
    /// Upload complete, ready to slice.
    UploadComplete,
    /// Slicing in progress.
    Slicing,
    /// Slicing complete, G-code ready for download.
    SliceComplete,
    /// Error occurred during processing.
    Error,
}

impl RequestStatus {
    /// Convert to database string representation.
    fn to_db(self) -> &'static str {
        match self {
            RequestStatus::AwaitingUpload => "awaiting_upload",
            RequestStatus::Uploading => "uploading",
            RequestStatus::UploadComplete => "upload_complete",
            RequestStatus::Slicing => "slicing",
            RequestStatus::SliceComplete => "slice_complete",
            RequestStatus::Error => "error",
        }
    }

    /// Parse from database string.
    fn from_db(s: &str) -> Result<Self> {
        Ok(match s {
            "awaiting_upload" => RequestStatus::AwaitingUpload,
            "uploading" => RequestStatus::Uploading,
            "upload_complete" => RequestStatus::UploadComplete,
            "slicing" => RequestStatus::Slicing,
            "slice_complete" => RequestStatus::SliceComplete,
            "error" => RequestStatus::Error,
            _ => return Err(anyhow!("Unknown status: {}", s)),
        })
    }
}

/// A single request session with associated upload/download files.
#[derive(Debug, Clone)]
pub struct RequestSession {
    pub request_uuid: Uuid,
    pub status: RequestStatus,
    pub original_filename: Option<String>,
    pub upload_file_path: Option<PathBuf>,
    pub upload_file_size: Option<u64>,
    pub download_file_path: Option<PathBuf>,
    pub download_file_size: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Database connection manager.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create the database at the given path. Initializes schema if needed.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize the database schema with optimized indices.
    fn init_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;

        conn.execute_batch(
            r#"
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS requests (
            request_uuid TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            original_filename TEXT,
            upload_file_path TEXT,
            upload_file_size INTEGER,
            download_file_path TEXT,
            download_file_size INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Index for status lookups (cleanup queries)
        CREATE INDEX IF NOT EXISTS idx_requests_status
            ON requests(status);

        -- Index for updated_at (time-based cleanup)
        CREATE INDEX IF NOT EXISTS idx_requests_updated_at
            ON requests(updated_at);

        -- Composite index for common queries (status + updated_at)
        CREATE INDEX IF NOT EXISTS idx_requests_status_updated
            ON requests(status, updated_at);
        "#,
        )?;
        Ok(())
    }

    /// Create a new request session.
    pub fn create_request(&self, request_uuid: Uuid) -> Result<RequestSession> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        conn.execute(
            "INSERT INTO requests
                (request_uuid, status, created_at, updated_at)
             VALUES (?, ?, ?, ?)",
            params![
                request_uuid.to_string(),
                RequestStatus::AwaitingUpload.to_db(),
                &now_str,
                &now_str,
            ],
        )?;

        Ok(RequestSession {
            request_uuid,
            status: RequestStatus::AwaitingUpload,
            original_filename: None,
            upload_file_path: None,
            upload_file_size: None,
            download_file_path: None,
            download_file_size: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Retrieve a request session by UUID.
    pub fn get_request(&self, request_uuid: Uuid) -> Result<Option<RequestSession>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        let mut stmt = conn.prepare(
            "SELECT
                request_uuid, status, original_filename,
                upload_file_path, upload_file_size,
                download_file_path, download_file_size,
                created_at, updated_at
             FROM requests
             WHERE request_uuid = ?",
        )?;

        let result = stmt
            .query_row([request_uuid.to_string()], |row| {
                let uuid_str: String = row.get(0)?;
                let status_str: String = row.get(1)?;
                let created_at_str: String = row.get(7)?;
                let updated_at_str: String = row.get(8)?;

                Ok((
                    uuid_str,
                    status_str,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<u64>>(6)?,
                    created_at_str,
                    updated_at_str,
                ))
            })
            .optional()?;

        if let Some((
            uuid_str,
            status_str,
            original_filename,
            upload_path,
            upload_size,
            download_path,
            download_size,
            created_at_str,
            updated_at_str,
        )) = result
        {
            let parsed_session = RequestSession {
                request_uuid: Uuid::parse_str(&uuid_str)?,
                status: RequestStatus::from_db(&status_str)?,
                original_filename,
                upload_file_path: upload_path.map(PathBuf::from),
                upload_file_size: upload_size,
                download_file_path: download_path.map(PathBuf::from),
                download_file_size: download_size,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at_str)?.with_timezone(&Utc),
            };
            Ok(Some(parsed_session))
        } else {
            Ok(None)
        }
    }

    /// Update request status.
    pub fn update_status(&self, request_uuid: Uuid, new_status: RequestStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        let rows = conn.execute(
            "UPDATE requests
             SET status = ?, updated_at = ?
             WHERE request_uuid = ?",
            params![new_status.to_db(), &now, request_uuid.to_string()],
        )?;

        if rows == 0 {
            return Err(anyhow!("Request not found: {}", request_uuid));
        }
        Ok(())
    }

    /// Record an uploaded file (STL).
    pub fn set_upload_file(
        &self,
        request_uuid: Uuid,
        original_filename: String,
        file_path: impl AsRef<Path>,
        file_size: u64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let path_str = file_path.as_ref().to_string_lossy().to_string();
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;

        conn.execute(
            "UPDATE requests
             SET original_filename = ?, upload_file_path = ?, upload_file_size = ?,
                 status = ?, updated_at = ?
             WHERE request_uuid = ?",
            params![
                &original_filename,
                &path_str,
                file_size,
                RequestStatus::UploadComplete.to_db(),
                &now,
                request_uuid.to_string(),
            ],
        )?;

        Ok(())
    }

    /// Record a downloaded/generated file (G-code).
    pub fn set_download_file(
        &self,
        request_uuid: Uuid,
        file_path: impl AsRef<Path>,
        file_size: u64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let path_str = file_path.as_ref().to_string_lossy().to_string();
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;

        conn.execute(
            "UPDATE requests
             SET download_file_path = ?, download_file_size = ?,
                 status = ?, updated_at = ?
             WHERE request_uuid = ?",
            params![
                &path_str,
                file_size,
                RequestStatus::SliceComplete.to_db(),
                &now,
                request_uuid.to_string(),
            ],
        )?;

        Ok(())
    }

    /// Delete old sessions (older than the specified number of hours).
    /// Also deletes associated files from disk.
    pub fn cleanup_old_sessions(&self, hours_old: i64) -> Result<usize> {
        let cutoff = Utc::now()
            .checked_sub_signed(chrono::Duration::hours(hours_old))
            .ok_or_else(|| anyhow!("Invalid duration"))?
            .to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;

        // Fetch old records before deleting
        let mut stmt = conn.prepare(
            "SELECT upload_file_path, download_file_path
             FROM requests
             WHERE updated_at < ?",
        )?;

        let files_to_delete: Vec<(Option<String>, Option<String>)> = stmt
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Delete files from disk
        for (upload_path, download_path) in files_to_delete {
            if let Some(path) = upload_path {
                let _ = std::fs::remove_file(&path);
            }
            if let Some(path) = download_path {
                let _ = std::fs::remove_file(&path);
            }
        }

        // Delete database records
        let rows = conn.execute(
            "DELETE FROM requests
             WHERE updated_at < ?",
            [&cutoff],
        )?;

        Ok(rows)
    }

    /// Get all sessions with a specific status (useful for monitoring/debugging).
    pub fn get_sessions_by_status(&self, status: RequestStatus) -> Result<Vec<RequestSession>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        let mut stmt = conn.prepare(
            "SELECT
                request_uuid, status, original_filename,
                upload_file_path, upload_file_size,
                download_file_path, download_file_size,
                created_at, updated_at
             FROM requests
             WHERE status = ?
             ORDER BY updated_at DESC",
        )?;

        let sessions_iter = stmt.query_map([status.to_db()], |row| {
            let uuid_str: String = row.get(0)?;
            let status_str: String = row.get(1)?;
            let created_at_str: String = row.get(7)?;
            let updated_at_str: String = row.get(8)?;

            Ok((
                uuid_str,
                status_str,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<u64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<u64>>(6)?,
                created_at_str,
                updated_at_str,
            ))
        })?;

        let mut sessions = Vec::new();
        for row_result in sessions_iter {
            let (
                uuid_str,
                status_str,
                original_filename,
                upload_path,
                upload_size,
                download_path,
                download_size,
                created_at_str,
                updated_at_str,
            ) = row_result?;
            sessions.push(RequestSession {
                request_uuid: Uuid::parse_str(&uuid_str)?,
                status: RequestStatus::from_db(&status_str)?,
                original_filename,
                upload_file_path: upload_path.map(PathBuf::from),
                upload_file_size: upload_size,
                download_file_path: download_path.map(PathBuf::from),
                download_file_size: download_size,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at_str)?.with_timezone(&Utc),
            });
        }

        Ok(sessions)
    }

    /// Get all completed slicing sessions, ordered by most recent first.
    pub fn get_completed_sessions(&self) -> Result<Vec<RequestSession>> {
        self.get_sessions_by_status(RequestStatus::SliceComplete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_retrieve_request() -> Result<()> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path)?;

        let uuid = Uuid::new_v4();
        let session = db.create_request(uuid)?;

        assert_eq!(session.request_uuid, uuid);
        assert_eq!(session.status, RequestStatus::AwaitingUpload);

        let retrieved = db.get_request(uuid)?.unwrap();
        assert_eq!(retrieved.request_uuid, uuid);
        assert_eq!(retrieved.status, RequestStatus::AwaitingUpload);

        Ok(())
    }

    #[test]
    fn test_update_status() -> Result<()> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path)?;

        let uuid = Uuid::new_v4();
        db.create_request(uuid)?;
        db.update_status(uuid, RequestStatus::Slicing)?;

        let retrieved = db.get_request(uuid)?.unwrap();
        assert_eq!(retrieved.status, RequestStatus::Slicing);

        Ok(())
    }

    #[test]
    fn test_set_upload_file() -> Result<()> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path)?;

        let uuid = Uuid::new_v4();
        db.create_request(uuid)?;

        let upload_path = dir.path().join("test.stl");
        std::fs::write(&upload_path, b"test")?;

        db.set_upload_file(uuid, "test.stl".to_string(), &upload_path, 1024)?;

        let retrieved = db.get_request(uuid)?.unwrap();
        assert_eq!(retrieved.upload_file_size, Some(1024));
        assert_eq!(
            retrieved.upload_file_path.as_ref().map(|p| p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()),
            Some("test.stl".to_string())
        );

        Ok(())
    }
}
