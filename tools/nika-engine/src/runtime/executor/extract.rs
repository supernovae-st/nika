//! Post-processing extraction for the fetch: verb.

use crate::error::NikaError;
use crate::error_domains::ExecutionError;

/// Apply extraction to a fetch response body.
/// Returns processed text or original body if no extraction configured.
pub(crate) fn apply_extract(
    body: &str,
    extract: Option<&str>,
    selector: Option<&str>,
) -> Result<String, NikaError> {
    match extract {
        None => Ok(body.to_string()),

        #[cfg(feature = "fetch-markdown")]
        Some("markdown") => {
            htmd::convert(body).map_err(|e| NikaError::ExtractError { reason: format!("HTML to markdown: {e}") })
        }

        #[cfg(feature = "fetch-html")]
        Some("text") => extract_text(body, selector),

        #[cfg(feature = "fetch-html")]
        Some("selector") => {
            let css = selector.ok_or_else(|| {
                NikaError::ExtractError {
                    reason: "extract: selector requires 'selector' field".to_string(),
                }
            })?;
            extract_html_by_selector(body, css)
        }

        #[cfg(feature = "fetch-html")]
        Some("metadata") => extract_metadata_json(body),

        #[cfg(feature = "fetch-html")]
        Some("links") => extract_links_json(body, None),

        #[cfg(feature = "fetch-article")]
        Some("article") => {
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
        Some("feed") => {
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
                "entry_count": feed.entries.len(),
                "entries": entries,
            })
            .to_string())
        }

        Some("jsonpath") => {
            let path = selector.ok_or_else(|| {
                NikaError::ExtractError {
                    reason: "extract: jsonpath requires 'selector' field with JSONPath expression"
                        .to_string(),
                }
            })?;
            extract_jsonpath(body, path)
        }

        #[cfg(not(feature = "fetch-markdown"))]
        Some("markdown") => Err(ExecutionError::ExtractFailed {
            reason: "extract: markdown requires feature 'fetch-markdown'. Build with: cargo build --features fetch-markdown".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-html"))]
        Some("text" | "selector" | "metadata" | "links") => Err(ExecutionError::ExtractFailed {
            reason: "extract: text/selector/metadata/links requires feature 'fetch-html'. Build with: cargo build --features fetch-html".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-article"))]
        Some("article") => Err(ExecutionError::ExtractFailed {
            reason: "extract: article requires feature 'fetch-article'. Build with: cargo build --features fetch-article".to_string(),
        }
        .into()),
        #[cfg(not(feature = "fetch-feed"))]
        Some("feed") => Err(ExecutionError::ExtractFailed {
            reason: "extract: feed requires feature 'fetch-feed'. Build with: cargo build --features fetch-feed".to_string(),
        }
        .into()),

        Some(unknown) => Err(ExecutionError::ExtractFailed {
            reason: format!(
                "Unknown extract mode '{}'. Available: markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt",
                unknown
            ),
        }
        .into()),
    }
}

#[cfg(feature = "fetch-html")]
fn extract_text(html: &str, selector: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
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
fn extract_metadata_json(html: &str) -> Result<String, NikaError> {
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

    serde_json::to_string(&meta).map_err(|e| NikaError::ExtractError {
        reason: format!("JSON serialize: {e}"),
    })
}

#[cfg(feature = "fetch-html")]
fn extract_links_json(html: &str, _base_url: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let a_sel = scraper::Selector::parse("a[href]").expect("static CSS selector");
    let links: Vec<serde_json::Value> = document
        .select(&a_sel)
        .map(|el| {
            let href = el.value().attr("href").unwrap_or_default();
            let anchor = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            let rel = el.value().attr("rel").unwrap_or_default();
            serde_json::json!({
                "url": href,
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
    fn unknown_extract_mode_returns_error() {
        let result = apply_extract("<html></html>", Some("invalid_mode"), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown extract mode"));
        assert!(err.contains("invalid_mode"));
    }

    #[test]
    fn jsonpath_extracts_single_value() {
        let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#;
        let result = apply_extract(json, Some("jsonpath"), Some("$.users[0].name")).unwrap();
        assert_eq!(result, "\"Alice\"");
    }

    #[test]
    fn jsonpath_extracts_multiple_values() {
        let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#;
        let result = apply_extract(json, Some("jsonpath"), Some("$.users[*].name")).unwrap();
        assert_eq!(result, "[\"Alice\",\"Bob\"]");
    }

    #[test]
    fn jsonpath_no_match_returns_null() {
        let json = r#"{"users": []}"#;
        let result = apply_extract(json, Some("jsonpath"), Some("$.users[0].name")).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn jsonpath_requires_selector() {
        let result = apply_extract(r#"{"a": 1}"#, Some("jsonpath"), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("jsonpath requires 'selector'"));
    }

    #[test]
    fn jsonpath_invalid_json_body() {
        let result = apply_extract("not json", Some("jsonpath"), Some("$.a"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not valid JSON"));
    }

    #[test]
    fn jsonpath_invalid_expression() {
        let result = apply_extract(r#"{"a": 1}"#, Some("jsonpath"), Some("$[invalid"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid JSONPath"));
    }

    #[cfg(feature = "fetch-markdown")]
    #[test]
    fn markdown_extract_converts_html() {
        let html = "<h1>Title</h1><p>Hello <strong>world</strong></p>";
        let result = apply_extract(html, Some("markdown"), None).unwrap();
        assert!(result.contains("# Title"));
        assert!(result.contains("**world**"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn text_extract_without_selector() {
        let html = "<html><body><h1>Title</h1><p>Hello world</p></body></html>";
        let result = apply_extract(html, Some("text"), None).unwrap();
        assert!(result.contains("Title"));
        assert!(result.contains("Hello world"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn text_extract_with_selector() {
        let html = r#"<html><body><p class="intro">First</p><p class="intro">Second</p><p>Third</p></body></html>"#;
        let result = apply_extract(html, Some("text"), Some("p.intro")).unwrap();
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
        assert!(!result.contains("Third"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn selector_extract_returns_html() {
        let html =
            r#"<html><body><div class="content"><p>Hello</p></div><div>Other</div></body></html>"#;
        let result = apply_extract(html, Some("selector"), Some("div.content")).unwrap();
        assert!(result.contains("<p>Hello</p>"));
    }

    #[cfg(feature = "fetch-html")]
    #[test]
    fn selector_extract_requires_selector_field() {
        let result = apply_extract("<html></html>", Some("selector"), None);
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
        let result = apply_extract(html, Some("metadata"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "My Page");
        assert_eq!(parsed["description"], "Page description");
        assert_eq!(parsed["og"]["title"], "OG Title");
        assert_eq!(parsed["og"]["image"], "https://example.com/img.png");
        assert_eq!(parsed["twitter"]["card"], "summary");
        assert_eq!(parsed["canonical"], "https://example.com/page");
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
        let result = apply_extract(html, Some("article"), None).unwrap();
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
        let result = apply_extract(rss, Some("feed"), None).unwrap();
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
        let result = apply_extract(atom, Some("feed"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Atom Feed");
        assert_eq!(parsed["entry_count"], 1);
    }

    #[cfg(feature = "fetch-feed")]
    #[test]
    fn feed_extract_invalid_input_returns_error() {
        let result = apply_extract("not xml at all", Some("feed"), None);
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
        let result = apply_extract(html, Some("links"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 2);
        let links = parsed["links"].as_array().unwrap();
        assert_eq!(links[0]["url"], "https://example.com");
        assert_eq!(links[0]["anchor"], "Example");
        assert_eq!(links[1]["url"], "/about");
        assert_eq!(links[1]["rel"], "nofollow");
    }
}
