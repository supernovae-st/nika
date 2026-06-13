// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: metadata` (head walk · title·description·og·twitter·
//! `canonical·lang·author·published_time·favicon·alternates·jsonld`) and
//! `mode: links` (every `<a href>` → absolute URL).
//!
//! Both are TOTAL: malformed HTML yields what the parser salvages
//! (html5ever error-recovers per the WHATWG spec) — honest emptiness,
//! never a failure. The static selectors are parsed ONCE
//! (`LazyLock` — `Selector` is `Send + Sync`); the impossible
//! parse-failure branch fails SOFT (the selector is skipped).

use std::sync::LazyLock;

use scraper::{Html, Selector};

/// A `LazyLock` over an infallible-by-construction static selector.
/// The `Option` carries the (unreachable) parse-failure branch without
/// an `expect` — consumers skip a `None` selector.
fn parse_static(css: &'static str) -> Option<Selector> {
    Selector::parse(css).ok()
}

static TITLE: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("head > title"));
static META: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("meta"));
static CANONICAL: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static(r#"link[rel="canonical"]"#));
// `rel~=` is the CSS word-match: catches `icon` AND `shortcut icon`.
static ICON: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static(r#"link[rel~="icon"]"#));
static HTML_TAG: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("html"));
static ANCHORS: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("a[href]"));
static BASE_HREF: LazyLock<Option<Selector>> = LazyLock::new(|| parse_static("base[href]"));
static JSONLD: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static(r#"script[type="application/ld+json"]"#));
// TOP-LEVEL microdata items only: `itemscope` WITHOUT `itemprop` (an
// `itemscope itemprop` element is a PROPERTY of its parent item, surfaced
// via nesting, never as a standalone top-level item · W3C Microdata
// §"top-level microdata items").
static MICRODATA_ITEMS: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static("[itemscope]:not([itemprop])"));

/// Cap on JSON-LD blocks surfaced — a page with thousands of
/// `<script type=ld+json>` is pathological; the real web has 1–3
/// (`Product` · `BreadcrumbList` · `Organization`). Bounds the output.
const MAX_JSONLD_BLOCKS: usize = 32;

/// Cap on JSON-LD blocks ATTEMPTED (parsed) — bounds the work when a
/// hostile page floods malformed `ld+json` scripts (each parse is
/// already body-cap-bounded; this bounds their COUNT).
const MAX_JSONLD_ATTEMPTS: usize = 256;

/// Cap on TOP-LEVEL microdata items surfaced — like [`MAX_JSONLD_BLOCKS`],
/// a page with thousands of `itemscope` roots is pathological.
const MAX_MICRODATA_ITEMS: usize = 64;

/// Max NESTED-item depth (`itemscope` inside an `itemprop` value). The
/// pre-parse depth guard already caps DOM nesting at 2048; this is a
/// tighter, defense-in-depth bound on the recursive item walk (real
/// schema.org nests 2–3 deep · `Product` → `offers` → `priceSpecification`).
const MAX_MICRODATA_DEPTH: usize = 16;

/// Fold one `<meta>` tag into the metadata maps — `og:*`/`twitter:*`
/// prefixes route to their objects · the scalar names (`description` ·
/// `author` · `article:published_time`) land at the top level ·
/// first-wins (`or_insert_with` · the page's head order is canonical).
fn fold_meta_tag(
    el: &scraper::ElementRef<'_>,
    out: &mut serde_json::Map<String, serde_json::Value>,
    og: &mut serde_json::Map<String, serde_json::Value>,
    twitter: &mut serde_json::Map<String, serde_json::Value>,
) {
    let Some(content) = el.value().attr("content") else {
        return;
    };
    if let Some(property) = el.value().attr("property") {
        if let Some(key) = property.strip_prefix("og:")
            && !key.is_empty()
        {
            og.entry(key.to_owned()).or_insert_with(|| content.into());
        } else if property.eq_ignore_ascii_case("article:published_time") {
            out.entry("published_time".to_owned())
                .or_insert_with(|| content.into());
        }
    }
    if let Some(name) = el.value().attr("name") {
        if let Some(key) = name.strip_prefix("twitter:")
            && !key.is_empty()
        {
            twitter
                .entry(key.to_owned())
                .or_insert_with(|| content.into());
        } else if name.eq_ignore_ascii_case("description") {
            out.entry("description".to_owned())
                .or_insert_with(|| content.into());
        } else if name.eq_ignore_ascii_case("author") {
            out.entry("author".to_owned())
                .or_insert_with(|| content.into());
        }
    }
}

/// `mode: metadata` — the spec's documented object shape. `og`/`twitter`
/// are ALWAYS objects (stable shape for jq consumers); scalar keys are
/// omitted when absent (JSON absence over null).
pub(crate) fn metadata(body: &str, base: Option<&str>) -> serde_json::Value {
    let doc = Html::parse_document(body);
    // Honor `<base href>`: canonical/favicon/microdata URLs resolve against
    // the document's effective base, not the fetch URL (WHATWG).
    let base = effective_base(&doc, base);
    let base = base.as_deref();
    let mut out = serde_json::Map::new();
    let mut og = serde_json::Map::new();
    let mut twitter = serde_json::Map::new();

    if let Some(sel) = TITLE.as_ref()
        && let Some(el) = doc.select(sel).next()
    {
        let title = el.text().collect::<String>().trim().to_owned();
        if !title.is_empty() {
            out.insert("title".to_owned(), title.into());
        }
    }

    if let Some(sel) = META.as_ref() {
        for el in doc.select(sel) {
            fold_meta_tag(&el, &mut out, &mut og, &mut twitter);
        }
    }

    if let Some(sel) = CANONICAL.as_ref()
        && let Some(href) = doc
            .select(sel)
            .next()
            .and_then(|el| el.value().attr("href"))
        && let Some(resolved) = resolve(href, base)
    {
        out.insert("canonical".to_owned(), resolved.into());
    }

    if let Some(sel) = ICON.as_ref()
        && let Some(href) = doc
            .select(sel)
            .next()
            .and_then(|el| el.value().attr("href"))
        && let Some(resolved) = resolve(href, base)
    {
        out.insert("favicon".to_owned(), resolved.into());
    }

    if let Some(sel) = HTML_TAG.as_ref()
        && let Some(lang) = doc
            .select(sel)
            .next()
            .and_then(|el| el.value().attr("lang"))
        && !lang.is_empty()
    {
        out.insert("lang".to_owned(), lang.into());
    }

    // schema.org structured data (JSON-LD) — the highest-yield path for
    // NON-article pages (products · listings · recipes), where the
    // useful fields live in `<script type=ld+json>` not the visible DOM
    // (WCXB arXiv:2605.21097 · +20-30 F1 on product pages). Surfaced as
    // `jsonld: [...]` (the parsed blocks, schema-agnostic — the caller's
    // jq/infer step walks `@type`). Invalid blocks are skipped, never
    // fatal: a malformed inline script must not sink the metadata.
    if let Some(sel) = JSONLD.as_ref() {
        let blocks: Vec<serde_json::Value> = doc
            .select(sel)
            // ATTEMPT cap before the VALID cap: `take(MAX_JSONLD_BLOCKS)`
            // alone would parse every block until 32 succeed — a page of
            // 10 000 malformed ld+json scripts would parse all 10 000.
            // Bound the parse attempts first (a real page has 1–3 blocks;
            // the attempt window is generous but finite).
            .take(MAX_JSONLD_ATTEMPTS)
            .filter_map(|el| {
                let raw = el.text().collect::<String>();
                serde_json::from_str::<serde_json::Value>(raw.trim()).ok()
            })
            .take(MAX_JSONLD_BLOCKS)
            .collect();
        if !blocks.is_empty() {
            out.insert("jsonld".to_owned(), serde_json::Value::Array(blocks));
        }
    }

    // schema.org MICRODATA (`itemscope`/`itemprop`/`itemtype`) — the OTHER
    // embedded-structured-data carrier (26% of pages · HTTP Archive Web
    // Almanac 2024 · legacy + recipe-heavy, where JSON-LD is absent).
    // Surfaced as `microdata: [{type, id?, properties}]` — the W3C item
    // model, schema-agnostic like `jsonld[]` (the caller's jq walks it).
    if let Some(sel) = MICRODATA_ITEMS.as_ref() {
        let items: Vec<serde_json::Value> = doc
            .select(sel)
            .take(MAX_MICRODATA_ITEMS)
            .map(|item| microdata_item(item, base, 0))
            .collect();
        if !items.is_empty() {
            out.insert("microdata".to_owned(), serde_json::Value::Array(items));
        }
    }

    // Fallback: a page with a missing/empty `<title>` or no
    // `<meta name=description>` (common on SPAs) borrows from `og:` then
    // `twitter:` — the standard extractor behavior (Mercury/readability).
    // Only fills an ABSENT key, so an explicit `<title>` always wins.
    for key in ["title", "description"] {
        if !out.contains_key(key)
            && let Some(v) = og.get(key).or_else(|| twitter.get(key))
        {
            out.insert(key.to_owned(), v.clone());
        }
    }

    out.insert("og".to_owned(), serde_json::Value::Object(og));
    out.insert("twitter".to_owned(), serde_json::Value::Object(twitter));
    serde_json::Value::Object(out)
}

/// One microdata item → `{ type: [...]?, id: "..."?, properties: { name:
/// [values] } }` (W3C HTML Microdata item model). `properties` values are
/// ALWAYS arrays (a property may repeat · stable shape for jq consumers).
fn microdata_item(
    item: scraper::ElementRef<'_>,
    base: Option<&str>,
    depth: usize,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(itemtype) = item.value().attr("itemtype") {
        let types: Vec<serde_json::Value> = itemtype.split_whitespace().map(Into::into).collect();
        if !types.is_empty() {
            obj.insert("type".to_owned(), serde_json::Value::Array(types));
        }
    }
    if let Some(id) = item.value().attr("itemid").filter(|s| !s.is_empty()) {
        obj.insert("id".to_owned(), id.into());
    }

    let mut properties = serde_json::Map::new();
    // Past the nesting cap, surface the item's identity but stop recursing
    // into its properties (bounded work · the cap is far below real pages).
    if depth < MAX_MICRODATA_DEPTH {
        for prop_el in property_elements(item) {
            let Some(names) = prop_el.value().attr("itemprop") else {
                continue;
            };
            let value = microdata_value(prop_el, base, depth);
            for name in names.split_whitespace() {
                if let Some(arr) = properties
                    .entry(name.to_owned())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                    .as_array_mut()
                {
                    arr.push(value.clone());
                }
            }
        }
    }
    obj.insert(
        "properties".to_owned(),
        serde_json::Value::Object(properties),
    );
    serde_json::Value::Object(obj)
}

/// The property elements of an item (W3C "the properties of an item"):
/// descendants carrying `itemprop`, NOT crossing into a nested `itemscope`
/// (whose own properties belong to IT). Iterative worklist — a hostile DOM
/// depth must never become stack depth (same discipline as `html::text`).
fn property_elements(item: scraper::ElementRef<'_>) -> Vec<scraper::ElementRef<'_>> {
    let mut results = Vec::new();
    // Seed with the item's direct children (NOT the item itself).
    let mut pending: Vec<scraper::ElementRef<'_>> = item
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .collect();
    let mut i = 0;
    while i < pending.len() {
        let el = pending[i];
        i += 1;
        let is_scope = el.value().attr("itemscope").is_some();
        if el.value().attr("itemprop").is_some() {
            results.push(el);
        }
        // Descend ONLY through non-item elements: an `itemscope` boundary
        // ends this item's property region (its children are its own).
        if !is_scope {
            pending.extend(el.children().filter_map(scraper::ElementRef::wrap));
        }
    }
    results
}

/// One property's value (W3C "property value"): a nested item when the
/// element is itself an `itemscope`, else the element-type-specific value
/// (URL attrs resolved against `base` · `meta@content` · `time@datetime`
/// · `data`/`meter@value` · else the text content).
fn microdata_value(
    el: scraper::ElementRef<'_>,
    base: Option<&str>,
    depth: usize,
) -> serde_json::Value {
    if el.value().attr("itemscope").is_some() {
        return microdata_item(el, base, depth + 1);
    }
    let attr = el.value();
    let text = || el.text().collect::<String>().trim().to_owned();
    let url = |raw: Option<&str>| {
        raw.map(|r| resolve(r, base).unwrap_or_else(|| r.to_owned()))
            .unwrap_or_default()
    };
    let value = match attr.name() {
        "meta" => attr.attr("content").unwrap_or("").to_owned(),
        "audio" | "embed" | "iframe" | "img" | "source" | "track" | "video" => {
            url(attr.attr("src"))
        }
        "a" | "area" | "link" => url(attr.attr("href")),
        "object" => url(attr.attr("data")),
        "data" | "meter" => attr.attr("value").unwrap_or("").to_owned(),
        // `time@datetime` is the machine value; fall back to the text it
        // wraps (`<time>3 days ago</time>` with no datetime attr).
        "time" => attr.attr("datetime").map_or_else(text, str::to_owned),
        _ => text(),
    };
    serde_json::Value::String(value)
}

/// `mode: links` — document-order `<a href>` targets, resolved against
/// `base`, http(s) only (mailto/javascript/tel are not crawlable),
/// deduplicated first-occurrence-wins. Without a base, relative hrefs
/// are skipped (nothing sound to resolve them against).
pub(crate) fn links(body: &str, base: Option<&str>) -> serde_json::Value {
    let doc = Html::parse_document(body);
    let base = effective_base(&doc, base);
    let base = base.as_deref();
    let mut seen = std::collections::BTreeSet::new();
    let mut out: Vec<serde_json::Value> = Vec::new();

    if let Some(sel) = ANCHORS.as_ref() {
        for el in doc.select(sel) {
            let Some(href) = el.value().attr("href") else {
                continue;
            };
            let Some(resolved) = resolve(href, base) else {
                continue;
            };
            if seen.insert(resolved.clone()) {
                out.push(resolved.into());
            }
        }
    }
    serde_json::Value::Array(out)
}

/// The document's EFFECTIVE base URL (WHATWG): the first `<base href>` in
/// tree order, resolved against the fetch URL (an `href` may itself be
/// relative), else the fetch URL unchanged. Pages that declare `<base
/// href>` (CMS, AMP, doc sites) resolve every relative link/canonical/img
/// against IT, not the fetch URL — honoring it is a correctness fix.
fn effective_base(doc: &Html, fetch: Option<&str>) -> Option<String> {
    let declared = BASE_HREF
        .as_ref()
        .and_then(|sel| doc.select(sel).next())
        .and_then(|el| el.value().attr("href"))
        .filter(|h| !h.is_empty());
    match declared {
        // `<base href>` resolved against the fetch URL (or absolute on its own).
        Some(href) => resolve_any(href, fetch).or_else(|| fetch.map(str::to_owned)),
        None => fetch.map(str::to_owned),
    }
}

/// Like [`resolve`] but keeps ANY scheme (the base URL itself need not be
/// http(s) to be a valid resolution root — though in practice it is).
fn resolve_any(href: &str, base: Option<&str>) -> Option<String> {
    match base.and_then(|b| url::Url::parse(b).ok()) {
        Some(base_url) => base_url.join(href).ok().map(|u| u.to_string()),
        None => url::Url::parse(href).ok().map(|u| u.to_string()),
    }
}

/// Resolve one href: against `base` when provided, else only absolute
/// http(s) URLs survive. Anything non-http(s) (mailto · javascript ·
/// tel · data) is dropped.
fn resolve(href: &str, base: Option<&str>) -> Option<String> {
    let url = match base.and_then(|b| url::Url::parse(b).ok()) {
        Some(base_url) => base_url.join(href).ok()?,
        None => url::Url::parse(href).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}
