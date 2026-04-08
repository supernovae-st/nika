// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:strip — Remove metadata from images.
//!
//! Decode → re-encode strips all EXIF, GPS, camera info.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::safety::{composite_on_white, decode_image_safe};
use super::{MediaOp, MediaOpResult};

pub struct StripOp;

impl MediaOp for StripOp {
    fn name(&self) -> &'static str {
        "strip"
    }

    fn description(&self) -> &'static str {
        "Remove all metadata (EXIF, GPS, camera info) from an image"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the image" },
            "format": { "type": "string", "enum": ["png", "jpeg"], "description": "Output format (default: same as input)" }
          },
          "required": ["hash"],
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
                .ok_or_else(|| invalid_args("strip", "missing 'hash'"))?;
            let format_override = args
                .get("format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let data = ctx.read_media(hash).await?;

            let output = ctx
                .compute
                .compute(
                    move || -> Result<(Vec<u8>, String, String), MediaToolError> {
                        let img = decode_image_safe(&data)?;
                        let (w, h) = (img.width(), img.height());

                        // Detect original format or use override
                        let format = format_override.as_deref().unwrap_or_else(|| {
                            if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
                                "jpeg"
                            } else {
                                "png"
                            }
                        });

                        let mut buf = Vec::new();
                        let (mime, ext) = match format {
                            "jpeg" | "jpg" => {
                                // SAFETY: to_rgb8() silently drops alpha — RGBA(255,0,0,0) becomes
                                // RGB(255,0,0). We must composite on white before JPEG encoding.
                                let rgb = composite_on_white(&img);
                                // Quality 100 minimizes re-encoding loss — strip should not degrade image data
                                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                    &mut buf, 100,
                                );
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgb.as_raw(),
                                    w,
                                    h,
                                    image::ExtendedColorType::Rgb8,
                                )
                                .map_err(|e| tool_error("strip", format!("encode: {e}")))?;
                                ("image/jpeg", "jpg")
                            }
                            _ => {
                                let rgba = img.to_rgba8();
                                let encoder = image::codecs::png::PngEncoder::new(&mut buf);
                                image::ImageEncoder::write_image(
                                    encoder,
                                    rgba.as_raw(),
                                    w,
                                    h,
                                    image::ExtendedColorType::Rgba8,
                                )
                                .map_err(|e| tool_error("strip", format!("encode: {e}")))?;
                                ("image/png", "png")
                            }
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
                  "stripped": true,
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
        let img = ImageBuffer::from_pixel(5, 5, Rgba([100u8, 200, 50, 255]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            5,
            5,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[tokio::test]
    async fn strip_png_produces_output() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = StripOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data,
            mime_type,
            metadata,
            ..
        } = result
        {
            assert_eq!(mime_type, "image/png");
            assert!(!data.is_empty());
            assert_eq!(metadata["stripped"], true);
        }
    }

    #[tokio::test]
    async fn strip_to_jpeg() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = StripOp;
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
            assert_eq!(&data[..2], &[0xFF, 0xD8]);
        }
    }
}
