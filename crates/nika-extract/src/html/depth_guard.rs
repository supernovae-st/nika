// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pre-parse DEPTH GUARD — a single byte scan that refuses a
//! pathologically nested document BEFORE html5ever ever sees it, plus the
//! tag-scanning primitives it is built from. Carved out of `html.rs`
//! verbatim (same code, same tests) so both files stay well inside the
//! 1500-line file cap; nothing here is reachable outside the crate beyond
//! the two items `html.rs` re-exports (`guard_depth` · `MAX_HTML_DEPTH`).

use nika_types::extract::ExtractMode;

use crate::ExtractError;

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
/// element opens a level (close tags pop). `/>` is honored ONLY inside
/// SVG/MathML, where the parser acknowledges the self-closing flag —
/// `<div/>` is NOT self-closing in HTML5 (html5ever nests it), so
/// counting `<div/>`×N as N levels matches the tree the parser builds.
/// Two shapes the parser closes for the author are honored too, both in
/// HTML context only: the OPTIONAL end tags (`<li>` closed by the next
/// `<li>`, `<p>` by a following block) and the implied end tags a close
/// tag generates (`</ul>` closing the items under it). Each is a strict
/// subset of the parser's own unwinding, so the scan can only ever count
/// MORE depth than the real tree.
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
                pop_close_tag(&mut stack, &mut context, &name);
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
                i = open_tag(bytes, name, j, &mut stack, &mut context, mode)?;
            }
            // A bare `<` not starting a tag — ordinary text.
            _ => i += 1,
        }
    }
    Ok(())
}

/// A close tag against the open-element stack: "generate implied end
/// tags" first — a `</ul>` closes the `<li>`s left open under it, and only
/// the optional-end elements are popped through, a strict subset of the
/// parser's own unwinding, so the count can never fall below the real
/// tree's depth — then the strict-LIFO pop of the matched top only.
fn pop_close_tag(stack: &mut Vec<String>, context: &mut Vec<bool>, name: &str) {
    if in_html_context(context) && stack.last().is_none_or(|top| *top != name) {
        let implied = stack
            .iter()
            .rev()
            .take_while(|open| OPTIONAL_END_TAGS.contains(&open.as_str()))
            .count();
        let below = stack.len().saturating_sub(implied);
        if implied > 0 && below > 0 && stack[below - 1] == name {
            stack.truncate(below);
        }
    }
    if stack.last().is_some_and(|top| *top == name) {
        stack.pop();
        if is_context_marker(name) {
            context.pop();
        }
    }
}

/// An open tag: void elements never nest; a self-closed element in foreign
/// content opens and closes at once (a page of syntax diagrams holds
/// thousands of `<path/>` — counting them open reads as thousands of
/// levels and refuses the page whole); an HTML-context RAWTEXT body is
/// TEXT, so it counts one transient level and jumps to its close (in
/// foreign content the same names fall through to real counting — the
/// SVG/MathML soundness fix); an element whose end tag the author may omit
/// is closed by its successor before the push (HTML context only), and a
/// real nesting element pushes itself and its context marker. Returns the
/// index to continue scanning from.
fn open_tag(
    bytes: &[u8],
    name: String,
    j: usize,
    stack: &mut Vec<String>,
    context: &mut Vec<bool>,
    mode: ExtractMode,
) -> Result<usize, ExtractError> {
    let after_open = advance_past_gt(bytes, j);
    if VOID_ELEMENTS.contains(&name.as_str()) {
        return Ok(after_open);
    }
    if !in_html_context(context) && is_self_closing(bytes, j, after_open) {
        return Ok(after_open);
    }
    if in_html_context(context) && RAWTEXT_ELEMENTS.contains(&name.as_str()) {
        if stack.len() + 1 > MAX_HTML_DEPTH {
            return Err(too_deep(mode));
        }
        return Ok(skip_to_close(bytes, name.as_bytes(), after_open));
    }
    if in_html_context(context) {
        while stack
            .last()
            .is_some_and(|top| closes_implicitly(&name, top))
        {
            stack.pop();
        }
    }
    let kind = context_kind(&name);
    stack.push(name);
    if stack.len() > MAX_HTML_DEPTH {
        return Err(too_deep(mode));
    }
    if let Some(html) = kind {
        context.push(html);
    }
    Ok(after_open)
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

/// Elements whose END TAG IS OPTIONAL (HTML §13.1.2.4): the parser closes
/// them implicitly, at the next sibling of the same kind or at the parent's
/// end tag. A scan that waits for a `</li>` that the author never wrote
/// counts one level per item — which is how a plain menu, or a page written
/// the way sqlite.org writes its documentation, reads as thousands of
/// nesting levels and gets refused whole. None of these names is a
/// foreign-context marker, so popping them never disturbs `context`.
const OPTIONAL_END_TAGS: &[&str] = &[
    "dd", "dt", "li", "optgroup", "option", "p", "rp", "rt", "tbody", "td", "tfoot", "th", "thead",
    "tr",
];

/// Block-level start tags that close an open `<p>` ("in body", the
/// `p`-in-button-scope rule). The list is the spec's, minus the elements
/// that cannot appear in a paragraph anyway.
const CLOSES_PARAGRAPH: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "search",
    "section",
    "table",
    "ul",
];

/// Whether the start tag ending just before `after_open` carried the XML
/// self-closing marker (`<path … />`). The slash must sit where a marker
/// can sit — right after the tag name, after whitespace, or after a QUOTED
/// attribute value. A slash that ends an UNQUOTED attribute value
/// (`<g a=b/>`) is part of that value, not a marker, and is deliberately
/// not honoured: reading it as self-closing would UNDER-count the tree and
/// reopen the very depth bypass this guard exists to close.
///
/// Only meaningful in FOREIGN content: html5ever acknowledges the
/// self-closing flag in SVG/MathML, and ignores it on ordinary HTML
/// elements (`<div/>` opens a div).
fn is_self_closing(bytes: &[u8], name_end: usize, after_open: usize) -> bool {
    let Some(gt) = after_open.checked_sub(1) else {
        return false;
    };
    if bytes.get(gt) != Some(&b'>') {
        return false; // unterminated tag: ran to EOF, no marker
    }
    let Some(slash) = gt.checked_sub(1) else {
        return false;
    };
    if bytes.get(slash) != Some(&b'/') || slash < name_end {
        return false;
    }
    slash == name_end
        || bytes
            .get(slash - 1)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'"' || *b == b'\'')
}

/// Whether the scan is in HTML parsing context (the `context` stack's top,
/// defaulting to HTML at the document root) — the implied-end-tag rules
/// belong to HTML, never to SVG/MathML foreign content.
fn in_html_context(context: &[bool]) -> bool {
    context.last().copied().unwrap_or(true)
}

/// Whether an opening `name` implicitly closes the element currently on
/// top of the stack. Deliberately a STRICT SUBSET of what the parser
/// closes (only the same-kind sibling, the cell/row pairs, and the
/// block-closes-paragraph rule), so the scan can only ever count MORE
/// depth than the real tree — never less.
///
/// The subset is the INTERSECTION over insertion modes: the scan has no
/// notion of "in select" or "in table", so an arm may pop only what the
/// pinned parser pops in EVERY context. Measured against html5ever
/// 0.39.0 (the version `scraper` 0.27 wires in), not recalled:
///
/// * `<option>` pops an open `option`, NEVER an `optgroup`
///   (`tree_builder/rules.rs:915-923` — in select scope
///   `generate_implied_end_except(optgroup)`, outside it a pop only when
///   the current node is an `option`).
/// * `<optgroup>` pops an open `optgroup` ONLY with a `select` in scope
///   (`rules.rs:930-941` → `generate_implied_end_tags(cursory_implied_end)`,
///   the set `dd dt li option optgroup p rb rp rt rtc` ·
///   `tag_sets.rs:70-71`). Outside a select it pops only an `option`, so
///   `<optgroup>`×N NESTS N levels — popping it here unconditionally
///   under-counted that flood and reopened the very bypass this guard
///   exists to close. The price is a knowing OVER-count inside a real
///   `<select>` (2048 sibling `<optgroup>`s would now read as deep) —
///   the safe direction, and the shape does not exist in the wild.
/// * the table arm pops `thead` too (a `<tbody>` after an open `<thead>`
///   clears the stack back to a table context). Outside a table these
///   start tags create NO node at all (`rules.rs:982-985` — parse error,
///   token ignored), so popping them can never fall below the real tree.
fn closes_implicitly(name: &str, top: &str) -> bool {
    match name {
        "li" => top == "li",
        "dd" | "dt" => matches!(top, "dd" | "dt"),
        "option" | "optgroup" => top == "option",
        "tr" => matches!(top, "td" | "th" | "tr"),
        "td" | "th" => matches!(top, "td" | "th"),
        "tbody" | "tfoot" | "thead" => {
            matches!(top, "td" | "th" | "tr" | "tbody" | "tfoot" | "thead")
        }
        _ => top == "p" && CLOSES_PARAGRAPH.contains(&name),
    }
}

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

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use scraper::Html;

    use super::{
        ExtractMode, MAX_HTML_DEPTH, advance_past, context_kind, guard_depth, is_context_marker,
        scan_name, skip_comment, skip_to_close,
    };

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

    // A long list whose `</li>` are omitted is SHALLOW, not deep: HTML makes
    // that end tag optional and the parser closes each item at the next one.
    // A scan that never closes them counts one level per item, so a menu or a
    // documentation page (sqlite.org writes markup exactly this way) reads as
    // thousands of levels and the whole page is refused — for every mode.
    #[test]
    fn guard_depth_omitted_list_item_close_is_shallow() {
        let mut body = String::from("<html><body><ul>");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<li><a href=\"/{i}\">item {i}</a>");
        }
        body.push_str("</ul></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "sibling list items with omitted end tags nest one level, not N"
        );
    }

    // The same for `<p>`: a block-level start tag closes an open paragraph,
    // so a page written as `<p>text<p>text…` is two levels deep, not N.
    #[test]
    fn guard_depth_omitted_paragraph_close_is_shallow() {
        let mut body = String::from("<html><body>");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<p>paragraph number {i} with some running words");
        }
        body.push_str("</body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "consecutive paragraphs with omitted end tags nest one level, not N"
        );
    }

    // The close tag pops through the items an end tag implies: `</ul>` with
    // open `<li>`s above it closes them, so a page made of many such lists
    // stays flat instead of accumulating a level per list.
    #[test]
    fn guard_depth_close_tag_pops_through_implied_end_tags() {
        let mut body = String::from("<html><body>");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<ul><li>only item of list {i}</ul>");
        }
        body.push_str("</body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "`</ul>` closes the `<li>` it implies, so the lists do not stack"
        );
    }

    // A page of SVG syntax diagrams is thousands of `<path … />` SIBLINGS,
    // not thousands of levels: foreign content acknowledges the XML
    // self-closing marker. Refusing such a page loses it for every mode.
    #[test]
    fn guard_depth_self_closing_svg_shapes_are_flat() {
        let mut body = String::from("<html><body><svg viewBox=\"0 0 10 10\">");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<path d=\"M{i} 0 L{i} 9\"/><circle r=\"1\" />");
        }
        body.push_str("</svg></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "self-closed SVG shapes are siblings, not nesting"
        );
    }

    // The marker is honoured ONLY in foreign content: `<div/>` opens a div
    // in HTML, exactly as html5ever reads it, so a flood of them is deep
    // and must still be refused.
    #[test]
    fn guard_depth_html_self_closing_syntax_still_nests() {
        let body = "<div/>".repeat(MAX_HTML_DEPTH + 5);
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "`<div/>` is not self-closing in HTML — the flood stays deep"
        );
    }

    // A slash that ENDS AN UNQUOTED ATTRIBUTE VALUE is part of the value,
    // not a self-closing marker. Reading it as one would under-count the
    // tree and hand an attacker a depth-guard bypass inside `<svg>`.
    #[test]
    fn guard_depth_unquoted_value_slash_is_not_a_marker() {
        let mut body = String::from("<html><body><svg>");
        for _ in 0..(MAX_HTML_DEPTH + 5) {
            body.push_str("<g a=b/>");
        }
        body.push_str("</svg></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "`<g a=b/>` opens a g — the slash belongs to the attribute value"
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
    // ── closes_implicitly · one test per remaining arm ────────────────
    //
    // The `li` and `p` arms are pinned above. These pin `option`,
    // `optgroup`, `tr`, `td`/`th` and the table-section trio, each in the
    // shape a real page writes PLUS the exact-cap boundary where the arm
    // decides the verdict — so a revert of either spec fix fails here.

    // A `<select>` whose `</option>` are omitted (every CMS writes it that
    // way) is ONE level under its optgroup, not one per choice.
    #[test]
    fn guard_depth_omitted_option_close_is_shallow() {
        let mut body = String::from("<html><body><select><optgroup label=\"g\">");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<option value=\"{i}\">choice number {i}");
        }
        body.push_str("</select></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "sibling options with omitted end tags nest one level, not N"
        );
    }

    // `<option>` closes an open `option` and NOTHING ELSE: an option
    // inside an optgroup is a level DEEPER, and at the cap that level is
    // the verdict. The pre-fix arm (`matches!(top, "option" | "optgroup")`)
    // popped the optgroup, read this document one level shallower, and
    // wrongly ACCEPTED it — an under-count is the bypass direction.
    #[test]
    fn guard_depth_option_does_not_close_its_optgroup() {
        let over = "<div>".repeat(MAX_HTML_DEPTH - 2) + "<select><optgroup><option>choice";
        assert!(
            guard_depth(&over, ExtractMode::Markdown).is_err(),
            "select+optgroup+option is 3 levels over 2046 wrappers — past the cap"
        );
        // One wrapper fewer lands EXACTLY on the cap and is accepted, so
        // the rejection above is that one option level, not a blanket no.
        let at_cap = "<div>".repeat(MAX_HTML_DEPTH - 3) + "<select><optgroup><option>choice";
        assert!(
            guard_depth(&at_cap, ExtractMode::Markdown).is_ok(),
            "the same document one level shallower sits exactly at the cap"
        );
    }

    // `<optgroup>` outside a `<select>` NESTS in html5ever (only an open
    // `option` is popped there · rules.rs:930-941), so a flood of them is
    // genuinely deep and must be REFUSED. The pre-fix arm popped
    // optgroup-on-optgroup unconditionally, held this flood flat at depth
    // 1, and handed the crash class back a bypass.
    #[test]
    fn guard_depth_optgroup_flood_outside_a_select_is_deep() {
        let body = "<optgroup>".repeat(MAX_HTML_DEPTH + 5);
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "optgroups outside a select nest — the flood must be refused"
        );
    }

    // The parser contract the test above rests on — MEASURED, not
    // recalled: html5ever nests `<optgroup>` in body context and flattens
    // it inside a `<select>`, which is exactly why the scan may pop only
    // the `option` case.
    #[test]
    fn html5ever_nests_optgroup_outside_a_select_only() {
        let loose = Html::parse_document(&"<optgroup>".repeat(20));
        let loose_depth = tree_depth(&loose);
        assert!(
            loose_depth >= 20,
            "20 loose optgroups must nest ~20 deep, measured {loose_depth}"
        );
        let selected =
            Html::parse_document(&format!("<select>{}</select>", "<optgroup>".repeat(20)));
        let selected_depth = tree_depth(&selected);
        assert!(
            selected_depth < loose_depth,
            "inside a select the parser pops each optgroup: {selected_depth} vs {loose_depth}"
        );
    }

    // Deepest ancestor chain in a parsed document — the empirical answer
    // to "does this shape nest?", used by the parser-contract pins.
    fn tree_depth(doc: &Html) -> usize {
        doc.tree
            .nodes()
            .map(|node| node.ancestors().count())
            .max()
            .unwrap_or(0)
    }

    // A table written as `<tr>` after `<tr>` with the end tags omitted is
    // one level under the table, not one per row.
    #[test]
    fn guard_depth_omitted_row_close_is_shallow() {
        let body = String::from("<html><body><table>")
            + &"<tr>".repeat(MAX_HTML_DEPTH + 50)
            + "</table></body></html>";
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "a `<tr>` closes the row above it — rows are siblings, not levels"
        );
    }

    // The same for cells: `<td>`/`<th>` close each other, so a row of
    // thousands of cells with omitted end tags stays flat.
    #[test]
    fn guard_depth_omitted_cell_close_is_shallow() {
        let mut body = String::from("<html><body><table><tr>");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(body, "<td>cell {i}<th>header {i}");
        }
        body.push_str("</tr></table></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "a cell start tag closes the cell above it — cells are siblings"
        );
    }

    // The table-section arm must count `thead` among the tops it closes:
    // a `<tbody>` after an open `<thead>` clears the stack back to the
    // table. Omitting `thead` from the matched set left a phantom level
    // open, and at the cap that phantom is the verdict — this document
    // was wrongly REFUSED before the fix.
    #[test]
    fn guard_depth_tbody_closes_an_open_thead() {
        let body = "<div>".repeat(MAX_HTML_DEPTH - 2) + "<table><thead><tbody>";
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "`<tbody>` closes the `<thead>` above it — the table is at the cap, not over it"
        );
    }

    // And the running shape: head/body/foot sections with every end tag
    // omitted, thousands of times, stay flat. Also dies on a revert of the
    // `thead` fix (the sections would stack one level per group).
    #[test]
    fn guard_depth_omitted_table_section_closes_are_shallow() {
        let mut body = String::from("<html><body><table>");
        for i in 0..(MAX_HTML_DEPTH + 50) {
            let _ = write!(
                body,
                "<thead><tr><td>head {i}<tbody><tr><td>body {i}<tfoot><tr><td>foot {i}"
            );
        }
        body.push_str("</table></body></html>");
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_ok(),
            "table sections close one another — they are siblings, not levels"
        );
    }

    // The arms are not a blanket flattening of table markup: a table
    // nested inside its own cell is GENUINELY deep (html5ever nests it
    // too), so the flood must still be refused. Pins that the cap itself
    // is unchanged by the two spec fixes.
    #[test]
    fn guard_depth_nested_tables_through_cells_are_still_refused() {
        assert_eq!(
            MAX_HTML_DEPTH, 2048,
            "the cap is unchanged by the implied-end fixes"
        );
        let body = "<table><tr><td>".repeat(MAX_HTML_DEPTH / 3 + 5);
        assert!(
            guard_depth(&body, ExtractMode::Markdown).is_err(),
            "each nested table adds three real levels — the flood is deep"
        );
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
