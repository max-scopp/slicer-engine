//! CLI command implementations

pub mod gen_schemas;
pub mod info;
pub mod settings;
pub mod slice;

pub use crate::server::ServeCommand;
pub use gen_schemas::GenSchemasCommand;
pub use info::InfoCommand;
pub use settings::SettingsCommand;
pub use slice::SliceCommand;
