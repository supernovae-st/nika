// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:optimize — Lossless PNG optimization.
//!
//! Uses `oxipng` with parallel decompression.
//! Level 2 = default (100-500ms), level 6 = zopfli (up to 15s).

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error, unsupported_format};
use super::{MediaOp, MediaOpResult};

pub struct OptimizeOp;

impl MediaOp for OptimizeOp {
    fn name(&self) -> &'static str {
        "optimize"
    }

    fn description(&self) -> &'static str {
        "Lossless PNG optimization (reduce file size without quality loss)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the PNG image" },
            "level": { "type": "integer", "description": "Optimization level (1-6, default 2)", "minimum": 1, "maximum": 6, "default": 2 },
            "strip": { "type": "boolean", "description": "Strip non-essential metadata chunks", "default": true }
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
                .ok_or_else(|| invalid_args("optimize", "missing 'hash'"))?;
            let level = args
                .get("level")
                .and_then(|v| v.as_u64())
                .unwrap_or(2)
                .clamp(1, 6) as u8;
            let strip = args.get("strip").and_then(|v| v.as_bool()).unwrap_or(true);

            let data = ctx.read_media(hash).await?;

            // Verify it's a PNG
            if data.len() < 8 || data[..4] != [137, 80, 78, 71] {
                return Err(unsupported_format("optimize", "input is not a PNG image"));
            }

            let original_size = data.len();

            let optimized = ctx
                .compute
                .compute(move || -> Result<Vec<u8>, MediaToolError> {
                    let mut opts = oxipng::Options::from_preset(level);
                    if strip {
                        opts.strip = oxipng::StripChunks::Safe;
                    }
                    oxipng::optimize_from_memory(&data, &opts)
                        .map_err(|e| tool_error("optimize", format!("optimization failed: {e}")))
                })
                .await??;

            let optimized_size = optimized.len();
            let savings_pct = if original_size > 0 {
                ((original_size as f64 - optimized_size as f64) / original_size as f64 * 100.0)
                    .max(0.0)
            } else {
                0.0
            };

            Ok(MediaOpResult::Binary {
                data: optimized,
                mime_type: "image/png".to_string(),
                extension: "png".to_string(),
                metadata: serde_json::json!({
                  "original_size": original_size,
                  "optimized_size": optimized_size,
                  "savings_pct": (savings_pct * 10.0).round() / 10.0,
                  "level": level,
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
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_fn(50, 50, |x, y| {
            Rgb([(x * 5 % 256) as u8, (y * 5 % 256) as u8, 128])
        });
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            50,
            50,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        buf
    }

    #[tokio::test]
    async fn optimize_png_output_valid() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = OptimizeOp;
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
            // Valid PNG magic bytes
            assert_eq!(&data[..4], &[137, 80, 78, 71]);
            assert!(metadata["original_size"].as_u64().unwrap() > 0);
            assert!(metadata["optimized_size"].as_u64().unwrap() > 0);
        }
    }

    #[tokio::test]
    async fn optimize_jpeg_rejected() {
        let (_dir, ctx) = setup().await;
        // JPEG-like data (starts with FF D8)
        let jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        ];
        let sr = ctx.cas.store(&jpeg).await.unwrap();

        let op = OptimizeOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-291"));
    }

    #[tokio::test]
    async fn optimize_corrupt_png_no_panic() {
        let (_dir, ctx) = setup().await;
        // PNG magic + garbage
        let data = vec![137, 80, 78, 71, 13, 10, 26, 10, 0xFF, 0xFE, 0xFD];
        let sr = ctx.cas.store(&data).await.unwrap();

        let op = OptimizeOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        // Should error, not panic
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn optimize_output_is_decodable_png() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = OptimizeOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, metadata, .. } = result {
            // Verify output is a real decodable PNG
            let img = image::load_from_memory(&data).expect("optimized output must be decodable");
            assert_eq!(img.width(), 50);
            assert_eq!(img.height(), 50);
            // Verify savings_pct is a number (not string)
            assert!(
                metadata["savings_pct"].is_f64() || metadata["savings_pct"].is_u64(),
                "savings_pct should be numeric, got: {:?}",
                metadata["savings_pct"]
            );
        }
    }

    #[tokio::test]
    async fn optimize_preserves_dimensions() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let original_size = png.len();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = OptimizeOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "level": 2}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, metadata, .. } = result {
            assert!(
                data.len() <= original_size,
                "optimized should not be larger than original"
            );
            assert_eq!(
                metadata["original_size"].as_u64().unwrap(),
                original_size as u64
            );
        }
    }
}
