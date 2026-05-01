//! SeaORM entity for the `files` table.

use sea_orm::entity::prelude::*;

/// ORM model for a row in the `files` table.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "files")]
pub struct Model {
    /// UUID stored as TEXT (RFC 4122 hyphenated string).
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_uuid: String,
    /// The workplate (request) this file belongs to.
    pub request_uuid: String,
    /// Original filename as sent by the browser.
    pub original_filename: String,
    /// On-disk path with the original extension preserved.
    pub file_path: String,
    /// File size in bytes.
    pub file_size: i64,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::requests::Entity",
        from = "Column::RequestUuid",
        to = "super::requests::Column::RequestUuid"
    )]
    Requests,
}

impl Related<super::requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Requests.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
