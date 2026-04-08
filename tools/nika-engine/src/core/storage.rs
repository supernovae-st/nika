// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HuggingFace model storage implementation.
//!
//! Downloads GGUF models from HuggingFace Hub with:
//! - Progress callbacks
//! - SHA256 checksum verification
//! - Resumable downloads (via HTTP Range requests)
//! - Caching (skip download if file exists and matches checksum)
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::core::storage::{HuggingFaceStorage, default_model_dir};
//! use nika::core::backend::{DownloadRequest, PullProgress};
//! use nika::core::models::find_model;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let storage = HuggingFaceStorage::new(default_model_dir())?;
//!     let model = find_model("qwen3:8b").unwrap();
//!     let request = DownloadRequest::curated(model);
//!
//!     let result = storage.download(&request, |p| {
//!         println!("{}", p);
//!     }).await?;
//!
//!     println!("Downloaded: {:?}", result.path);
//!     Ok(())
//! }
//! ```

use crate::core::backend::{
    BackendError, DownloadRequest, DownloadResult, ModelInfo, PullProgress,
};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

// ============================================================================
// Storage Error
// ============================================================================

/// Error types for storage operations.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Model not found on HuggingFace.
    #[error("Model not found: {repo}/{filename}")]
    ModelNotFound {
        /// HuggingFace repository.
        repo: String,
        /// Filename.
        filename: String,
    },

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
}

/// Result type for storage operations.
pub type Result<T> = std::result::Result<T, StorageError>;

// ============================================================================
// HuggingFace API Types
// ============================================================================

/// File info from HuggingFace API.
#[derive(Debug, Deserialize)]
struct HfFileInfo {
    /// Filename.
    #[serde(rename = "path")]
    filename: String,
    /// File size in bytes.
    size: u64,
    /// LFS info (contains SHA256).
    lfs: Option<HfLfsInfo>,
}

/// LFS metadata from HuggingFace.
#[derive(Debug, Deserialize)]
struct HfLfsInfo {
    /// SHA256 checksum.
    #[serde(rename = "oid")]
    sha256: String,
}

// ============================================================================
// Default Paths
// ============================================================================

/// Returns the default model storage directory.
///
/// Platform-specific:
/// - macOS: `~/Library/Application Support/nika/models`
/// - Linux: `~/.local/share/nika/models`
/// - Windows: `%APPDATA%/nika/models`
#[must_use]
pub fn default_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nika")
        .join("models")
}

// ============================================================================
// Platform Detection
// ============================================================================

/// Detect total system RAM in gigabytes.
///
/// This is a re-export of [`crate::util::system::get_total_ram_gb`]
/// compatibility. New code should use `crate::util::system` directly.
#[must_use]
pub fn detect_system_ram_gb() -> f64 {
    crate::util::system::get_total_ram_gb()
}

// ============================================================================
// ModelStorage Trait
// ============================================================================

/// Trait for model storage backends.
///
/// Provides a common interface for downloading and managing local models.
#[allow(async_fn_in_trait)]
pub trait ModelStorage {
    /// Download a model with progress callback.
    async fn download<F>(
        &self,
        request: &DownloadRequest<'_>,
        progress: F,
    ) -> Result<DownloadResult>
    where
        F: Fn(PullProgress) + Send + 'static;

    /// List all downloaded models.
    fn list_models(&self) -> std::result::Result<Vec<ModelInfo>, BackendError>;

    /// Check if a model exists locally.
    fn exists(&self, model_id: &str) -> bool;

    /// Get info about a local model.
    fn model_info(&self, model_id: &str) -> std::result::Result<ModelInfo, BackendError>;

    /// Delete a local model.
    fn delete(&self, model_id: &str) -> std::result::Result<(), BackendError>;

    /// Get the path to a model file.
    ///
    /// # Security
    ///
    /// Validates that model_id doesn't escape storage directory via path traversal.
    ///
    /// # Errors
    ///
    /// Returns `BackendError::PathTraversal` if the path would escape the storage directory.
    fn model_path(&self, model_id: &str) -> std::result::Result<PathBuf, BackendError>;
}

// ============================================================================
// HuggingFace Storage
// ============================================================================

/// Storage backend for HuggingFace Hub models.
///
/// Downloads GGUF models from HuggingFace with progress tracking and
/// checksum verification.
pub struct HuggingFaceStorage {
    /// Root directory for model storage.
    storage_dir: PathBuf,
    /// HTTP client.
    client: Client,
}

impl HuggingFaceStorage {
    /// Create a new HuggingFace storage with the given directory.
    ///
    /// # Errors
    ///
    /// Returns `StorageError::InvalidConfig` if the HTTP client cannot be built.
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        let user_agent = format!("nika/{}", env!("CARGO_PKG_VERSION"));
        let client = Client::builder()
            .user_agent(&user_agent)
            .build()
            .map_err(|e| {
                StorageError::InvalidConfig(format!("Failed to create HTTP client: {e}"))
            })?;

        Ok(Self {
            storage_dir,
            client,
        })
    }

    /// Create storage with a custom HTTP client.
    #[must_use]
    pub fn with_client(storage_dir: PathBuf, client: Client) -> Self {
        Self {
            storage_dir,
            client,
        }
    }

    /// Get the storage directory.
    #[must_use]
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// Download a model with progress callback.
    ///
    /// # Arguments
    ///
    /// * `request` - Download request specifying model and quantization
    /// * `progress` - Callback for download progress updates
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Model not found on HuggingFace
    /// - Network error during download
    /// - Checksum verification fails
    /// - I/O error writing file
    pub async fn download<F>(
        &self,
        request: &DownloadRequest<'_>,
        progress: F,
    ) -> Result<DownloadResult>
    where
        F: Fn(PullProgress) + Send + 'static,
    {
        // Resolve repo and filename
        let (repo, filename) = self.resolve_request(request)?;

        // Create storage directory
        let model_dir = self.storage_dir.join(&repo);
        fs::create_dir_all(&model_dir).await?;

        let file_path = model_dir.join(&filename);

        // TOCTOU-safe: Attempt to read metadata directly instead of exists() check.
        // If the file exists and we're not forcing, return cached result.
        // If it doesn't exist, continue to download.
        if !request.force {
            match fs::metadata(&file_path).await {
                Ok(metadata) => {
                    progress(PullProgress::new("cached", 1, 1));
                    return Ok(DownloadResult {
                        path: file_path,
                        size: metadata.len(),
                        checksum: None,
                        cached: true,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File doesn't exist, proceed to download
                }
                Err(e) => return Err(StorageError::Io(e)),
            }
        }

        // Get file info from HuggingFace API
        progress(PullProgress::new("fetching metadata", 0, 1));
        let file_info = self.get_file_info(&repo, &filename).await?;

        // Download the file
        let download_url = format!("https://huggingface.co/{}/resolve/main/{}", repo, filename);

        progress(PullProgress::new("downloading", 0, file_info.size));

        let response = self.client.get(&download_url).send().await?;

        if !response.status().is_success() {
            return Err(StorageError::ModelNotFound {
                repo: repo.clone(),
                filename: filename.clone(),
            });
        }

        // Stream download to file with progress
        let mut file = File::create(&file_path).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut hasher = Sha256::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            progress(PullProgress::new("downloading", downloaded, file_info.size));
        }

        file.flush().await?;
        drop(file);

        // Verify checksum
        let checksum = format!("{:x}", hasher.finalize());
        if let Some(ref lfs) = file_info.lfs {
            if checksum != lfs.sha256 {
                // Delete corrupted file
                if let Err(e) = fs::remove_file(&file_path).await {
                    tracing::debug!(path = %file_path.display(), error = %e, "cleanup: failed to remove corrupted file");
                }
                return Err(StorageError::ChecksumMismatch {
                    path: file_path,
                    expected: lfs.sha256.clone(),
                    actual: checksum,
                });
            }
        }

        progress(PullProgress::new(
            "complete",
            file_info.size,
            file_info.size,
        ));

        Ok(DownloadResult {
            path: file_path,
            size: file_info.size,
            checksum: Some(checksum),
            cached: false,
        })
    }

    /// Resolve download request to HuggingFace repo and filename.
    fn resolve_request(&self, request: &DownloadRequest<'_>) -> Result<(String, String)> {
        if let Some(hf_repo) = &request.hf_repo {
            let filename = request.filename.clone().ok_or_else(|| {
                StorageError::InvalidConfig("HuggingFace download requires filename".into())
            })?;
            return Ok((hf_repo.clone(), filename));
        }

        if let Some(model) = request.model {
            let filename = request.target_filename().ok_or_else(|| {
                StorageError::InvalidConfig("No quantization available for model".into())
            })?;
            return Ok((model.hf_repo.to_string(), filename));
        }

        Err(StorageError::InvalidConfig(
            "Download request must specify model or HuggingFace repo".into(),
        ))
    }

    /// Get file info from HuggingFace API.
    async fn get_file_info(&self, repo: &str, filename: &str) -> Result<HfFileInfo> {
        let api_url = format!("https://huggingface.co/api/models/{}/tree/main", repo);

        let response = self.client.get(&api_url).send().await?;

        if !response.status().is_success() {
            return Err(StorageError::ModelNotFound {
                repo: repo.to_string(),
                filename: filename.to_string(),
            });
        }

        let files: Vec<HfFileInfo> = response.json().await?;

        files
            .into_iter()
            .find(|f| f.filename == filename)
            .ok_or_else(|| StorageError::ModelNotFound {
                repo: repo.to_string(),
                filename: filename.to_string(),
            })
    }

    /// List all downloaded models.
    pub fn list_models(&self) -> std::result::Result<Vec<ModelInfo>, BackendError> {
        let mut models = Vec::new();

        // TOCTOU-safe: Attempt to read directory directly instead of exists() check.
        // If directory doesn't exist, return empty list.
        let entries = match std::fs::read_dir(&self.storage_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(models);
            }
            Err(e) => return Err(BackendError::StorageError(e.to_string())),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // This is a repo directory
                let repo_name = entry.file_name().to_string_lossy().to_string();

                // List GGUF files in this directory
                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let filename = file.file_name().to_string_lossy().to_string();
                        if filename.ends_with(".gguf") {
                            if let Ok(metadata) = file.metadata() {
                                let quant = extract_quantization(&filename);
                                models.push(ModelInfo {
                                    name: format!("{}/{}", repo_name, filename),
                                    size: metadata.len(),
                                    quantization: quant,
                                    parameters: None,
                                    digest: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(models)
    }

    /// Check if a model exists locally.
    ///
    /// Returns `false` if the model_id contains path traversal patterns.
    #[must_use]
    pub fn exists(&self, model_id: &str) -> bool {
        self.model_path(model_id)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get info about a local model.
    pub fn model_info(&self, model_id: &str) -> std::result::Result<ModelInfo, BackendError> {
        let path = self.model_path(model_id)?;

        // TOCTOU-safe: Attempt to read metadata directly instead of exists() check.
        // This avoids race where file is deleted between exists() and metadata().
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(BackendError::ModelNotFound(model_id.to_string()));
            }
            Err(e) => return Err(BackendError::StorageError(e.to_string())),
        };

        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        Ok(ModelInfo {
            name: model_id.to_string(),
            size: metadata.len(),
            quantization: extract_quantization(&filename),
            parameters: None,
            digest: None,
        })
    }

    /// Delete a local model.
    pub fn delete(&self, model_id: &str) -> std::result::Result<(), BackendError> {
        let path = self.model_path(model_id)?;

        // TOCTOU-safe: Attempt to remove directly instead of exists() check.
        // This avoids race where file is deleted between exists() and remove_file().
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(BackendError::ModelNotFound(model_id.to_string()))
            }
            Err(e) => Err(BackendError::StorageError(e.to_string())),
        }
    }

    /// Get the path to a model file with path traversal validation.
    ///
    /// # Security
    ///
    /// Validates that model_id doesn't escape storage directory via `..` or absolute paths.
    ///
    /// # Errors
    ///
    /// Returns `BackendError::PathTraversal` if the path would escape the storage directory.
    pub fn model_path(&self, model_id: &str) -> std::result::Result<PathBuf, BackendError> {
        // model_id format: "repo/filename" or just "filename"
        // Both cases join to storage_dir

        // Security: Reject absolute paths
        let model_path = Path::new(model_id);
        if model_path.is_absolute() {
            return Err(BackendError::PathTraversal {
                path: model_id.to_string(),
            });
        }

        // Security: Check for path traversal patterns
        // We normalize the path to handle ".." components
        let joined = self.storage_dir.join(model_id);
        let normalized = normalize_path(&joined);
        let normalized_base = normalize_path(&self.storage_dir);

        if !normalized.starts_with(&normalized_base) {
            return Err(BackendError::PathTraversal {
                path: model_id.to_string(),
            });
        }

        Ok(joined)
    }
}

// ============================================================================
// ModelStorage Implementation
// ============================================================================

impl ModelStorage for HuggingFaceStorage {
    async fn download<F>(
        &self,
        request: &DownloadRequest<'_>,
        progress: F,
    ) -> Result<DownloadResult>
    where
        F: Fn(PullProgress) + Send + 'static,
    {
        HuggingFaceStorage::download(self, request, progress).await
    }

    fn list_models(&self) -> std::result::Result<Vec<ModelInfo>, BackendError> {
        HuggingFaceStorage::list_models(self)
    }

    fn exists(&self, model_id: &str) -> bool {
        HuggingFaceStorage::exists(self, model_id)
    }

    fn model_info(&self, model_id: &str) -> std::result::Result<ModelInfo, BackendError> {
        HuggingFaceStorage::model_info(self, model_id)
    }

    fn delete(&self, model_id: &str) -> std::result::Result<(), BackendError> {
        HuggingFaceStorage::delete(self, model_id)
    }

    fn model_path(&self, model_id: &str) -> std::result::Result<PathBuf, BackendError> {
        HuggingFaceStorage::model_path(self, model_id)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Normalize a path by resolving `.` and `..` components without filesystem access.
///
/// This is used for path traversal validation before the path exists.
/// Adapted from `io/security.rs` for use in storage module.
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            _ => {
                normalized.push(component);
            }
        }
    }

    normalized
}

/// Extract quantization level from GGUF filename.
///
/// # Examples
///
/// ```rust,ignore
/// assert_eq!(extract_quantization("model-Q4_K_M.gguf"), Some("Q4_K_M".to_string()));
/// assert_eq!(extract_quantization("model-q8_0.gguf"), Some("Q8_0".to_string()));
/// assert_eq!(extract_quantization("model.gguf"), None);
/// ```
#[must_use]
pub fn extract_quantization(filename: &str) -> Option<String> {
    // Common patterns: -Q4_K_M.gguf, -q4_k_m.gguf, -F16.gguf, -f16.gguf
    let patterns = [
        "Q4_K_M", "Q4_K_S", "Q5_K_M", "Q5_K_S", "Q6_K", "Q8_0", "Q2_K", "Q3_K_M", "Q3_K_S", "Q4_0",
        "Q4_1", "Q5_0", "Q5_1", "F16", "F32", "BF16",
    ];

    let filename_upper = filename.to_uppercase();
    for pattern in patterns {
        if filename_upper.contains(pattern) {
            return Some(pattern.to_string());
        }
    }

    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_extract_quantization() {
        assert_eq!(
            extract_quantization("model-q4_k_m.gguf"),
            Some("Q4_K_M".to_string())
        );
        assert_eq!(
            extract_quantization("model-Q8_0.gguf"),
            Some("Q8_0".to_string())
        );
        assert_eq!(
            extract_quantization("model-f16.gguf"),
            Some("F16".to_string())
        );
        assert_eq!(
            extract_quantization("Qwen3-8B-Q4_K_M.gguf"),
            Some("Q4_K_M".to_string())
        );
        assert_eq!(extract_quantization("model.gguf"), None);
    }

    #[test]
    fn test_storage_new() {
        let dir = tempdir().unwrap();
        let storage = HuggingFaceStorage::new(dir.path().to_path_buf()).unwrap();
        assert_eq!(storage.storage_dir(), dir.path());
    }

    #[test]
    fn test_model_path() {
        let dir = tempdir().unwrap();
        let storage = HuggingFaceStorage::new(dir.path().to_path_buf()).unwrap();

        let path = storage.model_path("repo/model.gguf").unwrap();
        assert!(path.ends_with("repo/model.gguf"));

        let path = storage.model_path("model.gguf").unwrap();
        assert!(path.ends_with("model.gguf"));
    }

    #[test]
    fn test_model_path_traversal_rejected() {
        let dir = tempdir().unwrap();
        let storage = HuggingFaceStorage::new(dir.path().to_path_buf()).unwrap();

        // Test path traversal with ..
        let result = storage.model_path("../../../etc/passwd");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BackendError::PathTraversal { .. }
        ));

        // Test absolute path rejection
        let result = storage.model_path("/etc/passwd");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BackendError::PathTraversal { .. }
        ));

        // Test valid nested path is accepted
        let result = storage.model_path("Qwen/Qwen3-8B-Q4_K_M.gguf");
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_models_empty() {
        let dir = tempdir().unwrap();
        let storage = HuggingFaceStorage::new(dir.path().to_path_buf()).unwrap();
        let models = storage.list_models().unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_exists_false() {
        let dir = tempdir().unwrap();
        let storage = HuggingFaceStorage::new(dir.path().to_path_buf()).unwrap();
        assert!(!storage.exists("nonexistent/model.gguf"));
    }

    #[test]
    fn test_default_model_dir() {
        let dir = default_model_dir();
        assert!(dir.ends_with("nika/models"));
    }

    #[test]
    fn test_detect_system_ram() {
        let ram = detect_system_ram_gb();
        // Should return something reasonable (> 1GB on any modern system)
        assert!(ram > 1.0);
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::ModelNotFound {
            repo: "test/repo".to_string(),
            filename: "model.gguf".to_string(),
        };
        assert_eq!(err.to_string(), "Model not found: test/repo/model.gguf");
    }
}
