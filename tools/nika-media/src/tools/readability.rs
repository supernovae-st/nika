// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:readability — Article content extraction (Mozilla Readability).
//!
//! Extracts the main article content from a web page, stripping
//! navigation, footer, ads, and other non-content elements.
//! Uses `dom_smoothie` (Rust port of Mozilla's Readability.js).

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

/// Maximum HTML input size: 10 MB.
const MAX_HTML_SIZE: usize = 10 * 1024 * 1024;

pub struct ReadabilityOp;

impl MediaOp for ReadabilityOp {
    fn name(&self) -> &'static str {
        "readability"
    }

    fn description(&self) -> &'static str {
        "Extract main article content from HTML, stripping nav/footer/ads (Mozilla Readability)"
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
              "description": "Raw HTML string"
            },
            "url": {
              "type": "string",
              "description": "URL of the page (for resolving relative links)"
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
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if html.is_empty() {
                return Ok(MediaOpResult::Metadata(serde_json::json!({
                  "title": null,
                  "content": "",
                  "text_content": "",
                  "excerpt": null,
                  "char_count": 0
                })));
            }

            // dom_smoothie is CPU-intensive — run on compute pool
            let result = ctx
                .compute
                .compute(move || -> Result<serde_json::Value, MediaToolError> {
                    let mut readability =
                        dom_smoothie::Readability::new(html.as_str(), url.as_deref(), None)
                            .map_err(|e| {
                                tool_error("readability", format!("failed to initialize: {e}"))
                            })?;

                    let article = readability.parse().map_err(|e| {
                        tool_error("readability", format!("extraction failed: {e}"))
                    })?;

                    let content_str = article.content.to_string();
                    let text_content_str = article.text_content.to_string();
                    let char_count = text_content_str.len();

                    Ok(serde_json::json!({
                        "title": article.title,
                        "byline": article.byline,
                        "content": content_str,
                        "text_content": text_content_str,
                        "excerpt": article.excerpt,
                        "site_name": article.site_name,
                        "lang": article.lang,
                        "published_time": article.published_time,
                        "char_count": char_count,
                    }))
                })
                .await??;

            Ok(MediaOpResult::Metadata(result))
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
                "readability",
                format!(
                    "HTML content too large ({} bytes, max {} bytes)",
                    data.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        String::from_utf8(data).map_err(|_| {
            invalid_args(
                "readability",
                "CAS content is not valid UTF-8 (expected HTML)",
            )
        })
    } else if let Some(html) = args.get("html").and_then(|v| v.as_str()) {
        if html.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "readability",
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
            "readability",
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

    /// A realistic article HTML for testing.
    const ARTICLE_HTML: &str = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <title>The Future of Rust - A Deep Dive</title>
            <meta name="author" content="Alice Smith">
            <meta name="description" content="An in-depth look at Rust's future">
        </head>
        <body>
            <nav>
                <a href="/">Home</a>
                <a href="/blog">Blog</a>
            </nav>
            <article>
                <h1>The Future of Rust</h1>
                <p>Rust has become one of the most loved programming languages in the world.
                   Its focus on safety, performance, and concurrency makes it ideal for systems
                   programming, web development, and more. In this article, we explore what
                   the future holds for the Rust ecosystem.</p>
                <p>The Rust community has been growing steadily. With the introduction of
                   async/await, the language has become more accessible for network programming.
                   The borrow checker, once seen as a barrier, is now appreciated as a powerful
                   tool for preventing bugs at compile time.</p>
                <p>Looking ahead, improvements to compile times, better IDE support, and
                   expanding the standard library are key priorities. The Rust Foundation
                   continues to invest in the language's infrastructure and community.</p>
                <p>Many companies including Mozilla, Microsoft, Google, and Amazon are now
                   using Rust in production. The language's adoption in safety-critical systems,
                   embedded development, and WebAssembly is accelerating.</p>
                <p>In conclusion, Rust's future looks bright. The combination of performance,
                   safety, and a thriving community ensures that Rust will continue to grow
                   and evolve for years to come.</p>
            </article>
            <footer>
                <p>Copyright 2026 Example Corp</p>
                <a href="/privacy">Privacy Policy</a>
            </footer>
        </body>
        </html>
    "#;

    #[tokio::test]
    async fn extract_article_content() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"html": ARTICLE_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let text = v["text_content"].as_str().unwrap();
            assert!(text.contains("Rust"), "should extract article text: {text}");
            assert!(
                v["char_count"].as_u64().unwrap() > 100,
                "should have substantial content"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_title() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"html": ARTICLE_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let title = v["title"].as_str().unwrap();
            assert!(
                title.contains("Rust"),
                "should extract article title: {title}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn strips_navigation() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"html": ARTICLE_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let content = v["content"].as_str().unwrap();
            // Nav links should not appear in extracted content
            assert!(
                !content.contains("Privacy Policy"),
                "should strip footer: {content}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_from_cas_hash() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(ARTICLE_HTML.as_bytes()).await.unwrap();

        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(v["char_count"].as_u64().unwrap() > 0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_with_url() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": ARTICLE_HTML,
                    "url": "https://example.com/article"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(
                v["char_count"].as_u64().unwrap() > 0,
                "should extract content with URL context"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_empty_html() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"html": ""}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["char_count"], 0);
            assert_eq!(v["content"], "");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_missing_params() {
        let (_dir, ctx) = setup().await;
        let op = ReadabilityOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn extract_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ReadabilityOp;
        let result = op
            .execute(serde_json::json!({"html": ARTICLE_HTML}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
