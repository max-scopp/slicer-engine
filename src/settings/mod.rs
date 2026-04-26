//! Printer profiles, slicing parameters, and settings validation.
//!
//! # Modules
//! - [`profile`]: [`PrinterProfile`] — hardware constraints
//! - [`params`]: [`SlicingParams`], [`GlobalSettings`], [`ObjectSettings`]
//! - [`validator`]: [`SettingValidator`] trait + [`ValidationRules`] stubs
//! - [`diff`]: [`SettingsDiff`] struct + [`compare_settings`] function
//! - [`persistence`]: Settings file I/O — [`load_settings`], [`save_settings`]

pub mod diff;
pub mod params;
pub mod persistence;
pub mod profile;
pub mod validator;

pub use diff::{compare_settings, SettingsDiff};
pub use params::{GlobalSettings, ObjectSettings, SlicingParams};
pub use persistence::{
    config_dir, find_project_config, load_and_merge_settings, load_project_config,
    load_settings, merge_json_configs, save_settings, settings_file,
};
pub use profile::PrinterProfile;
pub use validator::{SettingValidator, ValidationRules};
