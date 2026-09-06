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
//! NOT an author-facing [`crate::ExtractMode`] — the single-fetch `mode:` set
//! stays the closed canonical 9; the digest is the fixed shape a crawl
//! yields per page (a 1-page digest = `traverse: { max_pages: 1 }`).
//! TOTAL like every HTML mode: malformed markup yields what html5ever
//! salvages — honest emptiness, never a failure. Caps bound every list
//! (a hostile page cannot balloon the crawl output).
//!
//! One admission, every DOM path: the digest passes the SAME depth guard
//! the author-facing HTML modes pass ([`html::guard_depth`]) BEFORE any
//! parser sees the body. It used to go straight to `Html::parse_document`
//! (measured 2026-09-06 · the V9 telemetry review · T9-F07): the one
//! path a crawl takes was the one path with no guard, so a depth bomb
//! that `mode: markdown` refused in microseconds could still reach a
//! recursive teardown through `traverse:`.
//!
//! Discovery is not the preview: the crawler reads EVERY link the page
//! carries ([`page_digest_discovering`]); the digest's `links` facet is
//! the ≤30 preview of that list. Capping discovery at what the preview
//! shows silently lost the 31st link (T9-F08).

use std::sync::LazyLock;

use scraper::{Html, Selector};

use crate::{ExtractError, ExtractMode, html, metadata};

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
///
/// # Errors
///
/// The depth admission every DOM mode passes: a pathologically deep
/// document is refused before any parser sees it (the `text` wording —
/// the digest's largest facet is the page text).
pub fn page_digest(body: &str, base: Option<&str>) -> Result<serde_json::Value, ExtractError> {
    page_digest_discovering(body, base).map(|(digest, _)| digest)
}

/// The digest plus EVERY link the page carries (absolute · the
/// `links`-mode contract) — the crawler's frontier input. The digest's
/// `links` facet is the ≤30 preview of this list, never its bound.
///
/// # Errors
///
/// The same depth admission as [`page_digest`].
pub fn page_digest_discovering(
    body: &str,
    base: Option<&str>,
) -> Result<(serde_json::Value, Vec<String>), ExtractError> {
    html::guard_depth(body, ExtractMode::Text)?;
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

    let discovered: Vec<String> = match metadata::links(body, base) {
        serde_json::Value::Array(all) => all
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let links: Vec<&str> = discovered
        .iter()
        .map(String::as_str)
        .take(MAX_LINKS)
        .collect();

    let images = collect_images(&document, base);

    let colors = scan_hex_colors(body);

    let text = match html::text(body) {
        serde_json::Value::String(t) => truncate_chars(&t, MAX_TEXT_CHARS),
        _ => String::new(),
    };

    let digest = serde_json::json!({
        "title": title,
        "description": description,
        "headings": headings,
        "links": links,
        "images": images,
        "colors": colors,
        "text": text,
    });
    Ok((digest, discovered))
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
        let d = page_digest(PAGE, Some("https://acme.test/")).expect("digest");
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
        let d = page_digest(PAGE, None).expect("digest");
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
        let d = page_digest(&hostile, Some("https://x.test/")).expect("digest");
        assert_eq!(d["headings"].as_array().expect("a").len(), MAX_HEADINGS);
        assert_eq!(d["images"].as_array().expect("a").len(), MAX_IMAGES);
        assert_eq!(d["colors"].as_array().expect("a").len(), MAX_COLORS);
    }

    #[test]
    fn hex_scan_rejects_non_color_runs() {
        let d = page_digest(
            "<p>#12 #abcdef123 sha #deadbeefcafe but #AbC and #a1b2c3 live</p>",
            None,
        )
        .expect("digest");
        assert_eq!(d["colors"], serde_json::json!(["#abc", "#a1b2c3"]));
    }

    /// T9-F07 · the one path a crawl takes passes the SAME admission the
    /// author-facing modes pass: a depth bomb is refused BEFORE any
    /// parser sees it, with the guard's own wording.
    #[test]
    fn a_depth_bomb_is_refused_before_any_parse() {
        let bomb = "<div>".repeat(html::MAX_HTML_DEPTH + 8);
        let err = page_digest(&bomb, Some("https://t.test/")).expect_err("refused");
        assert!(
            err.to_string().contains("nesting exceeds"),
            "the depth guard's wording: {err}"
        );
        let (_, discovered) =
            page_digest_discovering("<a href=\"/x\">x</a>", Some("https://t.test/"))
                .expect("a flat page still digests");
        assert_eq!(discovered, vec!["https://t.test/x".to_owned()]);
    }

    /// T9-F08 · discovery carries EVERY link; the digest keeps its ≤30
    /// preview (stdlib §fetch · traverse). The 31st link exists for the
    /// crawler even though the preview never shows it.
    #[test]
    fn discovery_is_not_capped_by_the_preview() {
        use std::fmt::Write as _;
        let mut anchors = String::new();
        for i in 0..40 {
            // Writing to a String is infallible.
            let _ = write!(anchors, "<a href=\"/p{i:02}\">p</a>");
        }
        let body = format!("<html><body>{anchors}</body></html>");
        let (digest, discovered) =
            page_digest_discovering(&body, Some("https://t.test/")).expect("digest");
        assert_eq!(
            discovered.len(),
            40,
            "every link is discovered: {discovered:?}"
        );
        assert_eq!(
            digest["links"].as_array().expect("preview").len(),
            MAX_LINKS,
            "the preview keeps the spec cap"
        );
        assert_eq!(
            discovered[30], "https://t.test/p30",
            "the 31st link is reachable"
        );
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
