//! Schema generation command - generates JSON Schema files for all emit payloads
//!
//! The schemas are derived from the actual Rust types using `schemars`,
//! ensuring they always match the real implementation.

use clap::Parser;
use std::path::PathBuf;

use crate::cli::schemas;

/// Generate JSON Schema files for all emit payload types
///
/// Schemas are automatically derived from the Rust types, ensuring they
/// stay in sync with the actual implementation.
#[derive(Parser, Debug)]
pub struct GenSchemasCommand {
    /// Output directory for schema files
    #[arg(short, long, default_value = "./schemas")]
    pub output_dir: PathBuf,

    /// Generate a single schema instead of all (by schema ID)
    #[arg(long)]
    pub schema: Option<String>,

    /// Pretty-print JSON schemas (default: true)
    #[arg(long, default_value = "true")]
    pub pretty: bool,
}

impl GenSchemasCommand {
    /// Execute the gen-schemas command
    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&self.output_dir)?;

        let all_schemas = schemas::all_schemas();

        if let Some(filter_id) = &self.schema {
            // Generate single schema by ID
            let found = all_schemas
                .iter()
                .find(|s| s.schema_id == filter_id)
                .ok_or_else(|| format!("Schema not found: {}", filter_id))?;

            self.write_schema(found)?;
        } else {
            // Generate all schemas
            for schema_def in all_schemas {
                self.write_schema(&schema_def)?;
            }
        }

        println!("\n✓ All schemas generated successfully!");
        Ok(())
    }

    fn write_schema(
        &self,
        schema_def: &schemas::SchemaDefinition,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let filename = schema_def.schema_id.replace('/', "-") + ".json";
        let path = self.output_dir.join(&filename);

        let json_output = if self.pretty {
            serde_json::to_string_pretty(&schema_def.schema)?
        } else {
            serde_json::to_string(&schema_def.schema)?
        };

        std::fs::write(&path, json_output)?;
        println!(
            "✓ Generated schema: {} -> {}",
            schema_def.schema_id,
            path.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_schemas_command_creation() {
        let cmd = GenSchemasCommand {
            output_dir: PathBuf::from("./schemas"),
            schema: None,
            pretty: true,
        };
        assert_eq!(cmd.output_dir.to_string_lossy(), "./schemas");
    }

    #[test]
    fn test_gen_schemas_with_filter() {
        let cmd = GenSchemasCommand {
            output_dir: PathBuf::from("./schemas"),
            schema: Some("slicer-engine/result-v1".to_string()),
            pretty: true,
        };
        assert_eq!(cmd.schema.as_ref().unwrap(), "slicer-engine/result-v1");
    }
}
