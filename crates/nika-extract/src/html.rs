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

/// Maximum HTML nesting depth before extraction refuses the document.
/// Real content nests a few dozen deep; browsers themselves cap the DOM
/// tree (Chrome ~512, Firefox similar) PRECISELY to stop the
/// stack-overflow class this guards. 2048 is generous vs. legit content
/// yet far below the depth where any recursive consumer dies.
///
/// EMPIRICAL (2026-06-12): a 50 000-deep `<div>` SIGABRTs the process —
/// `htmd` builds a `markup5ever_rcdom` whose `Drop` is RECURSIVE
/// (`Rc<Node>` → child drop → … → stack overflow on teardown). A
/// source-level "all walks are iterative" audit MISSES this because the
/// recursion lives in `Drop`, not in a visible walk — the runtime probe
/// is authoritative. NOTE the guard must run BEFORE any html5ever
/// parse: the tree builder is itself super-linear on pathological deep
/// nesting (a full parse of 50 000-deep hangs), so a "parse then
/// measure" guard trades the crash for a hang. The byte-scan below
/// never parses — it early-exits in O(cap), not O(input).
const MAX_HTML_DEPTH: usize = 2048;

/// HTML void elements (HTML5 §12.1.2) — they take no close tag, so they
/// never open a nesting level. A page of 50 000 `<br>` is FLAT, not deep.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Reject a document whose tag-nesting depth exceeds [`MAX_HTML_DEPTH`]
/// BEFORE any DOM parse. A single O(n) byte scan with an O(cap)
/// early-exit: a hostile 50 000-deep body is rejected after ~2049 tags,
/// never fully parsed.
///
/// SOUND against the nesting attack by construction: every start tag of
/// a NON-void element opens a level (close tags pop). Crucially it does
/// NOT honor `/>` self-closing — `<div/>` is NOT self-closing in HTML5
/// (html5ever nests it; only void + true foreign-content elements
/// self-close), so counting `<div/>`×N as N levels matches the tree the
/// parser would build. The scan is a conservative UPPER bound: a stray
/// `<word` in text/script or an auto-closing `<p>`/`<li>` left unclosed
/// slightly over-counts — harmless, since legit content never
/// approaches 2048 nested tags and the only thing that does is an
/// attack, which is exactly what we reject (the browser-cap philosophy).
pub(crate) fn guard_depth(body: &str, mode: ExtractMode) -> Result<(), ExtractError> {
    let bytes = body.as_bytes();
    let mut depth: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        match bytes.get(i + 1) {
            // Close tag: pop a level.
            Some(b'/') => {
                depth = depth.saturating_sub(1);
                i = advance_past_gt(bytes, i + 2);
            }
            // Comment / doctype / CDATA: skip to '>'.
            Some(b'!') => i = advance_past_gt(bytes, i + 2),
            // Open tag: a non-void element opens a level.
            Some(&c) if c.is_ascii_alphabetic() => {
                let name_start = i + 1;
                let mut j = name_start;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b':')
                {
                    j += 1;
                }
                let name = body[name_start..j].to_ascii_lowercase();
                if !VOID_ELEMENTS.contains(&name.as_str()) {
                    depth += 1;
                    if depth > MAX_HTML_DEPTH {
                        return Err(ExtractError::Html {
                            mode,
                            reason: format!(
                                "HTML nesting exceeds the {MAX_HTML_DEPTH}-level cap — refusing \
                                 to parse a pathologically deep document (DoS guard)"
                            ),
                        });
                    }
                }
                i = advance_past_gt(bytes, j);
            }
            // A bare `<` not starting a tag — ordinary text.
            _ => i += 1,
        }
    }
    Ok(())
}

/// Index just past the next `>` at or after `from` (or `len` if none).
fn advance_past_gt(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() && bytes[i] != b'>' {
        i += 1;
    }
    (i + 1).min(bytes.len())
}

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
    // Leading blanks: one drain, not remove(0)-per-line (the collapse
    // bounds it to ≤1 anyway — this keeps the shape O(n) by inspection).
    let lead = lines
        .iter()
        .position(|l| !l.is_empty())
        .unwrap_or(lines.len());
    lines.drain(..lead);
    lines.join("\n")
}

/// Output ceiling for `mode: selector`: NESTED matches each serialize
/// their full subtree, so N nested hits cost Σ(subtree sizes) ≈ O(N²)
/// bytes — a 1 MiB hostile page of deeply nested `<div>`s against the
/// ordinary selector `div` would otherwise allocate gigabytes (review
/// lens 2 · P1). 64 MiB mirrors the transport's response cap.
const SELECTOR_OUTPUT_CEILING: usize = 64 * 1024 * 1024;

/// `mode: selector` — raw HTML of every match, concatenated (spec: "if
/// multiple match · concatenated") under [`SELECTOR_OUTPUT_CEILING`].
pub(crate) fn selector(body: &str, sel: &str) -> Result<serde_json::Value, ExtractError> {
    let parsed = Selector::parse(sel).map_err(|e| ExtractError::Selector {
        selector: sel.to_owned(),
        reason: e.to_string(),
    })?;
    let doc = Html::parse_document(body);
    let mut out = String::new();
    for el in doc.select(&parsed) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&el.html());
        if out.len() > SELECTOR_OUTPUT_CEILING {
            return Err(ExtractError::Html {
                mode: ExtractMode::Selector,
                reason: format!(
                    "selector output exceeds the {SELECTOR_OUTPUT_CEILING}-byte ceiling \
                     (nested matches each serialize their whole subtree — narrow the selector)"
                ),
            });
        }
    }
    Ok(serde_json::Value::String(out))
}
