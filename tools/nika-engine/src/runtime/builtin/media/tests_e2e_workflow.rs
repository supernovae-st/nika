// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E workflow-level tests for media tools.
//!
//! Tests the FULL invoke dispatch path: BuiltinToolRouter → MediaToolAdapter → MediaOp → CAS.
//!
//! Categories:
//! 1. Router dispatch tests — nika:* prefix dispatch with real PNG data
//! 2. MediaToolAdapter tests — timeout, JSON parsing, CAS write, cancellation
//! 3. Cross-tool pipeline tests — chained media operations via router

#[cfg(test)]
#[cfg(feature = "media-thumbnail")]
mod tests {
    use std::sync::Arc;

    use crate::media::CasStore;
    use crate::runtime::builtin::media::context::MediaToolContext;
    use crate::runtime::builtin::media::{MediaOpResult, MediaToolAdapter};
    use crate::runtime::builtin::BuiltinToolRouter;
    use crate::runtime::media_context::EngineMediaContext;
    use crate::tools::{PermissionMode, ToolContext};

    /// Helper: wrap a MediaToolContext in EngineMediaContext for tests.
    fn engine_ctx(ctx: std::sync::Arc<nika_media::tools::context::MediaToolContext>) -> std::sync::Arc<EngineMediaContext> {
        std::sync::Arc::new(EngineMediaContext::new(ctx))
    }

    // ═══════════════════════════════════════════════════════════════
    // FIXTURES
    // ═══════════════════════════════════════════════════════════════

    /// 100x50 RGBA PNG — large enough for thumbnail, optimize, convert, etc.
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

    /// 10x10 solid red PNG — minimal for format conversion tests.
    fn fixture_png_10x10_red() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_pixel(10, 10, Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    /// Create a full router with file + media tools and a shared MediaToolContext.
    fn setup_router() -> (tempfile::TempDir, BuiltinToolRouter, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let tool_ctx = Arc::new(ToolContext::new(
            dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        let media_ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let router = BuiltinToolRouter::with_all_tools(tool_ctx, Arc::clone(&media_ctx));
        (dir, router, media_ctx)
    }

    /// Store fixture PNG in CAS and return the blake3 hash.
    async fn store_fixture(ctx: &MediaToolContext, data: &[u8]) -> String {
        let sr = ctx.cas.store(data).await.unwrap();
        sr.hash
    }

    // ═══════════════════════════════════════════════════════════════
    // 1. ROUTER DISPATCH TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn dispatch_dimensions_through_router_with_real_png() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:dimensions", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["width"], 100);
        assert_eq!(parsed["height"], 50);
        assert_eq!(parsed["orientation"], "landscape");
    }

    #[tokio::test]
    async fn dispatch_thumbnail_through_router_with_real_png() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash, "width": 50}).to_string();
        let result = router.dispatch("nika:thumbnail", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(
            parsed["hash"].as_str().unwrap().starts_with("blake3:"),
            "should return CAS hash"
        );
        assert_eq!(parsed["mime_type"], "image/png");
        assert_eq!(parsed["metadata"]["width"], 50);
        assert_eq!(parsed["metadata"]["height"], 25);
        assert!(parsed["size_bytes"].as_u64().unwrap() > 0);
    }

    #[cfg(feature = "media-optimize")]
    #[tokio::test]
    async fn dispatch_optimize_through_router_with_real_png() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:optimize", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["mime_type"], "image/png");
        assert!(parsed["metadata"]["original_size"].as_u64().unwrap() > 0);
        assert!(parsed["metadata"]["optimized_size"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn dispatch_convert_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let args = serde_json::json!({"hash": hash, "format": "jpeg"}).to_string();
        let result = router.dispatch("nika:convert", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["mime_type"], "image/jpeg");
        assert_eq!(parsed["extension"], "jpg");
    }

    #[tokio::test]
    async fn dispatch_strip_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:strip", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["mime_type"], "image/png");
        assert_eq!(parsed["metadata"]["stripped"], true);
    }

    #[cfg(feature = "media-metadata")]
    #[tokio::test]
    async fn dispatch_metadata_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:metadata", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["width"], 100);
        assert_eq!(parsed["height"], 50);
        assert!(parsed["mime_type"].as_str().unwrap().starts_with("image/"));
        assert!(parsed["size_bytes"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn dispatch_thumbhash_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:thumbhash", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let encoded = parsed["thumbhash"].as_str().unwrap();
        // Verify it is valid base64
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .expect("thumbhash must be valid base64");
        assert!(
            (3..=28).contains(&decoded.len()),
            "thumbhash must be 3-28 bytes per spec, got {}",
            decoded.len()
        );
    }

    #[tokio::test]
    async fn dispatch_dominant_color_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:dominant_color", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let colors = parsed["colors"].as_array().unwrap();
        assert!(!colors.is_empty(), "should extract at least one color");
        // First color should be reddish (solid red image)
        let first = &colors[0];
        assert!(
            first["r"].as_u64().unwrap() > 200,
            "red channel should be dominant"
        );
    }

    #[cfg(feature = "media-svg")]
    #[tokio::test]
    async fn dispatch_svg_render_through_router() {
        let (_dir, router, ctx) = setup_router();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="blue"/>
    </svg>"#;
        let hash = store_fixture(&ctx, svg.as_bytes()).await;

        let args = serde_json::json!({"hash": hash, "width": 50}).to_string();
        let result = router.dispatch("nika:svg_render", args).await;

        assert!(result.is_ok(), "dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["mime_type"], "image/png");
        assert_eq!(parsed["metadata"]["source_format"], "svg");
    }

    /// Verify all media tools are registered through create_media_tool_adapters.
    #[test]
    fn all_media_tools_registered_in_router() {
        let (_dir, router, _ctx) = setup_router();
        let names = router.tool_names();

        // Always-on (3)
        assert!(names.contains(&"dimensions"), "missing dimensions");
        assert!(names.contains(&"thumbhash"), "missing thumbhash");
        assert!(names.contains(&"dominant_color"), "missing dominant_color");

        // media-thumbnail feature (3)
        assert!(names.contains(&"thumbnail"), "missing thumbnail");
        assert!(names.contains(&"convert"), "missing convert");
        assert!(names.contains(&"strip"), "missing strip");

        // media-metadata feature (1)
        #[cfg(feature = "media-metadata")]
        assert!(names.contains(&"metadata"), "missing metadata");

        // media-optimize feature (1)
        #[cfg(feature = "media-optimize")]
        assert!(names.contains(&"optimize"), "missing optimize");

        // media-svg feature (1)
        #[cfg(feature = "media-svg")]
        assert!(names.contains(&"svg_render"), "missing svg_render");
    }

    /// Verify total tool count: 7 core + 5 file + N media.
    #[test]
    fn router_total_tool_count() {
        let (_dir, router, _ctx) = setup_router();
        let names = router.tool_names();
        let total = names.len();

        // 7 core + 5 file = 12 baseline
        // + 1 always-on (import)
        // + 3 always-on media (dimensions, thumbhash, dominant_color)
        // + 1 always-on (pipeline)
        // + 3 media-thumbnail (thumbnail, convert, strip)
        // + 1 media-metadata (metadata)
        // + 1 media-optimize (optimize)
        // + 1 media-svg (svg_render)
        // + 2 media-phash (phash, compare)
        // + 1 media-pdf (pdf_extract)
        // + 1 media-chart (chart)
        // + 1 media-provenance (provenance)
        let mut expected = 12 + 1 + 3 + 1; // baseline + import + always-on media + pipeline

        #[cfg(feature = "media-thumbnail")]
        {
            expected += 3;
        }
        #[cfg(feature = "media-metadata")]
        {
            expected += 1;
        }
        #[cfg(feature = "media-optimize")]
        {
            expected += 1;
        }
        #[cfg(feature = "media-svg")]
        {
            expected += 1;
        }
        #[cfg(feature = "media-phash")]
        {
            expected += 2; // phash + compare
        }
        #[cfg(feature = "media-pdf")]
        {
            expected += 1;
        }
        #[cfg(feature = "media-chart")]
        {
            expected += 1;
        }
        #[cfg(feature = "media-provenance")]
        {
            expected += 2; // provenance + verify
        }
        #[cfg(feature = "media-qr")]
        {
            expected += 1; // qr_validate
        }
        #[cfg(feature = "media-iqa")]
        {
            expected += 1; // quality
        }
        #[cfg(feature = "fetch-html")]
        {
            expected += 3; // css_select, extract_metadata, extract_links
        }
        #[cfg(feature = "fetch-markdown")]
        {
            expected += 1; // html_to_md
        }
        #[cfg(feature = "fetch-article")]
        {
            expected += 1; // readability
        }

        assert_eq!(
            total, expected,
            "expected {expected} tools, got {total}: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_returns_error() {
        let (_dir, router, _ctx) = setup_router();

        let result = router.dispatch("nika:nonexistent", "{}".to_string()).await;

        assert!(result.is_err(), "unknown tool should error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Unknown builtin tool"),
            "error should mention unknown tool: {}",
            err
        );
    }

    #[tokio::test]
    async fn dispatch_with_invalid_json_returns_nika_294() {
        let (_dir, router, _ctx) = setup_router();

        let result = router
            .dispatch("nika:dimensions", "this is not json".to_string())
            .await;

        assert!(result.is_err(), "invalid JSON should error");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NIKA-294"),
            "should produce NIKA-294, got: {}",
            err
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 2. MEDIA TOOL ADAPTER TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn adapter_timeout_fires_on_slow_op() {
        use std::future::Future;
        use std::pin::Pin;
        use std::time::Duration;

        use crate::runtime::builtin::BuiltinTool;

        /// A deliberately slow MediaOp that sleeps for 60 seconds.
        struct SlowOp;

        impl super::super::super::media::MediaOp for SlowOp {
            fn name(&self) -> &'static str {
                "slow"
            }
            fn description(&self) -> &'static str {
                "intentionally slow"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn execute<'a>(
                &'a self,
                _args: serde_json::Value,
                _ctx: &'a MediaToolContext,
            ) -> Pin<
                Box<
                    dyn Future<
                            Output = Result<
                                MediaOpResult,
                                nika_media::tools::error::MediaToolError,
                            >,
                        > + Send
                        + 'a,
                >,
            > {
                Box::pin(async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    Ok(MediaOpResult::Metadata(serde_json::json!({"done": true})))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let adapter = MediaToolAdapter::new(Arc::new(SlowOp), engine_ctx(ctx));

        let start = std::time::Instant::now();
        let result = adapter.call("{}".to_string()).await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "slow op should timeout");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-293"),
            "should produce NIKA-293 timeout"
        );
        // The default timeout is 30s. We expect it to fire well before 60s.
        assert!(
            elapsed < Duration::from_secs(45),
            "timeout should fire around 30s, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn adapter_valid_json_dispatches() {
        use crate::runtime::builtin::BuiltinTool;

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let op = crate::runtime::builtin::media::dimensions::DimensionsOp;
        let adapter = MediaToolAdapter::new(Arc::new(op), engine_ctx(Arc::clone(&ctx)));

        let result = adapter
            .call(serde_json::json!({"hash": hash}).to_string())
            .await;

        assert!(
            result.is_ok(),
            "valid JSON should succeed: {:?}",
            result.err()
        );
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["width"], 10);
        assert_eq!(parsed["height"], 10);
    }

    #[tokio::test]
    async fn adapter_invalid_json_returns_nika_294() {
        use crate::runtime::builtin::BuiltinTool;

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let op = crate::runtime::builtin::media::dimensions::DimensionsOp;
        let adapter = MediaToolAdapter::new(Arc::new(op), engine_ctx(ctx));

        let result = adapter.call("not valid json".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn adapter_empty_string_returns_nika_294() {
        use crate::runtime::builtin::BuiltinTool;

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let op = crate::runtime::builtin::media::dimensions::DimensionsOp;
        let adapter = MediaToolAdapter::new(Arc::new(op), engine_ctx(ctx));

        let result = adapter.call(String::new()).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "empty string is invalid JSON"
        );
    }

    #[tokio::test]
    async fn adapter_binary_result_writes_to_cas() {
        use crate::runtime::builtin::BuiltinTool;

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let adapter = MediaToolAdapter::new(Arc::new(op), engine_ctx(Arc::clone(&ctx)));

        let result = adapter
            .call(serde_json::json!({"hash": hash, "width": 30}).to_string())
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        let cas_hash = parsed["hash"].as_str().unwrap();
        assert!(
            cas_hash.starts_with("blake3:"),
            "result should contain CAS hash"
        );

        // Verify the binary data is readable from CAS
        let data = ctx.cas.read(cas_hash).await.unwrap();
        assert!(!data.is_empty(), "CAS should contain thumbnail data");

        // Verify it is a valid PNG
        assert_eq!(
            &data[..4],
            &[137, 80, 78, 71],
            "stored data should be valid PNG"
        );
    }

    #[tokio::test]
    async fn adapter_cancelled_context_returns_error() {
        use crate::runtime::builtin::BuiltinTool;

        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        // Cancel the workflow before dispatch
        ctx.cancel.cancel();

        let op = crate::runtime::builtin::media::dimensions::DimensionsOp;
        let adapter = MediaToolAdapter::new(Arc::new(op), engine_ctx(Arc::clone(&ctx)));

        let result = adapter
            .call(serde_json::json!({"hash": hash}).to_string())
            .await;

        assert!(result.is_err(), "cancelled context should stop execution");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("cancelled") || err.contains("NIKA-290"),
            "should mention cancellation: {}",
            err
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 3. CROSS-TOOL PIPELINE TESTS (via router)
    // ═══════════════════════════════════════════════════════════════

    /// thumbnail(100x50, width=50) -> dimensions -> verify width=50, height=25
    #[tokio::test]
    async fn pipeline_thumbnail_then_dimensions_verify_size() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        // Step 1: Thumbnail to 50px wide
        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": hash, "width": 50}).to_string(),
            )
            .await
            .unwrap();
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let thumb_hash = thumb["hash"].as_str().unwrap();

        // Step 2: Dimensions on the thumbnail output
        let dim_result = router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": thumb_hash}).to_string(),
            )
            .await
            .unwrap();
        let dims: serde_json::Value = serde_json::from_str(&dim_result).unwrap();

        assert_eq!(dims["width"], 50, "thumbnail width should be 50");
        assert_eq!(dims["height"], 25, "aspect ratio 100:50 -> 50:25");
    }

    /// thumbnail -> optimize -> verify output is still valid PNG
    #[cfg(feature = "media-optimize")]
    #[tokio::test]
    async fn pipeline_thumbnail_then_optimize_still_valid_png() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        // Step 1: Thumbnail
        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": hash, "width": 40}).to_string(),
            )
            .await
            .unwrap();
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let thumb_hash = thumb["hash"].as_str().unwrap();

        // Step 2: Optimize the thumbnail
        let opt_result = router
            .dispatch(
                "nika:optimize",
                serde_json::json!({"hash": thumb_hash}).to_string(),
            )
            .await
            .unwrap();
        let opt: serde_json::Value = serde_json::from_str(&opt_result).unwrap();
        let opt_hash = opt["hash"].as_str().unwrap();

        // Step 3: Read from CAS and verify it is a decodable PNG
        let data = ctx.cas.read(opt_hash).await.unwrap();
        assert_eq!(
            &data[..4],
            &[137, 80, 78, 71],
            "optimized output should be PNG"
        );
        let img = image::load_from_memory(&data).expect("optimized PNG must be decodable");
        assert_eq!(img.width(), 40);
        assert_eq!(img.height(), 20);
    }

    /// convert(png->jpeg) -> convert(jpeg->png) -> dimensions -> verify round-trip
    #[tokio::test]
    async fn pipeline_convert_roundtrip_png_jpeg_png() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        // Step 1: PNG -> JPEG
        let jpeg_result = router
            .dispatch(
                "nika:convert",
                serde_json::json!({"hash": hash, "format": "jpeg"}).to_string(),
            )
            .await
            .unwrap();
        let jpeg: serde_json::Value = serde_json::from_str(&jpeg_result).unwrap();
        let jpeg_hash = jpeg["hash"].as_str().unwrap();
        assert_eq!(jpeg["mime_type"], "image/jpeg");

        // Step 2: JPEG -> PNG
        let png_result = router
            .dispatch(
                "nika:convert",
                serde_json::json!({"hash": jpeg_hash, "format": "png"}).to_string(),
            )
            .await
            .unwrap();
        let png: serde_json::Value = serde_json::from_str(&png_result).unwrap();
        let png_hash = png["hash"].as_str().unwrap();
        assert_eq!(png["mime_type"], "image/png");

        // Step 3: Verify dimensions preserved after round-trip
        let dim_result = router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": png_hash}).to_string(),
            )
            .await
            .unwrap();
        let dims: serde_json::Value = serde_json::from_str(&dim_result).unwrap();
        assert_eq!(dims["width"], 10, "width should survive round-trip");
        assert_eq!(dims["height"], 10, "height should survive round-trip");
    }

    /// strip -> metadata -> verify no EXIF in stripped output
    #[cfg(feature = "media-metadata")]
    #[tokio::test]
    async fn pipeline_strip_then_metadata_no_exif() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        // Step 1: Strip metadata
        let strip_result = router
            .dispatch("nika:strip", serde_json::json!({"hash": hash}).to_string())
            .await
            .unwrap();
        let stripped: serde_json::Value = serde_json::from_str(&strip_result).unwrap();
        let stripped_hash = stripped["hash"].as_str().unwrap();
        assert_eq!(stripped["metadata"]["stripped"], true);

        // Step 2: Extract metadata from stripped output
        let meta_result = router
            .dispatch(
                "nika:metadata",
                serde_json::json!({"hash": stripped_hash}).to_string(),
            )
            .await
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&meta_result).unwrap();

        // Stripped PNG should have no EXIF data
        assert!(
            meta.get("exif").is_none() || meta["exif"].is_null(),
            "stripped image should have no EXIF, got: {:?}",
            meta.get("exif")
        );
        // But dimensions should be preserved
        assert_eq!(meta["width"], 100);
        assert_eq!(meta["height"], 50);
    }

    /// Store same data twice through different tools -> verify CAS dedup.
    #[tokio::test]
    async fn pipeline_same_cas_hash_dedup_across_tools() {
        let (_dir, router, ctx) = setup_router();
        let png = fixture_png_10x10_red();
        let hash = store_fixture(&ctx, &png).await;

        // Store the same data again — should be deduplicated
        let sr2 = ctx.cas.store(&png).await.unwrap();
        assert_eq!(sr2.hash, hash, "same data should produce same hash");
        assert!(
            sr2.deduplicated,
            "second store should be flagged as deduplicated"
        );

        // Two different tools reading the same hash should both succeed
        let dim_result = router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await;
        assert!(dim_result.is_ok(), "dimensions on deduped hash should work");

        let thumb_result = router
            .dispatch(
                "nika:thumbhash",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await;
        assert!(
            thumb_result.is_ok(),
            "thumbhash on deduped hash should work"
        );

        let color_result = router
            .dispatch(
                "nika:dominant_color",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await;
        assert!(
            color_result.is_ok(),
            "dominant_color on deduped hash should work"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // EXTENDED PIPELINE TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Full 4-step chain: convert(png->jpeg) -> thumbnail -> optimize is not
    /// possible (optimize requires PNG), so we verify the error path.
    #[cfg(feature = "media-optimize")]
    #[tokio::test]
    async fn pipeline_jpeg_through_optimize_rejected() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        // Step 1: Convert PNG -> JPEG
        let jpeg_result = router
            .dispatch(
                "nika:convert",
                serde_json::json!({"hash": hash, "format": "jpeg"}).to_string(),
            )
            .await
            .unwrap();
        let jpeg: serde_json::Value = serde_json::from_str(&jpeg_result).unwrap();
        let jpeg_hash = jpeg["hash"].as_str().unwrap();

        // Step 2: Try to optimize JPEG — should fail (optimize is PNG-only)
        let result = router
            .dispatch(
                "nika:optimize",
                serde_json::json!({"hash": jpeg_hash}).to_string(),
            )
            .await;

        assert!(result.is_err(), "optimizing JPEG should fail");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-291"),
            "should produce unsupported format error"
        );
    }

    /// thumbnail -> convert(png->webp) -> dimensions -> verify dimensions preserved
    #[tokio::test]
    async fn pipeline_thumbnail_convert_webp_dimensions() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        // Step 1: Thumbnail to 60px wide
        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": hash, "width": 60}).to_string(),
            )
            .await
            .unwrap();
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let thumb_hash = thumb["hash"].as_str().unwrap();

        // Step 2: Convert to WebP
        let webp_result = router
            .dispatch(
                "nika:convert",
                serde_json::json!({"hash": thumb_hash, "format": "webp"}).to_string(),
            )
            .await
            .unwrap();
        let webp: serde_json::Value = serde_json::from_str(&webp_result).unwrap();
        let webp_hash = webp["hash"].as_str().unwrap();
        assert_eq!(webp["mime_type"], "image/webp");

        // Step 3: Verify dimensions on WebP output
        let dim_result = router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": webp_hash}).to_string(),
            )
            .await
            .unwrap();
        let dims: serde_json::Value = serde_json::from_str(&dim_result).unwrap();
        assert_eq!(dims["width"], 60);
        assert_eq!(dims["height"], 30);
    }

    /// Run multiple tools concurrently on the same image — verify no race conditions.
    #[tokio::test]
    async fn pipeline_concurrent_tools_on_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let tool_ctx = Arc::new(ToolContext::new(
            dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        let media_ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let router = Arc::new(BuiltinToolRouter::with_all_tools(
            tool_ctx,
            Arc::clone(&media_ctx),
        ));
        let hash = store_fixture(&media_ctx, &fixture_png_100x50()).await;

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let router = Arc::clone(&router);
                let hash = hash.clone();
                tokio::spawn(async move {
                    match i % 3 {
                        0 => {
                            router
                                .dispatch(
                                    "nika:dimensions",
                                    serde_json::json!({"hash": hash}).to_string(),
                                )
                                .await
                        }
                        1 => {
                            router
                                .dispatch(
                                    "nika:thumbhash",
                                    serde_json::json!({"hash": hash}).to_string(),
                                )
                                .await
                        }
                        _ => {
                            router
                                .dispatch(
                                    "nika:dominant_color",
                                    serde_json::json!({"hash": hash}).to_string(),
                                )
                                .await
                        }
                    }
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        for (i, r) in results.iter().enumerate() {
            let inner = r.as_ref().unwrap();
            assert!(
                inner.is_ok(),
                "concurrent tool {} failed: {:?}",
                i,
                inner.as_ref().err()
            );
        }
    }

    /// Budget tracking across a multi-step pipeline.
    #[tokio::test]
    async fn pipeline_budget_tracks_across_steps() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;
        let budget_before = ctx.budget.current_bytes();

        // Thumbnail produces new binary output -> budget should increase
        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": hash, "width": 50}).to_string(),
            )
            .await
            .unwrap();

        let budget_after_thumb = ctx.budget.current_bytes();
        assert!(
            budget_after_thumb > budget_before,
            "budget should increase after thumbnail: {} vs {}",
            budget_after_thumb,
            budget_before
        );

        // Strip also produces new binary output -> budget should increase further
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let thumb_hash = thumb["hash"].as_str().unwrap();

        let _strip_result = router
            .dispatch(
                "nika:strip",
                serde_json::json!({"hash": thumb_hash}).to_string(),
            )
            .await
            .unwrap();

        let budget_after_strip = ctx.budget.current_bytes();
        assert!(
            budget_after_strip > budget_after_thumb,
            "budget should increase after strip: {} vs {}",
            budget_after_strip,
            budget_after_thumb
        );
    }

    /// Metadata-only tools (dimensions, thumbhash, dominant_color) do NOT increase CAS budget.
    #[tokio::test]
    async fn metadata_tools_do_not_increase_budget() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_10x10_red()).await;
        let budget_before = ctx.budget.current_bytes();

        // dimensions is metadata-only
        router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            ctx.budget.current_bytes(),
            budget_before,
            "dimensions should not change budget"
        );

        // thumbhash is metadata-only
        router
            .dispatch(
                "nika:thumbhash",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            ctx.budget.current_bytes(),
            budget_before,
            "thumbhash should not change budget"
        );

        // dominant_color is metadata-only
        router
            .dispatch(
                "nika:dominant_color",
                serde_json::json!({"hash": hash}).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(
            ctx.budget.current_bytes(),
            budget_before,
            "dominant_color should not change budget"
        );
    }

    /// Verify that errors propagate correctly through the router dispatch path.
    #[tokio::test]
    async fn router_error_propagation_missing_cas_hash() {
        let (_dir, router, _ctx) = setup_router();
        let fake_hash = "blake3:0000000000000000000000000000000000000000000000000000000000000000";

        // Every tool should produce a meaningful error for a missing hash
        let tools = vec![
            ("nika:dimensions", serde_json::json!({"hash": fake_hash})),
            (
                "nika:thumbnail",
                serde_json::json!({"hash": fake_hash, "width": 50}),
            ),
            ("nika:thumbhash", serde_json::json!({"hash": fake_hash})),
            (
                "nika:dominant_color",
                serde_json::json!({"hash": fake_hash}),
            ),
            (
                "nika:convert",
                serde_json::json!({"hash": fake_hash, "format": "jpeg"}),
            ),
            ("nika:strip", serde_json::json!({"hash": fake_hash})),
        ];

        for (tool_name, args) in tools {
            let result = router.dispatch(tool_name, args.to_string()).await;
            assert!(
                result.is_err(),
                "{} should fail with missing CAS hash",
                tool_name
            );
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("NIKA-253") || err.contains("not found") || err.contains("NIKA-290"),
                "{} error should mention missing hash: {}",
                tool_name,
                err
            );
        }
    }

    /// Verify that missing required parameters produce NIKA-294 across all tools.
    #[tokio::test]
    async fn router_missing_required_params_nika_294() {
        let (_dir, router, _ctx) = setup_router();

        // dimensions: missing "hash"
        let result = router.dispatch("nika:dimensions", "{}".to_string()).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "dimensions missing hash should be NIKA-294"
        );

        // thumbnail: missing "width"
        let result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": "blake3:abc"}).to_string(),
            )
            .await;
        assert!(result.is_err());
        // Could be NIKA-294 (missing width) or NIKA-253 (hash not found) depending on order
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NIKA-294") || err.contains("NIKA-253"),
            "should be a param or hash error: {}",
            err
        );

        // convert: missing "format"
        let result = router
            .dispatch(
                "nika:convert",
                serde_json::json!({"hash": "blake3:abc"}).to_string(),
            )
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NIKA-294") || err.contains("NIKA-253"),
            "convert missing format should error: {}",
            err
        );
    }

    /// Full pipeline: thumbnail(jpeg output) -> dimensions -> verify JPEG dimensions readable
    #[tokio::test]
    async fn pipeline_thumbnail_jpeg_output_readable_dimensions() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        // Thumbnail as JPEG
        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({
                  "hash": hash,
                  "width": 40,
                  "format": "jpeg"
                })
                .to_string(),
            )
            .await
            .unwrap();
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let thumb_hash = thumb["hash"].as_str().unwrap();
        assert_eq!(thumb["mime_type"], "image/jpeg");

        // Dimensions should still work on the JPEG output
        let dim_result = router
            .dispatch(
                "nika:dimensions",
                serde_json::json!({"hash": thumb_hash}).to_string(),
            )
            .await
            .unwrap();
        let dims: serde_json::Value = serde_json::from_str(&dim_result).unwrap();
        assert_eq!(dims["width"], 40);
        assert_eq!(dims["height"], 20);
    }

    /// Verify the CAS path stored in the router response actually exists on disk.
    #[tokio::test]
    async fn pipeline_cas_path_exists_on_disk() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let thumb_result = router
            .dispatch(
                "nika:thumbnail",
                serde_json::json!({"hash": hash, "width": 50}).to_string(),
            )
            .await
            .unwrap();
        let thumb: serde_json::Value = serde_json::from_str(&thumb_result).unwrap();
        let path_str = thumb["path"].as_str().unwrap();
        let path = std::path::Path::new(path_str);

        assert!(path.exists(), "CAS path should exist on disk: {}", path_str);
        // read_raw strips NK framing header so we get the original PNG bytes
        let on_disk = CasStore::read_raw(path).await.unwrap();
        assert!(!on_disk.is_empty(), "file on disk should not be empty");
        assert_eq!(
            &on_disk[..4],
            &[137, 80, 78, 71],
            "file on disk should be valid PNG"
        );
    }

    /// Verify deduplicated flag when producing identical thumbnails twice.
    #[tokio::test]
    async fn pipeline_duplicate_thumbnail_is_deduplicated() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash, "width": 50}).to_string();

        let result1 = router
            .dispatch("nika:thumbnail", args.clone())
            .await
            .unwrap();
        let r1: serde_json::Value = serde_json::from_str(&result1).unwrap();

        let result2 = router.dispatch("nika:thumbnail", args).await.unwrap();
        let r2: serde_json::Value = serde_json::from_str(&result2).unwrap();

        // Same input, same parameters => same hash
        assert_eq!(
            r1["hash"], r2["hash"],
            "identical thumbnails should produce same CAS hash"
        );
        // Second write should be deduplicated
        assert_eq!(
            r2["deduplicated"], true,
            "second identical write should be flagged as deduplicated"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 4. PR3b TOOL ROUTER DISPATCH TESTS
    // ═══════════════════════════════════════════════════════════════

    /// import dispatches through router (always-on, no feature gate)
    #[tokio::test]
    async fn dispatch_import_through_router() {
        let (_dir, router, _ctx) = setup_router();

        // Create a temp file with valid PNG data
        let png = fixture_png_10x10_red();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &png).unwrap();

        let args = serde_json::json!({"path": tmp.path().to_string_lossy()}).to_string();
        let result = router.dispatch("nika:import", args).await;

        assert!(result.is_ok(), "import dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
        assert_eq!(parsed["mime_type"], "image/png");
        assert!(parsed["size_bytes"].as_u64().unwrap() > 0);
    }

    /// chart dispatches through router (feature-gated)
    #[cfg(feature = "media-chart")]
    #[tokio::test]
    async fn dispatch_chart_through_router() {
        let (_dir, router, _ctx) = setup_router();

        let args = serde_json::json!({
          "type": "bar",
          "series": [{"name": "Revenue", "data": [10.0, 20.0, 30.0]}],
          "labels": ["Q1", "Q2", "Q3"]
        })
        .to_string();
        let result = router.dispatch("nika:chart", args).await;

        assert!(result.is_ok(), "chart dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
        assert_eq!(parsed["mime_type"], "image/png");
        assert_eq!(parsed["metadata"]["chart_type"], "bar");
    }

    /// provenance dispatches through router (feature-gated)
    #[cfg(feature = "media-provenance")]
    #[tokio::test]
    async fn dispatch_provenance_through_router() {
        let (_dir, router, ctx) = setup_router();

        // Store a JPEG for signing
        let jpeg = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(50u32, 50, Rgb([255u8, 0, 0]));
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
            buf.into_inner()
        };
        let hash = store_fixture(&ctx, &jpeg).await;

        let args = serde_json::json!({
          "hash": hash,
          "assertion": "ai.generated"
        })
        .to_string();
        let result = router.dispatch("nika:provenance", args).await;

        assert!(
            result.is_ok(),
            "provenance dispatch failed: {:?}",
            result.err()
        );
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
        assert_eq!(parsed["metadata"]["signed"], true);
        assert_eq!(parsed["metadata"]["assertion"], "ai.generated");
    }

    /// phash dispatches through router (feature-gated)
    #[cfg(feature = "media-phash")]
    #[tokio::test]
    async fn dispatch_phash_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash = store_fixture(&ctx, &fixture_png_100x50()).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:phash", args).await;

        assert!(result.is_ok(), "phash dispatch failed: {:?}", result.err());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["phash"].is_string());
        assert_eq!(parsed["algorithm"], "dct");
    }

    /// compare dispatches through router (feature-gated)
    #[cfg(feature = "media-phash")]
    #[tokio::test]
    async fn dispatch_compare_through_router() {
        let (_dir, router, ctx) = setup_router();
        let hash_a = store_fixture(&ctx, &fixture_png_100x50()).await;
        let hash_b = store_fixture(&ctx, &fixture_png_10x10_red()).await;

        let args = serde_json::json!({
          "hash_a": hash_a,
          "hash_b": hash_b
        })
        .to_string();
        let result = router.dispatch("nika:compare", args).await;

        assert!(
            result.is_ok(),
            "compare dispatch failed: {:?}",
            result.err()
        );
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["distance"].is_number());
        assert!(parsed["similarity_pct"].is_number());
    }

    /// pdf_extract dispatches through router (feature-gated)
    #[cfg(feature = "media-pdf")]
    #[tokio::test]
    async fn dispatch_pdf_extract_through_router() {
        let (_dir, router, ctx) = setup_router();

        // Minimal PDF
        let pdf = b"%PDF-1.0\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R/Resources<</Font<</F1 4 0 R>>>>/Contents 5 0 R>>endobj\n4 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n5 0 obj<</Length 44>>stream\nBT /F1 12 Tf 100 700 Td (Hello Nika!) Tj ET\nendstream\nendobj\nxref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \n0000000266 00000 n \n0000000340 00000 n \ntrailer<</Size 6/Root 1 0 R>>\nstartxref\n434\n%%EOF";
        let hash = store_fixture(&ctx, pdf).await;

        let args = serde_json::json!({"hash": hash}).to_string();
        let result = router.dispatch("nika:pdf_extract", args).await;

        // pdf-extract may succeed or fail on minimal PDF — either is acceptable
        match result {
            Ok(json_str) => {
                let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
                assert!(parsed["char_count"].is_number());
                assert!(parsed["word_count"].is_number());
            }
            Err(e) => {
                // Extraction error is OK, but it shouldn't panic
                assert!(
                    !e.to_string().contains("panicked"),
                    "pdf should not panic: {e}"
                );
            }
        }
    }
}
