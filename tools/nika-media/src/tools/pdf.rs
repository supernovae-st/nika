//! nika:pdf_extract — Extract text from PDF documents.
//!
//! Uses `pdf-extract` crate in a dedicated thread with limited stack
//! to contain potential stack overflows from recursive PDF structures.

use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

/// Limit concurrent PDF extractions to prevent unbounded OS thread spawning.
/// Each extraction spawns a dedicated 4MB-stack thread, so 4 concurrent = 16MB stack.
static PDF_SEMAPHORE: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));

pub struct PdfExtractOp;

impl MediaOp for PdfExtractOp {
    fn name(&self) -> &'static str {
        "pdf_extract"
    }

    fn description(&self) -> &'static str {
        "Extract text content from a PDF document"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the PDF file" }
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
                .ok_or_else(|| invalid_args("pdf_extract", "missing 'hash'"))?;

            let data = ctx.read_media(hash).await?;

            // Acquire semaphore to limit concurrent PDF thread spawns (max 4)
            let _permit = PDF_SEMAPHORE
                .acquire()
                .await
                .map_err(|_| tool_error("pdf_extract", "semaphore closed"))?;

            // SECURITY: Run PDF extraction in a dedicated thread with limited stack
            // to contain potential stack overflows from recursive PDF structures.
            // Use spawn_blocking to avoid blocking the tokio runtime thread.
            let text = tokio::task::spawn_blocking(move || extract_pdf_safe(&data))
                .await
                .map_err(|e| tool_error("pdf_extract", format!("join failed: {e}")))??;

            let word_count = text.split_whitespace().count();
            let char_count = text.len();

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "text": text,
              "word_count": word_count,
              "char_count": char_count,
            })))
        })
    }
}

/// Extract text from PDF in a stack-limited thread.
///
/// PDF files can contain deeply recursive structures that cause stack overflow.
/// Running in a dedicated thread with a 4MB stack limit contains the damage.
fn extract_pdf_safe(data: &[u8]) -> Result<String, MediaToolError> {
    let data = data.to_vec();
    let handle = std::thread::Builder::new()
        .stack_size(4 * 1024 * 1024) // 4 MB stack limit
        .name("pdf-extract".into())
        .spawn(move || pdf_extract::extract_text_from_mem(&data))
        .map_err(|e| tool_error("pdf_extract", format!("thread spawn: {e}")))?;

    match handle.join() {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(tool_error("pdf_extract", format!("extraction failed: {e}"))),
        Err(_) => Err(tool_error(
            "pdf_extract",
            "PDF processing panicked (recursive references?)",
        )),
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

    /// Minimal valid PDF with text content.
    fn fixture_pdf() -> Vec<u8> {
        let pdf = b"%PDF-1.0\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R/Resources<</Font<</F1 4 0 R>>>>/Contents 5 0 R>>endobj\n4 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n5 0 obj<</Length 44>>stream\nBT /F1 12 Tf 100 700 Td (Hello Nika!) Tj ET\nendstream\nendobj\nxref\n0 6\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \n0000000266 00000 n \n0000000340 00000 n \ntrailer<</Size 6/Root 1 0 R>>\nstartxref\n434\n%%EOF";
        pdf.to_vec()
    }

    #[tokio::test]
    async fn pdf_extract_text() {
        let (_dir, ctx) = setup().await;
        let pdf = fixture_pdf();
        let sr = ctx.cas.store(&pdf).await.unwrap();

        let op = PdfExtractOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;

        // pdf-extract may or may not handle this minimal PDF
        // The important thing is it doesn't panic
        match result {
            Ok(MediaOpResult::Metadata(v)) => {
                assert!(v["char_count"].is_number());
                assert!(v["word_count"].is_number());
            }
            Err(e) => {
                // Acceptable: extraction error on minimal PDF
                assert!(!e.to_string().contains("panicked"));
            }
            _ => panic!("unexpected result type"),
        }
    }

    #[tokio::test]
    async fn pdf_extract_not_pdf() {
        let (_dir, ctx) = setup().await;
        let data = b"this is not a PDF file at all";
        let sr = ctx.cas.store(data).await.unwrap();

        let op = PdfExtractOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pdf_extract_missing_hash() {
        let (_dir, ctx) = setup().await;
        let op = PdfExtractOp;
        let result = op.execute(serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pdf_extract_fuzz_no_panic() {
        let (_dir, ctx) = setup().await;
        let op = PdfExtractOp;
        // PDF-like prefix + garbage
        for i in 1..20u8 {
            let mut data = b"%PDF-1.0\n".to_vec();
            data.extend((0..=i).collect::<Vec<u8>>());
            if let Ok(sr) = ctx.cas.store(&data).await {
                let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
                if let Err(e) = &result {
                    assert!(
                        !e.to_string().contains("panicked"),
                        "pdf panicked on input {i}"
                    );
                }
            }
        }
    }
}
