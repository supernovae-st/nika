// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:quality — Image quality assessment via DSSIM (multi-scale SSIM).
//!
//! Compares two images (original vs processed) and returns a quality score.
//! Uses `dssim-core` for perceptual quality measurement.
//!
//! DSSIM score interpretation:
//! - 0.0 = identical images
//! - <0.01 = excellent (imperceptible difference)
//! - 0.01-0.02 = good (barely noticeable)
//! - 0.02-0.05 = acceptable (noticeable on close inspection)
//! - >0.05 = poor (clearly visible degradation)

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::safety::decode_image_safe;
use super::{MediaOp, MediaOpResult};

pub struct QualityOp;

/// Map DSSIM score to a human-readable quality grade.
fn quality_grade(dssim: f64) -> &'static str {
    if dssim < 0.01 {
        "excellent"
    } else if dssim < 0.02 {
        "good"
    } else if dssim < 0.05 {
        "acceptable"
    } else {
        "poor"
    }
}

impl MediaOp for QualityOp {
    fn name(&self) -> &'static str {
        "quality"
    }

    fn description(&self) -> &'static str {
        "Compare image quality between original and processed versions (DSSIM/SSIM)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "hash_a": {
                    "type": "string",
                    "description": "CAS hash of the original/reference image"
                },
                "hash_b": {
                    "type": "string",
                    "description": "CAS hash of the processed/test image"
                }
            },
            "required": ["hash_a", "hash_b"],
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

            let hash_a = args
                .get("hash_a")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("quality", "missing 'hash_a'"))?;
            let hash_b = args
                .get("hash_b")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("quality", "missing 'hash_b'"))?;

            let data_a = ctx.read_media(hash_a).await?;
            let data_b = ctx.read_media(hash_b).await?;

            // DSSIM comparison on compute pool (CPU-bound)
            let result = ctx
                .compute
                .compute(move || -> Result<serde_json::Value, MediaToolError> {
                    let img_a = decode_image_safe(&data_a)?;
                    let img_b = decode_image_safe(&data_b)?;

                    let rgba_a = img_a.to_rgba8();
                    let rgba_b = img_b.to_rgba8();

                    let (w_a, h_a) = (rgba_a.width() as usize, rgba_a.height() as usize);
                    let (w_b, h_b) = (rgba_b.width() as usize, rgba_b.height() as usize);

                    if w_a != w_b || h_a != h_b {
                        return Err(super::error::invalid_args(
                            "quality",
                            format!(
                                "Image dimensions must match: {}x{} vs {}x{}",
                                w_a, h_a, w_b, h_b
                            ),
                        ));
                    }

                    // Build DSSIM attribute images
                    // SAFETY: RGBA<u8> is #[repr(C)] with 4 bytes, same layout as [u8; 4]
                    let attr = dssim_core::Dssim::new();

                    let raw_a: &[u8] = rgba_a.as_raw();
                    let raw_b: &[u8] = rgba_b.as_raw();
                    let rgba_slice_a: &[rgb::RGBA<u8>] = rgb::AsPixels::as_pixels(raw_a);
                    let rgba_slice_b: &[rgb::RGBA<u8>] = rgb::AsPixels::as_pixels(raw_b);

                    let img_a_dssim =
                        attr.create_image_rgba(rgba_slice_a, w_a, h_a)
                            .ok_or_else(|| {
                                super::error::invalid_args(
                                    "quality",
                                    "Failed to create DSSIM image A",
                                )
                            })?;

                    let img_b_dssim =
                        attr.create_image_rgba(rgba_slice_b, w_b, h_b)
                            .ok_or_else(|| {
                                super::error::invalid_args(
                                    "quality",
                                    "Failed to create DSSIM image B",
                                )
                            })?;

                    let (dssim_val, _ssim_maps) = attr.compare(&img_a_dssim, img_b_dssim);
                    let dssim_f64: f64 = dssim_val.into();
                    let ssim = 1.0 / (1.0 + dssim_f64); // Approximate SSIM from DSSIM

                    let grade = quality_grade(dssim_f64);

                    Ok(serde_json::json!({
                        "dssim": dssim_f64,
                        "ssim": ssim,
                        "quality_grade": grade,
                        "dimensions": {
                            "width": w_a,
                            "height": h_a,
                        },
                    }))
                })
                .await??;

            Ok(MediaOpResult::Metadata(result))
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

    fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    #[tokio::test]
    async fn quality_identical_images() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 128, 128, 128);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = QualityOp;
        let result = op
            .execute(
                serde_json::json!({
                    "hash_a": sr.hash,
                    "hash_b": sr.hash,
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let dssim = v["dssim"].as_f64().unwrap();
            assert!(
                dssim < 0.001,
                "identical images should have DSSIM ~0, got {dssim}"
            );
            assert_eq!(v["quality_grade"], "excellent");
            let ssim = v["ssim"].as_f64().unwrap();
            assert!(
                ssim > 0.99,
                "identical images should have SSIM ~1.0, got {ssim}"
            );
        }
    }

    #[tokio::test]
    async fn quality_different_images() {
        let (_dir, ctx) = setup().await;
        let red = fixture_png(50, 50, 255, 0, 0);
        let blue = fixture_png(50, 50, 0, 0, 255);
        let sr_a = ctx.cas.store(&red).await.unwrap();
        let sr_b = ctx.cas.store(&blue).await.unwrap();

        let op = QualityOp;
        let result = op
            .execute(
                serde_json::json!({
                    "hash_a": sr_a.hash,
                    "hash_b": sr_b.hash,
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let dssim = v["dssim"].as_f64().unwrap();
            assert!(
                dssim > 0.01,
                "different images should have high DSSIM, got {dssim}"
            );
        }
    }

    #[tokio::test]
    async fn quality_slight_difference() {
        let (_dir, ctx) = setup().await;
        let a = fixture_png(50, 50, 128, 128, 128);
        let b = fixture_png(50, 50, 130, 128, 128); // slight red shift

        let sr_a = ctx.cas.store(&a).await.unwrap();
        let sr_b = ctx.cas.store(&b).await.unwrap();

        let op = QualityOp;
        let result = op
            .execute(
                serde_json::json!({
                    "hash_a": sr_a.hash,
                    "hash_b": sr_b.hash,
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let dssim = v["dssim"].as_f64().unwrap();
            // Slight difference: DSSIM should be small but non-zero
            assert!(
                dssim > 0.0,
                "slight difference should produce non-zero DSSIM"
            );
            assert!(
                dssim < 0.1,
                "slight difference should be small DSSIM, got {dssim}"
            );
        }
    }

    #[tokio::test]
    async fn quality_dimension_mismatch_error() {
        let (_dir, ctx) = setup().await;
        let small = fixture_png(50, 50, 128, 128, 128);
        let big = fixture_png(100, 100, 128, 128, 128);

        let sr_a = ctx.cas.store(&small).await.unwrap();
        let sr_b = ctx.cas.store(&big).await.unwrap();

        let op = QualityOp;
        let result = op
            .execute(
                serde_json::json!({
                    "hash_a": sr_a.hash,
                    "hash_b": sr_b.hash,
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err(), "dimension mismatch should error");
        assert!(result.unwrap_err().to_string().contains("dimensions"));
    }

    #[tokio::test]
    async fn quality_missing_hash_a() {
        let (_dir, ctx) = setup().await;
        let op = QualityOp;
        let result = op
            .execute(serde_json::json!({"hash_b": "blake3:abc"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn quality_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = QualityOp;
        let result = op
            .execute(serde_json::json!({"hash_a": "a", "hash_b": "b"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn quality_grade_boundaries() {
        assert_eq!(quality_grade(0.0), "excellent");
        assert_eq!(quality_grade(0.005), "excellent");
        assert_eq!(quality_grade(0.01), "good");
        assert_eq!(quality_grade(0.015), "good");
        assert_eq!(quality_grade(0.02), "acceptable");
        assert_eq!(quality_grade(0.04), "acceptable");
        assert_eq!(quality_grade(0.05), "poor");
        assert_eq!(quality_grade(0.1), "poor");
    }
}
