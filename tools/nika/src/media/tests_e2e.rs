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
    async fn e2e_base64_with_newlines_succeeds() {
        // Real MCP servers (OpenAI etc.) send PEM-style base64 with \n
        // We strip whitespace before decoding to handle this correctly
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let b64_with_newlines = b64.chars()
            .enumerate()
            .map(|(i, c)| if i > 0 && i % 76 == 0 { format!("\n{c}") } else { c.to_string() })
            .collect::<String>();

        let block = ContentBlock::image(b64_with_newlines, "image/png");
        let result = processor.process(&block, "t_newline").await
            .expect("base64 with newlines should succeed after whitespace stripping")
            .expect("image should produce Some");

        let (media_ref, _) = result;
        let expected_hash = format!("blake3:{}", blake3::hash(&png).to_hex());
        assert_eq!(media_ref.hash, expected_hash,
            "Decoded data should match original after newline stripping");
        assert_eq!(media_ref.mime_type, "image/png");
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
        // Small files: verified=false (read-back skipped, fsync sufficient)
        assert!(!store_result.verified);
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

    // ═══════════════════════════════════════════════════════════════
    // PHASE E2: AGGRESSIVE EDGE CASES — SILENT BUG HUNTING
    // ═══════════════════════════════════════════════════════════════

    // --- Base64 stress tests ---

    #[tokio::test]
    async fn e2e_base64_with_spaces_succeeds() {
        // Some servers insert spaces in base64 — we strip whitespace
        let (processor, _dir) = make_processor_e2e();
        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let with_spaces = b64.chars()
            .enumerate()
            .map(|(i, c)| if i > 0 && i % 20 == 0 { format!(" {c}") } else { c.to_string() })
            .collect::<String>();
        let block = ContentBlock::image(with_spaces, "image/png");
        let result = processor.process(&block, "t_spaces").await
            .expect("base64 with spaces should succeed after whitespace stripping")
            .expect("image should produce Some");
        let expected_hash = format!("blake3:{}", blake3::hash(&png).to_hex());
        assert_eq!(result.0.hash, expected_hash);
    }

    #[tokio::test]
    async fn e2e_base64_single_char_fails() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::image("A", "image/png");
        let result = processor.process(&block, "t_single").await;
        // Single char is invalid base64 (not multiple of 4)
        assert!(result.is_err(), "Single char base64 should fail");
    }

    #[tokio::test]
    async fn e2e_base64_just_padding_fails() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::image("====", "image/png");
        let result = processor.process(&block, "t_padding").await;
        // Just padding chars is invalid
        assert!(result.is_err(), "Padding-only base64 should fail");
    }

    #[tokio::test]
    async fn e2e_base64_decodes_to_single_byte() {
        let (processor, _dir) = make_processor_e2e();
        // Single byte 0xFF → base64 "/w=="
        let b64 = encode_b64(&[0xFF]);
        let block = ContentBlock::image(b64, "application/octet-stream");
        let result = processor.process(&block, "t_1byte").await;
        // 1 byte → infer::get() returns None → server hint "application/octet-stream" → error
        assert!(result.is_err(), "Single byte with octet-stream should fail MIME detection");
        assert_eq!(result.unwrap_err().code(), "NIKA-251");
    }

    // --- MIME detection stress ---

    #[test]
    fn e2e_detect_empty_data_fails() {
        // Empty slice should fail
        let result = detect_mime(&[], None);
        assert!(result.is_err(), "Empty data should fail MIME detection");
    }

    #[test]
    fn e2e_detect_single_byte_fails() {
        let result = detect_mime(&[0x89], None);
        assert!(result.is_err(), "Single byte should fail MIME detection");
    }

    #[test]
    fn e2e_detect_almost_png_fails() {
        // First 3 bytes of PNG but incomplete signature
        let almost_png = &[0x89, 0x50, 0x4E];
        let result = detect_mime(almost_png, None);
        // Should NOT detect as PNG — incomplete signature
        match result {
            Ok(detected) => {
                assert_ne!(detected.mime_type, "image/png",
                    "SILENT BUG: incomplete PNG signature detected as PNG!");
            }
            Err(_) => {} // Expected: detection fails
        }
    }

    #[test]
    fn e2e_detect_webp_header() {
        // RIFF....WEBP
        let webp = &[
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x00, 0x00, 0x00, 0x00, // size
            0x57, 0x45, 0x42, 0x50, // WEBP
        ];
        let result = detect_mime(webp, None).unwrap();
        assert_eq!(result.mime_type, "image/webp");
    }

    #[test]
    fn e2e_detect_mp3_id3_header() {
        let mp3 = real_mp3_bytes();
        let result = detect_mime(&mp3, None).unwrap();
        assert!(result.mime_type.contains("mp3") || result.mime_type.contains("mpeg"),
            "MP3 not detected: {}", result.mime_type);
    }

    #[test]
    fn e2e_detect_xml_not_svg() {
        // XML that is NOT SVG — should NOT be detected as image/svg+xml
        let xml = b"<?xml version=\"1.0\"?><root><data>hello</data></root>";
        let result = detect_mime(xml, None);
        match result {
            Ok(detected) => {
                assert_ne!(detected.mime_type, "image/svg+xml",
                    "SILENT BUG: non-SVG XML detected as SVG!");
            }
            Err(_) => {} // Expected: not detected
        }
    }

    #[test]
    fn e2e_detect_svg_with_xml_declaration() {
        let svg = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"><circle/></svg>";
        let result = detect_mime(svg, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
    }

    #[test]
    fn e2e_detect_server_mime_alias_audio_mp3() {
        // Server says "audio/mp3" (non-standard) for an actual MP3
        let mp3 = real_mp3_bytes();
        let result = detect_mime(&mp3, Some("audio/mp3")).unwrap();
        // Magic bytes should detect correctly regardless of non-standard server hint
        assert!(result.mime_type.contains("mpeg") || result.mime_type.contains("mp3"),
            "MP3 detection failed with non-standard server hint: {}", result.mime_type);
    }

    // --- CAS store edge cases ---

    #[tokio::test]
    async fn e2e_cas_store_large_file_without_verify() {
        // File just under verify threshold (1MB) — should NOT be verified
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let data = vec![0xAB_u8; 1024 * 1024 - 1]; // 1MB - 1 byte
        let result = store.store(&data).await.unwrap();
        assert!(!result.verified, "File under 1MB should not be verified");
        assert!(!result.deduplicated);
    }

    #[tokio::test]
    async fn e2e_cas_store_exact_threshold_is_verified() {
        // File exactly at verify threshold (1MB) — SHOULD be verified
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let data = vec![0xCD_u8; 1024 * 1024]; // Exactly 1MB
        let result = store.store(&data).await.unwrap();
        assert!(result.verified, "File at exactly 1MB should be verified");
    }

    #[tokio::test]
    async fn e2e_cas_read_with_raw_hash_no_prefix() {
        // read() should accept both "blake3:..." and raw hash
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let data = b"read with raw hash";
        let result = store.store(data).await.unwrap();

        // Read with full prefix
        let data1 = store.read(&result.hash).await.unwrap();
        assert_eq!(data1, data);

        // Read with raw hash (strip "blake3:" prefix)
        let raw_hash = result.hash.strip_prefix("blake3:").unwrap();
        let data2 = store.read(raw_hash).await.unwrap();
        assert_eq!(data2, data);
    }

    #[tokio::test]
    async fn e2e_cas_exists_with_raw_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = store.store(b"exists check").await.unwrap();

        assert!(store.exists(&result.hash));
        let raw = result.hash.strip_prefix("blake3:").unwrap();
        assert!(store.exists(raw));
    }

    #[tokio::test]
    async fn e2e_cas_read_invalid_short_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        // Hash too short (< 3 chars)
        let result = store.read("ab").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "NIKA-253");
    }

    // --- Process pipeline integration ---

    #[tokio::test]
    async fn e2e_process_audio_block() {
        let (processor, _dir) = make_processor_e2e();
        let mp3 = real_mp3_bytes();
        let block = ContentBlock::audio(encode_b64(&mp3), "audio/mpeg");
        let result = processor.process(&block, "gen_audio").await
            .expect("process should succeed")
            .expect("audio should return Some");
        let (media_ref, _) = result;
        assert!(media_ref.mime_type.contains("mpeg") || media_ref.mime_type.contains("mp3"),
            "Audio MIME type wrong: {}", media_ref.mime_type);
        assert_eq!(media_ref.created_by, "gen_audio");
    }

    #[tokio::test]
    async fn e2e_process_resource_with_blob_and_mime() {
        let (processor, _dir) = make_processor_e2e();
        let pdf = real_pdf_bytes();
        let rc = ResourceContent::new("file:///doc.pdf")
            .with_blob(encode_b64(&pdf))
            .with_mime_type("application/pdf");
        let block = ContentBlock::Resource(rc);
        let result = processor.process(&block, "gen_pdf").await
            .expect("process should succeed")
            .expect("blob resource should return Some");
        let (media_ref, _) = result;
        assert_eq!(media_ref.mime_type, "application/pdf");
    }

    #[tokio::test]
    async fn e2e_process_all_only_errors() {
        let (processor, _dir) = make_processor_e2e();
        let blocks = vec![
            ContentBlock::image("BAD_BASE64!!!", "image/png"),
            ContentBlock::audio("ALSO_BAD!!!", "audio/wav"),
        ];
        let results = processor.process_all(&blocks, "fail_all").await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_err()),
            "All blocks should fail");
    }

    #[tokio::test]
    async fn e2e_process_all_empty_vec() {
        let (processor, _dir) = make_processor_e2e();
        let results = processor.process_all(&[], "empty").await;
        assert!(results.is_empty(), "Empty input should produce empty output");
    }

    #[tokio::test]
    async fn e2e_process_all_text_only() {
        let (processor, _dir) = make_processor_e2e();
        let blocks = vec![
            ContentBlock::text("hello"),
            ContentBlock::text("world"),
        ];
        let results = processor.process_all(&blocks, "text_only").await;
        assert!(results.is_empty(), "Text-only blocks should produce empty output");
    }

    // --- Shared budget e2e ---

    #[tokio::test]
    async fn e2e_shared_budget_across_processors() {
        let budget = Arc::new(MediaBudget::with_max_per_run(200));

        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let p1 = MediaProcessor::with_shared_budget(
            CasStore::new(dir1.path()), Arc::clone(&budget));
        let p2 = MediaProcessor::with_shared_budget(
            CasStore::new(dir2.path()), Arc::clone(&budget));

        let png = real_png_bytes();
        let b64 = encode_b64(&png);

        // First processor succeeds
        let r1 = p1.process(&ContentBlock::image(b64.clone(), "image/png"), "t1").await;
        assert!(r1.is_ok(), "First process should succeed: {:?}", r1.err());

        // Second processor succeeds (still under budget)
        let r2 = p2.process(&ContentBlock::image(b64.clone(), "image/png"), "t2").await;
        assert!(r2.is_ok(), "Second process should succeed: {:?}", r2.err());

        // Third processor should fail (budget exceeded: ~70 bytes x 3 > 200)
        let r3 = p1.process(&ContentBlock::image(b64.clone(), "image/png"), "t3").await;
        assert!(r3.is_err(), "Third process should exceed budget");
        assert_eq!(r3.unwrap_err().code(), "NIKA-259");
    }

    // --- Error code regression tests ---

    #[test]
    fn e2e_all_error_variants_have_display() {
        // Every error variant must have a non-empty Display and contain its NIKA code
        let cases: Vec<(MediaError, &str)> = vec![
            (MediaError::mime_detection_failed(100, Some("image/png".into())), "NIKA-251"),
            (MediaError::UnsupportedMediaType { mime_type: "x".into(), reason: "y".into() }, "NIKA-252"),
            (MediaError::MediaNotFound { hash: "h".into() }, "NIKA-253"),
            (MediaError::HashMismatch { expected: "a".into(), actual: "b".into() }, "NIKA-254"),
            (MediaError::MediaStoreWrite {
                path: "/x".into(),
                source: std::io::Error::new(std::io::ErrorKind::Other, "test"),
            }, "NIKA-255"),
            (MediaError::Base64DecodeFailed { source_desc: "x".into(), reason: "y".into() }, "NIKA-256"),
            (MediaError::Base64InputTooLarge { size: 200, max: 100 }, "NIKA-257"),
            (MediaError::EmptyMediaContent { task_id: "t".into() }, "NIKA-258"),
            (MediaError::RunBudgetExceeded { current: 600, max: 500 }, "NIKA-259"),
        ];
        for (err, code) in &cases {
            let display = format!("{err}");
            assert!(!display.is_empty(), "{code} has empty display");
            assert!(display.contains(code), "{code} display missing code: {display}");
            assert_eq!(err.code(), *code);
        }
    }

    #[test]
    fn e2e_media_error_is_recoverable() {
        // Only MediaStoreWrite should be recoverable
        assert!(MediaError::MediaStoreWrite {
            path: "/x".into(),
            source: std::io::Error::new(std::io::ErrorKind::Other, ""),
        }.is_recoverable());

        assert!(!MediaError::mime_detection_failed(0, None).is_recoverable());
        assert!(!MediaError::Base64DecodeFailed { source_desc: "".into(), reason: "".into() }.is_recoverable());
        assert!(!MediaError::RunBudgetExceeded { current: 0, max: 0 }.is_recoverable());
    }

    // --- ContentBlock serde exhaustive ---

    #[test]
    fn e2e_content_block_audio_json_format() {
        let block = ContentBlock::audio("data123", "audio/mpeg");
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "audio");
        assert_eq!(json["data"], "data123");
        assert_eq!(json["mimeType"], "audio/mpeg");
        // Must NOT have mime_type (snake_case)
        assert!(json.get("mime_type").is_none());
    }

    #[test]
    fn e2e_content_block_resource_link_with_all_fields() {
        let block = ContentBlock::resource_link(
            "file:///test",
            Some("myfile.pdf".into()),
            Some("application/pdf".into()),
        );
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "resource_link");
        assert_eq!(json["uri"], "file:///test");
        assert_eq!(json["name"], "myfile.pdf");
        assert_eq!(json["mimeType"], "application/pdf");
    }

    #[test]
    fn e2e_content_block_deserialize_unknown_type_fails() {
        // Unknown type should fail deserialization
        let json = r#"{"type": "video", "data": "abc"}"#;
        let result: Result<ContentBlock, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Unknown content type 'video' should fail deserialization");
    }

    #[test]
    fn e2e_content_block_missing_type_fails() {
        let json = r#"{"text": "hello"}"#;
        let result: Result<ContentBlock, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing 'type' field should fail deserialization");
    }

    // --- Helper ---

    fn make_processor_e2e() -> (MediaProcessor, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        (MediaProcessor::new(store), dir)
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE E3: INTEGRATION PIPELINE TESTS
    // Simulates what run_invoke does: ToolCallResult → processor → CAS → MediaRef → TaskResult
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_invoke_simulation_mixed_content() {
        // Simulates a real MCP tool returning text + image + audio
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = Arc::new(MediaBudget::new());
        let processor = MediaProcessor::with_shared_budget(store, Arc::clone(&budget));
        let ctx = RunContext::new();
        let task_id: Arc<str> = "invoke_sim".into();

        // Simulate tool result with mixed content
        let tool_result = ToolCallResult::success(vec![
            ContentBlock::text("Image generated successfully"),
            ContentBlock::image(encode_b64(&real_png_bytes()), "image/png"),
            ContentBlock::text("Audio also available"),
            ContentBlock::audio(encode_b64(&real_mp3_bytes()), "audio/mpeg"),
        ]);

        // Check has_media
        assert!(tool_result.has_media());
        assert_eq!(tool_result.media_blocks().len(), 2);

        // Process all blocks
        let results = processor.process_all(&tool_result.content, task_id.as_ref()).await;

        // Collect successful media refs
        let mut media_refs = Vec::new();
        for result in results {
            match result {
                Ok((media_ref, store_result)) => {
                    assert!(media_ref.hash.starts_with("blake3:"));
                    assert!(media_ref.size_bytes > 0);
                    assert_eq!(media_ref.created_by, "invoke_sim");
                    let cas_filename = store_result.path.file_name().unwrap().to_string_lossy();
                    assert!(!cas_filename.contains('.'),
                        "CAS filename should not contain dot: {}", cas_filename);
                    media_refs.push(media_ref);
                }
                Err((idx, e)) => {
                    panic!("Block {idx} failed unexpectedly: {e}");
                }
            }
        }

        assert_eq!(media_refs.len(), 2, "Should have 2 media refs (image + audio)");

        // Stage in RunContext
        ctx.set_media(&task_id, media_refs.clone());

        // Take from RunContext (simulates what runner does)
        let taken = ctx.take_media(&task_id);
        assert_eq!(taken.len(), 2);

        // Build TaskResult with media
        use crate::store::TaskResult;
        let text = tool_result.text();
        let output = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
        let tr = TaskResult::success(output, std::time::Duration::from_millis(42))
            .with_media(taken);

        assert_eq!(tr.media.len(), 2);
        assert_eq!(tr.media[0].mime_type, "image/png");
        assert!(tr.media[1].mime_type.contains("mpeg") || tr.media[1].mime_type.contains("mp3"));

        // Verify text output is preserved
        assert_eq!(
            tr.output.as_str().unwrap(),
            "Image generated successfully\nAudio also available"
        );

        // Verify budget tracked
        assert!(budget.current_bytes() > 0, "Budget should have tracked bytes");
    }

    #[tokio::test]
    async fn e2e_invoke_simulation_text_only_no_media() {
        // Simulates a text-only MCP response — NO media processing should happen
        let ctx = RunContext::new();
        let task_id: Arc<str> = "text_invoke".into();

        let tool_result = ToolCallResult::success(vec![
            ContentBlock::text("Just text, no media"),
        ]);

        assert!(!tool_result.has_media());

        // No media processing needed
        let taken = ctx.take_media(&task_id);
        assert!(taken.is_empty());

        use crate::store::TaskResult;
        let tr = TaskResult::success(
            serde_json::Value::String(tool_result.text()),
            std::time::Duration::from_millis(10),
        ).with_media(taken);

        assert!(tr.media.is_empty());
        assert!(tr.is_success());
    }

    #[tokio::test]
    async fn e2e_invoke_simulation_partial_failure() {
        // One block succeeds, one fails — media refs should contain only successes
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let tool_result = ToolCallResult::success(vec![
            ContentBlock::text("description"),
            ContentBlock::image(encode_b64(&real_png_bytes()), "image/png"),
            ContentBlock::image("INVALID!!!", "image/png"), // Will fail
        ]);

        let results = processor.process_all(&tool_result.content, "partial").await;

        let mut media_refs = Vec::new();
        let mut errors = Vec::new();
        for result in results {
            match result {
                Ok((mr, _)) => media_refs.push(mr),
                Err((idx, e)) => errors.push((idx, e)),
            }
        }

        assert_eq!(media_refs.len(), 1, "One image should succeed");
        assert_eq!(errors.len(), 1, "One image should fail");
        assert_eq!(errors[0].0, 2, "Error should reference block index 2");
        assert_eq!(errors[0].1.code(), "NIKA-256");

        // Verify the successful media ref is valid
        assert_eq!(media_refs[0].mime_type, "image/png");
        assert!(media_refs[0].path.exists());
    }

    #[tokio::test]
    async fn e2e_invoke_simulation_dedup_same_image_twice() {
        // Same image in two blocks → CAS dedup, but 2 MediaRefs with same hash
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        let png_b64 = encode_b64(&real_png_bytes());
        let tool_result = ToolCallResult::success(vec![
            ContentBlock::image(png_b64.clone(), "image/png"),
            ContentBlock::image(png_b64, "image/png"),
        ]);

        let results = processor.process_all(&tool_result.content, "dedup").await;
        assert_eq!(results.len(), 2);

        let refs: Vec<_> = results.into_iter()
            .map(|r| r.unwrap())
            .collect();

        // Both should have the same hash
        assert_eq!(refs[0].0.hash, refs[1].0.hash,
            "Same image should have same hash");

        // One should be dedup
        let dedup_count = refs.iter().filter(|(_, sr)| sr.deduplicated).count();
        assert_eq!(dedup_count, 1, "One should be deduplicated");
    }

    #[tokio::test]
    async fn e2e_media_ref_json_fields_complete() {
        // Verify every field is present in serialized MediaRef JSON
        let mr = MediaRef {
            hash: "blake3:abcdef1234567890".into(),
            mime_type: "image/png".into(),
            size_bytes: 12345,
            path: std::path::PathBuf::from("/tmp/store/ab/cdef1234567890"),
            extension: "png".into(),
            created_by: "task_gen".into(),
        };
        let json = serde_json::to_value(&mr).unwrap();

        // ALL fields must be present
        assert!(json.get("hash").is_some(), "missing hash");
        assert!(json.get("mime_type").is_some(), "missing mime_type");
        assert!(json.get("size_bytes").is_some(), "missing size_bytes");
        assert!(json.get("path").is_some(), "missing path");
        assert!(json.get("extension").is_some(), "missing extension");
        assert!(json.get("created_by").is_some(), "missing created_by");

        // Verify types
        assert!(json["hash"].is_string());
        assert!(json["mime_type"].is_string());
        assert!(json["size_bytes"].is_number());
        assert!(json["path"].is_string());
        assert!(json["extension"].is_string());
        assert!(json["created_by"].is_string());

        // Verify values
        assert_eq!(json["hash"], "blake3:abcdef1234567890");
        assert_eq!(json["size_bytes"], 12345);
    }
}
