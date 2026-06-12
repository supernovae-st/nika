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
/// SOUND against the nesting attack: every start tag of a NON-void
/// element opens a level (close tags pop). It does NOT honor `/>`
/// self-closing — `<div/>` is NOT self-closing in HTML5 (html5ever
/// nests it; only void + true foreign-content elements self-close), so
/// counting `<div/>`×N as N levels matches the tree the parser builds.
///
/// The soundness HINGES on not mis-skipping content html5ever parses as
/// real markup — skip too much and the scan UNDER-counts and the crash
/// is reachable. Two regions are skipped:
///
/// 1. **RAWTEXT/RCDATA/`SCRIPT_DATA`/PLAINTEXT bodies** ([`RAWTEXT_ELEMENTS`])
///    — in HTML context `<script>`/`<style>`/`<title>`/… content is TEXT,
///    so a literal `</div>` there does NOT nest (verified P0, 2026-06-12:
///    `<div><script></div></script>`×n holds a naive close-counting scan
///    flat at depth 2 while the real tree nests +1/unit to a
///    stack-overflowing htmd Drop). We jump to the matching `</name>`. BUT
///    this holds ONLY in HTML context: inside SVG/MathML foreign content
///    those very names are ordinary NESTING elements (html5ever
///    `step_foreign`), so the skip is GATED on a foreign-context stack —
///    `<svg>`/`<math>` turn it OFF, integration points (`foreignObject`/
///    `desc`/`title`/`mi…`) turn it back ON. Verified P0, 2026-06-12:
///    without the gate, `<svg><title><g>`×N skips to a `</title>` that
///    never comes, counting depth 2 while html5ever builds an N-deep
///    foreign DOM → htmd recursive-Drop SIGABRT (markdown is the default
///    `nika:fetch` mode, so the default path crashed).
/// 2. **Comments** (`<!-- … -->`, incl. the abrupt `<!-->`/`<!--->`
///    empty forms) — never nesting.
///
/// With the context gate, skipping can never UNDER-count the real tree;
/// the residual over-count (stray `<word` in text, mis-nested or
/// auto-closing `<p>`, foreign rawtext past a breakout) only ever
/// OVER-rejects pathological input — the browser-cap philosophy (browsers
/// cap DOM depth ~512 for the same reason).
pub(crate) fn guard_depth(body: &str, mode: ExtractMode) -> Result<(), ExtractError> {
    let bytes = body.as_bytes();
    // Open-element stack (lowercased names of the non-void, non-skipped
    // elements currently open). Its LENGTH is the live nesting depth; the
    // cap fires on `stack.len()`. STRICT-LIFO: a close pops ONLY when it
    // matches the top — a stray `</x>` is ignored, never collapsing a
    // foreign root we are still inside (popping it early would re-enable
    // the rawtext skip and UNDER-count · the SVG/MathML bypass below).
    let mut stack: Vec<String> = Vec::new();
    // Foreign-content context, pushed/popped in lock-step with `stack` for
    // marker elements only. The TOP says whether we are in HTML parsing
    // (where RAWTEXT tokenization — hence the skip — is valid) or inside
    // SVG/MathML foreign content (where `<script>`/`<title>`/… are ordinary
    // NESTING elements, not rawtext · html5ever `step_foreign`).
    let mut context: Vec<bool> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        match bytes.get(i + 1) {
            // Close tag: strict-LIFO pop (the matched top only).
            Some(b'/') => {
                let (name, j) = scan_name(bytes, body, i + 2);
                if stack.last().is_some_and(|top| *top == name) {
                    stack.pop();
                    if is_context_marker(&name) {
                        context.pop();
                    }
                }
                i = advance_past_gt(bytes, j);
            }
            // `<!--` comment → skip to its terminator. MUST honor the
            // HTML5 abrupt-closing forms `<!-->` and `<!--->` (empty
            // comments that end IMMEDIATELY): scanning past them for a
            // full `-->` would over-consume real markup to EOF and
            // UNDER-count the tree (a crash-through bypass). Other `<!…`
            // (doctype/CDATA) and `<?…` (PI) → skip to `>`.
            Some(b'!') if bytes[i + 2..].starts_with(b"--") => {
                i = skip_comment(bytes, i + 4);
            }
            Some(b'!' | b'?') => i = advance_past_gt(bytes, i + 2),
            // Open tag.
            Some(&c) if c.is_ascii_alphabetic() => {
                let (name, j) = scan_name(bytes, body, i + 1);
                let after_open = advance_past_gt(bytes, j);
                if VOID_ELEMENTS.contains(&name.as_str()) {
                    // Void elements take no close tag — they never nest.
                    i = after_open;
                } else if context.last().copied().unwrap_or(true)
                    && RAWTEXT_ELEMENTS.contains(&name.as_str())
                {
                    // HTML-context RAWTEXT/RCDATA/SCRIPT_DATA/PLAINTEXT: the
                    // body is TEXT, so an internal `</div>` never nests —
                    // count one level transiently for the cap, then jump to
                    // the matching close. In FOREIGN content this branch is
                    // skipped (the gate above is false), so the same names
                    // fall through to real counting below — that is the
                    // SVG/MathML soundness fix.
                    if stack.len() + 1 > MAX_HTML_DEPTH {
                        return Err(too_deep(mode));
                    }
                    i = skip_to_close(bytes, name.as_bytes(), after_open);
                } else {
                    // A real nesting element: push it (and its foreign
                    // context marker, if it carries one).
                    let kind = context_kind(&name);
                    stack.push(name);
                    if stack.len() > MAX_HTML_DEPTH {
                        return Err(too_deep(mode));
                    }
                    if let Some(html) = kind {
                        context.push(html);
                    }
                    i = after_open;
                }
            }
            // A bare `<` not starting a tag — ordinary text.
            _ => i += 1,
        }
    }
    Ok(())
}

/// Elements whose content html5ever does NOT parse as markup — RAWTEXT
/// (`style`/`xmp`/`iframe`/`noembed`/`noframes`), `SCRIPT_DATA` (`script`),
/// RCDATA (`textarea`/`title`), and PLAINTEXT (`plaintext`, which makes
/// the rest of the document text — [`skip_to_close`] finds no close and
/// runs to EOF, exactly the parser's behavior). Their bodies hold text
/// that may LOOK like tags but never nests, so the scan jumps over them.
///
/// `noscript` is deliberately EXCLUDED: html5ever parses with scripting
/// DISABLED, so `<noscript>` content IS real nested markup — skipping it
/// would UNDER-count and reopen the crash.
const RAWTEXT_ELEMENTS: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];

/// Elements that switch the parser INTO SVG/MathML foreign content. Their
/// subtree is NOT HTML — the [`RAWTEXT_ELEMENTS`] names NEST there rather
/// than tokenizing as text (html5ever `step_foreign`), so the rawtext
/// skip must be OFF while we are inside one.
const FOREIGN_ROOTS: &[&str] = &["svg", "math"];

/// HTML integration points — inside foreign content these RESUME HTML
/// parsing for their own subtree (so rawtext tokenization is valid again
/// below them). SVG `foreignObject`/`desc`/`title` + the `MathML` text
/// integration points.
const INTEGRATION_POINTS: &[&str] = &[
    "foreignobject",
    "desc",
    "title",
    "mi",
    "mo",
    "mn",
    "ms",
    "mtext",
];

/// The foreign-context marker for `name`: `Some(false)` for a foreign root
/// (entering foreign content · rawtext skip OFF), `Some(true)` for an
/// integration point (resuming HTML · skip ON), `None` for an ordinary
/// element (context unchanged · not tracked on the `context` stack).
fn context_kind(name: &str) -> Option<bool> {
    if FOREIGN_ROOTS.contains(&name) {
        Some(false)
    } else if INTEGRATION_POINTS.contains(&name) {
        Some(true)
    } else {
        None
    }
}

/// Whether `name` carries a foreign-context marker, so its matching close
/// pops the `context` stack in lock-step with the element stack.
fn is_context_marker(name: &str) -> bool {
    context_kind(name).is_some()
}

/// The depth-cap rejection — one wording, two call sites (the transient
/// rawtext check and the real push).
fn too_deep(mode: ExtractMode) -> ExtractError {
    ExtractError::Html {
        mode,
        reason: format!(
            "HTML nesting exceeds the {MAX_HTML_DEPTH}-level cap — refusing \
             to parse a pathologically deep document (DoS guard)"
        ),
    }
}

/// Scan an ASCII tag name at `from` → `(lowercased name, index just past
/// it)`. The name charset is `[A-Za-z0-9:-]` (the open-tag arm has already
/// vetted the first byte ASCII-alpha; a close tag may yield an empty name,
/// which matches no open element), so `body[from..j]` is boundary-safe.
fn scan_name(bytes: &[u8], body: &str, from: usize) -> (String, usize) {
    let mut j = from;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b':')
    {
        j += 1;
    }
    (body[from..j].to_ascii_lowercase(), j)
}

/// Index just past the next `>` at or after `from` (or `len` if none).
fn advance_past_gt(bytes: &[u8], from: usize) -> usize {
    advance_past(bytes, from, b">")
}

/// Index just past the next occurrence of `needle` at or after `from`
/// (or `len` if the needle never appears — an unterminated region runs
/// to EOF, matching how a parser would consume it).
fn advance_past(bytes: &[u8], from: usize, needle: &[u8]) -> usize {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i..].starts_with(needle) {
            return (i + needle.len()).min(bytes.len());
        }
        i += 1;
    }
    bytes.len()
}

/// Index just past the end of an HTML comment whose `<!--` opener ended
/// at `from` (i.e. `from` points just past `<!--`). Honors the HTML5
/// abrupt-closing empty-comment forms `<!-->` (`from` at `>`) and
/// `<!--->` (`from` at `-` then `>`) — WITHOUT this, scanning for a full
/// `-->` would over-consume real markup to EOF and UNDER-count the tree.
fn skip_comment(bytes: &[u8], from: usize) -> usize {
    // `<!-->` → empty comment, ends at the immediate `>`.
    if bytes.get(from) == Some(&b'>') {
        return from + 1;
    }
    // `<!--->` → empty comment, ends at `-` then `>`.
    if bytes.get(from) == Some(&b'-') && bytes.get(from + 1) == Some(&b'>') {
        return from + 2;
    }
    advance_past(bytes, from, b"-->")
}

/// Index just past the matching `</name>` (case-insensitive) at or after
/// `from`, for a RAWTEXT/RCDATA element. EOF if unterminated.
fn skip_to_close(bytes: &[u8], name: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() {
        // Look for `</`.
        if bytes[i] == b'<' && bytes.get(i + 1) == Some(&b'/') {
            let tag_start = i + 2;
            let end = (tag_start + name.len()).min(bytes.len());
            if bytes[tag_start..end].eq_ignore_ascii_case(name)
                // The next byte must end the tag name (`>`, whitespace,
                // or `/`) so `</script>` matches but `</scriptx>` doesn't.
                && bytes
                    .get(end)
                    .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
            {
                return advance_past_gt(bytes, end);
            }
        }
        i += 1;
    }
    bytes.len()
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
