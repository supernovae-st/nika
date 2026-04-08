// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:compare — Visual comparison between two images.
//!
//! Uses perceptual hashing (image_hasher) to compute similarity distance.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::safety::decode_image_safe;
use super::{MediaOp, MediaOpResult};

pub struct CompareOp;

impl MediaOp for CompareOp {
    fn name(&self) -> &'static str {
        "compare"
    }

    fn description(&self) -> &'static str {
        "Compare two images visually using perceptual hashing (0=identical, higher=different)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash_a": { "type": "string", "description": "CAS hash of the first image" },
            "hash_b": { "type": "string", "description": "CAS hash of the second image" }
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
                .ok_or_else(|| invalid_args("compare", "missing 'hash_a'"))?;
            let hash_b = args
                .get("hash_b")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("compare", "missing 'hash_b'"))?;

            let data_a = ctx.read_media(hash_a).await?;
            let data_b = ctx.read_media(hash_b).await?;

            let result = ctx
                .compute
                .compute(move || -> Result<(u32, bool), MediaToolError> {
                    let img_a = decode_image_safe(&data_a)?;
                    let img_b = decode_image_safe(&data_b)?;

                    let hasher = image_hasher::HasherConfig::new()
                        .hash_size(8, 8)
                        .to_hasher();

                    let hash_a = hasher.hash_image(&img_a);
                    let hash_b = hasher.hash_image(&img_b);

                    let distance = hash_a.dist(&hash_b);
                    let identical = distance == 0;

                    Ok((distance, identical))
                })
                .await??;

            let (distance, identical) = result;
            // Similarity: 0 distance = 100% similar, 64 distance (max for 8x8) = 0%
            let similarity = ((64.0 - distance as f64) / 64.0 * 100.0).max(0.0);

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "distance": distance,
              "identical": identical,
              "similarity_pct": (similarity * 10.0).round() / 10.0,
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
    async fn compare_identical_images() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png(50, 50, 255, 0, 0);
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = CompareOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash_a": sr.hash, "hash_b": sr.hash
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["distance"], 0);
            assert_eq!(v["identical"], true);
            assert_eq!(v["similarity_pct"], 100.0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn compare_different_images() {
        let (_dir, ctx) = setup().await;
        // Use images with actual visual variation (not solid colors, which produce
        // identical DCT hashes because they have no frequency components)
        let img_a = {
            use image::{ImageBuffer, Rgb};
            let mut img = ImageBuffer::from_pixel(50u32, 50, Rgb([255u8, 0, 0]));
            // Add a white stripe on the left half
            for x in 0..25 {
                for y in 0..50 {
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                }
            }
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                50,
                50,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            buf
        };
        let img_b = {
            use image::{ImageBuffer, Rgb};
            let mut img = ImageBuffer::from_pixel(50u32, 50, Rgb([0u8, 0, 255]));
            // Add a black stripe on the bottom half
            for x in 0..50 {
                for y in 25..50 {
                    img.put_pixel(x, y, Rgb([0, 0, 0]));
                }
            }
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                50,
                50,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            buf
        };
        let sr1 = ctx.cas.store(&img_a).await.unwrap();
        let sr2 = ctx.cas.store(&img_b).await.unwrap();

        let op = CompareOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash_a": sr1.hash, "hash_b": sr2.hash
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["identical"], false);
            let sim = v["similarity_pct"].as_f64().unwrap();
            assert!(
                sim < 100.0,
                "visually different images should not be 100% similar"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn compare_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = CompareOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash_a": "x", "hash_b": "y"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn compare_fuzz_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = CompareOp;
        let bad_inputs = vec![
            serde_json::json!(null),
            serde_json::json!(42),
            serde_json::json!({"hash_a": 123, "hash_b": 456}),
            serde_json::json!({"hash_a": "x"}),
            serde_json::json!({"hash_b": "y"}),
            serde_json::json!({}),
        ];
        for input in bad_inputs {
            let result = op.execute(input.clone(), &ctx).await;
            assert!(result.is_err(), "bad input should error: {input}");
        }
    }

    #[tokio::test]
    async fn compare_nonexistent_hash() {
        let (_dir, ctx) = setup().await;
        let op = CompareOp;
        let result = op.execute(serde_json::json!({
      "hash_a": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
      "hash_b": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
    }), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compare_missing_param() {
        let (_dir, ctx) = setup().await;
        let op = CompareOp;
        let result = op.execute(serde_json::json!({"hash_a": "x"}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }
}
