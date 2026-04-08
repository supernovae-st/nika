// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:metadata — Universal media metadata extraction.
//!
//! Routes on MIME type:
//! - image/* → nom-exif (EXIF, GPS, camera) + imagesize (dimensions)
//! - audio/* → lofty (tags)
//! - fallback → { mime_type, size }

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::{MediaOp, MediaOpResult};

pub struct MetadataOp;

impl MediaOp for MetadataOp {
    fn name(&self) -> &'static str {
        "metadata"
    }

    fn description(&self) -> &'static str {
        "Extract metadata from any media file (EXIF, audio tags, video info)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the media file" }
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
                .ok_or_else(|| invalid_args("metadata", "missing 'hash'"))?;

            let data = ctx.read_media(hash).await?;
            let size = data.len();

            // Detect MIME type
            let mime = infer::get(&data)
                .map(|t| t.mime_type().to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let metadata = ctx
                .compute
                .compute(move || extract_metadata(&data, &mime, size))
                .await?;

            Ok(MediaOpResult::Metadata(metadata))
        })
    }
}

fn extract_metadata(data: &[u8], mime: &str, size: usize) -> serde_json::Value {
    let mut result = serde_json::json!({
      "mime_type": mime,
      "size_bytes": size,
    });

    if mime.starts_with("image/") {
        extract_image_metadata(data, result.as_object_mut().unwrap());
    } else if mime.starts_with("audio/") {
        extract_audio_metadata(data, result.as_object_mut().unwrap());
    }

    result
}

fn extract_image_metadata(data: &[u8], map: &mut serde_json::Map<String, serde_json::Value>) {
    // Dimensions via imagesize (always available, fast)
    if let Ok(size) = imagesize::blob_size(data) {
        map.insert("width".into(), serde_json::json!(size.width));
        map.insert("height".into(), serde_json::json!(size.height));
    }

    // EXIF via nom-exif (best-effort, never fails)
    extract_exif(data, map);
}

fn extract_exif(data: &[u8], map: &mut serde_json::Map<String, serde_json::Value>) {
    use nom_exif::{ExifIter, MediaParser, MediaSource};
    use std::io::Cursor;

    let mut parser = MediaParser::new();
    let Ok(ms) = MediaSource::seekable(Cursor::new(data)) else {
        tracing::trace!("metadata: EXIF source creation failed (not seekable or too small)");
        return;
    };
    let Ok(exif_iter): Result<ExifIter, _> = parser.parse(ms) else {
        tracing::trace!("metadata: EXIF parsing failed (no EXIF data or unsupported format)");
        return;
    };

    let mut exif_map = serde_json::Map::new();
    for entry in exif_iter {
        let tag_name = format!("{:?}", entry.tag());
        let value_str = entry
            .get_value()
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        exif_map.insert(tag_name, serde_json::Value::String(value_str));
    }

    if !exif_map.is_empty() {
        map.insert("exif".into(), serde_json::Value::Object(exif_map));
    }
}

fn extract_audio_metadata(data: &[u8], map: &mut serde_json::Map<String, serde_json::Value>) {
    use lofty::prelude::*;
    use std::io::Cursor;

    let probe = match lofty::probe::Probe::new(Cursor::new(data)).guess_file_type() {
        Ok(p) => p,
        Err(e) => {
            tracing::trace!("metadata: audio probe failed: {e}");
            return;
        }
    };

    let tagged = match probe.read() {
        Ok(t) => t,
        Err(e) => {
            tracing::trace!("metadata: audio tag read failed: {e}");
            return;
        }
    };

    if let Some(tag) = tagged.primary_tag() {
        if let Some(title) = tag.title() {
            map.insert("title".into(), serde_json::json!(title.to_string()));
        }
        if let Some(artist) = tag.artist() {
            map.insert("artist".into(), serde_json::json!(artist.to_string()));
        }
        if let Some(album) = tag.album() {
            map.insert("album".into(), serde_json::json!(album.to_string()));
        }
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
    fn fixture_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(10, 20, Rgb([128u8, 64, 32]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            10,
            20,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        buf
    }

    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn metadata_png_returns_dimensions() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = MetadataOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["width"], 10);
            assert_eq!(v["height"], 20);
            assert!(v["mime_type"].as_str().unwrap().starts_with("image/"));
        }
    }

    #[tokio::test]
    async fn metadata_unknown_format_minimal() {
        let (_dir, ctx) = setup().await;
        let data = b"not a real media file but long enough to store";
        let sr = ctx.cas.store(data).await.unwrap();

        let op = MetadataOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(v["size_bytes"].as_u64().unwrap() > 0);
        }
    }

    #[tokio::test]
    async fn metadata_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = MetadataOp;
        let result = op.execute(serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn metadata_random_bytes_no_panic() {
        let (_dir, ctx) = setup().await;
        for i in 1..50u8 {
            let data: Vec<u8> = (0..=i).collect();
            if let Ok(sr) = ctx.cas.store(&data).await {
                let op = MetadataOp;
                let _ = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
            }
        }
    }
}
