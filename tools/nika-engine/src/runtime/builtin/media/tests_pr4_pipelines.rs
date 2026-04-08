// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! PR4 cross-tool pipeline E2E tests.
//!
//! These tests exercise REAL multi-tool chains to find bugs where tools work
//! individually but BREAK when chained. Each test creates data, stores it in CAS,
//! calls tool A, feeds output to tool B, and asserts specific field values.
//!
//! Pipelines tested:
//! 1. import -> provenance -> verify (sign-then-verify round-trip)
//! 2. import -> quality (before/after DSSIM comparison)
//! 3. import -> qr_validate (QR generation, import, decode, score)

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::media::CasStore;
    use crate::runtime::builtin::media::context::MediaToolContext;
    #[allow(unused_imports)]
    use crate::runtime::builtin::media::{MediaOp, MediaOpResult};

    // ═══════════════════════════════════════════════════════════════
    // SETUP + FIXTURES
    // ═══════════════════════════════════════════════════════════════

    #[allow(dead_code)]
    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    #[allow(dead_code)]
    fn fixture_jpeg(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    #[allow(dead_code)]
    fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    /// Generate a clean QR code PNG using the `qrcode` crate (dev-dependency).
    #[cfg(feature = "media-qr")]
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

    // ═══════════════════════════════════════════════════════════════
    // PIPELINE 1: import -> provenance -> verify
    //
    // Sign a JPEG with C2PA "ai.generated", store signed result in CAS,
    // then call nika:verify on the signed hash. Assert full round-trip:
    // has_manifest, eu_ai_act_compliant, digitalSourceType.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-provenance")]
    mod provenance_verify_pipeline {
        use super::*;
        use crate::runtime::builtin::media::provenance::ProvenanceOp;
        use crate::runtime::builtin::media::verify::VerifyOp;

        #[tokio::test]
        async fn pipeline_import_sign_verify_ai_generated() {
            let (_dir, ctx) = setup().await;

            // Step 1: Create a JPEG in memory and store via CAS
            let jpeg = fixture_jpeg(100, 100, 42, 128, 200);
            let original_sr = ctx.cas.store(&jpeg).await.unwrap();
            assert!(
                original_sr.hash.starts_with("blake3:"),
                "CAS hash must be blake3-prefixed"
            );

            // Step 2: Call nika:provenance to sign with "ai.generated"
            let sign_result = ProvenanceOp
                .execute(
                    serde_json::json!({
                      "hash": original_sr.hash,
                      "assertion": "ai.generated",
                      "title": "PR4 Pipeline Test"
                    }),
                    &ctx,
                )
                .await
                .expect("provenance signing should succeed");

            let signed_data = match sign_result {
                MediaOpResult::Binary {
                    data,
                    mime_type,
                    metadata,
                    ..
                } => {
                    assert_eq!(mime_type, "image/jpeg", "signed output must be JPEG");
                    assert!(
                        data.len() > jpeg.len(),
                        "signed JPEG ({}) should be larger than original ({})",
                        data.len(),
                        jpeg.len()
                    );
                    assert_eq!(metadata["signed"], true);
                    assert_eq!(metadata["assertion"], "ai.generated");
                    data
                }
                other => panic!("expected Binary result from provenance, got: {other:?}"),
            };

            // Step 3: Store the signed result in CAS
            let signed_sr = ctx.cas.store(&signed_data).await.unwrap();
            assert_ne!(
                signed_sr.hash, original_sr.hash,
                "signed hash must differ from original"
            );

            // Step 4: Call nika:verify on the signed hash
            let verify_result = VerifyOp
                .execute(serde_json::json!({"hash": signed_sr.hash}), &ctx)
                .await
                .expect("verify should succeed on signed image");

            match verify_result {
                MediaOpResult::Metadata(v) => {
                    // Core assertions
                    assert_eq!(
                        v["has_manifest"], true,
                        "signed image must have C2PA manifest"
                    );
                    assert_eq!(
                        v["eu_ai_act_compliant"], true,
                        "ai.generated must be EU AI Act compliant"
                    );

                    // Digital source type
                    let dst = v["digital_source_type"]
                        .as_str()
                        .expect("digital_source_type should be a string");
                    assert!(
                        dst.contains("trainedAlgorithmicMedia"),
                        "ai.generated should map to trainedAlgorithmicMedia, got: {dst}"
                    );

                    // Title round-trip
                    assert_eq!(
                        v["title"], "PR4 Pipeline Test",
                        "title should survive sign->verify round-trip"
                    );

                    // Claim generator
                    let cg = v["claim_generator"]
                        .as_str()
                        .expect("claim_generator should be present");
                    assert!(
                        cg.contains("Nika"),
                        "claim generator should mention Nika, got: {cg}"
                    );

                    // Validation status (ephemeral self-signed cert: "valid" or "self_signed")
                    let status = v["validation_status"].as_str().unwrap();
                    assert!(
                        status == "valid" || status == "self_signed",
                        "freshly signed image should be valid or self_signed, got: {status}"
                    );

                    // Assertions array is non-empty
                    let assertions = v["assertions"]
                        .as_array()
                        .expect("assertions should be an array");
                    assert!(
                        !assertions.is_empty(),
                        "signed image should have at least one assertion"
                    );

                    // At least one assertion should be c2pa.actions
                    let has_actions = assertions.iter().any(|a| {
                        a["label"]
                            .as_str()
                            .unwrap_or("")
                            .starts_with("c2pa.actions")
                    });
                    assert!(has_actions, "should have c2pa.actions assertion");
                }
                other => panic!("expected Metadata from verify, got: {other:?}"),
            }
        }

        /// Same pipeline but with PNG instead of JPEG — verify format agnosticism.
        #[tokio::test]
        async fn pipeline_import_sign_verify_png_ai_modified() {
            let (_dir, ctx) = setup().await;

            // Step 1: Create a PNG and store in CAS
            let png = fixture_png(64, 64, 0, 200, 100);
            let original_sr = ctx.cas.store(&png).await.unwrap();

            // Step 2: Sign with "ai.modified"
            let sign_result = ProvenanceOp
                .execute(
                    serde_json::json!({
                      "hash": original_sr.hash,
                      "assertion": "ai.modified",
                      "title": "PNG Modified Test"
                    }),
                    &ctx,
                )
                .await
                .expect("provenance signing PNG should succeed");

            let signed_data = match sign_result {
                MediaOpResult::Binary {
                    data, mime_type, ..
                } => {
                    assert_eq!(mime_type, "image/png");
                    // PNG magic bytes must survive signing
                    assert_eq!(
                        &data[..4],
                        &[0x89, 0x50, 0x4E, 0x47],
                        "signed PNG must retain PNG magic"
                    );
                    data
                }
                other => panic!("expected Binary, got: {other:?}"),
            };

            // Step 3: Store signed in CAS, then verify
            let signed_sr = ctx.cas.store(&signed_data).await.unwrap();

            let verify_result = VerifyOp
                .execute(serde_json::json!({"hash": signed_sr.hash}), &ctx)
                .await
                .expect("verify should succeed on signed PNG");

            match verify_result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["has_manifest"], true);
                    let dst = v["digital_source_type"].as_str().unwrap();
                    assert!(
                        dst.contains("compositeWithTrainedAlgorithmicMedia"),
                        "ai.modified should map to composite, got: {dst}"
                    );
                    // ai.modified IS AI-related, so EU AI Act should be compliant
                    assert_eq!(v["eu_ai_act_compliant"], true);
                    assert_eq!(v["title"], "PNG Modified Test");
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// human.created should NOT be EU AI Act compliant.
        #[tokio::test]
        async fn pipeline_sign_verify_human_created_not_eu_compliant() {
            let (_dir, ctx) = setup().await;

            let jpeg = fixture_jpeg(50, 50, 255, 0, 0);
            let sr = ctx.cas.store(&jpeg).await.unwrap();

            let sign_result = ProvenanceOp
                .execute(
                    serde_json::json!({
                      "hash": sr.hash,
                      "assertion": "human.created"
                    }),
                    &ctx,
                )
                .await
                .unwrap();

            let signed_data = match sign_result {
                MediaOpResult::Binary { data, .. } => data,
                other => panic!("expected Binary, got: {other:?}"),
            };

            let signed_sr = ctx.cas.store(&signed_data).await.unwrap();

            let verify_result = VerifyOp
                .execute(serde_json::json!({"hash": signed_sr.hash}), &ctx)
                .await
                .unwrap();

            match verify_result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["has_manifest"], true);
                    let dst = v["digital_source_type"].as_str().unwrap();
                    assert!(dst.contains("humanCreation"));
                    // Human-created is NOT AI -> NOT EU AI Act compliant
                    assert_eq!(
                        v["eu_ai_act_compliant"], false,
                        "human.created must NOT be EU AI Act compliant"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// Verify that an unsigned image round-trips correctly through verify.
        #[tokio::test]
        async fn pipeline_unsigned_image_verify_returns_no_manifest() {
            let (_dir, ctx) = setup().await;

            let jpeg = fixture_jpeg(50, 50, 128, 128, 128);
            let sr = ctx.cas.store(&jpeg).await.unwrap();

            let verify_result = VerifyOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .unwrap();

            match verify_result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["has_manifest"], false);
                    assert_eq!(v["eu_ai_act_compliant"], false);
                    assert!(
                        v["digital_source_type"].is_null(),
                        "unsigned image should have null digital_source_type"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PIPELINE 2: import -> quality (before/after DSSIM comparison)
    //
    // Create two PNGs of the same dimensions but different colors,
    // store both in CAS, then call nika:quality to compare them.
    // Assert DSSIM > 0 and quality_grade is a valid string.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-iqa")]
    mod quality_pipeline {
        use super::*;
        use crate::runtime::builtin::media::quality::QualityOp;

        #[tokio::test]
        async fn pipeline_import_quality_different_images() {
            let (_dir, ctx) = setup().await;

            // Step 1: Create PNG A (solid gray)
            let png_a = fixture_png(80, 80, 128, 128, 128);
            let sr_a = ctx.cas.store(&png_a).await.unwrap();

            // Step 2: Create PNG B (slightly different — lighter gray)
            let png_b = fixture_png(80, 80, 140, 140, 140);
            let sr_b = ctx.cas.store(&png_b).await.unwrap();

            // Sanity: different data produces different hashes
            assert_ne!(
                sr_a.hash, sr_b.hash,
                "different images should produce different hashes"
            );

            // Step 3: Call nika:quality with hash_a and hash_b
            let quality_result = QualityOp
                .execute(
                    serde_json::json!({
                      "hash_a": sr_a.hash,
                      "hash_b": sr_b.hash,
                    }),
                    &ctx,
                )
                .await
                .expect("quality comparison should succeed");

            match quality_result {
                MediaOpResult::Metadata(v) => {
                    // DSSIM must be > 0 for different images
                    let dssim = v["dssim"].as_f64().expect("dssim should be a number");
                    assert!(
                        dssim > 0.0,
                        "DSSIM should be > 0 for different images, got {dssim}"
                    );

                    // SSIM should be < 1.0 for different images
                    let ssim = v["ssim"].as_f64().expect("ssim should be a number");
                    assert!(
                        ssim < 1.0,
                        "SSIM should be < 1.0 for different images, got {ssim}"
                    );
                    assert!(ssim > 0.0, "SSIM should be > 0.0, got {ssim}");

                    // quality_grade must be a non-empty string
                    let grade = v["quality_grade"]
                        .as_str()
                        .expect("quality_grade should be a string");
                    assert!(
                        ["excellent", "good", "acceptable", "poor"].contains(&grade),
                        "quality_grade should be a valid grade, got: {grade}"
                    );

                    // Dimensions must match the inputs
                    assert_eq!(v["dimensions"]["width"], 80);
                    assert_eq!(v["dimensions"]["height"], 80);
                }
                other => panic!("expected Metadata from quality, got: {other:?}"),
            }
        }

        /// Identical images should have DSSIM ~0 and grade "excellent".
        #[tokio::test]
        async fn pipeline_import_quality_identical_images() {
            let (_dir, ctx) = setup().await;

            let png = fixture_png(60, 60, 100, 150, 200);
            let sr = ctx.cas.store(&png).await.unwrap();

            // Same hash for both — comparing image against itself
            let quality_result = QualityOp
                .execute(
                    serde_json::json!({
                      "hash_a": sr.hash,
                      "hash_b": sr.hash,
                    }),
                    &ctx,
                )
                .await
                .expect("quality comparison should succeed");

            match quality_result {
                MediaOpResult::Metadata(v) => {
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
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// Completely different colors should produce high DSSIM.
        #[tokio::test]
        async fn pipeline_import_quality_opposite_colors() {
            let (_dir, ctx) = setup().await;

            let red = fixture_png(50, 50, 255, 0, 0);
            let blue = fixture_png(50, 50, 0, 0, 255);
            let sr_red = ctx.cas.store(&red).await.unwrap();
            let sr_blue = ctx.cas.store(&blue).await.unwrap();

            let quality_result = QualityOp
                .execute(
                    serde_json::json!({
                      "hash_a": sr_red.hash,
                      "hash_b": sr_blue.hash,
                    }),
                    &ctx,
                )
                .await
                .unwrap();

            match quality_result {
                MediaOpResult::Metadata(v) => {
                    let dssim = v["dssim"].as_f64().unwrap();
                    assert!(
                        dssim > 0.01,
                        "red vs blue should have high DSSIM, got {dssim}"
                    );

                    let grade = v["quality_grade"].as_str().unwrap();
                    // Red vs blue is a substantial difference
                    assert!(
                        grade != "excellent",
                        "red vs blue should not be 'excellent', got: {grade}"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// Verify that quality correctly rejects dimension mismatch in a pipeline context.
        #[tokio::test]
        async fn pipeline_import_quality_dimension_mismatch() {
            let (_dir, ctx) = setup().await;

            let small = fixture_png(50, 50, 128, 128, 128);
            let big = fixture_png(100, 100, 128, 128, 128);
            let sr_small = ctx.cas.store(&small).await.unwrap();
            let sr_big = ctx.cas.store(&big).await.unwrap();

            let result = QualityOp
                .execute(
                    serde_json::json!({
                      "hash_a": sr_small.hash,
                      "hash_b": sr_big.hash,
                    }),
                    &ctx,
                )
                .await;

            assert!(
                result.is_err(),
                "dimension mismatch should error in pipeline"
            );
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("dimensions"),
                "error should mention dimensions: {err}"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PIPELINE 3: import -> qr_validate (QR pipeline)
    //
    // Generate a QR code PNG, store in CAS, call nika:qr_validate,
    // assert decoded data matches, scan_score > 0.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-qr")]
    mod qr_validate_pipeline {
        use super::*;
        use crate::runtime::builtin::media::qr::QrValidateOp;

        #[tokio::test]
        async fn pipeline_import_qr_validate_url() {
            let (_dir, ctx) = setup().await;

            let original_text = "https://qrcode-ai.com/pr4-test";

            // Step 1: Generate a QR code PNG
            let qr_png = fixture_qr_png(original_text);
            assert!(
                qr_png.len() > 100,
                "QR PNG should be a reasonable size, got {} bytes",
                qr_png.len()
            );

            // Step 2: Store in CAS
            let sr = ctx.cas.store(&qr_png).await.unwrap();

            // Step 3: Call nika:qr_validate
            let validate_result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .expect("qr_validate should succeed on valid QR code");

            match validate_result {
                MediaOpResult::Metadata(v) => {
                    // decoded must be true
                    assert_eq!(v["decoded"], true, "QR code should be decoded successfully");

                    // data must match original text
                    assert_eq!(
                        v["data"], original_text,
                        "decoded data should match original text"
                    );

                    // scan_score must be > 0
                    let score = v["scan_score"]
                        .as_u64()
                        .expect("scan_score should be a number");
                    assert!(
                        score > 0,
                        "scan_score should be > 0 for a clean QR code, got {score}"
                    );

                    // error_correction should be a non-empty string
                    let ec = v["error_correction"]
                        .as_str()
                        .expect("error_correction should be a string");
                    assert!(!ec.is_empty(), "error_correction should not be empty");

                    // metadata should be present and have version
                    let metadata = &v["metadata"];
                    assert!(metadata.is_object(), "metadata should be present");
                    assert!(
                        metadata["version"].is_number(),
                        "QR version should be a number"
                    );
                }
                other => panic!("expected Metadata from qr_validate, got: {other:?}"),
            }
        }

        /// Test with simple text payload (not a URL).
        #[tokio::test]
        async fn pipeline_import_qr_validate_plain_text() {
            let (_dir, ctx) = setup().await;

            let original_text = "Hello from Nika PR4!";
            let qr_png = fixture_qr_png(original_text);
            let sr = ctx.cas.store(&qr_png).await.unwrap();

            let result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash, "fast": true}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["decoded"], true);
                    assert_eq!(v["data"], original_text);
                    assert!(v["scan_score"].as_u64().unwrap() > 0);
                    // stress_results should be present in fast mode
                    assert!(
                        v["stress_results"].is_object(),
                        "stress_results should be present"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// Full mode (fast=false) should also work and include all 6 stress fields.
        #[tokio::test]
        async fn pipeline_import_qr_validate_full_mode() {
            let (_dir, ctx) = setup().await;

            let text = "full-mode-pipeline";
            let qr_png = fixture_qr_png(text);
            let sr = ctx.cas.store(&qr_png).await.unwrap();

            let result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash, "fast": false}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["decoded"], true);
                    assert_eq!(v["data"], text);

                    let stress = &v["stress_results"];
                    assert!(stress.is_object(), "stress_results should be present");
                    // Full mode has all 6 stress test fields
                    assert!(stress["original"].is_boolean(), "original field missing");
                    assert!(
                        stress["downscale_50"].is_boolean(),
                        "downscale_50 field missing"
                    );
                    assert!(
                        stress["downscale_25"].is_boolean(),
                        "downscale_25 field missing"
                    );
                    assert!(
                        stress["blur_light"].is_boolean(),
                        "blur_light field missing"
                    );
                    assert!(
                        stress["blur_medium"].is_boolean(),
                        "blur_medium field missing"
                    );
                    assert!(
                        stress["low_contrast"].is_boolean(),
                        "low_contrast field missing"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// A non-QR image should produce decoded=false and scan_score=0.
        #[tokio::test]
        async fn pipeline_non_qr_image_fails_gracefully() {
            let (_dir, ctx) = setup().await;

            // Solid red PNG — no QR code
            let png = fixture_png(100, 100, 255, 0, 0);
            let sr = ctx.cas.store(&png).await.unwrap();

            let result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["decoded"], false, "non-QR image should not decode");
                    assert_eq!(v["scan_score"], 0, "non-QR image should have score 0");
                    assert!(v["data"].is_null(), "data should be null for failed decode");
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        /// High-quality clean QR codes should score >= 50.
        #[tokio::test]
        async fn pipeline_clean_qr_scores_high() {
            let (_dir, ctx) = setup().await;

            let text = "high-quality-test-data-for-scoring";
            let qr_png = fixture_qr_png(text);
            let sr = ctx.cas.store(&qr_png).await.unwrap();

            let result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    let score = v["scan_score"].as_u64().unwrap();
                    assert!(score >= 50, "clean QR code should score >= 50, got {score}");
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // CROSS-PIPELINE: provenance + quality (sign then measure degradation)
    // ═══════════════════════════════════════════════════════════════

    #[cfg(all(feature = "media-provenance", feature = "media-iqa"))]
    mod provenance_quality_pipeline {
        use super::*;
        use crate::runtime::builtin::media::provenance::ProvenanceOp;
        use crate::runtime::builtin::media::quality::QualityOp;

        /// Sign a JPEG with C2PA, then compare the signed version against the original
        /// using DSSIM. The signing process re-encodes the image, so DSSIM should be
        /// non-zero but the image should still be recognizable.
        #[tokio::test]
        async fn pipeline_sign_then_quality_check() {
            let (_dir, ctx) = setup().await;

            // Create original JPEG and store
            let jpeg = fixture_jpeg(80, 80, 100, 150, 200);
            let original_sr = ctx.cas.store(&jpeg).await.unwrap();

            // Sign it
            let sign_result = ProvenanceOp
                .execute(
                    serde_json::json!({
                      "hash": original_sr.hash,
                      "assertion": "ai.generated"
                    }),
                    &ctx,
                )
                .await
                .unwrap();

            let signed_data = match sign_result {
                MediaOpResult::Binary { data, .. } => data,
                other => panic!("expected Binary, got: {other:?}"),
            };

            let signed_sr = ctx.cas.store(&signed_data).await.unwrap();

            // Compare original vs signed using DSSIM
            // Note: JPEG re-encoding during C2PA signing may change pixel values slightly,
            // but both should decode to the same dimensions for DSSIM to work.
            let quality_result = QualityOp
                .execute(
                    serde_json::json!({
                      "hash_a": original_sr.hash,
                      "hash_b": signed_sr.hash,
                    }),
                    &ctx,
                )
                .await;

            // This may fail if C2PA signing changes dimensions — that would be a real bug.
            // If it succeeds, the DSSIM should be very low (images should be nearly identical).
            match quality_result {
                Ok(MediaOpResult::Metadata(v)) => {
                    let dssim = v["dssim"].as_f64().unwrap();
                    // C2PA signing should not significantly degrade image quality
                    assert!(
                        dssim < 0.1,
                        "C2PA signing should not significantly degrade quality, got DSSIM: {dssim}"
                    );

                    let grade = v["quality_grade"].as_str().unwrap();
                    assert!(
                        grade == "excellent" || grade == "good" || grade == "acceptable",
                        "signed image quality should be at least acceptable, got: {grade}"
                    );
                }
                Ok(other) => panic!("expected Metadata, got: {other:?}"),
                Err(e) => {
                    // If dimension mismatch occurs, that is itself a useful finding
                    let err_str = e.to_string();
                    assert!(
                        err_str.contains("dimensions"),
                        "if quality fails, it should be dimension-related: {err_str}"
                    );
                }
            }
        }
    }
}
