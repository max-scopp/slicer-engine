//! Mesh loading for the scene engine.
//!
//! Wraps [`crate::mesh::io`] with a single entry point that takes raw bytes
//! plus a [`MeshFormat`] enum. Phase-5 cleanup will fold the underlying
//! parsers into this module.

use crate::mesh::io;
use crate::mesh::types::Mesh;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported mesh file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MeshFormat {
    /// STL (binary or ASCII).
    Stl,
    /// Wavefront OBJ.
    Obj,
    /// 3D Manufacturing Format (3MF).
    Threemf,
}

impl MeshFormat {
    /// Infer the format from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "stl" => Some(Self::Stl),
            "obj" => Some(Self::Obj),
            "3mf" => Some(Self::Threemf),
            _ => None,
        }
    }

    /// Infer the format from a path's extension (case-insensitive).
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Canonical lowercase name (e.g. `"stl"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stl => "stl",
            Self::Obj => "obj",
            Self::Threemf => "3mf",
        }
    }
}

/// Load a mesh from raw bytes, given an explicit format.
pub fn load_bytes(bytes: &[u8], format: MeshFormat) -> Result<Mesh, String> {
    match format {
        MeshFormat::Stl => io::read_stl_from_bytes(bytes).map_err(|e| e.to_string()),
        MeshFormat::Obj => io::read_obj_from_bytes(bytes).map_err(|e| e.to_string()),
        MeshFormat::Threemf => io::read_3mf_from_bytes(bytes).map_err(|e| e.to_string()),
    }
}

/// Load a mesh from a path, auto-detecting the format from the extension.
pub fn load_path(path: &Path) -> Result<Mesh, String> {
    io::read_mesh(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_extension_is_case_insensitive() {
        assert_eq!(MeshFormat::from_extension("STL"), Some(MeshFormat::Stl));
        assert_eq!(MeshFormat::from_extension("3mf"), Some(MeshFormat::Threemf));
        assert_eq!(MeshFormat::from_extension("xyz"), None);
    }

    #[test]
    fn format_from_path_uses_extension() {
        assert_eq!(
            MeshFormat::from_path(Path::new("/tmp/cube.OBJ")),
            Some(MeshFormat::Obj)
        );
    }
}
