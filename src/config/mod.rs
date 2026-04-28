//! Centralized TOML configuration system.
//!
//! # Overview
//! `AppConfig` is the single source of truth persisted to `slicer.toml`.
//! It is loaded on startup and can be written back whenever a setting changes
//! from CLI, API, or UI.
//!
//! # File Locations and Precedence
//! 1. **Compiled-in defaults** — `AppConfig::default()`
//! 2. **Global user config** — `~/.config/slicer-engine/slicer.toml`
//! 3. **Project config** — `./slicer.toml` in the current working directory
//! 4. **CLI arguments** — override everything at runtime (not persisted)
//!
//! # Modules
//! - [`types`]: Data structures (`AppConfig`, `SlicingConfig`, `ServerConfig`, …)
//! - [`io`]: `load_config`, `save_config`, `load_and_merge_config`, path helpers

pub mod io;
pub mod types;

pub use io::{
    config_dir, config_file, find_project_config_toml, load_and_merge_config, load_config,
    save_config,
};
pub use types::{
    AppConfig, GlobalConfig, MachineConfig, MaterialProfile, ProfilesConfig, ServerConfig,
    SlicingConfig, SlicingPreset,
};
