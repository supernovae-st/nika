// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Page-type classification — a deterministic, heuristic (NOT ML) signal
//! that lets `mode: article` adapt its extraction per page type. The WCXB
//! benchmark (arXiv:2605.21097) shows article-tuned extractors FAIL on
//! non-article pages: a forum's posts wrapped in `.comment`/`.reply` get
//! stripped as boilerplate, products' descriptions live in structured
//! data, listings have no single "article" body. Knowing the type lets the
//! rule cascade keep the right zones and prune the right boilerplate.
//!
//! Sovereign by construction: URL substrings + DOM/schema signals only —
//! no model file, no language resource, no I/O (pure, like the rest of the
//! crate). A model-based classifier (rs-trafilatura's `XGBoost`) is
//! deliberately NOT carried: it would break the L1.5 zero-I/O purity and
//! the no-heavy-deps Diamond rule for a few F1 points the heuristics
//! already capture.

use std::sync::LazyLock;

use scraper::{Html, Selector};

/// The closed page-type set (WCXB's 7 collapsed to the 6 that change
/// extraction behavior — `error` pages are not a content type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageType {
    /// News / blog / docs — a single main prose body. The default target.
    Article,
    /// Product / item detail — the useful data is structured (JSON-LD /
    /// microdata, surfaced by `mode: metadata`); the prose is a description.
    Product,
    /// Forum / Q&A / discussion — the CONTENT is the posts/replies, so
    /// `.comment`/`.reply`/`.post` must NOT be pruned (the key WCXB fix).
    Forum,
    /// Listing / category / collection — a grid of links, no single body.
    Listing,
    /// Search-results page — query + ranked links, no single body.
    Search,
    /// Unknown — treat as Article (the safe default target).
    Generic,
}

impl PageType {
    /// Whether discussion zones (`.comment`/`.reply`/`.post`) are CONTENT
    /// for this type (forums) vs boilerplate to prune (everything else).
    pub(crate) fn comments_are_content(self) -> bool {
        matches!(self, PageType::Forum)
    }
}

/// Classify a page from its URL + DOM signals. URL is checked first (cheap,
/// no parse needed for a confident hit); DOM signals refine/confirm.
pub(crate) fn classify(body: &str, url: Option<&str>) -> PageType {
    if let Some(u) = url
        && let Some(t) = classify_url(u)
    {
        return t;
    }
    classify_dom(body)
}

/// URL-path signals (checked on the lowercased path+query). Ordered by
/// specificity — the first confident match wins. `None` = inconclusive,
/// fall through to DOM signals.
fn classify_url(url: &str) -> Option<PageType> {
    // Path+query only (the host rarely carries type signal and adds noise),
    // KEEPING the leading `/` so `/product`-style patterns match.
    let lower = url.to_ascii_lowercase();
    let after_scheme = lower.split_once("://").map_or(lower.as_str(), |(_, r)| r);
    // Tail = the path+query (drop the host). Start at the FIRST `/` OR `?` —
    // a query can ride directly on the host (`example.com?q=…`, no path), and
    // missing it dropped the type signal (a bare-host search → misclassified).
    let tail = after_scheme
        .find(['/', '?'])
        .map_or("/", |i| &after_scheme[i..]);

    // Search — distinctive query keys.
    if tail.contains("/search")
        || tail.contains("?q=")
        || tail.contains("&q=")
        || tail.contains("?query=")
        || tail.contains("?s=")
    {
        return Some(PageType::Search);
    }
    // Forum / discussion. (`/questions/` not bare `/q/` — the latter
    // over-matches quote/quarter/search endpoints for a behavior flip.)
    if [
        "/forum",
        "/thread",
        "/topic",
        "/discussion",
        "/discussions/",
        "/comments/",
        "/viewtopic",
        "/showthread",
        "/questions/",
        "/t/",
    ]
    .iter()
    .any(|p| tail.contains(p))
    {
        return Some(PageType::Forum);
    }
    // Product / item detail.
    if [
        "/product",
        "/products/",
        "/item/",
        "/p/",
        "/dp/",
        "/sku",
        "/listing/",
    ]
    .iter()
    .any(|p| tail.contains(p))
    {
        return Some(PageType::Product);
    }
    // Listing / collection / taxonomy.
    if [
        "/category",
        "/categories/",
        "/tag/",
        "/tags/",
        "/collections/",
        "/shop",
        "/browse",
    ]
    .iter()
    .any(|p| tail.contains(p))
    {
        return Some(PageType::Listing);
    }
    // Article — explicit blog/news/dated paths (a positive signal, not just
    // the default) so a DOM with weak markup still classifies right.
    if ["/article", "/blog/", "/news/", "/posts/", "/story/"]
        .iter()
        .any(|p| tail.contains(p))
        || dated_path(tail)
    {
        return Some(PageType::Article);
    }
    None
}

/// A `/YYYY/MM/`-style dated path segment — the near-universal blog/news
/// permalink shape (`/2026/06/headline`). Strong Article signal.
fn dated_path(tail: &str) -> bool {
    let mut segs = tail.split('/').filter(|s| !s.is_empty());
    while let Some(s) = segs.next() {
        if s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit()) {
            // Year segment followed by a valid 1-2 digit month (01-12) —
            // bounding the month keeps this a STRONG signal (a bare 4-digit
            // run + any number isn't a date).
            if let Some(next) = segs.next()
                && (1..=2).contains(&next.len())
                && next.parse::<u8>().is_ok_and(|m| (1..=12).contains(&m))
            {
                return true;
            }
        }
    }
    false
}

static OG_TYPE: LazyLock<Option<Selector>> =
    LazyLock::new(|| Selector::parse(r#"meta[property="og:type"]"#).ok());
static SCHEMA_PRODUCT: LazyLock<Option<Selector>> =
    LazyLock::new(|| Selector::parse(r#"[itemtype*="schema.org/Product" i]"#).ok());
static SCHEMA_DISCUSSION: LazyLock<Option<Selector>> = LazyLock::new(|| {
    Selector::parse(
        r#"[itemtype*="schema.org/DiscussionForumPosting" i], [itemtype*="schema.org/QAPage" i]"#,
    )
    .ok()
});
static SEARCH_FORM: LazyLock<Option<Selector>> =
    LazyLock::new(|| Selector::parse(r#"form[role="search"], [role="search"]"#).ok());
static SCHEMA_ARTICLE: LazyLock<Option<Selector>> = LazyLock::new(|| {
    Selector::parse(r#"[itemtype*="schema.org/Article" i], [itemtype*="schema.org/BlogPosting" i]"#)
        .ok()
});

/// DOM/schema signals — schema.org `itemtype`, `og:type`, structural
/// markers. Used when the URL is inconclusive (the common case for a clean
/// `/path` permalink).
fn classify_dom(body: &str) -> PageType {
    let doc = Html::parse_document(body);

    // og:type is the publisher's own declaration — trust it first.
    if let Some(sel) = OG_TYPE.as_ref()
        && let Some(t) = doc
            .select(sel)
            .next()
            .and_then(|el| el.value().attr("content"))
    {
        let t = t.to_ascii_lowercase();
        if t == "product" || t.starts_with("product.") {
            return PageType::Product;
        }
        if t == "article" {
            return PageType::Article;
        }
    }

    // schema.org microdata itemtype — precise structural type.
    if matches(&doc, &SCHEMA_DISCUSSION) {
        return PageType::Forum;
    }
    if matches(&doc, &SCHEMA_PRODUCT) {
        return PageType::Product;
    }
    if matches(&doc, &SCHEMA_ARTICLE) {
        return PageType::Article;
    }
    // A prominent search form with no other signal → a search surface.
    if matches(&doc, &SEARCH_FORM) {
        return PageType::Search;
    }
    PageType::Generic
}

fn matches(doc: &Html, sel: &LazyLock<Option<Selector>>) -> bool {
    sel.as_ref().is_some_and(|s| doc.select(s).next().is_some())
}

#[cfg(test)]
mod tests {
    use super::{PageType, classify, classify_url};

    fn t(url: &str) -> PageType {
        classify("<html><body></body></html>", Some(url))
    }

    #[test]
    fn url_signals_route_each_type() {
        assert_eq!(t("https://shop.x/product/widget-123"), PageType::Product);
        assert_eq!(t("https://x.com/dp/B0123"), PageType::Product);
        assert_eq!(t("https://forum.x/thread/4567"), PageType::Forum);
        assert_eq!(t("https://x.com/t/how-do-i-915"), PageType::Forum);
        assert_eq!(t("https://x.com/category/boots"), PageType::Listing);
        assert_eq!(t("https://x.com/tag/rust"), PageType::Listing);
        assert_eq!(t("https://x.com/search?q=boots"), PageType::Search);
        assert_eq!(t("https://x.com/2026/06/my-headline"), PageType::Article);
        assert_eq!(t("https://blog.x/blog/why-rust"), PageType::Article);
    }

    #[test]
    fn url_inconclusive_falls_through_to_dom() {
        // A bare slug URL → DOM decides.
        let product = classify(
            r#"<html><head><meta property="og:type" content="product"></head><body></body></html>"#,
            Some("https://x.com/widget"),
        );
        assert_eq!(product, PageType::Product);

        let forum = classify(
            r#"<html><body><div itemscope itemtype="https://schema.org/QAPage"></div></body></html>"#,
            Some("https://x.com/how-to"),
        );
        assert_eq!(forum, PageType::Forum);

        // No signal anywhere → Generic (safe default).
        assert_eq!(
            classify(
                "<html><body><p>hi</p></body></html>",
                Some("https://x.com/x")
            ),
            PageType::Generic
        );
    }

    #[test]
    fn no_url_uses_dom_only() {
        let article = classify(
            r#"<html><head><meta property="og:type" content="article"></head><body></body></html>"#,
            None,
        );
        assert_eq!(article, PageType::Article);
    }

    #[test]
    fn comments_are_content_only_for_forums() {
        assert!(PageType::Forum.comments_are_content());
        assert!(!PageType::Article.comments_are_content());
        assert!(!PageType::Product.comments_are_content());
        assert!(!PageType::Generic.comments_are_content());
    }

    /// Each search-query key classifies as Search ON ITS OWN — the keys are
    /// OR-ed, so flipping any `||` to `&&` (which would demand every key at
    /// once) is caught by a URL carrying exactly one key.
    #[test]
    fn each_search_key_alone_is_search() {
        for url in [
            "https://x.com/search",
            "https://x.com/p?q=rust",
            "https://x.com/p?a=1&q=rust",
            "https://x.com/p?query=rust",
            "https://x.com/p?s=rust",
        ] {
            assert_eq!(classify_url(url), Some(PageType::Search), "{url}");
        }
    }

    /// A query riding directly on the host (no path) still classifies — the
    /// tail starts at `/` OR `?` (regression for the host-only-query gap).
    #[test]
    fn host_only_query_string_is_search() {
        assert_eq!(
            classify_url("https://search.example.com?q=rust"),
            Some(PageType::Search)
        );
    }

    /// `dated_path` needs a 4-digit year AND a 1–2 digit month in 01–12 — both
    /// conjuncts matter, so a year+non-month and a year+out-of-range-month must
    /// NOT classify as Article (flipping either `&&` to `||` would accept them).
    #[test]
    fn dated_path_requires_year_and_valid_month() {
        assert_eq!(
            classify_url("https://x.com/2026/06/headline"),
            Some(PageType::Article),
            "a real /YYYY/MM/ permalink is an article"
        );
        assert_eq!(
            classify_url("https://x.com/2026/headline"),
            None,
            "year + non-numeric next segment is not dated"
        );
        assert_eq!(
            classify_url("https://x.com/2026/99/x"),
            None,
            "month out of 01–12 range is not dated"
        );
        assert_eq!(
            classify_url("https://x.com/abcd/06/x"),
            None,
            "a 4-char NON-numeric segment is not a year (the year is digits AND len 4)"
        );
    }
}
