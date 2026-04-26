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

impl SettingsCommand {
    /// Execute the settings command.
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.subcommand {
            SettingsSubcommand::Validate(args) => execute_validate(args),
            SettingsSubcommand::Diff(args) => execute_diff(args),
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
}
