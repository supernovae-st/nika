// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:qr_validate — Decode QR codes and compute a scan quality score.
//!
//! Uses `qrcode-ai-scanner-core` for robust QR decoding with multi-decoder
//! strategy (rxing + rqrr) and 4-tier brute-force preprocessing that handles
//! artistic/styled QR codes standard decoders fail on.
//!
//! Returns decoded data, QR metadata (version, EC level, modules), and a
//! weighted 0-100 scannability score from stress tests (blur, downscale, contrast).
//!
//! SECURITY:
//! - Decoded QR data is returned as a string — never executed.
//! - Images decoded via `decode_image_safe()` with Nika resource limits.
//! - Then passed to `multi_decode_image()` / `run_fast_stress_tests()`.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::safety::decode_image_safe;
use super::{MediaOp, MediaOpResult};

pub struct QrValidateOp;

impl MediaOp for QrValidateOp {
    fn name(&self) -> &'static str {
        "qr_validate"
    }

    fn description(&self) -> &'static str {
        "Decode QR codes from an image and compute scan quality score (0-100) via stress tests"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "CAS hash of the image containing QR code(s)"
                },
                "fast": {
                    "type": "boolean",
                    "description": "Use fast mode (2 stress tests instead of 6, ~2-3x faster)",
                    "default": true
                }
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
                .ok_or_else(|| invalid_args("qr_validate", "missing 'hash'"))?;

            let fast = args.get("fast").and_then(|v| v.as_bool()).unwrap_or(true);

            let data = ctx.read_media(hash).await?;

            // Decode + validate on compute pool (CPU-bound: image decode, multi-decoder, stress tests)
            let result = ctx
                .compute
                .compute(move || -> Result<serde_json::Value, MediaToolError> {
                    // Phase 1: Safe decode with Nika resource limits (10k x 10k, 256MB)
                    let img = decode_image_safe(&data)?;

                    // Phase 2: Multi-decoder with 4-tier preprocessing (rxing + rqrr)
                    let decode_result = qrcode_ai_scanner_core::decoder::multi_decode_image(&img);

                    match decode_result {
                        Ok(decode) => {
                            // Phase 3: Stress tests for scannability score
                            let stress = if fast {
                                qrcode_ai_scanner_core::scorer::run_fast_stress_tests(&img)
                            } else {
                                qrcode_ai_scanner_core::scorer::run_stress_tests_on_image(&img)
                            };

                            let num_decoders = decode
                                .metadata
                                .as_ref()
                                .map(|m| m.decoders_success.len())
                                .unwrap_or(1);

                            let score = match stress {
                                Ok(ref s) if fast => {
                                    qrcode_ai_scanner_core::scorer::calculate_fast_score(
                                        s,
                                        num_decoders,
                                    )
                                }
                                Ok(ref s) => {
                                    qrcode_ai_scanner_core::scorer::calculate_score(s, num_decoders)
                                }
                                Err(_) => 50, // Decode succeeded but stress tests failed — partial score
                            };

                            let stress_json = stress.as_ref().ok().map(|s| {
                                serde_json::json!({
                                    "original": s.original,
                                    "downscale_50": s.downscale_50,
                                    "downscale_25": s.downscale_25,
                                    "blur_light": s.blur_light,
                                    "blur_medium": s.blur_medium,
                                    "low_contrast": s.low_contrast,
                                })
                            });

                            let metadata_json = decode.metadata.as_ref().map(|m| {
                                serde_json::json!({
                                    "version": m.version,
                                    "error_correction": format!("{:?}", m.error_correction),
                                    "modules": m.modules,
                                    "decoders_success": m.decoders_success,
                                })
                            });

                            let ec = decode
                                .metadata
                                .as_ref()
                                .map(|m| format!("{:?}", m.error_correction))
                                .unwrap_or_default();

                            Ok(serde_json::json!({
                                "decoded": true,
                                "data": decode.content,
                                "error_correction": ec,
                                "scan_score": score,
                                "metadata": metadata_json,
                                "stress_results": stress_json,
                            }))
                        }
                        Err(_) => {
                            // QR decode failed entirely
                            Ok(serde_json::json!({
                                "decoded": false,
                                "data": null,
                                "error_correction": null,
                                "scan_score": 0,
                                "metadata": null,
                                "stress_results": null,
                            }))
                        }
                    }
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

    /// Generate a clean QR code PNG using the `qrcode` crate (dev-dependency).
    fn fixture_qr_png(text: &str) -> Vec<u8> {
        use image::{ImageEncoder, Luma};

        let code = qrcode::QrCode::new(text.as_bytes()).expect("QR encode should work");
        let module_count = code.width() as u32;

        // Scale up: each module = 8 pixels for reliable scanning
        let scale = 8u32;
        let quiet = 4u32; // quiet zone in modules
        let img_size = (module_count + quiet * 2) * scale;
        let mut img = image::GrayImage::from_pixel(img_size, img_size, Luma([255u8]));

        for y in 0..module_count {
            for x in 0..module_count {
                if code[(x as usize, y as usize)] == qrcode::types::Color::Dark {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            img.put_pixel(
                                (x + quiet) * scale + dx,
                                (y + quiet) * scale + dy,
                                Luma([0u8]),
                            );
                        }
                    }
                }
            }
        }

        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder
            .write_image(
                img.as_raw(),
                img_size,
                img_size,
                image::ExtendedColorType::L8,
            )
            .unwrap();
        buf
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
    async fn qr_decode_valid_qr() {
        let (_dir, ctx) = setup().await;
        let qr_png = fixture_qr_png("https://qrcode-ai.com/test");
        let sr = ctx.cas.store(&qr_png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["decoded"], true);
            assert_eq!(v["data"], "https://qrcode-ai.com/test");
            assert!(v["scan_score"].as_u64().unwrap() > 0, "score should be > 0");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn qr_decode_returns_metadata() {
        let (_dir, ctx) = setup().await;
        let qr_png = fixture_qr_png("hello");
        let sr = ctx.cas.store(&qr_png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert!(v["metadata"].is_object(), "metadata should be present");
            let meta = &v["metadata"];
            assert!(meta["version"].is_number(), "QR version should be present");
            assert!(meta["error_correction"].is_string(), "EC should be present");
        }
    }

    #[tokio::test]
    async fn qr_decode_returns_stress_results() {
        let (_dir, ctx) = setup().await;
        let qr_png = fixture_qr_png("stress-test");
        let sr = ctx.cas.store(&qr_png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "fast": true}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert!(
                v["stress_results"].is_object(),
                "stress_results should be present"
            );
        }
    }

    #[tokio::test]
    async fn qr_not_a_qr_returns_decoded_false() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(100, 100, 255, 0, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["decoded"], false);
            assert_eq!(v["scan_score"], 0);
        }
    }

    #[tokio::test]
    async fn qr_missing_hash_param() {
        let (_dir, ctx) = setup().await;
        let op = QrValidateOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn qr_nonexistent_hash() {
        let (_dir, ctx) = setup().await;
        let op = QrValidateOp;
        let result = op.execute(
            serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}),
            &ctx,
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn qr_non_image_data() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(b"this is not an image").await.unwrap();
        let op = QrValidateOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err(), "non-image data should error");
    }

    #[tokio::test]
    async fn qr_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": "blake3:abc"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn qr_fuzz_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = QrValidateOp;
        for i in 1..20u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "fuzz input {i} panicked"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn qr_score_clean_qr_is_high() {
        let (_dir, ctx) = setup().await;
        let qr_png = fixture_qr_png("high-quality");
        let sr = ctx.cas.store(&qr_png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            let score = v["scan_score"].as_u64().unwrap();
            assert!(score >= 50, "clean QR should score >= 50, got {score}");
        }
    }

    #[tokio::test]
    async fn qr_full_mode_works() {
        let (_dir, ctx) = setup().await;
        let qr_png = fixture_qr_png("full-mode-test");
        let sr = ctx.cas.store(&qr_png).await.unwrap();

        let op = QrValidateOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "fast": false}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["decoded"], true);
            let stress = &v["stress_results"];
            // Full mode should have all 6 stress test fields
            assert!(stress["original"].is_boolean());
            assert!(stress["blur_medium"].is_boolean());
            assert!(stress["low_contrast"].is_boolean());
        }
    }
}
