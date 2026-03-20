//! End-to-end extraction tests using realistic HTML fixtures.
//!
//! These tests exercise `apply_extract()` directly with embedded HTML/JSON/RSS
//! fixtures, covering every extraction mode: markdown, text, selector, metadata,
//! links, jsonpath, feed, and article.
//!
//! Run: `cargo test --lib --features fetch-extract -q -- tests_extraction_e2e`

use super::extract::apply_extract;

// ─── Fixtures ────────────────────────────────────────────────────────────────

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

// ─── Markdown extraction ─────────────────────────────────────────────────────

#[cfg(feature = "fetch-markdown")]
mod markdown {
    use super::*;

    #[test]
    fn extract_markdown_produces_headings() {
        let result = apply_extract(BLOG_HTML, Some("markdown"), None).unwrap();
        assert!(
            result.contains("# Blog Post Title"),
            "Expected '# Blog Post Title' in:\n{result}"
        );
    }

    #[test]
    fn extract_markdown_preserves_bold() {
        let result = apply_extract(BLOG_HTML, Some("markdown"), None).unwrap();
        assert!(
            result.contains("**main content**"),
            "Expected '**main content**' in:\n{result}"
        );
    }

    #[test]
    fn extract_markdown_produces_links() {
        let result = apply_extract(BLOG_HTML, Some("markdown"), None).unwrap();
        assert!(
            result.contains("[related link](/related)"),
            "Expected '[related link](/related)' in:\n{result}"
        );
    }

    /// htmd converts the full document including nav. We verify the conversion
    /// still produces markdown output (the nav links become markdown links).
    #[test]
    fn extract_markdown_includes_full_document() {
        let result = apply_extract(BLOG_HTML, Some("markdown"), None).unwrap();
        // htmd converts everything — nav included as markdown links
        assert!(
            result.contains("Blog Post Title"),
            "Expected article heading in markdown output"
        );
    }

    #[test]
    fn extract_markdown_preserves_list_items() {
        let result = apply_extract(BLOG_HTML, Some("markdown"), None).unwrap();
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
    fn extract_markdown_empty_html() {
        let result = apply_extract("", Some("markdown"), None).unwrap();
        assert!(
            result.trim().is_empty(),
            "Expected empty markdown for empty input"
        );
    }
}

// ─── Text extraction ─────────────────────────────────────────────────────────

#[cfg(feature = "fetch-html")]
mod text {
    use super::*;

    #[test]
    fn extract_text_all() {
        let result = apply_extract(BLOG_HTML, Some("text"), None).unwrap();
        assert!(
            result.contains("Blog Post Title"),
            "Expected 'Blog Post Title' in:\n{result}"
        );
        assert!(
            result.contains("main content"),
            "Expected 'main content' in:\n{result}"
        );
    }

    #[test]
    fn extract_text_with_selector_h1() {
        let result = apply_extract(BLOG_HTML, Some("text"), Some("article h1")).unwrap();
        assert!(
            result.contains("Blog Post Title"),
            "Expected 'Blog Post Title' in:\n{result}"
        );
    }

    #[test]
    fn extract_text_with_selector_p() {
        let result = apply_extract(BLOG_HTML, Some("text"), Some("article p")).unwrap();
        assert!(
            result.contains("main content"),
            "Expected 'main content' in:\n{result}"
        );
    }

    #[test]
    fn extract_text_no_html_tags() {
        let result = apply_extract(BLOG_HTML, Some("text"), None).unwrap();
        assert!(!result.contains("<h1>"), "Should not contain raw <h1> tags");
        assert!(!result.contains("<p>"), "Should not contain raw <p> tags");
        assert!(
            !result.contains("</article>"),
            "Should not contain closing tags"
        );
    }

    #[test]
    fn extract_text_selector_returns_only_matched() {
        let html =
            r#"<html><body><div class="a">Alpha</div><div class="b">Beta</div></body></html>"#;
        let result = apply_extract(html, Some("text"), Some("div.a")).unwrap();
        assert!(result.contains("Alpha"));
        assert!(!result.contains("Beta"));
    }

    #[test]
    fn extract_text_empty_html() {
        let result = apply_extract("", Some("text"), None).unwrap();
        // scraper parses empty string, text extraction returns whitespace or empty
        assert!(
            result.trim().is_empty(),
            "Expected empty text for empty input, got: '{result}'"
        );
    }

    #[test]
    fn extract_text_invalid_selector_returns_error() {
        let result = apply_extract(BLOG_HTML, Some("text"), Some("[[[invalid"));
        assert!(result.is_err(), "Invalid selector should produce an error");
    }
}

// ─── Metadata extraction ─────────────────────────────────────────────────────

#[cfg(feature = "fetch-html")]
mod metadata {
    use super::*;

    fn parse_metadata(html: &str) -> serde_json::Value {
        let result = apply_extract(html, Some("metadata"), None).unwrap();
        serde_json::from_str(&result).expect("metadata should be valid JSON")
    }

    #[test]
    fn extract_metadata_title() {
        let meta = parse_metadata(BLOG_HTML);
        assert_eq!(meta["title"], "Blog Post");
    }

    #[test]
    fn extract_metadata_description() {
        let meta = parse_metadata(BLOG_HTML);
        assert_eq!(meta["description"], "A great blog post");
    }

    #[test]
    fn extract_metadata_og() {
        let meta = parse_metadata(BLOG_HTML);
        assert_eq!(meta["og"]["title"], "OG Blog Title");
        assert_eq!(meta["og"]["image"], "https://example.com/og.jpg");
    }

    #[test]
    fn extract_metadata_twitter() {
        let meta = parse_metadata(BLOG_HTML);
        assert_eq!(meta["twitter"]["card"], "summary_large_image");
    }

    #[test]
    fn extract_metadata_jsonld() {
        let meta = parse_metadata(BLOG_HTML);
        let json_ld = meta["json_ld"]
            .as_array()
            .expect("json_ld should be an array");
        assert!(
            !json_ld.is_empty(),
            "json_ld should have at least one entry"
        );
        assert_eq!(json_ld[0]["@type"], "Article");
        assert_eq!(json_ld[0]["headline"], "JSON-LD Title");
    }

    #[test]
    fn extract_metadata_canonical() {
        let meta = parse_metadata(BLOG_HTML);
        assert_eq!(meta["canonical"], "https://example.com/blog/post");
    }

    #[test]
    fn extract_metadata_missing_fields() {
        let html = "<html><head></head><body></body></html>";
        let meta = parse_metadata(html);
        // No title, no og, no twitter — should all be absent / null
        assert!(meta.get("title").is_none() || meta["title"].is_null());
        assert!(meta.get("og").is_none() || meta["og"].is_null());
        assert!(meta.get("twitter").is_none() || meta["twitter"].is_null());
    }

    #[test]
    fn extract_metadata_returns_valid_json() {
        let result = apply_extract(BLOG_HTML, Some("metadata"), None).unwrap();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
        assert!(parsed.is_ok(), "metadata output must be valid JSON");
    }
}

// ─── Link extraction ─────────────────────────────────────────────────────────

#[cfg(feature = "fetch-html")]
mod links {
    use super::*;

    fn parse_links(html: &str) -> serde_json::Value {
        let result = apply_extract(html, Some("links"), None).unwrap();
        serde_json::from_str(&result).expect("links should be valid JSON")
    }

    #[test]
    fn extract_links_count() {
        let parsed = parse_links(BLOG_HTML);
        // BLOG_HTML has: /, /about, /related, https://twitter.com/example
        assert_eq!(parsed["count"], 4);
    }

    #[test]
    fn extract_links_internal_urls() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let urls: Vec<&str> = links.iter().map(|l| l["url"].as_str().unwrap()).collect();
        assert!(urls.contains(&"/"), "Expected '/' in links");
        assert!(urls.contains(&"/about"), "Expected '/about' in links");
        assert!(urls.contains(&"/related"), "Expected '/related' in links");
    }

    #[test]
    fn extract_links_external_urls() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let urls: Vec<&str> = links.iter().map(|l| l["url"].as_str().unwrap()).collect();
        assert!(
            urls.iter().any(|u| u.contains("twitter.com")),
            "Expected external twitter.com link"
        );
    }

    #[test]
    fn extract_links_anchor_text() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let home_link = links.iter().find(|l| l["url"] == "/").unwrap();
        assert_eq!(home_link["anchor"], "Home");
    }

    #[test]
    fn extract_links_nofollow() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let twitter_link = links
            .iter()
            .find(|l| l["url"].as_str().unwrap_or("").contains("twitter.com"))
            .expect("twitter link should exist");
        assert_eq!(twitter_link["rel"], "nofollow");
    }

    #[test]
    fn extract_links_about_anchor() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let about_link = links.iter().find(|l| l["url"] == "/about").unwrap();
        assert_eq!(about_link["anchor"], "About");
    }

    #[test]
    fn extract_links_related_anchor() {
        let parsed = parse_links(BLOG_HTML);
        let links = parsed["links"].as_array().unwrap();
        let related_link = links.iter().find(|l| l["url"] == "/related").unwrap();
        assert_eq!(related_link["anchor"], "related link");
    }

    #[test]
    fn extract_links_empty_html() {
        let parsed = parse_links("<html><body></body></html>");
        assert_eq!(parsed["count"], 0);
        assert!(parsed["links"].as_array().unwrap().is_empty());
    }
}

// ─── JSONPath extraction ─────────────────────────────────────────────────────

mod jsonpath {
    use super::*;

    #[test]
    fn extract_jsonpath_simple() {
        let result = apply_extract(JSON_API, Some("jsonpath"), Some("$.data.total")).unwrap();
        assert_eq!(result, "2");
    }

    #[test]
    fn extract_jsonpath_array() {
        let result =
            apply_extract(JSON_API, Some("jsonpath"), Some("$.data.items[*].name")).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn extract_jsonpath_nested() {
        let result =
            apply_extract(JSON_API, Some("jsonpath"), Some("$.data.items[0].score")).unwrap();
        assert_eq!(result, "95");
    }

    #[test]
    fn extract_jsonpath_second_item() {
        let result =
            apply_extract(JSON_API, Some("jsonpath"), Some("$.data.items[1].name")).unwrap();
        assert_eq!(result, "\"Beta\"");
    }

    #[test]
    fn extract_jsonpath_no_match() {
        let result = apply_extract(JSON_API, Some("jsonpath"), Some("$.data.nonexistent")).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn extract_jsonpath_invalid_expression() {
        let result = apply_extract(JSON_API, Some("jsonpath"), Some("$[invalid"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid JSONPath"));
    }

    #[test]
    fn extract_jsonpath_invalid_json_body() {
        let result = apply_extract("not json", Some("jsonpath"), Some("$.foo"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not valid JSON"));
    }

    #[test]
    fn extract_jsonpath_requires_selector() {
        let result = apply_extract(JSON_API, Some("jsonpath"), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("jsonpath requires 'selector'"));
    }

    #[test]
    fn extract_jsonpath_all_scores() {
        let result =
            apply_extract(JSON_API, Some("jsonpath"), Some("$.data.items[*].score")).unwrap();
        let parsed: Vec<u64> = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, vec![95, 42]);
    }
}

// ─── Feed extraction (feature-gated) ────────────────────────────────────────

#[cfg(feature = "fetch-feed")]
mod feed {
    use super::*;

    #[test]
    fn extract_feed_rss() {
        let result = apply_extract(RSS_FEED, Some("feed"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["title"], "Test Feed");
        assert!(parsed["entries"].as_array().is_some());
    }

    #[test]
    fn extract_feed_entry_title() {
        let result = apply_extract(RSS_FEED, Some("feed"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let entries = parsed["entries"].as_array().unwrap();
        assert!(!entries.is_empty(), "Feed should have at least one entry");
        assert_eq!(entries[0]["title"], "Entry 1");
    }

    #[test]
    fn extract_feed_entry_url() {
        let result = apply_extract(RSS_FEED, Some("feed"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["url"], "https://example.com/1");
    }

    #[test]
    fn extract_feed_entry_count() {
        let result = apply_extract(RSS_FEED, Some("feed"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["entry_count"], 1);
    }
}

// ─── Article extraction (feature-gated) ──────────────────────────────────────

#[cfg(feature = "fetch-article")]
mod article {
    use super::*;

    /// Readability needs a substantial amount of text to detect an article.
    const ARTICLE_HTML: &str = r#"<html><head><title>Big Article</title></head><body>
<nav><a href="/">Home</a><a href="/about">About</a></nav>
<article>
<h1>Big Article Title</h1>
<p>This is the main body of the article. It needs to be long enough
for the readability algorithm to consider it as content. The algorithm
typically requires a minimum amount of text content to identify an
article region. So we add several sentences here to make sure the
extraction works properly. This is important for testing purposes.
We continue adding more content so the readability heuristics work.</p>
<p>Second paragraph with more content to help the readability score.
The more text we have here, the better the extraction will work.
Adding even more text for the readability parser. This ensures that
the algorithm correctly identifies the main content block.</p>
</article>
<footer>Footer content</footer>
</body></html>"#;

    #[test]
    fn extract_article_content() {
        let result = apply_extract(ARTICLE_HTML, Some("article"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let text = parsed["text_content"].as_str().unwrap_or("");
        assert!(
            text.contains("main body"),
            "Article text should contain article content"
        );
    }

    #[test]
    fn extract_article_strips_nav() {
        let result = apply_extract(ARTICLE_HTML, Some("article"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let text = parsed["text_content"].as_str().unwrap_or("");
        // Readability should not include navigation in extracted content
        assert!(
            !text.contains("Home"),
            "Article text should not contain nav links"
        );
    }

    #[test]
    fn extract_article_returns_structured_json() {
        let result = apply_extract(ARTICLE_HTML, Some("article"), None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("title").is_some(), "Should have title field");
        assert!(parsed.get("content").is_some(), "Should have content field");
        assert!(
            parsed.get("text_content").is_some(),
            "Should have text_content field"
        );
    }
}

// ─── Selector extraction ─────────────────────────────────────────────────────

#[cfg(feature = "fetch-html")]
mod selector {
    use super::*;

    #[test]
    fn extract_selector_html() {
        let result = apply_extract(BLOG_HTML, Some("selector"), Some("article ul")).unwrap();
        assert!(
            result.contains("<li>"),
            "Selector should return raw HTML with <li>"
        );
        assert!(result.contains("Item 1"), "Selector should return content");
    }

    #[test]
    fn extract_selector_multiple_matches() {
        let result = apply_extract(BLOG_HTML, Some("selector"), Some("li")).unwrap();
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
    }

    #[test]
    fn extract_selector_returns_outer_html() {
        let result = apply_extract(BLOG_HTML, Some("selector"), Some("article h1")).unwrap();
        assert!(
            result.contains("<h1>"),
            "Selector should include the matched element's outer HTML"
        );
        assert!(result.contains("Blog Post Title"));
    }

    #[test]
    fn extract_selector_requires_selector_field() {
        let result = apply_extract(BLOG_HTML, Some("selector"), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires 'selector' field"));
    }

    #[test]
    fn extract_selector_no_match_returns_empty() {
        let result = apply_extract(BLOG_HTML, Some("selector"), Some("div.nonexistent")).unwrap();
        assert!(result.is_empty(), "No match should return empty string");
    }
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

mod edge_cases {
    use super::*;

    #[test]
    fn extract_none_returns_raw() {
        let result = apply_extract(BLOG_HTML, None, None).unwrap();
        assert_eq!(result, BLOG_HTML, "No extract should return original body");
    }

    #[test]
    fn extract_empty_html_none() {
        let result = apply_extract("", None, None).unwrap();
        assert_eq!(result, "", "Empty input with no extract returns empty");
    }

    #[test]
    fn extract_invalid_mode() {
        let result = apply_extract(BLOG_HTML, Some("bogus_mode"), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Unknown extract mode"),
            "Error should mention unknown mode"
        );
        assert!(
            err.contains("bogus_mode"),
            "Error should include the invalid mode name"
        );
    }

    #[test]
    fn extract_none_preserves_json() {
        let result = apply_extract(JSON_API, None, None).unwrap();
        assert_eq!(result, JSON_API, "JSON should be preserved verbatim");
    }

    #[test]
    fn extract_none_preserves_xml() {
        let result = apply_extract(RSS_FEED, None, None).unwrap();
        assert_eq!(result, RSS_FEED, "XML should be preserved verbatim");
    }
}
