// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:pipeline — Chain multiple media operations in-memory.
//!
//! 1 CAS read → N in-memory transforms → 1 CAS write.
//! Budget charged once for the final output, not per step.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, pipeline_empty, pipeline_step_failed};
#[cfg(feature = "media-thumbnail")]
use super::safety::{decode_image_safe, MAX_IMAGE_DIM};
use super::{MediaOp, MediaOpResult};

/// Maximum number of pipeline steps to prevent CPU exhaustion.
const MAX_PIPELINE_STEPS: usize = 50;

pub struct PipelineOp;

impl MediaOp for PipelineOp {
    fn name(&self) -> &'static str {
        "pipeline"
    }

    fn description(&self) -> &'static str {
        "Chain multiple image operations in-memory (1 read → N transforms → 1 write)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the source image" },
            "steps": {
              "type": "array",
              "description": "Array of operations to apply in order",
              "items": {
                "type": "object",
                "properties": {
                  "op": { "type": "string", "enum": ["thumbnail", "optimize", "convert", "strip"] },
                  "width": { "type": "integer" },
                  "height": { "type": "integer" },
                  "format": { "type": "string" },
                  "level": { "type": "integer" },
                  "strip": { "type": "boolean" },
                  "quality": { "type": "integer" }
                },
                "required": ["op"]
              }
            }
          },
          "required": ["hash", "steps"],
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
                .ok_or_else(|| invalid_args("pipeline", "missing 'hash'"))?;
            let steps = args
                .get("steps")
                .and_then(|v| v.as_array())
                .ok_or_else(|| invalid_args("pipeline", "missing 'steps' array"))?;

            if steps.is_empty() {
                return Err(pipeline_empty());
            }

            if steps.len() > MAX_PIPELINE_STEPS {
                return Err(invalid_args(
                    "pipeline",
                    format!(
                        "too many steps ({}, max {})",
                        steps.len(),
                        MAX_PIPELINE_STEPS
                    ),
                ));
            }

            let data = ctx.read_media(hash).await?;
            let steps_owned: Vec<serde_json::Value> = steps.clone();

            let output = ctx
                .compute
                .compute(
                    move || -> Result<(Vec<u8>, String, String), MediaToolError> {
                        let mut current_data = data;
                        let mut current_mime = "image/png".to_string();
                        let mut current_ext = "png".to_string();

                        for (i, step) in steps_owned.iter().enumerate() {
                            let op = step
                                .get("op")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| pipeline_step_failed(i, "missing 'op' field"))?;

                            match op {
                                #[cfg(feature = "media-thumbnail")]
                                "thumbnail" => {
                                    let width =
                                        step.get("width").and_then(|v| v.as_u64()).unwrap_or(256);
                                    if width == 0 || width > MAX_IMAGE_DIM as u64 {
                                        return Err(pipeline_step_failed(
                                            i,
                                            format!("width must be 1..10000, got {width}"),
                                        ));
                                    }
                                    let width = width as u32;
                                    let target_height = step.get("height").and_then(|v| v.as_u64());
                                    if let Some(h) = target_height {
                                        if h == 0 || h > MAX_IMAGE_DIM as u64 {
                                            return Err(pipeline_step_failed(
                                                i,
                                                format!("height must be 1..10000, got {h}"),
                                            ));
                                        }
                                    }
                                    let img = decode_image_safe(&current_data)?;
                                    let (orig_w, orig_h) = (img.width(), img.height());
                                    let th = target_height.map(|h| h as u32).unwrap_or_else(|| {
                                        let ratio = orig_h as f64 / orig_w as f64;
                                        (width as f64 * ratio).round().max(1.0) as u32
                                    });
                                    let resized = img.resize_exact(
                                        width,
                                        th,
                                        image::imageops::FilterType::Lanczos3,
                                    );
                                    let mut buf = Vec::new();
                                    let rgba = resized.to_rgba8();
                                    let (w, h) = (rgba.width(), rgba.height());
                                    let enc = image::codecs::png::PngEncoder::new(&mut buf);
                                    image::ImageEncoder::write_image(
                                        enc,
                                        rgba.as_raw(),
                                        w,
                                        h,
                                        image::ExtendedColorType::Rgba8,
                                    )
                                    .map_err(|e| pipeline_step_failed(i, format!("encode: {e}")))?;
                                    current_data = buf;
                                    current_mime = "image/png".to_string();
                                    current_ext = "png".to_string();
                                }
                                #[cfg(feature = "media-optimize")]
                                "optimize" => {
                                    let level = step
                                        .get("level")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(2)
                                        .clamp(1, 6)
                                        as u8;
                                    let mut opts = oxipng::Options::from_preset(level);
                                    if step.get("strip").and_then(|v| v.as_bool()).unwrap_or(true) {
                                        opts.strip = oxipng::StripChunks::Safe;
                                    }
                                    current_data =
                                        oxipng::optimize_from_memory(&current_data, &opts)
                                            .map_err(|e| {
                                                pipeline_step_failed(i, format!("optimize: {e}"))
                                            })?;
                                }
                                #[cfg(feature = "media-thumbnail")]
                                "convert" => {
                                    let format = step
                                        .get("format")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("png");
                                    let img = decode_image_safe(&current_data)?;
                                    let (w, h) = (img.width(), img.height());
                                    let mut buf = Vec::new();
                                    match format {
                                        "jpeg" | "jpg" => {
                                            let quality = step
                                                .get("quality")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(85)
                                                .clamp(1, 100)
                                                as u8;
                                            let rgb = super::safety::composite_on_white(&img);
                                            let enc =
                                                image::codecs::jpeg::JpegEncoder::new_with_quality(
                                                    &mut buf, quality,
                                                );
                                            image::ImageEncoder::write_image(
                                                enc,
                                                rgb.as_raw(),
                                                w,
                                                h,
                                                image::ExtendedColorType::Rgb8,
                                            )
                                            .map_err(
                                                |e| {
                                                    pipeline_step_failed(
                                                        i,
                                                        format!("jpeg encode: {e}"),
                                                    )
                                                },
                                            )?;
                                            current_mime = "image/jpeg".to_string();
                                            current_ext = "jpg".to_string();
                                        }
                                        "webp" => {
                                            img.write_to(
                                                &mut std::io::Cursor::new(&mut buf),
                                                image::ImageFormat::WebP,
                                            )
                                            .map_err(
                                                |e| {
                                                    pipeline_step_failed(
                                                        i,
                                                        format!("webp encode: {e}"),
                                                    )
                                                },
                                            )?;
                                            current_mime = "image/webp".to_string();
                                            current_ext = "webp".to_string();
                                        }
                                        _ => {
                                            let rgba = img.to_rgba8();
                                            let enc = image::codecs::png::PngEncoder::new(&mut buf);
                                            image::ImageEncoder::write_image(
                                                enc,
                                                rgba.as_raw(),
                                                w,
                                                h,
                                                image::ExtendedColorType::Rgba8,
                                            )
                                            .map_err(
                                                |e| {
                                                    pipeline_step_failed(
                                                        i,
                                                        format!("png encode: {e}"),
                                                    )
                                                },
                                            )?;
                                            current_mime = "image/png".to_string();
                                            current_ext = "png".to_string();
                                        }
                                    }
                                    current_data = buf;
                                }
                                #[cfg(feature = "media-thumbnail")]
                                "strip" => {
                                    let img = decode_image_safe(&current_data)?;
                                    let (w, h) = (img.width(), img.height());
                                    let mut buf = Vec::new();
                                    let rgba = img.to_rgba8();
                                    let enc = image::codecs::png::PngEncoder::new(&mut buf);
                                    image::ImageEncoder::write_image(
                                        enc,
                                        rgba.as_raw(),
                                        w,
                                        h,
                                        image::ExtendedColorType::Rgba8,
                                    )
                                    .map_err(|e| {
                                        pipeline_step_failed(i, format!("strip encode: {e}"))
                                    })?;
                                    current_data = buf;
                                }
                                other => {
                                    return Err(pipeline_step_failed(
                                        i,
                                        format!("unknown operation: {other}"),
                                    ));
                                }
                            }
                        }

                        Ok((current_data, current_mime, current_ext))
                    },
                )
                .await??;

            let (data, mime_type, extension) = output;

            // Charge budget for the final output (was missing — bypassed budget accounting)
            ctx.budget
                .check_and_add(data.len() as u64, "pipeline")
                .map_err(|e| -> MediaToolError { e.into() })?;

            Ok(MediaOpResult::Binary {
                data,
                mime_type,
                extension,
                metadata: serde_json::json!({
                  "steps_completed": steps.len(),
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

    #[cfg(feature = "media-thumbnail")]
    fn fixture_png() -> Vec<u8> {
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
    async fn pipeline_thumbnail_then_convert() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 50 },
                    { "op": "convert", "format": "jpeg" }
                  ]
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
            assert_eq!(mime_type, "image/jpeg");
            assert_eq!(&data[..2], &[0xFF, 0xD8]); // JPEG magic
            assert_eq!(metadata["steps_completed"], 2);
        }
    }

    #[tokio::test]
    async fn pipeline_empty_steps_error() {
        let (_dir, ctx) = setup().await;
        let data = b"some data for CAS";
        let sr = ctx.cas.store(data).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": []
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-296"));
    }

    #[tokio::test]
    async fn pipeline_unknown_op_error() {
        let (_dir, ctx) = setup().await;
        let data = b"some data";
        let sr = ctx.cas.store(data).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [{ "op": "nonexistent" }]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-295"));
    }

    #[tokio::test]
    async fn pipeline_missing_params() {
        let (_dir, ctx) = setup().await;
        let op = PipelineOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn pipeline_rejects_too_many_steps() {
        let (_dir, ctx) = setup().await;
        let data = b"some data for CAS";
        let sr = ctx.cas.store(data).await.unwrap();

        // Build 51 steps (over MAX_PIPELINE_STEPS=50)
        let steps: Vec<serde_json::Value> = (0..51)
            .map(|_| serde_json::json!({"op": "thumbnail", "width": 100}))
            .collect();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": steps
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "51 steps must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NIKA-294"),
            "should be invalid args error, got: {err}"
        );
        assert!(err.contains("too many steps"));
    }

    #[test]
    fn pipeline_step_limit_is_50() {
        assert_eq!(MAX_PIPELINE_STEPS, 50);
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug 22 regression: pipeline thumbnail must use height param
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug22_pipeline_thumbnail_respects_height() {
        // Bug 22: Pipeline thumbnail used resize(w, w) ignoring the height param.
        // Now it parses height and uses resize_exact(w, h).
        let (_dir, ctx) = setup().await;
        let png = fixture_png(); // 100x50
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 60, "height": 30 }
                  ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            let img = image::load_from_memory(&data).unwrap();
            assert_eq!(
                img.width(),
                60,
                "Bug 22 regression: pipeline thumbnail width must be respected"
            );
            assert_eq!(
                img.height(),
                30,
                "Bug 22 regression: pipeline thumbnail height must be respected"
            );
        } else {
            panic!("expected Binary result");
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug22_pipeline_thumbnail_aspect_ratio_without_height() {
        // When height is not specified, aspect ratio must be preserved.
        let (_dir, ctx) = setup().await;
        let png = fixture_png(); // 100x50
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 50 }
                  ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            let img = image::load_from_memory(&data).unwrap();
            assert_eq!(img.width(), 50);
            assert_eq!(
                img.height(),
                25,
                "Bug 22 regression: without height, aspect ratio must be preserved (100:50 = 50:25)"
            );
        } else {
            panic!("expected Binary result");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug 38 regression: pipeline thumbnail must reject out-of-range
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug38_pipeline_thumbnail_rejects_zero_width() {
        // Bug 38: Pipeline thumbnail silently clamped out-of-range values
        // instead of returning an error.
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 0 }
                  ]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "width=0 must be rejected, not clamped");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-295"),
            "must be a pipeline step error"
        );
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug38_pipeline_thumbnail_rejects_width_over_limit() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 20000 }
                  ]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "width=20000 must be rejected, not clamped");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-295"),
            "must be a pipeline step error"
        );
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug38_pipeline_thumbnail_rejects_zero_height() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 50, "height": 0 }
                  ]
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "height=0 must be rejected, not clamped");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-295"),
            "must be a pipeline step error"
        );
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn bug38_pipeline_thumbnail_rejects_height_over_limit() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PipelineOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "steps": [
                    { "op": "thumbnail", "width": 50, "height": 20000 }
                  ]
                }),
                &ctx,
            )
            .await;
        assert!(
            result.is_err(),
            "height=20000 must be rejected, not clamped"
        );
        assert!(
            result.unwrap_err().to_string().contains("NIKA-295"),
            "must be a pipeline step error"
        );
    }
}
