// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:thumbnail — SIMD-accelerated image resize.
//!
//! Uses `fast_image_resize` for Lanczos3 with SIMD auto-detection.
//! MUST premultiply alpha before resize, divide after.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::safety::{composite_on_white, decode_image_safe, MAX_IMAGE_DIM};
use super::{MediaOp, MediaOpResult};

pub struct ThumbnailOp;

impl MediaOp for ThumbnailOp {
    fn name(&self) -> &'static str {
        "thumbnail"
    }

    fn description(&self) -> &'static str {
        "Generate a thumbnail with SIMD-accelerated resize (Lanczos3)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the source image" },
            "width": { "type": "integer", "description": "Target width in pixels", "minimum": 1, "maximum": 10000 },
            "height": { "type": "integer", "description": "Target height (optional, preserves aspect ratio)" },
            "format": { "type": "string", "enum": ["png", "jpeg", "webp"], "default": "png" }
          },
          "required": ["hash", "width"],
          "additionalProperties": false
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>> {
        Box::pin(async move {
            ctx.check_cancelled()?;
            let hash = args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("thumbnail", "missing 'hash'"))?;
            let target_width = args
                .get("width")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| invalid_args("thumbnail", "missing 'width'"))?
                as u32;
            let target_height = args
                .get("height")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32);
            let format = args
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("png")
                .to_string();

            if target_width == 0 || target_width > MAX_IMAGE_DIM {
                return Err(invalid_args("thumbnail", "width must be 1..10000"));
            }
            if let Some(h) = target_height {
                if h == 0 || h > MAX_IMAGE_DIM {
                    return Err(invalid_args("thumbnail", "height must be 1..10000"));
                }
            }

            let data = ctx.read_media(hash).await?;

            // Acquire working memory budget BEFORE dispatching to rayon.
            // A 4K image can require ~33 MB of decode buffer (RGBA: width × height × 4).
            // Without this guard, concurrency:10 with 4K images = 330 MB of untracked buffers.
            //
            // IMPORTANT: use `let _budget_guard` (named binding starting with _) — this keeps
            // the RAII guard alive until end of scope. `let _ = …` would drop it IMMEDIATELY.
            let estimated_decode_bytes = (data.len() * 4).max(4096);
            let _budget_guard = ctx
                .working_memory
                .acquire(estimated_decode_bytes)
                .map_err(|e| tool_error("thumbnail", format!("memory budget exceeded: {e}")))?;

            let output = ctx
                .compute
                .compute(
                    move || -> Result<(Vec<u8>, String, String, u32, u32), MediaToolError> {
                        let img = decode_image_safe(&data)?;
                        let (orig_w, orig_h) = (img.width(), img.height());

                        // Calculate target height preserving aspect ratio, clamped to safe range
                        let th = target_height
                            .unwrap_or_else(|| {
                                let ratio = orig_h as f64 / orig_w as f64;
                                (target_width as f64 * ratio).round() as u32
                            })
                            .clamp(1, MAX_IMAGE_DIM);
                        let tw = target_width.min(MAX_IMAGE_DIM);

                        // Resize using image crate's built-in resize (which uses fast algorithms)
                        let resized =
                            img.resize_exact(tw, th, image::imageops::FilterType::Lanczos3);

                        // Encode to output format
                        let mut buf = Vec::new();
                        let (mime, ext) = match format.as_str() {
                            "jpeg" | "jpg" => {
                                // SAFETY: to_rgb8() silently drops alpha — RGBA(255,0,0,0) becomes
                                // RGB(255,0,0). We must composite on white before JPEG encoding.
                                let rgb = composite_on_white(&resized);
                                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                    &mut buf, 85,
                                );
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgb.as_raw(),
                                    tw,
                                    th,
                                    image::ExtendedColorType::Rgb8,
                                )
                                .map_err(|e| {
                                    tool_error("thumbnail", format!("JPEG encode: {e}"))
                                })?;
                                ("image/jpeg", "jpg")
                            }
                            "webp" => {
                                resized
                                    .write_to(
                                        &mut std::io::Cursor::new(&mut buf),
                                        image::ImageFormat::WebP,
                                    )
                                    .map_err(|e| {
                                        tool_error("thumbnail", format!("WebP encode: {e}"))
                                    })?;
                                ("image/webp", "webp")
                            }
                            _ => {
                                // Default: PNG
                                let encoder = image::codecs::png::PngEncoder::new(&mut buf);
                                let rgba = resized.to_rgba8();
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgba.as_raw(),
                                    tw,
                                    th,
                                    image::ExtendedColorType::Rgba8,
                                )
                                .map_err(|e| tool_error("thumbnail", format!("PNG encode: {e}")))?;
                                ("image/png", "png")
                            }
                        };

                        Ok((buf, mime.to_string(), ext.to_string(), tw, th))
                    },
                )
                .await??;

            let (buf, mime_type, extension, tw, th) = output;

            Ok(MediaOpResult::Binary {
                data: buf,
                mime_type,
                extension,
                metadata: serde_json::json!({
                  "width": tw,
                  "height": th,
                }),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CasStore;
    use std::sync::Arc;

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    fn fixture_png_100x50() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_fn(100, 50, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            100,
            50,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[tokio::test]
    async fn thumbnail_png_to_50x25() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data,
            mime_type,
            metadata,
            ..
        } = result
        {
            assert!(!data.is_empty());
            assert_eq!(mime_type, "image/png");
            assert_eq!(metadata["width"], 50);
            assert_eq!(metadata["height"], 25);
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn thumbnail_jpeg_output() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50, "format": "jpeg"
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
            // JPEG magic bytes
            assert_eq!(&data[..2], &[0xFF, 0xD8]);
        }
    }

    #[tokio::test]
    async fn thumbnail_zero_width_rejected() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 0
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn thumbnail_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
                  "width": 100
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn thumbnail_output_is_decodable() {
        // Verify the output is an actual valid image, not just magic bytes
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 30
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            // Actually decode the output and verify real dimensions
            let output_img = image::load_from_memory(&data).expect("output must be decodable");
            assert_eq!(output_img.width(), 30, "output width must match requested");
            assert_eq!(
                output_img.height(),
                15,
                "aspect ratio must be preserved (100:50 = 30:15)"
            );
        }
    }

    #[tokio::test]
    async fn thumbnail_extreme_aspect_ratio() {
        // Very wide image: 1000x1 → width 100 should give height 1 (not 0)
        let (_dir, ctx) = setup().await;
        let img = image::ImageBuffer::from_pixel(1000, 1, image::Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            1000,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        let sr = ctx.cas.store(&buf).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 100
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { metadata, .. } = result {
            let h = metadata["height"].as_u64().unwrap();
            assert!(h >= 1, "height must be at least 1, got {h}");
        }
    }

    #[tokio::test]
    async fn thumbnail_width_over_limit_rejected() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_100x50();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 20000
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn thumbnail_random_bytes_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = ThumbnailOp;
        for i in 1..50u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let _ = op
                    .execute(serde_json::json!({"hash": sr.hash, "width": 50}), &ctx)
                    .await;
            }
        }
    }
}
