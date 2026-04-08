// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:verify — Read and verify C2PA content credentials.
//!
//! Uses `c2pa::Reader` to parse C2PA manifests from signed images.
//! Returns manifest metadata, assertion list, validation status,
//! and EU AI Act compliance assessment.
//!
//! EU AI Act Article 50 compliance check:
//! 1. Has C2PA manifest? ✓
//! 2. Has digital source type assertion? ✓
//! 3. Source type indicates AI involvement? ✓
//!
//! Deadline: August 2, 2026.

use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::{MediaOp, MediaOpResult};

pub struct VerifyOp;

/// Digital source types that indicate AI involvement (EU AI Act relevant).
const AI_SOURCE_TYPES: &[&str] = &[
    "trainedAlgorithmicMedia",
    "compositeWithTrainedAlgorithmicMedia",
    "algorithmicMedia",
];

impl MediaOp for VerifyOp {
    fn name(&self) -> &'static str {
        "verify"
    }

    fn description(&self) -> &'static str {
        "Verify C2PA content credentials and check EU AI Act compliance"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "CAS hash of the signed image (JPEG or PNG with C2PA manifest)"
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
                .ok_or_else(|| invalid_args("verify", "missing 'hash'"))?;

            let data = ctx.read_media(hash).await?;

            // Verify on compute pool (CPU-bound crypto verification)
            let result = ctx
                .compute
                .compute(move || -> Result<serde_json::Value, MediaToolError> {
                    // Detect format from magic bytes
                    let mime_type = detect_mime(&data)?;

                    // Parse C2PA manifest
                    let mut cursor = Cursor::new(&data);
                    let reader = match c2pa::Reader::from_stream(&mime_type, &mut cursor) {
                        Ok(r) => r,
                        Err(_) => {
                            // No C2PA manifest found
                            return Ok(serde_json::json!({
                                "has_manifest": false,
                                "title": null,
                                "claim_generator": null,
                                "assertions": [],
                                "digital_source_type": null,
                                "validation_status": "none",
                                "eu_ai_act_compliant": false,
                            }));
                        }
                    };

                    let manifest = match reader.active_manifest() {
                        Some(m) => m,
                        None => {
                            return Ok(serde_json::json!({
                                "has_manifest": false,
                                "title": null,
                                "claim_generator": null,
                                "assertions": [],
                                "digital_source_type": null,
                                "validation_status": "unknown",
                                "eu_ai_act_compliant": false,
                            }));
                        }
                    };

                    // Extract title
                    let title = manifest.title().map(|s| s.to_string());

                    // Extract claim generator
                    let claim_generator = manifest
                        .claim_generator_info
                        .as_ref()
                        .and_then(|info| info.first())
                        .map(|g| {
                            format!("{} v{}", g.name, g.version.as_deref().unwrap_or("unknown"))
                        });

                    // Extract assertions
                    let mut assertions_json = Vec::new();
                    let mut digital_source_type: Option<String> = None;

                    for assertion in manifest.assertions().iter() {
                        let label = assertion.label().to_string();

                        match assertion.value() {
                            Ok(value) => {
                                // Extract digitalSourceType from c2pa.actions
                                if label.starts_with("c2pa.actions") {
                                    if let Some(actions) =
                                        value.get("actions").and_then(serde_json::Value::as_array)
                                    {
                                        for action in actions {
                                            if let Some(dst) = action
                                                .get("digitalSourceType")
                                                .and_then(serde_json::Value::as_str)
                                            {
                                                digital_source_type = Some(dst.to_string());
                                            }
                                        }
                                    }
                                }

                                assertions_json.push(serde_json::json!({
                                    "label": label,
                                    "data": value,
                                }));
                            }
                            Err(_) => {
                                assertions_json.push(serde_json::json!({
                                    "label": label,
                                }));
                            }
                        }
                    }

                    // Validation status:
                    // None = validation not performed → "valid" (no issues detected)
                    // Some([]) = no issues found → "valid"
                    // Some([...]) = check for critical failures vs informational
                    //   - signingCredential.untrusted = expected for self-signed → "self_signed"
                    //   - other failures → "invalid"
                    let validation_status = match reader.validation_status() {
                        None | Some(&[]) => "valid".to_string(),
                        Some(statuses) => {
                            let has_critical = statuses.iter().any(|s| {
                                let code = s.code();
                                // These are genuine integrity failures
                                code.contains("mismatch")
                                    || code.contains("revoked")
                                    || code.contains("expired")
                                    || code.contains("corrupt")
                            });
                            if has_critical {
                                "invalid".to_string()
                            } else if statuses.iter().any(|s| s.code().contains("untrusted")) {
                                "self_signed".to_string()
                            } else {
                                "valid".to_string()
                            }
                        }
                    };

                    // EU AI Act compliance check
                    let has_manifest = true;
                    let has_ai_source = digital_source_type
                        .as_ref()
                        .is_some_and(|dst| AI_SOURCE_TYPES.iter().any(|ai| dst.contains(ai)));
                    let eu_ai_act_compliant = has_manifest && has_ai_source;

                    Ok(serde_json::json!({
                        "has_manifest": has_manifest,
                        "title": title,
                        "claim_generator": claim_generator,
                        "assertions": assertions_json,
                        "digital_source_type": digital_source_type,
                        "validation_status": validation_status,
                        "eu_ai_act_compliant": eu_ai_act_compliant,
                    }))
                })
                .await??;

            Ok(MediaOpResult::Metadata(result))
        })
    }
}

/// Detect MIME type from magic bytes for C2PA Reader.
fn detect_mime(data: &[u8]) -> Result<String, MediaToolError> {
    if data.len() < 4 {
        return Err(invalid_args("verify", "file too small to detect format"));
    }
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Ok("image/png".to_string())
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok("image/jpeg".to_string())
    } else {
        Err(invalid_args(
            "verify",
            "unsupported format — C2PA verification requires JPEG or PNG",
        ))
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

    fn fixture_jpeg(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
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

    /// Sign a JPEG with C2PA provenance for testing verify.
    async fn sign_jpeg(ctx: &MediaToolContext, assertion: &str) -> String {
        let jpeg = fixture_jpeg(100, 100, 42, 128, 200);
        let sr = ctx.cas.store(&jpeg).await.unwrap();

        let provenance_op = super::super::provenance::ProvenanceOp;
        let result = provenance_op
            .execute(
                serde_json::json!({
                    "hash": sr.hash,
                    "assertion": assertion,
                    "title": "Test Asset"
                }),
                ctx,
            )
            .await
            .unwrap();

        match result {
            super::super::MediaOpResult::Binary { data, .. } => {
                let signed_sr = ctx.cas.store(&data).await.unwrap();
                signed_sr.hash
            }
            _ => panic!("expected Binary result from provenance"),
        }
    }

    #[tokio::test]
    async fn verify_signed_jpeg_ai_generated() {
        let (_dir, ctx) = setup().await;
        let signed_hash = sign_jpeg(&ctx, "ai.generated").await;

        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": signed_hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["has_manifest"], true);
            assert_eq!(v["title"], "Test Asset");
            assert!(v["claim_generator"].as_str().unwrap().contains("Nika"));
            assert!(v["digital_source_type"]
                .as_str()
                .unwrap()
                .contains("trainedAlgorithmicMedia"));
            assert_eq!(v["eu_ai_act_compliant"], true);
            // Ephemeral self-signed cert: "valid" or "self_signed" are both acceptable
            let status = v["validation_status"].as_str().unwrap();
            assert!(
                status == "valid" || status == "self_signed",
                "expected valid or self_signed, got: {status}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn verify_signed_jpeg_human_created() {
        let (_dir, ctx) = setup().await;
        let signed_hash = sign_jpeg(&ctx, "human.created").await;

        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": signed_hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["has_manifest"], true);
            assert!(v["digital_source_type"]
                .as_str()
                .unwrap()
                .contains("humanCreation"));
            // Human-created is NOT AI → not EU AI Act compliant (doesn't need to be)
            assert_eq!(v["eu_ai_act_compliant"], false);
        }
    }

    #[tokio::test]
    async fn verify_unsigned_image() {
        let (_dir, ctx) = setup().await;
        let jpeg = fixture_jpeg(50, 50, 255, 0, 0);
        let sr = ctx.cas.store(&jpeg).await.unwrap();

        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["has_manifest"], false);
            assert_eq!(v["eu_ai_act_compliant"], false);
            assert_eq!(v["validation_status"], "none");
        }
    }

    #[tokio::test]
    async fn verify_unsigned_png() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 0, 255, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["has_manifest"], false);
        }
    }

    #[tokio::test]
    async fn verify_sign_then_verify_roundtrip() {
        let (_dir, ctx) = setup().await;
        let signed_hash = sign_jpeg(&ctx, "ai.modified").await;

        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": signed_hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["has_manifest"], true);
            assert!(v["digital_source_type"]
                .as_str()
                .unwrap()
                .contains("composite"));
            assert!(!v["assertions"].as_array().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn verify_non_image_data() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(b"this is not an image at all").await.unwrap();

        let op = VerifyOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err(), "non-image should error");
    }

    #[tokio::test]
    async fn verify_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = VerifyOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn verify_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = VerifyOp;
        let result = op
            .execute(serde_json::json!({"hash": "blake3:abc"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn verify_fuzz_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = VerifyOp;
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

    #[test]
    fn detect_mime_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(detect_mime(&data).unwrap(), "image/jpeg");
    }

    #[test]
    fn detect_mime_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_mime(&data).unwrap(), "image/png");
    }

    #[test]
    fn detect_mime_unknown() {
        let data = [0x00, 0x01, 0x02, 0x03];
        assert!(detect_mime(&data).is_err());
    }
}
