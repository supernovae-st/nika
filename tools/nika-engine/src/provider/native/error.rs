// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native inference error types.
//!
//! This module provides the canonical error type for native inference operations.
//!
//! # Error Hierarchy
//!
//! `NativeError` is the canonical error type for native inference. Other error types
//! in the native inference module convert into `NativeError`:
//!
//! - `BackendError` (from `core::backend`) - General backend errors
//! - `StorageError` (from `core::storage`) - Download and storage errors
//! - `ModelResolveError` (from `core::models`) - Model resolution errors
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::provider::native::NativeError;
//! use nika::core::BackendError;
//!
//! let backend_err = BackendError::ModelNotFound("test".to_string());
//! let native_err: NativeError = backend_err.into();
//! ```
//!
//! Canonical error type with From implementations.

use crate::core::backend::BackendError;
use crate::core::models::ModelResolveError;
use crate::core::storage::StorageError;
use std::path::PathBuf;
use thiserror::Error;

/// Error types for native inference operations.
///
/// This is the canonical error type for the native inference module.
/// Other related error types (`BackendError`, `StorageError`, `ModelResolveError`)
/// implement `From` conversion to this type.
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

    /// Inference timed out.
    #[error("Inference timed out after {timeout_secs} seconds")]
    InferenceTimeout {
        /// Timeout in seconds.
        timeout_secs: u64,
    },

    /// Operation was cancelled.
    #[error("Operation was cancelled")]
    Cancelled,

    /// Path traversal attack detected.
    #[error("Path traversal detected: '{path}' escapes storage directory")]
    PathTraversal {
        /// The invalid path.
        path: String,
    },

    /// Vision inference not supported by this model/configuration.
    #[error("Vision not supported: {reason}")]
    VisionNotSupported {
        /// Human-readable explanation.
        reason: String,
    },

    /// Failed to load a vision model from HuggingFace.
    #[error("Vision model load failed for '{model_id}': {reason}")]
    VisionModelLoadFailed {
        /// HuggingFace model ID that failed to load.
        model_id: String,
        /// Explanation of the failure.
        reason: String,
    },

    /// Image data is invalid or unsupported by the vision pipeline.
    #[error("Invalid image data: {reason}")]
    InvalidImageData {
        /// What went wrong with the image.
        reason: String,
    },

    /// Backend-specific error (catch-all for BackendError variants).
    #[error("Backend error: {0}")]
    Backend(String),

    /// Storage error (catch-all for StorageError variants).
    #[error("Storage error: {0}")]
    Storage(String),

    /// Model resolution error.
    #[error("Model resolution error: {0}")]
    ModelResolve(String),
}

// ============================================================================
// From Implementations for Error Consolidation
// ============================================================================

impl From<BackendError> for NativeError {
    fn from(err: BackendError) -> Self {
        match err {
            BackendError::NotRunning => {
                NativeError::Backend("Backend server is not running".to_string())
            }
            BackendError::ModelNotFound(name) => NativeError::ModelNotFound {
                repo: "local".to_string(),
                filename: name,
            },
            BackendError::AlreadyLoaded(name) => {
                NativeError::InvalidConfig(format!("Model already loaded: {}", name))
            }
            BackendError::InsufficientMemory => {
                NativeError::InvalidConfig("Insufficient memory to load model".to_string())
            }
            BackendError::NetworkError(msg) => NativeError::Backend(format!("Network: {}", msg)),
            BackendError::ProcessError(msg) => NativeError::Backend(format!("Process: {}", msg)),
            BackendError::BackendSpecific(msg) => NativeError::Backend(msg),
            BackendError::MissingApiKey(provider) => {
                NativeError::InvalidConfig(format!("Missing API key for {}", provider))
            }
            BackendError::ApiError { status, message } => {
                NativeError::Backend(format!("API error ({}): {}", status, message))
            }
            BackendError::ParseError(msg) => NativeError::Backend(format!("Parse error: {}", msg)),
            BackendError::LoadError(msg) => {
                NativeError::InferenceFailed(format!("Load error: {}", msg))
            }
            BackendError::InferenceError(msg) => NativeError::InferenceFailed(msg),
            BackendError::InvalidConfig(msg) => NativeError::InvalidConfig(msg),
            BackendError::StorageError(msg) => NativeError::Storage(msg),
            BackendError::DownloadError(msg) => NativeError::Storage(format!("Download: {}", msg)),
            BackendError::ChecksumError { expected, actual } => NativeError::ChecksumMismatch {
                path: PathBuf::new(),
                expected,
                actual,
            },
            BackendError::PathTraversal { path } => NativeError::PathTraversal { path },
        }
    }
}

impl From<StorageError> for NativeError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::ModelNotFound { repo, filename } => {
                NativeError::ModelNotFound { repo, filename }
            }
            StorageError::ChecksumMismatch {
                path,
                expected,
                actual,
            } => NativeError::ChecksumMismatch {
                path,
                expected,
                actual,
            },
            StorageError::InvalidConfig(msg) => NativeError::InvalidConfig(msg),
            StorageError::Network(err) => NativeError::Network(err),
            StorageError::Io(err) => NativeError::Io(err),
        }
    }
}

impl From<ModelResolveError> for NativeError {
    fn from(err: ModelResolveError) -> Self {
        match err {
            ModelResolveError::UnknownModel { id } => NativeError::ModelNotFound {
                repo: "curated".to_string(),
                filename: id,
            },
            ModelResolveError::QuantizationNotAvailable {
                quantization,
                model_id,
            } => NativeError::InvalidConfig(format!(
                "Quantization {} not available for {}",
                quantization, model_id
            )),
            ModelResolveError::HomeDirectoryNotFound => {
                NativeError::InvalidConfig("Cannot determine home directory".to_string())
            }
            ModelResolveError::ModelNotDownloaded { model_id } => NativeError::ModelNotFound {
                repo: "local".to_string(),
                filename: model_id,
            },
            ModelResolveError::SnapshotsDirReadError { path, message } => {
                NativeError::Storage(format!(
                    "Cannot read snapshots directory {}: {}",
                    path.display(),
                    message
                ))
            }
            ModelResolveError::NoSnapshotsFound { model_id } => NativeError::ModelNotFound {
                repo: "cache".to_string(),
                filename: model_id,
            },
            ModelResolveError::ModelFileNotFound { path, model_id } => NativeError::ModelNotFound {
                repo: path.to_string_lossy().to_string(),
                filename: model_id,
            },
        }
    }
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

    // ========================================================================
    // From<BackendError> tests
    // ========================================================================

    #[test]
    fn test_from_backend_error_model_not_found() {
        let backend_err = BackendError::ModelNotFound("test-model".to_string());
        let native_err: NativeError = backend_err.into();
        match native_err {
            NativeError::ModelNotFound { repo, filename } => {
                assert_eq!(repo, "local");
                assert_eq!(filename, "test-model");
            }
            _ => panic!("Expected ModelNotFound variant"),
        }
    }

    #[test]
    fn test_from_backend_error_path_traversal() {
        let backend_err = BackendError::PathTraversal {
            path: "../../../etc/passwd".to_string(),
        };
        let native_err: NativeError = backend_err.into();
        match native_err {
            NativeError::PathTraversal { path } => {
                assert_eq!(path, "../../../etc/passwd");
            }
            _ => panic!("Expected PathTraversal variant"),
        }
    }

    #[test]
    fn test_from_backend_error_insufficient_memory() {
        let backend_err = BackendError::InsufficientMemory;
        let native_err: NativeError = backend_err.into();
        assert!(matches!(native_err, NativeError::InvalidConfig(_)));
    }

    // ========================================================================
    // From<StorageError> tests
    // ========================================================================

    #[test]
    fn test_from_storage_error_model_not_found() {
        let storage_err = StorageError::ModelNotFound {
            repo: "Qwen/Qwen3-8B-GGUF".to_string(),
            filename: "Qwen3-8B-Q4_K_M.gguf".to_string(),
        };
        let native_err: NativeError = storage_err.into();
        match native_err {
            NativeError::ModelNotFound { repo, filename } => {
                assert_eq!(repo, "Qwen/Qwen3-8B-GGUF");
                assert_eq!(filename, "Qwen3-8B-Q4_K_M.gguf");
            }
            _ => panic!("Expected ModelNotFound variant"),
        }
    }

    #[test]
    fn test_from_storage_error_checksum_mismatch() {
        let storage_err = StorageError::ChecksumMismatch {
            path: PathBuf::from("/tmp/model.gguf"),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let native_err: NativeError = storage_err.into();
        match native_err {
            NativeError::ChecksumMismatch {
                path,
                expected,
                actual,
            } => {
                assert_eq!(path, PathBuf::from("/tmp/model.gguf"));
                assert_eq!(expected, "abc123");
                assert_eq!(actual, "def456");
            }
            _ => panic!("Expected ChecksumMismatch variant"),
        }
    }

    // ========================================================================
    // From<ModelResolveError> tests
    // ========================================================================

    #[test]
    fn test_from_model_resolve_error_unknown_model() {
        let resolve_err = ModelResolveError::UnknownModel {
            id: "nonexistent:7b".to_string(),
        };
        let native_err: NativeError = resolve_err.into();
        match native_err {
            NativeError::ModelNotFound { repo, filename } => {
                assert_eq!(repo, "curated");
                assert_eq!(filename, "nonexistent:7b");
            }
            _ => panic!("Expected ModelNotFound variant"),
        }
    }

    #[test]
    fn test_from_model_resolve_error_not_downloaded() {
        let resolve_err = ModelResolveError::ModelNotDownloaded {
            model_id: "qwen3:8b".to_string(),
        };
        let native_err: NativeError = resolve_err.into();
        match native_err {
            NativeError::ModelNotFound { repo, filename } => {
                assert_eq!(repo, "local");
                assert_eq!(filename, "qwen3:8b");
            }
            _ => panic!("Expected ModelNotFound variant"),
        }
    }

    // ========================================================================
    // Vision error variant display tests
    // ========================================================================

    #[test]
    fn test_vision_not_supported_display() {
        let err = NativeError::VisionNotSupported {
            reason: "Model is text-only GGUF".to_string(),
        };
        assert!(err.to_string().contains("Vision not supported"));
        assert!(err.to_string().contains("text-only GGUF"));
    }

    #[test]
    fn test_vision_model_load_failed_display() {
        let err = NativeError::VisionModelLoadFailed {
            model_id: "Qwen/Qwen2.5-VL-7B".to_string(),
            reason: "network timeout".to_string(),
        };
        assert!(err.to_string().contains("Qwen/Qwen2.5-VL-7B"));
        assert!(err.to_string().contains("network timeout"));
    }

    #[test]
    fn test_invalid_image_data_display() {
        let err = NativeError::InvalidImageData {
            reason: "corrupt PNG header".to_string(),
        };
        assert!(err.to_string().contains("Invalid image data"));
        assert!(err.to_string().contains("corrupt PNG"));
    }
}
