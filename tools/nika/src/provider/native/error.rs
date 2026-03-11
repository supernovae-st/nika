//! Native inference error types.
//!
//! v0.27.0: Migrated from spn-native.

use std::path::PathBuf;
use thiserror::Error;

/// Error types for native inference operations.
#[derive(Error, Debug)]
pub enum NativeError {
    /// Model not found on HuggingFace or locally.
    #[error("Model not found: {repo}/{filename}")]
    ModelNotFound {
        /// HuggingFace repository.
        repo: String,
        /// Filename.
        filename: String,
    },

    /// No model is currently loaded.
    #[error("No model is loaded")]
    ModelNotLoaded,

    /// Checksum verification failed.
    #[error("Checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// File path.
        path: PathBuf,
        /// Expected checksum.
        expected: String,
        /// Actual checksum.
        actual: String,
    },

    /// Invalid configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Network error.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Inference failed.
    #[error("Inference failed: {0}")]
    InferenceFailed(String),
}

/// Result type for native inference operations.
pub type Result<T> = std::result::Result<T, NativeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = NativeError::ModelNotFound {
            repo: "test/repo".to_string(),
            filename: "model.gguf".to_string(),
        };
        assert_eq!(err.to_string(), "Model not found: test/repo/model.gguf");
    }

    #[test]
    fn test_model_not_loaded() {
        let err = NativeError::ModelNotLoaded;
        assert_eq!(err.to_string(), "No model is loaded");
    }
}
