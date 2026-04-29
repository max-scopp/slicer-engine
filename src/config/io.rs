//! Config file I/O — load, save, and path helpers.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::types::AppConfig;

/// Returns the platform-specific configuration directory for slicer-engine.
///
/// - Windows: `%APPDATA%\slicer-engine`
/// - macOS:   `~/Library/Application Support/slicer-engine`
/// - Linux:   `~/.config/slicer-engine` (respects `XDG_CONFIG_HOME`)
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("slicer-engine"))
        .unwrap_or_else(|| PathBuf::from(".config/slicer-engine"))
}

/// Returns the path to the global TOML config file.
pub fn config_file() -> PathBuf {
    config_dir().join("slicer.toml")
}

/// Searches for a `slicer.toml` in the current working directory.
///
/// Returns the path if found, `None` otherwise.
pub fn find_project_config_toml() -> Option<PathBuf> {
    let path = std::env::current_dir().ok()?.join("slicer.toml");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Load `AppConfig` from a specific TOML file.
///
/// Returns the default `AppConfig` if the file does not exist.  Returns an
/// error if the file exists but cannot be parsed.
pub fn load_config(path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Cannot read config '{}': {}", path.display(), e))?;
    let config: AppConfig = toml::from_str(&content)
        .map_err(|e| format!("Invalid TOML in '{}': {}", path.display(), e))?;
    Ok(config)
}

/// Persist `AppConfig` to a TOML file.
///
/// Creates intermediate directories as needed.
pub fn save_config(config: &AppConfig, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create config dir '{}': {}", parent.display(), e))?;
        }
    }

    let content =
        toml::to_string_pretty(config).map_err(|e| format!("Cannot serialize config: {}", e))?;
    fs::write(path, content)
        .map_err(|e| format!("Cannot write config '{}': {}", path.display(), e))?;
    Ok(())
}

/// Load and merge configuration following the precedence hierarchy:
///
/// 1. Compiled-in defaults
/// 2. Global user config (`~/.config/slicer-engine/slicer.toml`)
/// 3. Project config (`./slicer.toml` or `project_config_path` when provided)
///
/// Pass `None` for `project_config_path` to trigger automatic discovery.
/// Pass `Some(path)` to use an explicit file (skip if the file does not exist).
pub fn load_and_merge_config(
    project_config_path: Option<&Path>,
) -> Result<AppConfig, Box<dyn std::error::Error>> {
    // Layer 1: built-in defaults
    let mut merged = AppConfig::default();

    // Layer 2: global user config
    let global_path = config_file();
    if global_path.exists() {
        let global = load_config(&global_path)?;
        merge_config(&mut merged, global);
    }

    // Layer 3: project config (TOML only; .json files are handled by legacy JSON path)
    let project_path: Option<PathBuf> = project_config_path
        .filter(|p| p.exists() && p.extension().and_then(|e| e.to_str()) != Some("json"))
        .map(|p| p.to_path_buf())
        .or_else(find_project_config_toml);

    if let Some(ref p) = project_path {
        let project = load_config(p)?;
        merge_config(&mut merged, project);
    }

    Ok(merged)
}

/// Merge `overlay` on top of `base`, preferring `overlay` values wherever set.
///
/// Slicing: if the overlay has a `[slicing]` section, it wins entirely.
/// Server, machine, and global fields are merged field-by-field so that partial
/// project configs work correctly.
fn merge_config(base: &mut AppConfig, overlay: AppConfig) {
    if let Some(s) = overlay.slicing {
        base.slicing = Some(s);
    }
    merge_server(&mut base.server, overlay.server);
    merge_machine(&mut base.machine, overlay.machine);
    merge_global_cfg(&mut base.global, overlay.global);

    if overlay.start_print_gcode.is_some() {
        base.start_print_gcode = overlay.start_print_gcode;
    }
    if overlay.end_print_gcode.is_some() {
        base.end_print_gcode = overlay.end_print_gcode;
    }
    for (k, v) in overlay.lifecycle_markers {
        base.lifecycle_markers.insert(k, v);
    }

    // Profiles are additive: overlay keys win, base keys not in overlay are kept.
    for (k, v) in overlay.profiles.presets {
        base.profiles.presets.insert(k, v);
    }
    for (k, v) in overlay.profiles.machines {
        base.profiles.machines.insert(k, v);
    }
    for (k, v) in overlay.profiles.materials {
        base.profiles.materials.insert(k, v);
    }
}

fn merge_server(
    base: &mut crate::config::types::ServerConfig,
    overlay: crate::config::types::ServerConfig,
) {
    let defaults = crate::config::types::ServerConfig::default();
    if overlay.host != defaults.host {
        base.host = overlay.host;
    }
    if overlay.port != defaults.port {
        base.port = overlay.port;
    }
    if overlay.ui_dir != defaults.ui_dir {
        base.ui_dir = overlay.ui_dir;
    }
    if overlay.work_dir.is_some() {
        base.work_dir = overlay.work_dir;
    }
    if !overlay.cors_origins.is_empty() {
        base.cors_origins = overlay.cors_origins;
    }
}

fn merge_machine(
    base: &mut crate::config::types::MachineConfig,
    overlay: crate::config::types::MachineConfig,
) {
    let defaults = crate::config::types::MachineConfig::default();
    if overlay.name != defaults.name {
        base.name = overlay.name;
    }
    if overlay.nozzle_diameter != defaults.nozzle_diameter {
        base.nozzle_diameter = overlay.nozzle_diameter;
    }
    if overlay.min_layer_height != defaults.min_layer_height {
        base.min_layer_height = overlay.min_layer_height;
    }
    if overlay.max_layer_height != defaults.max_layer_height {
        base.max_layer_height = overlay.max_layer_height;
    }
    if overlay.max_print_speed != defaults.max_print_speed {
        base.max_print_speed = overlay.max_print_speed;
    }
    if overlay.max_acceleration != defaults.max_acceleration {
        base.max_acceleration = overlay.max_acceleration;
    }
    if overlay.build_volume_x != defaults.build_volume_x {
        base.build_volume_x = overlay.build_volume_x;
    }
    if overlay.build_volume_y != defaults.build_volume_y {
        base.build_volume_y = overlay.build_volume_y;
    }
    if overlay.build_volume_z != defaults.build_volume_z {
        base.build_volume_z = overlay.build_volume_z;
    }
}

fn merge_global_cfg(
    base: &mut crate::config::types::GlobalConfig,
    overlay: crate::config::types::GlobalConfig,
) {
    if overlay.log_level.is_some() {
        base.log_level = overlay.log_level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AppConfig;
    use crate::settings::params::SlicingParams;

    #[test]
    fn test_load_config_missing_file_returns_default() {
        let path = PathBuf::from("/nonexistent/path/slicer.toml");
        let config = load_config(&path).expect("should return default");
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn test_round_trip_toml() {
        let mut config = AppConfig::default();
        config.slicing = Some(SlicingParams {
            layer_height: 0.15,
            ..SlicingParams::default()
        });
        config.server.port = 5300;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("slicer.toml");

        save_config(&config, &path).expect("save");
        let loaded = load_config(&path).expect("load");
        assert_eq!(loaded.slicing.unwrap().layer_height, 0.15);
        assert_eq!(loaded.server.port, 5300);
    }

    #[test]
    fn test_merge_config_overlay_wins() {
        let mut base = AppConfig {
            slicing: Some(SlicingParams {
                layer_height: 0.2,
                ..SlicingParams::default()
            }),
            ..AppConfig::default()
        };

        let overlay = AppConfig {
            slicing: Some(SlicingParams {
                layer_height: 0.1,
                nozzle_temp: 220.0,
                ..SlicingParams::default()
            }),
            ..AppConfig::default()
        };

        merge_config(&mut base, overlay);
        let slicing = base.slicing.unwrap();
        assert_eq!(slicing.layer_height, 0.1);
        assert_eq!(slicing.nozzle_temp, 220.0);
    }

    #[test]
    fn test_merge_config_base_preserved_when_overlay_none() {
        let mut base = AppConfig {
            slicing: Some(SlicingParams {
                layer_height: 0.2,
                ..SlicingParams::default()
            }),
            ..AppConfig::default()
        };

        let overlay = AppConfig::default(); // slicing is None

        merge_config(&mut base, overlay);
        assert_eq!(base.slicing.unwrap().layer_height, 0.2);
    }

    #[test]
    fn test_load_and_merge_config_no_files() {
        // With no config files on disk this should always succeed with defaults.
        // We can't control the global file, but in CI there won't be one, and
        // CWD won't have slicer.toml either.
        let result = load_and_merge_config(Some(Path::new("/nonexistent/slicer.toml")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_slicing_config_partial_toml() {
        let toml_str = r#"
[slicing]
layer_height = 0.3
nozzle_temp = 215.0
"#;
        let config: AppConfig = toml::from_str(toml_str).expect("parse");
        let slicing = config.slicing.expect("slicing section present");
        assert_eq!(slicing.layer_height, 0.3);
        assert_eq!(slicing.nozzle_temp, 215.0);
        assert_eq!(slicing.bed_temp, 60.0); // default
    }
}
