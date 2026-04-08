// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Curated model definitions for native inference via mistral.rs.
//!
//! The pure data catalog (types, statics, lookup functions) lives in
//! [`nika_core::catalogs::models`]. This module re-exports everything from
//! there and adds runtime-dependent functions that require `crate::` access:
//!
//! - [`detect_available_ram_gb`] -- reads system RAM via `crate::util::system`
//! - [`resolve_model`] -- resolves a model ID to a local HuggingFace cache path
//! - [`ResolvedModel`] -- result of model resolution

// Re-export the entire catalog from nika-core
pub use nika_core::catalogs::models::*;

use std::path::PathBuf;

/// A resolved model with full path information.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// The known model definition
    pub model: &'static KnownModel,
    /// Selected quantization
    pub quantization: Quantization,
    /// Full path to the GGUF file
    pub path: PathBuf,
}

/// Detect available system RAM in gigabytes with headroom.
///
/// Returns 80% of total RAM to leave headroom for the OS and other processes.
/// This is used for auto-selecting quantization levels.
///
/// This is a re-export of [`crate::util::system::get_available_ram_gb`]
/// compatibility. New code should use `crate::util::system` directly.
#[must_use]
pub fn detect_available_ram_gb() -> u32 {
    crate::util::system::get_available_ram_gb()
}

/// Resolve a model ID to a full path.
///
/// Checks the HuggingFace cache directory for downloaded models.
///
/// # Arguments
///
/// * `id` - Model ID (e.g., "qwen3:8b")
/// * `quantization` - Optional specific quantization (auto-selects if None)
///
/// # Returns
///
/// * `Ok(ResolvedModel)` if model is found and downloaded
/// * `Err(ModelResolveError)` if model not found or not downloaded
pub fn resolve_model(
    id: &str,
    quantization: Option<Quantization>,
) -> Result<ResolvedModel, ModelResolveError> {
    let model =
        find_model(id).ok_or_else(|| ModelResolveError::UnknownModel { id: id.to_string() })?;

    let quant =
        quantization.unwrap_or_else(|| auto_select_quantization(model, detect_available_ram_gb()));

    // Find the filename for this quantization
    let filename = model
        .quantizations
        .iter()
        .find(|(q, _)| *q == quant)
        .map(|(_, f)| *f)
        .ok_or_else(|| ModelResolveError::QuantizationNotAvailable {
            quantization: quant,
            model_id: id.to_string(),
        })?;

    // Check HuggingFace cache
    let cache_dir = dirs::home_dir()
        .ok_or(ModelResolveError::HomeDirectoryNotFound)?
        .join(".cache/huggingface/hub");

    // HF cache uses repo name with -- separator
    let repo_dir_name = format!("models--{}", model.hf_repo.replace('/', "--"));
    let snapshots_dir = cache_dir.join(&repo_dir_name).join("snapshots");

    // Find the latest snapshot
    if !snapshots_dir.exists() {
        return Err(ModelResolveError::ModelNotDownloaded {
            model_id: id.to_string(),
        });
    }

    // Get the most recent snapshot directory
    let snapshot = std::fs::read_dir(&snapshots_dir)
        .map_err(|e| ModelResolveError::SnapshotsDirReadError {
            path: snapshots_dir.clone(),
            message: e.to_string(),
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

    let snapshot_dir = snapshot.ok_or_else(|| ModelResolveError::NoSnapshotsFound {
        model_id: id.to_string(),
    })?;
    let model_path = snapshot_dir.path().join(filename);

    if !model_path.exists() {
        return Err(ModelResolveError::ModelFileNotFound {
            path: model_path,
            model_id: id.to_string(),
        });
    }

    Ok(ResolvedModel {
        model,
        quantization: quant,
        path: model_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Tests for runtime functions only.
    // Catalog tests (types, statics, find_model, etc.) live in nika-core.
    // =========================================================================

    #[test]
    fn test_resolve_model_unknown_id() {
        let err = resolve_model("nonexistent:model", None).unwrap_err();
        match err {
            ModelResolveError::UnknownModel { id } => {
                assert_eq!(id, "nonexistent:model");
            }
            other => panic!("Expected UnknownModel, got: {:?}", other),
        }
    }

    #[test]
    fn test_resolve_model_unavailable_quantization() {
        // qwen3:1.7b only has Q8_0 -- request F16 should fail
        let err = resolve_model("qwen3:1.7b", Some(Quantization::F16)).unwrap_err();
        match err {
            ModelResolveError::QuantizationNotAvailable {
                quantization,
                model_id,
            } => {
                assert_eq!(quantization, Quantization::F16);
                assert_eq!(model_id, "qwen3:1.7b");
            }
            other => panic!("Expected QuantizationNotAvailable, got: {:?}", other),
        }
    }
}
