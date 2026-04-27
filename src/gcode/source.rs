//! G-code source resolution — reads a script from a file path or inline string.

/// Maximum allowed size (in bytes) for a G-code script file read by
/// [`resolve_gcode_source`].  Prevents memory exhaustion when a large file is
/// accidentally passed as a script path.
const MAX_GCODE_FILE_BYTES: u64 = 1024 * 1024; // 1 MiB

/// Resolve a G-code source string that may be either a file path or a direct
/// G-code snippet.
///
/// Resolution order:
/// 1. If `input` is the path to an existing file → read the file and return its
///    lines (blank lines and trailing whitespace are preserved).
/// 2. Otherwise → treat `input` as a literal G-code string and split on `'\n'`.
///
/// This allows callers to pass either `"./my-start.gcode"` or a multi-line
/// string such as `"G28\nM109 S210"` without any extra ceremony.
///
/// Primary project use: resolving optional start/end print scripts configured
/// via CLI flags or global settings before injecting them into the
/// [`crate::gcode::GcodeGenerator`] during `slice` command execution.
///
/// # Errors
/// Returns an [`std::io::Error`] if the path exists but cannot be read, or if
/// the file exceeds the 1 MiB size limit.
pub fn resolve_gcode_source(input: &str) -> Result<Vec<String>, std::io::Error> {
    let path = std::path::Path::new(input);
    if path.is_file() {
        let meta = std::fs::metadata(path)?;
        if meta.len() > MAX_GCODE_FILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "G-code file '{}' is too large ({} bytes; limit is {} bytes)",
                    path.display(),
                    meta.len(),
                    MAX_GCODE_FILE_BYTES
                ),
            ));
        }
        let content = std::fs::read_to_string(path)?;
        return Ok(content.lines().map(|l| l.to_string()).collect());
    }
    Ok(input.lines().map(|l| l.to_string()).collect())
}
