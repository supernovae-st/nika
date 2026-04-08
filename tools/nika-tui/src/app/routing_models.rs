// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native Model Management
//!
//! Handles pull, delete, and refresh operations for native GGUF models.
//! These are async operations spawned as background tasks via `spawn_tracked()`.

use nika_engine::core::backend::DownloadRequest;
use nika_engine::core::models::find_model;
use nika_engine::core::storage::{default_model_dir, HuggingFaceStorage};
use nika_engine::provider::rig::StreamChunk;

use super::App;

impl App {
    /// Pull a native model from HuggingFace.
    ///
    /// Spawns a background task to download the model with progress updates
    /// via the stream_chunk channel.
    pub(crate) fn pull_native_model(&mut self, model: String) {
        self.set_status(&format!("Starting pull: {}", model));

        // Look up the model in KNOWN_MODELS
        let known_model = match find_model(&model) {
            Some(m) => m,
            None => {
                self.set_status(&format!("Unknown model: {}", model));
                tracing::warn!("Unknown model ID: {}", model);
                return;
            }
        };

        // Clone data for the async task
        let tx = self.stream_chunk_tx.clone();
        let model_clone = model.clone();
        let repo = known_model.hf_repo.to_string();
        let filename = known_model.default_file.to_string();

        // Send start event
        let _ = tx.try_send(StreamChunk::NativeModelPullStarted {
            model: model.clone(),
        });

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::NativeModelPullFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                    return;
                }
            };

            // Create download request
            let request = DownloadRequest::huggingface(&repo, &filename);

            // Clone tx for progress callback
            let progress_tx = tx.clone();
            let progress_model = model_clone.clone();

            // Download with progress
            let result = storage
                .download(&request, move |progress| {
                    let _ = progress_tx.try_send(StreamChunk::NativeModelPullProgress {
                        model: progress_model.clone(),
                        status: progress.status.clone(),
                        completed: progress.completed,
                        total: progress.total,
                    });
                })
                .await;

            match result {
                Ok(download_result) => {
                    tracing::info!(
                        "Model {} downloaded to {:?} ({} bytes)",
                        model_clone,
                        download_result.path,
                        download_result.size
                    );
                    let _ = tx
                        .send(StreamChunk::NativeModelPulled {
                            model: model_clone,
                            path: download_result.path.display().to_string(),
                            size: download_result.size,
                        })
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to pull model {}: {}", model_clone, e);
                    let _ = tx
                        .send(StreamChunk::NativeModelPullFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });
    }

    /// Delete a native model from local storage.
    pub(crate) fn delete_native_model(&mut self, model: String) {
        self.set_status(&format!("Deleting: {}", model));

        let tx = self.stream_chunk_tx.clone();
        let model_clone = model.clone();

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleteFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                    return;
                }
            };

            // Delete the model
            match storage.delete(&model_clone) {
                Ok(()) => {
                    tracing::info!("Deleted model: {}", model_clone);
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleted { model: model_clone })
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to delete model {}: {}", model_clone, e);
                    let _ = tx
                        .send(StreamChunk::NativeModelDeleteFailed {
                            model: model_clone,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });
    }

    /// Refresh the list of native models.
    pub(crate) fn refresh_native_models(&mut self) {
        self.set_status("Refreshing native models...");

        let tx = self.stream_chunk_tx.clone();

        self.spawn_tracked(async move {
            // Create storage
            let storage = match HuggingFaceStorage::new(default_model_dir()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create storage for refresh: {}", e);
                    return;
                }
            };

            // List models
            match storage.list_models() {
                Ok(models) => {
                    let count = models.len();
                    tracing::info!("Found {} native models", count);
                    let _ = tx.send(StreamChunk::NativeModelsRefreshed { count }).await;
                }
                Err(e) => {
                    tracing::error!("Failed to list native models: {}", e);
                }
            }
        });
    }
}
