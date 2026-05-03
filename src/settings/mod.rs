//! Printer profiles, slicing parameters, and settings validation.
//!
//! # Modules
//! - [`profile`]: [`PrinterProfile`] — hardware constraints
//! - [`params`]: [`SlicingParams`], [`ObjectSettings`], [`LifecycleMarkerConfig`]
//! - [`validator`]: [`SettingValidator`] trait + [`ValidationRules`] stubs
//! - [`diff`]: [`SettingsDiff`] struct + [`compare_settings`] function

pub mod diff;
pub mod params;
pub mod profile;
pub mod validator;

pub use diff::{compare_settings, SettingsDiff};
pub use params::{FanConfig, LifecycleMarkerConfig, ObjectSettings, SlicingParams};
pub use profile::PrinterProfile;
pub use validator::{SettingValidator, ValidationRules};
