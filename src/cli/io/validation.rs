//! Path validation utilities

use crate::cli::error::CliError;
use crate::mesh::io::SUPPORTED_EXTENSIONS;
use std::path::{Path, PathBuf};

/// Path validator for security and consistency
pub struct PathValidator;

impl PathValidator {
    /// Validate input file path
    pub fn validate_input(path: &Path) -> Result<PathBuf, CliError> {
        if !path.exists() {
            return Err(CliError::invalid(format!(
                "Input file not found: {}",
                path.display()
            )));
        }

        if !path.is_file() {
            return Err(CliError::invalid(format!(
                "Input path is not a file: {}",
                path.display()
            )));
        }

        // Canonicalize the path to prevent directory traversal
        path.canonicalize().map_err(|e| {
            CliError::invalid(format!("Cannot resolve path {}: {}", path.display(), e))
        })
    }

    /// Validate output directory path
    pub fn validate_output_dir(path: &Path) -> Result<PathBuf, CliError> {
        if path.exists() {
            if !path.is_dir() {
                return Err(CliError::invalid(format!(
                    "Output path is not a directory: {}",
                    path.display()
                )));
            }
        } else {
            // Create directory if it doesn't exist
            std::fs::create_dir_all(path).map_err(CliError::from)?;
        }

        path.canonicalize().map_err(|e| {
            CliError::invalid(format!(
                "Cannot resolve output path {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Check if file has valid extension
    pub fn validate_extension(path: &Path, valid_extensions: &[&str]) -> Result<(), CliError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if valid_extensions.contains(&ext.as_str()) {
            Ok(())
        } else {
            Err(CliError::invalid(format!(
                "Invalid file extension '.{}'. Expected: {}",
                ext,
                valid_extensions.join(", ")
            )))
        }
    }

    /// Validate that the input path points to a supported 3D model file.
    ///
    /// Combines existence/file checks with extension validation against the
    /// engine's `SUPPORTED_EXTENSIONS` list (stl, obj, 3mf).
    pub fn validate_mesh_input(path: &Path) -> Result<PathBuf, CliError> {
        Self::validate_extension(path, SUPPORTED_EXTENSIONS)?;
        Self::validate_input(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_nonexistent_file() {
        let result = PathValidator::validate_input(Path::new("/nonexistent/file.stl"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extension_valid() {
        let result = PathValidator::validate_extension(Path::new("model.stl"), &["stl", "obj"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_extension_invalid() {
        let result = PathValidator::validate_extension(Path::new("model.txt"), &["stl", "obj"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extension_case_insensitive() {
        let result = PathValidator::validate_extension(Path::new("model.STL"), &["stl", "obj"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mesh_input_unsupported_extension() {
        let result = PathValidator::validate_mesh_input(Path::new("model.ply"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mesh_input_obj_extension_accepted() {
        // Extension is valid; file doesn't need to exist — the extension check
        // should pass and the existence check will fail (not "unsupported ext").
        let result = PathValidator::validate_mesh_input(Path::new("/nonexistent/model.obj"));
        let err = result.unwrap_err().to_string();
        assert!(
            !err.contains("Invalid file extension"),
            "OBJ extension should be accepted: {err}"
        );
    }

    #[test]
    fn test_validate_mesh_input_3mf_extension_accepted() {
        let result = PathValidator::validate_mesh_input(Path::new("/nonexistent/model.3mf"));
        let err = result.unwrap_err().to_string();
        assert!(
            !err.contains("Invalid file extension"),
            "3MF extension should be accepted: {err}"
        );
    }
}
