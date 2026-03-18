//! MediaProcessor: ContentBlock -> decode -> detect -> hash -> store -> MediaRef
//!
//! Fully async — CasStore::store() uses io::atomic internally.
//! Rejects base64 input exceeding MAX_BASE64_INPUT_BYTES before decoding.
//! Rejects empty decoded data.
//! Enforces per-run media budget via MediaBudget.

use crate::mcp::types::ContentBlock;

use super::detect::detect_mime;
use super::error::MediaError;
use super::store::{CasStore, StoreResult};
use super::types::{MediaBudget, MediaRef};

/// Maximum base64 input size (100 MB encoded, ~75 MB decoded).
const MAX_BASE64_INPUT_BYTES: usize = 100 * 1024 * 1024;

/// Processes MCP content blocks into stored media files.
pub struct MediaProcessor {
    store: CasStore,
    budget: MediaBudget,
}

impl MediaProcessor {
    /// Create a new processor with the given CAS store and default budget.
    pub fn new(store: CasStore) -> Self {
        Self {
            store,
            budget: MediaBudget::new(),
        }
    }

    /// Create a new processor with custom budget.
    pub fn with_budget(store: CasStore, budget: MediaBudget) -> Self {
        Self { store, budget }
    }

    /// Process a single content block (async).
    ///
    /// Returns `Ok(None)` for text blocks (no media to process).
    /// Returns `Ok(Some((MediaRef, StoreResult)))` for media blocks.
    pub async fn process(
        &self,
        block: &ContentBlock,
        task_id: &str,
    ) -> Result<Option<(MediaRef, StoreResult)>, MediaError> {
        match block {
            ContentBlock::Text { .. } => Ok(None),

            ContentBlock::Image { data, mime_type } => {
                self.process_base64(data, Some(mime_type.as_str()), task_id).await
            }

            ContentBlock::Audio { data, mime_type } => {
                self.process_base64(data, Some(mime_type.as_str()), task_id).await
            }

            ContentBlock::Resource(rc) => {
                if let Some(blob) = &rc.blob {
                    self.process_base64(
                        blob,
                        rc.mime_type.as_deref(),
                        task_id,
                    ).await
                } else {
                    Ok(None)
                }
            }

            ContentBlock::ResourceLink { uri, .. } => {
                tracing::debug!(uri = %uri, "ResourceLink skipped (no inline data)");
                Ok(None)
            }
        }
    }

    /// Process all content blocks, collecting results per block (async).
    ///
    /// Skips text blocks silently. The `usize` in Err is the block index.
    pub async fn process_all(
        &self,
        blocks: &[ContentBlock],
        task_id: &str,
    ) -> Vec<Result<(MediaRef, StoreResult), (usize, MediaError)>> {
        let mut results = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            if block.is_text() {
                continue;
            }
            match self.process(block, task_id).await {
                Ok(Some(pair)) => results.push(Ok(pair)),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        task_id = %task_id,
                        block_index = i,
                        error = %e,
                        "Failed to process media block"
                    );
                    results.push(Err((i, e)));
                }
            }
        }
        results
    }

    /// Decode base64 data, detect MIME, and store in CAS (async).
    async fn process_base64(
        &self,
        base64_data: &str,
        server_mime: Option<&str>,
        task_id: &str,
    ) -> Result<Option<(MediaRef, StoreResult)>, MediaError> {
        // Guard: reject oversized base64 BEFORE decode
        if base64_data.len() > MAX_BASE64_INPUT_BYTES {
            return Err(MediaError::Base64InputTooLarge {
                size: base64_data.len(),
                max: MAX_BASE64_INPUT_BYTES,
            });
        }

        // Guard: reject empty data
        if base64_data.is_empty() {
            tracing::warn!(task_id = %task_id, "Empty base64 data, skipping");
            return Ok(None);
        }

        // Decode base64
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| MediaError::Base64DecodeFailed {
                source_desc: format!("task {task_id}"),
                reason: e.to_string(),
            })?;

        // Guard: reject empty decoded data
        if decoded.is_empty() {
            return Err(MediaError::EmptyMediaContent {
                task_id: task_id.to_string(),
            });
        }

        // Check media budget before proceeding
        self.budget.check_and_add(decoded.len() as u64, task_id)?;

        // Detect MIME type
        let detected = detect_mime(&decoded, server_mime)?;

        // Store in CAS (hash-only filenames)
        let store_result = self.store.store(&decoded).await?;

        let media_ref = MediaRef {
            hash: store_result.hash.clone(),
            mime_type: detected.mime_type,
            size_bytes: store_result.size,
            path: store_result.path.clone(),
            extension: detected.extension,
            created_by: task_id.to_string(),
        };

        Ok(Some((media_ref, store_result)))
    }
}
