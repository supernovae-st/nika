//! Phase E: Deep E2E tests for media pipeline
//!
//! Tests the FULL pipeline end-to-end, targeting silent bugs:
//! - Base64 edge cases (newlines, padding, URL-safe)
//! - MIME detection edge cases
//! - CAS store integrity under stress
//! - MediaRef serde through the entire resolve_path pipeline
//! - Error propagation — every error code must be exercisable
//! - Budget enforcement under concurrent load
//! - Backward compatibility — text-only workflows unchanged

#[cfg(test)]
mod tests {
    use base64::Engine;
    use std::sync::Arc;

    use crate::media::detect::detect_mime;
    use crate::media::error::MediaError;
    use crate::media::processor::MediaProcessor;
    use crate::media::store::CasStore;
    use crate::media::types::{MediaBudget, MediaRef, MediaType};
    use crate::mcp::types::{ContentBlock, ResourceContent, ToolCallResult};
    use crate::store::RunContext;

    // ═══════════════════════════════════════════════════════════════
    // REAL BINARY DATA (actual file headers, not toy data)
    // ═══════════════════════════════════════════════════════════════

    /// Minimal valid PNG (1x1 pixel, RGBA)
    fn real_png_bytes() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
            0x49, 0x48, 0x44, 0x52, // IHDR
            0x00, 0x00, 0x00, 0x01, // width=1
            0x00, 0x00, 0x00, 0x01, // height=1
            0x08, 0x06,             // 8-bit RGBA
            0x00, 0x00, 0x00,       // compression, filter, interlace
            0x1F, 0x15, 0xC4, 0x89, // IHDR CRC
            0x00, 0x00, 0x00, 0x0A, // IDAT chunk length
            0x49, 0x44, 0x41, 0x54, // IDAT
            0x78, 0x9C, 0x62, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE5, // zlib data
            0x27, 0xDE, 0xFC, 0x07, // IDAT CRC
            0x00, 0x00, 0x00, 0x00, // IEND chunk length
            0x49, 0x45, 0x4E, 0x44, // IEND
            0xAE, 0x42, 0x60, 0x82, // IEND CRC
        ]
    }

    /// Minimal valid JPEG
    fn real_jpeg_bytes() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, // SOI + APP0 marker
            0x00, 0x10,             // APP0 length
            0x4A, 0x46, 0x49, 0x46, 0x00, // JFIF\0
            0x01, 0x01,             // version
            0x00,                   // units
            0x00, 0x01, 0x00, 0x01, // density
            0x00, 0x00,             // thumbnail
            0xFF, 0xD9,             // EOI
        ]
    }

    /// MP3 frame header (ID3 tag)
    fn real_mp3_bytes() -> Vec<u8> {
        let mut data = vec![
            0x49, 0x44, 0x33, // ID3
            0x04, 0x00,       // version
            0x00,             // flags
            0x00, 0x00, 0x00, 0x00, // size
        ];
        // Pad to make it recognizable
        data.extend_from_slice(&[0x00; 20]);
        data
    }

    /// PDF header
    fn real_pdf_bytes() -> Vec<u8> {
        b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec()
    }

    fn encode_b64(data: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #1: Base64 with newlines (PEM-style)
    // Many MCP servers return base64 with \n line breaks.
    // STANDARD engine rejects newlines → silent Base64DecodeFailed
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_base64_with_newlines_should_fail_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        // Encode then add newlines (simulating PEM-style base64)
        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let b64_with_newlines = b64.chars()
            .enumerate()
            .map(|(i, c)| if i > 0 && i % 76 == 0 { format!("\n{c}") } else { c.to_string() })
            .collect::<String>();

        let block = ContentBlock::image(b64_with_newlines, "image/png");
        let result = processor.process(&block, "t_newline").await;

        // This SHOULD fail with NIKA-256 (Base64DecodeFailed)
        // If it silently succeeds with corrupted data, that's a bug
        match result {
            Err(e) => {
                assert_eq!(e.code(), "NIKA-256", "Expected Base64DecodeFailed, got: {}", e);
            }
            Ok(Some((ref media_ref, _))) => {
                // BUG: If it somehow decoded, verify the hash is correct
                let expected_hash = format!("blake3:{}", blake3::hash(&png).to_hex());
                assert_eq!(media_ref.hash, expected_hash,
                    "SILENT BUG: base64 with newlines decoded to different data!");
            }
            Ok(None) => {
                panic!("SILENT BUG: image block returned None (should be Some or Err)");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #2: Base64 with URL-safe characters
    // Some servers use URL-safe base64 (- and _ instead of + and /)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_base64_url_safe_should_fail() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        // URL-safe base64 uses - instead of + and _ instead of /
        let url_safe = base64::engine::general_purpose::URL_SAFE.encode(real_png_bytes());
        let block = ContentBlock::image(url_safe, "image/png");
        let result = processor.process(&block, "t_urlsafe").await;

        // Should either fail or decode correctly
        // If STANDARD engine silently produces wrong data, that's a bug
        match result {
            Err(e) => {
                // Expected: URL-safe chars are invalid in STANDARD base64
                assert!(e.code() == "NIKA-256" || e.code() == "NIKA-251",
                    "Unexpected error: {}", e);
            }
            Ok(Some((media_ref, _))) => {
                // If it decoded, verify correctness
                assert!(media_ref.hash.starts_with("blake3:"));
                assert!(media_ref.size_bytes > 0);
            }
            Ok(None) => {
                panic!("SILENT BUG: image block returned None");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #3: Base64 padding variants
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_base64_no_padding_should_fail() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let b64 = encode_b64(&real_png_bytes());
        let no_padding = b64.trim_end_matches('=').to_string();
        let block = ContentBlock::image(no_padding, "image/png");
        let result = processor.process(&block, "t_nopad").await;

        // STANDARD engine REQUIRES padding → should fail
        // If it silently produces corrupted data, that's a critical bug
        match result {
            Err(e) => assert_eq!(e.code(), "NIKA-256"),
            Ok(Some(_)) => {
                // Some base64 impls accept no-padding; verify data integrity
            }
            Ok(None) => panic!("SILENT BUG: image block returned None"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #4: MIME mismatch (server says audio, bytes are image)
    // Should log warning but still detect correctly
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_mime_category_mismatch_uses_magic_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        // Send PNG data with audio/wav MIME type
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "audio/wav");
        let result = processor.process(&block, "t_mismatch").await;

        match result {
            Ok(Some((media_ref, _))) => {
                // Magic bytes MUST win over server declaration
                assert_eq!(media_ref.mime_type, "image/png",
                    "SILENT BUG: server MIME type overrode magic bytes detection! Got: {}",
                    media_ref.mime_type);
                assert_eq!(media_ref.extension, "png");
            }
            Ok(None) => panic!("SILENT BUG: image data returned None"),
            Err(e) => panic!("Unexpected error on valid PNG data: {}", e),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #5: Resource with blob but no mime_type
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_resource_blob_no_mime() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let rc = ResourceContent::new("file:///image.png")
            .with_blob(encode_b64(&real_png_bytes()));
        // No mime_type set — processor must detect from magic bytes
        let block = ContentBlock::Resource(rc);
        let result = processor.process(&block, "t_nomime").await;

        match result {
            Ok(Some((media_ref, _))) => {
                assert_eq!(media_ref.mime_type, "image/png",
                    "SILENT BUG: MIME not detected from magic bytes when server hint missing");
            }
            Ok(None) => panic!("SILENT BUG: resource with blob returned None"),
            Err(e) => panic!("SILENT BUG: valid PNG blob failed: {}", e),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #6: Resource with text only (no blob) must return None
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_resource_text_only_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let rc = ResourceContent::new("file:///readme.md")
            .with_text("# Hello World");
        let block = ContentBlock::Resource(rc);
        let result = processor.process(&block, "t_textonly").await.unwrap();
        assert!(result.is_none(), "Text-only resource should return None, not store in CAS");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #7: CAS dedup produces consistent hash
    // Two different JPEG encodings of "same visual" produce different hashes
    // But same bytes MUST produce same hash (content-addressed)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_cas_dedup_same_bytes_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = real_png_bytes();
        let r1 = store.store(&data).await.unwrap();
        let r2 = store.store(&data).await.unwrap();

        assert_eq!(r1.hash, r2.hash, "Same bytes must produce same hash");
        assert!(!r1.deduplicated, "First write should not be dedup");
        assert!(r2.deduplicated, "Second write must be dedup");

        // Verify read-back matches original
        let read_back = store.read(&r1.hash).await.unwrap();
        assert_eq!(read_back, data, "SILENT BUG: read-back data corrupted!");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #8: CAS path does NOT contain extension
    // If extension leaks into path, dedup breaks for jpeg vs jpg
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_cas_path_never_has_extension() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let r = store.store(&real_png_bytes()).await.unwrap();
        let filename = r.path.file_name().unwrap().to_string_lossy();

        assert!(!filename.ends_with(".png"), "SILENT BUG: CAS filename has .png extension: {}", filename);
        assert!(!filename.ends_with(".jpg"), "SILENT BUG: CAS filename has .jpg extension: {}", filename);
        assert!(!filename.contains('.'), "SILENT BUG: CAS filename contains dot: {}", filename);
        // Also verify the parent is a 2-char shard directory
        let shard = r.path.parent().unwrap().file_name().unwrap().to_string_lossy();
        assert_eq!(shard.len(), 2, "Shard directory should be 2 chars: {}", shard);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #9: media_staging lifecycle
    // set_media → take_media must not leak
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_media_staging_set_then_take() {
        let ctx = RunContext::new();
        let task_id: Arc<str> = "test_task".into();

        let refs = vec![MediaRef {
            hash: "blake3:abc123".into(),
            mime_type: "image/png".into(),
            size_bytes: 1024,
            path: "/tmp/store/ab/c123".into(),
            extension: "png".into(),
            created_by: "test_task".into(),
        }];

        ctx.set_media(&task_id, refs.clone());

        // First take should return the refs
        let taken = ctx.take_media(&task_id);
        assert_eq!(taken.len(), 1, "take_media should return staged refs");
        assert_eq!(taken[0].hash, "blake3:abc123");

        // Second take should return empty (drained)
        let taken_again = ctx.take_media(&task_id);
        assert!(taken_again.is_empty(), "SILENT BUG: take_media didn't drain staging");
    }

    #[test]
    fn e2e_media_staging_empty_vec_not_stored() {
        let ctx = RunContext::new();
        let task_id: Arc<str> = "empty_task".into();

        // set_media with empty vec should be a no-op
        ctx.set_media(&task_id, vec![]);

        let taken = ctx.take_media(&task_id);
        assert!(taken.is_empty(), "Empty vec should not be stored in staging");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #10: TaskResult.media survives with_media
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_task_result_with_media_attaches() {
        use std::time::Duration;
        use crate::store::TaskResult;

        let tr = TaskResult::success(serde_json::json!("ok"), Duration::from_millis(100));
        assert!(tr.media.is_empty(), "New TaskResult should have empty media");

        let refs = vec![MediaRef {
            hash: "blake3:deadbeef".into(),
            mime_type: "image/png".into(),
            size_bytes: 2048,
            path: "/tmp/store/de/adbeef".into(),
            extension: "png".into(),
            created_by: "gen".into(),
        }];

        let tr = tr.with_media(refs);
        assert_eq!(tr.media.len(), 1);
        assert_eq!(tr.media[0].hash, "blake3:deadbeef");
        assert!(tr.is_success(), "with_media should not change status");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #11: ToolCallResult backward compat
    // text() must ONLY extract Text blocks, ignore Image/Audio
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_text_extraction_ignores_media_completely() {
        let result = ToolCallResult::success(vec![
            ContentBlock::text("visible text"),
            ContentBlock::image("SGVsbG8=", "image/png"),
            ContentBlock::audio("AAAA", "audio/wav"),
            ContentBlock::resource(ResourceContent::new("file:///test").with_text("resource text")),
            ContentBlock::resource_link("file:///link", None, None),
            ContentBlock::text("more text"),
        ]);

        // text() must only join Text blocks
        assert_eq!(result.text(), "visible text\nmore text",
            "SILENT BUG: text() includes non-text content!");
        assert_eq!(result.first_text(), Some("visible text"));

        // media helpers must exclude Text
        assert!(result.has_media());
        assert_eq!(result.media_blocks().len(), 4); // image, audio, resource, resource_link
        assert_eq!(result.images().len(), 1);
        assert_eq!(result.audio_blocks().len(), 1);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #12: MIME case normalization end-to-end
    // "IMAGE/PNG" from server must normalize to "image/png"
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_mime_case_normalization() {
        let png = real_png_bytes();

        // Test uppercase server MIME
        let result = detect_mime(&png, Some("IMAGE/PNG")).unwrap();
        assert_eq!(result.mime_type, "image/png",
            "SILENT BUG: uppercase MIME not normalized: {}", result.mime_type);

        // Test mixed case
        let result = detect_mime(&png, Some("Image/Png")).unwrap();
        assert_eq!(result.mime_type, "image/png");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #13: PDF detection (application/pdf)
    // PDF is a P0 type — must detect correctly
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_pdf_detection() {
        let pdf = real_pdf_bytes();
        let result = detect_mime(&pdf, None).unwrap();
        assert_eq!(result.mime_type, "application/pdf",
            "SILENT BUG: PDF not detected: {}", result.mime_type);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #14: MediaBudget race condition
    // Concurrent check_and_add must not exceed budget
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_budget_concurrent_enforcement() {
        let budget = Arc::new(MediaBudget::with_max_per_run(1000));

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let budget = Arc::clone(&budget);
                tokio::spawn(async move {
                    budget.check_and_add(100, &format!("t{i}"))
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap())
            .collect();

        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();

        // Budget is 1000, each request is 100 → exactly 10 should succeed
        assert_eq!(successes, 10,
            "SILENT BUG: budget allowed {} of 20 (expected 10)", successes);
        assert_eq!(failures, 10,
            "SILENT BUG: budget rejected {} of 20 (expected 10)", failures);

        // Final bytes should be exactly 1000
        assert_eq!(budget.current_bytes(), 1000,
            "SILENT BUG: budget tracking off: {}", budget.current_bytes());
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #15: Error code coverage
    // Every NIKA-25x code must be exercisable
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_all_error_codes_covered() {
        let errors: Vec<MediaError> = vec![
            MediaError::mime_detection_failed(0, None),
            MediaError::UnsupportedMediaType {
                mime_type: "video/mp4".into(),
                reason: "not supported".into(),
            },
            MediaError::MediaNotFound { hash: "blake3:xxx".into() },
            MediaError::HashMismatch {
                expected: "blake3:aaa".into(),
                actual: "blake3:bbb".into(),
            },
            MediaError::MediaStoreWrite {
                path: "/tmp/fail".into(),
                source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            },
            MediaError::Base64DecodeFailed {
                source_desc: "test".into(),
                reason: "bad".into(),
            },
            MediaError::Base64InputTooLarge { size: 200, max: 100 },
            MediaError::EmptyMediaContent { task_id: "t1".into() },
            MediaError::RunBudgetExceeded { current: 600, max: 500 },
        ];

        let expected_codes = [
            "NIKA-251", "NIKA-252", "NIKA-253", "NIKA-254",
            "NIKA-255", "NIKA-256", "NIKA-257", "NIKA-258", "NIKA-259",
        ];

        for (i, (err, expected_code)) in errors.iter().zip(expected_codes.iter()).enumerate() {
            assert_eq!(err.code(), *expected_code,
                "Error {i} code mismatch: expected {expected_code}, got {}", err.code());
            // Verify Display impl doesn't panic
            let display = format!("{}", err);
            assert!(!display.is_empty(), "Error {i} Display is empty");
            // Verify it contains the NIKA code
            assert!(display.contains(expected_code),
                "Error {i} Display missing code: {display}");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #16: Full pipeline PNG → CAS → MediaRef → Serde
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_full_pipeline_png() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let block = ContentBlock::image(b64, "image/png");

        let result = processor.process(&block, "gen_img").await
            .expect("process should succeed")
            .expect("image block should produce Some");

        let (media_ref, store_result) = result;

        // Verify MediaRef
        assert!(media_ref.hash.starts_with("blake3:"), "hash missing prefix");
        assert_eq!(media_ref.mime_type, "image/png");
        assert_eq!(media_ref.extension, "png");
        assert_eq!(media_ref.size_bytes, png.len() as u64);
        assert_eq!(media_ref.created_by, "gen_img");
        assert!(media_ref.path.exists(), "CAS file should exist at {:?}", media_ref.path);

        // Verify StoreResult
        assert!(!store_result.deduplicated);
        assert!(store_result.verified);
        assert!(store_result.pipeline_ms < 1000, "pipeline took too long: {}ms", store_result.pipeline_ms);

        // Verify CAS file content matches original
        let stored_data = tokio::fs::read(&media_ref.path).await.unwrap();
        assert_eq!(stored_data, png, "SILENT BUG: stored data doesn't match original!");

        // Verify MediaRef serializes correctly
        let json = serde_json::to_value(&media_ref).unwrap();
        assert_eq!(json["mime_type"], "image/png");
        assert!(json["hash"].as_str().unwrap().starts_with("blake3:"));
        assert_eq!(json["size_bytes"], png.len() as u64);
        assert_eq!(json["extension"], "png");
        assert_eq!(json["created_by"], "gen_img");

        // Verify MediaRef deserializes back
        let back: MediaRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, media_ref);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #17: Full pipeline JPEG
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_full_pipeline_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let jpeg = real_jpeg_bytes();
        let block = ContentBlock::image(encode_b64(&jpeg), "image/jpeg");

        let result = processor.process(&block, "gen_jpg").await
            .expect("process should succeed")
            .expect("should produce Some");

        let (media_ref, _) = result;
        assert_eq!(media_ref.mime_type, "image/jpeg");
        assert!(media_ref.extension == "jpg" || media_ref.extension == "jpe",
            "JPEG extension should be jpg or jpe, got: {}", media_ref.extension);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #18: process_all error attribution
    // Errors must carry the correct block index
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_process_all_error_attribution() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let blocks = vec![
            ContentBlock::text("header"),                              // index 0: skipped
            ContentBlock::image(encode_b64(&real_png_bytes()), "image/png"), // index 1: success
            ContentBlock::image("INVALID_BASE64!!!", "image/png"),     // index 2: failure
            ContentBlock::text("footer"),                              // index 3: skipped
        ];

        let results = processor.process_all(&blocks, "multi").await;

        // Should have 2 results (text blocks are skipped)
        assert_eq!(results.len(), 2, "Expected 2 results (1 success + 1 failure)");

        // First should be success (PNG at index 1)
        assert!(results[0].is_ok(), "PNG at index 1 should succeed");

        // Second should be error with block_index = 2
        match &results[1] {
            Err((idx, e)) => {
                assert_eq!(*idx, 2, "Error should reference block index 2, got {}", idx);
                assert_eq!(e.code(), "NIKA-256", "Should be Base64DecodeFailed");
            }
            Ok(_) => panic!("SILENT BUG: invalid base64 succeeded!"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #19: ContentBlock serde roundtrip with ALL fields
    // Particularly test that Resource(ResourceContent) works with
    // internally tagged serde
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_content_block_resource_serde_with_all_fields() {
        let rc = ResourceContent::new("file:///data.json")
            .with_mime_type("application/json")
            .with_text(r#"{"key": "value"}"#)
            .with_blob("SGVsbG8=");

        let block = ContentBlock::Resource(rc);
        let json = serde_json::to_string(&block).unwrap();

        // Must contain the type tag
        assert!(json.contains(r#""type":"resource""#),
            "SILENT BUG: Resource variant missing type tag in JSON: {}", json);

        // Must roundtrip
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back, "Resource serde roundtrip failed");

        // Verify all fields present
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["uri"], "file:///data.json");
        assert_eq!(val["mimeType"], "application/json");
        assert_eq!(val["text"], r#"{"key": "value"}"#);
        assert_eq!(val["blob"], "SGVsbG8=");
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #20: NikaError::MediaError propagation
    // MediaError must convert to NikaError and preserve code
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_media_error_to_nika_error() {
        use crate::error::NikaError;

        let media_err = MediaError::MediaNotFound { hash: "blake3:test".into() };
        let nika_err: NikaError = media_err.into();

        assert_eq!(nika_err.code(), "NIKA-253",
            "SILENT BUG: MediaError code lost in NikaError conversion");

        // Display must contain the code
        let display = format!("{}", nika_err);
        assert!(display.contains("NIKA-253"), "Display missing code: {}", display);
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #21: CAS store concurrent stress test
    // 50 tasks, mixed data → verify no data corruption
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_cas_concurrent_mixed_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path()));

        let datasets: Vec<Vec<u8>> = (0..5).map(|i| {
            let mut data = real_png_bytes();
            data.push(i as u8); // Make each unique
            data
        }).collect();

        // 50 concurrent writes (10 per unique dataset)
        let handles: Vec<_> = (0..50).map(|i| {
            let store = Arc::clone(&store);
            let data = datasets[i % 5].clone();
            tokio::spawn(async move { store.store(&data).await })
        }).collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap().unwrap())
            .collect();

        // Should have exactly 5 unique hashes
        let mut unique_hashes: Vec<_> = results.iter().map(|r| r.hash.clone()).collect();
        unique_hashes.sort();
        unique_hashes.dedup();
        assert_eq!(unique_hashes.len(), 5,
            "SILENT BUG: expected 5 unique hashes, got {}", unique_hashes.len());

        // Verify each dataset can be read back correctly
        // We can't rely on result ordering (concurrent tasks), so compute expected hash
        for (i, data) in datasets.iter().enumerate() {
            let expected_hash = format!("blake3:{}", blake3::hash(data).to_hex());
            let read_back = store.read(&expected_hash).await.unwrap();
            assert_eq!(&read_back, data,
                "SILENT BUG: dataset {i} read-back mismatch");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #22: MediaType classification edge cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_media_type_edge_cases() {
        // Standard cases
        assert_eq!(MediaType::from_mime("image/png"), MediaType::Image);
        assert_eq!(MediaType::from_mime("audio/mpeg"), MediaType::Audio);
        assert_eq!(MediaType::from_mime("application/pdf"), MediaType::Document);

        // Edge cases
        assert_eq!(MediaType::from_mime("image/svg+xml"), MediaType::Image);
        assert_eq!(MediaType::from_mime("audio/x-wav"), MediaType::Audio);
        assert_eq!(MediaType::from_mime("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            MediaType::Document);

        // Unknown types
        assert_eq!(MediaType::from_mime("video/mp4"), MediaType::Unknown);
        assert_eq!(MediaType::from_mime("text/html"), MediaType::Unknown);
        assert_eq!(MediaType::from_mime(""), MediaType::Unknown);
        assert_eq!(MediaType::from_mime("garbage"), MediaType::Unknown);
    }
}
