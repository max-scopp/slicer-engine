//! Config command — manage the centralized TOML configuration file.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::cli::emit::Emitter;
use crate::cli::output::{EmitPayload, OutputFormat};
use crate::config::{config_file, load_and_merge_config, load_config, save_config, AppConfig};

/// Manage the centralized slicer.toml configuration file.
#[derive(Parser, Debug)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub subcommand: ConfigSubcommand,
}

/// Available config subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigSubcommand {
    /// Display the resolved configuration (merged from all layers).
    Show(ConfigShowArgs),
    /// Generate a default slicer.toml in the target location.
    Init(ConfigInitArgs),
    /// Display the path to the global slicer.toml.
    Path(ConfigPathArgs),
}

/// Arguments for `config show`.
#[derive(Parser, Debug)]
pub struct ConfigShowArgs {
    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,

    /// Path to an explicit project slicer.toml to include in the merge.
    #[arg(long)]
    pub project: Option<PathBuf>,
}

/// Arguments for `config init`.
#[derive(Parser, Debug)]
pub struct ConfigInitArgs {
    /// Write the default config to this path.
    ///
    /// Defaults to `./slicer.toml` (project-level) when not specified.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Overwrite an existing file.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

/// Arguments for `config path`.
#[derive(Parser, Debug)]
pub struct ConfigPathArgs {
    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

impl ConfigCommand {
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.subcommand {
            ConfigSubcommand::Show(args) => execute_show(args),
            ConfigSubcommand::Init(args) => execute_init(args),
            ConfigSubcommand::Path(args) => execute_path(args),
        }
    }
}

// ── Payload types ─────────────────────────────────────────────────────────────

struct ShowResult<'a> {
    config: &'a AppConfig,
}

impl EmitPayload for ShowResult<'_> {
    fn schema(&self) -> &'static str {
        "slicer-engine/config-show-result-v1"
    }

    fn display_human(&self) -> String {
        toml::to_string_pretty(self.config).unwrap_or_else(|e| format!("<error: {}>", e))
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self.config).unwrap_or(serde_json::Value::Null)
    }
}

struct InitResult {
    path: PathBuf,
    already_existed: bool,
}

impl EmitPayload for InitResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/config-init-result-v1"
    }

    fn display_human(&self) -> String {
        if self.already_existed {
            format!("Config already exists at {}", self.path.display())
        } else {
            format!("✓ Created default config at {}", self.path.display())
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path.to_string_lossy(),
            "created": !self.already_existed,
        })
    }
}

struct PathResult {
    path: PathBuf,
    exists: bool,
}

impl EmitPayload for PathResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/config-path-result-v1"
    }

    fn display_human(&self) -> String {
        let status = if self.exists { "(exists)" } else { "(not found)" };
        format!("{} {}", self.path.display(), status)
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": self.path.to_string_lossy(),
            "exists": self.exists,
        })
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn execute_show(args: &ConfigShowArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);
    let config = load_and_merge_config(args.project.as_deref())?;

    emitter.emit(&ShowResult { config: &config });
    Ok(())
}

fn execute_init(args: &ConfigInitArgs) -> Result<(), Box<dyn std::error::Error>> {
    let path = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from("./slicer.toml"));

    if path.exists() && !args.force {
        let emitter = Emitter::new(OutputFormat::Human);
        emitter.emit(&InitResult {
            path,
            already_existed: true,
        });
        return Ok(());
    }

    let default_config = AppConfig::default();
    save_config(&default_config, &path)?;

    let emitter = Emitter::new(OutputFormat::Human);
    emitter.emit(&InitResult {
        path,
        already_existed: false,
    });
    Ok(())
}

fn execute_path(args: &ConfigPathArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);
    let path = config_file();
    let exists = path.exists();

    emitter.emit(&PathResult { path, exists });
    Ok(())
}

/// Update a single field in the global TOML config by loading, modifying, and saving it.
///
/// This is also called from the `settings set` flow to keep the TOML in sync.
pub fn update_config_field(
    path: &std::path::Path,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = load_config(path)?;
    apply_config_field(&mut config, key, value)?;
    save_config(&config, path)
}

/// Apply a key→value pair to an `AppConfig`.
///
/// Supported keys mirror the `[slicing]` and `[server]` section fields.
pub fn apply_config_field(
    config: &mut AppConfig,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    // Route by section prefix
    if let Some(slicing_key) = key.strip_prefix("slicing.") {
        apply_slicing_field(config, slicing_key, value)?;
    } else if let Some(server_key) = key.strip_prefix("server.") {
        apply_server_field(&mut config.server, server_key, value)?;
    } else if let Some(machine_key) = key.strip_prefix("machine.") {
        apply_machine_field(&mut config.machine, machine_key, value)?;
    } else {
        return Err(format!(
            "Unknown config key '{}'. Use section-prefixed keys like 'slicing.layer_height'.",
            key
        )
        .into());
    }
    Ok(())
}

fn apply_slicing_field(
    config: &mut AppConfig,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::settings::params::SlicingParams;

    let slicing = config.slicing.get_or_insert_with(SlicingParams::default);
    let mut slicing_val = serde_json::to_value(&*slicing)
        .map_err(|e| format!("Failed to serialize slicing params: {}", e))?;
    let obj = slicing_val
        .as_object_mut()
        .ok_or("Internal error: slicing params is not a JSON object")?;

    if !obj.contains_key(key) {
        return Err(format!("Unknown slicing config key: '{}'", key).into());
    }
    obj.insert(key.to_string(), value.clone());
    *slicing = serde_json::from_value(slicing_val)
        .map_err(|e| format!("Invalid value for '{}': {}", key, e))?;
    Ok(())
}

fn apply_server_field(
    server: &mut crate::config::ServerConfig,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    match key {
        "host" => {
            server.host = value
                .as_str()
                .ok_or("'server.host' must be a string")?
                .to_string();
        }
        "port" => {
            server.port = value
                .as_u64()
                .ok_or("'server.port' must be a number")? as u16;
        }
        "ui_dir" => {
            server.ui_dir = value
                .as_str()
                .ok_or("'server.ui_dir' must be a string")?
                .to_string();
        }
        "work_dir" => {
            server.work_dir = Some(
                value
                    .as_str()
                    .ok_or("'server.work_dir' must be a string")?
                    .to_string(),
            );
        }
        _ => return Err(format!("Unknown server config key: '{}'", key).into()),
    }
    Ok(())
}

fn apply_machine_field(
    machine: &mut crate::config::MachineConfig,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    macro_rules! set_f64 {
        ($field:ident) => {
            if key == stringify!($field) {
                machine.$field = value
                    .as_f64()
                    .ok_or_else(|| format!("'{}' must be a number", key))?;
                return Ok(());
            }
        };
    }

    if key == "name" {
        machine.name = value
            .as_str()
            .ok_or("'machine.name' must be a string")?
            .to_string();
        return Ok(());
    }

    set_f64!(nozzle_diameter);
    set_f64!(min_layer_height);
    set_f64!(max_layer_height);
    set_f64!(max_print_speed);
    set_f64!(max_acceleration);
    set_f64!(build_volume_x);
    set_f64!(build_volume_y);
    set_f64!(build_volume_z);

    Err(format!("Unknown machine config key: '{}'", key).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn test_apply_slicing_layer_height() {
        let mut config = AppConfig::default();
        apply_config_field(&mut config, "slicing.layer_height", &serde_json::json!(0.12))
            .unwrap();
        assert_eq!(config.slicing.unwrap().layer_height, 0.12);
    }

    #[test]
    fn test_apply_server_port() {
        let mut config = AppConfig::default();
        apply_config_field(&mut config, "server.port", &serde_json::json!(8080)).unwrap();
        assert_eq!(config.server.port, 8080);
    }

    #[test]
    fn test_apply_machine_nozzle_diameter() {
        let mut config = AppConfig::default();
        apply_config_field(
            &mut config,
            "machine.nozzle_diameter",
            &serde_json::json!(0.6),
        )
        .unwrap();
        assert_eq!(config.machine.nozzle_diameter, 0.6);
    }

    #[test]
    fn test_apply_unknown_section_returns_error() {
        let mut config = AppConfig::default();
        assert!(apply_config_field(&mut config, "unknown.key", &serde_json::json!(1)).is_err());
    }

    #[test]
    fn test_apply_slicing_unknown_key_returns_error() {
        let mut config = AppConfig::default();
        assert!(
            apply_config_field(&mut config, "slicing.not_a_field", &serde_json::json!(1.0))
                .is_err()
        );
    }

    #[test]
    fn test_execute_init_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slicer.toml");
        let args = ConfigInitArgs {
            output: Some(path.clone()),
            force: false,
        };
        execute_init(&args).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        // Verify it's valid TOML
        let _parsed: AppConfig = toml::from_str(&content).unwrap();
    }

    #[test]
    fn test_execute_init_does_not_overwrite_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slicer.toml");
        std::fs::write(&path, "# existing").unwrap();
        let args = ConfigInitArgs {
            output: Some(path.clone()),
            force: false,
        };
        execute_init(&args).unwrap();
        // Should not have overwritten
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.trim(), "# existing");
    }

    #[test]
    fn test_execute_init_overwrites_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slicer.toml");
        std::fs::write(&path, "# existing").unwrap();
        let args = ConfigInitArgs {
            output: Some(path.clone()),
            force: true,
        };
        execute_init(&args).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_ne!(content.trim(), "# existing");
    }
}
