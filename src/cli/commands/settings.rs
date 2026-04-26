//! Settings command — validate and diff printer/slicing settings.

use clap::{Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;

use crate::cli::emit::Emitter;
use crate::cli::error::CliError;
use crate::cli::output::{EmitPayload, OutputFormat};
use crate::settings::diff::compare_settings;
use crate::settings::params::{GlobalSettings, ObjectSettings};
use crate::settings::validator::SettingValidator;
use crate::settings::{load_settings, save_settings};

/// Manage and validate slicing settings.
#[derive(Parser, Debug)]
pub struct SettingsCommand {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub subcommand: SettingsSubcommand,
}

/// Available settings subcommands.
#[derive(Subcommand, Debug)]
pub enum SettingsSubcommand {
    /// Validate global and object settings against physical constraints.
    Validate(ValidateArgs),
    /// Show differences (overrides) between global and object settings.
    Diff(DiffArgs),
    /// Display all global settings.
    Show(ShowArgs),
    /// Get a specific global setting value.
    Get(GetArgs),
    /// Set a global setting value.
    Set(SetArgs),
}

/// Arguments for the `settings validate` subcommand.
#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Path to global settings JSON file.
    #[arg(long)]
    pub global: PathBuf,

    /// Path to object settings JSON file.
    #[arg(long)]
    pub object: PathBuf,

    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

/// Arguments for the `settings diff` subcommand.
#[derive(Parser, Debug)]
pub struct DiffArgs {
    /// Path to global settings JSON file.
    #[arg(long)]
    pub global: PathBuf,

    /// Path to object settings JSON file.
    #[arg(long)]
    pub object: PathBuf,

    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

/// Arguments for the `settings show` subcommand.
#[derive(Parser, Debug)]
pub struct ShowArgs {
    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

/// Arguments for the `settings get` subcommand.
#[derive(Parser, Debug)]
pub struct GetArgs {
    /// Setting key (e.g., "layer_height", "nozzle_temp", "params.infill_density").
    pub key: String,

    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

/// Arguments for the `settings set` subcommand.
#[derive(Parser, Debug)]
pub struct SetArgs {
    /// Setting key (e.g., "layer_height", "nozzle_temp").
    pub key: String,

    /// Setting value (as JSON for numeric/boolean values, or plain string for text).
    pub value: String,

    /// Output format (json, human).
    #[arg(long, default_value = "human")]
    pub output_format: String,
}

impl SettingsCommand {
    /// Execute the settings command.
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.subcommand {
            SettingsSubcommand::Validate(args) => execute_validate(args),
            SettingsSubcommand::Diff(args) => execute_diff(args),
            SettingsSubcommand::Show(args) => execute_show(args),
            SettingsSubcommand::Get(args) => execute_get(args),
            SettingsSubcommand::Set(args) => execute_set(args),
        }
    }
}

// ── Payload types ─────────────────────────────────────────────────────────────

struct ValidateResult {
    message: &'static str,
}

impl EmitPayload for ValidateResult {
    fn schema(&self) -> &'static str {
        "slicer-engine/settings-validate-result-v1"
    }

    fn display_human(&self) -> String {
        format!("✓ {}", self.message)
    }

    fn to_json(&self) -> Value {
        serde_json::json!({
            "status": "valid",
            "message": self.message,
        })
    }
}

struct DiffResult<'a> {
    object_name: &'a str,
    diffs: &'a [crate::settings::diff::SettingsDiff],
}

impl EmitPayload for DiffResult<'_> {
    fn schema(&self) -> &'static str {
        "slicer-engine/settings-diff-result-v1"
    }

    fn display_human(&self) -> String {
        let mut lines = vec![
            format!("Settings diff for object '{}':", self.object_name),
            format!("{:<20} {:<15} {:<15} Override", "Field", "Global", "Object"),
            "-".repeat(60),
        ];
        for d in self.diffs {
            let marker = if d.is_override { "✓" } else { "" };
            lines.push(format!(
                "{:<20} {:<15} {:<15} {}",
                d.field_name, d.global_value, d.object_value, marker
            ));
        }
        lines.join("\n")
    }

    fn to_json(&self) -> Value {
        serde_json::to_value(self.diffs).unwrap_or(serde_json::json!([]))
    }
}

fn load_global(path: &PathBuf) -> Result<GlobalSettings, CliError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::invalid(format!(
            "Cannot read global settings '{}': {}",
            path.display(),
            e
        ))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::invalid(format!(
            "Invalid global settings JSON '{}': {}",
            path.display(),
            e
        ))
    })
}

fn load_object(path: &PathBuf) -> Result<ObjectSettings, CliError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::invalid(format!(
            "Cannot read object settings '{}': {}",
            path.display(),
            e
        ))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::invalid(format!(
            "Invalid object settings JSON '{}': {}",
            path.display(),
            e
        ))
    })
}

fn execute_validate(args: &ValidateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);

    let global = load_global(&args.global)?;
    let object = load_object(&args.object)?;

    let global_result = global.params.validate();
    let object_result = object
        .overrides
        .as_ref()
        .map(|p| p.validate())
        .unwrap_or(Ok(()));

    let mut all_errors: Vec<String> = Vec::new();
    if let Err(errors) = global_result {
        all_errors.extend(errors.into_iter().map(|e| format!("[global] {}", e)));
    }
    if let Err(errors) = object_result {
        all_errors.extend(
            errors
                .into_iter()
                .map(|e| format!("[object:{}] {}", object.object_name, e)),
        );
    }

    if all_errors.is_empty() {
        emitter.emit(&ValidateResult {
            message: "Settings are valid",
        });
    } else {
        let errors_text = all_errors.join("; ");
        emitter.error("Settings validation failed", Some(&errors_text));
        return Err(CliError::failed("Settings validation failed").into());
    }

    Ok(())
}

fn execute_diff(args: &DiffArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);

    let global = load_global(&args.global)?;
    let object = load_object(&args.object)?;

    let diffs = compare_settings(&global, &object);

    emitter.emit(&DiffResult {
        object_name: &object.object_name,
        diffs: &diffs,
    });

    Ok(())
}

fn execute_show(args: &ShowArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);
    let settings = load_settings()?;

    emitter.emit(&ShowResult {
        settings: &settings,
    });

    Ok(())
}

fn execute_get(args: &GetArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);
    let settings = load_settings()?;

    let value = get_setting_value(&settings, &args.key)?;

    emitter.emit(&GetResult {
        key: &args.key,
        value: &value,
    });

    Ok(())
}

fn execute_set(args: &SetArgs) -> Result<(), Box<dyn std::error::Error>> {
    let format = args
        .output_format
        .parse::<OutputFormat>()
        .map_err(|e| format!("Invalid output format: {}", e))?;

    let emitter = Emitter::new(format);

    // Parse the value as JSON, falling back to string
    let parsed_value: Value =
        serde_json::from_str(&args.value).unwrap_or_else(|_| Value::String(args.value.clone()));

    // Load current settings, apply the change, and save
    let mut settings = load_settings()?;
    set_setting_value(&mut settings, &args.key, &parsed_value)?;
    save_settings(&settings)?;

    emitter.emit(&SetResult {
        key: &args.key,
        value: &parsed_value,
    });

    Ok(())
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Get a setting value by key from global settings
fn get_setting_value(
    settings: &GlobalSettings,
    key: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    match key {
        "layer_height" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.layer_height)
                .ok_or("Invalid float value")?,
        )),
        "wall_thickness" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.wall_thickness)
                .ok_or("Invalid float value")?,
        )),
        "infill_density" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.infill_density)
                .ok_or("Invalid float value")?,
        )),
        "print_speed" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.print_speed)
                .ok_or("Invalid float value")?,
        )),
        "nozzle_temp" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.nozzle_temp)
                .ok_or("Invalid float value")?,
        )),
        "bed_temp" => Ok(Value::Number(
            serde_json::Number::from_f64(settings.params.bed_temp).ok_or("Invalid float value")?,
        )),
        "gcode_flavor" => Ok(Value::String(settings.gcode_flavor.clone())),
        "start_print_gcode" => Ok(settings
            .start_print_gcode
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null)),
        "end_print_gcode" => Ok(settings
            .end_print_gcode
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null)),
        _ => Err(format!("Unknown setting key: {}", key).into()),
    }
}

/// Set a setting value by key in global settings
fn set_setting_value(
    settings: &mut GlobalSettings,
    key: &str,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    // String settings — handle before numeric parsing
    match key {
        "gcode_flavor" => {
            let flavor = value
                .as_str()
                .ok_or("Value for 'gcode_flavor' must be a string")?;
            // Validate it's a known flavor before persisting
            flavor
                .parse::<crate::gcode::GcodeFlavor>()
                .map_err(|e| format!("Invalid gcode_flavor: {}", e))?;
            settings.gcode_flavor = flavor.to_string();
            return Ok(());
        }
        "start_print_gcode" => {
            settings.start_print_gcode = if value.is_null() {
                None
            } else {
                Some(
                    value
                        .as_str()
                        .ok_or("Value for 'start_print_gcode' must be a string or null")?
                        .to_string(),
                )
            };
            return Ok(());
        }
        "end_print_gcode" => {
            settings.end_print_gcode = if value.is_null() {
                None
            } else {
                Some(
                    value
                        .as_str()
                        .ok_or("Value for 'end_print_gcode' must be a string or null")?
                        .to_string(),
                )
            };
            return Ok(());
        }
        _ => {}
    }

    let float_value = value
        .as_f64()
        .ok_or_else(|| format!("Value for '{}' must be a number", key))?;

    match key {
        "layer_height" => settings.params.layer_height = float_value,
        "wall_thickness" => settings.params.wall_thickness = float_value,
        "infill_density" => {
            if !(0.0..=1.0).contains(&float_value) {
                return Err("infill_density must be between 0.0 and 1.0".into());
            }
            settings.params.infill_density = float_value;
        }
        "print_speed" => settings.params.print_speed = float_value,
        "nozzle_temp" => settings.params.nozzle_temp = float_value,
        "bed_temp" => settings.params.bed_temp = float_value,
        _ => return Err(format!("Unknown setting key: {}", key).into()),
    }
    Ok(())
}

// ── Payload types ─────────────────────────────────────────────────────────────

struct ShowResult<'a> {
    settings: &'a GlobalSettings,
}

impl EmitPayload for ShowResult<'_> {
    fn schema(&self) -> &'static str {
        "slicer-engine/settings-show-result-v1"
    }

    fn display_human(&self) -> String {
        let mut lines = vec![
            "Global Settings:".to_string(),
            format!("  layer_height: {} mm", self.settings.params.layer_height),
            format!(
                "  wall_thickness: {} mm",
                self.settings.params.wall_thickness
            ),
            format!(
                "  infill_density: {:.0}%",
                self.settings.params.infill_density * 100.0
            ),
            format!("  print_speed: {} mm/s", self.settings.params.print_speed),
            format!("  nozzle_temp: {}°C", self.settings.params.nozzle_temp),
            format!("  bed_temp: {}°C", self.settings.params.bed_temp),
            format!("  gcode_flavor: {}", self.settings.gcode_flavor),
        ];
        if let Some(s) = &self.settings.start_print_gcode {
            lines.push(format!("  start_print_gcode: {}", s));
        }
        if let Some(s) = &self.settings.end_print_gcode {
            lines.push(format!("  end_print_gcode: {}", s));
        }
        lines.join("\n")
    }

    fn to_json(&self) -> Value {
        serde_json::to_value(self.settings).unwrap_or(Value::Null)
    }
}

struct GetResult<'a> {
    key: &'a str,
    value: &'a Value,
}

impl EmitPayload for GetResult<'_> {
    fn schema(&self) -> &'static str {
        "slicer-engine/settings-get-result-v1"
    }

    fn display_human(&self) -> String {
        format!("{}: {}", self.key, self.value)
    }

    fn to_json(&self) -> Value {
        serde_json::json!({
            "key": self.key,
            "value": self.value,
        })
    }
}

struct SetResult<'a> {
    key: &'a str,
    value: &'a Value,
}

impl EmitPayload for SetResult<'_> {
    fn schema(&self) -> &'static str {
        "slicer-engine/settings-set-result-v1"
    }

    fn display_human(&self) -> String {
        format!("✓ Set {} = {}", self.key, self.value)
    }

    fn to_json(&self) -> Value {
        serde_json::json!({
            "key": self.key,
            "value": self.value,
            "message": "Setting updated successfully",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::params::{GlobalSettings, ObjectSettings, SlicingParams};

    #[test]
    fn test_settings_command_creation() {
        let _cmd = SettingsCommand {
            subcommand: SettingsSubcommand::Validate(ValidateArgs {
                global: PathBuf::from("global.json"),
                object: PathBuf::from("object.json"),
                output_format: "human".to_string(),
            }),
        };
    }

    #[test]
    fn test_settings_diff_command_creation() {
        let _cmd = SettingsCommand {
            subcommand: SettingsSubcommand::Diff(DiffArgs {
                global: PathBuf::from("global.json"),
                object: PathBuf::from("object.json"),
                output_format: "json".to_string(),
            }),
        };
    }

    #[test]
    fn test_compare_settings_via_command_logic() {
        let global = GlobalSettings::default();
        let object = ObjectSettings {
            object_name: "test".to_string(),
            overrides: Some(SlicingParams {
                layer_height: 0.1,
                ..SlicingParams::default()
            }),
        };
        let diffs = compare_settings(&global, &object);
        let lh = diffs
            .iter()
            .find(|d| d.field_name == "layer_height")
            .unwrap();
        assert!(lh.is_override);
    }

    #[test]
    fn test_validate_result_schema() {
        let r = ValidateResult {
            message: "Settings are valid",
        };
        assert_eq!(r.schema(), "slicer-engine/settings-validate-result-v1");
    }

    #[test]
    fn test_diff_result_schema() {
        let r = DiffResult {
            object_name: "cube",
            diffs: &[],
        };
        assert_eq!(r.schema(), "slicer-engine/settings-diff-result-v1");
    }

    #[test]
    fn test_show_command_creation() {
        let _cmd = SettingsCommand {
            subcommand: SettingsSubcommand::Show(ShowArgs {
                output_format: "human".to_string(),
            }),
        };
    }

    #[test]
    fn test_get_command_creation() {
        let _cmd = SettingsCommand {
            subcommand: SettingsSubcommand::Get(GetArgs {
                key: "layer_height".to_string(),
                output_format: "json".to_string(),
            }),
        };
    }

    #[test]
    fn test_set_command_creation() {
        let _cmd = SettingsCommand {
            subcommand: SettingsSubcommand::Set(SetArgs {
                key: "layer_height".to_string(),
                value: "0.15".to_string(),
                output_format: "human".to_string(),
            }),
        };
    }

    #[test]
    fn test_get_setting_value_layer_height() {
        let settings = GlobalSettings::default();
        let value = get_setting_value(&settings, "layer_height").unwrap();
        assert_eq!(value.as_f64().unwrap(), 0.2);
    }

    #[test]
    fn test_get_setting_value_nozzle_temp() {
        let settings = GlobalSettings::default();
        let value = get_setting_value(&settings, "nozzle_temp").unwrap();
        assert_eq!(value.as_f64().unwrap(), 210.0);
    }

    #[test]
    fn test_get_setting_value_invalid_key() {
        let settings = GlobalSettings::default();
        assert!(get_setting_value(&settings, "invalid_key").is_err());
    }

    #[test]
    fn test_show_result_schema() {
        let settings = GlobalSettings::default();
        let r = ShowResult {
            settings: &settings,
        };
        assert_eq!(r.schema(), "slicer-engine/settings-show-result-v1");
    }

    #[test]
    fn test_get_result_schema() {
        let value = serde_json::json!(0.2);
        let r = GetResult {
            key: "layer_height",
            value: &value,
        };
        assert_eq!(r.schema(), "slicer-engine/settings-get-result-v1");
    }

    #[test]
    fn test_set_result_schema() {
        let value = serde_json::json!(0.15);
        let r = SetResult {
            key: "layer_height",
            value: &value,
        };
        assert_eq!(r.schema(), "slicer-engine/settings-set-result-v1");
    }

    #[test]
    fn test_set_setting_value_layer_height() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!(0.15);
        assert!(set_setting_value(&mut settings, "layer_height", &value).is_ok());
        assert_eq!(settings.params.layer_height, 0.15);
    }

    #[test]
    fn test_set_setting_value_nozzle_temp() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!(220.0);
        assert!(set_setting_value(&mut settings, "nozzle_temp", &value).is_ok());
        assert_eq!(settings.params.nozzle_temp, 220.0);
    }

    #[test]
    fn test_set_setting_value_infill_density_valid() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!(0.5);
        assert!(set_setting_value(&mut settings, "infill_density", &value).is_ok());
        assert_eq!(settings.params.infill_density, 0.5);
    }

    #[test]
    fn test_set_setting_value_infill_density_invalid() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!(1.5);
        assert!(set_setting_value(&mut settings, "infill_density", &value).is_err());
    }

    #[test]
    fn test_set_setting_value_invalid_type() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!("not a number");
        assert!(set_setting_value(&mut settings, "layer_height", &value).is_err());
    }

    #[test]
    fn test_set_setting_value_invalid_key() {
        let mut settings = GlobalSettings::default();
        let value = serde_json::json!(0.2);
        assert!(set_setting_value(&mut settings, "invalid_key", &value).is_err());
    }
}
