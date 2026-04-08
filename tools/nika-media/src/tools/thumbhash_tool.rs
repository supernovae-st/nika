// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:thumbhash — Compact 25-byte image placeholder.
//!
//! Generates a ThumbHash (tiny visual placeholder) from an image.
//! Requires resize to max 100x100 before hashing.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::{MediaOp, MediaOpResult};

pub struct ThumbhashOp;

impl MediaOp for ThumbhashOp {
    fn name(&self) -> &'static str {
        "thumbhash"
    }

    fn description(&self) -> &'static str {
        "Generate a compact 25-byte image placeholder (ThumbHash)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": {
              "type": "string",
              "description": "CAS hash of the image (blake3:...)"
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
                .ok_or_else(|| invalid_args("thumbhash", "missing required parameter 'hash'"))?;

            let data = ctx.read_media(hash).await?;

            // ThumbHash needs decoded RGBA pixels, max 100x100.
            // We use imagesize for dimensions and a simple pixel extraction.
            let thumb_hash = ctx
                .compute
                .compute(move || compute_thumbhash(&data))
                .await??;

            let encoded =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &thumb_hash);

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "thumbhash": encoded,
              "size_bytes": thumb_hash.len(),
            })))
        })
    }
}

/// Compute thumbhash from raw image data.
///
/// Decodes the image, resizes to max 100x100, extracts RGBA, then hashes.
fn compute_thumbhash(data: &[u8]) -> Result<Vec<u8>, MediaToolError> {
    // Try to decode with image crate if available
    #[cfg(feature = "media-thumbnail")]
    {
        use super::safety::decode_image_safe;
        let img = decode_image_safe(data)?;
        let small = img.resize(100, 100, image::imageops::FilterType::Triangle);
        let rgba = small.to_rgba8();
        let (w, h) = rgba.dimensions();
        let hash = thumbhash::rgba_to_thumb_hash(w as usize, h as usize, rgba.as_raw());
        Ok(hash)
    }

    // Without the image crate, we cannot decode pixels for thumbhash
    #[cfg(not(feature = "media-thumbnail"))]
    {
        let _ = data;
        Err(super::error::dependency_missing(
            "thumbhash",
            "media-thumbnail",
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

    /// Minimal valid PNG fixture.
    fn fixture_png() -> Vec<u8> {
        // Use a known-good minimal PNG
        let mut buf = Vec::new();
        buf.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature
                                                                   // IHDR: 2x2, 8-bit RGB
        let ihdr: [u8; 13] = [0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0];
        let ihdr_crc = png_crc(b"IHDR", &ihdr);
        buf.extend_from_slice(&13u32.to_be_bytes());
        buf.extend_from_slice(b"IHDR");
        buf.extend_from_slice(&ihdr);
        buf.extend_from_slice(&ihdr_crc.to_be_bytes());
        // IDAT: 2 scanlines, filter=0
        let raw: Vec<u8> = vec![
            0, 255, 0, 0, 0, 255, 0, // row 0: filter + 2 RGB pixels
            0, 0, 0, 255, 255, 255, 0, // row 1: filter + 2 RGB pixels
        ];
        let compressed = zlib_stored(&raw);
        let idat_crc = png_crc(b"IDAT", &compressed);
        buf.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        buf.extend_from_slice(b"IDAT");
        buf.extend_from_slice(&compressed);
        buf.extend_from_slice(&idat_crc.to_be_bytes());
        // IEND
        let iend_crc = png_crc(b"IEND", &[]);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"IEND");
        buf.extend_from_slice(&iend_crc.to_be_bytes());
        buf
    }

    fn png_crc(chunk_type: &[u8], data: &[u8]) -> u32 {
        let table = crc32_table();
        let mut crc: u32 = 0xFFFFFFFF;
        for &b in chunk_type.iter().chain(data.iter()) {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFFFFFF
    }

    fn crc32_table() -> [u32; 256] {
        let mut t = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB88320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            t[n as usize] = c;
        }
        t
    }

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01, 0x01];
        let len = data.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        let adler = adler32(data);
        out.extend_from_slice(&adler.to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    #[tokio::test]
    async fn thumbhash_returns_base64() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbhashOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let encoded = v["thumbhash"].as_str().unwrap();
            // Verify it's valid base64
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .expect("thumbhash should be valid base64");
            // ThumbHash spec: output is 3-28 bytes
            assert!(
                (3..=28).contains(&decoded.len()),
                "thumbhash should be 3-28 bytes per spec, got {} bytes",
                decoded.len()
            );
            // Verify size_bytes matches
            assert_eq!(v["size_bytes"].as_u64().unwrap(), decoded.len() as u64);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn thumbhash_deterministic() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = ThumbhashOp;
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
                v1["thumbhash"], v2["thumbhash"],
                "same input should produce same hash"
            );
        }
    }

    #[tokio::test]
    async fn thumbhash_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = ThumbhashOp;
        let result = op
      .execute(
        serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}),
        &ctx,
      )
      .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn thumbhash_random_bytes_no_panic() {
        let (_dir, ctx) = setup().await;

        for i in 1..50u8 {
            let data: Vec<u8> = (0..=i).collect();
            let sr = ctx.cas.store(&data).await.unwrap();
            let op = ThumbhashOp;
            // Should not panic, may error
            let _ = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        }
    }
}
