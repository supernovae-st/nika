// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end tests for all 9 fetch extract modes.
//!
//! For each mode we verify:
//! 1. YAML parse -> FetchParams has the correct extract/selector values
//! 2. apply_extract() produces the correct output given realistic test data
//! 3. Edge cases: no extract (backward compat), invalid mode, selector without extract

use crate::ast::extract::ExtractMode;
use crate::ast::parse_workflow;
use crate::ast::{FetchParams, TaskAction};
use crate::runtime::executor::extract::apply_extract;

// ═══════════════════════════════════════════════════════════════════════════
// Helper: parse a minimal workflow YAML and extract the FetchParams
// ═══════════════════════════════════════════════════════════════════════════

/// Parse a workflow YAML string and return the FetchParams from the first task.
fn parse_fetch_params(yaml: &str) -> FetchParams {
    let workflow = parse_workflow(yaml).expect("workflow should parse");
    assert!(!workflow.tasks.is_empty(), "workflow should have tasks");
    match &workflow.tasks[0].action {
        TaskAction::Fetch { fetch } => fetch.clone(),
        other => panic!("expected Fetch action, got {:?}", other.verb_name()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared HTML test fixtures
// ═══════════════════════════════════════════════════════════════════════════

const RICH_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <title>Test Page</title>
  <meta name="description" content="A test page for extract modes">
  <meta property="og:title" content="OG Test Page">
  <meta property="og:description" content="OG description here">
  <meta property="og:image" content="https://example.com/og.png">
  <meta property="og:url" content="https://example.com/page">
  <meta property="og:type" content="article">
  <meta name="twitter:card" content="summary_large_image">
  <meta name="twitter:title" content="Twitter Title">
  <meta name="twitter:image" content="https://example.com/tw.png">
  <link rel="canonical" href="https://example.com/canonical">
  <script type="application/ld+json">
  {"@type": "Article", "name": "JSON-LD Article"}
  </script>
</head>
<body>
  <nav><a href="/">Home</a><a href="/about">About</a></nav>
  <h1>Main Heading</h1>
  <p class="intro">This is the <strong>introduction</strong> paragraph.</p>
  <p class="intro">Second intro paragraph.</p>
  <p>A regular paragraph with a <a href="https://rust-lang.org" rel="noopener">Rust link</a>.</p>
  <div class="sidebar">Sidebar content</div>
  <a href="/contact">Contact Us</a>
  <a href="https://external.com" rel="nofollow">External</a>
</body>
</html>"#;

const BLOG_HTML: &str = r#"<html><head><title>Blog Post</title>
<meta name="description" content="A great blog post">
<meta property="og:title" content="OG Blog Title">
<meta property="og:image" content="https://example.com/og.jpg">
<meta name="twitter:card" content="summary_large_image">
<link rel="canonical" href="https://example.com/blog/post">
<script type="application/ld+json">{"@type":"Article","headline":"JSON-LD Title"}</script>
</head><body>
<nav><a href="/">Home</a><a href="/about">About</a></nav>
<article><h1>Blog Post Title</h1><p>This is the <strong>main content</strong> with a <a href="/related">related link</a>.</p>
<ul><li>Item 1</li><li>Item 2</li></ul></article>
<footer><a href="https://twitter.com/example" rel="nofollow">Twitter</a></footer>
</body></html>"#;

const JSON_API: &str =
    r#"{"data":{"items":[{"name":"Alpha","score":95},{"name":"Beta","score":42}],"total":2}}"#;

const RSS_FEED: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel>
<title>Test Feed</title><item><title>Entry 1</title><link>https://example.com/1</link>
<pubDate>Mon, 20 Mar 2026 00:00:00 GMT</pubDate></item></channel></rss>"#;

// ═══════════════════════════════════════════════════════════════════════════
// 1. markdown: HTML -> Markdown
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-markdown")]
mod markdown {
    use super::*;

    #[test]
    fn parse_sets_extract_markdown() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Markdown));
        assert!(params.selector.is_none());
    }

    #[test]
    fn apply_converts_headings() {
        let html = "<h1>Title</h1><h2>Subtitle</h2><p>Body text.</p>";
        let result = apply_extract(html, Some(ExtractMode::Markdown), None).unwrap();
        assert!(result.contains("# Title"), "should convert h1: {}", result);
        assert!(
            result.contains("## Subtitle"),
            "should convert h2: {}",
            result
        );
        assert!(
            result.contains("Body text."),
            "should keep text: {}",
            result
        );
    }

    #[test]
    fn apply_converts_bold_and_links() {
        let html =
            r#"<p>Click <strong>here</strong> for <a href="https://example.com">info</a>.</p>"#;
        let result = apply_extract(html, Some(ExtractMode::Markdown), None).unwrap();
        assert!(
            result.contains("**here**"),
            "should convert bold: {}",
            result
        );
        assert!(
            result.contains("[info](https://example.com)"),
            "should convert links: {}",
            result
        );
    }

    #[test]
    fn e2e_rich_html_to_markdown() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Markdown), None).unwrap();
        assert!(
            result.contains("# Main Heading"),
            "heading missing: {}",
            result
        );
        assert!(
            result.contains("**introduction**"),
            "bold missing: {}",
            result
        );
        assert!(
            result.contains("[Rust link](https://rust-lang.org)"),
            "link missing: {}",
            result
        );
    }

    #[test]
    fn apply_preserves_list_items() {
        let result = apply_extract(BLOG_HTML, Some(ExtractMode::Markdown), None).unwrap();
        assert!(
            result.contains("Item 1"),
            "Expected 'Item 1' in markdown output"
        );
        assert!(
            result.contains("Item 2"),
            "Expected 'Item 2' in markdown output"
        );
    }

    #[test]
    fn apply_empty_html_returns_empty() {
        let result = apply_extract("", Some(ExtractMode::Markdown), None).unwrap();
        assert!(
            result.trim().is_empty(),
            "Expected empty markdown for empty input"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. article: HTML -> article content (readability)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-article")]
mod article {
    use super::*;

    #[test]
    fn parse_sets_extract_article() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: read
    fetch:
      url: "https://example.com/blog/post"
      extract: article
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Article));
    }

    #[test]
    fn apply_strips_nav_keeps_content() {
        // Readability needs enough content for its algorithm.
        let html = r#"<!DOCTYPE html>
<html><head><title>Blog Post Title</title></head>
<body>
  <nav><a href="/">Home</a><a href="/tags">Tags</a></nav>
  <article>
    <h1>Blog Post Title</h1>
    <p>This is the first paragraph of the blog post with enough content for
    the readability algorithm to detect it as the main article body. We need
    multiple sentences to pass the content threshold that readability uses.</p>
    <p>The second paragraph continues the article body. More text is required
    to ensure extraction works properly. Let us add extra sentences that make
    the overall content sufficient for detection. This paragraph also contains
    useful information about the topic being discussed in this test article.</p>
    <p>A third paragraph seals the deal. The readability score should be high
    enough by now. If not, we can always add more content to the article body
    to make the extraction algorithm confident in its selection.</p>
  </article>
  <footer>Copyright 2024</footer>
</body></html>"#;
        let result = apply_extract(html, Some(ExtractMode::Article), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Article should have structured fields
        assert!(parsed.get("title").is_some(), "should have title");
        assert!(parsed.get("content").is_some(), "should have content");
        assert!(
            parsed.get("text_content").is_some(),
            "should have text_content"
        );

        // Content should contain article body, not nav
        let text_content = parsed["text_content"].as_str().unwrap_or_default();
        assert!(
            text_content.contains("first paragraph"),
            "article body missing: {}",
            text_content
        );
        // Nav content should be stripped
        assert!(
            !text_content.contains("Copyright 2024"),
            "footer should be stripped: {}",
            text_content
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. text: HTML -> plain text (no tags)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-html")]
mod text_plain {
    use super::*;

    #[test]
    fn parse_sets_extract_text() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: text
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Text));
        assert!(params.selector.is_none());
    }

    #[test]
    fn apply_strips_all_tags() {
        let html = "<html><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>";
        let result = apply_extract(html, Some(ExtractMode::Text), None).unwrap();
        assert!(
            !result.contains('<'),
            "should have no HTML tags: {}",
            result
        );
        assert!(
            !result.contains('>'),
            "should have no HTML tags: {}",
            result
        );
        assert!(result.contains("Title"), "text content missing: {}", result);
        assert!(result.contains("world"), "text content missing: {}", result);
    }

    #[test]
    fn e2e_rich_html_to_text() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Text), None).unwrap();
        assert!(
            !result.contains("<h1>"),
            "should strip tags: {}",
            &result[..200]
        );
        assert!(
            result.contains("Main Heading"),
            "text missing: {}",
            &result[..200]
        );
        assert!(
            result.contains("introduction"),
            "text missing: {}",
            &result[..200]
        );
    }

    #[test]
    fn apply_empty_html_returns_empty() {
        let result = apply_extract("", Some(ExtractMode::Text), None).unwrap();
        assert!(
            result.trim().is_empty(),
            "Expected empty text for empty input, got: '{result}'"
        );
    }

    #[test]
    fn apply_invalid_selector_returns_error() {
        let result = apply_extract(BLOG_HTML, Some(ExtractMode::Text), Some("[[[invalid"));
        assert!(result.is_err(), "Invalid selector should produce an error");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. text + selector: HTML + CSS -> filtered text
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-html")]
mod text_with_selector {
    use super::*;

    #[test]
    fn parse_sets_extract_text_with_selector() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: text
      selector: "p.intro"
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Text));
        assert_eq!(params.selector.as_deref(), Some("p.intro"));
    }

    #[test]
    fn apply_filters_by_css_selector() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Text), Some("p.intro")).unwrap();
        assert!(
            result.contains("introduction"),
            "intro text missing: {}",
            result
        );
        assert!(
            result.contains("Second intro"),
            "second intro missing: {}",
            result
        );
        // Non-matching elements should be excluded
        assert!(
            !result.contains("Sidebar content"),
            "sidebar should be excluded: {}",
            result
        );
        assert!(
            !result.contains("Main Heading"),
            "heading should be excluded: {}",
            result
        );
    }

    #[test]
    fn apply_empty_result_for_no_match() {
        let html = "<html><body><p>Only plain p</p></body></html>";
        let result = apply_extract(html, Some(ExtractMode::Text), Some("p.nonexistent")).unwrap();
        assert!(result.is_empty(), "no matches -> empty: {}", result);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. selector: HTML + CSS -> matching HTML elements
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-html")]
mod selector_html {
    use super::*;

    #[test]
    fn parse_sets_extract_selector() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: selector
      selector: "div.sidebar"
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Selector));
        assert_eq!(params.selector.as_deref(), Some("div.sidebar"));
    }

    #[test]
    fn apply_returns_matching_html_elements() {
        let result =
            apply_extract(RICH_HTML, Some(ExtractMode::Selector), Some("div.sidebar")).unwrap();
        assert!(
            result.contains("<div class=\"sidebar\">"),
            "should return raw HTML: {}",
            result
        );
        assert!(
            result.contains("Sidebar content"),
            "content should be preserved: {}",
            result
        );
    }

    #[test]
    fn apply_returns_multiple_matches() {
        let result =
            apply_extract(RICH_HTML, Some(ExtractMode::Selector), Some("p.intro")).unwrap();
        // Two p.intro elements in RICH_HTML
        let count = result.matches("<p class=\"intro\">").count();
        assert_eq!(count, 2, "should match both p.intro elements: {}", result);
    }

    #[test]
    fn apply_requires_selector_field() {
        let result = apply_extract("<html></html>", Some(ExtractMode::Selector), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("requires 'selector' field"),
            "error should mention selector: {}",
            err
        );
    }

    #[test]
    fn apply_returns_outer_html() {
        let result =
            apply_extract(BLOG_HTML, Some(ExtractMode::Selector), Some("article h1")).unwrap();
        assert!(
            result.contains("<h1>"),
            "Selector should include the matched element's outer HTML"
        );
        assert!(result.contains("Blog Post Title"));
    }

    #[test]
    fn apply_no_match_returns_empty() {
        let result = apply_extract(
            BLOG_HTML,
            Some(ExtractMode::Selector),
            Some("div.nonexistent"),
        )
        .unwrap();
        assert!(result.is_empty(), "No match should return empty string");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. metadata: HTML -> JSON with OG, Twitter, JSON-LD
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-html")]
mod metadata {
    use super::*;

    #[test]
    fn parse_sets_extract_metadata() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: meta
    fetch:
      url: "https://example.com"
      extract: metadata
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Metadata));
    }

    #[test]
    fn apply_extracts_all_metadata() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Metadata), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Title
        assert_eq!(parsed["title"], "Test Page", "title: {}", result);

        // Description
        assert_eq!(
            parsed["description"], "A test page for extract modes",
            "description: {}",
            result
        );

        // OG tags
        assert_eq!(parsed["og"]["title"], "OG Test Page");
        assert_eq!(parsed["og"]["description"], "OG description here");
        assert_eq!(parsed["og"]["image"], "https://example.com/og.png");
        assert_eq!(parsed["og"]["url"], "https://example.com/page");
        assert_eq!(parsed["og"]["type"], "article");

        // Twitter cards
        assert_eq!(parsed["twitter"]["card"], "summary_large_image");
        assert_eq!(parsed["twitter"]["title"], "Twitter Title");
        assert_eq!(parsed["twitter"]["image"], "https://example.com/tw.png");

        // JSON-LD
        let json_ld = parsed["json_ld"]
            .as_array()
            .expect("json_ld should be array");
        assert_eq!(json_ld.len(), 1);
        assert_eq!(json_ld[0]["@type"], "Article");
        assert_eq!(json_ld[0]["name"], "JSON-LD Article");

        // Canonical
        assert_eq!(parsed["canonical"], "https://example.com/canonical");
    }

    #[test]
    fn apply_handles_minimal_html() {
        let html = "<html><head><title>Only Title</title></head><body></body></html>";
        let result = apply_extract(html, Some(ExtractMode::Metadata), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Only Title");
        // No OG, Twitter, JSON-LD -> those keys should not exist
        assert!(parsed.get("og").is_none(), "no og: {}", result);
        assert!(parsed.get("twitter").is_none(), "no twitter: {}", result);
        assert!(parsed.get("json_ld").is_none(), "no json_ld: {}", result);
    }

    #[test]
    fn apply_returns_valid_json() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Metadata), None).unwrap();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
        assert!(parsed.is_ok(), "metadata output must be valid JSON");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. links: HTML -> JSON with link classification
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-html")]
mod links {
    use super::*;

    #[test]
    fn parse_sets_extract_links() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: crawl
    fetch:
      url: "https://example.com"
      extract: links
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Links));
    }

    #[test]
    fn apply_extracts_all_links() {
        let result = apply_extract(RICH_HTML, Some(ExtractMode::Links), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // RICH_HTML has 5 anchor tags: /, /about, rust-lang.org, /contact, external.com
        let count = parsed["count"].as_u64().unwrap();
        assert_eq!(count, 5, "should find 5 links: {}", result);

        let links = parsed["links"].as_array().unwrap();

        // Check first link (internal)
        assert_eq!(links[0]["url"], "/");
        assert_eq!(links[0]["anchor"], "Home");

        // Check Rust link
        let rust_link = links
            .iter()
            .find(|l| l["url"] == "https://rust-lang.org")
            .expect("rust-lang.org link should exist");
        assert_eq!(rust_link["anchor"], "Rust link");
        assert_eq!(rust_link["rel"], "noopener");

        // Check external link with nofollow
        let external = links
            .iter()
            .find(|l| l["url"] == "https://external.com")
            .expect("external link should exist");
        assert_eq!(external["rel"], "nofollow");
    }

    #[test]
    fn apply_handles_no_links() {
        let html = "<html><body><p>No links here</p></body></html>";
        let result = apply_extract(html, Some(ExtractMode::Links), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 0);
        assert!(parsed["links"].as_array().unwrap().is_empty());
    }

    #[test]
    fn apply_extracts_blog_html_links() {
        let result = apply_extract(BLOG_HTML, Some(ExtractMode::Links), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // BLOG_HTML has: /, /about, /related, https://twitter.com/example
        assert_eq!(parsed["count"], 4);

        let links = parsed["links"].as_array().unwrap();
        let urls: Vec<&str> = links.iter().map(|l| l["url"].as_str().unwrap()).collect();
        assert!(urls.contains(&"/"), "Expected '/' in links");
        assert!(urls.contains(&"/about"), "Expected '/about' in links");
        assert!(urls.contains(&"/related"), "Expected '/related' in links");

        // Anchor text
        let home_link = links.iter().find(|l| l["url"] == "/").unwrap();
        assert_eq!(home_link["anchor"], "Home");

        let about_link = links.iter().find(|l| l["url"] == "/about").unwrap();
        assert_eq!(about_link["anchor"], "About");

        let related_link = links.iter().find(|l| l["url"] == "/related").unwrap();
        assert_eq!(related_link["anchor"], "related link");

        // Nofollow on twitter link
        let twitter_link = links
            .iter()
            .find(|l| l["url"].as_str().unwrap_or("").contains("twitter.com"))
            .expect("twitter link should exist");
        assert_eq!(twitter_link["rel"], "nofollow");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. jsonpath: JSON + path -> extracted value
// ═══════════════════════════════════════════════════════════════════════════

mod jsonpath {
    use super::*;

    #[test]
    fn parse_sets_extract_jsonpath_with_selector() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: api
    fetch:
      url: "https://api.example.com/users"
      extract: jsonpath
      selector: "$.data[*].name"
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Jsonpath));
        assert_eq!(params.selector.as_deref(), Some("$.data[*].name"));
    }

    #[test]
    fn apply_extracts_single_value() {
        let json = r#"{"data": {"name": "Alice", "age": 30}}"#;
        let result = apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.data.name")).unwrap();
        assert_eq!(result, "\"Alice\"");
    }

    #[test]
    fn apply_extracts_multiple_values() {
        let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}, {"name": "Charlie"}]}"#;
        let result =
            apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.users[*].name")).unwrap();
        assert_eq!(result, r#"["Alice","Bob","Charlie"]"#);
    }

    #[test]
    fn apply_extracts_nested_object() {
        let json = r#"{"response": {"metadata": {"version": 2, "lang": "en"}}}"#;
        let result = apply_extract(
            json,
            Some(ExtractMode::Jsonpath),
            Some("$.response.metadata"),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["version"], 2);
        assert_eq!(parsed["lang"], "en");
    }

    #[test]
    fn apply_returns_null_for_no_match() {
        let json = r#"{"data": []}"#;
        let result =
            apply_extract(json, Some(ExtractMode::Jsonpath), Some("$.data[0].name")).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn apply_requires_selector() {
        let result = apply_extract(r#"{"a": 1}"#, Some(ExtractMode::Jsonpath), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("jsonpath requires 'selector'"));
    }

    #[test]
    fn apply_rejects_invalid_json() {
        let result = apply_extract("not json", Some(ExtractMode::Jsonpath), Some("$.a"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not valid JSON"));
    }

    #[test]
    fn apply_rejects_invalid_jsonpath() {
        let result = apply_extract(
            r#"{"a": 1}"#,
            Some(ExtractMode::Jsonpath),
            Some("$[invalid"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSONPath"));
    }

    #[test]
    fn apply_extracts_simple_path() {
        let result =
            apply_extract(JSON_API, Some(ExtractMode::Jsonpath), Some("$.data.total")).unwrap();
        assert_eq!(result, "2");
    }

    #[test]
    fn apply_extracts_array_names() {
        let result = apply_extract(
            JSON_API,
            Some(ExtractMode::Jsonpath),
            Some("$.data.items[*].name"),
        )
        .unwrap();
        let parsed: Vec<String> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn apply_extracts_second_item_name() {
        let result = apply_extract(
            JSON_API,
            Some(ExtractMode::Jsonpath),
            Some("$.data.items[1].name"),
        )
        .unwrap();
        assert_eq!(result, "\"Beta\"");
    }

    #[test]
    fn apply_extracts_all_scores() {
        let result = apply_extract(
            JSON_API,
            Some(ExtractMode::Jsonpath),
            Some("$.data.items[*].score"),
        )
        .unwrap();
        let parsed: Vec<u64> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, vec![95, 42]);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. feed: RSS/Atom XML -> JSON with entries
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "fetch-feed")]
mod feed {
    use super::*;

    #[test]
    fn parse_sets_extract_feed() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: rss
    fetch:
      url: "https://blog.example.com/feed.xml"
      extract: feed
"#;
        let params = parse_fetch_params(yaml);
        assert_eq!(params.extract, Some(ExtractMode::Feed));
    }

    #[test]
    fn apply_parses_rss_feed() {
        let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Rust Blog</title>
    <description>News from the Rust project</description>
    <item>
      <title>Rust 1.80 Released</title>
      <link>https://blog.rust-lang.org/1.80</link>
      <description>New features in Rust 1.80</description>
      <pubDate>Thu, 25 Jul 2024 00:00:00 GMT</pubDate>
    </item>
    <item>
      <title>Async Foundations Update</title>
      <link>https://blog.rust-lang.org/async</link>
      <description>Progress on async Rust</description>
    </item>
    <item>
      <title>Cargo 2024 Roadmap</title>
      <link>https://blog.rust-lang.org/cargo</link>
    </item>
  </channel>
</rss>"#;
        let result = apply_extract(rss, Some(ExtractMode::Feed), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["title"], "Rust Blog");
        assert_eq!(parsed["entry_count"], 3);

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["title"], "Rust 1.80 Released");
        assert_eq!(entries[0]["url"], "https://blog.rust-lang.org/1.80");
        assert!(
            entries[0]["summary"]
                .as_str()
                .unwrap()
                .contains("New features"),
            "summary missing"
        );
        assert!(
            entries[0]["published"].is_string(),
            "published should exist"
        );

        assert_eq!(entries[1]["title"], "Async Foundations Update");
        assert_eq!(entries[2]["title"], "Cargo 2024 Roadmap");
    }

    #[test]
    fn apply_parses_atom_feed() {
        let atom = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Feed</title>
  <entry>
    <title>First Entry</title>
    <link href="https://example.com/1"/>
    <summary>Summary of first entry</summary>
    <published>2024-07-25T12:00:00Z</published>
  </entry>
  <entry>
    <title>Second Entry</title>
    <link href="https://example.com/2"/>
  </entry>
</feed>"#;
        let result = apply_extract(atom, Some(ExtractMode::Feed), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["title"], "Atom Feed");
        assert_eq!(parsed["entry_count"], 2);
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["title"], "First Entry");
        assert_eq!(entries[0]["url"], "https://example.com/1");
        assert_eq!(entries[1]["title"], "Second Entry");
    }

    #[test]
    fn apply_rejects_invalid_feed() {
        let result = apply_extract("this is not XML at all", Some(ExtractMode::Feed), None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Feed parse failed"));
    }

    #[test]
    fn apply_parses_minimal_rss_feed() {
        let result = apply_extract(RSS_FEED, Some(ExtractMode::Feed), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Test Feed");
        assert!(parsed["entries"].as_array().is_some());

        let entries = parsed["entries"].as_array().unwrap();
        assert!(!entries.is_empty(), "Feed should have at least one entry");
        assert_eq!(entries[0]["title"], "Entry 1");
        assert_eq!(entries[0]["url"], "https://example.com/1");
        assert_eq!(parsed["entry_count"], 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Backward compatibility: no extract -> returns raw body
// ═══════════════════════════════════════════════════════════════════════════

mod no_extract {
    use super::*;

    #[test]
    fn parse_no_extract_field() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: plain
    fetch:
      url: "https://api.example.com/data"
"#;
        let params = parse_fetch_params(yaml);
        assert!(params.extract.is_none());
        assert!(params.selector.is_none());
    }

    #[test]
    fn apply_returns_body_unchanged() {
        let body = r#"{"data": [1, 2, 3]}"#;
        let result = apply_extract(body, None, None).unwrap();
        assert_eq!(result, body, "raw body should pass through unchanged");
    }

    #[test]
    fn apply_returns_html_unchanged() {
        let html = "<html><body><h1>Hello</h1></body></html>";
        let result = apply_extract(html, None, None).unwrap();
        assert_eq!(result, html);
    }

    #[test]
    fn apply_preserves_json() {
        let result = apply_extract(JSON_API, None, None).unwrap();
        assert_eq!(result, JSON_API, "JSON should be preserved verbatim");
    }

    #[test]
    fn apply_preserves_xml() {
        let result = apply_extract(RSS_FEED, None, None).unwrap();
        assert_eq!(result, RSS_FEED, "XML should be preserved verbatim");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Invalid mode → caught at analysis (type system prevents runtime invalid modes)
// ═══════════════════════════════════════════════════════════════════════════

mod invalid_mode {
    use super::*;

    #[test]
    fn none_extract_returns_body_as_is() {
        let result = apply_extract("<html></html>", None, None).unwrap();
        assert_eq!(result, "<html></html>");
    }

    #[test]
    fn parse_invalid_extract_becomes_none() {
        // Invalid extract modes are caught at analysis and become None.
        // The type system (ExtractMode enum) prevents invalid modes at compile time.
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad
    fetch:
      url: "https://example.com"
      extract: invented_mode
"#;
        let workflow = parse_workflow(yaml).expect("parse succeeds with warning");
        match &workflow.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert!(
                    fetch.extract.is_none(),
                    "invalid extract mode should become None after analysis"
                );
            }
            _ => panic!("expected Fetch"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// selector without extract -> validation error
// ═══════════════════════════════════════════════════════════════════════════

mod selector_without_extract {
    use super::*;

    #[test]
    fn validate_rejects_selector_alone() {
        let params = FetchParams {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: rustc_hash::FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
            response: None,
            extract: None,
            selector: Some("div.content".to_string()),
            session: None,
            cache: None,
        };
        let err = params.validate().unwrap_err();
        assert!(err.to_string().contains("selector"), "error: {}", err);
        assert!(err.to_string().contains("requires"), "error: {}", err);
    }

    #[test]
    fn parse_passes_selector_without_extract_but_validate_catches_it() {
        // The analyzer does NOT validate selector-without-extract.
        // Validation happens at runtime via FetchParams::validate().
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad
    fetch:
      url: "https://example.com"
      selector: "div.content"
"#;
        let workflow = parse_workflow(yaml).expect("parse succeeds (validation is deferred)");
        match &workflow.tasks[0].action {
            TaskAction::Fetch { fetch } => {
                assert!(fetch.extract.is_none());
                assert_eq!(fetch.selector.as_deref(), Some("div.content"));
                // Runtime validation catches it
                let err = fetch.validate().unwrap_err();
                assert!(
                    err.to_string().contains("selector"),
                    "validate should reject selector without extract: {}",
                    err
                );
            }
            _ => panic!("expected Fetch"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Cross-cutting: parse all 9 modes in one workflow
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parse_all_nine_extract_modes() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: t1
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: t2
    fetch:
      url: "https://example.com"
      extract: article

  - id: t3
    fetch:
      url: "https://example.com"
      extract: text

  - id: t4
    fetch:
      url: "https://example.com"
      extract: text
      selector: "p.intro"

  - id: t5
    fetch:
      url: "https://example.com"
      extract: selector
      selector: "div.content"

  - id: t6
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: t7
    fetch:
      url: "https://example.com"
      extract: links

  - id: t8
    fetch:
      url: "https://api.example.com/data"
      extract: jsonpath
      selector: "$.data[*].name"

  - id: t9
    fetch:
      url: "https://blog.example.com/feed.xml"
      extract: feed
"#;
    let workflow = parse_workflow(yaml).expect("all 9 extract modes should parse");
    assert_eq!(workflow.tasks.len(), 9);

    let modes: Vec<Option<ExtractMode>> = workflow
        .tasks
        .iter()
        .map(|t| match &t.action {
            TaskAction::Fetch { fetch } => fetch.extract,
            _ => panic!("expected fetch"),
        })
        .collect();

    assert_eq!(
        modes,
        vec![
            Some(ExtractMode::Markdown),
            Some(ExtractMode::Article),
            Some(ExtractMode::Text),
            Some(ExtractMode::Text),
            Some(ExtractMode::Selector),
            Some(ExtractMode::Metadata),
            Some(ExtractMode::Links),
            Some(ExtractMode::Jsonpath),
            Some(ExtractMode::Feed),
        ]
    );

    // Verify selector is set for modes that need it
    let get_selector = |idx: usize| -> Option<&str> {
        match &workflow.tasks[idx].action {
            TaskAction::Fetch { fetch } => fetch.selector.as_deref(),
            _ => None,
        }
    };
    assert!(get_selector(0).is_none(), "markdown: no selector");
    assert!(get_selector(1).is_none(), "article: no selector");
    assert!(get_selector(2).is_none(), "text: no selector");
    assert_eq!(get_selector(3), Some("p.intro"), "text+selector");
    assert_eq!(get_selector(4), Some("div.content"), "selector mode");
    assert!(get_selector(5).is_none(), "metadata: no selector");
    assert!(get_selector(6).is_none(), "links: no selector");
    assert_eq!(get_selector(7), Some("$.data[*].name"), "jsonpath selector");
    assert!(get_selector(8).is_none(), "feed: no selector");
}
