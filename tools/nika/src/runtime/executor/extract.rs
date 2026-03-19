//! Post-processing extraction for the fetch: verb.

use crate::error::NikaError;

/// Apply extraction to a fetch response body.
/// Returns processed text or original body if no extraction configured.
pub fn apply_extract(
    body: &str,
    extract: Option<&str>,
    selector: Option<&str>,
) -> Result<String, NikaError> {
    match extract {
        None => Ok(body.to_string()),

        #[cfg(feature = "fetch-markdown")]
        Some("markdown") => {
            htmd::convert(body).map_err(|e| NikaError::Execution(format!("HTML to markdown: {e}")))
        }

        #[cfg(feature = "fetch-html")]
        Some("text") => extract_text(body, selector),

        #[cfg(feature = "fetch-html")]
        Some("selector") => {
            let css = selector.ok_or_else(|| {
                NikaError::Execution(
                    "extract: selector requires 'selector' field".to_string(),
                )
            })?;
            extract_html_by_selector(body, css)
        }

        #[cfg(feature = "fetch-html")]
        Some("metadata") => extract_metadata_json(body),

        #[cfg(feature = "fetch-html")]
        Some("links") => extract_links_json(body, None),

        Some("jsonpath") => {
            let path = selector.ok_or_else(|| {
                NikaError::Execution(
                    "extract: jsonpath requires 'selector' field with JSONPath expression"
                        .to_string(),
                )
            })?;
            extract_jsonpath(body, path)
        }

        Some(unknown) => Err(NikaError::Execution(format!(
            "Unknown extract mode '{}'. Available: markdown, article, text, selector, metadata, links, jsonpath",
            unknown
        ))),
    }
}

#[cfg(feature = "fetch-html")]
fn extract_text(html: &str, selector: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    if let Some(css) = selector {
        let sel = scraper::Selector::parse(css)
            .map_err(|_| NikaError::Execution(format!("Invalid CSS selector: {css}")))?;
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
    let sel = scraper::Selector::parse(css)
        .map_err(|_| NikaError::Execution(format!("Invalid CSS selector: {css}")))?;
    let parts: Vec<String> = document.select(&sel).map(|el| el.html()).collect();
    Ok(parts.join("\n"))
}

#[cfg(feature = "fetch-html")]
fn extract_metadata_json(html: &str) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let mut meta = serde_json::Map::new();

    // <title>
    let title_sel = scraper::Selector::parse("title").unwrap();
    if let Some(el) = document.select(&title_sel).next() {
        meta.insert(
            "title".into(),
            el.text().collect::<String>().trim().to_string().into(),
        );
    }

    // meta name="description"
    let meta_sel = scraper::Selector::parse("meta[name=description]").unwrap();
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
    let jsonld_sel = scraper::Selector::parse("script[type=\"application/ld+json\"]").unwrap();
    let json_ld: Vec<serde_json::Value> = document
        .select(&jsonld_sel)
        .filter_map(|el| serde_json::from_str(&el.text().collect::<String>()).ok())
        .collect();
    if !json_ld.is_empty() {
        meta.insert("json_ld".into(), json_ld.into());
    }

    // Canonical
    let canon_sel = scraper::Selector::parse("link[rel=canonical]").unwrap();
    if let Some(el) = document.select(&canon_sel).next() {
        if let Some(href) = el.value().attr("href") {
            meta.insert("canonical".into(), href.into());
        }
    }

    serde_json::to_string(&meta).map_err(|e| NikaError::Execution(format!("JSON serialize: {e}")))
}

#[cfg(feature = "fetch-html")]
fn extract_links_json(html: &str, _base_url: Option<&str>) -> Result<String, NikaError> {
    let document = scraper::Html::parse_document(html);
    let a_sel = scraper::Selector::parse("a[href]").unwrap();
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
    .map_err(|e| NikaError::Execution(format!("JSON serialize: {e}")))
}

fn extract_jsonpath(body: &str, path: &str) -> Result<String, NikaError> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| NikaError::Execution(format!("Response is not valid JSON: {e}")))?;
    let jsonpath = serde_json_path::JsonPath::parse(path)
        .map_err(|e| NikaError::Execution(format!("Invalid JSONPath '{}': {e}", path)))?;
    let results: Vec<&serde_json::Value> = jsonpath.query(&json).all();
    match results.len() {
        0 => Ok("null".to_string()),
        1 => serde_json::to_string(results[0]).map_err(|e| NikaError::Execution(e.to_string())),
        _ => serde_json::to_string(&results).map_err(|e| NikaError::Execution(e.to_string())),
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
