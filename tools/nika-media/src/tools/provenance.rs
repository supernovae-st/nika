// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:provenance — Add C2PA content credentials to media files.
//!
//! Uses the c2pa crate with rust_native_crypto (Ed25519, no OpenSSL).
//! Signs images with an ephemeral self-signed certificate and embeds
//! C2PA manifests with provenance assertions (e.g., "ai.generated").
//!
//! SECURITY: Never log private keys.

use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

pub struct ProvenanceOp;

/// Supported assertion presets.
const KNOWN_ASSERTIONS: &[&str] = &["ai.generated", "ai.modified", "human.created"];

impl MediaOp for ProvenanceOp {
    fn name(&self) -> &'static str {
        "provenance"
    }

    fn description(&self) -> &'static str {
        "Add C2PA content credentials (provenance) to an image"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": {
              "type": "string",
              "description": "CAS hash of the source image (JPEG or PNG)"
            },
            "assertion": {
              "type": "string",
              "description": "Provenance assertion: ai.generated, ai.modified, or human.created",
              "enum": ["ai.generated", "ai.modified", "human.created"]
            },
            "title": {
              "type": "string",
              "description": "Asset title (optional, defaults to 'nika-output')"
            }
          },
          "required": ["hash", "assertion"],
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
                .ok_or_else(|| invalid_args("provenance", "missing 'hash'"))?;

            let assertion = args
                .get("assertion")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("provenance", "missing 'assertion'"))?;

            if !KNOWN_ASSERTIONS.contains(&assertion) {
                return Err(invalid_args(
                    "provenance",
                    format!(
                        "unknown assertion '{assertion}', use one of: {}",
                        KNOWN_ASSERTIONS.join(", ")
                    ),
                ));
            }

            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("nika-output")
                .to_string();

            // Read source image from CAS
            let data = ctx.read_media(hash).await?;

            // Detect format from magic bytes
            let (source_mime, extension) = detect_image_format(&data)?;

            let assertion_owned = assertion.to_string();
            let title_for_metadata = title.clone();
            let assertion_for_metadata = assertion_owned.clone();

            // Sign on compute pool (CPU-bound crypto)
            let signed_data = ctx
                .compute
                .compute(move || -> Result<Vec<u8>, MediaToolError> {
                    sign_with_c2pa(&data, &source_mime, &title, &assertion_owned)
                })
                .await??;

            // Build metadata before moving extension (borrow via as_str)
            let result_mime = extension_to_mime(&extension);
            let meta = serde_json::json!({
              "assertion": assertion_for_metadata,
              "title": title_for_metadata,
              "format": extension.as_str(),
              "signed": true,
            });

            Ok(MediaOpResult::Binary {
                data: signed_data,
                mime_type: result_mime,
                extension,
                metadata: meta,
            })
        })
    }
}

/// Detect image format from magic bytes.
fn detect_image_format(data: &[u8]) -> Result<(String, String), MediaToolError> {
    if data.len() < 4 {
        return Err(invalid_args(
            "provenance",
            "file too small to detect format",
        ));
    }

    // PNG: 89 50 4E 47
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Ok(("image/png".to_string(), "png".to_string()));
    }

    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Ok(("image/jpeg".to_string(), "jpg".to_string()));
    }

    Err(invalid_args(
        "provenance",
        "unsupported format — C2PA requires JPEG or PNG",
    ))
}

fn extension_to_mime(ext: &str) -> String {
    match ext {
        "png" => "image/png".to_string(),
        "jpg" => "image/jpeg".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// Build the C2PA digital source type for an assertion preset.
fn digital_source_type(assertion: &str) -> &'static str {
    match assertion {
        "ai.generated" => "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
        "ai.modified" => {
            "http://cv.iptc.org/newscodes/digitalsourcetype/compositeWithTrainedAlgorithmicMedia"
        }
        "human.created" => "http://cv.iptc.org/newscodes/digitalsourcetype/humanCreation",
        _ => unreachable!(
            "assertion validated against KNOWN_ASSERTIONS before calling digital_source_type"
        ),
    }
}

/// Sign data with C2PA using an ephemeral Ed25519 certificate.
fn sign_with_c2pa(
    data: &[u8],
    mime_type: &str,
    title: &str,
    assertion: &str,
) -> Result<Vec<u8>, MediaToolError> {
    use c2pa::{Builder, EphemeralSigner};

    // Create ephemeral signer (self-signed Ed25519, no OpenSSL)
    let signer = EphemeralSigner::new("nika-provenance")
        .map_err(|e| tool_error("provenance", format!("signer creation failed: {e}")))?;

    // Build manifest definition
    let definition = serde_json::json!({
      "title": title,
      "claim_generator_info": [{
        "name": "Nika",
        "version": env!("CARGO_PKG_VERSION"),
      }],
      "assertions": [
        {
          "label": "c2pa.actions",
          "data": {
            "actions": [
              {
                "action": "c2pa.created",
                "digitalSourceType": digital_source_type(assertion),
                "softwareAgent": {
                  "name": "Nika Workflow Engine",
                  "version": env!("CARGO_PKG_VERSION"),
                }
              }
            ]
          }
        }
      ]
    });

    let mut builder = Builder::from_json(&definition.to_string())
        .map_err(|e| tool_error("provenance", format!("builder creation failed: {e}")))?;

    let mut source = Cursor::new(data);
    let mut dest = Cursor::new(Vec::new());

    builder
        .sign(&signer, mime_type, &mut source, &mut dest)
        .map_err(|e| tool_error("provenance", format!("signing failed: {e}")))?;

    Ok(dest.into_inner())
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

    #[tokio::test]
    async fn provenance_sign_jpeg_ai_generated() {
        let (_dir, ctx) = setup().await;
        let jpeg = fixture_jpeg(100, 100, 255, 0, 0);
        let sr = ctx.cas.store(&jpeg).await.unwrap();

        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "assertion": "ai.generated"
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
            // Signed JPEG should be larger than original (embedded manifest)
            assert!(data.len() > jpeg.len(), "signed file should be larger");
            // Still a valid JPEG (starts with FF D8 FF)
            assert_eq!(
                &data[..3],
                &[0xFF, 0xD8, 0xFF],
                "should still be valid JPEG"
            );
            assert_eq!(metadata["signed"], true);
            assert_eq!(metadata["assertion"], "ai.generated");
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn provenance_sign_png_human_created() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 0, 255, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "assertion": "human.created",
                  "title": "My Artwork"
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
            assert_eq!(mime_type, "image/png");
            // Signed PNG should start with PNG magic
            assert_eq!(&data[..4], &[0x89, 0x50, 0x4E, 0x47]);
            assert_eq!(metadata["title"], "My Artwork");
        } else {
            panic!("expected Binary result");
        }
    }

    #[tokio::test]
    async fn provenance_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "assertion": "ai.generated"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn provenance_missing_assertion() {
        let (_dir, ctx) = setup().await;
        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn provenance_unknown_assertion() {
        let (_dir, ctx) = setup().await;
        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
                  "assertion": "unknown.type"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown assertion"));
    }

    #[tokio::test]
    async fn provenance_unsupported_format() {
        let (_dir, ctx) = setup().await;
        // Store a text file — not a valid image
        let sr = ctx.cas.store(b"this is not an image at all").await.unwrap();

        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "assertion": "ai.generated"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported format"));
    }

    #[tokio::test]
    async fn provenance_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": "blake3:abcd",
                  "assertion": "ai.generated"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn detect_format_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let (mime, ext) = detect_image_format(&data).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(ext, "png");
    }

    #[test]
    fn detect_format_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let (mime, ext) = detect_image_format(&data).unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn detect_format_unknown() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let result = detect_image_format(&data);
        assert!(result.is_err());
    }

    #[test]
    fn detect_format_too_small() {
        let data = [0x89, 0x50];
        let result = detect_image_format(&data);
        assert!(result.is_err());
    }

    #[test]
    fn digital_source_type_mappings() {
        assert!(digital_source_type("ai.generated").contains("trainedAlgorithmicMedia"));
        assert!(digital_source_type("ai.modified").contains("composite"));
        assert!(digital_source_type("human.created").contains("humanCreation"));
    }

    /// C2PA readback: sign a JPEG, then use c2pa::Reader to verify the manifest
    /// round-trips correctly — title, claim generator, and c2pa.actions assertion.
    #[tokio::test]
    async fn provenance_readback_c2pa_manifest() {
        use c2pa::Reader;

        let (_dir, ctx) = setup().await;
        let jpeg = fixture_jpeg(100, 100, 42, 128, 200);
        let sr = ctx.cas.store(&jpeg).await.unwrap();

        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "assertion": "ai.generated",
                  "title": "Readback Test"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let signed_data = match result {
            MediaOpResult::Binary { data, .. } => data,
            other => panic!("expected Binary result, got: {other:?}"),
        };

        // Read the C2PA manifest back from the signed JPEG bytes
        let mut cursor = std::io::Cursor::new(&signed_data);
        let reader = Reader::from_stream("image/jpeg", &mut cursor)
            .expect("Reader should parse the signed JPEG");

        let manifest = reader
            .active_manifest()
            .expect("signed output should have an active manifest");

        // Verify title
        assert_eq!(
            manifest.title(),
            Some("Readback Test"),
            "manifest title must match"
        );

        // Verify claim generator contains "Nika"
        let cgi = manifest
            .claim_generator_info
            .as_ref()
            .expect("claim_generator_info should be present");
        assert!(
            cgi.iter().any(|g| g.name.contains("Nika")),
            "claim generator should contain 'Nika', got: {cgi:?}"
        );

        // Verify c2pa.actions assertion exists with correct digitalSourceType
        let assertion_labels: Vec<&str> = manifest.assertions().iter().map(|a| a.label()).collect();
        let actions_assertion = manifest
            .assertions()
            .iter()
            .find(|a| a.label().starts_with("c2pa.actions"))
            .unwrap_or_else(|| {
                panic!(
                    "c2pa.actions assertion should be present, found labels: {assertion_labels:?}"
                )
            });

        let value = actions_assertion
            .value()
            .expect("c2pa.actions should have JSON value");
        let actions = value["actions"]
            .as_array()
            .expect("actions should be an array");
        assert!(!actions.is_empty(), "actions array should not be empty");

        let first_action = &actions[0];
        assert_eq!(first_action["action"], "c2pa.created");
        let dst = first_action["digitalSourceType"]
            .as_str()
            .expect("digitalSourceType should be a string");
        assert!(
            dst.contains("trainedAlgorithmicMedia"),
            "ai.generated should map to trainedAlgorithmicMedia, got: {dst}"
        );
    }

    /// C2PA readback for PNG: ensure PNG signing also produces a valid manifest.
    #[tokio::test]
    async fn provenance_readback_c2pa_png() {
        use c2pa::Reader;

        let (_dir, ctx) = setup().await;
        let png = fixture_png(64, 64, 0, 200, 100);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ProvenanceOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash,
                  "assertion": "human.created",
                  "title": "PNG Readback"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let signed_data = match result {
            MediaOpResult::Binary { data, .. } => data,
            other => panic!("expected Binary result, got: {other:?}"),
        };

        let mut cursor = std::io::Cursor::new(&signed_data);
        let reader = Reader::from_stream("image/png", &mut cursor)
            .expect("Reader should parse the signed PNG");

        let manifest = reader
            .active_manifest()
            .expect("signed PNG should have an active manifest");

        assert_eq!(manifest.title(), Some("PNG Readback"));

        // Verify human.created maps to correct digitalSourceType
        let actions_assertion = manifest
            .assertions()
            .iter()
            .find(|a| a.label().starts_with("c2pa.actions"))
            .expect("c2pa.actions assertion should be present");
        let value = actions_assertion.value().unwrap();
        let dst = value["actions"][0]["digitalSourceType"].as_str().unwrap();
        assert!(
            dst.contains("humanCreation"),
            "human.created should map to humanCreation, got: {dst}"
        );
    }
}
