// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media tool integration tests — cross-tool chains, CAS integrity, security, E2E.

#[cfg(test)]
mod tests {
    use crate::media::CasStore;
    use crate::runtime::builtin::media::context::MediaToolContext;
    use crate::runtime::builtin::media::dimensions::DimensionsOp;
    use crate::runtime::builtin::media::safety::sanitize_svg;
    use crate::runtime::builtin::media::thumbhash_tool::ThumbhashOp;
    use crate::runtime::builtin::media::MediaOp;
    use crate::runtime::media_context::EngineMediaContext;
    #[cfg(feature = "media-thumbnail")]
    use crate::runtime::builtin::media::MediaOpResult;
    use std::sync::Arc;

    /// Helper: wrap a MediaToolContext in EngineMediaContext for tests.
    fn engine_ctx(ctx: std::sync::Arc<nika_media::tools::context::MediaToolContext>) -> std::sync::Arc<EngineMediaContext> {
        std::sync::Arc::new(EngineMediaContext::new(ctx))
    }

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    // ═══════════════════════════════════════════
    // CROSS-TOOL INTEGRATION
    // ═══════════════════════════════════════════

    #[cfg(feature = "media-thumbnail")]
    fn fixture_png_100x50() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_fn(100, 50, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            100,
            50,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn integration_thumbnail_then_dimensions() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        // Step 1: Thumbnail to 50xN
        let thumb_op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = thumb_op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50
                }),
                &ctx,
            )
            .await
            .unwrap();

        let thumb_hash = if let MediaOpResult::Binary { data, .. } = &result {
            let sr2 = ctx.cas.store(data).await.unwrap();
            sr2.hash.clone()
        } else {
            panic!("expected Binary result");
        };

        // Step 2: Dimensions on thumbnail output
        let dim_op = DimensionsOp;
        let dim_result = dim_op
            .execute(serde_json::json!({"hash": thumb_hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = dim_result {
            assert_eq!(v["width"], 50);
            assert_eq!(v["height"], 25); // 100:50 ratio preserved
        } else {
            panic!("expected Metadata result");
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn integration_thumbnail_then_convert() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        // Thumbnail → PNG
        let thumb_op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = thumb_op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50
                }),
                &ctx,
            )
            .await
            .unwrap();

        let thumb_hash = if let MediaOpResult::Binary { data, .. } = &result {
            let sr2 = ctx.cas.store(data).await.unwrap();
            sr2.hash
        } else {
            panic!("expected Binary");
        };

        // Convert PNG → JPEG
        let convert_op = crate::runtime::builtin::media::convert::ConvertOp;
        let result = convert_op
            .execute(
                serde_json::json!({
                  "hash": thumb_hash, "format": "jpeg"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data, mime_type, ..
        } = result
        {
            assert_eq!(mime_type, "image/jpeg");
            assert_eq!(&data[..2], &[0xFF, 0xD8]); // JPEG magic
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn integration_strip_then_dimensions() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        // Strip metadata
        let strip_op = crate::runtime::builtin::media::strip::StripOp;
        let result = strip_op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        let stripped_hash = if let MediaOpResult::Binary { data, .. } = &result {
            let sr2 = ctx.cas.store(data).await.unwrap();
            sr2.hash
        } else {
            panic!("expected Binary");
        };

        // Dimensions should be preserved
        let dim_op = DimensionsOp;
        let result = dim_op
            .execute(serde_json::json!({"hash": stripped_hash}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["width"], 100);
            assert_eq!(v["height"], 50);
        }
    }

    // ═══════════════════════════════════════════
    // CAS INTEGRITY
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn cas_tool_dedup_same_input() {
        let (_dir, ctx) = setup().await;
        let data = b"identical data for dedup test";
        let sr1 = ctx.cas.store(data).await.unwrap();
        let sr2 = ctx.cas.store(data).await.unwrap();
        assert_eq!(sr1.hash, sr2.hash);
        assert!(sr2.deduplicated);
    }

    #[tokio::test]
    async fn cas_concurrent_tool_writes() {
        let (_dir, ctx) = setup().await;
        let ctx = Arc::clone(&ctx);

        let handles: Vec<_> = (0..10u8)
            .map(|i| {
                let ctx = Arc::clone(&ctx);
                tokio::spawn(async move {
                    let data = format!("concurrent write {i}");
                    ctx.cas.store(data.as_bytes()).await
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let ok_count = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();
        assert_eq!(ok_count, 10, "all concurrent writes should succeed");
    }

    #[tokio::test]
    async fn cas_budget_across_tools() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());

        // Store some data, budget should track
        let data1 = vec![0u8; 1000];
        ctx.store_media(&data1, "t1").await.unwrap();
        assert_eq!(ctx.budget.current_bytes(), 1000);

        let data2 = vec![1u8; 2000];
        ctx.store_media(&data2, "t2").await.unwrap();
        assert_eq!(ctx.budget.current_bytes(), 3000);
    }

    // ═══════════════════════════════════════════
    // SECURITY TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn security_svg_xss_comprehensive() {
        let xss_payloads = vec![
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><SCRIPT>alert(1)</SCRIPT></svg>"#,
            r#"<svg><ScRiPt>alert(1)</ScRiPt></svg>"#,
            r#"<svg><foreignObject><div>html</div></foreignObject></svg>"#,
            r#"<svg><a href="javascript:alert(1)">click</a></svg>"#,
            r#"<svg onload="alert(1)"><rect/></svg>"#,
            r#"<svg><rect onclick="alert(1)"/></svg>"#,
            r#"<svg><rect onerror ="alert(1)"/></svg>"#,
            r#"<svg><rect onmouseover="alert(1)"/></svg>"#,
        ];

        for (i, payload) in xss_payloads.iter().enumerate() {
            let result = sanitize_svg(payload);
            assert!(result.is_err(), "payload {i} should be rejected: {payload}");
            assert!(
                result.unwrap_err().to_string().contains("NIKA-297"),
                "payload {i} should produce NIKA-297"
            );
        }
    }

    #[test]
    fn security_svg_safe_patterns() {
        let safe_svgs = [
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect fill="red" width="100" height="100"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><circle cx="50" cy="50" r="40" fill="blue"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><text x="10" y="40">Hello</text></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M 10 10 L 90 90" stroke="black"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g transform="rotate(45)"><rect width="10" height="10"/></g></svg>"#,
        ];

        for (i, svg) in safe_svgs.iter().enumerate() {
            let result = sanitize_svg(svg);
            assert!(
                result.is_ok(),
                "safe SVG {i} should pass: {:?}",
                result.err()
            );
        }
    }

    // ═══════════════════════════════════════════
    // BUG REGRESSION TESTS (from deep analysis)
    // ═══════════════════════════════════════════

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug_color_count_1_does_not_panic() {
        // BUG: color_thief asserts max_colors >= 2 internally.
        // count=1 was allowed by schema, causing rayon panic.
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::color::DominantColorOp;
        // count=1 should be clamped to 2, not panic
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "count": 1
                }),
                &ctx,
            )
            .await;
        assert!(
            result.is_ok(),
            "count=1 should succeed (clamped to 2), got: {:?}",
            result.err()
        );

        if let Ok(MediaOpResult::Metadata(v)) = result {
            let colors = v["colors"].as_array().unwrap();
            assert!(!colors.is_empty(), "should return at least 1 color");
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug_thumbnail_extreme_aspect_no_oom() {
        // BUG: 1x10000 image + width=10000 computes height=100M, causing OOM.
        // Computed height must be clamped to 10000.
        let (_dir, ctx) = setup().await;

        let img = image::ImageBuffer::from_pixel(1, 5000, image::Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            1,
            5000,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        let sr = ctx.cas.store(&buf).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 5000
                }),
                &ctx,
            )
            .await;

        // Should succeed but with clamped height, NOT OOM
        assert!(
            result.is_ok(),
            "extreme aspect ratio should not OOM: {:?}",
            result.err()
        );
        if let Ok(MediaOpResult::Binary { metadata, .. }) = result {
            let h = metadata["height"].as_u64().unwrap();
            assert!(h <= 10_000, "height must be clamped to 10000, got {h}");
        }
    }

    // ═══════════════════════════════════════════
    // SECURITY HARDENING TESTS (from audit)
    // ═══════════════════════════════════════════

    #[test]
    fn security_svg_xlink_href_blocked() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image xlink:href="https://evil.com/track" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "xlink:href should be blocked");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    #[test]
    fn security_svg_file_protocol_blocked() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image href="file:///etc/passwd" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "file:// should be blocked");
    }

    #[test]
    fn security_svg_data_html_blocked() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image href="data:text/html,<script>alert(1)</script>" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "data:text/html should be blocked");
    }

    #[tokio::test]
    async fn security_cas_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        // Attempt path traversal via crafted hash
        let result = store.read("blake3:../../etc/passwd").await;
        assert!(result.is_err(), "path traversal hash should be rejected");
    }

    #[tokio::test]
    async fn security_cas_non_hex_hash_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = store.read("blake3:zzzzzzzzzz").await;
        assert!(result.is_err(), "non-hex hash should be rejected");
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn security_thumbnail_width_clamped() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        // Width > 10000 should be rejected
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50000
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn security_cancelled_workflow_stops_tools() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel(); // Cancel the workflow

        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}), &ctx).await;
        assert!(
            result.is_err(),
            "cancelled workflow should stop tool execution"
        );
    }

    // ═══════════════════════════════════════════
    // FUZZ — No tool should ever panic
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn fuzz_dimensions_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        for i in 1..100u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
                // Errors are fine, but panics (caught by rayon) are not
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "tool panicked on input {i}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn fuzz_thumbhash_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = ThumbhashOp;
        for i in 1..50u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "tool panicked on input {i}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn fuzz_thumbnail_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        for i in 1..100u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op
                    .execute(serde_json::json!({"hash": sr.hash, "width": 50}), &ctx)
                    .await;
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "tool panicked on input {i}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "media-optimize")]
    #[tokio::test]
    async fn fuzz_optimize_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::optimize::OptimizeOp;
        // PNG-like header + garbage
        for i in 1..50u8 {
            let mut data = vec![137, 80, 78, 71, 13, 10, 26, 10];
            data.extend((0..=i).collect::<Vec<u8>>());
            if let Ok(sr) = ctx.cas.store(&data).await {
                let _ = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
            }
        }
    }

    #[cfg(feature = "media-svg")]
    #[tokio::test]
    async fn fuzz_svg_render_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = crate::runtime::builtin::media::svg::SvgRenderOp;
        let repeated = "<svg>".repeat(50);
        let fuzz_inputs = vec![
            "",
            "not xml",
            "<svg></svg>",
            "<svg><invalid></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'></svg>",
            &repeated,
            "<svg><rect width='100' height='100'/></svg>",
        ];
        for input in fuzz_inputs {
            if let Ok(sr) = ctx.cas.store(input.as_bytes()).await {
                let _ = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
            }
        }
    }

    // ═══════════════════════════════════════════
    // FEATURE FLAG TESTS
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn feature_all_tools_registered() {
        let (_dir, ctx) = setup().await;
        let tools = crate::runtime::builtin::media::create_media_tool_adapters(engine_ctx(ctx));
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

        // Always-on tools must be present
        assert!(
            names.contains(&"dimensions"),
            "dimensions should always be registered"
        );
        assert!(
            names.contains(&"thumbhash"),
            "thumbhash should always be registered"
        );
        assert!(
            names.contains(&"dominant_color"),
            "dominant_color should always be registered"
        );

        // Feature-gated tools
        #[cfg(feature = "media-thumbnail")]
        {
            assert!(
                names.contains(&"thumbnail"),
                "thumbnail should be registered with media-thumbnail"
            );
            assert!(
                names.contains(&"convert"),
                "convert should be registered with media-thumbnail"
            );
            assert!(
                names.contains(&"strip"),
                "strip should be registered with media-thumbnail"
            );
        }
        #[cfg(feature = "media-metadata")]
        assert!(
            names.contains(&"metadata"),
            "metadata should be registered with media-metadata"
        );
        #[cfg(feature = "media-optimize")]
        assert!(
            names.contains(&"optimize"),
            "optimize should be registered with media-optimize"
        );
        #[cfg(feature = "media-svg")]
        assert!(
            names.contains(&"svg_render"),
            "svg_render should be registered with media-svg"
        );
    }

    // ═══════════════════════════════════════════
    // ROUTER INTEGRATION
    // ═══════════════════════════════════════════

    #[tokio::test]
    async fn router_dispatches_media_tools() {
        use crate::runtime::builtin::BuiltinToolRouter;
        use crate::tools::{PermissionMode, ToolContext};

        let dir = tempfile::tempdir().unwrap();
        let tool_ctx = Arc::new(ToolContext::new(
            dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        let media_ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let router = BuiltinToolRouter::with_file_tools(tool_ctx)
            .with_media(engine_ctx(media_ctx));

        // Media tools should be registered
        assert!(router.has_tool("dimensions"), "dimensions not in router");
        assert!(router.has_tool("thumbhash"), "thumbhash not in router");
        assert!(
            router.has_tool("dominant_color"),
            "dominant_color not in router"
        );

        // nika: prefix dispatch works
        assert!(crate::runtime::builtin::BuiltinToolRouter::is_builtin(
            "nika:dimensions"
        ));
    }

    #[tokio::test]
    async fn router_has_all_expected_tools() {
        use crate::runtime::builtin::BuiltinToolRouter;
        use crate::tools::{PermissionMode, ToolContext};

        let dir = tempfile::tempdir().unwrap();
        let tool_ctx = Arc::new(ToolContext::new(
            dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        let media_ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let router = BuiltinToolRouter::with_file_tools(tool_ctx)
            .with_media(engine_ctx(media_ctx));

        let names = router.tool_names();
        // 7 core + 5 file + N media
        assert!(
            names.len() >= 15,
            "expected >= 15 tools, got {}: {:?}",
            names.len(),
            names
        );
    }
}
