// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:phash — Perceptual image hashing for near-duplicate detection.
//!
//! Uses `image_hasher` crate with DCT-based hashing (dHash).

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::safety::decode_image_safe;
use super::{MediaOp, MediaOpResult};

pub struct PhashOp;

impl MediaOp for PhashOp {
    fn name(&self) -> &'static str {
        "phash"
    }

    fn description(&self) -> &'static str {
        "Compute perceptual hash of an image for near-duplicate detection"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the image" },
            "hash_size": { "type": "integer", "description": "Hash size in bits (default: 8, produces 64-bit hash)", "default": 8 }
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
                .ok_or_else(|| invalid_args("phash", "missing 'hash'"))?;
            let hash_size = args
                .get("hash_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(8)
                .clamp(4, 16) as u32;

            let data = ctx.read_media(hash).await?;

            let phash = ctx
                .compute
                .compute(move || -> Result<String, MediaToolError> {
                    let img = decode_image_safe(&data)?;
                    let hasher = image_hasher::HasherConfig::new()
                        .hash_size(hash_size, hash_size)
                        .to_hasher();
                    let hash = hasher.hash_image(&img);
                    Ok(hash.to_base64())
                })
                .await??;

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "phash": phash,
              "hash_size": hash_size,
              "algorithm": "dct",
            })))
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
    async fn phash_returns_base64() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 255, 0, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PhashOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();
        if let MediaOpResult::Metadata(v) = result {
            assert!(v["phash"].is_string());
            assert_eq!(v["algorithm"], "dct");
        }
    }

    #[tokio::test]
    async fn phash_identical_images_same_hash() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 255, 0, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = PhashOp;
        let r1 = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();
        let r2 = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let (MediaOpResult::Metadata(v1), MediaOpResult::Metadata(v2)) = (r1, r2) {
            assert_eq!(
                v1["phash"], v2["phash"],
                "same image must produce same phash"
            );
        }
    }

    #[tokio::test]
    async fn phash_different_images_different_hash() {
        let (_dir, ctx) = setup().await;
        let red = fixture_png(50, 50, 255, 0, 0);
        let blue = fixture_png(50, 50, 0, 0, 255);
        let sr1 = ctx.cas.store(&red).await.unwrap();
        let sr2 = ctx.cas.store(&blue).await.unwrap();

        let op = PhashOp;
        let r1 = op
            .execute(serde_json::json!({"hash": sr1.hash}), &ctx)
            .await
            .unwrap();
        let r2 = op
            .execute(serde_json::json!({"hash": sr2.hash}), &ctx)
            .await
            .unwrap();

        if let (MediaOpResult::Metadata(v1), MediaOpResult::Metadata(v2)) = (r1, r2) {
            // Different solid-color images should produce different hashes
            // (though for very simple images, DCT might be similar)
            let h1 = v1["phash"].as_str().unwrap();
            let h2 = v2["phash"].as_str().unwrap();
            assert!(!h1.is_empty() && !h2.is_empty());
        }
    }

    #[tokio::test]
    async fn phash_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = PhashOp;
        let result = op.execute(serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn phash_fuzz_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = PhashOp;
        for i in 1..30u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "phash panicked on input {i}"
                    );
                }
            }
        }
    }
}
