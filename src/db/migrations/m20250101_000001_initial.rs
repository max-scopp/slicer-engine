//! Initial migration — creates the `requests` and `files` tables together
//! with their indices. This migration records the schema that previously lived
//! in `Database::init_schema` (the raw `CREATE TABLE IF NOT EXISTS` batch),
//! making it a proper, reversible, version-controlled migration.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20250101_000001_initial"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Enable WAL mode for better write concurrency.
        manager
            .get_connection()
            .execute_unprepared("PRAGMA journal_mode = WAL")
            .await?;

        // ── requests ──────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Requests::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Requests::RequestUuid)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Requests::Status).string().not_null())
                    .col(ColumnDef::new(Requests::DownloadFilePath).string().null())
                    .col(
                        ColumnDef::new(Requests::DownloadFileSize)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(Requests::CreatedAt).string().not_null())
                    .col(ColumnDef::new(Requests::UpdatedAt).string().not_null())
                    .to_owned(),
            )
            .await?;

        // ── files ─────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Files::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Files::FileUuid)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Files::RequestUuid).string().not_null())
                    .col(ColumnDef::new(Files::OriginalFilename).string().not_null())
                    .col(ColumnDef::new(Files::FilePath).string().not_null())
                    .col(ColumnDef::new(Files::FileSize).big_integer().not_null())
                    .col(ColumnDef::new(Files::CreatedAt).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Files::Table, Files::RequestUuid)
                            .to(Requests::Table, Requests::RequestUuid),
                    )
                    .to_owned(),
            )
            .await?;

        // ── indices ───────────────────────────────────────────────────────────
        manager
            .create_index(
                Index::create()
                    .name("idx_files_request_uuid")
                    .table(Files::Table)
                    .col(Files::RequestUuid)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_requests_status")
                    .table(Requests::Table)
                    .col(Requests::Status)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_requests_updated_at")
                    .table(Requests::Table)
                    .col(Requests::UpdatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_requests_status_updated")
                    .table(Requests::Table)
                    .col(Requests::Status)
                    .col(Requests::UpdatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop indices first (implicit in SQLite, but explicit for other DBs).
        for idx in [
            "idx_requests_status_updated",
            "idx_requests_updated_at",
            "idx_requests_status",
            "idx_files_request_uuid",
        ] {
            manager
                .drop_index(Index::drop().name(idx).to_owned())
                .await?;
        }

        manager
            .drop_table(Table::drop().table(Files::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Requests::Table).to_owned())
            .await?;

        Ok(())
    }
}

// ── Table / column identifiers used by the schema builder ─────────────────────

#[derive(Iden)]
enum Requests {
    Table,
    RequestUuid,
    Status,
    DownloadFilePath,
    DownloadFileSize,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Files {
    Table,
    FileUuid,
    RequestUuid,
    OriginalFilename,
    FilePath,
    FileSize,
    CreatedAt,
}
