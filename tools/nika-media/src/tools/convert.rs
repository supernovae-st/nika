// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:convert — Format conversion (PNG↔JPEG↔WebP).
//!
//! Transparent PNG → JPEG composites on white background.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error, unsupported_format};
use super::safety::{composite_on_white, decode_image_safe};
use super::{MediaOp, MediaOpResult};

pub struct ConvertOp;

impl MediaOp for ConvertOp {
    fn name(&self) -> &'static str {
        "convert"
    }

    fn description(&self) -> &'static str {
        "Convert image between formats (PNG, JPEG, WebP)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the source image" },
            "format": { "type": "string", "enum": ["png", "jpeg", "webp"], "description": "Target format" },
            "quality": { "type": "integer", "description": "JPEG quality (1-100, default 85)", "default": 85 }
          },
          "required": ["hash", "format"],
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
                .ok_or_else(|| invalid_args("convert", "missing 'hash'"))?;
            let format = args
                .get("format")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("convert", "missing 'format'"))?;
            let quality = args
                .get("quality")
                .and_then(|v| v.as_u64())
                .unwrap_or(85)
                .clamp(1, 100) as u8;

            let data = ctx.read_media(hash).await?;
            let format_owned = format.to_string();

            let output = ctx
                .compute
                .compute(
                    move || -> Result<(Vec<u8>, String, String), MediaToolError> {
                        let img = decode_image_safe(&data)?;
                        let (w, h) = (img.width(), img.height());
                        let mut buf = Vec::new();

                        let (mime, ext) = match format_owned.as_str() {
                            "jpeg" | "jpg" => {
                                // SAFETY: to_rgb8() silently drops alpha — RGBA(255,0,0,0) becomes
                                // RGB(255,0,0). We must composite on white before JPEG encoding.
                                let rgb = composite_on_white(&img);
                                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                    &mut buf, quality,
                                );
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgb.as_raw(),
                                    w,
                                    h,
                                    image::ExtendedColorType::Rgb8,
                                )
                                .map_err(|e| tool_error("convert", format!("JPEG encode: {e}")))?;
                                ("image/jpeg", "jpg")
                            }
                            "webp" => {
                                img.write_to(
                                    &mut std::io::Cursor::new(&mut buf),
                                    image::ImageFormat::WebP,
                                )
                                .map_err(|e| tool_error("convert", format!("WebP encode: {e}")))?;
                                ("image/webp", "webp")
                            }
                            "png" => {
                                let rgba = img.to_rgba8();
                                let encoder = image::codecs::png::PngEncoder::new(&mut buf);
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgba.as_raw(),
                                    w,
                                    h,
                                    image::ExtendedColorType::Rgba8,
                                )
                                .map_err(|e| tool_error("convert", format!("PNG encode: {e}")))?;
                                ("image/png", "png")
                            }
                            other => return Err(unsupported_format("convert", other)),
                        };

                        Ok((buf, mime.to_string(), ext.to_string()))
                    },
                )
                .await??;

            let (buf, mime_type, extension) = output;

            Ok(MediaOpResult::Binary {
                data: buf,
                mime_type,
                extension,
                metadata: serde_json::json!({
                  "converted_to": format,
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

    fn fixture_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_pixel(10, 10, Rgba([255u8, 0, 0, 128]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[tokio::test]
    async fn convert_png_to_jpeg() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg"
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

    #[tokio::test]
    async fn convert_png_to_jpeg_decodable() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            let img = image::load_from_memory(&data).expect("JPEG output must be decodable");
            assert_eq!(img.width(), 10);
            assert_eq!(img.height(), 10);
        }
    }

    #[tokio::test]
    async fn convert_jpeg_to_png() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        // First convert to JPEG
        let op = ConvertOp;
        let jpeg_result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let jpeg_hash = if let MediaOpResult::Binary { data, .. } = &jpeg_result {
            ctx.cas.store(data).await.unwrap().hash
        } else {
            panic!("expected Binary");
        };

        // Then convert back to PNG
        let png_result = op
            .execute(
                serde_json::json!({
                  "hash": jpeg_hash, "format": "png"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data, mime_type, ..
        } = png_result
        {
            assert_eq!(mime_type, "image/png");
            assert_eq!(&data[..4], &[137, 80, 78, 71]); // PNG magic
            let img = image::load_from_memory(&data).expect("PNG must be decodable");
            assert_eq!(img.width(), 10);
        }
    }

    #[tokio::test]
    async fn convert_corrupt_input_no_panic() {
        let (_dir, ctx) = setup().await;
        for i in 1..30u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let op = ConvertOp;
                let _ = op
                    .execute(serde_json::json!({"hash": sr.hash, "format": "png"}), &ctx)
                    .await;
            }
        }
    }

    #[tokio::test]
    async fn convert_transparent_png_to_jpeg_white_background() {
        // CRITICAL: transparent PNG → JPEG must composite on white, not just drop alpha
        let (_dir, ctx) = setup().await;
        // Fully transparent red pixel: RGBA(255, 0, 0, 0)
        let img = image::ImageBuffer::from_pixel(10, 10, image::Rgba([255u8, 0, 0, 0]));
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
        let sr = ctx.cas.store(&buf).await.unwrap();

        let op = ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            let output = image::load_from_memory(&data).unwrap().to_rgb8();
            let pixel = output.get_pixel(5, 5);
            // Fully transparent → should be white (255,255,255), NOT red (255,0,0)
            assert!(pixel[0] > 250, "R should be ~255 (white), got {}", pixel[0]);
            assert!(pixel[1] > 250, "G should be ~255 (white), got {}", pixel[1]);
            assert!(pixel[2] > 250, "B should be ~255 (white), got {}", pixel[2]);
        }
    }

    #[tokio::test]
    async fn convert_semitransparent_png_to_jpeg_blends() {
        // Semi-transparent red RGBA(255,0,0,128) on white → ~RGB(255,128,128)
        let (_dir, ctx) = setup().await;
        let png = fixture_png(); // Uses RGBA(255,0,0,128)
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            let output = image::load_from_memory(&data).unwrap().to_rgb8();
            let pixel = output.get_pixel(5, 5);
            // 50% alpha red on white → approximately (255, 128, 128) ± JPEG compression
            assert!(pixel[0] > 200, "R should be high (~255), got {}", pixel[0]);
            assert!(
                pixel[1] > 90 && pixel[1] < 180,
                "G should be ~128, got {}",
                pixel[1]
            );
            assert!(
                pixel[2] > 90 && pixel[2] < 180,
                "B should be ~128, got {}",
                pixel[2]
            );
        }
    }

    #[tokio::test]
    async fn convert_missing_format() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ConvertOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }
}
