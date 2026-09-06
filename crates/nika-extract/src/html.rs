// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HTML modes — `markdown` (htmd) · `text` (DOM walk) · `selector`
//! (CSS select). `article.rs` reuses the htmd leg on
//! readability-cleaned HTML.

use ego_tree::iter::Edge;
use nika_types::extract::ExtractMode;
use scraper::{Html, Node, Selector};

use crate::ExtractError;

mod depth_guard;

pub(crate) use depth_guard::guard_depth;
// The cap itself is read only by the depth-bomb tests of the callers
// (`digest.rs`) — a plain re-export would be an unused import in a
// non-test build.
#[cfg(test)]
pub(crate) use depth_guard::MAX_HTML_DEPTH;

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

/// The tag name the fast path looks for, as bytes.
const NOSCRIPT_TAG: &[u8] = b"noscript";

/// Whether `body` mentions `noscript` AT ALL — one allocation-free,
/// case-insensitive pass (html5ever lowercases tag names, the source
/// need not: `<NOSCRIPT>` is legal markup).
///
/// [`noscript_source`] runs on every article whose primary extraction
/// came back thin, and its `Html::parse_document` is a SECOND full parse
/// of the document — tens of milliseconds on a real page. The
/// overwhelming majority of thin pages carry no `<noscript>` at all, and
/// this scan settles those before the parser is ever built.
fn mentions_noscript(body: &str) -> bool {
    body.as_bytes()
        .windows(NOSCRIPT_TAG.len())
        .any(|window| window.eq_ignore_ascii_case(NOSCRIPT_TAG))
}

/// Below this many characters of markup, a `<noscript>` block is the
/// standard "enable JavaScript" notice, not a page. The server-rendered
/// fallbacks that matter (a forum thread, a product sheet) ship tens of
/// kilobytes; the notices in the wild ship a couple of hundred bytes.
const NOSCRIPT_MIN_SOURCE: usize = 1024;

/// The largest `<noscript>` payload, as SOURCE, when it is substantial
/// markup rather than a notice.
///
/// With scripting ENABLED the parser keeps a `<noscript>` subtree as RAW
/// TEXT: a server-rendered no-JS fallback is therefore invisible to every
/// DOM-walking extractor, however good. Handing that source back lets a
/// caller re-parse it as the markup it is. Returns `None` when no block
/// clears the floor or none looks like markup.
///
/// Scripting IS enabled on this parse, and the whole function rests on
/// it: `scraper::Html::parse_document` hands html5ever
/// `ParseOpts::default()` (scraper-0.27.0 `src/html/mod.rs:80-83`), whose
/// `TreeBuilderOpts::default()` sets `scripting_enabled: true`
/// (html5ever-0.39.0 `src/tree_builder/mod.rs:75`); the in-body arm at
/// `rules.rs:990` then takes `parse_raw_data`. Were that default to flip
/// on a dependency bump, the block would parse as a normal subtree, the
/// `contains('<')` filter below would stop matching and the rescue would
/// silently die — `html5ever_keeps_a_noscript_body_as_raw_text` is the
/// test that would fail first. See also `depth_guard::RAWTEXT_ELEMENTS`,
/// which excludes `noscript` because THIS function re-parses it.
pub(crate) fn noscript_source(body: &str) -> Option<String> {
    if !mentions_noscript(body) {
        return None;
    }
    let doc = Html::parse_document(body);
    let selector = Selector::parse("noscript").ok()?;
    doc.select(&selector)
        .map(|el| el.text().collect::<String>())
        .filter(|raw| raw.len() >= NOSCRIPT_MIN_SOURCE && raw.contains('<'))
        .max_by_key(String::len)
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
    use scraper::{Html, Selector};

    use super::{best_srcset, mentions_noscript, noscript_source, selector, tidy_text};

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

    // ── noscript_source · the fast path ──────────────────────────────
    //
    // The helper decides whether the SECOND full parse happens at all, so
    // it is pinned in both directions: a page with no noscript must never
    // reach the parser, and a page that spells the tag in capitals must.

    #[test]
    fn mentions_noscript_is_false_without_the_tag() {
        assert!(!mentions_noscript(
            "<html><body><article><p>a real page with a script tag \
             and a nosc typo and nothing else</p></article></body></html>"
        ));
        assert!(!mentions_noscript(""));
        // Shorter than the needle — `windows` must not be handed a slice
        // it cannot fill.
        assert!(!mentions_noscript("<p>hi"));
    }

    #[test]
    fn mentions_noscript_is_case_insensitive() {
        assert!(mentions_noscript("<body><noscript>x</noscript></body>"));
        assert!(mentions_noscript("<body><NOSCRIPT>x</NOSCRIPT></body>"));
        assert!(mentions_noscript("<body><NoScript>x</NoScript></body>"));
    }

    #[test]
    fn noscript_source_returns_none_on_the_fast_path() {
        // No noscript anywhere: the helper is false, so `noscript_source`
        // returns without parsing.
        let page = format!(
            "<html><body><div>{}</div></body></html>",
            "<p>filler paragraph with enough text to clear any floor</p>".repeat(40)
        );
        assert!(!mentions_noscript(&page), "the fixture carries no noscript");
        assert!(noscript_source(&page).is_none());
    }

    #[test]
    fn noscript_source_still_reads_an_uppercase_tag() {
        // The fast path must not cost the capitalised form its rescue —
        // a plain `body.contains("noscript")` pre-check would.
        // The prose must not itself spell the tag: a case-SENSITIVE
        // pre-check would then pass for the wrong reason (it did, first
        // draft of this test).
        let payload = "<p>the server rendered fallback carries the entire \
             article inside this hidden block</p>"
            .repeat(20);
        let page = format!("<html><body><NOSCRIPT>{payload}</NOSCRIPT></body></html>");
        let found = noscript_source(&page).expect("the uppercase block is still found");
        assert!(
            found.contains("server rendered fallback"),
            "the block's source must come back, got: {found}"
        );
    }

    // The parser contract `noscript_source` rests on, measured on the
    // pinned scraper/html5ever rather than recalled: with scripting
    // enabled a `<noscript>` body is a single TEXT node holding its own
    // markup as literal characters. A dependency bump that flips the
    // default fails here first, loudly, instead of silently killing the
    // no-JS rescue.
    #[test]
    fn html5ever_keeps_a_noscript_body_as_raw_text() {
        let doc = Html::parse_document(
            "<html><body><noscript><div id=\"f\">hi</div></noscript></body></html>",
        );
        let sel = Selector::parse("noscript").expect("static selector");
        let block = doc.select(&sel).next().expect("the noscript element");
        assert_eq!(
            block.child_elements().count(),
            0,
            "scripting enabled: the block holds no element children"
        );
        let text = block.text().collect::<String>();
        assert!(
            text.contains("<div id=\"f\">"),
            "the block's own markup comes back as literal text, got: {text}"
        );
        // And the div is nowhere in the DOM — it was never parsed.
        let div = Selector::parse("div#f").expect("static selector");
        assert!(
            doc.select(&div).next().is_none(),
            "a rawtext body contributes no elements to the tree"
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
}
