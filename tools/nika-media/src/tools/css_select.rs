// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:css_select — CSS selector extraction from HTML content.
//!
//! Accepts a CAS hash or raw HTML string and a CSS selector.
//! Returns matching elements as text or HTML fragments.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::{MediaOp, MediaOpResult};

/// Maximum HTML input size: 10 MB.
const MAX_HTML_SIZE: usize = 10 * 1024 * 1024;

/// Maximum number of matches to return (prevents unbounded output).
const MAX_MATCHES: usize = 1000;

pub struct CssSelectOp;

impl MediaOp for CssSelectOp {
    fn name(&self) -> &'static str {
        "css_select"
    }

    fn description(&self) -> &'static str {
        "Extract elements from HTML using CSS selectors (returns text or HTML fragments)"
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
              "description": "Raw HTML string to query"
            },
            "selector": {
              "type": "string",
              "description": "CSS selector (e.g., 'div.product h2', '#main a')"
            },
            "output": {
              "type": "string",
              "enum": ["text", "html"],
              "description": "Output mode: 'text' (default) extracts text content, 'html' returns HTML fragments",
              "default": "text"
            },
            "limit": {
              "type": "integer",
              "description": "Maximum number of matches to return (default: 1000)",
              "default": 1000
            }
          },
          "required": ["selector"],
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

            let selector_str = args
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("css_select", "missing 'selector' parameter"))?
                .to_string();

            let output_mode = args
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("text")
                .to_string();

            if output_mode != "text" && output_mode != "html" {
                return Err(invalid_args(
                    "css_select",
                    format!("invalid output mode '{output_mode}', expected 'text' or 'html'"),
                ));
            }

            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(MAX_MATCHES as u64)
                .min(MAX_MATCHES as u64) as usize;

            let html = resolve_html(&args, ctx).await?;

            // Parse and select on the compute pool (can be CPU-intensive)
            let matches = ctx
                .compute
                .compute(move || -> Result<Vec<String>, MediaToolError> {
                    let document = scraper::Html::parse_document(&html);

                    let selector = scraper::Selector::parse(&selector_str).map_err(|e| {
                        invalid_args(
                            "css_select",
                            format!("invalid CSS selector '{selector_str}': {e}"),
                        )
                    })?;

                    let results: Vec<String> = document
                        .select(&selector)
                        .take(limit)
                        .map(|el| {
                            if output_mode == "html" {
                                el.html()
                            } else {
                                el.text().collect::<Vec<_>>().join("")
                            }
                        })
                        .collect();

                    Ok(results)
                })
                .await??;

            let count = matches.len();

            Ok(MediaOpResult::Metadata(serde_json::json!({
              "matches": matches,
              "count": count
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
                "css_select",
                format!(
                    "HTML content too large ({} bytes, max {} bytes)",
                    data.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        String::from_utf8(data).map_err(|_| {
            invalid_args(
                "css_select",
                "CAS content is not valid UTF-8 (expected HTML)",
            )
        })
    } else if let Some(html) = args.get("html").and_then(|v| v.as_str()) {
        if html.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "css_select",
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
            "css_select",
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

    const SAMPLE_HTML: &str = r#"
        <html>
        <body>
            <h1 id="title">Main Title</h1>
            <div class="product">
                <h2>Product A</h2>
                <p class="price">$10</p>
            </div>
            <div class="product">
                <h2>Product B</h2>
                <p class="price">$20</p>
            </div>
            <ul>
                <li>Item 1</li>
                <li>Item 2</li>
            </ul>
        </body>
        </html>
    "#;

    #[tokio::test]
    async fn select_by_tag() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": "h2"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0], "Product A");
            assert_eq!(matches[1], "Product B");
            assert_eq!(v["count"], 2);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_by_class() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": ".price"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0], "$10");
            assert_eq!(matches[1], "$20");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_by_id() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": "#title"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0], "Main Title");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_nested() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": "div.product h2"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0], "Product A");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_text_mode() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": SAMPLE_HTML,
                    "selector": ".product",
                    "output": "text"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 2);
            let text = matches[0].as_str().unwrap();
            assert!(
                text.contains("Product A"),
                "text should contain title: {text}"
            );
            assert!(text.contains("$10"), "text should contain price: {text}");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_html_mode() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": SAMPLE_HTML,
                    "selector": "li",
                    "output": "html"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 2);
            let html = matches[0].as_str().unwrap();
            assert!(
                html.contains("<li>"),
                "html mode should include tags: {html}"
            );
            assert!(html.contains("Item 1"), "html should contain text: {html}");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_invalid_selector() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": "!!!invalid"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn select_no_matches() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": SAMPLE_HTML, "selector": "span.nonexistent"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert!(matches.is_empty());
            assert_eq!(v["count"], 0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_missing_selector_param() {
        let (_dir, ctx) = setup().await;
        let op = CssSelectOp;
        let result = op
            .execute(serde_json::json!({"html": "<p>test</p>"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn select_from_cas_hash() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(SAMPLE_HTML.as_bytes()).await.unwrap();

        let op = CssSelectOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash, "selector": "h1"}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let matches = v["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0], "Main Title");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn select_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = CssSelectOp;
        let result = op
            .execute(
                serde_json::json!({"html": "<p>x</p>", "selector": "p"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
