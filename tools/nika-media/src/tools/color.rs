// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:dominant_color — Extract dominant color palette from an image.
//!
//! Uses `color-thief` crate. Quality 1 = best quality (slowest).

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

pub struct DominantColorOp;

impl MediaOp for DominantColorOp {
    fn name(&self) -> &'static str {
        "dominant_color"
    }

    fn description(&self) -> &'static str {
        "Extract dominant color palette from an image"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": {
              "type": "string",
              "description": "CAS hash of the image (blake3:...)"
            },
            "count": {
              "type": "integer",
              "description": "Number of colors to extract (default: 5, max: 20)",
              "default": 5,
              "minimum": 2,
              "maximum": 20
            },
            "quality": {
              "type": "integer",
              "description": "Quality 1 = best, 10 = fastest (default: 5)",
              "default": 5,
              "minimum": 1,
              "maximum": 10
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
            let hash = args.get("hash").and_then(|v| v.as_str()).ok_or_else(|| {
                invalid_args("dominant_color", "missing required parameter 'hash'")
            })?;

            let count = args
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(5)
                .clamp(2, 20) as u8; // color_thief requires max_colors >= 2

            let quality = args
                .get("quality")
                .and_then(|v| v.as_u64())
                .unwrap_or(5)
                .clamp(1, 10) as u8;

            let data = ctx.read_media(hash).await?;

            let colors = ctx
                .compute
                .compute(move || extract_palette(&data, count, quality))
                .await??;

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "colors": colors,
              "count": colors.len(),
            })))
        })
    }
}

/// Extract color palette from raw image data.
fn extract_palette(
    data: &[u8],
    max_colors: u8,
    quality: u8,
) -> Result<Vec<serde_json::Value>, MediaToolError> {
    #[cfg(not(feature = "media-thumbnail"))]
    {
        let _ = (data, quality, max_colors);
        return Err(super::error::dependency_missing(
            "dominant_color",
            "media-thumbnail",
        ));
    }

    #[cfg(feature = "media-thumbnail")]
    let (pixels, format) = {
        use super::safety::decode_image_safe;
        let img = decode_image_safe(data)?;
        let rgb = img.to_rgb8();
        (rgb.into_raw(), color_thief::ColorFormat::Rgb)
    };

    #[cfg(feature = "media-thumbnail")]
    {
        let palette = color_thief::get_palette(&pixels, format, quality, max_colors)
            .map_err(|e| tool_error("dominant_color", format!("palette extraction failed: {e}")))?;

        let colors: Vec<serde_json::Value> = palette
            .iter()
            .map(|c| {
                serde_json::json!({
                  "r": c.r,
                  "g": c.g,
                  "b": c.b,
                  "hex": format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
                })
            })
            .collect();

        Ok(colors)
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
    fn fixture_red_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(10, 10, Rgb([255u8, 0, 0]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        buf
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn dominant_color_solid_red() {
        let (_dir, ctx) = setup().await;
        let png = fixture_red_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = DominantColorOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let colors = v["colors"].as_array().unwrap();
            assert!(!colors.is_empty());
            // First color should be red-ish
            let first = &colors[0];
            assert!(
                first["r"].as_u64().unwrap() > 200,
                "red channel should be high"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn dominant_color_count_param() {
        let (_dir, ctx) = setup().await;
        let png = fixture_red_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = DominantColorOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "count": 3}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let colors = v["colors"].as_array().unwrap();
            assert!(colors.len() <= 3);
        }
    }

    #[tokio::test]
    async fn dominant_color_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = DominantColorOp;
        let result = op
      .execute(
        serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}),
        &ctx,
      )
      .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn dominant_color_missing_param() {
        let (_dir, ctx) = setup().await;
        let op = DominantColorOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }
}
