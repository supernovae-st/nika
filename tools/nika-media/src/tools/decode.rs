//! nika:decode — Decode base64-encoded data into the CAS media store.
//!
//! Bridge for APIs that return inline base64 (Gemini, Replicate, fal.ai,
//! Stability AI). Validates MIME, strips whitespace, decodes, stores in CAS.

use std::future::Future;
use std::pin::Pin;

use base64::Engine;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

/// Maximum decoded size: 100 MB (matches CAS MAX_STORE_SIZE and import limit).
const MAX_DECODE_SIZE: usize = 100 * 1024 * 1024;

/// MIME types that are too generic to be useful for decode.
const REJECTED_MIMES: &[&str] = &["application/octet-stream"];

pub struct DecodeOp;

impl MediaOp for DecodeOp {
    fn name(&self) -> &'static str {
        "decode"
    }

    fn description(&self) -> &'static str {
        "Decode base64-encoded data into the CAS media store (image, audio, video, PDF)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "data": {
              "type": "string",
              "description": "Base64-encoded binary data"
            },
            "mime_type": {
              "type": "string",
              "description": "MIME type of the decoded data (e.g. image/png, audio/mp3)"
            }
          },
          "required": ["data", "mime_type"],
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

            let data_str = args
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("decode", "missing 'data' parameter"))?;

            let mime_type = args
                .get("mime_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("decode", "missing 'mime_type' parameter"))?;

            // Validate MIME type
            if mime_type.is_empty() {
                return Err(invalid_args("decode", "mime_type must not be empty"));
            }
            for rejected in REJECTED_MIMES {
                if mime_type == *rejected {
                    return Err(invalid_args(
                        "decode",
                        format!("mime_type '{mime_type}' is too generic — specify the actual type"),
                    ));
                }
            }

            // Validate data is non-empty
            if data_str.is_empty() {
                return Err(invalid_args("decode", "data must not be empty"));
            }

            // Strip whitespace (PEM-style newlines, spaces from JSON formatting)
            let clean: String = data_str
                .chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();

            // Decode base64
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&clean)
                .or_else(|_| {
                    // Try URL-safe variant as fallback
                    base64::engine::general_purpose::URL_SAFE.decode(&clean)
                })
                .map_err(|e| tool_error("decode", format!("invalid base64: {e}")))?;

            // Size check
            if decoded.is_empty() {
                return Err(invalid_args("decode", "decoded data is empty"));
            }
            if decoded.len() > MAX_DECODE_SIZE {
                return Err(invalid_args(
                    "decode",
                    format!(
                        "decoded data too large ({} bytes, max {} bytes)",
                        decoded.len(),
                        MAX_DECODE_SIZE
                    ),
                ));
            }

            ctx.check_cancelled()?;

            let size_bytes = decoded.len() as u64;

            // Store in CAS (budget-checked)
            let store_result = ctx.store_media(&decoded, "decode").await?;

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "hash": store_result.hash,
              "mime_type": mime_type,
              "size_bytes": size_bytes,
              "deduplicated": store_result.deduplicated,
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

    /// Create a minimal valid PNG in memory.
    fn fixture_png() -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(4, 4, Rgb([255u8, 0, 0]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(enc, img.as_raw(), 4, 4, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    fn to_b64(data: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    #[tokio::test]
    async fn decode_valid_png_base64() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let b64 = to_b64(&png);

        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": b64, "mime_type": "image/png"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
            assert_eq!(v["mime_type"], "image/png");
            assert_eq!(v["size_bytes"], png.len() as u64);
            assert_eq!(v["deduplicated"], false);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn decode_with_whitespace_in_base64() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let b64 = to_b64(&png);
        // Insert PEM-style line breaks
        let with_newlines: String = b64
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                if i > 0 && i % 76 == 0 {
                    vec!['\n', c]
                } else {
                    vec![c]
                }
            })
            .collect();

        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": with_newlines, "mime_type": "image/png"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
            assert_eq!(v["size_bytes"], png.len() as u64);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn decode_rejects_empty_base64() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": "", "mime_type": "image/png"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn decode_rejects_invalid_base64() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": "!@#$%^&*()", "mime_type": "image/png"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-290"));
    }

    #[tokio::test]
    async fn decode_rejects_empty_mime_type() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": "aGVsbG8=", "mime_type": ""}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn decode_rejects_octet_stream_mime() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": "aGVsbG8=", "mime_type": "application/octet-stream"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("NIKA-294"), "got: {err}");
        assert!(err.contains("too generic"));
    }

    #[tokio::test]
    async fn decode_missing_data_param() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(serde_json::json!({"mime_type": "image/png"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn decode_missing_mime_type_param() {
        let (_dir, ctx) = setup().await;
        let op = DecodeOp;
        let result = op
            .execute(serde_json::json!({"data": "aGVsbG8="}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn decode_deduplicates() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let b64 = to_b64(&png);

        let op = DecodeOp;

        let r1 = op
            .execute(
                serde_json::json!({"data": &b64, "mime_type": "image/png"}),
                &ctx,
            )
            .await
            .unwrap();

        let r2 = op
            .execute(
                serde_json::json!({"data": &b64, "mime_type": "image/png"}),
                &ctx,
            )
            .await
            .unwrap();

        if let (MediaOpResult::Metadata(v1), MediaOpResult::Metadata(v2)) = (r1, r2) {
            assert_eq!(v1["hash"], v2["hash"]);
            assert_eq!(v2["deduplicated"], true);
        }
    }

    #[tokio::test]
    async fn decode_roundtrip() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png();
        let b64 = to_b64(&png);

        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": b64, "mime_type": "image/png"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let hash = v["hash"].as_str().unwrap();
            let read_back = ctx.read_media(hash).await.unwrap();
            assert_eq!(read_back, png, "CAS roundtrip must preserve data exactly");
        }
    }

    #[tokio::test]
    async fn decode_various_mimes() {
        let (_dir, ctx) = setup().await;
        let data = b"some binary data";
        let b64 = to_b64(data);
        let op = DecodeOp;

        for mime in ["image/jpeg", "audio/mp3", "application/pdf", "video/mp4"] {
            let result = op
                .execute(serde_json::json!({"data": &b64, "mime_type": mime}), &ctx)
                .await;
            assert!(result.is_ok(), "mime {mime} should be accepted");
        }
    }

    #[tokio::test]
    async fn decode_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = DecodeOp;
        let result = op
            .execute(
                serde_json::json!({"data": "aGVsbG8=", "mime_type": "image/png"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
