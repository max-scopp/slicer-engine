//! Settings persistence - load and save global settings from/to disk.

use std::fs;
use std::path::PathBuf;

use dirs;
use crate::settings::params::GlobalSettings;

/// Determines the configuration directory for slicer-engine settings.
/// 
/// Uses the `dirs` crate to get platform-specific config directories:
/// - Windows: `%APPDATA%\slicer-engine` (e.g., `C:\Users\User\AppData\Roaming\slicer-engine`)
/// - macOS: `~/Library/Application Support/slicer-engine`
/// - Linux: `~/.config/slicer-engine` (respects `XDG_CONFIG_HOME`)
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|path| path.join("slicer-engine"))
        .unwrap_or_else(|| PathBuf::from(".config/slicer-engine"))
}

/// Returns the path to the global settings file.
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// Load global settings from disk.
///
/// If the settings file doesn't exist, returns the default settings.
/// If the file exists but is invalid, returns an error.
pub fn load_settings() -> Result<GlobalSettings, Box<dyn std::error::Error>> {
    let path = settings_file();

    if !path.exists() {
        // File doesn't exist; return defaults
        return Ok(GlobalSettings::default());
    }

    let content = fs::read_to_string(&path)?;
    let settings: GlobalSettings = serde_json::from_str(&content)?;
    Ok(settings)
}

/// Save global settings to disk.
///
/// Creates the config directory if it doesn't exist.
pub fn save_settings(settings: &GlobalSettings) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_file();
    let dir = path.parent().expect("settings file should have parent directory");

    // Create config directory if it doesn't exist
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }

    let json = serde_json::to_string_pretty(&settings)?;
    fs::write(&path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_returns_path() {
        let dir = config_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_settings_file_has_json_extension() {
        let path = settings_file();
        assert!(path.to_string_lossy().ends_with("settings.json"));
    }

    #[test]
    fn test_settings_file_includes_config_dir() {
        let path = settings_file();
        let config = config_dir();
        assert!(path.starts_with(&config));
    }
}
