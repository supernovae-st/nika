// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:extract_links — Rich SEO link classification from HTML.
//!
//! Classifies links by DOM context (nav, header, footer, content, sidebar),
//! internal vs external (via eTLD+1 comparison), and extracts rel attributes,
//! anchor text, dofollow/nofollow status.

use std::future::Future;
use std::pin::Pin;

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::{MediaOp, MediaOpResult};

/// Maximum HTML input size: 10 MB.
const MAX_HTML_SIZE: usize = 10 * 1024 * 1024;

/// Maximum number of links to return.
const MAX_LINKS: usize = 5000;

pub struct ExtractLinksOp;

impl MediaOp for ExtractLinksOp {
    fn name(&self) -> &'static str {
        "extract_links"
    }

    fn description(&self) -> &'static str {
        "Extract and classify links from HTML by context (nav/content/footer), internal/external, nofollow"
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
            "base_url": {
              "type": "string",
              "description": "Base URL for resolving relative links and classifying internal/external"
            }
          },
          "required": ["base_url"],
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

            let base_url_str = args
                .get("base_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("extract_links", "missing 'base_url' parameter"))?
                .to_string();

            let html = resolve_html(&args, ctx).await?;

            // Extract links on compute pool
            let result = ctx
                .compute
                .compute(move || extract_links(&html, &base_url_str))
                .await??;

            Ok(MediaOpResult::Metadata(result))
        })
    }
}

/// A classified link.
#[derive(Debug)]
struct LinkInfo {
    href: String,
    text: String,
    rel: String,
    nofollow: bool,
    context: String,
}

impl LinkInfo {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "href": self.href,
            "text": self.text,
            "rel": self.rel,
            "nofollow": self.nofollow,
            "context": self.context,
        })
    }
}

/// Extract and classify all links from HTML.
fn extract_links(html: &str, base_url_str: &str) -> Result<serde_json::Value, MediaToolError> {
    let base_url = url::Url::parse(base_url_str).map_err(|e| {
        invalid_args(
            "extract_links",
            format!("invalid base_url '{base_url_str}': {e}"),
        )
    })?;

    let base_domain = registrable_domain(base_url.host_str().unwrap_or(""));

    let document = scraper::Html::parse_document(html);
    let a_selector = scraper::Selector::parse("a[href]")
        .map_err(|e| tool_error("extract_links", format!("selector error: {e}")))?;

    let mut internal_links: Vec<serde_json::Value> = Vec::new();
    let mut external_links: Vec<serde_json::Value> = Vec::new();

    let mut total_count = 0usize;
    let mut internal_count = 0usize;
    let mut external_count = 0usize;
    let mut nofollow_count = 0usize;

    for element in document.select(&a_selector) {
        if total_count >= MAX_LINKS {
            break;
        }

        let el_val = element.value();
        let raw_href = match el_val.attr("href") {
            Some(h) => h.trim(),
            None => continue,
        };

        // Skip anchors, javascript:, mailto:, tel:
        if raw_href.is_empty()
            || raw_href.starts_with('#')
            || raw_href.starts_with("javascript:")
            || raw_href.starts_with("mailto:")
            || raw_href.starts_with("tel:")
        {
            continue;
        }

        // Resolve relative URLs
        let resolved = match base_url.join(raw_href) {
            Ok(u) => u.to_string(),
            Err(_) => raw_href.to_string(),
        };

        let text: String = element
            .text()
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();
        let rel = el_val.attr("rel").unwrap_or("").to_string();
        let nofollow = rel.contains("nofollow");

        // Classify context by walking ancestors
        let context = classify_context(&element);

        // Classify internal/external via eTLD+1
        let link_url = url::Url::parse(&resolved);
        let is_internal = match &link_url {
            Ok(u) => {
                let link_domain = registrable_domain(u.host_str().unwrap_or(""));
                link_domain == base_domain
            }
            Err(_) => {
                // If we can't parse, it's likely a relative URL (internal)
                true
            }
        };

        let info = LinkInfo {
            href: resolved,
            text,
            rel,
            nofollow,
            context,
        };

        total_count += 1;
        if nofollow {
            nofollow_count += 1;
        }

        if is_internal {
            internal_count += 1;
            internal_links.push(info.to_json());
        } else {
            external_count += 1;
            external_links.push(info.to_json());
        }
    }

    Ok(serde_json::json!({
        "internal": internal_links,
        "external": external_links,
        "summary": {
            "total": total_count,
            "internal": internal_count,
            "external": external_count,
            "nofollow": nofollow_count,
        }
    }))
}

/// Classify link context by walking ancestor elements.
fn classify_context(element: &scraper::ElementRef) -> String {
    use scraper::Node;

    let mut current = element.parent();
    while let Some(node) = current {
        if let Node::Element(el) = node.value() {
            let tag = el.name();
            match tag {
                "nav" => return "nav".to_string(),
                "header" => return "header".to_string(),
                "footer" => return "footer".to_string(),
                "aside" => return "sidebar".to_string(),
                "main" | "article" | "section" => return "content".to_string(),
                _ => {}
            }
            // Check role attribute
            if let Some(role) = el.attr("role") {
                match role {
                    "navigation" => return "nav".to_string(),
                    "banner" => return "header".to_string(),
                    "contentinfo" => return "footer".to_string(),
                    "complementary" => return "sidebar".to_string(),
                    "main" => return "content".to_string(),
                    _ => {}
                }
            }
        }
        current = node.parent();
    }

    "content".to_string() // Default to content
}

/// Extract the registrable domain (eTLD+1) using the `psl` crate.
fn registrable_domain(host: &str) -> String {
    let host_bytes = host.as_bytes();
    match psl::domain(host_bytes) {
        Some(domain) => {
            // psl::domain returns bytes, convert back
            std::str::from_utf8(domain.as_bytes())
                .unwrap_or(host)
                .to_lowercase()
        }
        None => host.to_lowercase(),
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
                "extract_links",
                format!(
                    "HTML content too large ({} bytes, max {} bytes)",
                    data.len(),
                    MAX_HTML_SIZE
                ),
            ));
        }
        String::from_utf8(data).map_err(|_| {
            invalid_args(
                "extract_links",
                "CAS content is not valid UTF-8 (expected HTML)",
            )
        })
    } else if let Some(html) = args.get("html").and_then(|v| v.as_str()) {
        if html.len() > MAX_HTML_SIZE {
            return Err(invalid_args(
                "extract_links",
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
            "extract_links",
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

    const LINKS_HTML: &str = r#"
        <html>
        <body>
            <nav>
                <a href="/about">About</a>
                <a href="/contact">Contact</a>
            </nav>
            <header>
                <a href="/">Home</a>
            </header>
            <main>
                <article>
                    <a href="https://example.com/article">Internal Article</a>
                    <a href="https://other.com/page" rel="nofollow">External Link</a>
                    <a href="https://blog.example.com/post">Subdomain Post</a>
                </article>
            </main>
            <aside>
                <a href="https://ads.com/click">Ad Link</a>
            </aside>
            <footer>
                <a href="/privacy">Privacy</a>
                <a href="https://twitter.com/example" rel="nofollow noopener">Twitter</a>
            </footer>
        </body>
        </html>
    "#;

    #[tokio::test]
    async fn extract_internal_links() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let internal = v["internal"].as_array().unwrap();
            assert!(
                internal.len() >= 4,
                "should have internal links: {internal:?}"
            );
            // Check that relative links were resolved
            let hrefs: Vec<&str> = internal.iter().filter_map(|l| l["href"].as_str()).collect();
            assert!(
                hrefs.iter().any(|h| h.contains("/about")),
                "should resolve /about: {hrefs:?}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_external_links() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let external = v["external"].as_array().unwrap();
            assert!(
                !external.is_empty(),
                "should have external links: {external:?}"
            );
            let hrefs: Vec<&str> = external.iter().filter_map(|l| l["href"].as_str()).collect();
            assert!(
                hrefs.iter().any(|h| h.contains("other.com")),
                "should find other.com: {hrefs:?}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn classify_subdomain_as_internal() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let internal = v["internal"].as_array().unwrap();
            let hrefs: Vec<&str> = internal.iter().filter_map(|l| l["href"].as_str()).collect();
            assert!(
                hrefs.iter().any(|h| h.contains("blog.example.com")),
                "subdomain should be internal: {hrefs:?}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn detect_nofollow() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let summary = &v["summary"];
            assert!(
                summary["nofollow"].as_u64().unwrap() >= 2,
                "should detect nofollow links: {summary}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn classify_nav_context() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let internal = v["internal"].as_array().unwrap();
            let nav_links: Vec<&serde_json::Value> =
                internal.iter().filter(|l| l["context"] == "nav").collect();
            assert!(
                !nav_links.is_empty(),
                "should find nav context links: {internal:?}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn classify_footer_context() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            // Combine both internal and external
            let all_links: Vec<&serde_json::Value> = v["internal"]
                .as_array()
                .unwrap()
                .iter()
                .chain(v["external"].as_array().unwrap().iter())
                .collect();
            let footer_links: Vec<&&serde_json::Value> = all_links
                .iter()
                .filter(|l| l["context"] == "footer")
                .collect();
            assert!(
                !footer_links.is_empty(),
                "should find footer context links: {all_links:?}"
            );
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn summary_counts() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            let summary = &v["summary"];
            let total = summary["total"].as_u64().unwrap();
            let internal = summary["internal"].as_u64().unwrap();
            let external = summary["external"].as_u64().unwrap();
            assert_eq!(total, internal + external);
            assert!(total > 0, "should have some links");
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn extract_from_cas_hash() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(LINKS_HTML.as_bytes()).await.unwrap();

        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "hash": sr.hash,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert!(v["summary"]["total"].as_u64().unwrap() > 0);
        } else {
            panic!("expected Metadata result");
        }
    }

    #[tokio::test]
    async fn missing_base_url() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(serde_json::json!({"html": "<a href='/x'>x</a>"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn invalid_base_url() {
        let (_dir, ctx) = setup().await;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": "<a href='/x'>x</a>",
                    "base_url": "not-a-url"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn extract_cancelled() {
        let (_dir, ctx) = setup().await;
        ctx.cancel.cancel();
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": LINKS_HTML,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn skips_anchors_and_mailto() {
        let (_dir, ctx) = setup().await;
        let html = r##"
            <a href="#section">Anchor</a>
            <a href="mailto:test@example.com">Email</a>
            <a href="tel:+1234567890">Phone</a>
            <a href="javascript:void(0)">JS</a>
            <a href="https://example.com/real">Real Link</a>
        "##;
        let op = ExtractLinksOp;
        let result = op
            .execute(
                serde_json::json!({
                    "html": html,
                    "base_url": "https://example.com"
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Metadata(v) = result {
            assert_eq!(
                v["summary"]["total"].as_u64().unwrap(),
                1,
                "should only count the real link"
            );
        } else {
            panic!("expected Metadata result");
        }
    }
}
