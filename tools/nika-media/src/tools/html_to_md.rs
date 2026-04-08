// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:html_to_md — Convert HTML content to Markdown.
//!
//! Accepts either a CAS hash (reads HTML from store) or raw HTML string.
//! Uses `htmd` (turndown.js-inspired) for high-fidelity conversion.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

/// Maximum HTML input size: 10 MB.
const MAX_HTML_SIZE: usize = 10 * 1024 * 1024;

pub struct HtmlToMdOp;

impl MediaOp for HtmlToMdOp {
    fn name(&self) -> &'static str {
        "html_to_md"
    }

    fn description(&self) -> &'static str {
        "Convert HTML content to Markdown (from CAS hash or raw HTML string)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": {
              "type": "string",
              "description": "CAS hash of HTML content (blake3:...)"
            },
            "html": {
              "type": "string",
              "description": "Raw HTML string to convert"
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

            let html = resolve_html(&args, ctx).await?;

            if html.is_empty() {
                return Ok(MediaOpResult::Metadata(serde_json::json!({
                  "markdown": "",
                  "char_count": 0
                })));
            }

            // Convert on the compute pool (htmd can be CPU-intensive for large docs)
            let markdown = ctx
                .compute
                .compute(move || -> Result<String, MediaToolError> {
                    htmd::convert(&html)
                        .map_err(|e| tool_error("html_to_md", format!("conversion failed: {e}")))
                })
                .await??;

            let char_count = markdown.len();

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "markdown": markdown,
              "char_count": char_count
            })))
        })
    }
}

/// Resolve HTML content from either a CAS hash or raw HTML string.
async fn resolve_html(
    args: &serde_json::Value,
    ctx: &MediaToolContext,
) -> Result<String, MediaToolError> {
    if let Some(hash) = args.get("hash").and_then(|v| v.as_str()) {
        let data = ctx.read_media(hash).await?;
        if data.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "html_to_md",
                format!(
                    "HTML content too large ({} bytes, max {} bytes)",
                    data.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        String::from_utf8(data).map_err(|_| {
            invalid_args(
                "html_to_md",
                "CAS content is not valid UTF-8 (expected HTML)",
            )
        })
    } else if let Some(html) = args.get("html").and_then(|v| v.as_str()) {
        if html.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "html_to_md",
                format!(
                    "HTML string too large ({} bytes, max {} bytes)",
                    html.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        Ok(html.to_string())
    } else {
        Err(invalid_args(
            "html_to_md",
            "missing 'hash' or 'html' parameter",
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

    #[tokio::test]
    async fn convert_basic_html() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let result = op
            .execute(
                serde_json::json!({"html": "<h1>Hello</h1><p>World</p>"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let md = v["markdown"].as_str().unwrap();
            assert!(md.contains("Hello"), "should contain heading text");
            assert!(md.contains("World"), "should contain paragraph text");
            assert!(v["char_count"].as_u64().unwrap() > 0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_html_with_tables() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let html =
            "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Alice</td><td>30</td></tr></table>";
        let result = op
            .execute(serde_json::json!({"html": html}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let md = v["markdown"].as_str().unwrap();
            assert!(md.contains("Name"), "should contain table header");
            assert!(md.contains("Alice"), "should contain table data");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_html_with_code_blocks() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let html = "<pre><code>fn main() { }</code></pre>";
        let result = op
            .execute(serde_json::json!({"html": html}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let md = v["markdown"].as_str().unwrap();
            assert!(
                md.contains("fn main()"),
                "should contain code content: {md}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_html_with_links() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let html = r#"<a href="https://example.com">Example</a>"#;
        let result = op
            .execute(serde_json::json!({"html": html}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let md = v["markdown"].as_str().unwrap();
            assert!(md.contains("[Example]"), "should contain link text: {md}");
            assert!(
                md.contains("https://example.com"),
                "should contain link URL: {md}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_from_cas_hash() {
        let (_dir, ctx) = setup().await;
        let html = b"<h2>From CAS</h2><p>Stored content</p>";
        let sr = ctx.cas.store(html).await.unwrap();

        let op = HtmlToMdOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let md = v["markdown"].as_str().unwrap();
            assert!(md.contains("From CAS"), "should convert CAS content: {md}");
            assert!(
                md.contains("Stored content"),
                "should contain paragraph: {md}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_empty_html() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let result = op
            .execute(serde_json::json!({"html": ""}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["markdown"], "");
            assert_eq!(v["char_count"], 0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn convert_missing_params() {
        let (_dir, ctx) = setup().await;
        let op = HtmlToMdOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn convert_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = HtmlToMdOp;
        let result = op
            .execute(serde_json::json!({"html": "<p>test</p>"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
