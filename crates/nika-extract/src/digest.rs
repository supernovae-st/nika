// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The page digest — the composite per-page shape `nika:fetch`'s
//! `traverse:` emits (stdlib §fetch · traverse). One page, one object:
//!
//! ```json
//! { "title": "…", "description": "…", "headings": [..≤16],
//!   "links": [..≤30 absolute], "images": [..≤24 absolute],
//!   "colors": ["#0400ff", ..≤20], "text": "…≤4000 chars" }
//! ```
//!
//! NOT an author-facing [`ExtractMode`] — the single-fetch `mode:` set
//! stays the closed canonical 9; the digest is the fixed shape a crawl
//! yields per page (a 1-page digest = `traverse: { max_pages: 1 }`).
//! TOTAL like every HTML mode: malformed markup yields what html5ever
//! salvages — honest emptiness, never a failure. Caps bound every list
//! (a hostile page cannot balloon the crawl output).

use std::sync::LazyLock;

use scraper::{Html, Selector};

use crate::{html, metadata};

/// One `LazyLock` static selector (the `metadata.rs` idiom — the
/// unreachable parse-failure branch fails SOFT as a skipped selector).
fn parse_static(css: &'static str) -> Option<Selector> {
    Selector::parse(css).ok()
}

static TITLE: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("head > title"));
static META_DESCRIPTION: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static(r#"meta[name="description"], meta[property="og:description"]"#));
static HEADINGS: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("h1, h2, h3"));
static IMAGES: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static("img[src], img[data-src]"));
static OG_IMAGE: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static(r#"meta[property="og:image"]"#));

/// The digest caps — the crawl-output budget per page.
const MAX_HEADINGS: usize = 16;
const MAX_LINKS: usize = 30;
const MAX_IMAGES: usize = 24;
const MAX_COLORS: usize = 20;
const MAX_TEXT_CHARS: usize = 4000;

/// Extract the composite page digest. `base` resolves relative
/// `href`/`src` (absent → relative references are skipped, the
/// `links`-mode contract).
#[must_use]
pub fn page_digest(body: &str, base: Option<&str>) -> serde_json::Value {
    let document = Html::parse_document(body);

    let title = TITLE
        .as_ref()
        .and_then(|s| document.select(s).next())
        .map(|el| collapse_ws(&el.text().collect::<String>()))
        .unwrap_or_default();

    let description = META_DESCRIPTION
        .as_ref()
        .and_then(|s| document.select(s).find_map(|el| el.value().attr("content")))
        .map(collapse_ws)
        .unwrap_or_default();

    let headings: Vec<String> = HEADINGS
        .as_ref()
        .map(|s| {
            dedup_take(
                document
                    .select(s)
                    .map(|el| collapse_ws(&el.text().collect::<String>()))
                    .filter(|h| !h.is_empty()),
                MAX_HEADINGS,
            )
        })
        .unwrap_or_default();

    let links = match metadata::links(body, base) {
        serde_json::Value::Array(all) => all
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .take(MAX_LINKS)
            .collect(),
        _ => Vec::new(),
    };

    let images = collect_images(&document, base);

    let colors = scan_hex_colors(body);

    let text = match html::text(body) {
        serde_json::Value::String(t) => truncate_chars(&t, MAX_TEXT_CHARS),
        _ => String::new(),
    };

    serde_json::json!({
        "title": title,
        "description": description,
        "headings": headings,
        "links": links,
        "images": images,
        "colors": colors,
        "text": text,
    })
}

/// `img[src|data-src]` + `og:image`, absolutized against `base`,
/// first-seen dedup, capped.
fn collect_images(document: &Html, base: Option<&str>) -> Vec<String> {
    let mut seen = Vec::new();
    let mut push = |raw: &str| {
        if seen.len() >= MAX_IMAGES {
            return;
        }
        if let Some(resolved) = absolutize(raw, base)
            && !seen.contains(&resolved)
        {
            seen.push(resolved);
        }
    };
    if let Some(s) = IMAGES.as_ref() {
        for el in document.select(s) {
            if let Some(src) = el
                .value()
                .attr("src")
                .or_else(|| el.value().attr("data-src"))
            {
                push(src);
            }
        }
    }
    if let Some(s) = OG_IMAGE.as_ref() {
        for el in document.select(s) {
            if let Some(content) = el.value().attr("content") {
                push(content);
            }
        }
    }
    seen
}

/// Resolve a reference to an absolute http(s) URL — the `links`-mode
/// contract: no base → absolute inputs only; non-http(s) schemes are
/// dropped (mailto · javascript · data).
fn absolutize(raw: &str, base: Option<&str>) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resolved = if let Ok(absolute) = url::Url::parse(trimmed) {
        absolute
    } else {
        url::Url::parse(base?).ok()?.join(trimmed).ok()?
    };
    matches!(resolved.scheme(), "http" | "https").then(|| resolved.to_string())
}

/// Scan `#hex` color tokens (3–8 hex digits · CSS forms) in appearance
/// order, lowercase-normalized, first-seen dedup, capped. A plain byte
/// scan — the token grammar is too small to warrant a regex engine.
fn scan_hex_colors(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() && out.len() < MAX_COLORS {
        if bytes[i] == b'#' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && end - start < 9 && bytes[end].is_ascii_hexdigit() {
                end += 1;
            }
            let len = end - start;
            // CSS hex forms: #rgb · #rgba · #rrggbb · #rrggbbaa — and the
            // run must END there (a 9th hex digit means "not a color").
            let terminated = end >= bytes.len() || !bytes[end].is_ascii_hexdigit();
            if matches!(len, 3 | 4 | 6 | 8) && terminated {
                // ASCII-only slice — safe to lowercase bytewise.
                let token = format!("#{}", body[start..end].to_lowercase());
                if !out.contains(&token) {
                    out.push(token);
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Whitespace-collapse (the heading/title normalizer).
fn collapse_ws(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// First-seen dedup + cap.
fn dedup_take(items: impl Iterator<Item = String>, cap: usize) -> Vec<String> {
    let mut seen = Vec::new();
    for item in items {
        if seen.len() >= cap {
            break;
        }
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen
}

/// Truncate on a char boundary (never split a code point).
fn truncate_chars(raw: &str, cap: usize) -> String {
    if raw.chars().count() <= cap {
        return raw.to_owned();
    }
    raw.chars().take(cap).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PAGE: &str = r#"<html><head>
        <title>  Acme   Widgets </title>
        <meta name="description" content="The widget shop.">
        <meta property="og:image" content="/og/cover.png">
        <style>.hero { color: #0400FF; background: #fff; } .x { border: #0400ff; }</style>
        </head><body>
        <h1>Widgets</h1><h2>Catalog</h2><h3></h3>
        <a href="/shop">Shop</a>
        <a href="https://other.test/away">Away</a>
        <img src="/img/hero.png"><img data-src="/img/lazy.jpg">
        <img src="mailto:no">
        <p>Body prose here.</p>
        </body></html>"#;

    #[test]
    fn digest_extracts_every_facet_of_the_page() {
        let d = page_digest(PAGE, Some("https://acme.test/"));
        assert_eq!(d["title"], "Acme Widgets", "whitespace-collapsed");
        assert_eq!(d["description"], "The widget shop.");
        assert_eq!(d["headings"], serde_json::json!(["Widgets", "Catalog"]));
        let links: Vec<&str> = d["links"]
            .as_array()
            .expect("links array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(links.contains(&"https://acme.test/shop"), "{links:?}");
        assert!(links.contains(&"https://other.test/away"), "{links:?}");
        let images = d["images"].as_array().expect("images array");
        assert!(
            images.contains(&serde_json::json!("https://acme.test/img/hero.png")),
            "src absolutized: {images:?}"
        );
        assert!(
            images.contains(&serde_json::json!("https://acme.test/img/lazy.jpg")),
            "data-src counts: {images:?}"
        );
        assert!(
            images.contains(&serde_json::json!("https://acme.test/og/cover.png")),
            "og:image counts: {images:?}"
        );
        assert!(
            !images
                .iter()
                .any(|v| { v.as_str().is_some_and(|s| s.starts_with("mailto")) }),
            "non-http schemes dropped"
        );
        assert_eq!(
            d["colors"],
            serde_json::json!(["#0400ff", "#fff"]),
            "lowercase · first-seen dedup"
        );
        let text = d["text"].as_str().expect("text");
        assert!(text.contains("Body prose here."));
    }

    #[test]
    fn no_base_url_keeps_absolute_references_only() {
        let d = page_digest(PAGE, None);
        let images = d["images"].as_array().expect("array");
        assert!(images.is_empty(), "relative srcs skipped: {images:?}");
    }

    #[test]
    fn caps_bound_every_list() {
        use std::fmt::Write as _;
        let mut body = String::new();
        for i in 0..200 {
            // Writing to a String is infallible.
            let _ = writeln!(
                body,
                "<h2>h{i}</h2><img src=\"https://x.test/{i}.png\"> #a{i:05x}"
            );
        }
        let hostile = format!("<html><body>{body}</body></html>");
        let d = page_digest(&hostile, Some("https://x.test/"));
        assert_eq!(d["headings"].as_array().expect("a").len(), MAX_HEADINGS);
        assert_eq!(d["images"].as_array().expect("a").len(), MAX_IMAGES);
        assert_eq!(d["colors"].as_array().expect("a").len(), MAX_COLORS);
    }

    #[test]
    fn hex_scan_rejects_non_color_runs() {
        let d = page_digest(
            "<p>#12 #abcdef123 sha #deadbeefcafe but #AbC and #a1b2c3 live</p>",
            None,
        );
        assert_eq!(d["colors"], serde_json::json!(["#abc", "#a1b2c3"]));
    }

    proptest! {
        /// TOTAL: the digest never panics, whatever the bytes (the
        /// crate-wide totality contract every HTML mode carries).
        #[test]
        fn digest_is_total(body in ".{0,2048}") {
            let _ = page_digest(&body, Some("https://t.test/"));
            let _ = page_digest(&body, None);
        }
    }
}
