//! MediaProcessor: ContentBlock -> decode -> detect -> hash -> store -> MediaRef
//!
//! Fully async — CasStore::store() uses io::atomic internally.
//! Rejects base64 input exceeding MAX_BASE64_INPUT_BYTES before decoding.
//! Rejects empty decoded data.
//! Enforces per-run media budget via MediaBudget.

use std::sync::Arc;

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
    budget: Arc<MediaBudget>,
}

impl MediaProcessor {
    /// Create a new processor with the given CAS store and default budget.
    pub fn new(store: CasStore) -> Self {
        Self {
            store,
            budget: Arc::new(MediaBudget::new()),
        }
    }

    /// Create a new processor with custom owned budget.
    pub fn with_budget(store: CasStore, budget: MediaBudget) -> Self {
        Self { store, budget: Arc::new(budget) }
    }

    /// Create a new processor with a shared per-run budget.
    pub fn with_shared_budget(store: CasStore, budget: Arc<MediaBudget>) -> Self {
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
                    tracing::error!(
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

        // Guard: reject empty data — malformed Image/Audio block
        if base64_data.is_empty() {
            return Err(MediaError::EmptyMediaContent {
                task_id: task_id.to_string(),
            });
        }

        // Decode base64 — strip whitespace first since many MCP servers
        // return PEM-style base64 with \n at 76-char boundaries (OpenAI, etc.)
        use base64::Engine;
        let clean_b64: String = base64_data.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&clean_b64)
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
        let decoded_size = decoded.len() as u64;
        self.budget.check_and_add(decoded_size, task_id)?;

        // Detect MIME type
        let detected = match detect_mime(&decoded, server_mime) {
            Ok(d) => d,
            Err(e) => {
                self.budget.rollback(decoded_size);
                return Err(e);
            }
        };

        // Store in CAS (hash-only filenames)
        let store_result = match self.store.store(&decoded).await {
            Ok(r) => r,
            Err(e) => {
                self.budget.rollback(decoded_size);
                return Err(e);
            }
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::ContentBlock;
    use base64::Engine;

    fn make_processor_with_dir() -> (MediaProcessor, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        (MediaProcessor::new(store), dir)
    }

    /// Encode PNG header as base64 for tests
    fn png_base64() -> String {
        let png_data: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
            0x49, 0x48, 0x44, 0x52, // IHDR
            0x00, 0x00, 0x00, 0x01, // width=1
            0x00, 0x00, 0x00, 0x01, // height=1
            0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, etc
        ];
        base64::engine::general_purpose::STANDARD.encode(&png_data)
    }

    #[tokio::test]
    async fn process_text_block_returns_none() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::text("hello");
        let result = processor.process(&block, "t1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn process_image_block_returns_media_ref() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image(png_base64(), "image/png");
        let result = processor.process(&block, "t1").await.unwrap();
        assert!(result.is_some());
        let (media_ref, _store_result) = result.unwrap();
        assert!(media_ref.hash.starts_with("blake3:"));
        assert_eq!(media_ref.mime_type, "image/png");
        assert_eq!(media_ref.extension, "png");
        assert!(media_ref.size_bytes > 0);
        assert_eq!(media_ref.created_by, "t1");
    }

    #[tokio::test]
    async fn process_invalid_base64_returns_error() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image("not!valid!base64!!!", "image/png");
        let result = processor.process(&block, "t1").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "NIKA-256");
    }

    #[tokio::test]
    async fn process_empty_base64_returns_error() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image("", "image/png");
        let result = processor.process(&block, "t1").await;
        assert!(result.is_err(), "Empty base64 image should error (NIKA-258)");
        assert_eq!(result.unwrap_err().code(), "NIKA-258");
    }

    #[tokio::test]
    async fn process_oversized_base64_returns_error() {
        let (processor, _dir) = make_processor_with_dir();
        // Create a string > 100MB
        let big = "A".repeat(MAX_BASE64_INPUT_BYTES + 1);
        let block = ContentBlock::image(big, "image/png");
        let result = processor.process(&block, "t1").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "NIKA-257");
    }

    #[tokio::test]
    async fn process_all_mixed_blocks() {
        let (processor, _dir) = make_processor_with_dir();
        let blocks = vec![
            ContentBlock::text("some text"),
            ContentBlock::image(png_base64(), "image/png"),
            ContentBlock::text("more text"),
        ];
        let results = processor.process_all(&blocks, "t1").await;
        // Should only have 1 result (the image), text blocks are skipped
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
    }

    #[tokio::test]
    async fn resource_link_skipped() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::resource_link("file:///test", None, None);
        let result = processor.process(&block, "t1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn budget_enforcement() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = MediaBudget::with_max_per_run(50); // Tiny budget
        let processor = MediaProcessor::with_budget(store, budget);

        let block = ContentBlock::image(png_base64(), "image/png");
        // First process should succeed (PNG header is ~25 bytes)
        let r1 = processor.process(&block, "t1").await;
        assert!(r1.is_ok());

        // Second process should fail (budget exceeded)
        let r2 = processor.process(&block, "t2").await;
        assert!(r2.is_err());
        assert_eq!(r2.unwrap_err().code(), "NIKA-259");
    }
}
