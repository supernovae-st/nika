// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-extract` — the fetch extract modes (L1.5 · pure).
//!
//! The transformation half of `nika:fetch` (`stdlib/extract-modes-v0.1.md`):
//! `&str` in, [`serde_json::Value`] out, **zero I/O · zero async · zero
//! locks**. The mode vocabulary is [`ExtractMode`] (`nika-types` L0 — ONE
//! closed set shared with the static checker and the builtin).
//!
//! | Mode | Composes | Output |
//! |---|---|---|
//! | `markdown` | `htmd` | String (Markdown) |
//! | `article` | rule cascade → `dom_smoothie` → boilerpipe | String (Markdown) |
//! | `text` | `scraper` DOM walk | String (plain · block `\n`) |
//! | `selector` | `scraper` CSS select | String (HTML of matches) |
//! | `metadata` | `scraper` head walk | Object (title·description·og·twitter·canonical·lang) |
//! | `links` | `scraper` + `url` join | Array of absolute URLs |
//! | `feed` | `feed-rs` | Object (title·description·link·updated·items) |
//! | `sitemap` | `quick-xml` | Array of `{loc, lastmod?}` |
//! | `raw` | identity | String (the decoded body) |
//! | `jq` | **NOT here** — the builtin composes its jq engine | — |
//!
//! Charset is the CALLER's contract (`nika-builtin` decodes — v0.1 is
//! strict UTF-8 per the spec's `raw` rule); this crate never sees bytes.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod article;
mod blocks;
mod digest;
mod feed;
mod html;
pub mod link_header;
mod metadata;
mod page_type;
mod sitemap;
mod zones;

pub use digest::{page_digest, page_digest_discovering};
pub use feed::feed_from_bytes;
pub use nika_types::extract::{EXTRACT_MODE_NAMES, ExtractMode, UnknownExtractMode};

/// Mode-specific options for [`extract`] (`selector:` for `mode:
/// selector`, the fetch URL as `base_url` for link/canonical
/// resolution).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ExtractOptions<'a> {
    /// CSS selector (`mode: selector` only — its REQUIRED argument).
    pub selector: Option<&'a str>,
    /// The fetched URL — base for relative `href`/canonical resolution
    /// (`links` · `metadata`). Without it, relative links are skipped.
    pub base_url: Option<&'a str>,
}

impl ExtractOptions<'_> {
    /// No selector, no base URL.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Extraction failures. The builtin maps every variant onto
/// `NIKA-BUILTIN-FETCH-001` (the spec's single fetch error code) — the
/// variants exist for actionable MESSAGES, not for a parallel taxonomy.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ExtractError {
    /// A mode's required argument is missing (`selector` without `selector:`).
    #[error("mode `{mode}` requires the `{arg}:` argument")]
    MissingArg {
        /// The mode that needs it.
        mode: ExtractMode,
        /// The missing argument name.
        arg: &'static str,
    },

    /// The CSS selector does not parse.
    #[error("invalid CSS selector `{selector}`: {reason}")]
    Selector {
        /// The selector as received.
        selector: String,
        /// Parser detail.
        reason: String,
    },

    /// HTML transformation failed (markdown · article · text · selector).
    #[error("{mode} extraction failed: {reason}")]
    Html {
        /// The failing mode.
        mode: ExtractMode,
        /// Converter detail.
        reason: String,
    },

    /// The body is not a parseable RSS/Atom/JSON feed.
    #[error("feed parse failed: {reason}")]
    Feed {
        /// Parser detail.
        reason: String,
    },

    /// The body is not a parseable sitemap / sitemap index.
    #[error("sitemap parse failed: {reason}")]
    Sitemap {
        /// Parser detail.
        reason: String,
    },

    /// The mode is not handled by this crate (jq composes at the
    /// builtin layer · future stdlib modes are absent from this build).
    #[error("mode `{mode}` is not handled by nika-extract — {hint}")]
    Unsupported {
        /// The unhandled mode.
        mode: ExtractMode,
        /// Where it IS handled (or why it cannot be).
        hint: &'static str,
    },
}

/// Apply one extract mode to a decoded response body.
///
/// Total over arbitrary input: malformed HTML/XML degrades to an
/// `Err`, never a panic (property-tested per mode).
///
/// # Errors
///
/// [`ExtractError`] — see each variant; the builtin renders every one
/// into `NIKA-BUILTIN-FETCH-001 · <message>`.
pub fn extract(
    body: &str,
    mode: ExtractMode,
    opts: &ExtractOptions<'_>,
) -> Result<serde_json::Value, ExtractError> {
    // Every HTML-DOM mode passes the body to a parser/recursive
    // consumer (htmd → markup5ever_rcdom recursive Drop · dom_smoothie ·
    // scraper). One cheap byte-scan depth guard up front stops the
    // stack-overflow-on-teardown DoS for ALL of them before any parse
    // (raw/jq are passthrough; feed/sitemap are streaming XML, not an
    // HTML DOM — no recursive teardown).
    if matches!(
        mode,
        ExtractMode::Markdown
            | ExtractMode::Article
            | ExtractMode::Text
            | ExtractMode::Selector
            | ExtractMode::Metadata
            | ExtractMode::Links
    ) {
        html::guard_depth(body, mode)?;
    }
    match mode {
        ExtractMode::Raw => Ok(serde_json::Value::String(body.to_owned())),
        ExtractMode::Markdown => html::markdown(body),
        ExtractMode::Article => article::article(body, opts.base_url),
        ExtractMode::Text => Ok(html::text(body)),
        ExtractMode::Selector => {
            let selector = opts.selector.ok_or(ExtractError::MissingArg {
                mode: ExtractMode::Selector,
                arg: "selector",
            })?;
            html::selector(body, selector)
        }
        ExtractMode::Metadata => Ok(metadata::metadata(body, opts.base_url)),
        ExtractMode::Links => Ok(metadata::links(body, opts.base_url)),
        ExtractMode::Feed => feed::feed(body),
        ExtractMode::Sitemap => sitemap::sitemap(body),
        ExtractMode::Jq => Err(ExtractError::Unsupported {
            mode: ExtractMode::Jq,
            hint: "the builtin layer composes its jq engine (one data language, one engine)",
        }),
        other => Err(ExtractError::Unsupported {
            mode: other,
            hint: "future stdlib mode — not in this build",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!DOCTYPE html>
<html lang="en"><head>
  <title>Demo Page</title>
  <meta name="description" content="A demo description.">
  <meta property="og:title" content="Demo OG">
  <meta property="og:image" content="https://example.com/og.jpg">
  <meta name="twitter:card" content="summary_large_image">
  <link rel="canonical" href="/article">
  <style>.x { color: red }</style>
  <script>alert("evil")</script>
</head><body>
  <nav><a href="/nav">navigation</a></nav>
  <h1>Demo Heading</h1>
  <p>First paragraph with a <a href="/relative">relative link</a>.</p>
  <p>Second paragraph with an <a href="https://other.example/abs">absolute link</a>.</p>
  <ul><li>alpha</li><li>beta</li></ul>
  <div class="content"><p>Inside the div.</p></div>
  <a href="mailto:x@example.com">mail</a>
  <a href="javascript:void(0)">js</a>
  <a href="/relative">duplicate relative</a>
  <footer>footer text</footer>
</body></html>"#;

    fn run(body: &str, mode: ExtractMode) -> Result<serde_json::Value, ExtractError> {
        extract(body, mode, &ExtractOptions::new())
    }

    fn with_base(body: &str, mode: ExtractMode) -> serde_json::Value {
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://example.com/article");
        extract(body, mode, &opts).expect("mode succeeds")
    }

    // ─── raw ─────────────────────────────────────────────────────────

    #[test]
    fn raw_is_the_identity() {
        let out = run("plain  body\n", ExtractMode::Raw).expect("raw");
        assert_eq!(out, serde_json::Value::String("plain  body\n".to_owned()));
    }

    // ─── markdown ────────────────────────────────────────────────────

    #[test]
    fn markdown_converts_and_strips_noise() {
        let out = run(PAGE, ExtractMode::Markdown).expect("markdown");
        let md = out.as_str().expect("string output");
        assert!(md.contains("# Demo Heading"), "heading survives: {md}");
        assert!(md.contains("First paragraph"), "prose survives");
        assert!(
            md.contains("[absolute link](https://other.example/abs)"),
            "links survive as markdown: {md}"
        );
        assert!(!md.contains("alert("), "script stripped");
        assert!(!md.contains("color: red"), "style stripped");
        assert!(!md.contains("navigation"), "nav stripped");
        assert!(!md.contains("footer text"), "footer stripped");
        assert!(md.contains("alpha"), "list items survive");
    }

    #[test]
    fn markdown_resolves_lazy_images() {
        // Lazy-loaded images (placeholder src + data-src/srcset) must emit
        // the REAL url, not the blank `data:` placeholder (the SOTA cheap
        // win · Firecrawl does the same). A normal <img src> is unchanged.
        let html = r#"<html><body>
          <img src="data:image/gif;base64,R0lGOD" data-src="https://cdn.example/real.jpg" alt="lazy one">
          <img srcset="https://cdn.example/small.jpg 320w, https://cdn.example/big.jpg 1280w" alt="responsive">
          <img src="https://cdn.example/normal.png" alt="normal">
          <img src="data:image/svg+xml,placeholder" data-lazy-src="https://cdn.example/lazy2.webp" alt="lazy two">
        </body></html>"#;
        let out = run(html, ExtractMode::Markdown).expect("markdown");
        let md = out.as_str().expect("string");
        // data-src resolved (placeholder src ignored).
        assert!(
            md.contains("(https://cdn.example/real.jpg)"),
            "data-src resolved: {md}"
        );
        assert!(!md.contains("base64"), "placeholder src dropped: {md}");
        // srcset → biggest (1280w over 320w).
        assert!(
            md.contains("(https://cdn.example/big.jpg)"),
            "srcset biggest: {md}"
        );
        assert!(!md.contains("small.jpg"), "srcset small dropped: {md}");
        // normal img unchanged (htmd-faithful).
        assert!(
            md.contains("![normal](https://cdn.example/normal.png)"),
            "normal img: {md}"
        );
        // data-lazy-src resolved.
        assert!(
            md.contains("(https://cdn.example/lazy2.webp)"),
            "data-lazy-src: {md}"
        );
    }

    // ─── text ────────────────────────────────────────────────────────

    #[test]
    fn text_emits_breaks_for_br_and_block_closes() {
        // Pins the two `skip_depth == 0` arms (mutation: `!=` flips
        // them): <br> emits a line split · block closes emit one too.
        let html = "<html><body><p>one<br>two</p><p>three</p></body></html>";
        let out = run(html, ExtractMode::Text).expect("text");
        let text = out.as_str().expect("string");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines, ["one", "two", "three"], "{text:?}");
    }

    #[test]
    fn text_strips_tags_and_scripts_keeps_breaks() {
        let out = run(PAGE, ExtractMode::Text).expect("text");
        let text = out.as_str().expect("string output");
        assert!(text.contains("Demo Heading"));
        assert!(text.contains("First paragraph with a relative link."));
        assert!(!text.contains('<'), "no markup: {text}");
        assert!(!text.contains("alert("), "script content stripped");
        assert!(!text.contains("color: red"), "style content stripped");
        // Block-level structure surfaces as line breaks.
        assert!(
            text.lines().count() >= 4,
            "block elements produce line structure: {text:?}"
        );
        // Spec: headers/footers PRESERVED for text mode (unlike markdown).
        assert!(text.contains("footer text"));
    }

    // ─── selector ────────────────────────────────────────────────────

    #[test]
    fn selector_returns_matching_html() {
        let mut opts = ExtractOptions::new();
        opts.selector = Some("div.content");
        let out = extract(PAGE, ExtractMode::Selector, &opts).expect("selector");
        let html = out.as_str().expect("string output");
        assert!(html.contains("<p>Inside the div.</p>"), "{html}");
        assert!(html.starts_with("<div class=\"content\">"));
    }

    #[test]
    fn selector_concatenates_multiple_matches() {
        let mut opts = ExtractOptions::new();
        opts.selector = Some("li");
        let out = extract(PAGE, ExtractMode::Selector, &opts).expect("selector");
        let html = out.as_str().expect("string output");
        assert!(html.contains("<li>alpha</li>"));
        assert!(html.contains("<li>beta</li>"));
    }

    #[test]
    fn selector_requires_its_argument_and_a_valid_selector() {
        let missing = run(PAGE, ExtractMode::Selector).expect_err("no selector arg");
        assert!(matches!(
            missing,
            ExtractError::MissingArg {
                arg: "selector",
                ..
            }
        ));

        let mut opts = ExtractOptions::new();
        opts.selector = Some(":::not-a-selector");
        let invalid = extract(PAGE, ExtractMode::Selector, &opts).expect_err("bad selector");
        assert!(
            matches!(invalid, ExtractError::Selector { .. }),
            "{invalid}"
        );
    }

    // ─── metadata ────────────────────────────────────────────────────

    #[test]
    fn metadata_extracts_the_spec_shape() {
        let out = with_base(PAGE, ExtractMode::Metadata);
        assert_eq!(out["title"], "Demo Page");
        assert_eq!(out["description"], "A demo description.");
        assert_eq!(out["og"]["title"], "Demo OG");
        assert_eq!(out["og"]["image"], "https://example.com/og.jpg");
        assert_eq!(out["twitter"]["card"], "summary_large_image");
        // Relative canonical resolved against the base URL.
        assert_eq!(out["canonical"], "https://example.com/article");
        assert_eq!(out["lang"], "en");
    }

    #[test]
    fn metadata_extracts_jsonld_structured_data() {
        // A product page: the price lives in JSON-LD, not the visible DOM
        // (the WCXB non-article case). metadata mode surfaces it under
        // `jsonld` for a downstream jq/infer step to walk.
        let html = r#"<html><head><title>Widget</title>
            <script type="application/ld+json">
            {"@context":"https://schema.org","@type":"Product","name":"Widget",
             "offers":{"@type":"Offer","price":"19.99","priceCurrency":"USD"}}
            </script>
            <script type="application/ld+json">{"@type":"BreadcrumbList"}</script>
            <script type="application/ld+json">{ this is not json }</script>
        </head><body></body></html>"#;
        let out = run(html, ExtractMode::Metadata).expect("metadata");
        let blocks = out["jsonld"].as_array().expect("jsonld array");
        // Two VALID blocks; the malformed third is skipped, not fatal.
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        assert_eq!(blocks[0]["@type"], "Product");
        assert_eq!(blocks[0]["offers"]["price"], "19.99");
        assert_eq!(blocks[1]["@type"], "BreadcrumbList");
        // A page with no JSON-LD omits the key (absence over empty array).
        let bare =
            run("<html><body><p>x</p></body></html>", ExtractMode::Metadata).expect("metadata");
        assert!(bare.get("jsonld").is_none(), "no key when absent: {bare}");
    }

    #[test]
    fn metadata_extracts_microdata_structured_data() {
        // A schema.org Product in MICRODATA (the 26% of pages JSON-LD
        // doesn't cover · HTTP Archive 2024). Exercises: itemtype → type[],
        // the W3C value-by-element rules (meta@content · a@href resolved ·
        // time@datetime · img@src · plain text), a NESTED item (offers →
        // Offer), and multi-token itemprop.
        let html = r#"<html><body>
          <div itemscope itemtype="https://schema.org/Product">
            <span itemprop="name">Widget</span>
            <img itemprop="image" src="/w.png">
            <a itemprop="url" href="/widget">link</a>
            <meta itemprop="sku" content="W-123">
            <time itemprop="releaseDate" datetime="2026-06-01">June</time>
            <div itemprop="offers" itemscope itemtype="https://schema.org/Offer">
              <data itemprop="price" value="19.99">$19.99</data>
              <span itemprop="priceCurrency">USD</span>
            </div>
          </div>
        </body></html>"#;
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://shop.example/");
        let out = extract(html, ExtractMode::Metadata, &opts).expect("metadata");
        let items = out["microdata"].as_array().expect("microdata array");
        assert_eq!(items.len(), 1, "one top-level item: {items:?}");
        let p = &items[0];
        assert_eq!(p["type"][0], "https://schema.org/Product");
        // properties values are ALWAYS arrays (a property may repeat).
        assert_eq!(p["properties"]["name"][0], "Widget");
        assert_eq!(p["properties"]["sku"][0], "W-123", "meta@content");
        assert_eq!(
            p["properties"]["image"][0], "https://shop.example/w.png",
            "img@src resolved vs base"
        );
        assert_eq!(
            p["properties"]["url"][0], "https://shop.example/widget",
            "a@href resolved"
        );
        assert_eq!(
            p["properties"]["releaseDate"][0], "2026-06-01",
            "time@datetime"
        );
        // The nested Offer is surfaced as a sub-item under `offers`, NOT
        // flattened into the parent (W3C item nesting · no-cross rule).
        let offer = &p["properties"]["offers"][0];
        assert_eq!(offer["type"][0], "https://schema.org/Offer");
        assert_eq!(offer["properties"]["price"][0], "19.99", "data@value");
        assert_eq!(offer["properties"]["priceCurrency"][0], "USD");
        // The parent must NOT have absorbed the nested item's props.
        assert!(
            p["properties"].get("price").is_none(),
            "no-cross: price belongs to Offer, not Product: {p}"
        );
        // A page with no microdata omits the key (absence over empty array).
        let bare =
            run("<html><body><p>x</p></body></html>", ExtractMode::Metadata).expect("metadata");
        assert!(
            bare.get("microdata").is_none(),
            "no key when absent: {bare}"
        );
    }

    #[test]
    fn metadata_microdata_multivalue_and_text_fallback() {
        // A repeated itemprop accumulates into the value array; a bare
        // element (no special attr) falls back to its text content.
        let html = r#"<div itemscope itemtype="https://schema.org/Recipe">
          <span itemprop="recipeIngredient">flour</span>
          <span itemprop="recipeIngredient">water</span>
          <p itemprop="description">  A   simple  bread.  </p>
        </div>"#;
        let out = run(html, ExtractMode::Metadata).expect("metadata");
        let r = &out["microdata"][0];
        let ingredients = r["properties"]["recipeIngredient"]
            .as_array()
            .expect("array");
        assert_eq!(ingredients.len(), 2, "repeated itemprop accumulates");
        assert_eq!(ingredients[0], "flour");
        assert_eq!(ingredients[1], "water");
        // text-content value is trimmed (inner whitespace is the DOM's).
        assert_eq!(r["properties"]["description"][0], "A   simple  bread.");
    }

    #[test]
    fn metadata_enrichment_author_published_favicon() {
        let html = r#"<html lang="fr"><head>
            <title>T</title>
            <meta name="author" content="Ada Lovelace">
            <meta property="article:published_time" content="2026-06-01T10:00:00Z">
            <link rel="shortcut icon" href="/favicon.ico">
        </head><body></body></html>"#;
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://example.com/post");
        let out = extract(html, ExtractMode::Metadata, &opts).expect("metadata");
        assert_eq!(out["author"], "Ada Lovelace");
        assert_eq!(out["published_time"], "2026-06-01T10:00:00Z");
        // rel~="icon" word-matches `shortcut icon` · resolved vs base.
        assert_eq!(out["favicon"], "https://example.com/favicon.ico");
    }

    #[test]
    fn metadata_on_bare_html_yields_stable_shape() {
        let out = run("<html><body><p>x</p></body></html>", ExtractMode::Metadata)
            .expect("metadata is total");
        // og/twitter are ALWAYS objects (stable shape for jq consumers).
        assert!(out["og"].is_object());
        assert!(out["twitter"].is_object());
        assert!(out.get("title").is_none() || out["title"].is_string());
    }

    #[test]
    fn metadata_title_description_fall_back_to_og_then_twitter() {
        // A SPA-style page with NO <title>/<meta description> but good og:/
        // twitter: tags — title/description borrow from them (standard).
        let spa = r#"<html><head>
            <meta property="og:title" content="OG Headline">
            <meta property="og:description" content="OG summary line.">
        </head><body></body></html>"#;
        let out = run(spa, ExtractMode::Metadata).expect("metadata");
        assert_eq!(out["title"], "OG Headline", "title ← og: {out}");
        assert_eq!(out["description"], "OG summary line.", "desc ← og: {out}");

        // twitter: fills when og: is absent.
        let tw = r#"<html><head>
            <meta name="twitter:title" content="TW Headline">
        </head><body></body></html>"#;
        let out = run(tw, ExtractMode::Metadata).expect("metadata");
        assert_eq!(out["title"], "TW Headline", "title ← twitter: {out}");

        // An EXPLICIT <title>/<meta description> always wins over og:.
        let explicit = r#"<html><head>
            <title>Real Title</title>
            <meta name="description" content="Real description.">
            <meta property="og:title" content="OG Title">
            <meta property="og:description" content="OG desc">
        </head><body></body></html>"#;
        let out = run(explicit, ExtractMode::Metadata).expect("metadata");
        assert_eq!(out["title"], "Real Title", "explicit title wins: {out}");
        assert_eq!(
            out["description"], "Real description.",
            "explicit desc wins: {out}"
        );
    }

    #[test]
    fn metadata_og_twitter_image_urls_are_absolutized() {
        // og:image/og:url/twitter:image are routinely relative — a
        // social-preview consumer needs absolute URLs (like canonical).
        let html = r#"<html><head>
            <meta property="og:image" content="/img/hero.jpg">
            <meta property="og:image:width" content="1200">
            <meta property="og:url" content="article">
            <meta name="twitter:image" content="//cdn.example/tw.png">
            <meta name="twitter:card" content="summary">
        </head><body></body></html>"#;
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://site.example/blog/post.html");
        let out = extract(html, ExtractMode::Metadata, &opts).expect("metadata");
        // relative → absolute against base.
        assert_eq!(
            out["og"]["image"], "https://site.example/img/hero.jpg",
            "{out}"
        );
        assert_eq!(
            out["og"]["url"], "https://site.example/blog/article",
            "{out}"
        );
        // protocol-relative → base scheme.
        assert_eq!(
            out["twitter"]["image"], "https://cdn.example/tw.png",
            "{out}"
        );
        // a NON-URL subkey (image:width) is untouched.
        assert_eq!(out["og"]["image:width"], "1200", "subkey untouched: {out}");
        assert_eq!(out["twitter"]["card"], "summary", "{out}");

        // An already-absolute og:image is kept verbatim.
        let abs = r#"<html><head>
            <meta property="og:image" content="https://other.example/x.jpg">
        </head><body></body></html>"#;
        let out = extract(abs, ExtractMode::Metadata, &opts).expect("metadata");
        assert_eq!(
            out["og"]["image"], "https://other.example/x.jpg",
            "absolute kept: {out}"
        );
    }

    // ─── links ───────────────────────────────────────────────────────

    #[test]
    fn links_resolve_dedupe_and_skip_non_http() {
        let out = with_base(PAGE, ExtractMode::Links);
        let links: Vec<&str> = out
            .as_array()
            .expect("array output")
            .iter()
            .map(|v| v.as_str().expect("string entries"))
            .collect();
        assert!(links.contains(&"https://example.com/relative"), "{links:?}");
        assert!(links.contains(&"https://other.example/abs"));
        assert!(links.contains(&"https://example.com/nav"));
        // mailto/javascript are not crawlable links.
        assert!(!links.iter().any(|l| l.starts_with("mailto:")));
        assert!(!links.iter().any(|l| l.starts_with("javascript:")));
        // Duplicates collapse (first occurrence wins).
        let unique: std::collections::BTreeSet<_> = links.iter().collect();
        assert_eq!(unique.len(), links.len(), "deduped: {links:?}");
    }

    #[test]
    fn links_without_base_keep_absolute_only() {
        let out = run(PAGE, ExtractMode::Links).expect("links");
        let links = out.as_array().expect("array");
        assert!(
            links
                .iter()
                .all(|l| l.as_str().is_some_and(|s| s.starts_with("http"))),
            "relative links are skipped without a base: {links:?}"
        );
    }

    #[test]
    fn base_href_overrides_the_fetch_url_for_relative_resolution() {
        // WHATWG: a <base href> is the document base for ALL relative URLs,
        // overriding the fetch URL. links + metadata(canonical) must honor it.
        let html = r#"<html><head>
            <base href="https://cdn.example/sub/">
            <link rel="canonical" href="canon">
        </head><body>
            <a href="page-one">one</a>
            <a href="/abs-path">two</a>
        </body></html>"#;
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://fetch.example/orig/doc.html");
        // links resolve against <base>, NOT the fetch URL.
        let links = extract(html, ExtractMode::Links, &opts).expect("links");
        let links = links.as_array().expect("array");
        assert!(
            links
                .iter()
                .any(|l| l.as_str() == Some("https://cdn.example/sub/page-one")),
            "relative link resolved against <base href>: {links:?}"
        );
        // Root-absolute path uses the <base>'s ORIGIN.
        assert!(
            links
                .iter()
                .any(|l| l.as_str() == Some("https://cdn.example/abs-path")),
            "root-absolute uses <base> origin: {links:?}"
        );
        assert!(
            !links
                .iter()
                .any(|l| l.as_str().is_some_and(|s| s.contains("fetch.example"))),
            "fetch URL must NOT be the base when <base href> is present: {links:?}"
        );
        // canonical too.
        let meta = extract(html, ExtractMode::Metadata, &opts).expect("metadata");
        assert_eq!(meta["canonical"], "https://cdn.example/sub/canon", "{meta}");
    }

    #[test]
    fn relative_base_href_resolves_against_fetch_url() {
        // A <base href> can itself be relative — it resolves against the
        // fetch URL first, then links resolve against THAT.
        let html = r#"<html><head><base href="/cdn/"></head>
            <body><a href="img.jpg">x</a></body></html>"#;
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://site.example/a/b/page.html");
        let links = extract(html, ExtractMode::Links, &opts).expect("links");
        assert!(
            links
                .as_array()
                .expect("array")
                .iter()
                .any(|l| l.as_str() == Some("https://site.example/cdn/img.jpg")),
            "relative <base> resolved against fetch URL: {links:?}"
        );
    }

    // ─── feed ────────────────────────────────────────────────────────

    const RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Demo Feed</title>
  <description>Feed description</description>
  <link>https://example.com</link>
  <item>
    <title>First post</title>
    <link>https://example.com/post-1</link>
    <description>Post summary</description>
    <pubDate>Mon, 01 Jun 2026 10:00:00 GMT</pubDate>
  </item>
</channel></rss>"#;

    const ATOM: &str = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Demo</title>
  <updated>2026-06-01T10:00:00Z</updated>
  <entry>
    <title>Atom entry</title>
    <link href="https://example.com/atom-1"/>
    <summary>Atom summary</summary>
  </entry>
</feed>"#;

    #[test]
    fn feed_normalizes_rss_and_atom() {
        let rss = run(RSS, ExtractMode::Feed).expect("rss parses");
        assert_eq!(rss["title"], "Demo Feed");
        assert_eq!(rss["description"], "Feed description");
        // feed-rs URL-normalizes the channel link (trailing slash).
        assert_eq!(rss["link"], "https://example.com/");
        assert_eq!(rss["items"][0]["title"], "First post");
        assert_eq!(rss["items"][0]["link"], "https://example.com/post-1");
        assert_eq!(rss["items"][0]["summary"], "Post summary");
        assert!(
            rss["items"][0]["published"]
                .as_str()
                .is_some_and(|d| d.starts_with("2026-06-01")),
            "{rss}"
        );

        let atom = run(ATOM, ExtractMode::Feed).expect("atom parses");
        assert_eq!(atom["title"], "Atom Demo");
        assert_eq!(atom["items"][0]["title"], "Atom entry");
        assert_eq!(atom["items"][0]["link"], "https://example.com/atom-1");
    }

    #[test]
    fn feed_rejects_non_feeds() {
        let err = run("not xml at all", ExtractMode::Feed).expect_err("not a feed");
        assert!(matches!(err, ExtractError::Feed { .. }), "{err}");
        let err = run(PAGE, ExtractMode::Feed).expect_err("html is not a feed");
        assert!(matches!(err, ExtractError::Feed { .. }), "{err}");
    }

    // ─── sitemap ─────────────────────────────────────────────────────

    const SITEMAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/</loc><lastmod>2026-05-20</lastmod></url>
  <url><loc>https://example.com/about</loc></url>
</urlset>"#;

    const SITEMAP_INDEX: &str = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://example.com/sitemap-1.xml</loc><lastmod>2026-05-01</lastmod></sitemap>
  <sitemap><loc>https://example.com/sitemap-2.xml</loc></sitemap>
</sitemapindex>"#;

    #[test]
    fn sitemap_parses_urlset_and_index() {
        let out = run(SITEMAP, ExtractMode::Sitemap).expect("urlset");
        assert_eq!(out[0]["loc"], "https://example.com/");
        assert_eq!(out[0]["lastmod"], "2026-05-20");
        assert_eq!(out[1]["loc"], "https://example.com/about");
        assert!(out[1].get("lastmod").is_none(), "lastmod omitted: {out}");

        let idx = run(SITEMAP_INDEX, ExtractMode::Sitemap).expect("index");
        assert_eq!(idx[0]["loc"], "https://example.com/sitemap-1.xml");
        assert_eq!(idx[1]["loc"], "https://example.com/sitemap-2.xml");
    }

    #[test]
    fn sitemap_locs_survive_entities_and_cdata() {
        // sitemaps.org MANDATES entity-escaping — `&` in query strings is
        // the COMMON case. quick-xml 0.40 emits `&amp;` as a separate
        // GeneralRef event between two Text events: the handler must
        // ACCUMULATE, never assign (review lens 2 · P1).
        let xml = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://e.com/search?q=rust&amp;page=2&amp;lang=fr</loc></url>
  <url><loc><![CDATA[https://e.com/cdata?a=1&b=2]]></loc></url>
  <url><loc>https://e.com/mixed&#47;path</loc></url>
</urlset>"#;
        let out = run(xml, ExtractMode::Sitemap).expect("urlset");
        let entries = out.as_array().expect("array");
        assert_eq!(
            entries[0]["loc"], "https://e.com/search?q=rust&page=2&lang=fr",
            "entity-split text must reassemble: {entries:?}"
        );
        assert_eq!(
            entries[1]["loc"], "https://e.com/cdata?a=1&b=2",
            "CDATA loc must be captured: {entries:?}"
        );
        assert_eq!(
            entries[2]["loc"], "https://e.com/mixed/path",
            "numeric char refs resolve: {entries:?}"
        );
    }

    #[test]
    fn sitemap_carries_changefreq_and_priority() {
        let xml = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://e.com/</loc><changefreq>daily</changefreq><priority>0.8</priority></url>
</urlset>"#;
        let out = run(xml, ExtractMode::Sitemap).expect("urlset");
        assert_eq!(out[0]["loc"], "https://e.com/");
        assert_eq!(out[0]["changefreq"], "daily");
        assert_eq!(out[0]["priority"], "0.8");
    }

    #[test]
    fn feed_items_carry_id_and_categories() {
        let rss = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>F</title>
  <item><title>P</title><guid>post-guid-1</guid>
    <category>rust</category><category>web</category></item>
</channel></rss>"#;
        let out = run(rss, ExtractMode::Feed).expect("rss");
        assert_eq!(out["items"][0]["id"], "post-guid-1");
        assert_eq!(
            out["items"][0]["categories"],
            serde_json::json!(["rust", "web"])
        );
    }

    #[test]
    fn sitemap_ignores_entries_outside_the_root_and_text_after_loc_close() {
        // An entry BEFORE the root element does not count (the
        // `saw_root` guard); junk text after `</loc>` must not
        // overwrite the captured loc (the field-clearing End arm).
        let xml = r#"<?xml version="1.0"?>
<url><loc>https://outside.example/</loc></url>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/real</loc>junk<lastmod>2026-01-01</lastmod></url>
</urlset>"#;
        let out = run(xml, ExtractMode::Sitemap).expect("urlset parses");
        let entries = out.as_array().expect("array");
        assert_eq!(entries.len(), 1, "pre-root entry ignored: {entries:?}");
        assert_eq!(entries[0]["loc"], "https://example.com/real");
        assert_eq!(entries[0]["lastmod"], "2026-01-01");
    }

    #[test]
    fn sitemap_rejects_non_sitemaps() {
        let err = run("plain text", ExtractMode::Sitemap).expect_err("not xml");
        assert!(matches!(err, ExtractError::Sitemap { .. }), "{err}");
        let err = run(PAGE, ExtractMode::Sitemap).expect_err("html has no urlset");
        assert!(matches!(err, ExtractError::Sitemap { .. }), "{err}");
    }

    // ─── article ─────────────────────────────────────────────────────

    #[test]
    fn article_falls_back_to_boilerpipe_on_div_soup() {
        // No <p>, no semantic classes, prose drowning in link rows —
        // the readability stage starves here; the shallow-feature
        // fallback (Boilerpipe Alg. 2 · WSDM'10) recovers the prose.
        let prose = "This page hides genuinely long running prose inside an \
                     anonymous division with no paragraph markup whatsoever, \
                     which is exactly the structural shape where class-signal \
                     and paragraph-based extractors traditionally starve and \
                     return nothing of substance at all. The shallow text \
                     features still see a long low-link-density block here.";
        let html = format!(
            r#"<html><body>
            <div><a href="/1">Home</a> <a href="/2">Shop</a> <a href="/3">Blog</a></div>
            <div>{prose}</div>
            <div>{prose}</div>
            <div><a href="/l">Legal</a> <a href="/p">Privacy</a></div>
            </body></html>"#
        );
        let out = run(&html, ExtractMode::Article).expect("article never dies on div soup");
        let md = out.as_str().expect("string");
        assert!(
            md.contains("genuinely long running prose"),
            "prose recovered: {md}"
        );
        assert!(!md.contains("Privacy"), "link rows stay dead: {md}");
    }

    #[test]
    fn article_extracts_the_main_body_as_markdown() {
        // Readability needs enough content mass to identify an article.
        let long = format!(
            r#"<html><head><title>Story</title></head><body>
            <nav><a href="/x">site nav</a></nav>
            <article><h1>The Story Headline</h1>{}</article>
            <footer>site footer</footer></body></html>"#,
            "<p>A sentence of real article prose that carries actual content mass for readability scoring.</p>".repeat(12)
        );
        let out = run(&long, ExtractMode::Article).expect("article extracts");
        let md = out.as_str().expect("string output");
        assert!(md.contains("article prose"), "body survives: {md}");
        assert!(!md.contains("site nav"), "chrome stripped: {md}");
    }

    #[test]
    fn article_rule_cascade_strips_within_zone_boilerplate() {
        // The Trafilatura-grade STAGE 1 win: boilerplate INSIDE the semantic
        // <article> (related-posts, share buttons, a comments section) — the
        // stuff readability often leaves — is pruned by the zone cascade.
        let prose = "<p>A full sentence of genuine article prose that carries real \
                     content mass for the reader who came to this page today.</p>";
        let page = format!(
            r#"<html><head><title>Story</title></head><body>
            <nav><a href="/x">site nav links here</a></nav>
            <article class="entry-content">
              <h1>The Real Story Headline</h1>
              {prose}{prose}{prose}{prose}
              <aside class="related-posts"><a href="/r1">Related story one</a>
                 <a href="/r2">Related story two</a></aside>
              <div class="share-bar"><a href="/tw">Tweet this</a> <a href="/fb">Share this</a></div>
              <section class="comments"><h3>Comments</h3>
                <div class="comment"><p>first spammy comment text</p></div></section>
            </article>
            <footer>site footer junk</footer></body></html>"#
        );
        let mut opts = ExtractOptions::new();
        opts.base_url = Some("https://blog.example/2026/06/the-real-story");
        let out = extract(&page, ExtractMode::Article, &opts).expect("article extracts");
        let md = out.as_str().expect("string");
        assert!(md.contains("genuine article prose"), "body survives: {md}");
        assert!(
            md.contains("Real Story Headline"),
            "headline survives: {md}"
        );
        // Within-zone boilerplate gone (what the rule cascade adds over plain readability).
        assert!(!md.contains("site nav"), "nav stripped: {md}");
        assert!(!md.contains("Related story"), "related pruned: {md}");
        assert!(!md.contains("Tweet this"), "share pruned: {md}");
        assert!(!md.contains("spammy comment"), "comments pruned: {md}");
        assert!(!md.contains("footer junk"), "footer stripped: {md}");
    }

    // ─── jq + unsupported ────────────────────────────────────────────

    #[test]
    fn jq_routes_upstream() {
        let err = run("{}", ExtractMode::Jq).expect_err("jq is the builtin's job");
        assert!(matches!(err, ExtractError::Unsupported { .. }));
        assert!(err.to_string().contains("one data language"), "{err}");
    }

    // ─── totality (Gate 6) ───────────────────────────────────────────

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            /// Every mode is TOTAL over arbitrary input: Ok or typed Err,
            /// never a panic, never a hang. The mode range is bound to
            /// `ExtractMode::ALL.len()` (not a hardcoded count) so a mode
            /// added to the vocabulary can never silently escape the fuzz;
            /// the `(?s)` dotall flag lets `.` match newlines, so multi-line
            /// HTML/XML/feed bodies (the real shape) are exercised too.
            #[test]
            fn extract_never_panics(body in "(?s).{0,2000}", mode_idx in 0usize..ExtractMode::ALL.len()) {
                let mode = ExtractMode::ALL[mode_idx];
                let mut opts = ExtractOptions::new();
                opts.selector = Some("p");
                opts.base_url = Some("https://example.com/");
                let _ = extract(&body, mode, &opts);
            }

            /// Link extraction never panics on hostile href/base pairs
            /// (newlines included — a `base_url` carrying a `\n` is fair game).
            #[test]
            fn links_total_over_hostile_bases(body in "(?s).{0,500}", base in "(?s).{0,80}") {
                let mut opts = ExtractOptions::new();
                opts.base_url = Some(&base);
                let _ = extract(&body, ExtractMode::Links, &opts);
            }
        }
    }

    /// The fetch-pipeline rehearsal: the PAGE served over a REAL socket,
    /// fetched by the production `nika-http` client, extracted by this
    /// crate — the exact composition the `nika:fetch` builtin performs
    /// (step 13). `SsrfMode::Disabled`: the target IS loopback; SSRF has
    /// its own suite in `nika-http`.
    mod e2e {
        use super::*;
        use nika_http::{HttpConfig, ReqwestHttp, SsrfMode};
        use nika_kernel::http::{HttpGetDyn, HttpRequest};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn serve_page(body: &'static str) -> std::net::SocketAddr {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind loopback");
            let addr = listener.local_addr().expect("addr");
            tokio::spawn(async move {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
            addr
        }

        /// Serve an OWNED body (built at runtime, e.g. a depth bomb) with a
        /// chosen `Content-Type`.
        async fn serve_owned(body: Vec<u8>, content_type: &'static str) -> std::net::SocketAddr {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind loopback");
            let addr = listener.local_addr().expect("addr");
            tokio::spawn(async move {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let mut response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                response.extend_from_slice(&body);
                let _ = socket.write_all(&response).await;
                let _ = socket.shutdown().await;
            });
            addr
        }

        #[tokio::test]
        async fn fetch_then_extract_over_a_real_socket() {
            let addr = serve_page(PAGE).await;
            let mut config = HttpConfig::new();
            config.ssrf = SsrfMode::Disabled;
            let client = ReqwestHttp::with_config(config).expect("client");

            let url = format!("http://{addr}/article");
            let response = client
                .get(HttpRequest::get(url.clone()))
                .await
                .expect("fetch succeeds");
            assert_eq!(response.status, 200);
            let body = std::str::from_utf8(&response.body).expect("utf-8 page");

            // markdown — the spec default the builtin will apply.
            let mut opts = ExtractOptions::new();
            opts.base_url = Some(response.final_url.as_str());
            let md = extract(body, ExtractMode::Markdown, &opts).expect("markdown");
            assert!(md.as_str().is_some_and(|m| m.contains("# Demo Heading")));

            // links resolve against the LIVE final_url.
            let links = extract(body, ExtractMode::Links, &opts).expect("links");
            let links = links.as_array().expect("array");
            assert!(
                links
                    .iter()
                    .any(|l| l.as_str() == Some(format!("http://{addr}/relative").as_str())),
                "relative href resolved against the live origin: {links:?}"
            );

            // metadata canonical resolves too.
            let meta = extract(body, ExtractMode::Metadata, &opts).expect("metadata");
            assert_eq!(meta["canonical"], format!("http://{addr}/article"));
        }

        #[tokio::test]
        async fn guard_rejects_foreign_bomb_over_a_real_socket() {
            // The verified SVG foreign-content bypass, served by a real
            // socket and pulled through the real reqwest client: extraction
            // must REJECT it via the depth guard, never SIGABRT the process
            // through htmd's recursive rcdom Drop. `markdown` is the default
            // nika:fetch mode — the exact path that crashed pre-fix.
            let bomb = format!("<svg><title>{}", "<g>".repeat(50_000)).into_bytes();
            let addr = serve_owned(bomb, "text/html; charset=utf-8").await;
            let mut config = HttpConfig::new();
            config.ssrf = SsrfMode::Disabled;
            let client = ReqwestHttp::with_config(config).expect("client");

            let response = client
                .get(HttpRequest::get(format!("http://{addr}/bomb")))
                .await
                .expect("fetch succeeds");
            let body = std::str::from_utf8(&response.body).expect("utf-8 page");
            let opts = ExtractOptions::new();
            assert!(
                extract(body, ExtractMode::Markdown, &opts).is_err(),
                "foreign-content bomb must be rejected by the guard, not crash"
            );
        }
    }
}
