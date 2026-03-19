//! nika:import — Import local files into the CAS media store.
//!
//! Reads any file from the filesystem, detects MIME type via magic bytes,
//! stores it in the Content-Addressable Store (CAS), and returns hash + metadata.
//! This unblocks audio, video, PDF, and other non-image media in workflows.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use crate::error::NikaError;
use super::context::MediaToolContext;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

pub struct ImportOp;

impl MediaOp for ImportOp {
  fn name(&self) -> &'static str {
    "import"
  }

  fn description(&self) -> &'static str {
    "Import a local file into the CAS media store (any format: image, audio, video, PDF, etc.)"
  }

  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {
        "path": {
          "type": "string",
          "description": "File path to import (absolute or relative to working directory)"
        }
      },
      "required": ["path"],
      "additionalProperties": false
    })
  }

  fn execute<'a>(
    &'a self,
    args: serde_json::Value,
    ctx: &'a MediaToolContext,
  ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>> {
    Box::pin(async move {
      ctx.check_cancelled()?;

      let path_str = args.get("path").and_then(|v| v.as_str())
        .ok_or_else(|| invalid_args("import", "missing 'path' parameter"))?;

      let path = PathBuf::from(path_str);

      // Validate: exists and is a regular file
      if !path.exists() {
        return Err(invalid_args("import", format!("file not found: {path_str}")));
      }
      if !path.is_file() {
        return Err(invalid_args("import", format!("not a regular file: {path_str}")));
      }

      // Read the file
      let data = tokio::fs::read(&path).await
        .map_err(|e| tool_error("import", format!("read failed: {e}")))?;

      if data.is_empty() {
        return Err(invalid_args("import", "file is empty"));
      }

      // Detect MIME type via magic bytes
      let mime_type = infer::get(&data)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

      let size_bytes = data.len() as u64;

      // Store in CAS (budget-checked)
      let store_result = ctx.store_media(&data, "import").await?;

      Ok(MediaOpResult::Metadata(serde_json::json!({
        "hash": store_result.hash,
        "mime_type": mime_type,
        "size_bytes": size_bytes,
        "path": store_result.path.to_string_lossy(),
        "deduplicated": store_result.deduplicated,
      })))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::media::CasStore;
  use std::sync::Arc;

  async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    (dir, ctx)
  }

  /// Create a minimal valid PNG in memory.
  fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
    let mut buf = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8).unwrap();
    buf
  }

  /// Create a minimal valid JPEG in memory.
  fn fixture_jpeg(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
    buf.into_inner()
  }

  #[tokio::test]
  async fn import_png_file() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let png_data = fixture_png(10, 10, 255, 0, 0);
    std::fs::write(tmp.path(), &png_data).unwrap();

    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert!(v["hash"].as_str().unwrap().starts_with("blake3:"), "hash must be blake3-prefixed");
      assert_eq!(v["mime_type"], "image/png");
      assert_eq!(v["size_bytes"], png_data.len() as u64);
      assert_eq!(v["deduplicated"], false);
    } else {
      panic!("expected Metadata result");
    }
  }

  #[tokio::test]
  async fn import_jpeg_file() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let jpeg_data = fixture_jpeg(10, 10, 0, 128, 255);
    std::fs::write(tmp.path(), &jpeg_data).unwrap();

    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      assert!(v["hash"].as_str().unwrap().starts_with("blake3:"));
      assert_eq!(v["mime_type"], "image/jpeg");
      assert!(v["size_bytes"].as_u64().unwrap() > 0);
    } else {
      panic!("expected Metadata result");
    }
  }

  #[tokio::test]
  async fn import_plain_text_file() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"hello world plain text").unwrap();

    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      // Plain text has no magic bytes — should fall back to octet-stream
      assert_eq!(v["mime_type"], "application/octet-stream");
      assert_eq!(v["size_bytes"], 22);
    } else {
      panic!("expected Metadata result");
    }
  }

  #[tokio::test]
  async fn import_deduplicates() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let data = fixture_png(5, 5, 0, 255, 0);
    std::fs::write(tmp.path(), &data).unwrap();

    let op = ImportOp;

    // First import
    let r1 = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    // Second import of same file — should deduplicate
    let r2 = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let (MediaOpResult::Metadata(v1), MediaOpResult::Metadata(v2)) = (r1, r2) {
      assert_eq!(v1["hash"], v2["hash"], "same content must produce same hash");
      assert_eq!(v2["deduplicated"], true, "second import should be deduplicated");
    }
  }

  #[tokio::test]
  async fn import_can_be_read_back_from_cas() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let data = fixture_png(8, 8, 128, 128, 128);
    std::fs::write(tmp.path(), &data).unwrap();

    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await.unwrap();

    if let MediaOpResult::Metadata(v) = result {
      let hash = v["hash"].as_str().unwrap();
      let read_back = ctx.read_media(hash).await.unwrap();
      assert_eq!(read_back, data, "CAS roundtrip must preserve data exactly");
    }
  }

  #[tokio::test]
  async fn import_missing_path_param() {
    let (_dir, ctx) = setup().await;
    let op = ImportOp;
    let result = op.execute(serde_json::json!({}), &ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NIKA-294"));
  }

  #[tokio::test]
  async fn import_nonexistent_file() {
    let (_dir, ctx) = setup().await;
    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": "/tmp/this_file_surely_does_not_exist_12345.xyz"}),
      &ctx,
    ).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("file not found"));
  }

  #[tokio::test]
  async fn import_directory_rejected() {
    let (_dir, ctx) = setup().await;
    let tmp_dir = tempfile::tempdir().unwrap();
    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp_dir.path().to_string_lossy()}),
      &ctx,
    ).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not a regular file"));
  }

  #[tokio::test]
  async fn import_empty_file_rejected() {
    let (_dir, ctx) = setup().await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"").unwrap();

    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": tmp.path().to_string_lossy()}),
      &ctx,
    ).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
  }

  #[tokio::test]
  async fn import_cancelled_workflow() {
    let (_dir, ctx) = setup().await;
    ctx.cancel.cancel();
    let op = ImportOp;
    let result = op.execute(
      serde_json::json!({"path": "/tmp/anything"}),
      &ctx,
    ).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cancelled"));
  }

  #[tokio::test]
  async fn import_fuzz_no_panic() {
    let (_dir, ctx) = setup().await;
    let op = ImportOp;
    // Various bad inputs — none should panic
    let bad_inputs = vec![
      serde_json::json!(null),
      serde_json::json!(42),
      serde_json::json!({"path": 123}),
      serde_json::json!({"path": ""}),
      serde_json::json!({"path": null}),
      serde_json::json!({"wrong_key": "value"}),
    ];
    for input in bad_inputs {
      let result = op.execute(input.clone(), &ctx).await;
      assert!(result.is_err(), "bad input should error, not panic: {input}");
    }
  }
}
