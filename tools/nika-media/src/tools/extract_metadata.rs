// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:extract_metadata — Extract SEO metadata from HTML pages.
//!
//! Extracts title, description, Open Graph, Twitter Cards, JSON-LD,
//! canonical URL, favicon, RSS/Atom feeds, robots directives, and author.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::invalid_args;
use super::error::MediaToolError;
use super::{MediaOp, MediaOpResult};

/// Maximum HTML input size: 10 MB.
const MAX_HTML_SIZE: usize = 10 * 1024 * 1024;

pub struct ExtractMetadataOp;

impl MediaOp for ExtractMetadataOp {
    fn name(&self) -> &'static str {
        "extract_metadata"
    }

    fn description(&self) -> &'static str {
        "Extract SEO metadata from HTML: title, description, OG, Twitter, JSON-LD, canonical, favicon, feeds"
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
              "description": "Raw HTML string to extract metadata from"
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

            // Extract metadata on compute pool
            let metadata = ctx
                .compute
                .compute(move || extract_all_metadata(&html))
                .await??;

            Ok(MediaOpResult::Metadata(metadata))
        })
    }
}

/// Extract all metadata from HTML.
fn extract_all_metadata(html: &str) -> Result<serde_json::Value, MediaToolError> {
    let document = scraper::Html::parse_document(html);

    let title = extract_title(&document);
    let description = extract_meta_content(&document, "description");
    let author = extract_meta_content(&document, "author");
    let robots = extract_meta_content(&document, "robots");
    let canonical = extract_canonical(&document);
    let favicon = extract_favicon(&document);
    let og = extract_og_tags(&document);
    let twitter = extract_twitter_tags(&document);
    let json_ld = extract_json_ld(&document);
    let feeds = extract_feeds(&document);

    let mut result = serde_json::json!({});
    let obj = result
        .as_object_mut()
        .expect("json!({}) is always an object");

    if let Some(t) = title {
        obj.insert("title".to_string(), serde_json::Value::String(t));
    }
    if let Some(d) = description {
        obj.insert("description".to_string(), serde_json::Value::String(d));
    }
    if let Some(a) = author {
        obj.insert("author".to_string(), serde_json::Value::String(a));
    }
    if let Some(r) = robots {
        obj.insert("robots".to_string(), serde_json::Value::String(r));
    }
    if let Some(c) = canonical {
        obj.insert("canonical".to_string(), serde_json::Value::String(c));
    }
    if let Some(f) = favicon {
        obj.insert("favicon".to_string(), serde_json::Value::String(f));
    }
    if !og.is_empty() {
        obj.insert("og".to_string(), serde_json::json!(og));
    }
    if !twitter.is_empty() {
        obj.insert("twitter".to_string(), serde_json::json!(twitter));
    }
    if !json_ld.is_empty() {
        obj.insert("json_ld".to_string(), serde_json::Value::Array(json_ld));
    }
    if !feeds.is_empty() {
        obj.insert("feeds".to_string(), serde_json::Value::Array(feeds));
    }

    Ok(result)
}

/// Extract the <title> tag content.
fn extract_title(doc: &scraper::Html) -> Option<String> {
    let selector = scraper::Selector::parse("title").ok()?;
    doc.select(&selector)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract a <meta name="..." content="..."> value.
fn extract_meta_content(doc: &scraper::Html, name: &str) -> Option<String> {
    let selector = scraper::Selector::parse("meta").ok()?;
    for el in doc.select(&selector) {
        let el_val = el.value();
        if let Some(n) = el_val.attr("name") {
            if n.eq_ignore_ascii_case(name) {
                if let Some(content) = el_val.attr("content") {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    }
    None
}

/// Extract <link rel="canonical" href="...">
fn extract_canonical(doc: &scraper::Html) -> Option<String> {
    let selector = scraper::Selector::parse(r#"link[rel="canonical"]"#).ok()?;
    doc.select(&selector)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract favicon from <link rel="icon"> or <link rel="shortcut icon">
fn extract_favicon(doc: &scraper::Html) -> Option<String> {
    // Try rel="icon" first, then "shortcut icon"
    for rel_val in &["icon", "shortcut icon"] {
        let selector_str = format!(r#"link[rel="{rel_val}"]"#);
        let selector = match scraper::Selector::parse(&selector_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(el) = doc.select(&selector).next() {
            if let Some(href) = el.value().attr("href") {
                let trimmed = href.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

/// Extract Open Graph meta tags (og:*).
fn extract_og_tags(doc: &scraper::Html) -> std::collections::HashMap<String, String> {
    let mut og = std::collections::HashMap::new();
    let selector = match scraper::Selector::parse(r#"meta[property]"#) {
        Ok(s) => s,
        Err(_) => return og,
    };

    for el in doc.select(&selector) {
        let el_val = el.value();
        if let Some(property) = el_val.attr("property") {
            if let Some(stripped) = property.strip_prefix("og:") {
                if let Some(content) = el_val.attr("content") {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        og.insert(stripped.to_string(), trimmed);
                    }
                }
            }
        }
    }
    og
}

/// Extract Twitter Card meta tags (twitter:*).
fn extract_twitter_tags(doc: &scraper::Html) -> std::collections::HashMap<String, String> {
    let mut twitter = std::collections::HashMap::new();
    let selector = match scraper::Selector::parse("meta") {
        Ok(s) => s,
        Err(_) => return twitter,
    };

    for el in doc.select(&selector) {
        let el_val = el.value();
        // Twitter uses both name="twitter:*" and property="twitter:*"
        let key = el_val.attr("name").or_else(|| el_val.attr("property"));

        if let Some(name) = key {
            if let Some(stripped) = name.strip_prefix("twitter:") {
                if let Some(content) = el_val.attr("content") {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        twitter.insert(stripped.to_string(), trimmed);
                    }
                }
            }
        }
    }
    twitter
}

/// Extract JSON-LD scripts.
fn extract_json_ld(doc: &scraper::Html) -> Vec<serde_json::Value> {
    let selector = match scraper::Selector::parse(r#"script[type="application/ld+json"]"#) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for el in doc.select(&selector) {
        let text: String = el.text().collect();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            // Try to parse as JSON; if valid, include it
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                results.push(parsed);
            }
        }
    }
    results
}

/// Extract RSS/Atom feed links.
fn extract_feeds(doc: &scraper::Html) -> Vec<serde_json::Value> {
    let mut feeds = Vec::new();

    for feed_type in &["application/rss+xml", "application/atom+xml"] {
        let selector_str = format!(r#"link[type="{feed_type}"]"#);
        let selector = match scraper::Selector::parse(&selector_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for el in doc.select(&selector) {
            let el_val = el.value();
            let href = el_val.attr("href").unwrap_or("").trim().to_string();
            let title = el_val.attr("title").unwrap_or("").trim().to_string();
            if !href.is_empty() {
                feeds.push(serde_json::json!({
                    "type": feed_type,
                    "href": href,
                    "title": title
                }));
            }
        }
    }
    feeds
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
                "extract_metadata",
                format!(
                    "HTML content too large ({} bytes, max {} bytes)",
                    data.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        String::from_utf8(data).map_err(|_| {
            invalid_args(
                "extract_metadata",
                "CAS content is not valid UTF-8 (expected HTML)",
            )
        })
    } else if let Some(html) = args.get("html").and_then(|v| v.as_str()) {
        if html.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "extract_metadata",
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
            "extract_metadata",
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

    const FULL_HTML: &str = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <title>My Page Title</title>
            <meta name="description" content="A great page about things">
            <meta name="author" content="Jane Doe">
            <meta name="robots" content="index, follow">
            <meta property="og:title" content="OG Title">
            <meta property="og:description" content="OG Description">
            <meta property="og:image" content="https://example.com/image.jpg">
            <meta property="og:type" content="article">
            <meta name="twitter:card" content="summary_large_image">
            <meta name="twitter:title" content="Twitter Title">
            <meta name="twitter:image" content="https://example.com/tw-image.jpg">
            <link rel="canonical" href="https://example.com/page">
            <link rel="icon" href="/favicon.ico">
            <link rel="alternate" type="application/rss+xml" title="RSS Feed" href="/feed.xml">
            <link rel="alternate" type="application/atom+xml" title="Atom Feed" href="/atom.xml">
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "Article",
                "headline": "My Article"
            }
            </script>
        </head>
        <body><h1>Content</h1></body>
        </html>
    "#;

    #[tokio::test]
    async fn extract_title() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["title"], "My Page Title");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_description() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["description"], "A great page about things");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_og_tags() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["og"]["title"], "OG Title");
            assert_eq!(v["og"]["description"], "OG Description");
            assert_eq!(v["og"]["image"], "https://example.com/image.jpg");
            assert_eq!(v["og"]["type"], "article");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_twitter_cards() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["twitter"]["card"], "summary_large_image");
            assert_eq!(v["twitter"]["title"], "Twitter Title");
            assert_eq!(v["twitter"]["image"], "https://example.com/tw-image.jpg");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_json_ld() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let json_ld = v["json_ld"].as_array().unwrap();
            assert_eq!(json_ld.len(), 1);
            assert_eq!(json_ld[0]["@type"], "Article");
            assert_eq!(json_ld[0]["headline"], "My Article");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_canonical() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["canonical"], "https://example.com/page");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_favicon() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["favicon"], "/favicon.ico");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_feeds() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let feeds = v["feeds"].as_array().unwrap();
            assert_eq!(feeds.len(), 2);
            assert_eq!(feeds[0]["href"], "/feed.xml");
            assert_eq!(feeds[0]["title"], "RSS Feed");
            assert_eq!(feeds[1]["href"], "/atom.xml");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_empty_page() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(
                serde_json::json!({"html": "<html><body></body></html>"}),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            // Empty page should return empty object with no fields
            assert!(v.get("title").is_none());
            assert!(v.get("og").is_none());
            assert!(v.get("twitter").is_none());
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_author_and_robots() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": FULL_HTML}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["author"], "Jane Doe");
            assert_eq!(v["robots"], "index, follow");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_from_cas_hash() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(FULL_HTML.as_bytes()).await.unwrap();

        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(v["title"], "My Page Title");
            assert!(v["og"]["title"].is_string());
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_missing_params() {
        let (_dir, ctx) = setup().await;
        let op = ExtractMetadataOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn extract_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ExtractMetadataOp;
        let result = op
            .execute(serde_json::json!({"html": "<html></html>"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
