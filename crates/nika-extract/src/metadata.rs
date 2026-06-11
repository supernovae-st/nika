// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: metadata` (head walk · title·description·og·twitter·
//! canonical·lang) and `mode: links` (every `<a href>` → absolute URL).
//!
//! Both are TOTAL: malformed HTML yields what the parser salvages
//! (html5ever error-recovers per the WHATWG spec) — honest emptiness,
//! never a failure. The selectors are compile-time constants; the
//! infallible signatures fail SOFT on the impossible parse branch.

use scraper::{Html, Selector};

/// `mode: metadata` — the spec's documented object shape. `og`/`twitter`
/// are ALWAYS objects (stable shape for jq consumers); scalar keys are
/// omitted when absent (JSON absence over null).
pub(crate) fn metadata(body: &str, base: Option<&str>) -> serde_json::Value {
    let doc = Html::parse_document(body);
    let mut out = serde_json::Map::new();
    let mut og = serde_json::Map::new();
    let mut twitter = serde_json::Map::new();

    if let Ok(sel) = Selector::parse("head > title")
        && let Some(el) = doc.select(&sel).next()
    {
        let title = el.text().collect::<String>().trim().to_owned();
        if !title.is_empty() {
            out.insert("title".to_owned(), title.into());
        }
    }

    if let Ok(sel) = Selector::parse("meta") {
        for el in doc.select(&sel) {
            let Some(content) = el.value().attr("content") else {
                continue;
            };
            if let Some(property) = el.value().attr("property")
                && let Some(key) = property.strip_prefix("og:")
                && !key.is_empty()
            {
                og.entry(key.to_owned()).or_insert_with(|| content.into());
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
                }
            }
        }
    }

    if let Ok(sel) = Selector::parse(r#"link[rel="canonical"]"#)
        && let Some(href) = doc
            .select(&sel)
            .next()
            .and_then(|el| el.value().attr("href"))
        && let Some(resolved) = resolve(href, base)
    {
        out.insert("canonical".to_owned(), resolved.into());
    }

    if let Ok(sel) = Selector::parse("html")
        && let Some(lang) = doc
            .select(&sel)
            .next()
            .and_then(|el| el.value().attr("lang"))
        && !lang.is_empty()
    {
        out.insert("lang".to_owned(), lang.into());
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

    if let Ok(sel) = Selector::parse("a[href]") {
        for el in doc.select(&sel) {
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
