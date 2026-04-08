// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:dimensions — Image dimensions from headers (zero decode).
//!
//! Uses `imagesize` crate which reads only the header bytes (~0.1ms).

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, unsupported_format};
use super::{MediaOp, MediaOpResult};

pub struct DimensionsOp;

impl MediaOp for DimensionsOp {
    fn name(&self) -> &'static str {
        "dimensions"
    }

    fn description(&self) -> &'static str {
        "Get image dimensions from headers without decoding the full image"
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
                .ok_or_else(|| invalid_args("dimensions", "missing required parameter 'hash'"))?;

            let data = ctx.read_media(hash).await?;

            let size = imagesize::blob_size(&data).map_err(|_| {
                unsupported_format("dimensions", "could not read image dimensions from header")
            })?;

            let orientation = if size.width > size.height {
                "landscape"
            } else if size.width < size.height {
                "portrait"
            } else {
                "square"
            };

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "width": size.width,
              "height": size.height,
              "orientation": orientation,
            })))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CasStore;
    use std::sync::Arc;

    /// Create a minimal valid PNG (1x1 red pixel).
    fn fixture_png_1x1() -> Vec<u8> {
        // Minimal valid PNG: 1x1 red pixel
        let mut buf = Vec::new();
        // PNG signature
        buf.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        // IHDR chunk
        let ihdr_data: [u8; 13] = [
            0, 0, 0, 1, // width = 1
            0, 0, 0, 1, // height = 1
            8, // bit depth
            2, // color type RGB
            0, // compression
            0, // filter
            0, // interlace
        ];
        let ihdr_crc = crc32(b"IHDR", &ihdr_data);
        buf.extend_from_slice(&(13u32).to_be_bytes()); // length
        buf.extend_from_slice(b"IHDR");
        buf.extend_from_slice(&ihdr_data);
        buf.extend_from_slice(&ihdr_crc.to_be_bytes());
        // IDAT chunk (minimal scanline: filter=0, R=255, G=0, B=0)
        let raw_data: &[u8] = &[0, 255, 0, 0]; // filter + RGB
        let compressed = miniz_compress(raw_data);
        let idat_crc = crc32(b"IDAT", &compressed);
        buf.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        buf.extend_from_slice(b"IDAT");
        buf.extend_from_slice(&compressed);
        buf.extend_from_slice(&idat_crc.to_be_bytes());
        // IEND chunk
        let iend_crc = crc32(b"IEND", &[]);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"IEND");
        buf.extend_from_slice(&iend_crc.to_be_bytes());
        buf
    }

    fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        let table = crc32_table();
        for &byte in chunk_type.iter().chain(data.iter()) {
            crc = table[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    fn crc32_table() -> [u32; 256] {
        let mut table = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                if c & 1 != 0 {
                    c = 0xEDB88320 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            table[n as usize] = c;
        }
        table
    }

    fn miniz_compress(data: &[u8]) -> Vec<u8> {
        // Minimal zlib: stored block (no compression)
        let mut out = Vec::new();
        out.push(0x78); // CMF
        out.push(0x01); // FLG
                        // BFINAL=1, BTYPE=00 (stored)
        out.push(0x01);
        let len = data.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        // Adler32
        let adler = adler32(data);
        out.extend_from_slice(&adler.to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    #[tokio::test]
    async fn dimensions_png_1x1() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_1x1();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = DimensionsOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["width"], 1);
            assert_eq!(v["height"], 1);
            assert_eq!(v["orientation"], "square");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn dimensions_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op
      .execute(
        serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}),
        &ctx,
      )
      .await;
        assert!(result.is_err());
        // Should be NIKA-253 (MediaNotFound)
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NIKA-253"),
            "expected NIKA-253, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn dimensions_corrupt_header_no_panic() {
        let (_dir, ctx) = setup().await;
        let data = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8];
        let sr = ctx.cas.store(&data).await.unwrap();

        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        // Should be NIKA-291 (unsupported format)
        assert!(result.unwrap_err().to_string().contains("NIKA-291"));
    }

    #[tokio::test]
    async fn dimensions_missing_param() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }
}
