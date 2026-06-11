// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HTML modes — `markdown` (htmd) · `text` (DOM walk) · `selector`
//! (CSS select). `article.rs` reuses the htmd leg on
//! readability-cleaned HTML.

use ego_tree::iter::Edge;
use nika_types::extract::ExtractMode;
use scraper::{Html, Node, Selector};

use crate::ExtractError;

/// Tags whose SUBTREES never contribute content (any mode).
const SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

/// Chrome stripped in `markdown` mode per the spec ("removes scripts ·
/// styles · nav · footer · ads") — `text` mode keeps page furniture
/// ("headers/footers preserved").
const MARKDOWN_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template", "nav", "footer"];

/// Block-level elements whose close emits a line break in `text` mode.
const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "blockquote",
    "pre",
    "title",
];

/// `mode: markdown` — HTML → cleaned Markdown via `htmd`.
pub(crate) fn markdown(body: &str) -> Result<serde_json::Value, ExtractError> {
    convert_markdown(body, ExtractMode::Markdown, MARKDOWN_SKIP_TAGS)
}

/// The shared htmd leg (`markdown` strips chrome; `article` receives
/// readability-cleaned HTML and only strips script/style).
pub(crate) fn convert_markdown(
    html: &str,
    mode: ExtractMode,
    skip: &[&str],
) -> Result<serde_json::Value, ExtractError> {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(skip.to_vec())
        .build();
    let md = converter.convert(html).map_err(|e| ExtractError::Html {
        mode,
        reason: e.to_string(),
    })?;
    Ok(serde_json::Value::String(md))
}

/// `mode: text` — tags stripped, text preserved with block-level line
/// breaks. Iterative tree walk (parser-hostile depth must never become
/// stack depth); script/style subtrees skipped via a depth counter.
/// Infallible: html5ever error-recovers, the walk is total.
pub(crate) fn text(body: &str) -> serde_json::Value {
    let doc = Html::parse_document(body);
    let mut out = String::new();
    let mut skip_depth = 0usize;

    for edge in doc.root_element().traverse() {
        match edge {
            Edge::Open(node) => match node.value() {
                Node::Element(el) => {
                    let name = el.name();
                    if SKIP_TAGS.contains(&name) {
                        skip_depth += 1;
                    } else if skip_depth == 0 && name == "br" {
                        out.push('\n');
                    }
                }
                Node::Text(t) if skip_depth == 0 => out.push_str(t),
                _ => {}
            },
            Edge::Close(node) => {
                if let Node::Element(el) = node.value() {
                    let name = el.name();
                    if SKIP_TAGS.contains(&name) {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else if skip_depth == 0 && BLOCK_TAGS.contains(&name) {
                        out.push('\n');
                    }
                }
            }
        }
    }

    serde_json::Value::String(tidy_text(&out))
}

/// Collapse intra-line whitespace runs and blank-line runs (HTML
/// whitespace is presentation, not content).
fn tidy_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        match (
            collapsed.is_empty(),
            lines.last().is_some_and(String::is_empty),
        ) {
            (true, true) => {} // collapse blank runs
            (true, false) => lines.push(String::new()),
            (false, _) => lines.push(collapsed),
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    while lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    lines.join("\n")
}

/// `mode: selector` — raw HTML of every match, concatenated (spec: "if
/// multiple match · concatenated").
pub(crate) fn selector(body: &str, sel: &str) -> Result<serde_json::Value, ExtractError> {
    let parsed = Selector::parse(sel).map_err(|e| ExtractError::Selector {
        selector: sel.to_owned(),
        reason: e.to_string(),
    })?;
    let doc = Html::parse_document(body);
    let html: Vec<String> = doc.select(&parsed).map(|el| el.html()).collect();
    Ok(serde_json::Value::String(html.join("\n")))
}
