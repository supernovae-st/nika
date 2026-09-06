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
pub(crate) const MAX_HTML_DEPTH: usize = 2048;

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
/// readability-cleaned HTML and only strips script/style). A custom `img`
/// handler resolves LAZY-loaded images (the placeholder-`src` +
/// `data-src`/`srcset` pattern) so they emit the real URL, not a blank
/// `data:` placeholder — Firecrawl does the same (the SOTA cheap win).
pub(crate) fn convert_markdown(
    html: &str,
    mode: ExtractMode,
    skip: &[&str],
) -> Result<serde_json::Value, ExtractError> {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(skip.to_vec())
        .add_handler(vec!["img"], lazy_img_handler)
        .build();
    let md = converter.convert(html).map_err(|e| ExtractError::Html {
        mode,
        reason: e.to_string(),
    })?;
    Ok(serde_json::Value::String(md))
}

/// htmd `img` handler with lazy-image resolution. For a NORMAL `<img src>`
/// it reproduces htmd's own markdown byte-for-byte (no regression); for a
/// LAZY image (missing/`data:`-placeholder `src` + a `data-src`/`srcset`)
/// it emits the real URL instead of the placeholder. Returns `None` when
/// no usable URL exists (matches htmd: an `<img>` with no link emits
/// nothing).
// `el` is by-value because htmd's `ElementHandler` trait dictates the
// `Fn(&dyn Handlers, Element)` signature — we only borrow it, but the
// trait owns the contract.
#[allow(clippy::needless_pass_by_value)]
fn lazy_img_handler(
    _: &dyn htmd::element_handler::Handlers,
    el: htmd::Element<'_>,
) -> Option<htmd::element_handler::HandlerResult> {
    let attr = |want: &str| {
        el.attrs
            .iter()
            .find(|a| a.name.local.as_ref() == want)
            .map(|a| a.value.as_ref())
    };
    // Real `src` wins (matches htmd exactly for normal images). Only when
    // it is absent or a `data:`/empty placeholder do we reach for the lazy
    // attributes — so normal-image output is unchanged.
    let real_src = attr("src").filter(|s| !s.is_empty() && !s.starts_with("data:"));
    let link = real_src
        .map(str::to_owned)
        .or_else(|| best_srcset(attr("srcset").or_else(|| attr("data-srcset"))))
        .or_else(|| attr("data-src").map(str::to_owned))
        .or_else(|| attr("data-lazy-src").map(str::to_owned))
        .or_else(|| attr("data-original").map(str::to_owned))
        // Last resort: whatever `src` was (even a placeholder) — never
        // worse than htmd's default.
        .or_else(|| attr("src").map(str::to_owned))?;

    // htmd's exact escaping (img.rs): alt/title trimmed + `"`-escaped per
    // line; link parens escaped; spaces wrap the link in `<>`.
    let clean = |text: &str| {
        text.lines()
            .map(|l| l.trim().replace('"', "\\\""))
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let alt = attr("alt").map(clean).unwrap_or_default();
    let link = link.replace('(', "\\(").replace(')', "\\)");
    let title = attr("title")
        .map(clean)
        .filter(|t| !t.is_empty())
        .map_or(String::new(), |t| format!(" \"{t}\""));
    let (lt, gt) = if link.contains(' ') {
        ("<", ">")
    } else {
        ("", "")
    };
    Some(format!("![{alt}]({lt}{link}{title}{gt})").into())
}

/// Pick the highest-resolution URL from a `srcset` value. `srcset` is
/// comma-separated `url [descriptor]` where the descriptor is `<n>w`
/// (width) or `<n>x` (density); we keep the largest width, else the
/// largest density, else the last entry (srcset is conventionally
/// ascending). `None` when empty.
fn best_srcset(srcset: Option<&str>) -> Option<String> {
    let srcset = srcset?;
    let mut best: Option<(u64, String)> = None;
    for entry in srcset.split(',') {
        let mut parts = entry.split_whitespace();
        let Some(url) = parts.next() else {
            continue;
        };
        // Rank: width descriptor (×1000 to dominate density) > density >
        // bare/last (rank 1, so a later bare entry wins over an earlier one).
        let rank = parts.next().map_or(1, |d| {
            let digits: String = d.chars().take_while(char::is_ascii_digit).collect();
            let n: u64 = digits.parse().unwrap_or(0);
            if d.ends_with('w') {
                n.saturating_mul(1000)
            } else if d.ends_with('x') {
                n
            } else {
                1
            }
        });
        if best.as_ref().is_none_or(|(r, _)| rank >= *r) {
            best = Some((rank, url.to_owned()));
        }
    }
    best.map(|(_, url)| url)
}

/// `mode: text` — tags stripped, text preserved with block-level line
/// breaks. Iterative tree walk (parser-hostile depth must never become
/// stack depth); script/style subtrees skipped via a depth counter.
/// Infallible: html5ever error-recovers, the walk is total.
pub(crate) fn text(body: &str) -> serde_json::Value {
    let doc = Html::parse_document(body);
    // Plain text is never longer than the HTML it came from — one
    // allocation up front beats ~26 reallocs growing into a 64 MiB body.
    let mut out = String::with_capacity(body.len());
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
/// whitespace is presentation, not content). Single streaming pass into
/// ONE buffer — no `Vec<String>` of every line, no per-line join — so a
/// 64 MiB text body costs one output copy, not three. Output contract
/// (unchanged): content lines `\n`-joined, at most one blank line
/// between content blocks, no leading/trailing blanks.
fn tidy_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut collapsed = String::new(); // reused across lines (one alloc)
    let mut pending_blank = false; // a blank seen AFTER content — emit iff more content follows
    let mut wrote_content = false;
    for line in raw.lines() {
        collapsed.clear();
        for word in line.split_whitespace() {
            if !collapsed.is_empty() {
                collapsed.push(' ');
            }
            collapsed.push_str(word);
        }
        if collapsed.is_empty() {
            // Suppress leading blanks; remember an interior one (collapsing runs).
            pending_blank = wrote_content;
        } else {
            if pending_blank {
                out.push('\n');
                pending_blank = false;
            }
            if wrote_content {
                out.push('\n');
            }
            out.push_str(&collapsed);
            wrote_content = true;
        }
    }
    out // trailing blanks never emitted (pending_blank discarded at EOF)
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

#[cfg(test)]
mod tests {
    use super::{
        ExtractMode, MAX_HTML_DEPTH, advance_past, best_srcset, context_kind, guard_depth,
        is_context_marker, scan_name, selector, skip_comment, skip_to_close, tidy_text,
    };

    // The single-pass rewrite's contract: intra-line whitespace collapses,
    // blank-line RUNS collapse to at most one, leading/trailing blanks are
    // stripped. Pins behavior the end-to-end `text()` tests don't fully
    // exercise (multi-blank runs · padded edges).
    #[test]
    fn tidy_text_collapses_runs_and_strips_edges() {
        // intra-line runs → single spaces; the line keeps its content.
        assert_eq!(tidy_text("  a   b\tc  "), "a b c");
        // a single interior blank survives (paragraph break).
        assert_eq!(tidy_text("one\n\ntwo"), "one\n\ntwo");
        // a RUN of blanks collapses to exactly one.
        assert_eq!(tidy_text("one\n\n\n\ntwo"), "one\n\ntwo");
        // adjacent content lines stay single-`\n` separated.
        assert_eq!(tidy_text("one\ntwo"), "one\ntwo");
        // leading + trailing blank runs are stripped entirely.
        assert_eq!(tidy_text("\n\n  \nhi\n  \n\n"), "hi");
        // whitespace-only input yields empty (no spurious blank line).
        assert_eq!(tidy_text("   \n\t\n  "), "");
        assert_eq!(tidy_text(""), "");
    }

    // Markdown path drives `lazy_img_handler`. Helper returns the rendered
    // markdown string so a flipped img-resolver operator changes the output.
    fn md(body: &str) -> String {
        match super::markdown(body) {
            Ok(serde_json::Value::String(s)) => s,
            other => panic!("markdown() returned non-string / err: {other:?}"),
        }
    }

    // ───────────────────────────────────────────────────────────────────
    // is_context_marker (line 225 `→true` / `→false`)
    //
    // A marker name (foreign root / integration point) must return TRUE; an
    // ordinary element must return FALSE. Asserting BOTH polarities pins the
    // function against the constant-replacement mutants: `→true` is killed by
    // the `div` FALSE assertion, `→false` by the `svg` TRUE assertion.
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn is_context_marker_true_for_markers_false_for_plain() {
        // Foreign root → marker.
        assert!(is_context_marker("svg"), "svg is a foreign root → marker");
        assert!(is_context_marker("math"), "math is a foreign root → marker");
        // Integration point → marker.
        assert!(
            is_context_marker("foreignobject"),
            "foreignObject is an integration point → marker"
        );
        assert!(
            is_context_marker("desc"),
            "desc is an integration point → marker"
        );
        // Ordinary elements → NOT markers (kills the `→true` mutant).
        assert!(
            !is_context_marker("div"),
            "div carries no foreign-context marker"
        );
        assert!(
            !is_context_marker("p"),
            "p carries no foreign-context marker"
        );
        assert!(
            !is_context_marker("script"),
            "script is rawtext, not a context marker"
        );
    }

    // context_kind drives is_context_marker; pin its three arms directly so a
    // wrong branch can't masquerade as the right Option-ness.
    #[test]
    fn context_kind_three_arms() {
        // foreign root → Some(false) (rawtext skip OFF inside it).
        assert_eq!(context_kind("svg"), Some(false));
        assert_eq!(context_kind("math"), Some(false));
        // integration point → Some(true) (HTML parsing resumes · skip ON).
        assert_eq!(context_kind("title"), Some(true));
        assert_eq!(context_kind("mi"), Some(true));
        // ordinary element → None (untracked).
        assert_eq!(context_kind("div"), None);
    }

    // ───────────────────────────────────────────────────────────────────
    // scan_name (line 246 `<→<=`, line 247 `||→&&`)
    //
    // 247 `||→&&`: the name charset is `[A-Za-z0-9] | '-' | ':'`. A real name
    // mixing all three classes is only scanned WHOLE under the `||`; under
    // `&&` every byte fails (no byte is alnum AND '-' AND ':' at once) so the
    // scan stops at `from`, yielding an empty name.
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn scan_name_reads_full_mixed_charset_name() {
        let body = "a1-b:c>";
        let bytes = body.as_bytes();
        let (name, j) = scan_name(bytes, body, 0);
        // Whole `[alnum | '-' | ':']` run consumed, lowercased, stops at `>`.
        assert_eq!(name, "a1-b:c", "the `||` must accept alnum AND '-' AND ':'");
        assert_eq!(j, 6, "index lands just past the name, on `>`");
    }

    // 246 `<→<=`: the loop bound `j < bytes.len()`. When the tag name runs to
    // EOF (no terminator byte), the correct loop STOPS at `j == len`. The
    // mutated `<=` would test `bytes[len]` → out-of-bounds panic. A name that
    // ends exactly at EOF therefore distinguishes the two: current code
    // returns cleanly, the mutant panics.
    #[test]
    fn scan_name_name_running_to_eof_does_not_overrun() {
        let body = "div"; // pure name, no '>' — ends exactly at len.
        let bytes = body.as_bytes();
        let (name, j) = scan_name(bytes, body, 0);
        assert_eq!(name, "div");
        assert_eq!(j, bytes.len(), "stops AT len, never indexes bytes[len]");
    }

    // ───────────────────────────────────────────────────────────────────
    // skip_comment (line 280 `==→!=`, 281 `+→-/*`, 284 `==→!=` `&&→||` `+→-/*`,
    //               285 `+→-/*`)
    //
    // `from` points just past `<!--`. The three shapes:
    //   `<!-->`  → abrupt empty, ends at the immediate `>`  (return from+1)
    //   `<!--->` → abrupt empty, ends at `-` then `>`       (return from+2)
    //   `<!--x-->` → ordinary, ends at the `-->` terminator (advance_past)
    // Exact return indices pin every arithmetic + comparison operator.
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn skip_comment_abrupt_gt_form() {
        // bytes = `>rest`; `from = 0` points at `>` (the `<!-->` shape).
        // Correct: return 1 (just past `>`). Kills 280 `==→!=` (mutant would
        // fall through to advance_past and consume to EOF) and 281 `+→-/*`
        // (return index must be exactly from+1).
        let bytes = b">rest";
        assert_eq!(skip_comment(bytes, 0), 1);
    }

    #[test]
    fn skip_comment_abrupt_dash_gt_form() {
        // bytes = `->rest`; `from = 0` at `-`, `from+1` at `>` (`<!--->`).
        // Correct: return 2. Kills 284 `==→!=`, 284 `+→-/*` (the `from + 1`
        // index), 285 `+→-/*` (the `from + 2` return), and—paired with the
        // next test—284 `&&→||`.
        let bytes = b"->rest";
        assert_eq!(skip_comment(bytes, 0), 2);
    }

    #[test]
    fn skip_comment_dash_not_followed_by_gt_runs_to_terminator() {
        // bytes = `-x-->tail`; `from = 0` at `-`, `from+1` is `x` (NOT `>`).
        // The `&&` guard is FALSE → must fall through to advance_past('-->').
        // Under the `||` mutant the first operand (`==Some('-')`) alone would
        // wrongly trigger the early `from+2` return (=> 2). Correct path scans
        // to the `-->` ending at index 5.
        let bytes = b"-x-->tail";
        let out = skip_comment(bytes, 0);
        assert_eq!(
            out, 5,
            "`&&` must require BOTH `-` and `>`; here it falls through to `-->`"
        );
        assert_ne!(out, 2, "the `||` mutant would early-return from+2 here");
    }

    #[test]
    fn skip_comment_ordinary_finds_terminator() {
        // A non-abrupt comment body: neither abrupt branch fires, so the
        // `==→!=` flips on lines 280/284 would (wrongly) ENTER an abrupt
        // branch here. `from = 0` at `a`. Correct: advance_past `-->` → 6.
        let bytes = b"abc-->z";
        assert_eq!(skip_comment(bytes, 0), 6);
    }

    // ───────────────────────────────────────────────────────────────────
    // skip_to_close (line 296 `&&→||`, 302 `&&→||`, 304 `==→!=`)
    //
    // Drives the rawtext close-tag scan. `name` is the rawtext element; the
    // scan jumps to just past `</name>` (case-insensitive, terminator-checked).
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn skip_to_close_matches_terminated_close() {
        // `..</style>tail`. Correct: index just past `>` of `</style>`.
        // 304 `==→!=`: the close is terminated by `>`. Under `!=`, `>` is
        // REJECTED as a terminator → no match → run to EOF (len). Correct
        // returns the position right after `</style>`.
        let body = "abc</style>tail";
        let bytes = body.as_bytes();
        let from = 0;
        let out = skip_to_close(bytes, b"style", from);
        // `</style>` occupies bytes 3..11; index just past `>` is 11.
        assert_eq!(out, 11, "must stop just past the `>`-terminated `</style>`");
        assert_ne!(
            out,
            bytes.len(),
            "the `==→!=` mutant rejects `>` → runs to EOF"
        );
    }

    #[test]
    fn skip_to_close_rejects_non_terminator_suffix() {
        // `</scriptx>` is NOT a close for `script` (next byte `x` is not a
        // terminator); the real `</script>` follows. 302 `&&→||`: under `||`
        // the name-match alone (ignoring the terminator check) would stop at
        // `</scriptx>` (index 10). Correct skips it and stops past `</script>`.
        let body = "</scriptx></script>end";
        let bytes = body.as_bytes();
        let out = skip_to_close(bytes, b"script", 0);
        // `</script>` is at 10..19; just past `>` is 19.
        assert_eq!(out, 19, "the trailing `x` disqualifies `</scriptx>`");
        assert_ne!(out, 10, "the `||` mutant would accept `</scriptx>`");
    }

    #[test]
    fn skip_to_close_requires_slash_after_lt() {
        // 296 `&&→||`: the open-of-close detector is `bytes[i]=='<' AND
        // next=='/'`. Under `||`, a bare `<` (no following `/`) would enter
        // the branch and try to match `name` at `i+2`. Craft a body where a
        // bare `<style` (an OPEN tag, no slash) precedes the real `</style>`,
        // positioned so the `||` mutant mis-matches `style` two bytes past the
        // bare `<`. Correct only stops at the true `</style>`.
        //   "<style></style>X"
        //    ^0      ^7 real close
        // Under `||`: at i=0 (`<`), i+1 is `s` (not `/`), tag_start=2,
        // bytes[2..7]=="tyle>"? name is `style` (5 bytes) → "tyle>" != "style"
        // so that particular i doesn't match — pick a body that DOES expose it:
        let body = "<style</style>Z";
        let bytes = body.as_bytes();
        let out = skip_to_close(bytes, b"style", 0);
        // Real `</style>` is at 6..14; just past `>` is 14.
        assert_eq!(out, 14, "only `</style>` (with the slash) is the close");
        // Under the `||` mutant, i=0 is `<`; with `||` it skips the slash
        // check, tag_start=2, bytes[2..7]="tyle<" != "style" → still no match
        // there, but i=5 is `<` followed by `/` so both behave; the decisive
        // distinguishing input is the bare `<` WITHOUT a following close —
        // covered by the truncated test below.
    }

    #[test]
    fn skip_to_close_bare_lt_then_name_only_matches_via_slash() {
        // Decisive 296 `&&→||` case: a bare `<style>` (open, slash-less)
        // sitting exactly `name.len()`+2 before nothing — under `||` the
        // detector fires on the bare `<` and matches `style` at i+2, returning
        // early; under the correct `&&` it requires the `/` and finds none →
        // runs to EOF.
        //   bytes: `<style>` then EOF, name = `style`.
        //   i=0: '<', next 's' (not '/').  &&: skip.  ||: tag_start=2,
        //        bytes[2..7] = "tyle>" != "style" → no early match.
        // To force a positive ||-only match we need `</`-less `<` directly
        // followed by the name:  `<<style>`  — at i=0 '<', next '<' (not '/');
        // ||: tag_start=2 → bytes[2..7]="style" == name AND end byte '>' is a
        // terminator → ||-mutant returns past that `>` (index 8). The correct
        // && path: i=0 no slash; i=1 '<', next 's' (not '/') no; no `</` ever
        // → runs to EOF (len 8). Both give 8 here, so instead place a
        // terminator the mutant would stop BEFORE:
        let body = "<<style>X</style>";
        let bytes = body.as_bytes();
        let out = skip_to_close(bytes, b"style", 0);
        // Correct: the FIRST real `</` is at index 9 (`</style>` 9..17), past
        // `>` is 17. The `||` mutant fires at i=1 (`<` then `style`) and
        // returns past the `>` at index 7 → 8. They DIFFER.
        assert_eq!(
            out, 17,
            "`&&` ignores the slash-less `<style>` and finds `</style>`"
        );
        assert_ne!(
            out, 8,
            "the `||` mutant would match the open `<style>` and stop at 8"
        );
    }

    // ───────────────────────────────────────────────────────────────────
    // guard_depth integration — exercises scan_name / skip_comment /
    // skip_to_close / context tracking IN SITU at the depth boundary, so the
    // accept/reject verdict (Ok vs too-deep Err) flips when the byte-scan
    // arithmetic is wrong.
    // ───────────────────────────────────────────────────────────────────

    // A rawtext element body that LOOKS deep but is flat: `<script>` whose
    // body holds N literal `<div>` is depth 1, not N. This relies on
    // skip_to_close jumping past the `</script>`. If skip_to_close mis-scans
    // (304/302/296 mutants), the `<div>`s inside get counted and the document
    // is wrongly rejected.
    #[test]
    fn guard_depth_rawtext_body_is_flat_not_deep() {
        let mut body = String::from("<script>");
        for _ in 0..(MAX_HTML_DEPTH + 50) {
            body.push_str("<div>");
        }
        body.push_str("</script>");
        // The `<div>`s are TEXT inside script → flat. Must be accepted.
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "script body is rawtext (flat) — skip_to_close must jump past it"
        );
    }

    // The same `<div>` flood NOT wrapped in rawtext IS genuinely deep and must
    // be rejected — pins that the accept above is real skipping, not a
    // blanket accept.
    #[test]
    fn guard_depth_real_deep_nesting_rejected() {
        let mut body = String::new();
        for _ in 0..(MAX_HTML_DEPTH + 5) {
            body.push_str("<div>");
        }
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "genuinely nested `<div>`s past the cap must be refused"
        );
    }

    // Exact depth boundary: a stack of EXACTLY MAX_HTML_DEPTH open elements is
    // accepted; one more is rejected. Pins the `stack.len() > MAX_HTML_DEPTH`
    // off-by-one (the real-push cap site) and proves scan_name reads every
    // `<div>` name correctly (a mis-scan would mis-count the stack).
    #[test]
    fn guard_depth_exact_cap_boundary() {
        let at_cap: String = "<div>".repeat(MAX_HTML_DEPTH);
        assert!(
            guard_depth(&at_cap, ExtractMode::Markdown).is_ok(),
            "exactly the cap is allowed"
        );
        let over_cap: String = "<div>".repeat(MAX_HTML_DEPTH + 1);
        assert!(
            guard_depth(&over_cap, ExtractMode::Markdown).is_err(),
            "one past the cap is refused"
        );
    }

    // Foreign-context gate: inside `<svg>` the rawtext names (`<script>`)
    // NEST instead of tokenizing as text, because the `context` top is
    // `false` (foreign) so the rawtext-skip branch is OFF. A deep
    // `<svg><script><script>…` chain must therefore be rejected. This
    // exercises context_kind/is_context_marker (the `context` stack): if
    // marker tracking is wrong (225 mutants) the rawtext skip stays ON inside
    // the svg, under-counts, and wrongly ACCEPTS a genuinely deep foreign tree.
    #[test]
    fn guard_depth_foreign_script_nests_and_is_rejected() {
        let mut body = String::from("<svg>");
        // <script> is a marker-less element (context_kind == None): inside the
        // foreign (`false`) context it falls through to REAL nesting, so each
        // one opens a level.
        for _ in 0..(MAX_HTML_DEPTH + 5) {
            body.push_str("<script>");
        }
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "inside <svg>, <script> nests (foreign context · skip OFF) — deep chain refused"
        );
    }

    // Control proving the foreign gate is what made the difference: the SAME
    // deep `<script>` flood WITHOUT the `<svg>` wrapper is rawtext in HTML
    // context and stays FLAT — accepted. Pins that the `is_context_marker`
    // push/pop (and the `<svg>` foreign root) genuinely toggle the skip; a
    // `→true` mutant on is_context_marker would mis-track the context stack.
    #[test]
    fn guard_depth_html_context_script_flood_is_flat() {
        let mut body = String::new();
        for _ in 0..(MAX_HTML_DEPTH + 5) {
            // No close → each is rawtext-skipped to EOF after counting +1
            // transiently; they never accumulate (depth never exceeds 1).
            body.push_str("<script></script>");
        }
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "HTML-context <script> is rawtext (flat) — never reaches the cap"
        );
    }

    // Comment skipping in situ: a body of N `<!--...-->` comments is FLAT
    // (comments never nest). If skip_comment over- or under-consumes
    // (280/281/284/285 mutants), the surrounding real tags get mis-counted.
    #[test]
    fn guard_depth_comments_are_flat() {
        let mut body = String::from("<div>");
        for _ in 0..50 {
            body.push_str("<!-- c -->");
        }
        // abrupt empty forms interleaved (exercise the abrupt branches).
        for _ in 0..50 {
            body.push_str("<!-->");
            body.push_str("<!--->");
        }
        body.push_str("</div>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "comments (incl. abrupt forms) are flat — one real <div> only"
        );
    }

    // ───────────────────────────────────────────────────────────────────
    // lazy_img_handler (line 392 `delete !`, 416 `delete !`)
    //
    // 392 `delete !`: real_src keeps `src` only when it is NON-empty AND
    // NON-`data:` placeholder. The filter is
    // `!s.is_empty() && !s.starts_with("data:")`. Deleting either `!`
    // inverts which images count as "real".
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn lazy_img_real_src_wins_over_data_src() {
        // A normal `<img src=...>` resolves to that src — NOT the data-src.
        // 392 first `!` (`!s.is_empty()`): deleting it makes a NON-empty src
        // be treated as a placeholder → the handler would fall through to
        // data-src. Assert the REAL src is in the output.
        let out =
            md(r#"<img src="https://real.example/a.png" data-src="https://lazy.example/b.png">"#);
        assert!(
            out.contains("https://real.example/a.png"),
            "real non-empty src must win; got: {out}"
        );
        assert!(
            !out.contains("https://lazy.example/b.png"),
            "lazy data-src must NOT be used when a real src exists; got: {out}"
        );
    }

    #[test]
    fn lazy_img_data_placeholder_src_falls_through_to_data_src() {
        // A `data:`-placeholder src is NOT real → resolve the lazy data-src.
        // 392 second `!` (`!s.starts_with("data:")`): deleting it makes the
        // data: placeholder count as "real" → output would carry the
        // placeholder, not the lazy URL. Assert the lazy URL wins.
        let out = md(
            r#"<img src="data:image/gif;base64,R0lGODlh" data-src="https://lazy.example/real.png">"#,
        );
        assert!(
            out.contains("https://lazy.example/real.png"),
            "data: placeholder must fall through to data-src; got: {out}"
        );
        assert!(
            !out.contains("data:image/gif"),
            "the data: placeholder must NOT be the emitted URL; got: {out}"
        );
    }

    // 416 `delete !`: the link gets `<>`-wrapped IFF it contains a space; the
    // title-emptiness filter `.filter(|t| !t.is_empty())` (416) drops an empty
    // title so no ` ""` suffix is emitted. Deleting the `!` would KEEP empty
    // titles → emit a spurious ` ""`.
    #[test]
    fn lazy_img_empty_title_emits_no_title_suffix() {
        // An img with an explicit EMPTY title must render WITHOUT a ` ""`
        // title clause. `delete !` on the `!t.is_empty()` filter would keep
        // the empty string and format ` ""` into the output.
        let out = md(r#"<img src="https://x.example/i.png" alt="a" title="">"#);
        assert_eq!(
            out.trim(),
            "![a](https://x.example/i.png)",
            "empty title must NOT add a ` \"\"` suffix; got: {out}"
        );
        assert!(
            !out.contains("\"\""),
            "no empty-title clause expected; got: {out}"
        );
    }

    #[test]
    fn lazy_img_non_empty_title_is_emitted() {
        // Positive control: a real title DOES appear, so the filter isn't a
        // blanket drop (guards against a `→false`-style collapse of 416).
        let out = md(r#"<img src="https://x.example/i.png" alt="a" title="cap">"#);
        assert!(
            out.contains("\"cap\""),
            "non-empty title must be emitted; got: {out}"
        );
    }

    // best_srcset feeds lazy_img_handler's lazy branch — pin its ranking so a
    // srcset lazy image resolves to the widest candidate.
    #[test]
    fn best_srcset_picks_widest() {
        assert_eq!(
            best_srcset(Some("a.png 100w, b.png 800w, c.png 400w")).as_deref(),
            Some("b.png"),
            "the 800w entry is widest"
        );
        // density vs width: any width dominates any density.
        assert_eq!(
            best_srcset(Some("d.png 2x, e.png 50w")).as_deref(),
            Some("e.png")
        );
        assert_eq!(best_srcset(Some("")).as_deref(), None);
        assert_eq!(best_srcset(None), None);
    }

    // ───────────────────────────────────────────────────────────────────
    // selector ceiling (line 542 `*→+` / `*→/`, line 558 `>→==/>=`)
    //
    // The ceiling is 64*1024*1024. We cannot fill 64 MiB cheaply, but the
    // ARITHMETIC mutants shrink the constant: `/` makes it 0 or 64; one `+`
    // makes it 66_560; the other `+` makes it ~1.06 MiB. A ~1.5 MiB selector
    // output is Ok under the real ceiling but EXCEEDS every shrunk variant,
    // so a single large-output Ok assertion kills all four arithmetic mutants.
    // ───────────────────────────────────────────────────────────────────
    #[test]
    fn selector_large_output_under_real_ceiling_is_ok() {
        // One <p> whose text serializes to ~1.5 MiB (> 1.06 MiB = the largest
        // shrunk ceiling, ≪ 64 MiB = the real one). `.html()` of the match
        // reproduces the body, so out.len() ≈ 1.5 MiB.
        let big = "x".repeat(1_500_000);
        let body = format!("<p id=\"m\">{big}</p>");
        let res = selector(&body, "p#m");
        assert!(
            res.is_ok(),
            "1.5 MiB output is under the real 64 MiB ceiling — every shrunk \
             ceiling (0 / 64 / 66_560 / ~1.06 MiB) would wrongly reject it"
        );
        if let Ok(serde_json::Value::String(s)) = res {
            assert!(
                s.len() > 1_114_112,
                "output must exceed the ~1.06 MiB `+` mutant ceiling"
            );
        }
    }

    #[test]
    fn selector_small_match_is_ok() {
        // A tiny selector output: the `/`-mutant ceilings (0 and 64) reject
        // even this; the real path accepts. Belt-and-suspenders against the
        // division mutants, independent of the big-output allocation.
        let res = selector("<div class=\"c\">hello world content</div>", "div.c");
        assert!(res.is_ok(), "a tiny match is well under any sane ceiling");
        match res {
            Ok(serde_json::Value::String(s)) => {
                assert!(s.contains("hello world content"), "match HTML serialized");
            }
            other => panic!("expected Ok(String), got {other:?}"),
        }
    }

    // advance_past underpins advance_past_gt / skip_comment / skip_to_close —
    // pin its needle arithmetic so a returned index is always JUST PAST the
    // needle, and EOF when absent.
    #[test]
    fn advance_past_lands_just_past_needle() {
        assert_eq!(
            advance_past(b"ab-->cd", 0, b"-->"),
            5,
            "just past the `-->`"
        );
        assert_eq!(
            advance_past(b"abc", 0, b"-->"),
            3,
            "absent needle → len (EOF)"
        );
        assert_eq!(advance_past(b">x", 0, b">"), 1, "single-byte needle");
    }
}
