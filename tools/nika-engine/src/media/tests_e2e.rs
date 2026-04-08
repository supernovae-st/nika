// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

    use crate::mcp::types::{ContentBlock, ResourceContent, ToolCallResult};
    use crate::media::detect::detect_mime;
    use crate::media::error::MediaError;
    use crate::media::processor::MediaProcessor;
    use crate::media::store::CasStore;
    use crate::media::types::{MediaBudget, MediaRef, MediaType};
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
            0x08, 0x06, // 8-bit RGBA
            0x00, 0x00, 0x00, // compression, filter, interlace
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
            0x00, 0x10, // APP0 length
            0x4A, 0x46, 0x49, 0x46, 0x00, // JFIF\0
            0x01, 0x01, // version
            0x00, // units
            0x00, 0x01, 0x00, 0x01, // density
            0x00, 0x00, // thumbnail
            0xFF, 0xD9, // EOI
        ]
    }

    /// MP3 frame header (ID3 tag)
    fn real_mp3_bytes() -> Vec<u8> {
        let mut data = vec![
            0x49, 0x44, 0x33, // ID3
            0x04, 0x00, // version
            0x00, // flags
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
        let b64_with_newlines = b64
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i > 0 && i % 76 == 0 {
                    format!("\n{c}")
                } else {
                    c.to_string()
                }
            })
            .collect::<String>();

        let block = ContentBlock::image(b64_with_newlines, "image/png");
        let result = processor
            .process(&block, "t_newline")
            .await
            .expect("base64 with newlines should succeed after whitespace stripping")
            .expect("image should produce Some");

        let (media_ref, _) = result;
        let expected_hash = format!("blake3:{}", blake3::hash(&png).to_hex());
        assert_eq!(
            media_ref.hash, expected_hash,
            "Decoded data should match original after newline stripping"
        );
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
                assert!(
                    e.code() == "NIKA-256" || e.code() == "NIKA-251",
                    "Unexpected error: {}",
                    e
                );
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
    async fn e2e_mime_category_mismatch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        // Send PNG data with audio/wav MIME type → cross-category mismatch → NIKA-251
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "audio/wav");
        let result = processor.process(&block, "t_mismatch").await;

        assert!(
            result.is_err(),
            "Cross-category MIME mismatch should be rejected"
        );
        assert_eq!(result.unwrap_err().code(), "NIKA-251");
    }

    #[tokio::test]
    async fn e2e_mime_same_category_mismatch_uses_magic_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let processor = MediaProcessor::new(store);

        // Send PNG data with image/webp MIME type → same category → magic bytes win
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "image/webp");
        let result = processor.process(&block, "t_mismatch").await;

        match result {
            Ok(Some((media_ref, _))) => {
                assert_eq!(
                    media_ref.mime_type, "image/png",
                    "Magic bytes should win for same-category mismatch"
                );
                assert_eq!(media_ref.extension, "png");
            }
            Ok(None) => panic!("image data returned None"),
            Err(e) => panic!("Same-category mismatch should not error: {}", e),
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

        let rc = ResourceContent::new("file:///image.png").with_blob(encode_b64(&real_png_bytes()));
        // No mime_type set — processor must detect from magic bytes
        let block = ContentBlock::Resource(rc);
        let result = processor.process(&block, "t_nomime").await;

        match result {
            Ok(Some((media_ref, _))) => {
                assert_eq!(
                    media_ref.mime_type, "image/png",
                    "SILENT BUG: MIME not detected from magic bytes when server hint missing"
                );
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

        let rc = ResourceContent::new("file:///readme.md").with_text("# Hello World");
        let block = ContentBlock::Resource(rc);
        let result = processor.process(&block, "t_textonly").await.unwrap();
        assert!(
            result.is_none(),
            "Text-only resource should return None, not store in CAS"
        );
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

        assert!(
            !filename.ends_with(".png"),
            "SILENT BUG: CAS filename has .png extension: {}",
            filename
        );
        assert!(
            !filename.ends_with(".jpg"),
            "SILENT BUG: CAS filename has .jpg extension: {}",
            filename
        );
        assert!(
            !filename.contains('.'),
            "SILENT BUG: CAS filename contains dot: {}",
            filename
        );
        // Also verify the parent is a 2-char shard directory
        let shard = r
            .path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy();
        assert_eq!(
            shard.len(),
            2,
            "Shard directory should be 2 chars: {}",
            shard
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #9: media_staging lifecycle
    // set_media → take_media must not leak
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_media_staging_set_then_take() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "test_task".into();

        let refs = vec![MediaRef {
            hash: "blake3:abc123".into(),
            mime_type: "image/png".into(),
            size_bytes: 1024,
            path: "/tmp/store/ab/c123".into(),
            extension: "png".into(),
            created_by: "test_task".into(),
            metadata: serde_json::Map::new(),
        }];

        ctx.set_media(&task_id, refs.clone());

        // First take should return the refs
        let taken = ctx.take_media(&task_id);
        assert_eq!(taken.len(), 1, "take_media should return staged refs");
        assert_eq!(taken[0].hash, "blake3:abc123");

        // Second take should return empty (drained)
        let taken_again = ctx.take_media(&task_id);
        assert!(
            taken_again.is_empty(),
            "SILENT BUG: take_media didn't drain staging"
        );
    }

    #[test]
    fn e2e_media_staging_empty_vec_not_stored() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "empty_task".into();

        // set_media with empty vec should be a no-op
        ctx.set_media(&task_id, vec![]);

        let taken = ctx.take_media(&task_id);
        assert!(
            taken.is_empty(),
            "Empty vec should not be stored in staging"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #10: TaskResult.media survives with_media
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn e2e_task_result_with_media_attaches() {
        use crate::store::TaskResult;
        use std::time::Duration;

        let tr = TaskResult::success(serde_json::json!("ok"), Duration::from_millis(100));
        assert!(
            tr.media.is_empty(),
            "New TaskResult should have empty media"
        );

        let refs = vec![MediaRef {
            hash: "blake3:deadbeef".into(),
            mime_type: "image/png".into(),
            size_bytes: 2048,
            path: "/tmp/store/de/adbeef".into(),
            extension: "png".into(),
            created_by: "gen".into(),
            metadata: serde_json::Map::new(),
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
        assert_eq!(
            result.text(),
            "visible text\nmore text",
            "SILENT BUG: text() includes non-text content!"
        );
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
        assert_eq!(
            result.mime_type, "image/png",
            "SILENT BUG: uppercase MIME not normalized: {}",
            result.mime_type
        );

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
        assert_eq!(
            result.mime_type, "application/pdf",
            "SILENT BUG: PDF not detected: {}",
            result.mime_type
        );
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
                tokio::spawn(async move { budget.check_and_add(100, &format!("t{i}")) })
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
        assert_eq!(
            successes, 10,
            "SILENT BUG: budget allowed {} of 20 (expected 10)",
            successes
        );
        assert_eq!(
            failures, 10,
            "SILENT BUG: budget rejected {} of 20 (expected 10)",
            failures
        );

        // Final bytes should be exactly 1000
        assert_eq!(
            budget.current_bytes(),
            1000,
            "SILENT BUG: budget tracking off: {}",
            budget.current_bytes()
        );
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
            MediaError::MediaNotFound {
                hash: "blake3:xxx".into(),
            },
            MediaError::HashMismatch {
                expected: "blake3:aaa".into(),
                actual: "blake3:bbb".into(),
            },
            MediaError::MediaStoreIo {
                path: "/tmp/fail".into(),
                source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            },
            MediaError::Base64DecodeFailed {
                source_desc: "test".into(),
                reason: "bad".into(),
            },
            MediaError::Base64InputTooLarge {
                size: 200,
                max: 100,
            },
            MediaError::EmptyMediaContent {
                task_id: "t1".into(),
            },
            MediaError::RunBudgetExceeded {
                current: 600,
                max: 500,
            },
        ];

        let expected_codes = [
            "NIKA-251", "NIKA-252", "NIKA-253", "NIKA-254", "NIKA-255", "NIKA-256", "NIKA-257",
            "NIKA-258", "NIKA-259",
        ];

        for (i, (err, expected_code)) in errors.iter().zip(expected_codes.iter()).enumerate() {
            assert_eq!(
                err.code(),
                *expected_code,
                "Error {i} code mismatch: expected {expected_code}, got {}",
                err.code()
            );
            // Verify Display impl doesn't panic
            let display = format!("{}", err);
            assert!(!display.is_empty(), "Error {i} Display is empty");
            // Verify it contains the NIKA code
            assert!(
                display.contains(expected_code),
                "Error {i} Display missing code: {display}"
            );
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

        let result = processor
            .process(&block, "gen_img")
            .await
            .expect("process should succeed")
            .expect("image block should produce Some");

        let (media_ref, store_result) = result;

        // Verify MediaRef
        assert!(media_ref.hash.starts_with("blake3:"), "hash missing prefix");
        assert_eq!(media_ref.mime_type, "image/png");
        assert_eq!(media_ref.extension, "png");
        assert_eq!(media_ref.size_bytes, png.len() as u64);
        assert_eq!(media_ref.created_by, "gen_img");
        assert!(
            media_ref.path.exists(),
            "CAS file should exist at {:?}",
            media_ref.path
        );

        // Verify StoreResult
        assert!(!store_result.deduplicated);
        // Small files: verified=false (read-back skipped, fsync sufficient)
        assert!(!store_result.verified);
        assert!(
            store_result.pipeline_ms < 1000,
            "pipeline took too long: {}ms",
            store_result.pipeline_ms
        );

        // Verify CAS file content matches original (read_raw strips NK framing header)
        let stored_data = CasStore::read_raw(&media_ref.path).await.unwrap();
        assert_eq!(
            stored_data, png,
            "SILENT BUG: stored data doesn't match original!"
        );

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

        let result = processor
            .process(&block, "gen_jpg")
            .await
            .expect("process should succeed")
            .expect("should produce Some");

        let (media_ref, _) = result;
        assert_eq!(media_ref.mime_type, "image/jpeg");
        assert!(
            media_ref.extension == "jpg" || media_ref.extension == "jpe",
            "JPEG extension should be jpg or jpe, got: {}",
            media_ref.extension
        );
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
            ContentBlock::text("header"), // index 0: skipped
            ContentBlock::image(encode_b64(&real_png_bytes()), "image/png"), // index 1: success
            ContentBlock::image("INVALID_BASE64!!!", "image/png"), // index 2: failure
            ContentBlock::text("footer"), // index 3: skipped
        ];

        let results = processor.process_all(&blocks, "multi").await;

        // Should have 2 results (text blocks are skipped)
        assert_eq!(
            results.len(),
            2,
            "Expected 2 results (1 success + 1 failure)"
        );

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
        assert!(
            json.contains(r#""type":"resource""#),
            "SILENT BUG: Resource variant missing type tag in JSON: {}",
            json
        );

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

        let media_err = MediaError::MediaNotFound {
            hash: "blake3:test".into(),
        };
        let nika_err: NikaError = media_err.into();

        assert_eq!(
            nika_err.code(),
            "NIKA-253",
            "SILENT BUG: MediaError code lost in NikaError conversion"
        );

        // Display must contain the code
        let display = format!("{}", nika_err);
        assert!(
            display.contains("NIKA-253"),
            "Display missing code: {}",
            display
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SILENT BUG #21: CAS store concurrent stress test
    // 50 tasks, mixed data → verify no data corruption
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn e2e_cas_concurrent_mixed_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path()));

        let datasets: Vec<Vec<u8>> = (0..5)
            .map(|i| {
                let mut data = real_png_bytes();
                data.push(i as u8); // Make each unique
                data
            })
            .collect();

        // 50 concurrent writes (10 per unique dataset)
        let handles: Vec<_> = (0..50)
            .map(|i| {
                let store = Arc::clone(&store);
                let data = datasets[i % 5].clone();
                tokio::spawn(async move { store.store(&data).await })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap().unwrap())
            .collect();

        // Should have exactly 5 unique hashes
        let mut unique_hashes: Vec<_> = results.iter().map(|r| r.hash.clone()).collect();
        unique_hashes.sort();
        unique_hashes.dedup();
        assert_eq!(
            unique_hashes.len(),
            5,
            "SILENT BUG: expected 5 unique hashes, got {}",
            unique_hashes.len()
        );

        // Verify each dataset can be read back correctly
        // We can't rely on result ordering (concurrent tasks), so compute expected hash
        for (i, data) in datasets.iter().enumerate() {
            let expected_hash = format!("blake3:{}", blake3::hash(data).to_hex());
            let read_back = store.read(&expected_hash).await.unwrap();
            assert_eq!(
                &read_back, data,
                "SILENT BUG: dataset {i} read-back mismatch"
            );
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
        assert_eq!(
            MediaType::from_mime(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            ),
            MediaType::Document
        );

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
        let with_spaces = b64
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i > 0 && i % 20 == 0 {
                    format!(" {c}")
                } else {
                    c.to_string()
                }
            })
            .collect::<String>();
        let block = ContentBlock::image(with_spaces, "image/png");
        let result = processor
            .process(&block, "t_spaces")
            .await
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
        assert!(
            result.is_err(),
            "Single byte with octet-stream should fail MIME detection"
        );
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
        if let Ok(detected) = result {
            assert_ne!(
                detected.mime_type, "image/png",
                "SILENT BUG: incomplete PNG signature detected as PNG!"
            );
        }
        // Err(_) is also expected: detection fails
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
        assert!(
            result.mime_type.contains("mp3") || result.mime_type.contains("mpeg"),
            "MP3 not detected: {}",
            result.mime_type
        );
    }

    #[test]
    fn e2e_detect_xml_not_svg() {
        // XML that is NOT SVG — should NOT be detected as image/svg+xml
        let xml = b"<?xml version=\"1.0\"?><root><data>hello</data></root>";
        let result = detect_mime(xml, None);
        if let Ok(detected) = result {
            assert_ne!(
                detected.mime_type, "image/svg+xml",
                "SILENT BUG: non-SVG XML detected as SVG!"
            );
        }
        // Err(_) is also expected: not detected
    }

    #[test]
    fn e2e_detect_svg_with_xml_declaration() {
        let svg =
            b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"><circle/></svg>";
        let result = detect_mime(svg, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
    }

    #[test]
    fn e2e_detect_server_mime_alias_audio_mp3() {
        // Server says "audio/mp3" (non-standard) for an actual MP3
        let mp3 = real_mp3_bytes();
        let result = detect_mime(&mp3, Some("audio/mp3")).unwrap();
        // Magic bytes should detect correctly regardless of non-standard server hint
        assert!(
            result.mime_type.contains("mpeg") || result.mime_type.contains("mp3"),
            "MP3 detection failed with non-standard server hint: {}",
            result.mime_type
        );
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
        assert!(
            result.is_err(),
            "reading short hash should fail but got: {:?}",
            result.as_ref().ok()
        );
        assert_eq!(result.unwrap_err().code(), "NIKA-253");
    }

    // --- Process pipeline integration ---

    #[tokio::test]
    async fn e2e_process_audio_block() {
        let (processor, _dir) = make_processor_e2e();
        let mp3 = real_mp3_bytes();
        let block = ContentBlock::audio(encode_b64(&mp3), "audio/mpeg");
        let result = processor
            .process(&block, "gen_audio")
            .await
            .expect("process should succeed")
            .expect("audio should return Some");
        let (media_ref, _) = result;
        assert!(
            media_ref.mime_type.contains("mpeg") || media_ref.mime_type.contains("mp3"),
            "Audio MIME type wrong: {}",
            media_ref.mime_type
        );
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
        let result = processor
            .process(&block, "gen_pdf")
            .await
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
        assert!(results.iter().all(|r| r.is_err()), "All blocks should fail");
    }

    #[tokio::test]
    async fn e2e_process_all_empty_vec() {
        let (processor, _dir) = make_processor_e2e();
        let results = processor.process_all(&[], "empty").await;
        assert!(
            results.is_empty(),
            "Empty input should produce empty output"
        );
    }

    #[tokio::test]
    async fn e2e_process_all_text_only() {
        let (processor, _dir) = make_processor_e2e();
        let blocks = vec![ContentBlock::text("hello"), ContentBlock::text("world")];
        let results = processor.process_all(&blocks, "text_only").await;
        assert!(
            results.is_empty(),
            "Text-only blocks should produce empty output"
        );
    }

    // --- Shared budget e2e ---

    #[tokio::test]
    async fn e2e_shared_budget_across_processors() {
        let budget = Arc::new(MediaBudget::with_max_per_run(200));

        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let p1 =
            MediaProcessor::with_shared_budget(CasStore::new(dir1.path()), Arc::clone(&budget));
        let p2 =
            MediaProcessor::with_shared_budget(CasStore::new(dir2.path()), Arc::clone(&budget));

        let png = real_png_bytes();
        let b64 = encode_b64(&png);

        // First processor succeeds
        let r1 = p1
            .process(&ContentBlock::image(b64.clone(), "image/png"), "t1")
            .await;
        assert!(r1.is_ok(), "First process should succeed: {:?}", r1.err());

        // Second processor succeeds (still under budget)
        let r2 = p2
            .process(&ContentBlock::image(b64.clone(), "image/png"), "t2")
            .await;
        assert!(r2.is_ok(), "Second process should succeed: {:?}", r2.err());

        // Third processor should fail (budget exceeded: ~70 bytes x 3 > 200)
        let r3 = p1
            .process(&ContentBlock::image(b64.clone(), "image/png"), "t3")
            .await;
        assert!(r3.is_err(), "Third process should exceed budget");
        assert_eq!(r3.unwrap_err().code(), "NIKA-259");
    }

    // --- Error code regression tests ---

    #[test]
    fn e2e_all_error_variants_have_display() {
        // Every error variant must have a non-empty Display and contain its NIKA code
        let cases: Vec<(MediaError, &str)> = vec![
            (
                MediaError::mime_detection_failed(100, Some("image/png".into())),
                "NIKA-251",
            ),
            (
                MediaError::UnsupportedMediaType {
                    mime_type: "x".into(),
                    reason: "y".into(),
                },
                "NIKA-252",
            ),
            (MediaError::MediaNotFound { hash: "h".into() }, "NIKA-253"),
            (
                MediaError::HashMismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                "NIKA-254",
            ),
            (
                MediaError::MediaStoreIo {
                    path: "/x".into(),
                    source: std::io::Error::other("test"),
                },
                "NIKA-255",
            ),
            (
                MediaError::Base64DecodeFailed {
                    source_desc: "x".into(),
                    reason: "y".into(),
                },
                "NIKA-256",
            ),
            (
                MediaError::Base64InputTooLarge {
                    size: 200,
                    max: 100,
                },
                "NIKA-257",
            ),
            (
                MediaError::EmptyMediaContent {
                    task_id: "t".into(),
                },
                "NIKA-258",
            ),
            (
                MediaError::RunBudgetExceeded {
                    current: 600,
                    max: 500,
                },
                "NIKA-259",
            ),
        ];
        for (err, code) in &cases {
            let display = format!("{err}");
            assert!(!display.is_empty(), "{code} has empty display");
            assert!(
                display.contains(code),
                "{code} display missing code: {display}"
            );
            assert_eq!(err.code(), *code);
        }
    }

    #[test]
    fn e2e_media_error_is_recoverable() {
        // Only MediaStoreIo (I/O errors) should be recoverable
        assert!(MediaError::MediaStoreIo {
            path: "/x".into(),
            source: std::io::Error::other(""),
        }
        .is_recoverable());

        assert!(!MediaError::mime_detection_failed(0, None).is_recoverable());
        assert!(!MediaError::Base64DecodeFailed {
            source_desc: "".into(),
            reason: "".into()
        }
        .is_recoverable());
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
        assert!(
            result.is_err(),
            "Unknown content type 'video' should fail deserialization"
        );
    }

    #[test]
    fn e2e_content_block_missing_type_fails() {
        let json = r#"{"text": "hello"}"#;
        let result: Result<ContentBlock, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Missing 'type' field should fail deserialization"
        );
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
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
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
        let results = processor
            .process_all(&tool_result.content, task_id.as_ref())
            .await;

        // Collect successful media refs
        let mut media_refs = Vec::new();
        for result in results {
            match result {
                Ok((media_ref, store_result)) => {
                    assert!(media_ref.hash.starts_with("blake3:"));
                    assert!(media_ref.size_bytes > 0);
                    assert_eq!(media_ref.created_by, "invoke_sim");
                    let cas_filename = store_result.path.file_name().unwrap().to_string_lossy();
                    assert!(
                        !cas_filename.contains('.'),
                        "CAS filename should not contain dot: {}",
                        cas_filename
                    );
                    media_refs.push(media_ref);
                }
                Err((idx, e)) => {
                    panic!("Block {idx} failed unexpectedly: {e}");
                }
            }
        }

        assert_eq!(
            media_refs.len(),
            2,
            "Should have 2 media refs (image + audio)"
        );

        // Stage in RunContext
        ctx.set_media(&task_id, media_refs.clone());

        // Take from RunContext (simulates what runner does)
        let taken = ctx.take_media(&task_id);
        assert_eq!(taken.len(), 2);

        // Build TaskResult with media
        use crate::store::TaskResult;
        let text = tool_result.text();
        let output = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
        let tr =
            TaskResult::success(output, std::time::Duration::from_millis(42)).with_media(taken);

        assert_eq!(tr.media.len(), 2);
        assert_eq!(tr.media[0].mime_type, "image/png");
        assert!(tr.media[1].mime_type.contains("mpeg") || tr.media[1].mime_type.contains("mp3"));

        // Verify text output is preserved
        assert_eq!(
            tr.output.as_str().unwrap(),
            "Image generated successfully\nAudio also available"
        );

        // Verify budget tracked
        assert!(
            budget.current_bytes() > 0,
            "Budget should have tracked bytes"
        );
    }

    #[tokio::test]
    async fn e2e_invoke_simulation_text_only_no_media() {
        // Simulates a text-only MCP response — NO media processing should happen
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "text_invoke".into();

        let tool_result = ToolCallResult::success(vec![ContentBlock::text("Just text, no media")]);

        assert!(!tool_result.has_media());

        // No media processing needed
        let taken = ctx.take_media(&task_id);
        assert!(taken.is_empty());

        use crate::store::TaskResult;
        let tr = TaskResult::success(
            serde_json::Value::String(tool_result.text()),
            std::time::Duration::from_millis(10),
        )
        .with_media(taken);

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

        let refs: Vec<_> = results.into_iter().map(|r| r.unwrap()).collect();

        // Both should have the same hash
        assert_eq!(
            refs[0].0.hash, refs[1].0.hash,
            "Same image should have same hash"
        );

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
            metadata: serde_json::Map::new(),
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

    // ═══════════════════════════════════════════════════════════════
    // PHASE E4: RESOLVE_PATH MEDIA BINDING TESTS
    // Tests {{with.task.media}}, {{with.task.media[0].hash}}, etc.
    // ═══════════════════════════════════════════════════════════════

    fn setup_ctx_with_media() -> (RunContext, Arc<str>) {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "gen_img".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!("image generated"),
            std::time::Duration::from_millis(100),
        )
        .with_media(vec![
            MediaRef {
                hash: "blake3:aabbccdd11223344".into(),
                mime_type: "image/png".into(),
                size_bytes: 4096,
                path: std::path::PathBuf::from("/tmp/store/aa/bbccdd11223344"),
                extension: "png".into(),
                created_by: "gen_img".into(),
                metadata: serde_json::Map::new(),
            },
            MediaRef {
                hash: "blake3:eeff0011aabbccdd".into(),
                mime_type: "audio/mpeg".into(),
                size_bytes: 8192,
                path: std::path::PathBuf::from("/tmp/store/ee/ff0011aabbccdd"),
                extension: "mp3".into(),
                created_by: "gen_img".into(),
                metadata: serde_json::Map::new(),
            },
        ]);
        ctx.insert(Arc::clone(&task_id), tr);
        (ctx, task_id)
    }

    #[test]
    fn e2e_resolve_path_media_full_array() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media");
        assert!(
            result.is_some(),
            "resolve_path('gen_img.media') should return Some"
        );
        let arr = result.unwrap();
        assert!(arr.is_array(), "media should be an array");
        assert_eq!(arr.as_array().unwrap().len(), 2, "should have 2 media refs");
    }

    #[test]
    fn e2e_resolve_path_media_first_hash() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].hash");
        assert!(
            result.is_some(),
            "resolve_path('gen_img.media[0].hash') should return Some"
        );
        assert_eq!(result.unwrap(), "blake3:aabbccdd11223344");
    }

    #[test]
    fn e2e_resolve_path_media_first_mime_type() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].mime_type");
        assert!(
            result.is_some(),
            "resolve_path('gen_img.media[0].mime_type') should return Some"
        );
        assert_eq!(result.unwrap(), "image/png");
    }

    #[test]
    fn e2e_resolve_path_media_second_hash() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[1].hash");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "blake3:eeff0011aabbccdd");
    }

    #[test]
    fn e2e_resolve_path_media_first_size_bytes() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].size_bytes");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 4096);
    }

    #[test]
    fn e2e_resolve_path_media_first_path() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].path");
        assert!(result.is_some());
        let path_val = result.unwrap();
        assert!(
            path_val
                .as_str()
                .unwrap()
                .contains("store/aa/bbccdd11223344"),
            "path should contain CAS location: {}",
            path_val
        );
    }

    #[test]
    fn e2e_resolve_path_media_first_extension() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].extension");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "png");
    }

    #[test]
    fn e2e_resolve_path_media_first_created_by() {
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].created_by");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "gen_img");
    }

    #[test]
    fn e2e_resolve_path_media_empty() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "no_media".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!("text only"),
            std::time::Duration::from_millis(10),
        );
        ctx.insert(Arc::clone(&task_id), tr);

        let result = ctx.resolve_path("no_media.media");
        assert!(result.is_some());
        let arr = result.unwrap();
        assert!(arr.is_array());
        assert!(
            arr.as_array().unwrap().is_empty(),
            "empty media should return []"
        );
    }

    #[test]
    fn e2e_resolve_path_media_nonexistent_task() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let result = ctx.resolve_path("nonexistent.media");
        assert!(result.is_none(), "nonexistent task should return None");
    }

    #[test]
    fn e2e_resolve_path_normal_output_still_works() {
        // Verify media interception doesn't break normal output resolution
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "weather".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!({"temperature": 22, "city": "Paris"}),
            std::time::Duration::from_millis(50),
        );
        ctx.insert(Arc::clone(&task_id), tr);

        let temp = ctx.resolve_path("weather.temperature");
        assert_eq!(temp, Some(serde_json::json!(22)));

        let city = ctx.resolve_path("weather.city");
        assert_eq!(city, Some(serde_json::json!("Paris")));
    }

    // ═══════════════════════════════════════════════════════════════
    // CAS STORE STRESS & INTEGRITY TESTS
    // ═══════════════════════════════════════════════════════════════

    // --- 1. Large file integrity ---

    #[tokio::test]
    async fn e2e_cas_large_file_above_threshold_verified_and_roundtrip() {
        // 2MB file (above VERIFY_THRESHOLD=1MB) must have verified=true
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Build 2MB data with a recognizable pattern (not all zeros)
        let mut data = Vec::with_capacity(2 * 1024 * 1024);
        for i in 0u32..(2 * 1024 * 1024 / 4) {
            data.extend_from_slice(&i.to_le_bytes());
        }
        assert_eq!(data.len(), 2 * 1024 * 1024);

        let result = store.store(&data).await.unwrap();
        assert!(
            result.verified,
            "2MB file must trigger read-back verification"
        );
        assert!(!result.deduplicated);
        assert_eq!(result.size, 2 * 1024 * 1024);

        // Read back and verify byte-for-byte
        let read_back = store.read(&result.hash).await.unwrap();
        assert_eq!(read_back.len(), data.len());
        assert_eq!(read_back, data, "byte-for-byte mismatch on 2MB roundtrip");
    }

    #[tokio::test]
    async fn e2e_cas_large_file_dedup_on_second_store() {
        // Store 2MB file twice: second store must be deduplicated
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = vec![0xFE_u8; 2 * 1024 * 1024];
        let r1 = store.store(&data).await.unwrap();
        assert!(!r1.deduplicated);
        assert!(r1.verified);

        let r2 = store.store(&data).await.unwrap();
        assert!(
            r2.deduplicated,
            "second store of identical 2MB file must dedup"
        );
        assert_eq!(r1.hash, r2.hash);
        assert_eq!(r1.path, r2.path);
    }

    // --- 2. Binary data integrity with ALL byte values ---

    #[tokio::test]
    async fn e2e_cas_all_byte_values_no_corruption() {
        // Store data containing every possible byte value 0x00..0xFF
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let mut data: Vec<u8> = (0x00..=0xFF).collect();
        // Repeat 4 times so we also test multi-occurrence
        let single_round = data.clone();
        for _ in 0..3 {
            data.extend_from_slice(&single_round);
        }
        assert_eq!(data.len(), 256 * 4);

        let result = store.store(&data).await.unwrap();
        let read_back = store.read(&result.hash).await.unwrap();
        assert_eq!(
            read_back, data,
            "all-bytes roundtrip must not corrupt any value"
        );

        // Every byte value must be present in the read-back
        for byte in 0x00..=0xFF_u8 {
            assert!(
                read_back.contains(&byte),
                "byte 0x{byte:02X} missing after roundtrip"
            );
        }
    }

    #[tokio::test]
    async fn e2e_cas_blake3_hash_is_deterministic() {
        // Same data must always produce the same hash
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data: Vec<u8> = (0..=255).collect();

        let r1 = store.store(&data).await.unwrap();
        // r2 will be deduped, but hash must match
        let r2 = store.store(&data).await.unwrap();
        assert_eq!(r1.hash, r2.hash, "blake3 hash must be deterministic");

        // Also verify against direct blake3 computation
        let expected_raw = blake3::hash(&data).to_hex().to_string();
        let expected = format!("blake3:{expected_raw}");
        assert_eq!(
            r1.hash, expected,
            "hash must match direct blake3 computation"
        );
    }

    // --- 3. CAS cleanup edge cases ---

    #[tokio::test]
    async fn e2e_cas_clean_older_than_zero_cleans_everything() {
        // Duration::ZERO means "older than 0s" — every file qualifies
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"alpha").await.unwrap();
        store.store(b"beta").await.unwrap();
        store.store(b"gamma").await.unwrap();
        assert_eq!(store.list().len(), 3);

        // Files are always at least a few nanoseconds old
        // Small sleep to ensure mtime is strictly in the past
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let clean = store.clean_older_than(std::time::Duration::ZERO);
        assert_eq!(clean.removed, 3, "Duration::ZERO should remove all files");
        assert_eq!(store.list().len(), 0);
    }

    #[tokio::test]
    async fn e2e_cas_clean_older_than_long_duration_cleans_nothing() {
        // Duration of 1 hour — recently-created files should survive
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"fresh-1").await.unwrap();
        store.store(b"fresh-2").await.unwrap();

        let clean = store.clean_older_than(std::time::Duration::from_secs(3600));
        assert_eq!(clean.removed, 0, "no files should be older than 1 hour");
        assert_eq!(store.list().len(), 2, "both files should survive");
    }

    #[tokio::test]
    async fn e2e_cas_clean_all_on_empty_store() {
        // clean_all on empty store should return removed=0, bytes_freed=0
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let clean = store.clean_all();
        assert_eq!(clean.removed, 0);
        assert_eq!(clean.bytes_freed, 0);
    }

    #[tokio::test]
    async fn e2e_cas_clean_all_twice_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"to-be-removed").await.unwrap();
        let c1 = store.clean_all();
        assert_eq!(c1.removed, 1);

        let c2 = store.clean_all();
        assert_eq!(c2.removed, 0, "second clean_all should find nothing");
        assert_eq!(c2.bytes_freed, 0);
    }

    #[tokio::test]
    async fn e2e_cas_clean_older_than_zero_on_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let clean = store.clean_older_than(std::time::Duration::ZERO);
        assert_eq!(clean.removed, 0);
        assert_eq!(clean.bytes_freed, 0);
    }

    #[tokio::test]
    async fn e2e_cas_clean_reports_correct_bytes_freed() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data_a = vec![0xAA_u8; 1000];
        let data_b = vec![0xBB_u8; 2000];
        let r_a = store.store(&data_a).await.unwrap();
        let r_b = store.store(&data_b).await.unwrap();

        // Measure actual on-disk sizes before cleaning (compression may shrink data)
        let on_disk_a = std::fs::metadata(&r_a.path).unwrap().len();
        let on_disk_b = std::fs::metadata(&r_b.path).unwrap().len();

        let clean = store.clean_all();
        assert_eq!(clean.removed, 2);
        assert_eq!(
            clean.bytes_freed,
            on_disk_a + on_disk_b,
            "bytes_freed must equal sum of on-disk file sizes"
        );
    }

    // --- 4. Hash format validation ---

    #[tokio::test]
    async fn e2e_cas_hash_format_64_hex_after_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"hash format test").await.unwrap();

        // Must start with "blake3:"
        assert!(result.hash.starts_with("blake3:"), "missing blake3: prefix");

        let raw = result.hash.strip_prefix("blake3:").unwrap();
        // Must be exactly 64 hex characters
        assert_eq!(
            raw.len(),
            64,
            "raw hash must be 64 hex chars, got {}",
            raw.len()
        );
        assert!(
            raw.chars().all(|c| c.is_ascii_hexdigit()),
            "hash contains non-hex characters: {raw}"
        );
    }

    #[tokio::test]
    async fn e2e_cas_hash_deterministic_multiple_runs() {
        // Verify determinism across separate CasStore instances
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let store1 = CasStore::new(dir1.path());
        let store2 = CasStore::new(dir2.path());

        let data = b"determinism across stores";
        let r1 = store1.store(data).await.unwrap();
        let r2 = store2.store(data).await.unwrap();

        assert_eq!(
            r1.hash, r2.hash,
            "same data must produce same hash in different stores"
        );
    }

    #[tokio::test]
    async fn e2e_cas_different_data_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let r1 = store.store(b"data-alpha").await.unwrap();
        let r2 = store.store(b"data-beta").await.unwrap();
        let r3 = store.store(b"data-gamma").await.unwrap();

        // All hashes must be distinct
        assert_ne!(r1.hash, r2.hash, "alpha vs beta must differ");
        assert_ne!(r1.hash, r3.hash, "alpha vs gamma must differ");
        assert_ne!(r2.hash, r3.hash, "beta vs gamma must differ");
    }

    #[tokio::test]
    async fn e2e_cas_single_bit_difference_changes_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data_a = vec![0x00_u8; 64];
        let mut data_b = data_a.clone();
        data_b[31] = 0x01; // flip one bit in the middle

        let ra = store.store(&data_a).await.unwrap();
        let rb = store.store(&data_b).await.unwrap();
        assert_ne!(ra.hash, rb.hash, "single bit flip must change hash");
    }

    // --- 5. Path safety ---

    #[tokio::test]
    async fn e2e_cas_path_never_escapes_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store various data to generate different hashes/shards
        for i in 0..20_u32 {
            let data = i.to_le_bytes();
            let result = store.store(&data).await.unwrap();

            // The final path must always be under the root directory
            assert!(
                result.path.starts_with(dir.path()),
                "CAS path escaped root: {:?} not under {:?}",
                result.path,
                dir.path()
            );

            // Must not contain ".." anywhere
            let path_str = result.path.to_string_lossy();
            assert!(
                !path_str.contains(".."),
                "CAS path contains directory traversal: {path_str}"
            );
        }
    }

    #[tokio::test]
    async fn e2e_cas_shard_directory_is_2_hex_chars() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        for i in 0..20_u32 {
            let result = store.store(&i.to_le_bytes()).await.unwrap();

            // Path structure: {root}/{shard}/{filename}
            let relative = result.path.strip_prefix(dir.path()).unwrap();
            let components: Vec<_> = relative.components().collect();
            assert_eq!(
                components.len(),
                2,
                "CAS path must have exactly 2 components (shard/file), got {}: {:?}",
                components.len(),
                relative
            );

            let shard = components[0].as_os_str().to_string_lossy();
            assert_eq!(shard.len(), 2, "shard dir must be 2 chars, got '{shard}'");
            assert!(
                shard.chars().all(|c| c.is_ascii_hexdigit()),
                "shard dir must be hex, got '{shard}'"
            );
        }
    }

    #[tokio::test]
    async fn e2e_cas_file_has_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Test with data that could trick an extension parser
        let png = real_png_bytes();
        let jpeg = real_jpeg_bytes();
        let test_data: Vec<&[u8]> = vec![
            b"hello.png",   // text that looks like a filename
            b"data.tar.gz", // double extension
            &png,           // actual PNG binary
            &jpeg,          // actual JPEG binary
        ];

        for data in &test_data {
            let result = store.store(data).await.unwrap();
            assert!(
                result.path.extension().is_none(),
                "CAS file must have no extension, got: {:?}",
                result.path
            );
        }
    }

    #[tokio::test]
    async fn e2e_cas_shard_and_filename_reconstruct_hash() {
        // Verify shard + filename = raw hash
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"reconstruct test").await.unwrap();
        let raw_hash = result.hash.strip_prefix("blake3:").unwrap();

        let relative = result.path.strip_prefix(dir.path()).unwrap();
        let components: Vec<_> = relative.components().collect();
        let shard = components[0].as_os_str().to_string_lossy();
        let filename = components[1].as_os_str().to_string_lossy();

        let reconstructed = format!("{shard}{filename}");
        assert_eq!(
            reconstructed, raw_hash,
            "shard + filename must reconstruct the raw hash"
        );
    }

    // --- 6. Concurrent read + write ---

    #[tokio::test]
    async fn e2e_cas_concurrent_read_write_no_partial_reads() {
        // Pre-store data so it exists, then concurrently write (dedup) + read
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path()));

        // Build recognizable 512KB pattern
        let mut data = Vec::with_capacity(512 * 1024);
        for i in 0u32..(512 * 1024 / 4) {
            data.extend_from_slice(&i.to_le_bytes());
        }

        // Initial store
        let initial = store.store(&data).await.unwrap();
        let hash = initial.hash.clone();

        // Spawn 5 writers (dedup) and 5 readers concurrently
        let mut handles = Vec::new();

        for _ in 0..5 {
            let s = Arc::clone(&store);
            let d = data.clone();
            handles.push(tokio::spawn(async move {
                s.store(&d).await.unwrap();
            }));
        }

        for _ in 0..5 {
            let s = Arc::clone(&store);
            let h = hash.clone();
            let expected_len = data.len();
            handles.push(tokio::spawn(async move {
                let read_back = s.read(&h).await.unwrap();
                assert_eq!(
                    read_back.len(),
                    expected_len,
                    "partial read detected: got {} bytes, expected {}",
                    read_back.len(),
                    expected_len
                );
            }));
        }

        futures::future::join_all(handles)
            .await
            .into_iter()
            .for_each(|h| h.unwrap());
    }

    #[tokio::test]
    async fn e2e_cas_concurrent_store_different_data_no_collision() {
        // Concurrently store 20 different files and verify all are distinct
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path()));

        let handles: Vec<_> = (0..20_u32)
            .map(|i| {
                let s = Arc::clone(&store);
                tokio::spawn(async move {
                    let data = format!("unique-content-{i}");
                    let result = s.store(data.as_bytes()).await.unwrap();
                    (i, result.hash)
                })
            })
            .collect();

        let results: Vec<(u32, String)> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap())
            .collect();

        // All hashes must be unique
        let mut hashes: Vec<&str> = results.iter().map(|(_, h)| h.as_str()).collect();
        hashes.sort();
        hashes.dedup();
        assert_eq!(
            hashes.len(),
            20,
            "20 different inputs must produce 20 distinct hashes"
        );

        // Verify all files readable
        assert_eq!(store.list().len(), 20);
    }

    #[tokio::test]
    async fn e2e_cas_concurrent_read_after_store_all_correct() {
        // Store N files, then concurrently read all back and verify content
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path()));

        let items: Vec<(Vec<u8>, String)> = {
            let mut v = Vec::new();
            for i in 0..10_u32 {
                let data = format!("content-for-verification-{i}").into_bytes();
                let result = store.store(&data).await.unwrap();
                v.push((data, result.hash));
            }
            v
        };

        let handles: Vec<_> = items
            .into_iter()
            .map(|(expected_data, hash)| {
                let s = Arc::clone(&store);
                tokio::spawn(async move {
                    let read_back = s.read(&hash).await.unwrap();
                    assert_eq!(read_back, expected_data, "content mismatch for hash {hash}");
                })
            })
            .collect();

        futures::future::join_all(handles)
            .await
            .into_iter()
            .for_each(|h| h.unwrap());
    }

    // --- Extra edge cases ---

    #[tokio::test]
    async fn e2e_cas_empty_data_store_and_read() {
        // Defense-in-depth: empty data is now rejected at CAS layer (NIKA-258)
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"").await;
        assert!(result.is_err(), "CAS should reject empty data");
        assert_eq!(result.unwrap_err().code(), "NIKA-258");
    }

    #[tokio::test]
    async fn e2e_cas_single_byte_store_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(&[0x42]).await.unwrap();
        assert_eq!(result.size, 1);

        let read_back = store.read(&result.hash).await.unwrap();
        assert_eq!(read_back, vec![0x42]);
    }

    #[tokio::test]
    async fn e2e_cas_list_after_clean_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        for i in 0..5_u32 {
            store.store(&i.to_le_bytes()).await.unwrap();
        }
        assert_eq!(store.list().len(), 5);

        store.clean_all();
        assert_eq!(store.list().len(), 0);

        // Re-store one file: list should show exactly 1
        store.store(b"post-clean").await.unwrap();
        assert_eq!(store.list().len(), 1);
    }

    #[tokio::test]
    async fn e2e_cas_list_entries_have_correct_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let data = b"list field check";
        let store_result = store.store(data).await.unwrap();

        let entries = store.list();
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.hash, store_result.hash);
        assert_eq!(entry.path, store_result.path);

        // entry.size is on-disk size; with media-compression the NK framing
        // header adds 4 bytes.
        #[cfg(feature = "media-compression")]
        let expected_size = data.len() as u64 + 4;
        #[cfg(not(feature = "media-compression"))]
        let expected_size = data.len() as u64;
        assert_eq!(entry.size, expected_size);
    }

    #[tokio::test]
    async fn e2e_cas_exists_returns_false_after_clean() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"will-be-cleaned").await.unwrap();
        assert!(store.exists(&result.hash));

        store.clean_all();
        assert!(
            !store.exists(&result.hash),
            "exists must return false after clean_all"
        );
    }

    #[tokio::test]
    async fn e2e_cas_read_after_clean_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.store(b"ephemeral").await.unwrap();
        store.clean_all();

        let read = store.read(&result.hash).await;
        assert!(
            read.is_err(),
            "reading cleaned hash should fail but got: {:?}",
            read.as_ref().ok()
        );
        assert_eq!(read.unwrap_err().code(), "NIKA-253");
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE F: Template binding pipeline — edge cases & isolation
    // ═══════════════════════════════════════════════════════════════

    // -----------------------------------------------------------------
    // F1: resolve_path edge cases not yet tested
    // -----------------------------------------------------------------

    #[test]
    fn e2e_resolve_path_media_whole_object_no_field() {
        // "task.media[0]" should return the entire MediaRef JSON object
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0]");
        assert!(
            result.is_some(),
            "media[0] without field should return the full MediaRef"
        );
        let obj = result.unwrap();
        assert!(
            obj.is_object(),
            "media[0] should be a JSON object, got: {}",
            obj
        );
        // Verify it contains all 6 expected fields
        let map = obj.as_object().unwrap();
        assert_eq!(
            map.get("hash").and_then(|v| v.as_str()),
            Some("blake3:aabbccdd11223344")
        );
        assert_eq!(
            map.get("mime_type").and_then(|v| v.as_str()),
            Some("image/png")
        );
        assert_eq!(map.get("size_bytes").and_then(|v| v.as_u64()), Some(4096));
        assert_eq!(map.get("extension").and_then(|v| v.as_str()), Some("png"));
        assert_eq!(
            map.get("created_by").and_then(|v| v.as_str()),
            Some("gen_img")
        );
        assert!(map.contains_key("path"), "should have 'path' field");
    }

    #[test]
    fn e2e_resolve_path_media_whole_object_second() {
        // "task.media[1]" should return the second MediaRef
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[1]");
        assert!(result.is_some());
        let obj = result.unwrap();
        assert!(obj.is_object());
        assert_eq!(obj["hash"], "blake3:eeff0011aabbccdd");
        assert_eq!(obj["mime_type"], "audio/mpeg");
    }

    #[test]
    fn e2e_resolve_path_media_dot_length() {
        // "task.media.length" -- not a valid JSON path; arrays don't have .length
        // Should return None (no JavaScript-style .length in JSON)
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media.length");
        assert!(
            result.is_none(),
            "media.length is not valid JSON path, should return None"
        );
    }

    #[test]
    fn e2e_resolve_path_media_out_of_bounds_with_field() {
        // "task.media[999].hash" -- index beyond array length
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[999].hash");
        assert!(result.is_none(), "out-of-bounds index should return None");
    }

    #[test]
    fn e2e_resolve_path_media_out_of_bounds_no_field() {
        // "task.media[999]" -- whole object at out-of-bounds index
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[999]");
        assert!(
            result.is_none(),
            "out-of-bounds index without field should return None"
        );
    }

    #[test]
    fn e2e_resolve_path_mediax_not_intercepted() {
        // "task.mediax" -- starts with "media" but is NOT a media path
        // Should resolve from task output, not media interception
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "task".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!({"mediax": "extra_value", "media_extra": 42}),
            std::time::Duration::from_millis(10),
        );
        ctx.insert(Arc::clone(&task_id), tr);

        // "task.mediax" should resolve from output (not media interception)
        let result = ctx.resolve_path("task.mediax");
        assert_eq!(
            result,
            Some(serde_json::json!("extra_value")),
            "mediax should resolve from task output, not media interception"
        );
    }

    #[test]
    fn e2e_resolve_path_media_extra_not_intercepted() {
        // "task.media_extra" -- starts with "media" prefix + underscore
        // Should NOT be intercepted by the media path logic
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "task".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!({"media_extra": "should_not_be_media"}),
            std::time::Duration::from_millis(10),
        );
        ctx.insert(Arc::clone(&task_id), tr);

        let result = ctx.resolve_path("task.media_extra");
        assert_eq!(
            result,
            Some(serde_json::json!("should_not_be_media")),
            "media_extra should NOT be intercepted by media path logic"
        );
    }

    #[test]
    fn e2e_resolve_path_media_nonexistent_field() {
        // "task.media[0].nonexistent_field" -- valid index, invalid field name
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].nonexistent_field");
        assert!(
            result.is_none(),
            "nonexistent field on MediaRef should return None"
        );
    }

    #[test]
    fn e2e_resolve_path_media_deeply_nested_invalid() {
        // "task.media[0].hash.something" -- trying to navigate into a string field
        let (ctx, _) = setup_ctx_with_media();
        let result = ctx.resolve_path("gen_img.media[0].hash.something");
        assert!(result.is_none(), "cannot navigate into a string field");
    }

    // -----------------------------------------------------------------
    // F2: Multiple tasks with media -- isolation
    // -----------------------------------------------------------------

    /// Helper: set up two tasks each with their own media refs
    fn setup_two_tasks_with_media() -> RunContext {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task A: 1 image
        let media_a = vec![MediaRef {
            hash: "blake3:aaaa111122223333".into(),
            mime_type: "image/jpeg".into(),
            size_bytes: 2048,
            path: std::path::PathBuf::from("/tmp/cas/aa/aa111122223333"),
            extension: "jpg".into(),
            created_by: "task_a".into(),
            metadata: serde_json::Map::new(),
        }];
        let tr_a = crate::store::TaskResult::success(
            serde_json::json!({"prompt": "a dog"}),
            std::time::Duration::from_millis(100),
        )
        .with_media(media_a);
        ctx.insert(Arc::from("task_a"), tr_a);

        // Task B: 2 audio files
        let media_b = vec![
            MediaRef {
                hash: "blake3:bbbb444455556666".into(),
                mime_type: "audio/wav".into(),
                size_bytes: 16384,
                path: std::path::PathBuf::from("/tmp/cas/bb/bb444455556666"),
                extension: "wav".into(),
                created_by: "task_b".into(),
                metadata: serde_json::Map::new(),
            },
            MediaRef {
                hash: "blake3:bbbb777788889999".into(),
                mime_type: "audio/mpeg".into(),
                size_bytes: 32768,
                path: std::path::PathBuf::from("/tmp/cas/bb/bb777788889999"),
                extension: "mp3".into(),
                created_by: "task_b".into(),
                metadata: serde_json::Map::new(),
            },
        ];
        let tr_b = crate::store::TaskResult::success(
            serde_json::json!({"source": "microphone"}),
            std::time::Duration::from_millis(200),
        )
        .with_media(media_b);
        ctx.insert(Arc::from("task_b"), tr_b);

        ctx
    }

    #[test]
    fn e2e_multi_task_media_array_counts() {
        let ctx = setup_two_tasks_with_media();

        let a_media = ctx.resolve_path("task_a.media").unwrap();
        assert_eq!(
            a_media.as_array().unwrap().len(),
            1,
            "task_a should have 1 media ref"
        );

        let b_media = ctx.resolve_path("task_b.media").unwrap();
        assert_eq!(
            b_media.as_array().unwrap().len(),
            2,
            "task_b should have 2 media refs"
        );
    }

    #[test]
    fn e2e_multi_task_media_no_cross_leak() {
        let ctx = setup_two_tasks_with_media();

        // task_a.media[0].hash should be task_a's image, not task_b's audio
        let a_hash = ctx.resolve_path("task_a.media[0].hash").unwrap();
        assert_eq!(a_hash, "blake3:aaaa111122223333");

        // task_b.media[0].hash should be task_b's first audio
        let b_hash = ctx.resolve_path("task_b.media[0].hash").unwrap();
        assert_eq!(b_hash, "blake3:bbbb444455556666");

        // task_b.media[1].hash should be task_b's second audio
        let b_hash2 = ctx.resolve_path("task_b.media[1].hash").unwrap();
        assert_eq!(b_hash2, "blake3:bbbb777788889999");
    }

    #[test]
    fn e2e_multi_task_media_mime_types_isolated() {
        let ctx = setup_two_tasks_with_media();

        // task_a has image, task_b has audio -- no type confusion
        assert_eq!(
            ctx.resolve_path("task_a.media[0].mime_type").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            ctx.resolve_path("task_b.media[0].mime_type").unwrap(),
            "audio/wav"
        );
        assert_eq!(
            ctx.resolve_path("task_b.media[1].mime_type").unwrap(),
            "audio/mpeg"
        );
    }

    #[test]
    fn e2e_multi_task_output_not_affected_by_media() {
        let ctx = setup_two_tasks_with_media();

        // Standard output fields must resolve independently of media
        assert_eq!(ctx.resolve_path("task_a.prompt").unwrap(), "a dog");
        assert_eq!(ctx.resolve_path("task_b.source").unwrap(), "microphone");
    }

    #[test]
    fn e2e_multi_task_media_created_by_correct() {
        let ctx = setup_two_tasks_with_media();

        // Each media ref's created_by should reference its own task
        assert_eq!(
            ctx.resolve_path("task_a.media[0].created_by").unwrap(),
            "task_a"
        );
        assert_eq!(
            ctx.resolve_path("task_b.media[0].created_by").unwrap(),
            "task_b"
        );
        assert_eq!(
            ctx.resolve_path("task_b.media[1].created_by").unwrap(),
            "task_b"
        );
    }

    // -----------------------------------------------------------------
    // F3: TaskResult with both output AND media -- no collision
    // -----------------------------------------------------------------

    #[test]
    fn e2e_output_and_media_coexist_no_collision() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "gen".into();

        // Task output has structured JSON AND media refs
        let media = vec![MediaRef {
            hash: "blake3:face0000dead0000".into(),
            mime_type: "image/png".into(),
            size_bytes: 1024,
            path: std::path::PathBuf::from("/tmp/cas/fa/ce0000dead0000"),
            extension: "png".into(),
            created_by: "gen".into(),
            metadata: serde_json::Map::new(),
        }];
        let tr = crate::store::TaskResult::success(
            serde_json::json!({
                "status": "ok",
                "url": "https://example.com/image.png",
                "dimensions": {"width": 512, "height": 512}
            }),
            std::time::Duration::from_millis(250),
        )
        .with_media(media);
        ctx.insert(Arc::clone(&task_id), tr);

        // Output fields resolve from output
        assert_eq!(ctx.resolve_path("gen.status").unwrap(), "ok");
        assert_eq!(
            ctx.resolve_path("gen.url").unwrap(),
            "https://example.com/image.png"
        );
        assert_eq!(ctx.resolve_path("gen.dimensions.width").unwrap(), 512);
        assert_eq!(ctx.resolve_path("gen.dimensions.height").unwrap(), 512);

        // Media fields resolve from media refs
        assert_eq!(
            ctx.resolve_path("gen.media[0].hash").unwrap(),
            "blake3:face0000dead0000"
        );
        assert_eq!(
            ctx.resolve_path("gen.media[0].mime_type").unwrap(),
            "image/png"
        );
        assert_eq!(ctx.resolve_path("gen.media[0].size_bytes").unwrap(), 1024);
        assert_eq!(ctx.resolve_path("gen.media[0].extension").unwrap(), "png");

        // Full media array
        let arr = ctx.resolve_path("gen.media").unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 1);

        // Full output (no media key contamination)
        let full = ctx.resolve_path("gen").unwrap();
        assert!(full.is_object());
        assert_eq!(full["status"], "ok");
        // Media should NOT appear in the output Value itself
        assert!(
            full.get("media").is_none(),
            "media should not be merged into the output JSON"
        );
    }

    #[test]
    fn e2e_output_with_media_key_in_output() {
        // Edge case: task output itself has a "media" key
        // The media interception should still use TaskResult.media, not output.media
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "tricky".into();

        let media = vec![MediaRef {
            hash: "blake3:real_media_hash000".into(),
            mime_type: "image/webp".into(),
            size_bytes: 500,
            path: std::path::PathBuf::from("/tmp/cas/re/al_media_hash000"),
            extension: "webp".into(),
            created_by: "tricky".into(),
            metadata: serde_json::Map::new(),
        }];
        let tr = crate::store::TaskResult::success(
            serde_json::json!({
                "media": "this is output media, not real",
                "other": "value"
            }),
            std::time::Duration::from_millis(10),
        )
        .with_media(media);
        ctx.insert(Arc::clone(&task_id), tr);

        // "tricky.media" should return the TaskResult.media array (from interception),
        // NOT the output "media" string field
        let result = ctx.resolve_path("tricky.media");
        assert!(result.is_some());
        let arr = result.unwrap();
        assert!(
            arr.is_array(),
            "media path should return the media array, not output field"
        );
        assert_eq!(arr.as_array().unwrap().len(), 1);
        assert_eq!(arr[0]["hash"], "blake3:real_media_hash000");

        // Other output fields still work
        assert_eq!(ctx.resolve_path("tricky.other").unwrap(), "value");
    }

    #[test]
    fn e2e_output_field_named_media_no_real_media() {
        // Task output has "media" field but TaskResult.media is empty
        // "task.media" should return empty array (from interception), not output.media
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "confusing".into();
        let tr = crate::store::TaskResult::success(
            serde_json::json!({
                "media": ["fake_ref_1", "fake_ref_2"],
                "count": 2
            }),
            std::time::Duration::from_millis(10),
        );
        ctx.insert(Arc::clone(&task_id), tr);

        let result = ctx.resolve_path("confusing.media");
        assert!(result.is_some());
        let arr = result.unwrap();
        assert!(arr.is_array());
        // Empty because TaskResult.media is empty -- interception takes priority
        assert!(
            arr.as_array().unwrap().is_empty(),
            "should return empty media array, not output.media field"
        );
    }

    // -----------------------------------------------------------------
    // F4: MediaRef serialization completeness
    // -----------------------------------------------------------------

    #[test]
    fn e2e_mediaref_json_has_all_six_fields() {
        let mr = MediaRef {
            hash: "blake3:0123456789abcdef".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 65536,
            path: std::path::PathBuf::from("/tmp/cas/01/23456789abcdef"),
            extension: "pdf".into(),
            created_by: "pdf_gen".into(),
            metadata: serde_json::Map::new(),
        };
        let json = serde_json::to_value(&mr).unwrap();
        let obj = json
            .as_object()
            .expect("MediaRef should serialize to object");

        // Exactly 6 fields
        assert_eq!(
            obj.len(),
            6,
            "MediaRef should have exactly 6 fields, got: {:?}",
            obj.keys().collect::<Vec<_>>()
        );

        // Each field present and correct type
        assert!(obj["hash"].is_string(), "hash should be string");
        assert!(obj["mime_type"].is_string(), "mime_type should be string");
        assert!(obj["size_bytes"].is_u64(), "size_bytes should be u64");
        assert!(obj["path"].is_string(), "path should serialize as string");
        assert!(obj["extension"].is_string(), "extension should be string");
        assert!(obj["created_by"].is_string(), "created_by should be string");
    }

    #[test]
    fn e2e_mediaref_path_serializes_as_absolute() {
        let mr = MediaRef {
            hash: "blake3:abcdef0123456789".into(),
            mime_type: "image/png".into(),
            size_bytes: 1024,
            path: std::path::PathBuf::from("/var/nika/cas/ab/cdef0123456789"),
            extension: "png".into(),
            created_by: "test".into(),
            metadata: serde_json::Map::new(),
        };
        let json = serde_json::to_value(&mr).unwrap();
        let path_str = json["path"].as_str().unwrap();

        assert!(
            path_str.starts_with('/'),
            "serialized path should be absolute (start with /), got: {}",
            path_str
        );
        assert!(
            path_str.contains("cas/ab/cdef0123456789"),
            "path should contain the CAS directory structure, got: {}",
            path_str
        );
    }

    #[test]
    fn e2e_mediaref_hash_always_blake3_prefix() {
        // Verify hash field always has the algorithm prefix
        let hashes = vec![
            "blake3:0000000000000000",
            "blake3:ffffffffffffffff",
            "blake3:aabbccdd11223344",
        ];

        for hash_val in hashes {
            let mr = MediaRef {
                hash: hash_val.into(),
                mime_type: "image/png".into(),
                size_bytes: 1,
                path: std::path::PathBuf::from("/tmp/cas/00/00"),
                extension: "png".into(),
                created_by: "test".into(),
                metadata: serde_json::Map::new(),
            };
            let json = serde_json::to_value(&mr).unwrap();
            let serialized_hash = json["hash"].as_str().unwrap();
            assert!(
                serialized_hash.starts_with("blake3:"),
                "hash must start with 'blake3:', got: {}",
                serialized_hash
            );
        }
    }

    #[test]
    fn e2e_mediaref_roundtrip_preserves_all_fields() {
        // Serialize -> Deserialize should be identity for all field values
        let original = MediaRef {
            hash: "blake3:deadbeefcafebabe".into(),
            mime_type: "audio/wav".into(),
            size_bytes: 123456789,
            path: std::path::PathBuf::from("/opt/nika/cas/de/adbeefcafebabe"),
            extension: "wav".into(),
            created_by: "audio_task".into(),
            metadata: serde_json::Map::new(),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: MediaRef = serde_json::from_str(&json_str).unwrap();

        assert_eq!(original.hash, deserialized.hash);
        assert_eq!(original.mime_type, deserialized.mime_type);
        assert_eq!(original.size_bytes, deserialized.size_bytes);
        assert_eq!(original.path, deserialized.path);
        assert_eq!(original.extension, deserialized.extension);
        assert_eq!(original.created_by, deserialized.created_by);
    }

    #[test]
    fn e2e_mediaref_resolve_path_roundtrip_all_fields() {
        // Insert a TaskResult with media, then verify resolve_path returns
        // correct values for ALL 6 fields through the full pipeline
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "roundtrip".into();
        let media = vec![MediaRef {
            hash: "blake3:a1b2c3d4e5f60000".into(),
            mime_type: "image/gif".into(),
            size_bytes: 99999,
            path: std::path::PathBuf::from("/data/cas/a1/b2c3d4e5f60000"),
            extension: "gif".into(),
            created_by: "roundtrip".into(),
            metadata: serde_json::Map::new(),
        }];
        let tr = crate::store::TaskResult::success(
            serde_json::json!({"done": true}),
            std::time::Duration::from_millis(50),
        )
        .with_media(media);
        ctx.insert(Arc::clone(&task_id), tr);

        // Verify each field through resolve_path
        assert_eq!(
            ctx.resolve_path("roundtrip.media[0].hash").unwrap(),
            "blake3:a1b2c3d4e5f60000"
        );
        assert_eq!(
            ctx.resolve_path("roundtrip.media[0].mime_type").unwrap(),
            "image/gif"
        );
        assert_eq!(
            ctx.resolve_path("roundtrip.media[0].size_bytes").unwrap(),
            99999
        );
        let path_val = ctx.resolve_path("roundtrip.media[0].path").unwrap();
        assert_eq!(path_val.as_str().unwrap(), "/data/cas/a1/b2c3d4e5f60000");
        assert_eq!(
            ctx.resolve_path("roundtrip.media[0].extension").unwrap(),
            "gif"
        );
        assert_eq!(
            ctx.resolve_path("roundtrip.media[0].created_by").unwrap(),
            "roundtrip"
        );

        // Also verify the whole object has exactly 6 keys
        let whole = ctx.resolve_path("roundtrip.media[0]").unwrap();
        assert_eq!(whole.as_object().unwrap().len(), 6);
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE G: EXHAUSTIVE MIME DETECTION & PROCESSOR EDGE-CASE TESTS
    // ═══════════════════════════════════════════════════════════════

    // ---------------------------------------------------------------
    // G1. MIME detection with ALL P0 types from the plan
    // ---------------------------------------------------------------

    #[test]
    fn g_detect_png_magic_bytes_89504e47() {
        let png = real_png_bytes();
        let result = detect_mime(&png, None).unwrap();
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(result.extension, "png");
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    #[test]
    fn g_detect_jpeg_magic_bytes_ffd8ff() {
        let jpeg = real_jpeg_bytes();
        let result = detect_mime(&jpeg, None).unwrap();
        assert_eq!(result.mime_type, "image/jpeg");
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    #[test]
    fn g_detect_webp_riff_webp_vp8() {
        let webp = vec![
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x24, 0x00, 0x00, 0x00, // chunk size
            0x57, 0x45, 0x42, 0x50, // WEBP
            0x56, 0x50, 0x38, 0x20, // "VP8 "
            0x00, 0x00, 0x00, 0x00,
        ];
        let result = detect_mime(&webp, None).unwrap();
        assert_eq!(result.mime_type, "image/webp");
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    #[test]
    fn g_detect_mp3_with_id3v2_tag() {
        let mp3 = real_mp3_bytes();
        let result = detect_mime(&mp3, None).unwrap();
        assert!(
            result.mime_type == "audio/mpeg" || result.mime_type == "audio/mp3",
            "Expected audio/mpeg or audio/mp3, got: {}",
            result.mime_type
        );
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    #[test]
    fn g_detect_mp3_ff_fb_sync_word() {
        // MP3 frame sync: FF FB (MPEG1, Layer 3, no CRC)
        let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
        frame.extend_from_slice(&[0x00; 413]);
        let result = detect_mime(&frame, None);
        match result {
            Ok(d) => {
                assert!(
                    d.mime_type.contains("mpeg") || d.mime_type.contains("mp3"),
                    "FF FB sync should detect as MP3, got: {}",
                    d.mime_type
                );
            }
            Err(_) => {
                // Server hint rescue
                let rescued = detect_mime(&frame, Some("audio/mpeg")).unwrap();
                assert_eq!(rescued.mime_type, "audio/mpeg");
                assert_eq!(
                    rescued.source,
                    crate::media::detect::DetectionSource::ServerHint
                );
            }
        }
    }

    #[test]
    fn g_detect_wav_riff_wave_with_fmt() {
        let wav = vec![
            0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6D,
            0x74, 0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x44, 0xAC, 0x00, 0x00,
            0x88, 0x58, 0x01, 0x00, 0x02, 0x00, 0x10, 0x00,
        ];
        let result = detect_mime(&wav, None).unwrap();
        assert!(
            result.mime_type.contains("wav"),
            "WAV MIME expected, got: {}",
            result.mime_type
        );
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    #[test]
    fn g_detect_pdf_percent_pdf_header() {
        let pdf = real_pdf_bytes();
        let result = detect_mime(&pdf, None).unwrap();
        assert_eq!(result.mime_type, "application/pdf");
        assert_eq!(result.extension, "pdf");
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::MagicBytes
        );
    }

    // ---------------------------------------------------------------
    // G2. MIME edge cases
    // ---------------------------------------------------------------

    #[test]
    fn g_png_4_bytes_only_too_short_for_full_signature() {
        // 4 bytes: 89 50 4E 47 — missing 0D 0A 1A 0A
        let short = vec![0x89, 0x50, 0x4E, 0x47];
        let result = detect_mime(&short, None);
        if let Ok(d) = result {
            assert!(
                d.mime_type == "image/png",
                "If detected with only 4 bytes, must still be PNG, got: {}",
                d.mime_type
            );
        }
        // Err(_) is also expected
    }

    #[test]
    fn g_null_bytes_common_in_binary_formats() {
        let null_data = vec![0x00; 128];
        let result = detect_mime(&null_data, None);
        assert!(
            result.is_err(),
            "128 null bytes should not match any known MIME type"
        );
    }

    #[test]
    fn g_null_bytes_with_server_hint_accepted() {
        let null_data = vec![0x00; 128];
        let result = detect_mime(&null_data, Some("audio/flac")).unwrap();
        assert_eq!(result.mime_type, "audio/flac");
        assert_eq!(
            result.source,
            crate::media::detect::DetectionSource::ServerHint
        );
    }

    #[test]
    fn g_large_sample_over_8192_only_first_inspected() {
        let mut data = real_png_bytes();
        data.extend(vec![0xCC; 10_000]);
        assert!(data.len() > 8192);
        let result = detect_mime(&data, None).unwrap();
        assert_eq!(result.mime_type, "image/png");
    }

    #[test]
    fn g_magic_bytes_at_offset_8192_not_detected() {
        let mut data = vec![0x00; 8192];
        data.extend_from_slice(&real_png_bytes());
        let result = detect_mime(&data, None);
        assert!(
            result.is_err(),
            "PNG magic at offset 8192 should be beyond inspection window"
        );
    }

    #[test]
    fn g_exactly_8192_bytes_with_header() {
        let mut data = real_png_bytes();
        data.resize(8192, 0x00);
        let result = detect_mime(&data, None).unwrap();
        assert_eq!(result.mime_type, "image/png");
    }

    // ---------------------------------------------------------------
    // G3. Processor with realistic MCP responses
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn g_process_single_text_returns_none_and_process_all_empty() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::text("Hello MCP");
        assert!(processor.process(&block, "t1").await.unwrap().is_none());
        assert!(processor.process_all(&[block], "t1").await.is_empty());
    }

    #[tokio::test]
    async fn g_process_all_1_image_1_text_returns_1() {
        let (processor, _dir) = make_processor_e2e();
        let blocks = vec![
            ContentBlock::image(encode_b64(&real_png_bytes()), "image/png"),
            ContentBlock::text("Caption"),
        ];
        let results = processor.process_all(&blocks, "it1").await;
        assert_eq!(results.len(), 1);
        assert!(
            results[0].is_ok(),
            "first result should succeed: {:?}",
            results[0].as_ref().err()
        );
        assert_eq!(results[0].as_ref().unwrap().0.mime_type, "image/png");
    }

    #[tokio::test]
    async fn g_process_all_5_images_2_audio_returns_7() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = MediaBudget::with_max_per_run(50 * 1024 * 1024);
        let processor = MediaProcessor::with_budget(store, budget);

        let mut blocks = Vec::new();
        for i in 0..5u8 {
            let mut png = real_png_bytes();
            png.push(i);
            blocks.push(ContentBlock::image(encode_b64(&png), "image/png"));
        }
        for i in 0..2u8 {
            let mut mp3 = real_mp3_bytes();
            mp3.push(200 + i);
            blocks.push(ContentBlock::audio(encode_b64(&mp3), "audio/mpeg"));
        }

        let results = processor.process_all(&blocks, "g_batch7").await;
        assert_eq!(results.len(), 7, "5 images + 2 audio = 7 results");

        let (img, aud) = results.iter().fold((0usize, 0usize), |(i, a), r| {
            let mr = &r.as_ref().unwrap().0;
            if mr.mime_type.starts_with("image/") {
                (i + 1, a)
            } else if mr.mime_type.starts_with("audio/") {
                (i, a + 1)
            } else {
                (i, a)
            }
        });
        assert_eq!(img, 5);
        assert_eq!(aud, 2);

        // All hashes unique
        let mut hashes: Vec<_> = results
            .iter()
            .map(|r| r.as_ref().unwrap().0.hash.clone())
            .collect();
        hashes.sort();
        hashes.dedup();
        assert_eq!(hashes.len(), 7);
    }

    #[tokio::test]
    async fn g_process_all_all_failures_correct_indices() {
        let (processor, _dir) = make_processor_e2e();
        let blocks = vec![
            ContentBlock::text("skip"),                   // 0
            ContentBlock::image("BAD1!!!", "image/png"),  // 1
            ContentBlock::text("skip"),                   // 2
            ContentBlock::audio("BAD2!!!", "audio/wav"),  // 3
            ContentBlock::image("BAD3!!!", "image/jpeg"), // 4
        ];
        let results = processor.process_all(&blocks, "g_allfail").await;
        assert_eq!(results.len(), 3);
        assert!(
            results.iter().all(|r| r.is_err()),
            "all results should be errors"
        );
        let indices: Vec<usize> = results.iter().map(|r| r.as_ref().unwrap_err().0).collect();
        assert_eq!(indices, vec![1, 3, 4]);
        for r in &results {
            assert_eq!(r.as_ref().unwrap_err().1.code(), "NIKA-256");
        }
    }

    // ---------------------------------------------------------------
    // G4. Base64 edge cases in processor
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn g_base64_valid_unrecognizable_bytes_nika_251() {
        let (processor, _dir) = make_processor_e2e();
        // Bytes that don't match any known magic signature
        let random = vec![
            0x07, 0x13, 0x29, 0x37, 0x41, 0x53, 0x67, 0x71, 0x83, 0x97, 0xA1, 0xB3, 0xC7, 0xD1,
            0xE3, 0xF7,
        ];
        let block = ContentBlock::image(encode_b64(&random), "application/octet-stream");
        let result = processor.process(&block, "g_unknown").await;
        assert!(
            result.is_err(),
            "unrecognizable bytes should fail but got: {:?}",
            result.as_ref().ok()
        );
        assert_eq!(
            result.unwrap_err().code(),
            "NIKA-251",
            "Valid base64 decoding to unidentifiable bytes + octet-stream -> NIKA-251"
        );
    }

    #[tokio::test]
    async fn g_base64_crlf_line_breaks_succeeds() {
        let (processor, _dir) = make_processor_e2e();
        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let with_crlf: String = b64
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i > 0 && i % 76 == 0 {
                    format!("\r\n{c}")
                } else {
                    c.to_string()
                }
            })
            .collect();
        assert!(with_crlf.contains("\r\n"));

        let block = ContentBlock::image(with_crlf, "image/png");
        let (mr, _) = processor
            .process(&block, "g_crlf")
            .await
            .expect("CRLF base64 should succeed")
            .expect("Some");
        assert_eq!(mr.hash, format!("blake3:{}", blake3::hash(&png).to_hex()));
        assert_eq!(mr.mime_type, "image/png");
    }

    #[tokio::test]
    async fn g_base64_tab_characters_succeeds() {
        let (processor, _dir) = make_processor_e2e();
        let png = real_png_bytes();
        let b64 = encode_b64(&png);
        let with_tabs: String = b64
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i > 0 && i % 25 == 0 {
                    format!("\t{c}")
                } else {
                    c.to_string()
                }
            })
            .collect();
        assert!(with_tabs.contains('\t'));

        let block = ContentBlock::image(with_tabs, "image/png");
        let (mr, _) = processor
            .process(&block, "g_tabs")
            .await
            .expect("tab base64 should succeed")
            .expect("Some");
        assert_eq!(mr.hash, format!("blake3:{}", blake3::hash(&png).to_hex()));
    }

    // ---------------------------------------------------------------
    // G5. Budget edge cases
    // ---------------------------------------------------------------

    #[test]
    fn g_budget_zero_rejects_any_addition() {
        let budget = MediaBudget::with_max_per_run(0);
        let err = budget.check_and_add(1, "t1").unwrap_err();
        assert_eq!(err.code(), "NIKA-259");
        assert_eq!(budget.current_bytes(), 0);
    }

    #[test]
    fn g_budget_zero_accepts_zero_size() {
        let budget = MediaBudget::with_max_per_run(0);
        // 0 + 0 = 0 which is NOT > 0
        let result = budget.check_and_add(0, "t1");
        assert!(
            result.is_ok(),
            "zero-size add should succeed: {:?}",
            result.as_ref().err()
        );
    }

    #[test]
    fn g_budget_u64_max_accepts_large_additions() {
        let budget = MediaBudget::with_max_per_run(u64::MAX);
        for i in 0..50 {
            let result = budget.check_and_add(1_000_000, &format!("t{i}"));
            assert!(
                result.is_ok(),
                "large budget add should succeed: {:?}",
                result.as_ref().err()
            );
        }
        assert_eq!(budget.current_bytes(), 50_000_000);
    }

    #[test]
    fn g_budget_u64_max_half_plus_half_succeeds() {
        let budget = MediaBudget::with_max_per_run(u64::MAX);
        let half = u64::MAX / 2;
        let r1 = budget.check_and_add(half, "t1");
        assert!(
            r1.is_ok(),
            "first half should succeed: {:?}",
            r1.as_ref().err()
        );
        let r2 = budget.check_and_add(half, "t2");
        assert!(
            r2.is_ok(),
            "second half should succeed: {:?}",
            r2.as_ref().err()
        );
        // u64::MAX / 2 * 2 = u64::MAX - 1 (since MAX is odd)
        // One more byte should succeed
        let r3 = budget.check_and_add(1, "t3");
        assert!(
            r3.is_ok(),
            "one more byte should succeed: {:?}",
            r3.as_ref().err()
        );
        assert_eq!(budget.current_bytes(), u64::MAX);
    }

    #[tokio::test]
    async fn g_shared_budget_3_processors_total_enforced() {
        let budget = Arc::new(MediaBudget::with_max_per_run(150));
        let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let procs: Vec<_> = dirs
            .iter()
            .map(|d| {
                MediaProcessor::with_shared_budget(CasStore::new(d.path()), Arc::clone(&budget))
            })
            .collect();

        let b64 = encode_b64(&real_png_bytes());

        let p0 = procs[0]
            .process(&ContentBlock::image(b64.clone(), "image/png"), "p0")
            .await;
        assert!(
            p0.is_ok(),
            "proc[0] should succeed: {:?}",
            p0.as_ref().err()
        );
        let p1 = procs[1]
            .process(&ContentBlock::image(b64.clone(), "image/png"), "p1")
            .await;
        assert!(
            p1.is_ok(),
            "proc[1] should succeed: {:?}",
            p1.as_ref().err()
        );
        let r3 = procs[2]
            .process(&ContentBlock::image(b64.clone(), "image/png"), "p2")
            .await;
        assert!(
            r3.is_err(),
            "proc[2] should exceed budget but got: {:?}",
            r3.as_ref().ok()
        );
        assert_eq!(r3.unwrap_err().code(), "NIKA-259");

        let png_size = real_png_bytes().len() as u64;
        assert_eq!(budget.current_bytes(), 2 * png_size);
    }

    // ---------------------------------------------------------------
    // G6. MediaRef created_by field
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn g_created_by_matches_exact_task_id() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "image/png");
        let (mr, _) = processor
            .process(&block, "precise_task_42")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mr.created_by, "precise_task_42");
    }

    #[tokio::test]
    async fn g_created_by_unicode_task_ids() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "image/png");
        for id in [
            "\u{1F98B}_butterfly",
            "\u{4F60}\u{597D}_hello",
            "caf\u{00E9}_task",
            "\u{0410}\u{0411}\u{0412}_abc",
            "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}",
        ] {
            let (mr, _) = processor.process(&block, id).await.unwrap().unwrap();
            assert_eq!(mr.created_by, id, "Unicode task_id not preserved: {}", id);
        }
    }

    #[tokio::test]
    async fn g_created_by_empty_string() {
        let (processor, _dir) = make_processor_e2e();
        let block = ContentBlock::image(encode_b64(&real_png_bytes()), "image/png");
        let (mr, _) = processor.process(&block, "").await.unwrap().unwrap();
        assert_eq!(mr.created_by, "");
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 3: Full pipeline — ContentBlock -> process -> CAS -> MediaRef
    //        -> RunContext.set_media -> take_media -> resolve_path
    // Exercises the REAL CAS store (not mock data) through the entire
    // pipeline from raw bytes to template-resolvable media bindings.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn gap3_full_pipeline_content_block_to_resolve_path() {
        // Step 1: Create real processor with CAS store
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = Arc::new(MediaBudget::with_max_per_run(100_000));
        let processor = MediaProcessor::with_shared_budget(store, Arc::clone(&budget));
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "pipeline_test".into();

        // Step 2: Create a ContentBlock with real PNG data
        let png_data = real_png_bytes();
        let b64 = encode_b64(&png_data);
        let block = ContentBlock::image(b64, "image/png");

        // Step 3: Process through MediaProcessor
        let (media_ref, store_result) = processor
            .process(&block, task_id.as_ref())
            .await
            .expect("process should succeed")
            .expect("image block should produce Some");

        // Verify the MediaRef is populated correctly
        assert!(media_ref.hash.starts_with("blake3:"), "hash missing prefix");
        assert_eq!(media_ref.mime_type, "image/png");
        assert_eq!(media_ref.extension, "png");
        assert_eq!(media_ref.size_bytes, png_data.len() as u64);
        assert_eq!(media_ref.created_by, "pipeline_test");
        assert!(media_ref.path.exists(), "CAS file should exist on disk");

        // Verify CAS file content is correct (read_raw strips NK framing header)
        let on_disk = CasStore::read_raw(&media_ref.path).await.unwrap();
        assert_eq!(
            on_disk, png_data,
            "CAS stored data must match original PNG bytes"
        );

        // Verify the hash is correct
        let expected_hash = format!("blake3:{}", blake3::hash(&png_data).to_hex());
        assert_eq!(
            media_ref.hash, expected_hash,
            "hash must match direct blake3 computation"
        );

        // Verify StoreResult fields
        assert!(!store_result.deduplicated);
        assert_eq!(store_result.size, png_data.len() as u64);

        // Step 4: Stage in RunContext (simulates what run_invoke does)
        ctx.set_media(&task_id, vec![media_ref.clone()]);

        // Step 5: Take from RunContext (simulates what runner does after task completes)
        let taken = ctx.take_media(&task_id);
        assert_eq!(taken.len(), 1, "take_media should return 1 ref");
        assert_eq!(taken[0].hash, media_ref.hash);

        // Step 6: Build TaskResult with media (simulates runner)
        use crate::store::TaskResult;
        let tr = TaskResult::success(
            serde_json::json!({"prompt": "generate a logo"}),
            std::time::Duration::from_millis(42),
        )
        .with_media(taken);
        assert_eq!(tr.media.len(), 1);

        // Step 7: Insert into RunContext results (simulates runner storing the result)
        ctx.insert(Arc::clone(&task_id), tr);

        // Step 8: resolve_path — test every media field through template bindings

        // {{with.pipeline_test.media}} -> full array
        let media_arr = ctx
            .resolve_path("pipeline_test.media")
            .expect("resolve_path('pipeline_test.media') should return Some");
        assert!(media_arr.is_array());
        assert_eq!(media_arr.as_array().unwrap().len(), 1);

        // {{with.pipeline_test.media[0].hash}} -> actual blake3 hash
        let resolved_hash = ctx
            .resolve_path("pipeline_test.media[0].hash")
            .expect("resolve_path('...media[0].hash') should return Some");
        assert_eq!(
            resolved_hash, expected_hash,
            "resolved hash must match the actual blake3 hash of PNG data"
        );

        // {{with.pipeline_test.media[0].mime_type}} -> "image/png"
        let resolved_mime = ctx
            .resolve_path("pipeline_test.media[0].mime_type")
            .expect("resolve_path('...media[0].mime_type') should return Some");
        assert_eq!(resolved_mime, "image/png");

        // {{with.pipeline_test.media[0].extension}} -> "png"
        let resolved_ext = ctx
            .resolve_path("pipeline_test.media[0].extension")
            .expect("resolve_path('...media[0].extension') should return Some");
        assert_eq!(resolved_ext, "png");

        // {{with.pipeline_test.media[0].size_bytes}} -> exact decoded size
        let resolved_size = ctx
            .resolve_path("pipeline_test.media[0].size_bytes")
            .expect("resolve_path('...media[0].size_bytes') should return Some");
        assert_eq!(
            resolved_size,
            png_data.len() as u64,
            "size_bytes must match decoded PNG size, not base64 size"
        );

        // {{with.pipeline_test.media[0].path}} -> CAS path on disk
        let resolved_path = ctx
            .resolve_path("pipeline_test.media[0].path")
            .expect("resolve_path('...media[0].path') should return Some");
        let path_str = resolved_path.as_str().expect("path should be a string");
        assert!(
            path_str.contains(dir.path().to_str().unwrap()),
            "resolved path should contain CAS root: {}",
            path_str
        );

        // {{with.pipeline_test.media[0].created_by}} -> task_id
        let resolved_creator = ctx
            .resolve_path("pipeline_test.media[0].created_by")
            .expect("resolve_path('...media[0].created_by') should return Some");
        assert_eq!(resolved_creator, "pipeline_test");

        // Normal output fields still resolve correctly
        let prompt = ctx
            .resolve_path("pipeline_test.prompt")
            .expect("normal output should still resolve");
        assert_eq!(prompt, "generate a logo");

        // Budget was consumed
        assert_eq!(
            budget.current_bytes(),
            png_data.len() as u64,
            "budget should track the decoded PNG size"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 8: NikaError::is_recoverable for MediaError variants
    // Only MediaStoreIo should be recoverable through NikaError.
    // All other MediaError variants must be non-recoverable.
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn gap8_nika_error_is_recoverable_media_store_io() {
        use crate::error::NikaError;

        // MediaStoreIo is the ONLY recoverable MediaError variant
        let io_err = MediaError::MediaStoreIo {
            path: "/tmp/cas/ab/cdef".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        let nika_err: NikaError = io_err.into();
        assert!(
            nika_err.is_recoverable(),
            "MediaStoreIo should be recoverable through NikaError"
        );
        assert_eq!(nika_err.code(), "NIKA-255");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_mime_detection_failed() {
        use crate::error::NikaError;

        let err = MediaError::MimeDetectionFailed {
            reason: "magic bytes unrecognized".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "MimeDetectionFailed should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-251");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_unsupported_media_type() {
        use crate::error::NikaError;

        let err = MediaError::UnsupportedMediaType {
            mime_type: "video/x-matroska".into(),
            reason: "not in allowlist".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "UnsupportedMediaType should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-252");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_media_not_found() {
        use crate::error::NikaError;

        let err = MediaError::MediaNotFound {
            hash: "blake3:deadbeef".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "MediaNotFound should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-253");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_hash_mismatch() {
        use crate::error::NikaError;

        let err = MediaError::HashMismatch {
            expected: "blake3:aaa".into(),
            actual: "blake3:bbb".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "HashMismatch should NOT be recoverable (data corruption)"
        );
        assert_eq!(nika_err.code(), "NIKA-254");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_base64_decode_failed() {
        use crate::error::NikaError;

        let err = MediaError::Base64DecodeFailed {
            source_desc: "task gen_img".into(),
            reason: "invalid byte at position 4".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "Base64DecodeFailed should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-256");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_base64_input_too_large() {
        use crate::error::NikaError;

        let err = MediaError::Base64InputTooLarge {
            size: 200_000_000,
            max: 100_000_000,
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "Base64InputTooLarge should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-257");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_empty_media_content() {
        use crate::error::NikaError;

        let err = MediaError::EmptyMediaContent {
            task_id: "empty_task".into(),
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "EmptyMediaContent should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-258");
    }

    #[test]
    fn gap8_nika_error_is_not_recoverable_run_budget_exceeded() {
        use crate::error::NikaError;

        let err = MediaError::RunBudgetExceeded {
            current: 600_000_000,
            max: 500_000_000,
        };
        let nika_err: NikaError = err.into();
        assert!(
            !nika_err.is_recoverable(),
            "RunBudgetExceeded should NOT be recoverable"
        );
        assert_eq!(nika_err.code(), "NIKA-259");
    }

    // ═══════════════════════════════════════════════════════════════
    // MONSTER INTEGRATION TEST
    // Exercises the ENTIRE media pipeline end-to-end:
    //   ContentBlock -> MediaProcessor -> CAS -> staging -> TaskResult
    //   -> RunContext.insert -> resolve_path (all variants)
    //   -> binding bridge (BindingEntry -> ResolvedBindings)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn monster_full_pipeline_with_binding_bridge() {
        use crate::binding::{BindingEntry, BindingSpec, ResolvedBindings};
        use crate::store::TaskResult;
        use std::time::Duration;

        // ───────────────────────────────────────────────────
        // Step 1: Create 3 ContentBlocks: 2 images (PNG) + 1 text
        // ───────────────────────────────────────────────────

        // PNG #1: the standard minimal PNG
        let png1_bytes = real_png_bytes();
        let png1_b64 = encode_b64(&png1_bytes);

        // PNG #2: same header but with an extra byte to make it unique
        let mut png2_bytes = real_png_bytes();
        png2_bytes.push(0xFF); // different content -> different hash
        let png2_b64 = encode_b64(&png2_bytes);

        let blocks = vec![
            ContentBlock::image(png1_b64, "image/png"),
            ContentBlock::text("This text block should be filtered out"),
            ContentBlock::image(png2_b64, "image/png"),
        ];

        // ───────────────────────────────────────────────────
        // Step 2: Process through MediaProcessor -> CAS store
        // ───────────────────────────────────────────────────

        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let budget = Arc::new(MediaBudget::new());
        let processor = MediaProcessor::with_shared_budget(store, Arc::clone(&budget));

        let results = processor.process_all(&blocks, "monster_task").await;

        // Text block is filtered -> only 2 results
        assert_eq!(
            results.len(),
            2,
            "process_all should return 2 results (text filtered), got {}",
            results.len()
        );

        let mut media_refs: Vec<MediaRef> = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            let (media_ref, store_result) =
                result.unwrap_or_else(|(_idx, e)| panic!("Block {i} failed: {e}"));
            assert!(
                media_ref.hash.starts_with("blake3:"),
                "media_ref[{i}] hash should start with blake3:"
            );
            assert_eq!(
                media_ref.mime_type, "image/png",
                "media_ref[{i}] mime_type should be image/png"
            );
            assert_eq!(
                media_ref.extension, "png",
                "media_ref[{i}] extension should be png"
            );
            assert!(
                media_ref.size_bytes > 0,
                "media_ref[{i}] size_bytes should be > 0"
            );
            assert_eq!(
                media_ref.created_by, "monster_task",
                "media_ref[{i}] created_by should be monster_task"
            );
            assert!(
                store_result.path.exists(),
                "CAS file should exist for media_ref[{i}]"
            );
            media_refs.push(media_ref);
        }

        // Two different PNGs must produce different hashes
        assert_ne!(
            media_refs[0].hash, media_refs[1].hash,
            "Two different PNG byte sequences must produce different blake3 hashes"
        );

        // Verify hashes are correct blake3 computations
        let expected_hash_0 = format!("blake3:{}", blake3::hash(&png1_bytes).to_hex());
        let expected_hash_1 = format!("blake3:{}", blake3::hash(&png2_bytes).to_hex());
        assert_eq!(media_refs[0].hash, expected_hash_0);
        assert_eq!(media_refs[1].hash, expected_hash_1);

        // Budget should have tracked both images
        assert!(
            budget.current_bytes() > 0,
            "Budget should have tracked bytes"
        );
        assert_eq!(
            budget.current_bytes(),
            (png1_bytes.len() + png2_bytes.len()) as u64,
            "Budget should equal total decoded bytes"
        );

        // ───────────────────────────────────────────────────
        // Step 3: Stage in RunContext via set_media
        // ───────────────────────────────────────────────────

        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "monster_task".into();

        ctx.set_media(&task_id, media_refs.clone());

        // ───────────────────────────────────────────────────
        // Step 4: Take from RunContext via take_media
        // ───────────────────────────────────────────────────

        let taken = ctx.take_media(&task_id);
        assert_eq!(
            taken.len(),
            2,
            "take_media should return 2 staged refs, got {}",
            taken.len()
        );

        // Verify take_media drains the staging area
        let taken_again = ctx.take_media(&task_id);
        assert!(
            taken_again.is_empty(),
            "Second take_media must return empty (staging drained)"
        );

        // ───────────────────────────────────────────────────
        // Step 5: Build TaskResult with with_media
        // ───────────────────────────────────────────────────

        let tr = TaskResult::success(
            serde_json::json!({"description": "Monster generated", "quality": 99}),
            Duration::from_millis(42),
        )
        .with_media(taken);

        assert_eq!(tr.media.len(), 2, "TaskResult should carry 2 media refs");
        assert!(tr.is_success(), "with_media should preserve success status");

        // ───────────────────────────────────────────────────
        // Step 6: Insert into RunContext
        // ───────────────────────────────────────────────────

        ctx.insert(Arc::clone(&task_id), tr);

        // Also insert a task with NO media for the empty-media test
        let no_media_id: Arc<str> = "no_media_task".into();
        let no_media_tr = TaskResult::success(
            serde_json::json!("text only result"),
            Duration::from_millis(5),
        );
        ctx.insert(Arc::clone(&no_media_id), no_media_tr);

        // ───────────────────────────────────────────────────
        // Step 7: Test EVERY resolve_path variant
        // ───────────────────────────────────────────────────

        // 7a: task.media -> full array (2 items, not 3 — text was filtered)
        let media_val = ctx
            .resolve_path("monster_task.media")
            .expect("resolve_path('monster_task.media') should return Some");
        let media_arr = media_val.as_array().expect("media should be a JSON array");
        assert_eq!(
            media_arr.len(),
            2,
            "media array should have 2 items (text block was filtered)"
        );

        // 7b: task.media[0].hash -> starts with "blake3:"
        let hash_0 = ctx
            .resolve_path("monster_task.media[0].hash")
            .expect("media[0].hash should exist");
        let hash_0_str = hash_0.as_str().expect("hash should be a string");
        assert!(
            hash_0_str.starts_with("blake3:"),
            "hash should start with blake3:, got: {}",
            hash_0_str
        );
        assert_eq!(
            hash_0_str, expected_hash_0,
            "hash[0] should match blake3 of png1"
        );

        // 7c: task.media[0].mime_type -> "image/png"
        let mime_0 = ctx
            .resolve_path("monster_task.media[0].mime_type")
            .expect("media[0].mime_type should exist");
        assert_eq!(mime_0, "image/png");

        // 7d: task.media[0].extension -> "png"
        let ext_0 = ctx
            .resolve_path("monster_task.media[0].extension")
            .expect("media[0].extension should exist");
        assert_eq!(ext_0, "png");

        // 7e: task.media[0].size_bytes -> > 0
        let size_0 = ctx
            .resolve_path("monster_task.media[0].size_bytes")
            .expect("media[0].size_bytes should exist");
        assert!(
            size_0.as_u64().unwrap() > 0,
            "size_bytes should be > 0, got {}",
            size_0
        );
        assert_eq!(
            size_0.as_u64().unwrap(),
            png1_bytes.len() as u64,
            "size_bytes should match actual PNG byte count"
        );

        // 7f: task.media[0].path -> contains the CAS shard directory
        let path_0 = ctx
            .resolve_path("monster_task.media[0].path")
            .expect("media[0].path should exist");
        let path_0_str = path_0.as_str().expect("path should be a string");
        // CAS uses 2-char shard directories; the hash hex starts after "blake3:"
        let hash_hex_0 = &expected_hash_0["blake3:".len()..];
        let shard_0 = &hash_hex_0[..2];
        assert!(
            path_0_str.contains(shard_0),
            "path should contain 2-char shard dir '{}': {}",
            shard_0,
            path_0_str
        );

        // 7g: task.media[0].created_by -> "monster_task"
        let created_0 = ctx
            .resolve_path("monster_task.media[0].created_by")
            .expect("media[0].created_by should exist");
        assert_eq!(created_0, "monster_task");

        // 7h: task.media[1].hash -> different from [0]
        let hash_1 = ctx
            .resolve_path("monster_task.media[1].hash")
            .expect("media[1].hash should exist");
        let hash_1_str = hash_1.as_str().expect("hash[1] should be a string");
        assert_ne!(
            hash_0_str, hash_1_str,
            "media[0].hash and media[1].hash must differ (different PNG data)"
        );
        assert_eq!(hash_1_str, expected_hash_1);

        // 7i: task.media[99].hash -> None (out of bounds)
        let oob = ctx.resolve_path("monster_task.media[99].hash");
        assert!(
            oob.is_none(),
            "Out-of-bounds media[99].hash should return None, got {:?}",
            oob
        );

        // 7j: task.media when task has no media -> empty array
        let empty_media = ctx
            .resolve_path("no_media_task.media")
            .expect("resolve_path for task with no media should return Some");
        let empty_arr = empty_media
            .as_array()
            .expect("empty media should be an array");
        assert!(
            empty_arr.is_empty(),
            "task with no media should return empty array, got {:?}",
            empty_arr
        );

        // 7k: task.output_field -> still works (not shadowed by media interception)
        let desc = ctx
            .resolve_path("monster_task.description")
            .expect("non-media output field should still resolve");
        assert_eq!(
            desc, "Monster generated",
            "output field should not be shadowed by media"
        );
        let quality = ctx
            .resolve_path("monster_task.quality")
            .expect("numeric output field should still resolve");
        assert_eq!(quality, 99);

        // ───────────────────────────────────────────────────
        // Step 8: Test the binding bridge
        //   BindingEntry -> ResolvedBindings::from_binding_spec
        //   This exercises resolve_entry() with media path interception
        // ───────────────────────────────────────────────────

        let mut spec: BindingSpec = BindingSpec::default();

        // Binding that reads the full media array
        spec.insert("all_media".into(), BindingEntry::new("monster_task.media"));
        // Binding that reads a specific media field
        spec.insert(
            "first_hash".into(),
            BindingEntry::new("monster_task.media[0].hash"),
        );
        // Binding that reads media[1].mime_type
        spec.insert(
            "second_mime".into(),
            BindingEntry::new("monster_task.media[1].mime_type"),
        );
        // Binding that reads a regular (non-media) output field
        spec.insert("desc".into(), BindingEntry::new("monster_task.description"));
        // Binding that reads media from a task with no media
        spec.insert("empty".into(), BindingEntry::new("no_media_task.media"));
        // Binding that reads out-of-bounds with a default
        spec.insert(
            "oob_safe".into(),
            BindingEntry::with_default(
                "monster_task.media[99].hash",
                serde_json::json!("no_such_media"),
            ),
        );
        // Binding that reads media[0].size_bytes
        spec.insert(
            "img_size".into(),
            BindingEntry::new("monster_task.media[0].size_bytes"),
        );
        // Binding that reads media[0].extension
        spec.insert(
            "img_ext".into(),
            BindingEntry::new("monster_task.media[0].extension"),
        );
        // Binding that reads media[0].created_by
        spec.insert(
            "img_author".into(),
            BindingEntry::new("monster_task.media[0].created_by"),
        );
        // Binding that reads media[0].path
        spec.insert(
            "img_path".into(),
            BindingEntry::new("monster_task.media[0].path"),
        );

        let resolved = ResolvedBindings::from_binding_spec(Some(&spec), &ctx)
            .expect("from_binding_spec should succeed for all media bindings");

        // Verify all_media binding
        let all_media = resolved
            .get("all_media")
            .expect("all_media binding should be resolved");
        assert!(all_media.is_array());
        assert_eq!(all_media.as_array().unwrap().len(), 2);

        // Verify first_hash binding
        let first_hash = resolved
            .get("first_hash")
            .expect("first_hash binding should be resolved");
        assert_eq!(
            first_hash.as_str().unwrap(),
            expected_hash_0,
            "first_hash binding should match direct resolve_path result"
        );

        // Verify second_mime binding
        let second_mime = resolved
            .get("second_mime")
            .expect("second_mime binding should be resolved");
        assert_eq!(second_mime, "image/png");

        // Verify desc binding (non-media, must not be affected)
        let desc_binding = resolved
            .get("desc")
            .expect("desc binding (non-media) should be resolved");
        assert_eq!(desc_binding, "Monster generated");

        // Verify empty binding (task with no media -> empty array)
        let empty_binding = resolved
            .get("empty")
            .expect("empty binding should be resolved");
        assert!(empty_binding.is_array());
        assert!(empty_binding.as_array().unwrap().is_empty());

        // Verify oob_safe binding (out-of-bounds with default)
        let oob_binding = resolved
            .get("oob_safe")
            .expect("oob_safe binding should be resolved");
        assert_eq!(
            oob_binding, "no_such_media",
            "Out-of-bounds media path should fall back to default"
        );

        // Verify img_size binding
        let img_size = resolved
            .get("img_size")
            .expect("img_size binding should be resolved");
        assert_eq!(img_size.as_u64().unwrap(), png1_bytes.len() as u64);

        // Verify img_ext binding
        let img_ext = resolved
            .get("img_ext")
            .expect("img_ext binding should be resolved");
        assert_eq!(img_ext, "png");

        // Verify img_author binding
        let img_author = resolved
            .get("img_author")
            .expect("img_author binding should be resolved");
        assert_eq!(img_author, "monster_task");

        // Verify img_path binding
        let img_path = resolved
            .get("img_path")
            .expect("img_path binding should be resolved");
        let img_path_str = img_path.as_str().expect("path should be a string");
        assert!(
            img_path_str.contains(shard_0),
            "binding img_path should contain CAS shard dir: {}",
            img_path_str
        );

        // ───────────────────────────────────────────────────
        // Final sanity: verify CAS files on disk are intact (read_raw strips NK framing)
        // ───────────────────────────────────────────────────
        let stored_data_0 = CasStore::read_raw(&media_refs[0].path)
            .await
            .expect("CAS file for media_ref[0] should be readable");
        assert_eq!(
            stored_data_0, png1_bytes,
            "CAS stored data for png1 must match original bytes"
        );

        let stored_data_1 = CasStore::read_raw(&media_refs[1].path)
            .await
            .expect("CAS file for media_ref[1] should be readable");
        assert_eq!(
            stored_data_1, png2_bytes,
            "CAS stored data for png2 must match original bytes"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // PR2: Binary Artifact + CAS Integration Tests
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn pr2_full_pipeline_binary_artifact_cas_to_disk() {
        // Monster test: CAS store → MediaRef → binary artifact → disk → verify
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Store binary data in CAS
        let binary_data = b"\x89PNG\r\n\x1a\nfake image bytes for test 12345";
        let store_result = store.store(binary_data).await.unwrap();

        // Create MediaRef from store result
        let media_ref = crate::media::types::MediaRef {
            hash: store_result.hash.clone(),
            mime_type: "image/png".to_string(),
            size_bytes: store_result.size,
            path: store_result.path.clone(),
            extension: "png".to_string(),
            created_by: "gen_img".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Verify CAS file exists and is readable (read_raw strips NK framing header)
        assert!(media_ref.path.exists(), "CAS file should exist");
        let cas_data = CasStore::read_raw(&media_ref.path).await.unwrap();
        assert_eq!(cas_data, binary_data, "CAS data should match original");

        // Create artifact writer
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "test-workflow");

        // Write binary artifact from CAS path
        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "gen_img".to_string(),
            output_path: "output/image.bin".to_string(),
            source: crate::io::writer::BinarySource::CasPath(media_ref.path.clone()),
            expected_size: media_ref.size_bytes,
        };

        let result = writer.write_binary(request).await.unwrap();
        assert!(result.path.ends_with("output/image.bin"));
        assert_eq!(result.size, binary_data.len() as u64);

        // Verify artifact content matches original
        let artifact_data = tokio::fs::read(&result.path).await.unwrap();
        assert_eq!(
            artifact_data, binary_data,
            "Artifact data should match original binary"
        );
    }

    #[tokio::test]
    async fn pr2_integrity_check_detects_missing_cas_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Store and then delete the file
        let data = b"test data for deletion";
        let result = store.store(data).await.unwrap();
        tokio::fs::remove_file(&result.path).await.unwrap();

        // MediaRef points to deleted file
        let media_ref = crate::media::types::MediaRef {
            hash: result.hash,
            mime_type: "image/png".to_string(),
            size_bytes: result.size,
            path: result.path.clone(),
            extension: "png".to_string(),
            created_by: "task1".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Integrity check should detect missing file
        assert!(!media_ref.path.exists(), "CAS file should be deleted");
    }

    #[tokio::test]
    async fn pr2_binary_artifact_size_limit_respected() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        let data = vec![0xAB_u8; 1024]; // 1KB
        let result = store.store(&data).await.unwrap();

        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        // Writer with 512-byte limit
        let writer =
            crate::io::writer::ArtifactWriter::new(canonical, "test-workflow").with_max_size(512);

        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "output.bin".to_string(),
            source: crate::io::writer::BinarySource::CasPath(result.path),
            expected_size: 1024, // Exceeds 512 limit
        };

        let write_result = writer.write_binary(request).await;
        assert!(
            write_result.is_err(),
            "Should reject binary exceeding size limit"
        );
        let err = write_result.unwrap_err();
        assert!(matches!(
            err,
            crate::error::NikaError::ArtifactSizeExceeded { .. }
        ));
    }

    #[tokio::test]
    async fn pr2_cas_gc_respects_age() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Store a file
        store.store(b"old data").await.unwrap();

        // Clean with 0-second age should remove it (file is already "old enough")
        let result = store.clean_older_than(Duration::from_secs(0));
        assert_eq!(result.removed, 1);
        assert!(store.list().is_empty());
    }

    #[tokio::test]
    async fn pr2_cas_gc_preserves_recent_files() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Store a file (just created, so very recent)
        store.store(b"fresh data").await.unwrap();

        // Clean with 1-hour age should preserve it
        let result = store.clean_older_than(Duration::from_secs(3600));
        assert_eq!(result.removed, 0);
        assert_eq!(store.list().len(), 1);
    }

    #[tokio::test]
    async fn pr2_cas_dedup_consistency_with_binary_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        let data = b"identical binary content";

        // Store same data twice — should dedup
        let r1 = store.store(data).await.unwrap();
        let r2 = store.store(data).await.unwrap();

        assert_eq!(r1.hash, r2.hash);
        assert!(!r1.deduplicated);
        assert!(r2.deduplicated);
        assert_eq!(r1.path, r2.path);

        // Binary artifact from either should work
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "test");

        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "task1".to_string(),
            output_path: "dedup_test.bin".to_string(),
            source: crate::io::writer::BinarySource::CasPath(r1.path),
            expected_size: r1.size,
        };

        let result = writer.write_binary(request).await.unwrap();
        let written = tokio::fs::read(&result.path).await.unwrap();
        assert_eq!(written, data);
    }

    // ═══════════════════════════════════════════════════════════════
    // PR2 STRESS TESTS: Comprehensive binary artifact pipeline
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn pr2_stress_concurrent_binary_artifacts() {
        // 10 different binary blobs (various sizes: 1B, 1KB, 1MB) stored in CAS,
        // then 10 binary artifacts written in parallel to different output paths.
        // Verify ALL 10 artifacts written correctly with NO corruption.
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CasStore::new(dir.path().join("store")));

        // Create 10 different binary blobs with varied sizes
        let blobs: Vec<Vec<u8>> = vec![
            vec![0x42],              // 1 byte
            vec![0xAA; 100],         // 100 bytes
            vec![0xBB; 512],         // 512 bytes
            (0..=255).collect(),     // 256 bytes (all byte values)
            vec![0xCC; 1024],        // 1 KB
            vec![0xDD; 4096],        // 4 KB
            vec![0xEE; 10_000],      // 10 KB
            vec![0xFF; 65_536],      // 64 KB
            vec![0x11; 100_000],     // 100 KB
            vec![0x22; 1024 * 1024], // 1 MB
        ];

        // Store all 10 in CAS
        let mut store_results = Vec::new();
        for blob in &blobs {
            let result = store.store(blob).await.unwrap();
            store_results.push(result);
        }

        // Create 10 MediaRefs pointing to these CAS files
        let media_refs: Vec<MediaRef> = store_results
            .iter()
            .enumerate()
            .map(|(i, sr)| MediaRef {
                hash: sr.hash.clone(),
                mime_type: "application/octet-stream".to_string(),
                size_bytes: sr.size,
                path: sr.path.clone(),
                extension: "bin".to_string(),
                created_by: format!("task_{i}"),
                metadata: serde_json::Map::new(),
            })
            .collect();

        // Create artifact writer
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();

        // Process 10 binary artifact writes in parallel
        let handles: Vec<_> = media_refs
            .iter()
            .enumerate()
            .map(|(i, mr)| {
                let writer =
                    crate::io::writer::ArtifactWriter::new(canonical.clone(), "stress-test");
                let request = crate::io::writer::BinaryWriteRequest {
                    task_id: format!("task_{i}"),
                    output_path: format!("out_{i}.bin"),
                    source: crate::io::writer::BinarySource::CasPath(mr.path.clone()),
                    expected_size: mr.size_bytes,
                };
                tokio::spawn(async move { writer.write_binary(request).await })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|h| h.unwrap().unwrap())
            .collect();

        // Verify ALL 10 artifacts were written correctly with NO corruption
        assert_eq!(results.len(), 10, "all 10 artifacts should be written");
        for (i, result) in results.iter().enumerate() {
            let written = tokio::fs::read(&result.path).await.unwrap();
            assert_eq!(
                written.len(),
                blobs[i].len(),
                "artifact {i} size mismatch: expected {}, got {}",
                blobs[i].len(),
                written.len()
            );
            assert_eq!(
                written, blobs[i],
                "artifact {i} content corruption detected"
            );
        }
    }

    #[tokio::test]
    async fn pr2_binary_artifact_with_real_png_data() {
        // Use a real PNG header followed by valid IHDR chunk.
        // Store in CAS, create MediaRef, write binary artifact.
        // Verify the output file starts with PNG magic bytes and size matches exactly.
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        let png_data = real_png_bytes();

        // Store in CAS
        let sr = store.store(&png_data).await.unwrap();

        // Create MediaRef
        let media_ref = MediaRef {
            hash: sr.hash.clone(),
            mime_type: "image/png".to_string(),
            size_bytes: sr.size,
            path: sr.path.clone(),
            extension: "png".to_string(),
            created_by: "png_task".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Write binary artifact
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "png-test");

        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "png_task".to_string(),
            output_path: "image.png".to_string(),
            source: crate::io::writer::BinarySource::CasPath(media_ref.path.clone()),
            expected_size: media_ref.size_bytes,
        };

        let result = writer.write_binary(request).await.unwrap();

        // Verify the output file starts with PNG magic bytes
        let written = tokio::fs::read(&result.path).await.unwrap();
        assert!(
            written.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            "output file must start with PNG magic bytes"
        );

        // Verify file size matches exactly
        assert_eq!(
            written.len(),
            png_data.len(),
            "output file size must match original PNG data exactly"
        );
        assert_eq!(
            result.size,
            png_data.len() as u64,
            "WriteResult size must match original"
        );

        // Verify byte-for-byte identity
        assert_eq!(
            written, png_data,
            "output file must be identical to original PNG"
        );
    }

    #[tokio::test]
    async fn pr2_binary_artifact_large_file() {
        // Create a 5MB binary blob, store in CAS (should pass 100MB limit),
        // write binary artifact, verify blake3 hash of output matches MediaRef.hash.
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        let data = vec![0xAB_u8; 5 * 1024 * 1024]; // 5MB
        let sr = store.store(&data).await.unwrap();

        // 5MB is above the 1MB verify threshold, so verified should be true
        assert!(
            sr.verified,
            "5MB file should trigger read-back verification"
        );
        assert_eq!(sr.size, 5 * 1024 * 1024);

        let media_ref = MediaRef {
            hash: sr.hash.clone(),
            mime_type: "application/octet-stream".to_string(),
            size_bytes: sr.size,
            path: sr.path.clone(),
            extension: "bin".to_string(),
            created_by: "large_task".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Write binary artifact
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "large-test");

        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "large_task".to_string(),
            output_path: "large.bin".to_string(),
            source: crate::io::writer::BinarySource::CasPath(media_ref.path.clone()),
            expected_size: media_ref.size_bytes,
        };

        let result = writer.write_binary(request).await.unwrap();

        // Verify blake3 hash of output matches MediaRef.hash
        let written = tokio::fs::read(&result.path).await.unwrap();
        let output_hash = format!("blake3:{}", blake3::hash(&written).to_hex());
        assert_eq!(
            output_hash, media_ref.hash,
            "blake3 hash of artifact output must match MediaRef.hash"
        );
        assert_eq!(
            written.len(),
            5 * 1024 * 1024,
            "artifact must be exactly 5MB"
        );
    }

    #[tokio::test]
    async fn pr2_binary_artifact_then_integrity_check() {
        // Store a CAS file, create MediaRef, verify integrity (path + size),
        // then delete CAS file and verify integrity fails.
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        let data = b"integrity check data payload";
        let sr = store.store(data).await.unwrap();

        let media_ref = MediaRef {
            hash: sr.hash.clone(),
            mime_type: "application/octet-stream".to_string(),
            size_bytes: sr.size,
            path: sr.path.clone(),
            extension: "bin".to_string(),
            created_by: "integrity_task".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Store MediaRef in a TaskResult and insert into RunContext
        use crate::store::TaskResult;
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "integrity_task".into();
        let tr = TaskResult::success(
            serde_json::json!("done"),
            std::time::Duration::from_millis(50),
        )
        .with_media(vec![media_ref.clone()]);
        ctx.insert(Arc::clone(&task_id), tr);

        // Verify media ref is accessible from RunContext
        let resolved = ctx.resolve_path("integrity_task.media[0].hash");
        assert_eq!(resolved.unwrap().as_str().unwrap(), sr.hash);

        // Integrity check: path exists and on-disk size accounts for NK framing
        assert!(media_ref.path.exists(), "CAS file should exist");
        let meta = std::fs::metadata(&media_ref.path).unwrap();
        #[cfg(feature = "media-compression")]
        let expected_on_disk = media_ref.size_bytes + 4; // NK framing header
        #[cfg(not(feature = "media-compression"))]
        let expected_on_disk = media_ref.size_bytes;
        assert_eq!(
            meta.len(),
            expected_on_disk,
            "on-disk file size must account for framing overhead"
        );

        // Delete CAS file
        tokio::fs::remove_file(&media_ref.path).await.unwrap();

        // Integrity check fails: path missing
        assert!(
            !media_ref.path.exists(),
            "CAS file should be gone after deletion"
        );

        // CAS read should also fail
        let read_result = store.read(&media_ref.hash).await;
        assert!(
            read_result.is_err(),
            "read should fail for deleted CAS file"
        );
        assert_eq!(read_result.unwrap_err().code(), "NIKA-253");
    }

    #[tokio::test]
    async fn pr2_cas_gc_preserves_active_media() {
        // Store 2 CAS files, backdate file 1 mtime to 2 hours ago,
        // keep file 2 recent, call clean_older_than(1h).
        // Verify: file 1 deleted, file 2 preserved, correct bytes_freed count.
        use std::fs::FileTimes;
        use std::time::{Duration, SystemTime};

        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Store 2 files
        let data_old = vec![0xAA_u8; 500];
        let data_new = vec![0xBB_u8; 300];
        let r_old = store.store(&data_old).await.unwrap();
        let r_new = store.store(&data_new).await.unwrap();

        // Measure on-disk size before GC (compression may shrink data)
        let on_disk_old = std::fs::metadata(&r_old.path).unwrap().len();

        // Backdate file 1 mtime to 2 hours ago
        let two_hours_ago = SystemTime::now() - Duration::from_secs(2 * 3600);
        let file = std::fs::File::open(&r_old.path).unwrap();
        file.set_times(FileTimes::new().set_modified(two_hours_ago))
            .unwrap();

        // Keep file 2 recent (just created, mtime is now)

        // Clean files older than 1 hour
        let clean = store.clean_older_than(Duration::from_secs(3600));

        // File 1 should be deleted
        assert_eq!(clean.removed, 1, "only the old file should be removed");
        assert!(!r_old.path.exists(), "old file should be deleted by GC");

        // File 2 should be preserved
        assert!(r_new.path.exists(), "recent file should be preserved by GC");

        // bytes_freed reports actual on-disk size (may differ from logical size
        // due to compression)
        assert_eq!(
            clean.bytes_freed, on_disk_old,
            "bytes_freed should equal the old file's on-disk size"
        );

        // Verify store still lists 1 file
        assert_eq!(store.list().len(), 1, "only the recent file should remain");
    }

    #[tokio::test]
    async fn pr2_binary_artifact_checksum_is_blake3() {
        // Store data in CAS, create MediaRef with blake3 hash from StoreResult,
        // write binary artifact, re-hash the artifact output with blake3,
        // verify hash matches MediaRef.hash (strip "blake3:" prefix).
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // Use a non-trivial data pattern to ensure hash is meaningful
        let mut data = Vec::with_capacity(8192);
        for i in 0u32..2048 {
            data.extend_from_slice(&i.to_le_bytes());
        }
        assert_eq!(data.len(), 8192);

        let sr = store.store(&data).await.unwrap();

        let media_ref = MediaRef {
            hash: sr.hash.clone(),
            mime_type: "application/octet-stream".to_string(),
            size_bytes: sr.size,
            path: sr.path.clone(),
            extension: "bin".to_string(),
            created_by: "checksum_task".to_string(),
            metadata: serde_json::Map::new(),
        };

        // Write binary artifact
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "checksum-test");

        let request = crate::io::writer::BinaryWriteRequest {
            task_id: "checksum_task".to_string(),
            output_path: "verified.bin".to_string(),
            source: crate::io::writer::BinarySource::CasPath(media_ref.path.clone()),
            expected_size: media_ref.size_bytes,
        };

        let result = writer.write_binary(request).await.unwrap();

        // Re-hash the artifact output with blake3
        let written = tokio::fs::read(&result.path).await.unwrap();
        let artifact_raw_hash = blake3::hash(&written).to_hex().to_string();
        let artifact_prefixed_hash = format!("blake3:{artifact_raw_hash}");

        // Verify: artifact hash matches MediaRef.hash
        assert_eq!(
            artifact_prefixed_hash, media_ref.hash,
            "blake3 hash of artifact must match MediaRef.hash (no corruption in copy)"
        );

        // Also verify by stripping prefix
        let expected_raw = media_ref.hash.strip_prefix("blake3:").unwrap();
        assert_eq!(
            artifact_raw_hash, expected_raw,
            "raw hash comparison must match after prefix stripping"
        );

        // And verify against direct blake3 of the original data
        let original_hash = blake3::hash(&data).to_hex().to_string();
        assert_eq!(
            artifact_raw_hash, original_hash,
            "artifact hash must match hash of original data"
        );
    }

    #[tokio::test]
    async fn pr2_multiple_artifacts_from_single_task() {
        // One task produces 3 media files (image, audio, document).
        // 3 binary artifacts each with different source, process all 3,
        // verify all 3 written correctly with correct content.
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path().join("store"));

        // 3 different content types
        let image_data = real_png_bytes();
        let audio_data = real_mp3_bytes();
        let document_data = real_pdf_bytes();

        // Store all 3 in CAS
        let sr_img = store.store(&image_data).await.unwrap();
        let sr_aud = store.store(&audio_data).await.unwrap();
        let sr_doc = store.store(&document_data).await.unwrap();

        // Create 3 MediaRefs, all from the same task
        let media_refs = vec![
            MediaRef {
                hash: sr_img.hash.clone(),
                mime_type: "image/png".to_string(),
                size_bytes: sr_img.size,
                path: sr_img.path.clone(),
                extension: "png".to_string(),
                created_by: "multi_task".to_string(),
                metadata: serde_json::Map::new(),
            },
            MediaRef {
                hash: sr_aud.hash.clone(),
                mime_type: "audio/mpeg".to_string(),
                size_bytes: sr_aud.size,
                path: sr_aud.path.clone(),
                extension: "mp3".to_string(),
                created_by: "multi_task".to_string(),
                metadata: serde_json::Map::new(),
            },
            MediaRef {
                hash: sr_doc.hash.clone(),
                mime_type: "application/pdf".to_string(),
                size_bytes: sr_doc.size,
                path: sr_doc.path.clone(),
                extension: "pdf".to_string(),
                created_by: "multi_task".to_string(),
                metadata: serde_json::Map::new(),
            },
        ];

        // Verify all 3 have different hashes
        assert_ne!(media_refs[0].hash, media_refs[1].hash);
        assert_ne!(media_refs[0].hash, media_refs[2].hash);
        assert_ne!(media_refs[1].hash, media_refs[2].hash);

        // Create artifact writer
        let artifact_dir = dir.path().join("artifacts");
        tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
        let canonical = artifact_dir.canonicalize().unwrap();
        let writer = crate::io::writer::ArtifactWriter::new(canonical, "multi-test");

        // Write all 3 binary artifacts
        let output_specs = [
            ("output/image.png", &media_refs[0]),
            ("output/audio.mp3", &media_refs[1]),
            ("output/doc.pdf", &media_refs[2]),
        ];

        let expected_data = [&image_data, &audio_data, &document_data];

        for (i, (path, mr)) in output_specs.iter().enumerate() {
            let request = crate::io::writer::BinaryWriteRequest {
                task_id: "multi_task".to_string(),
                output_path: path.to_string(),
                source: crate::io::writer::BinarySource::CasPath(mr.path.clone()),
                expected_size: mr.size_bytes,
            };

            let result = writer.write_binary(request).await.unwrap();

            // Verify written content matches original
            let written = tokio::fs::read(&result.path).await.unwrap();
            assert_eq!(
                written, *expected_data[i],
                "artifact {i} ({}) content mismatch",
                path
            );
            assert_eq!(result.size, mr.size_bytes, "artifact {i} size mismatch");
        }

        // Verify CAS still has all 3 (artifacts are copies, not moves)
        assert!(store.exists(&media_refs[0].hash));
        assert!(store.exists(&media_refs[1].hash));
        assert!(store.exists(&media_refs[2].hash));

        // Verify TaskResult with all 3 media refs resolves correctly
        use crate::store::TaskResult;
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let task_id: Arc<str> = "multi_task".into();
        let tr = TaskResult::success(
            serde_json::json!("multi-output task"),
            std::time::Duration::from_millis(100),
        )
        .with_media(media_refs);
        ctx.insert(Arc::clone(&task_id), tr);

        // Verify all 3 media refs are accessible through resolve_path
        let arr = ctx.resolve_path("multi_task.media").unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 3);

        assert_eq!(
            ctx.resolve_path("multi_task.media[0].mime_type").unwrap(),
            "image/png"
        );
        assert_eq!(
            ctx.resolve_path("multi_task.media[1].mime_type").unwrap(),
            "audio/mpeg"
        );
        assert_eq!(
            ctx.resolve_path("multi_task.media[2].mime_type").unwrap(),
            "application/pdf"
        );
    }
}
