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
    pub upload_file_size: Option<i64>,
    pub download_file_path: Option<PathBuf>,
    pub download_file_size: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single uploaded file belonging to a workplate (request).
///
/// Files are addressed by their own `file_uuid` (distinct from the workplate's
/// `request_uuid`). `file_path` retains the original extension so the slicer
/// can pick the right loader without re-encoding any format hints into the URL
/// or the wire protocol.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub file_uuid: Uuid,
    pub request_uuid: Uuid,
    pub original_filename: String,
    pub file_path: PathBuf,
    pub file_size: i64,
    pub created_at: DateTime<Utc>,
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

        -- Per-file rows for a workplate. Each upload (single file today, but
        -- the protocol is multi-file ready) writes one row here. Files are
        -- referenced by `file_uuid` in the slice protocol; `file_path` keeps
        -- the original extension so the slicer can pick the right loader
        -- without the server having to guess.
        CREATE TABLE IF NOT EXISTS files (
            file_uuid TEXT PRIMARY KEY,
            request_uuid TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (request_uuid) REFERENCES requests(request_uuid)
        );

        CREATE INDEX IF NOT EXISTS idx_files_request_uuid
            ON files(request_uuid);

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
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
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
                file_size as i64,
                RequestStatus::UploadComplete.to_db(),
                &now,
                request_uuid.to_string(),
            ],
        )?;

        Ok(())
    }

    /// Add an uploaded file row for a workplate. The single source of truth
    /// for "which files belong to which request" — slicing references files
    /// by `file_uuid`, never by `request_uuid` (the legacy "request UUID is
    /// also the file ID" convention has been removed).
    pub fn add_upload_file(
        &self,
        request_uuid: Uuid,
        file_uuid: Uuid,
        original_filename: &str,
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
            "INSERT INTO files
                (file_uuid, request_uuid, original_filename, file_path, file_size, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                file_uuid.to_string(),
                request_uuid.to_string(),
                original_filename,
                &path_str,
                file_size as i64,
                &now,
            ],
        )?;

        // Promote the workplate to UploadComplete on first file.
        conn.execute(
            "UPDATE requests
             SET status = ?, updated_at = ?
             WHERE request_uuid = ?",
            params![
                RequestStatus::UploadComplete.to_db(),
                &now,
                request_uuid.to_string(),
            ],
        )?;

        Ok(())
    }

    /// Look up a single file row by its `file_uuid`.
    pub fn get_file(&self, file_uuid: Uuid) -> Result<Option<FileEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        let mut stmt = conn.prepare(
            "SELECT file_uuid, request_uuid, original_filename, file_path, file_size, created_at
             FROM files
             WHERE file_uuid = ?",
        )?;
        let result = stmt
            .query_row([file_uuid.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .optional()?;

        match result {
            Some((
                file_uuid_str,
                request_uuid_str,
                original_filename,
                file_path,
                file_size,
                created_at_str,
            )) => Ok(Some(FileEntry {
                file_uuid: Uuid::parse_str(&file_uuid_str)?,
                request_uuid: Uuid::parse_str(&request_uuid_str)?,
                original_filename,
                file_path: PathBuf::from(file_path),
                file_size,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
            })),
            None => Ok(None),
        }
    }

    /// All files belonging to a workplate, ordered by upload time.
    pub fn get_files_for_request(&self, request_uuid: Uuid) -> Result<Vec<FileEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow!("Failed to lock database"))?;
        let mut stmt = conn.prepare(
            "SELECT file_uuid, request_uuid, original_filename, file_path, file_size, created_at
             FROM files
             WHERE request_uuid = ?
             ORDER BY created_at ASC",
        )?;

        let rows = stmt
            .query_map([request_uuid.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut out = Vec::with_capacity(rows.len());
        for (
            file_uuid_str,
            request_uuid_str,
            original_filename,
            file_path,
            file_size,
            created_at_str,
        ) in rows
        {
            out.push(FileEntry {
                file_uuid: Uuid::parse_str(&file_uuid_str)?,
                request_uuid: Uuid::parse_str(&request_uuid_str)?,
                original_filename,
                file_path: PathBuf::from(file_path),
                file_size,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
            });
        }
        Ok(out)
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
                file_size as i64,
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
            "SELECT request_uuid, upload_file_path, download_file_path
             FROM requests
             WHERE updated_at < ?",
        )?;

        let request_rows: Vec<(String, Option<String>, Option<String>)> = stmt
            .query_map([&cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Also collect file rows (new-style uploads) belonging to those requests.
        let mut files_stmt = conn.prepare("SELECT file_path FROM files WHERE request_uuid = ?")?;
        let mut on_disk_files: Vec<String> = Vec::new();
        for (request_uuid, upload_path, download_path) in &request_rows {
            if let Some(path) = upload_path {
                on_disk_files.push(path.clone());
            }
            if let Some(path) = download_path {
                on_disk_files.push(path.clone());
            }
            let file_paths: Vec<String> = files_stmt
                .query_map([request_uuid.as_str()], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            on_disk_files.extend(file_paths);
        }

        // Delete files from disk
        for path in on_disk_files {
            let _ = std::fs::remove_file(&path);
        }

        // Delete child rows first to satisfy the foreign key, then the requests.
        conn.execute(
            "DELETE FROM files
             WHERE request_uuid IN (
                 SELECT request_uuid FROM requests WHERE updated_at < ?
             )",
            [&cutoff],
        )?;

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
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
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

    /// `add_upload_file` should write a row to `files` keyed by file_uuid and
    /// promote the request to `UploadComplete`. `get_file` and
    /// `get_files_for_request` should return what we just wrote.
    #[test]
    fn test_add_and_get_files() -> Result<()> {
        let dir = TempDir::new()?;
        let db = Database::open(dir.path().join("test.db"))?;
        let request_uuid = Uuid::new_v4();
        db.create_request(request_uuid)?;

        let file_uuid = Uuid::new_v4();
        let file_path = dir.path().join(format!("{}.obj", file_uuid));
        std::fs::write(&file_path, b"dummy")?;
        db.add_upload_file(request_uuid, file_uuid, "model.obj", &file_path, 5)?;

        let entry = db.get_file(file_uuid)?.expect("file row exists");
        assert_eq!(entry.file_uuid, file_uuid);
        assert_eq!(entry.request_uuid, request_uuid);
        assert_eq!(entry.original_filename, "model.obj");
        assert_eq!(entry.file_path, file_path);
        assert_eq!(entry.file_size, 5);

        let files = db.get_files_for_request(request_uuid)?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_uuid, file_uuid);

        // Status should advance.
        let session = db.get_request(request_uuid)?.unwrap();
        assert_eq!(session.status, RequestStatus::UploadComplete);

        Ok(())
    }

    /// Cleanup must delete files-table rows and their on-disk artifacts in
    /// addition to the legacy upload/download paths in `requests`.
    #[test]
    fn test_cleanup_removes_files_table_rows() -> Result<()> {
        let dir = TempDir::new()?;
        let db = Database::open(dir.path().join("test.db"))?;
        let request_uuid = Uuid::new_v4();
        db.create_request(request_uuid)?;
        let file_uuid = Uuid::new_v4();
        let file_path = dir.path().join(format!("{}.stl", file_uuid));
        std::fs::write(&file_path, b"dummy")?;
        db.add_upload_file(request_uuid, file_uuid, "m.stl", &file_path, 5)?;

        // Force the row to be older than the cutoff.
        {
            let conn = db.conn.lock().unwrap();
            let old = (Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
            conn.execute(
                "UPDATE requests SET updated_at = ? WHERE request_uuid = ?",
                params![&old, request_uuid.to_string()],
            )?;
        }

        let removed = db.cleanup_old_sessions(24)?;
        assert_eq!(removed, 1);
        assert!(db.get_file(file_uuid)?.is_none());
        assert!(!file_path.exists());

        Ok(())
    }
}
