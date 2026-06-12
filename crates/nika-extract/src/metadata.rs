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
static JSONLD: LazyLock<Option<Selector>> =
    LazyLock::new(|| parse_static(r#"script[type="application/ld+json"]"#));

/// Cap on JSON-LD blocks surfaced — a page with thousands of
/// `<script type=ld+json>` is pathological; the real web has 1–3
/// (`Product` · `BreadcrumbList` · `Organization`). Bounds the output.
const MAX_JSONLD_BLOCKS: usize = 32;

/// `mode: metadata` — the spec's documented object shape. `og`/`twitter`
/// are ALWAYS objects (stable shape for jq consumers); scalar keys are
/// omitted when absent (JSON absence over null).
pub(crate) fn metadata(body: &str, base: Option<&str>) -> serde_json::Value {
    let doc = Html::parse_document(body);
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
            let Some(content) = el.value().attr("content") else {
                continue;
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

    out.insert("og".to_owned(), serde_json::Value::Object(og));
    out.insert("twitter".to_owned(), serde_json::Value::Object(twitter));
    serde_json::Value::Object(out)
}

/// `mode: links` — document-order `<a href>` targets, resolved against
/// `base`, http(s) only (mailto/javascript/tel are not crawlable),
/// deduplicated first-occurrence-wins. Without a base, relative hrefs
/// are skipped (nothing sound to resolve them against).
pub(crate) fn links(body: &str, base: Option<&str>) -> serde_json::Value {
    let doc = Html::parse_document(body);
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
