//! Post-processing extraction for the fetch: verb.

use nika_core::ast::extract::ExtractMode;

use crate::error::NikaError;

/// Parse HTTP `Link:` headers for hreflang alternate links.
///
/// Format: `<https://example.com/fr/>; rel="alternate"; hreflang="fr"`
///
/// Returns a vec of `{"lang": "fr", "href": "https://example.com/fr/"}` objects.
pub(crate) fn parse_link_header_hreflang(link_values: &[String]) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    for value in link_values {
        // A single Link header can have multiple comma-separated entries
        for entry in value.split(',') {
            let entry = entry.trim();
            // Extract URL: <...>
            let url = match (entry.find('<'), entry.find('>')) {
                (Some(start), Some(end)) if start < end => &entry[start + 1..end],
                _ => continue,
            };
            let params = &entry[entry.find('>').unwrap_or(0)..];
            // Check rel="alternate"
            let is_alternate = params
                .split(';')
                .any(|p| {
                    let p = p.trim().to_lowercase();
                    p == "rel=\"alternate\"" || p == "rel=alternate"
                });
            if !is_alternate {
                continue;
            }
            // Extract hreflang="xx"
            let hreflang = params.split(';').find_map(|p| {
                let p = p.trim();
                let lower = p.to_lowercase();
                if lower.starts_with("hreflang=") {
                    Some(
                        p[9..]
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string(),
                    )
                } else {
                    None
                }
            });
            if let Some(lang) = hreflang {
                results.push(serde_json::json!({
                    "lang": lang,
                    "href": url,
                }));
            }
        }
    }
    results
}
#[cfg(not(all(
    feature = "fetch-markdown",
    feature = "fetch-html",
    feature = "fetch-article",
    feature = "fetch-feed"
)))]
use crate::error_domains::ExecutionError;

/// Apply extraction to a fetch response body (without base URL).
/// Convenience wrapper for tests and callers that don't have a base URL.
#[cfg(test)]
pub(crate) fn apply_extract(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
) -> Result<String, NikaError> {
    apply_extract_with_base(body, extract, selector, None)
}

/// Apply extraction with an optional base URL for resolving relative links.
pub(crate) fn apply_extract_with_base(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, NikaError> {
    let mode = match extract {
        None => return Ok(body.to_string()),
        Some(m) => m,
    };

    match mode {
        #[cfg(feature = "fetch-markdown")]
        ExtractMode::Markdown => {
            // Strip <style> and <script> tags before conversion to avoid
            // CSS/JS content appearing as plain text in the markdown output.
            let clean = strip_non_content_tags(body);
            htmd::convert(&clean).map_err(|e| NikaError::ExtractError { reason: format!("HTML to markdown: {e}") })
        }

        #[cfg(feature = "fetch-html")]
        ExtractMode::Text => extract_text(body, selector),

        #[cfg(feature = "fetch-html")]
        ExtractMode::Selector => {
            let css = selector.ok_or_else(|| {
                NikaError::ExtractError {
                    reason: "extract: selector requires 'selector' field".to_string(),
                }
            })?;
            extract_html_by_selector(body, css)
        }

        #[cfg(feature = "fetch-html")]
        ExtractMode::Metadata => extract_metadata_json(body, base_url),

        #[cfg(feature = "fetch-html")]
        ExtractMode::Links => extract_links_json(body, base_url),

        #[cfg(feature = "fetch-article")]
        ExtractMode::Article => {
            let mut readability =
                dom_smoothie::Readability::new(body, None, None)
                    .map_err(|e| NikaError::ExtractError { reason: format!("Readability init failed: {e}") })?;
            let article = readability.parse().map_err(|e| {
                NikaError::ExtractError { reason: format!("Readability parse failed: {e}") }
            })?;
            Ok(serde_json::json!({
                "title": article.title,
                "content": article.content.to_string(),
                "text_content": article.text_content.to_string(),
                "excerpt": article.excerpt,
                "byline": article.byline,
            })
            .to_string())
        }

        #[cfg(feature = "fetch-feed")]
        ExtractMode::Feed => {
            let feed = feed_rs::parser::parse(body.as_bytes())
                .map_err(|e| NikaError::ExtractError { reason: format!("Feed parse failed: {e}") })?;
            let entries: Vec<serde_json::Value> = feed
                .entries
                .iter()
                .take(100)
                .map(|entry| {
                    serde_json::json!({
                        "title": entry.title.as_ref().map(|t| &t.content),
                        "url": entry.links.first().map(|l| &l.href),
                        "published": entry.published.map(|d| d.to_rfc3339()),
                        "summary": entry.summary.as_ref().map(|s| &s.content),
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "title": feed.title.map(|t| t.content),
                "entry_count": entries.len(),
                "total_count": feed.entries.len(),
                "entries": entries,
            })
            .to_string())
        }

        ExtractMode::Jsonpath => {
            let path = selector.ok_or_else(|| {
                NikaError::ExtractError {
                    reason: "extract: jsonpath requires 'selector' field with JSONPath expression"
                        .to_string(),
                }
            })?;
            extract_jsonpath(body, path)
        }

        // LlmTxt is handled in fetch.rs before apply_extract is called.
        // If it somehow reaches here, return the body as-is.
        ExtractMode::LlmTxt => Ok(body.to_string()),

        #[cfg(not(feature = "fetch-markdown"))]
        ExtractMode::Markdown => Err(ExecutionError::ExtractFailed {
            reason: "extract: markdown requires feature 'fetch-markdown'. Build with: cargo build --features fetch-markdown".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-html"))]
        ExtractMode::Text | ExtractMode::Selector | ExtractMode::Metadata | ExtractMode::Links => Err(ExecutionError::ExtractFailed {
            reason: "extract: text/selector/metadata/links requires feature 'fetch-html'. Build with: cargo build --features fetch-html".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-article"))]
        ExtractMode::Article => Err(ExecutionError::ExtractFailed {
            reason: "extract: article requires feature 'fetch-article'. Build with: cargo build --features fetch-article".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-feed"))]
        ExtractMode::Feed => Err(ExecutionError::ExtractFailed {
            reason: "extract: feed requires feature 'fetch-feed'. Build with: cargo build --features fetch-feed".to_string(),
        }
        .into()),
    }
}

/// Strip `<style>`, `<script>`, and `<noscript>` tags and their content from HTML.
/// Used before markdown/text extraction to prevent CSS/JS from appearing as text.
#[cfg(any(feature = "fetch-markdown", feature = "fetch-html"))]
fn strip_non_content_tags(html: &str) -> String {
    // One regex per tag (regex crate doesn't support backreferences).
    static STYLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::RegexBuilder::new(r"<style\b[^>]*>[\s\S]*?</style>")
            .case_insensitive(true)
            .build()
            .expect("STYLE_RE is a valid regex")
    });
    static SCRIPT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::RegexBuilder::new(r"<script\b[^>]*>[\s\S]*?</script>")
            .case_insensitive(true)
            .build()
            .expect("SCRIPT_RE is a valid regex")
    });
    static NOSCRIPT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::RegexBuilder::new(r"<noscript\b[^>]*>[\s\S]*?</noscript>")
            .case_insensitive(true)
            .build()
            .expect("NOSCRIPT_RE is a valid regex")
    });
    let result = STYLE.replace_all(html, "");
    let result = SCRIPT.replace_all(&result, "");
    NOSCRIPT.replace_all(&result, "").into_owned()
}

#[cfg(feature = "fetch-html")]
fn extract_text(html: &str, selector: Option<&str>) -> Result<String, NikaError> {
    // Strip style/script/noscript tags to avoid CSS/JS in visible text
    let clean = strip_non_content_tags(html);
    let document = scraper::Html::parse_document(&clean);
    if let Some(css) = selector {
        let sel = scraper::Selector::parse(css).map_err(|_| NikaError::ExtractError {
            reason: format!("Invalid CSS selector: {css}"),
        })?;
        let texts: Vec<String> = document
            .select(&sel)
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        Ok(texts.join("\n"))
    } else {
        Ok(document.root_element().text().collect::<Vec<_>>().join(" "))
    }
}

#[cfg(feature = "fetch-html")]
fn extract_html_by_selector(html: &str, css: &str) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse(css).map_err(|_| NikaError::ExtractError {
        reason: format!("Invalid CSS selector: {css}"),
    })?;
    let parts: Vec<String> = document.select(&sel).map(|el| el.html()).collect();
    Ok(parts.join("\n"))
}

#[cfg(feature = "fetch-html")]
fn extract_metadata_json(html: &str, base_url: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let mut meta = serde_json::Map::new();

    // <title>
    let title_sel = scraper::Selector::parse("title").expect("static CSS selector");
    if let Some(el) = document.select(&title_sel).next() {
        meta.insert(
            "title".into(),
            el.text().collect::<String>().trim().to_string().into(),
        );
    }

    // meta name="description"
    let meta_sel = scraper::Selector::parse("meta[name=description]").expect("static CSS selector");
    if let Some(el) = document.select(&meta_sel).next() {
        if let Some(content) = el.value().attr("content") {
            meta.insert("description".into(), content.into());
        }
    }

    // OG tags
    let mut og = serde_json::Map::new();
    for prop in &["title", "description", "image", "url", "type", "site_name"] {
        let sel_str = format!("meta[property=\"og:{}\"]", prop);
        let sel = match scraper::Selector::parse(&sel_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(el) = document.select(&sel).next() {
            if let Some(content) = el.value().attr("content") {
                og.insert(prop.to_string(), content.into());
            }
        }
    }
    if !og.is_empty() {
        meta.insert("og".into(), og.into());
    }

    // Twitter cards
    let mut tw = serde_json::Map::new();
    for name in &["card", "title", "description", "image", "site", "creator"] {
        let sel_str = format!("meta[name=\"twitter:{}\"]", name);
        let sel = match scraper::Selector::parse(&sel_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(el) = document.select(&sel).next() {
            if let Some(content) = el.value().attr("content") {
                tw.insert(name.to_string(), content.into());
            }
        }
    }
    if !tw.is_empty() {
        meta.insert("twitter".into(), tw.into());
    }

    // JSON-LD
    let jsonld_sel = scraper::Selector::parse("script[type=\"application/ld+json\"]")
        .expect("static CSS selector");
    let json_ld: Vec<serde_json::Value> = document
        .select(&jsonld_sel)
        .filter_map(|el| serde_json::from_str(&el.text().collect::<String>()).ok())
        .collect();
    if !json_ld.is_empty() {
        meta.insert("json_ld".into(), json_ld.into());
    }

    // Canonical
    let canon_sel = scraper::Selector::parse("link[rel=canonical]").expect("static CSS selector");
    if let Some(el) = document.select(&canon_sel).next() {
        if let Some(href) = el.value().attr("href") {
            meta.insert("canonical".into(), href.into());
        }
    }

    // Favicon (link[rel~=icon])
    let favicon_sel = scraper::Selector::parse("link[rel~=icon], link[rel=\"shortcut icon\"]")
        .expect("static CSS selector");
    if let Some(el) = document.select(&favicon_sel).next() {
        if let Some(href) = el.value().attr("href") {
            meta.insert("favicon".into(), href.into());
        }
    }

    // Author (meta name="author")
    let author_sel = scraper::Selector::parse("meta[name=author]").expect("static CSS selector");
    if let Some(el) = document.select(&author_sel).next() {
        if let Some(content) = el.value().attr("content") {
            meta.insert("author".into(), content.into());
        }
    }

    // Robots (meta name="robots")
    let robots_sel = scraper::Selector::parse("meta[name=robots]").expect("static CSS selector");
    if let Some(el) = document.select(&robots_sel).next() {
        if let Some(content) = el.value().attr("content") {
            meta.insert("robots".into(), content.into());
        }
    }

    // RSS/Atom feeds (link[type="application/rss+xml"] or link[type="application/atom+xml"])
    let feed_sel = scraper::Selector::parse(
        "link[type=\"application/rss+xml\"], link[type=\"application/atom+xml\"]",
    )
    .expect("static CSS selector");
    let feeds: Vec<serde_json::Value> = document
        .select(&feed_sel)
        .filter_map(|el| {
            let href = el.value().attr("href")?;
            let title = el.value().attr("title").unwrap_or_default();
            let feed_type = el.value().attr("type").unwrap_or_default();
            Some(serde_json::json!({
                "url": href,
                "title": title,
                "type": feed_type,
            }))
        })
        .collect();
    if !feeds.is_empty() {
        meta.insert("feeds".into(), feeds.into());
    }

    // Hreflang alternate language links (link[rel=alternate][hreflang])
    // HREFLANG-002: resolve relative hrefs against the fetched page URL
    let hreflang_sel =
        scraper::Selector::parse("link[rel=alternate][hreflang]").expect("static CSS selector");
    let parsed_base = base_url.and_then(|u| url::Url::parse(u).ok());
    let hreflang: Vec<serde_json::Value> = document
        .select(&hreflang_sel)
        .filter_map(|el| {
            let lang = el.value().attr("hreflang")?;
            let href = el.value().attr("href")?;
            let resolved = if let Some(ref base) = parsed_base {
                base.join(href)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| href.to_string())
            } else {
                href.to_string()
            };
            Some(serde_json::json!({
                "lang": lang,
                "href": resolved,
            }))
        })
        .collect();
    if !hreflang.is_empty() {
        meta.insert("hreflang".into(), hreflang.into());
    }

    serde_json::to_string(&meta).map_err(|e| NikaError::ExtractError {
        reason: format!("JSON serialize: {e}"),
    })
}

#[cfg(feature = "fetch-html")]
fn extract_links_json(html: &str, base_url: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let a_sel = scraper::Selector::parse("a[href]").expect("static CSS selector");
    let parsed_base = base_url.and_then(|u| url::Url::parse(u).ok());
    let links: Vec<serde_json::Value> = document
        .select(&a_sel)
        .map(|el| {
            let href = el.value().attr("href").unwrap_or_default();
            // Resolve relative URLs against the fetched page URL
            let resolved = if let Some(ref base) = parsed_base {
                base.join(href)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| href.to_string())
            } else {
                href.to_string()
            };
            let anchor = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            let rel = el.value().attr("rel").unwrap_or_default();
            serde_json::json!({
                "url": resolved,
                "anchor": anchor,
                "rel": rel,
            })
        })
        .collect();
    let count = links.len();
    serde_json::to_string(&serde_json::json!({
        "links": links,
        "count": count,
    }))
    .map_err(|e| NikaError::ExtractError {
        reason: format!("JSON serialize: {e}"),
    })
}

fn extract_jsonpath(body: &str, path: &str) -> Result<String, NikaError> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| NikaError::ExtractError {
            reason: format!("Response is not valid JSON: {e}"),
        })?;
    let jsonpath = serde_json_path::JsonPath::parse(path).map_err(|e| NikaError::ExtractError {
        reason: format!("Invalid JSONPath '{}': {e}", path),
    })?;
    let results: Vec<&serde_json::Value> = jsonpath.query(&json).all();
    match results.len() {
        0 => Ok("null".to_string()),
        1 => serde_json::to_string(results[0]).map_err(|e| NikaError::ExtractError {
            reason: e.to_string(),
        }),
        _ => serde_json::to_string(&results).map_err(|e| NikaError::ExtractError {
            reason: e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_extract_returns_body_unchanged() {
        let body = "<html><body>Hello</body></html>";
        let result = apply_extract(body, None, None).unwrap();
        assert_eq!(result, body);
    }

    #[test]
    fn none_extract_returns_body() {
        // With ExtractMode enum, invalid modes are prevented at compile time.
        // None returns body as-is.
        let result = apply_extract("<html></html>", None, None).unwrap();
        assert_eq!(result, "<html></html>");
    }

    #[test]
    fn jsonpath_extracts_single_value() {
        let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#;
        let result =
            apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.users[0].name")).unwrap();
        assert_eq!(result, "\"Alice\"");
    }

    #[test]
    fn jsonpath_extracts_multiple_values() {
        let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#;
        let result =
            apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.users[*].name")).unwrap();
        assert_eq!(result, "[\"Alice\",\"Bob\"]");
    }

    #[test]
    fn jsonpath_no_match_returns_null() {
        let json = r#"{"users": []}"#;
        let result =
            apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.users[0].name")).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn jsonpath_requires_selector() {
        let result = apply_extract(r#"{"a": 1}"#, Some(ExtractMode::Jsonpath), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("jsonpath requires 'selector'"));
    }

    #[test]
    fn jsonpath_invalid_json_body() {
        let result = apply_extract("not json", Some(ExtractMode::Jsonpath), Some("$.a"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not valid JSON"));
    }

    #[test]
    fn jsonpath_invalid_expression() {
        let result = apply_extract(
            r#"{"a": 1}"#,
            Some(ExtractMode::Jsonpath),
            Some("$[invalid"),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid JSONPath"));
    }

    #[cfg(feature = "fetch-markdown")]
    #[test]
    fn markdown_extract_converts_html() {
        let html = "<h1>Title</h1><p>Hello <strong>world</strong></p>";
        let result = apply_extract(html, Some(ExtractMode::Markdown), None).unwrap();
        assert!(result.contains("# Title"));
        assert!(result.contains("**world**"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn text_extract_without_selector() {
        let html = "<html><body><h1>Title</h1><p>Hello world</p></body></html>";
        let result = apply_extract(html, Some(ExtractMode::Text), None).unwrap();
        assert!(result.contains("Title"));
        assert!(result.contains("Hello world"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn text_extract_with_selector() {
        let html = r#"<html><body><p class="intro">First</p><p class="intro">Second</p><p>Third</p></body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Text), Some("p.intro")).unwrap();
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
        assert!(!result.contains("Third"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn selector_extract_returns_html() {
        let html =
            r#"<html><body><div class="content"><p>Hello</p></div><div>Other</div></body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Selector), Some("div.content")).unwrap();
        assert!(result.contains("<p>Hello</p>"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn selector_extract_requires_selector_field() {
        let result = apply_extract("<html></html>", Some(ExtractMode::Selector), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires 'selector' field"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn metadata_extracts_title_and_og() {
        let html = r#"<html><head>
            <title>My Page</title>
            <meta name="description" content="Page description">
            <meta property="og:title" content="OG Title">
            <meta property="og:image" content="https://example.com/img.png">
            <meta name="twitter:card" content="summary">
            <link rel="canonical" href="https://example.com/page">
        </head><body></body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Metadata), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "My Page");
        assert_eq!(parsed["description"], "Page description");
        assert_eq!(parsed["og"]["title"], "OG Title");
        assert_eq!(parsed["og"]["image"], "https://example.com/img.png");
        assert_eq!(parsed["twitter"]["card"], "summary");
        assert_eq!(parsed["canonical"], "https://example.com/page");
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn metadata_extracts_favicon_author_robots_feeds() {
        let html = r#"<html><head>
            <title>My Page</title>
            <link rel="icon" href="/favicon.ico">
            <meta name="author" content="Alice">
            <meta name="robots" content="index, follow">
            <link rel="alternate" type="application/rss+xml" title="RSS" href="/feed.xml">
            <link rel="alternate" type="application/atom+xml" title="Atom" href="/atom.xml">
        </head><body></body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Metadata), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "My Page");
        assert_eq!(parsed["favicon"], "/favicon.ico");
        assert_eq!(parsed["author"], "Alice");
        assert_eq!(parsed["robots"], "index, follow");
        let feeds = parsed["feeds"].as_array().unwrap();
        assert_eq!(feeds.len(), 2);
        assert_eq!(feeds[0]["url"], "/feed.xml");
        assert_eq!(feeds[0]["title"], "RSS");
        assert_eq!(feeds[1]["url"], "/atom.xml");
        assert_eq!(feeds[1]["type"], "application/atom+xml");
    }

    #[cfg(feature = "fetch-article")]
    #[test]
    fn article_extract_returns_structured_json() {
        let html = r#"<html><head><title>Test Article</title></head>
        <body>
            <article>
                <h1>Test Article</h1>
                <p>This is the main body of the article. It needs to be long enough
                for the readability algorithm to consider it as content. The algorithm
                typically requires a minimum amount of text content to identify an
                article region. So we add several sentences here to make sure the
                extraction works properly. This is important for testing purposes.</p>
                <p>Second paragraph with more content to help the readability score.
                The more text we have here, the better the extraction will work.</p>
            </article>
        </body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Article), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("title").is_some());
        assert!(parsed.get("content").is_some());
        assert!(parsed.get("text_content").is_some());
    }

    #[cfg(feature = "fetch-feed")]
    #[test]
    fn feed_extract_parses_rss() {
        let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
            <channel>
                <title>Test Feed</title>
                <item>
                    <title>First Post</title>
                    <link>https://example.com/post1</link>
                    <description>Summary of first post</description>
                    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
                </item>
                <item>
                    <title>Second Post</title>
                    <link>https://example.com/post2</link>
                </item>
            </channel>
        </rss>"#;
        let result = apply_extract(rss, Some(ExtractMode::Feed), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Test Feed");
        assert_eq!(parsed["entry_count"], 2);
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["title"], "First Post");
        assert_eq!(entries[0]["url"], "https://example.com/post1");
    }

    #[cfg(feature = "fetch-feed")]
    #[test]
    fn feed_extract_parses_atom() {
        let atom = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
            <title>Atom Feed</title>
            <entry>
                <title>Atom Entry</title>
                <link href="https://example.com/entry1"/>
                <summary>Atom summary</summary>
            </entry>
        </feed>"#;
        let result = apply_extract(atom, Some(ExtractMode::Feed), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Atom Feed");
        assert_eq!(parsed["entry_count"], 1);
    }

    #[cfg(feature = "fetch-feed")]
    #[test]
    fn feed_extract_invalid_input_returns_error() {
        let result = apply_extract("not xml at all", Some(ExtractMode::Feed), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Feed parse failed"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn links_extracts_anchors() {
        let html = r#"<html><body>
            <a href="https://example.com">Example</a>
            <a href="/about" rel="nofollow">About</a>
        </body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Links), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 2);
        let links = parsed["links"].as_array().unwrap();
        assert_eq!(links[0]["url"], "https://example.com");
        assert_eq!(links[0]["anchor"], "Example");
        assert_eq!(links[1]["url"], "/about");
        assert_eq!(links[1]["rel"], "nofollow");
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn links_resolves_relative_urls_with_base() {
        let html = r##"<html><body>
            <a href="/docs/">Docs</a>
            <a href="../other">Other</a>
            <a href="./sub">Sub</a>
            <a href="https://external.com">External</a>
            <a href="#fragment">Fragment</a>
        </body></html>"##;
        let result = apply_extract_with_base(
            html,
            Some(ExtractMode::Links),
            None,
            Some("https://example.com/blog/post"),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let links = parsed["links"].as_array().unwrap();
        assert_eq!(links[0]["url"], "https://example.com/docs/");
        assert_eq!(links[1]["url"], "https://example.com/other");
        assert_eq!(links[2]["url"], "https://example.com/blog/sub");
        // Absolute URLs stay unchanged (url::Url normalizes trailing slash on bare domains)
        assert_eq!(links[3]["url"], "https://external.com/");
        // Fragment-only resolves against base
        assert_eq!(links[4]["url"], "https://example.com/blog/post#fragment");
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn links_no_base_keeps_raw_href() {
        let html = r#"<html><body><a href="/path">Link</a></body></html>"#;
        let result =
            apply_extract_with_base(html, Some(ExtractMode::Links), None, None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let links = parsed["links"].as_array().unwrap();
        assert_eq!(links[0]["url"], "/path");
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn metadata_hreflang_resolves_relative_urls() {
        let html = r#"<html><head>
            <title>Multilang</title>
            <link rel="alternate" hreflang="en" href="/en/page">
            <link rel="alternate" hreflang="fr" href="../fr/page">
            <link rel="alternate" hreflang="de" href="https://example.com/de/page">
        </head><body></body></html>"#;
        let result = apply_extract_with_base(
            html,
            Some(ExtractMode::Metadata),
            None,
            Some("https://example.com/blog/article"),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let hreflang = parsed["hreflang"].as_array().unwrap();
        assert_eq!(hreflang.len(), 3);
        assert_eq!(hreflang[0]["href"], "https://example.com/en/page");
        assert_eq!(hreflang[1]["href"], "https://example.com/fr/page");
        // Already absolute stays unchanged
        assert_eq!(hreflang[2]["href"], "https://example.com/de/page");
    }

    #[test]
    fn parse_link_header_hreflang_basic() {
        let headers = vec![
            r#"<https://example.com/en/>; rel="alternate"; hreflang="en""#.to_string(),
            r#"<https://example.com/fr/>; rel="alternate"; hreflang="fr""#.to_string(),
        ];
        let result = parse_link_header_hreflang(&headers);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["lang"], "en");
        assert_eq!(result[0]["href"], "https://example.com/en/");
        assert_eq!(result[1]["lang"], "fr");
        assert_eq!(result[1]["href"], "https://example.com/fr/");
    }

    #[test]
    fn parse_link_header_hreflang_ignores_non_alternate() {
        let headers = vec![
            r#"<https://example.com/style.css>; rel="stylesheet""#.to_string(),
            r#"<https://example.com/fr/>; rel="alternate"; hreflang="fr""#.to_string(),
        ];
        let result = parse_link_header_hreflang(&headers);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["lang"], "fr");
    }

    #[test]
    fn parse_link_header_hreflang_comma_separated() {
        // Multiple entries in a single header value
        let headers = vec![
            r#"<https://example.com/en/>; rel="alternate"; hreflang="en", <https://example.com/de/>; rel="alternate"; hreflang="de""#.to_string(),
        ];
        let result = parse_link_header_hreflang(&headers);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["lang"], "en");
        assert_eq!(result[1]["lang"], "de");
    }

    #[test]
    fn parse_link_header_hreflang_empty() {
        let result = parse_link_header_hreflang(&[]);
        assert!(result.is_empty());
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn metadata_hreflang_no_base_keeps_raw() {
        let html = r#"<html><head>
            <title>No Base</title>
            <link rel="alternate" hreflang="fr" href="/fr/page">
        </head><body></body></html>"#;
        let result = apply_extract_with_base(
            html,
            Some(ExtractMode::Metadata),
            None,
            None,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let hreflang = parsed["hreflang"].as_array().unwrap();
        assert_eq!(hreflang[0]["href"], "/fr/page");
    }
}
