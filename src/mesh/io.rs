//! STL file reading (binary and ASCII).
//!
//! The `stl_io` crate handles both formats transparently.
//! Loaded meshes are in native STL coordinates — no transforms are applied on import.

use std::fs::OpenOptions;
use std::io::Cursor;
use std::path::Path;

use crate::mesh::types::{Face, Mesh, Vertex};

/// Load a mesh from a binary or ASCII STL file.
///
/// # Errors
/// Returns an error if the file cannot be opened, is not a valid STL file,
/// or cannot be converted to the internal mesh representation.
///
/// # Example
/// ```no_run
/// use std::path::Path;
/// use slicer_engine::mesh::io::read_stl;
/// let mesh = read_stl(Path::new("model.stl")).unwrap();
/// ```
pub fn read_stl(path: &Path) -> Result<Mesh, Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| format!("Cannot open STL file '{}': {}", path.display(), e))?;

    let indexed = stl_io::read_stl(&mut file)
        .map_err(|e| format!("Failed to parse STL file '{}': {}", path.display(), e))?;

    // Convert stl_io vertices (f32) to our Vertex type (f64)
    let vertices: Vec<Vertex> = indexed
        .vertices
        .iter()
        .map(|v| Vertex::new(v[0] as f64, v[1] as f64, v[2] as f64))
        .collect();

    // Reconstruct full Face structs from indexed triangles
    let faces: Vec<Face> = indexed
        .faces
        .iter()
        .map(|tri| {
            let v0 = vertices[tri.vertices[0]];
            let v1 = vertices[tri.vertices[1]];
            let v2 = vertices[tri.vertices[2]];

            let normal_vec = tri.normal;
            let normal = if normal_vec[0] != 0.0 || normal_vec[1] != 0.0 || normal_vec[2] != 0.0 {
                Some(Vertex::new(
                    normal_vec[0] as f64,
                    normal_vec[1] as f64,
                    normal_vec[2] as f64,
                ))
            } else {
                None
            };

            Face {
                vertices: [v0, v1, v2],
                normal,
            }
        })
        .collect();

    Ok(Mesh {
        vertices,
        faces,
        aabb: None,
    })
}

/// Load a mesh from raw STL bytes (binary or ASCII).
///
/// Useful when the STL data has already been read into memory (e.g. uploaded
/// over a WebSocket) rather than read from a file path.
///
/// # Errors
/// Returns an error if the bytes are not a valid STL file or cannot be
/// converted to the internal mesh representation.
pub fn read_stl_from_bytes(bytes: &[u8]) -> Result<Mesh, Box<dyn std::error::Error>> {
    let mut cursor = Cursor::new(bytes);

    let indexed = stl_io::read_stl(&mut cursor)
        .map_err(|e| format!("Failed to parse STL from bytes: {}", e))?;

    let vertices: Vec<Vertex> = indexed
        .vertices
        .iter()
        .map(|v| Vertex::new(v[0] as f64, v[1] as f64, v[2] as f64))
        .collect();

    let faces: Vec<Face> = indexed
        .faces
        .iter()
        .map(|tri| {
            let v0 = vertices[tri.vertices[0]];
            let v1 = vertices[tri.vertices[1]];
            let v2 = vertices[tri.vertices[2]];

            let normal_vec = tri.normal;
            let normal = if normal_vec[0] != 0.0 || normal_vec[1] != 0.0 || normal_vec[2] != 0.0 {
                Some(Vertex::new(
                    normal_vec[0] as f64,
                    normal_vec[1] as f64,
                    normal_vec[2] as f64,
                ))
            } else {
                None
            };

            Face {
                vertices: [v0, v1, v2],
                normal,
            }
        })
        .collect();

    Ok(Mesh {
        vertices,
        faces,
        aabb: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn test_read_binary_stl() {
        let mesh = read_stl(&fixture("simple-cube.stl")).expect("Failed to read binary STL");
        // A unit cube in STL: 12 triangles, 8 unique vertices
        assert_eq!(mesh.faces.len(), 12, "Expected 12 faces");
        assert_eq!(mesh.vertices.len(), 8, "Expected 8 unique vertices");
    }

    #[test]
    fn test_read_ascii_stl() {
        let mesh = read_stl(&fixture("simple-cube-ascii.stl")).expect("Failed to read ASCII STL");
        assert_eq!(mesh.faces.len(), 12, "Expected 12 faces");
        assert_eq!(mesh.vertices.len(), 8, "Expected 8 unique vertices");
    }

    #[test]
    fn test_read_stl_from_bytes() {
        let path = fixture("simple-cube.stl");
        let bytes = std::fs::read(&path).expect("Failed to read fixture bytes");
        let mesh = read_stl_from_bytes(&bytes).expect("Failed to parse STL from bytes");
        assert_eq!(mesh.faces.len(), 12, "Expected 12 faces");
        assert_eq!(mesh.vertices.len(), 8, "Expected 8 unique vertices");
    }

    #[test]
    fn test_read_stl_from_invalid_bytes() {
        let result = read_stl_from_bytes(b"not valid stl data at all");
        assert!(result.is_err(), "Should fail on invalid bytes");
    }

    #[test]
    fn test_missing_file_returns_error() {
        let result = read_stl(Path::new("/nonexistent/path/mesh.stl"));
        assert!(result.is_err(), "Should fail on missing file");
    }

    #[test]
    fn test_invalid_file_returns_error() {
        // Write a temp file with garbage content
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"not an stl file garbage").unwrap();
        let result = read_stl(tmp.path());
        assert!(result.is_err(), "Should fail on invalid STL content");
    }
}
