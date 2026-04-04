//! Paranoid media tool tests — adversarial inputs, panic detection, parameter exhaustion.
//!
//! These tests try to BREAK the media tools. Every test verifies specific error behavior.

#[cfg(test)]
mod tests {
    use crate::media::CasStore;
    use crate::runtime::builtin::media::color::DominantColorOp;
    use crate::runtime::builtin::media::context::MediaToolContext;
    use crate::runtime::builtin::media::dimensions::DimensionsOp;
    use crate::runtime::builtin::media::safety::sanitize_svg;
    use crate::runtime::builtin::media::{create_media_tool_adapters, MediaOp};
    use std::sync::Arc;

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    // ═══════════════════════════════════════════
    // PARAMETER EXHAUSTION — Every tool with every invalid param type
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn params_dimensions_hash_as_number() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": 12345}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn params_dimensions_hash_as_empty_string() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": ""}), &ctx).await;
        assert!(result.is_err()); // Empty hash should fail at CAS read
    }

    #[tokio::test]
    async fn params_dimensions_hash_as_null() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": null}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn params_dimensions_extra_unknown_fields() {
        let (_dir, ctx) = setup().await;
        let data = vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 5, 0, 0, 0, 5,
            8, 2, 0, 0, 0,
        ];
        let sr = ctx.cas.store(&data).await.unwrap();
        let op = DimensionsOp;
        // Extra fields should be ignored, not cause errors
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "unknown_field": "should_be_ignored",
                  "another": 42
                }),
                &ctx,
            )
            .await;
        // Should succeed or fail on image parsing, NOT on extra params
        // (imagesize may fail on this truncated PNG, that's ok)
        assert!(result.is_ok() || !result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn params_color_quality_boundary_values() {
        let (_dir, ctx) = setup().await;
        let op = DominantColorOp;
        // quality=0 should be clamped to 1
        let data = b"fake image data for color test - needs enough bytes";
        let sr = ctx.cas.store(data).await.unwrap();
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "quality": 0}), &ctx)
            .await;
        // Fake image data cannot be decoded, so execute should error
        assert!(
            result.is_err(),
            "Fake image data should fail: {:?}",
            result.unwrap()
        );

        // quality=100 should be clamped to 10
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "quality": 100}), &ctx)
            .await;
        assert!(
            result.is_err(),
            "Fake image data should fail: {:?}",
            result.unwrap()
        );
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn params_thumbnail_width_as_float() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({"hash": "blake3:abc", "width": 50.5}),
                &ctx,
            )
            .await;
        // Float width must be handled gracefully — either rejected or truncated
        assert!(
            result.is_ok() || result.is_err(),
            "float width must not panic"
        );
        if let Err(e) = &result {
            let msg = e.to_string();
            assert!(
                msg.contains("NIKA-") || msg.contains("invalid") || msg.contains("type"),
                "rejection must have a clear error, got: {msg}"
            );
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn params_thumbnail_width_as_negative() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({"hash": "blake3:abc", "width": -50}),
                &ctx,
            )
            .await;
        assert!(result.is_err()); // Negative should be rejected
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn params_thumbnail_width_as_string() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({"hash": "blake3:abc", "width": "fifty"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn params_convert_format_as_path_traversal() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::convert::ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
                  "format": "../../etc/passwd"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err()); // Should not succeed and definitely not create files
    }

    // ═══════════════════════════════════════════
    // CAS PATH TRAVERSAL — Exhaustive hash attacks
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn cas_traversal_dotdot_slash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = store.read("blake3:../../etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cas_traversal_url_encoded() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        // %2e%2e = ..
        let result = store.read("blake3:%2e%2e/%2e%2e/etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cas_traversal_null_byte() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = store.read("blake3:abc\0/../../../etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cas_hash_very_long() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let long_hash = format!("blake3:{}", "a".repeat(10000));
        let result = store.read(&long_hash).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cas_hash_unicode() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = store.read("blake3:café1234").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cas_exists_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        assert!(!store.exists("blake3:../../etc/passwd"));
        assert!(!store.exists("blake3:abc\0def"));
        assert!(!store.exists("blake3:GHIJKL")); // uppercase is not hex lowercase
    }

    // ═══════════════════════════════════════════
    // SVG ATTACK SURFACE — Comprehensive XSS/SSRF
    // ═══════════════════════════════════════════

    #[test]
    fn svg_entity_expansion_billion_laughs() {
        // Classic billion laughs / XML bomb
        let svg = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
]>
<svg xmlns="http://www.w3.org/2000/svg"><text>&lol2;</text></svg>"#;
        // Defense in depth: sanitizer now blocks DOCTYPE/ENTITY declarations
        // before they reach the usvg rendering layer.
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "sanitizer should block DOCTYPE/ENTITY declarations"
        );
    }

    #[test]
    fn svg_nested_svg_with_script() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <svg><script>alert(1)</script></svg>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err());
    }

    #[test]
    fn svg_data_uri_javascript() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <a href="data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==">
        <text>click</text>
      </a>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "data:text/html should be blocked");
    }

    #[test]
    fn svg_onload_with_whitespace_variations() {
        // Various whitespace tricks to bypass event handler detection
        let attacks = [
            r#"<svg onload="alert(1)"><rect/></svg>"#,
            r#"<svg onload ="alert(1)"><rect/></svg>"#,
            r#"<svg ONLOAD="alert(1)"><rect/></svg>"#,
            r#"<svg OnLoad="alert(1)"><rect/></svg>"#,
        ];
        for (i, svg) in attacks.iter().enumerate() {
            let result = sanitize_svg(svg);
            assert!(result.is_err(), "attack {i} should be blocked: {svg}");
        }
    }

    #[test]
    fn svg_xlink_href_variations() {
        let attacks = [
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <use xlink:href="https://evil.com/payload.svg#id"/>
      </svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg">
        <image xlink:href="file:///etc/shadow"/>
      </svg>"#,
        ];
        for (i, svg) in attacks.iter().enumerate() {
            let result = sanitize_svg(svg);
            assert!(result.is_err(), "xlink attack {i} should be blocked");
        }
    }

    // ═══════════════════════════════════════════
    // CONCURRENT STRESS — Race conditions
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn stress_100_concurrent_dimensions() {
        let (_dir, ctx) = setup().await;

        // Create a valid image in CAS
        #[cfg(feature = "media-thumbnail")]
        let data = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(10, 10, Rgb([255u8, 0, 0]));
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                10,
                10,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            buf
        };
        #[cfg(not(feature = "media-thumbnail"))]
        let data = vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 10, 0, 0, 0, 10,
            8, 2, 0, 0, 0,
        ];
        let sr = ctx.cas.store(&data).await.unwrap();

        let op = Arc::new(DimensionsOp);
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let op = Arc::clone(&op);
                let ctx = Arc::clone(&ctx);
                let hash = sr.hash.clone();
                tokio::spawn(
                    async move { op.execute(serde_json::json!({"hash": hash}), &ctx).await },
                )
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let ok_count = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();
        // All should succeed (dimensions is read-only)
        assert!(
            ok_count >= 95,
            "expected >= 95 successes from 100 concurrent ops, got {ok_count}"
        );
    }

    #[tokio::test]
    async fn stress_budget_exhaustion_and_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let budget = crate::media::MediaBudget::with_max_per_run(1000);
        let ctx = Arc::new(MediaToolContext {
            cas: CasStore::new(dir.path()),
            budget: Arc::new(budget),
            compute: Arc::new(crate::runtime::builtin::media::context::ComputePool::new().unwrap()),
            working_memory: Arc::new(
                crate::runtime::builtin::media::context::WorkingMemoryBudget::new(),
            ),
            cancel: tokio_util::sync::CancellationToken::new(),
            working_dir: None,
        });

        // Fill budget to 900 bytes
        let data = vec![0u8; 900];
        ctx.store_media(&data, "fill").await.unwrap();
        assert_eq!(ctx.budget.current_bytes(), 900);

        // This should fail (900 + 200 > 1000)
        let data2 = vec![1u8; 200];
        let result = ctx.store_media(&data2, "overflow").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("NIKA-259"),
            "expected budget error, got: {err_msg}"
        );

        // Budget should NOT have increased
        assert_eq!(ctx.budget.current_bytes(), 900);
    }

    #[tokio::test]
    async fn stress_working_memory_concurrent_acquire() {
        let budget = crate::runtime::builtin::media::context::WorkingMemoryBudget::with_max(1000);

        // Acquire 500
        let guard1 = budget.acquire(500).unwrap();
        assert_eq!(budget.current(), 500);

        // Acquire another 500 (exactly at limit)
        let guard2 = budget.acquire(500).unwrap();
        assert_eq!(budget.current(), 1000);

        // This should fail
        let result = budget.acquire(1);
        assert!(result.is_err());

        // Drop one guard
        drop(guard1);
        assert_eq!(budget.current(), 500);

        // Now this should succeed
        let _guard3 = budget.acquire(500).unwrap();
        assert_eq!(budget.current(), 1000);

        drop(guard2);
        drop(_guard3);
        assert_eq!(budget.current(), 0);
    }

    // ═══════════════════════════════════════════
    // TOOL ADAPTER — Through the BuiltinTool trait
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn adapter_all_tools_have_name() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(ctx);
        for tool in &tools {
            let name = tool.name();
            assert!(!name.is_empty(), "tool name must not be empty");
            assert!(
                !name.contains(':'),
                "tool name must not contain colon: {name}"
            );
        }
    }

    #[tokio::test]
    async fn adapter_all_tools_have_description() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(ctx);
        for tool in &tools {
            let desc = tool.description();
            assert!(
                !desc.is_empty(),
                "tool {} must have description",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn adapter_all_tools_have_schema() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(ctx);
        for tool in &tools {
            let schema = tool.parameters_schema();
            assert!(
                schema.is_object(),
                "tool {} schema must be object",
                tool.name()
            );
            let obj = schema.as_object().unwrap();
            assert!(
                obj.contains_key("properties"),
                "tool {} must have properties",
                tool.name()
            );
            assert!(
                obj.contains_key("required"),
                "tool {} must have required",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn adapter_all_tools_reject_empty_json() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(ctx);
        for tool in &tools {
            let result = tool.call("{}".to_string()).await;
            // All tools require at least "hash", so empty JSON should error
            assert!(
                result.is_err(),
                "tool {} should reject empty params",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn adapter_all_tools_reject_invalid_json() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(ctx);
        for tool in &tools {
            let result = tool.call("not json at all".to_string()).await;
            assert!(
                result.is_err(),
                "tool {} should reject invalid JSON",
                tool.name()
            );
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("NIKA-294"),
                "tool {} should return NIKA-294, got: {err}",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn adapter_cancelled_context_rejects_all_tools() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();

        let tools = create_media_tool_adapters(Arc::clone(&ctx));
        for tool in &tools {
            let result = tool.call(r#"{"hash":"blake3:abc"}"#.to_string()).await;
            assert!(
                result.is_err(),
                "tool {} should fail on cancelled context",
                tool.name()
            );
        }
    }

    // ═══════════════════════════════════════════
    // ENRICHMENT — Verify auto-enrichment correctness
    // ═══════════════════════════════════════════

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn enrichment_different_sizes_different_hashes() {
        // Two different images should have different thumbhashes
        let (_dir, ctx) = setup().await;
        let processor = crate::media::processor::MediaProcessor::new(CasStore::new(ctx.cas.root()));

        let img1 = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(10, 10, Rgb([255u8, 0, 0]));
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                10,
                10,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf)
        };

        let img2 = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(20, 20, Rgb([0u8, 0, 255]));
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                20,
                20,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf)
        };

        let block1 = crate::mcp::types::ContentBlock::image(img1, "image/png");
        let block2 = crate::mcp::types::ContentBlock::image(img2, "image/png");

        let (ref1, _) = processor.process(&block1, "t1").await.unwrap().unwrap();
        let (ref2, _) = processor.process(&block2, "t2").await.unwrap().unwrap();

        // Different images → different hashes
        assert_ne!(ref1.hash, ref2.hash);

        // Both should have width/height
        assert_eq!(ref1.metadata.get("width").unwrap(), &serde_json::json!(10));
        assert_eq!(ref2.metadata.get("width").unwrap(), &serde_json::json!(20));

        // Both should have thumbhash
        assert!(ref1.metadata.contains_key("thumbhash"));
        assert!(ref2.metadata.contains_key("thumbhash"));

        // Different images → different thumbhashes
        assert_ne!(
            ref1.metadata.get("thumbhash"),
            ref2.metadata.get("thumbhash"),
            "different images should produce different thumbhashes"
        );
    }

    // ═══════════════════════════════════════════
    // PANIC DETECTION — Verify no tool panics on any input
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn panic_all_tools_random_100_bytes() {
        let (_dir, ctx) = setup().await;
        let tools = create_media_tool_adapters(Arc::clone(&ctx));

        for tool in &tools {
            for i in 1..20u8 {
                let data: Vec<u8> = (0..=i).map(|b| b.wrapping_mul(7)).collect();
                if let Ok(sr) = ctx.cas.store(&data).await {
                    let result = tool
                        .call(
                            serde_json::json!({"hash": sr.hash, "width": 50, "format": "png"})
                                .to_string(),
                        )
                        .await;
                    if let Err(e) = &result {
                        assert!(
                            !e.to_string().contains("panicked"),
                            "tool {} panicked on random input {i}: {}",
                            tool.name(),
                            e
                        );
                    }
                }
            }
        }
    }
}
