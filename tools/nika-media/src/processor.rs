// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MediaProcessor: ContentBlock -> decode -> detect -> hash -> store -> MediaRef
//!
//! Fully async — CasStore::store() uses io::atomic internally.
//! Rejects base64 input exceeding MAX_BASE64_INPUT_BYTES before decoding.
//! Rejects empty decoded data.
//! Enforces per-run media budget via MediaBudget.

use std::sync::Arc;

use nika_mcp::ContentBlock;

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
    #[allow(dead_code)] // Used in tests
    pub fn new(store: CasStore) -> Self {
        Self {
            store,
            budget: Arc::new(MediaBudget::new()),
        }
    }

    /// Create a new processor with custom owned budget.
    #[allow(dead_code)] // Used in tests
    pub fn with_budget(store: CasStore, budget: MediaBudget) -> Self {
        Self {
            store,
            budget: Arc::new(budget),
        }
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
                self.process_base64(data, Some(mime_type.as_str()), task_id)
                    .await
            }

            ContentBlock::Audio { data, mime_type } => {
                self.process_base64(data, Some(mime_type.as_str()), task_id)
                    .await
            }

            ContentBlock::Resource(rc) => {
                if let Some(blob) = &rc.blob {
                    self.process_base64(blob, rc.mime_type.as_deref(), task_id)
                        .await
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
        tracing::debug!(
            task_id = %task_id,
            base64_len = base64_data.len(),
            server_mime = ?server_mime,
            "media: processing base64 block"
        );

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
        let clean_b64: String = base64_data
            .chars()
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

        tracing::trace!(
            task_id = %task_id,
            decoded_bytes = decoded.len(),
            budget_used = self.budget.current_bytes(),
            "media: base64 decoded successfully"
        );

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

        tracing::debug!(
            task_id = %task_id,
            hash = %store_result.hash,
            mime = %detected.mime_type,
            size = store_result.size,
            dedup = store_result.deduplicated,
            verified = store_result.verified,
            pipeline_ms = store_result.pipeline_ms,
            "media: stored in CAS"
        );

        // Auto-enrich metadata for images
        let mut metadata = serde_json::Map::new();
        if detected.mime_type.starts_with("image/") {
            // Dimensions (fast, header-only via imagesize)
            if let Ok(size) = imagesize::blob_size(&decoded) {
                metadata.insert("width".into(), serde_json::json!(size.width));
                metadata.insert("height".into(), serde_json::json!(size.height));
            }

            // ThumbHash (skip images > 10MB to avoid excessive CPU)
            if decoded.len() < 10_000_000 {
                if let Some(hash) = compute_thumbhash_for_enrichment(&decoded) {
                    metadata.insert("thumbhash".into(), serde_json::json!(hash));
                }
            }
        }

        let media_ref = MediaRef {
            hash: store_result.hash.clone(),
            mime_type: detected.mime_type,
            size_bytes: store_result.size,
            path: store_result.path.clone(),
            extension: detected.extension,
            created_by: task_id.to_string(),
            metadata,
        };

        Ok(Some((media_ref, store_result)))
    }
}

/// Compute a thumbhash for auto-enrichment.
///
/// Best-effort: returns None if decoding fails.
/// Uses the image crate when available, otherwise returns a content-based hash.
fn compute_thumbhash_for_enrichment(data: &[u8]) -> Option<String> {
    #[cfg(feature = "media-thumbnail")]
    {
        // SECURITY: Use image::Limits — never load_from_memory() directly.
        // Enrichment is best-effort, so errors silently return None.
        let img = {
            use image::ImageReader;
            use std::io::Cursor;
            let mut reader = ImageReader::new(Cursor::new(data))
                .with_guessed_format()
                .ok()?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(256 * 1024 * 1024); // 256 MB
            limits.max_image_width = Some(16384);
            limits.max_image_height = Some(16384);
            reader.limits(limits);
            reader.decode().ok()?
        };
        let small = img.resize(100, 100, image::imageops::FilterType::Triangle);
        let rgba = small.to_rgba8();
        let (w, h) = rgba.dimensions();
        let hash = thumbhash::rgba_to_thumb_hash(w as usize, h as usize, rgba.as_raw());
        use base64::Engine;
        Some(base64::engine::general_purpose::STANDARD.encode(&hash))
    }

    #[cfg(not(feature = "media-thumbnail"))]
    {
        // Without image crate, cannot decode pixels for real thumbhash
        let _ = data;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use nika_mcp::ContentBlock;

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
        assert!(
            result.is_err(),
            "Empty base64 image should error (NIKA-258)"
        );
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

    // ═══════════════════════════════════════════════════════════════
    // GAP 1: Budget rollback on MIME detection failure
    // When MIME detection fails AFTER budget was charged, the budget
    // must be rolled back so future tasks are not unfairly penalized.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn budget_rollback_on_mime_detection_failure() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = Arc::new(MediaBudget::with_max_per_run(1000));
        let processor = MediaProcessor::with_shared_budget(store, Arc::clone(&budget));

        // Encode random bytes that infer cannot detect as any MIME type.
        // Use application/octet-stream as server hint, which is explicitly
        // rejected by detect_mime, causing MimeDetectionFailed.
        let unknown_data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&unknown_data);

        // Verify budget starts at 0
        assert_eq!(budget.current_bytes(), 0, "budget should start at 0");

        // Process should fail on MIME detection (NIKA-251)
        let block = ContentBlock::image(b64, "application/octet-stream");
        let result = processor.process(&block, "t_mime_fail").await;
        assert!(
            result.is_err(),
            "unrecognizable bytes with octet-stream should fail"
        );
        assert_eq!(result.unwrap_err().code(), "NIKA-251");

        // CRITICAL: budget must be rolled back to 0
        assert_eq!(
            budget.current_bytes(),
            0,
            "budget must be rolled back after MIME detection failure, got {}",
            budget.current_bytes()
        );

        // Prove the budget is usable: a valid image should still succeed
        let block2 = ContentBlock::image(png_base64(), "image/png");
        let result2 = processor.process(&block2, "t_after_rollback").await;
        assert!(
            result2.is_ok(),
            "valid image should succeed after rollback: {:?}",
            result2.err()
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 2: Budget rollback on CAS store failure
    // When CAS store fails AFTER budget was charged (e.g. I/O error),
    // the budget must be rolled back.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn budget_rollback_on_cas_store_failure() {
        // Create a CAS store pointing to a read-only or nonexistent path
        // to force a MediaStoreIo error during store()
        let dir = tempfile::tempdir().unwrap();
        let readonly_path = dir.path().join("readonly_store");
        // Create the directory then make it read-only
        std::fs::create_dir_all(&readonly_path).unwrap();

        // On macOS/Linux: make the directory read-only so CAS writes fail
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&readonly_path, std::fs::Permissions::from_mode(0o444))
                .unwrap();
        }

        let store = CasStore::new(&readonly_path);
        let budget = Arc::new(MediaBudget::with_max_per_run(10000));
        let processor = MediaProcessor::with_shared_budget(store, Arc::clone(&budget));

        assert_eq!(budget.current_bytes(), 0, "budget should start at 0");

        let block = ContentBlock::image(png_base64(), "image/png");
        let result = processor.process(&block, "t_cas_fail").await;

        // Should fail with I/O error (NIKA-255)
        assert!(result.is_err(), "CAS store to read-only dir should fail");
        let err = result.unwrap_err();
        assert_eq!(
            err.code(),
            "NIKA-255",
            "expected MediaStoreIo (NIKA-255), got {}",
            err.code()
        );

        // CRITICAL: budget must be rolled back to 0
        assert_eq!(
            budget.current_bytes(),
            0,
            "budget must be rolled back after CAS store failure, got {}",
            budget.current_bytes()
        );

        // Restore permissions for cleanup
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&readonly_path, std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
    }

    // ═══════════════════════════════════════════
    // AUTO-ENRICHMENT TESTS
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn enrichment_png_has_dimensions() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image(png_base64(), "image/png");
        let result = processor.process(&block, "t1").await.unwrap();
        let (media_ref, _) = result.unwrap();

        // PNG should have width/height in metadata
        assert!(
            media_ref.metadata.contains_key("width"),
            "metadata should contain 'width', got: {:?}",
            media_ref.metadata
        );
        assert!(
            media_ref.metadata.contains_key("height"),
            "metadata should contain 'height', got: {:?}",
            media_ref.metadata
        );
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn enrichment_png_has_thumbhash() {
        // Need a FULL decodable PNG for thumbhash (png_base64 is just header)
        let full_png = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(4, 4, Rgb([255u8, 0, 0]));
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                4,
                4,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            base64::engine::general_purpose::STANDARD.encode(&buf)
        };

        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image(full_png, "image/png");
        let result = processor.process(&block, "t1").await.unwrap();
        let (media_ref, _) = result.unwrap();

        // Should have thumbhash
        assert!(
            media_ref.metadata.contains_key("thumbhash"),
            "metadata should contain 'thumbhash', got: {:?}",
            media_ref.metadata
        );
        let th = media_ref
            .metadata
            .get("thumbhash")
            .unwrap()
            .as_str()
            .unwrap();
        // Verify it's valid base64
        assert!(
            base64::engine::general_purpose::STANDARD.decode(th).is_ok(),
            "thumbhash should be valid base64: {th}"
        );
    }

    #[tokio::test]
    async fn enrichment_non_image_no_dimensions() {
        // WAV RIFF header (minimal)
        let wav_data: Vec<u8> = vec![
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0x24, 0x00, 0x00, 0x00, // chunk size
            0x57, 0x41, 0x56, 0x45, // "WAVE"
            0x66, 0x6D, 0x74, 0x20, // "fmt "
            0x10, 0x00, 0x00, 0x00, // subchunk size
            0x01, 0x00, // PCM
            0x01, 0x00, // mono
            0x44, 0xAC, 0x00, 0x00, // 44100 Hz
            0x88, 0x58, 0x01, 0x00, // byte rate
            0x02, 0x00, // block align
            0x10, 0x00, // bits per sample
            0x64, 0x61, 0x74, 0x61, // "data"
            0x00, 0x00, 0x00, 0x00, // data size
        ];
        let wav_b64 = base64::engine::general_purpose::STANDARD.encode(&wav_data);
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::audio(wav_b64, "audio/wav");
        let result = processor.process(&block, "t1").await.unwrap();
        let (media_ref, _) = result.unwrap();

        // Audio should NOT have dimensions or thumbhash
        assert!(
            !media_ref.metadata.contains_key("width"),
            "audio should not have width in metadata"
        );
        assert!(
            !media_ref.metadata.contains_key("thumbhash"),
            "audio should not have thumbhash in metadata"
        );
    }

    #[tokio::test]
    async fn enrichment_metadata_serializes_cleanly() {
        let (processor, _dir) = make_processor_with_dir();
        let block = ContentBlock::image(png_base64(), "image/png");
        let result = processor.process(&block, "t1").await.unwrap();
        let (media_ref, _) = result.unwrap();

        // Roundtrip through JSON
        let json = serde_json::to_string(&media_ref).unwrap();
        let back: MediaRef = serde_json::from_str(&json).unwrap();
        assert_eq!(media_ref, back);

        // With metadata populated, the JSON should contain the metadata
        if media_ref.metadata.contains_key("width") {
            assert!(json.contains("metadata"));
            assert!(json.contains("width"));
        }
    }

    #[tokio::test]
    async fn enrichment_empty_metadata_not_serialized() {
        // Test that empty metadata is skipped in serialization
        let mr = MediaRef {
            hash: "blake3:test".to_string(),
            mime_type: "text/plain".to_string(),
            size_bytes: 10,
            path: std::path::PathBuf::from("/tmp/test"),
            extension: "txt".to_string(),
            created_by: "test".to_string(),
            metadata: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&mr).unwrap();
        assert!(
            !json.contains("metadata"),
            "empty metadata should not appear in JSON: {json}"
        );
    }
}
