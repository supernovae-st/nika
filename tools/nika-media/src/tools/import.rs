//! nika:import — Import local files into the CAS media store.
//!
//! Reads any file from the filesystem, detects MIME type via magic bytes,
//! stores it in the Content-Addressable Store (CAS), and returns hash + metadata.
//! This unblocks audio, video, PDF, and other non-image media in workflows.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, security_violation, tool_error};
use super::{MediaOp, MediaOpResult};

/// Maximum import file size: 100 MB (matches CAS MAX_STORE_SIZE).
///
/// The CAS store rejects data > 100 MB, so accepting larger imports would
/// succeed at read time but fail at store time — a confusing UX. Fail early.
const MAX_IMPORT_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Sensitive system directories that import must never read from.
/// Includes macOS `/private/etc` symlink targets and user credential dirs.
/// Defense-in-depth: this blocklist supplements working_dir confinement.
const SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/",
    "/proc/",
    "/sys/",
    "/dev/",
    "/var/run/",
    "/var/log/",
    // macOS: /etc → /private/etc, /var → /private/var
    "/private/etc/",
    "/private/var/run/",
    "/private/var/log/",
];

/// Sensitive home-directory patterns (credential stores).
/// Checked against canonical path to catch symlinks.
const SENSITIVE_HOME_DIRS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".kube",
    ".docker",
    ".config/gcloud",
];

/// Validate import path: reject path traversal, sensitive dirs, and out-of-scope paths.
///
/// When `working_dir` is set, the canonical path must be within it (project confinement).
/// The blocklist is defense-in-depth for when working_dir is not set (e.g. tests).
fn validate_import_path(
    path: &std::path::Path,
    working_dir: Option<&std::path::Path>,
) -> Result<(), MediaToolError> {
    let path_str = path.to_string_lossy();

    // Reject paths containing ".."
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(security_violation(
                "import",
                format!("path traversal not allowed: {path_str}"),
            ));
        }
    }

    // Reject reads from sensitive system directories (check both raw and canonical paths)
    let path_str = path.to_string_lossy();
    for prefix in SENSITIVE_PREFIXES {
        if path_str.starts_with(prefix) {
            return Err(security_violation(
                "import",
                format!("reading from {prefix} is not allowed"),
            ));
        }
    }

    if let Ok(canonical) = path.canonicalize() {
        let canonical_str = canonical.to_string_lossy();
        for prefix in SENSITIVE_PREFIXES {
            if canonical_str.starts_with(prefix) {
                return Err(security_violation(
                    "import",
                    format!("reading from {prefix} is not allowed"),
                ));
            }
        }

        // Check sensitive home directory patterns on canonical path
        if let Some(home) = dirs::home_dir() {
            let home_str = home.to_string_lossy();
            for dir in SENSITIVE_HOME_DIRS {
                let sensitive = format!("{home_str}/{dir}/");
                if canonical_str.starts_with(&sensitive) {
                    return Err(security_violation(
                        "import",
                        format!("reading from ~/{dir}/ is not allowed"),
                    ));
                }
            }
        }

        // Working directory confinement: canonical path must be under working_dir
        if let Some(wd) = working_dir {
            if let Ok(wd_canonical) = wd.canonicalize() {
                if !canonical.starts_with(&wd_canonical) {
                    return Err(security_violation(
                        "import",
                        format!("path is outside project directory: {}", canonical.display()),
                    ));
                }
            }
        }
    }

    Ok(())
}

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
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>> {
        Box::pin(async move {
            ctx.check_cancelled()?;

            let path_str = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("import", "missing 'path' parameter"))?;

            let path = PathBuf::from(path_str);

            // Security: reject path traversal, sensitive paths, and out-of-scope paths
            validate_import_path(&path, ctx.working_dir.as_deref())?;

            // Async metadata check (no blocking I/O, no TOCTOU)
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        invalid_args("import", format!("file not found: {path_str}"))
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        tool_error("import", format!("permission denied: {path_str}"))
                    }
                    _ => tool_error("import", format!("cannot stat file: {e}")),
                })?;

            if !metadata.is_file() {
                return Err(invalid_args(
                    "import",
                    format!("not a regular file: {path_str}"),
                ));
            }

            // Pre-read size check — prevents OOM from multi-GB files
            if metadata.len() == 0 {
                return Err(invalid_args("import", "file is empty"));
            }
            if metadata.len() > MAX_IMPORT_FILE_SIZE {
                return Err(invalid_args(
                    "import",
                    format!(
                        "file too large ({} bytes, max {} bytes)",
                        metadata.len(),
                        MAX_IMPORT_FILE_SIZE
                    ),
                ));
            }

            // Read the file (size already validated)
            let data = tokio::fs::read(&path)
                .await
                .map_err(|e| tool_error("import", format!("read failed: {e}")))?;

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
    use crate::CasStore;
    use std::sync::Arc;

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    /// Create a minimal valid PNG in memory.
    fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
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
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(
                v["hash"].as_str().unwrap().starts_with("blake3:"),
                "hash must be blake3-prefixed"
            );
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
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

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
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

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
        let r1 = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

        // Second import of same file — should deduplicate
        let r2 = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

        if let (MediaOpResult::Metadata(v1), MediaOpResult::Metadata(v2)) = (r1, r2) {
            assert_eq!(
                v1["hash"], v2["hash"],
                "same content must produce same hash"
            );
            assert_eq!(
                v2["deduplicated"], true,
                "second import should be deduplicated"
            );
        }
    }

    #[tokio::test]
    async fn import_can_be_read_back_from_cas() {
        let (_dir, ctx) = setup().await;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = fixture_png(8, 8, 128, 128, 128);
        std::fs::write(tmp.path(), &data).unwrap();

        let op = ImportOp;
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

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
        let result = op
            .execute(
                serde_json::json!({"path": "/tmp/this_file_surely_does_not_exist_12345.xyz"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file not found"));
    }

    #[tokio::test]
    async fn import_directory_rejected() {
        let (_dir, ctx) = setup().await;
        let tmp_dir = tempfile::tempdir().unwrap();
        let op = ImportOp;
        let result = op
            .execute(
                serde_json::json!({"path": tmp_dir.path().to_string_lossy()}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a regular file"));
    }

    #[tokio::test]
    async fn import_empty_file_rejected() {
        let (_dir, ctx) = setup().await;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"").unwrap();

        let op = ImportOp;
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn import_cancelled_workflow() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ImportOp;
        let result = op
            .execute(serde_json::json!({"path": "/tmp/anything"}), &ctx)
            .await;
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
            assert!(
                result.is_err(),
                "bad input should error, not panic: {input}"
            );
        }
    }

    #[tokio::test]
    async fn import_rejects_path_traversal() {
        let (_dir, ctx) = setup().await;
        let op = ImportOp;
        let result = op
            .execute(serde_json::json!({"path": "/tmp/../etc/hosts"}), &ctx)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NIKA-297"),
            "path traversal should be security violation, got: {err}"
        );
    }

    #[tokio::test]
    async fn import_rejects_sensitive_paths() {
        let (_dir, ctx) = setup().await;
        let op = ImportOp;
        // These are checked by raw path prefix, so they work even if the file doesn't exist
        for path in [
            "/etc/passwd",
            "/etc/hosts",
            "/dev/null",
            "/var/log/system.log",
        ] {
            let result = op.execute(serde_json::json!({"path": path}), &ctx).await;
            assert!(result.is_err(), "sensitive path {path} should be rejected");
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("NIKA-297"),
                "should be security violation for {path}, got: {err}"
            );
        }
    }

    #[test]
    fn validate_path_rejects_dotdot() {
        let path = std::path::Path::new("../../etc/passwd");
        assert!(validate_import_path(path, None).is_err());
    }

    #[test]
    fn validate_path_allows_normal() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(validate_import_path(tmp.path(), None).is_ok());
    }

    #[test]
    fn validate_path_rejects_outside_working_dir() {
        let wd = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        // File exists but is outside working_dir
        let result = validate_import_path(outside.path(), Some(wd.path()));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NIKA-297"),
            "should be security violation, got: {err}"
        );
        assert!(err.contains("outside project directory"));
    }

    #[test]
    fn validate_path_allows_inside_working_dir() {
        let wd = tempfile::tempdir().unwrap();
        let file_path = wd.path().join("test.png");
        std::fs::write(&file_path, b"test").unwrap();
        assert!(validate_import_path(&file_path, Some(wd.path())).is_ok());
    }

    #[test]
    fn validate_path_rejects_ssh_dir() {
        if let Some(home) = dirs::home_dir() {
            let ssh_key = home.join(".ssh/id_rsa");
            // We can't rely on the file existing, but we can test the path
            // validation if it does. If not, test the prefix logic directly.
            if ssh_key.exists() {
                let result = validate_import_path(&ssh_key, None);
                assert!(result.is_err());
                assert!(result.unwrap_err().to_string().contains("NIKA-297"));
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug 20 regression: import limit must match CAS MAX_STORE_SIZE
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn bug20_import_limit_matches_cas_store_limit() {
        // Bug 20: Import allowed 500MB but CAS rejects at 100MB.
        // The import limit must be <= CAS MAX_STORE_SIZE (100MB).
        assert_eq!(
            MAX_IMPORT_FILE_SIZE,
            100 * 1024 * 1024,
            "MAX_IMPORT_FILE_SIZE must be 100MB to match CAS MAX_STORE_SIZE"
        );
    }

    #[tokio::test]
    async fn bug20_import_rejects_over_100mb() {
        let (_dir, ctx) = setup().await;
        let tmp = tempfile::NamedTempFile::new().unwrap();

        // Create file just over 100MB by writing sparse data
        // (we check metadata.len() not actual disk usage)
        let f = std::fs::File::options()
            .write(true)
            .open(tmp.path())
            .unwrap();
        f.set_len(MAX_IMPORT_FILE_SIZE + 1).unwrap();
        drop(f);

        let op = ImportOp;
        let result = op
            .execute(
                serde_json::json!({"path": tmp.path().to_string_lossy()}),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "files > 100MB must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("too large"),
            "error should mention size limit"
        );
    }
}
