//! CLI error types and conversions

use std::io;

/// CLI operation errors
#[derive(Debug)]
pub enum CliError {
    /// File I/O error
    Io(io::Error),
    /// Invalid input or configuration
    Invalid(String),
    /// Operation failed
    Failed(String),
    /// Slicing operation error
    Slicing(String),
}

impl CliError {
    /// Create an invalid input error
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }

    /// Create a failed operation error
    pub fn failed(msg: impl Into<String>) -> Self {
        Self::Failed(msg.into())
    }

    /// Create a slicing error
    pub fn slicing(msg: impl Into<String>) -> Self {
        Self::Slicing(msg.into())
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Io(e) => write!(f, "I/O error: {}", e),
            CliError::Invalid(msg) => write!(f, "Invalid input: {}", msg),
            CliError::Failed(msg) => write!(f, "Operation failed: {}", msg),
            CliError::Slicing(msg) => write!(f, "Slicing error: {}", msg),
        }
    }
}

impl std::error::Error for CliError {}

impl From<io::Error> for CliError {
    fn from(err: io::Error) -> Self {
        CliError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = CliError::invalid("test error");
        assert_eq!(err.to_string(), "Invalid input: test error");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let cli_err = CliError::from(io_err);
        assert!(cli_err.to_string().contains("I/O error"));
    }
}
