// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: article` — a THREE-stage extraction cascade (the failure modes
//! are decorrelated, so the cascade beats any single extractor — the trick
//! Trafilatura's benchmarks proved out, DOI 10.18653/v1/2021.acl-demo.15):
//!
//! 1. **Rule cascade** (`zones.rs`) — Trafilatura-grade zone targeting +
//!    boilerplate prune, PAGE-TYPE aware (`page_type.rs`). Tops every
//!    2024-2026 benchmark (rs-trafilatura 0.970 vs readability 0.947 vs
//!    `dom_smoothie` 0.865 · `ScrapingHub` Feb-2026; WCXB arXiv:2605.21097).
//!    The precise win on pages with semantic markup. Abstains (→ stage 2)
//!    when no semantic container is found.
//! 2. **Readability** (`dom_smoothie`) — the scoring approach for
//!    markup-poor pages (best MEDIAN F1, SIGIR'23 DOI 10.1145/3539618.3591920).
//! 3. **Boilerpipe Algorithm 2** (`blocks.rs` · WSDM 2010) — the
//!    shallow-text-density recall floor for div-soup pages where both
//!    DOM-structure extractors starve.
//!
//! Spec contract unchanged: "article body only · Markdown string".

use nika_types::extract::ExtractMode;

use crate::ExtractError;

/// Only the universally-dead subtrees — the rule cascade / readability
/// already removed nav/sidebars/ads; stripping more would eat content.
const ARTICLE_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

/// Below this many characters the result counts as THIN and the next
/// stage fires (Trafilatura's `MIN_EXTRACTED_SIZE`: a real article body is
/// rarely shorter).
const THIN_THRESHOLD: usize = 250;

pub(crate) fn article(body: &str, base: Option<&str>) -> Result<serde_json::Value, ExtractError> {
    let page_type = crate::page_type::classify(body, base);
    let primary = cascade(body, base, page_type);
    if primary.as_ref().is_ok_and(|value| !is_thin(value)) {
        return primary;
    }
    // Every stage starved on the served markup. A JS-rendered shell can
    // still carry its whole page in a server-rendered `<noscript>`
    // fallback, which the HTML parser keeps as RAW TEXT (see
    // `html::noscript_source`) — re-run the cascade on that source, with
    // the page type classified on the OUTER document (the URL and the
    // head are the classification signals, and the fallback has neither).
    // Gated on a THIN primary: a page that extracted cleanly is never
    // overridden by its own "enable JavaScript" notice.
    if let Some(inner) = crate::html::noscript_source(body)
        && let Ok(rescued) = cascade(&inner, base, page_type)
        && !is_thin(&rescued)
    {
        return Ok(rescued);
    }
    primary
}

/// The three-stage cascade over one piece of markup.
fn cascade(
    body: &str,
    base: Option<&str>,
    page_type: crate::page_type::PageType,
) -> Result<serde_json::Value, ExtractError> {
    // Stage 1 — the rule cascade (page-type aware). The precise primary.
    if let Some(zone_html) = crate::zones::rule_content(body, page_type)
        && let Ok(md) =
            crate::html::convert_markdown(&zone_html, ExtractMode::Article, ARTICLE_SKIP_TAGS)
        && !is_thin(&md)
    {
        return Ok(md);
    }

    // Stage 2 — readability scoring (markup-poor pages).
    match readability(body, base) {
        Ok(value) if !is_thin(&value) => Ok(value),
        // Stage 3 — the decorrelated boilerpipe recall floor. A page where
        // ALL THREE starve yields whatever boilerpipe finds — honest
        // emptiness beats fabricated content.
        thin_or_err => {
            let fallback = crate::blocks::boilerpipe_content(body);
            // Compare on TRIMMED length everywhere (the same metric
            // `is_thin` uses) — htmd emits leading/trailing whitespace, so
            // an untrimmed `.len()` could let a whitespace-padded near-empty
            // readability result outrank real boilerpipe prose, or pass the
            // threshold on padding alone.
            if fallback.trim().len() >= THIN_THRESHOLD {
                return Ok(serde_json::Value::String(fallback));
            }
            // Keep the RICHER of the two thin results (or the original
            // readability error when it produced nothing at all).
            match thin_or_err {
                Ok(value)
                    if value.as_str().map_or(0, |s| s.trim().len()) >= fallback.trim().len() =>
                {
                    Ok(value)
                }
                Ok(_) | Err(_) if !fallback.is_empty() => Ok(serde_json::Value::String(fallback)),
                other => other,
            }
        }
    }
}

/// Stage 1: `dom_smoothie` readability → Markdown.
fn readability(body: &str, base: Option<&str>) -> Result<serde_json::Value, ExtractError> {
    let html = |reason: String| ExtractError::Html {
        mode: ExtractMode::Article,
        reason,
    };
    let mut readability = dom_smoothie::Readability::new(body, base, None)
        .map_err(|e| html(format!("readability init: {e:?}")))?;
    let parsed = readability
        .parse()
        .map_err(|e| html(format!("readability parse: {e:?}")))?;
    crate::html::convert_markdown(
        parsed.content.as_ref(),
        ExtractMode::Article,
        ARTICLE_SKIP_TAGS,
    )
}

fn is_thin(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .is_none_or(|s| s.trim().len() < THIN_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    // ── is_thin · the THIN gate (directly unit-testable) ──────────────
    //
    // `is_thin(v)` = v is NOT a string, OR its TRIMMED length is strictly
    // below THIN_THRESHOLD (250). These cases pin every operator the
    // mutation run left alive on lines 95-98.

    /// `THIN_THRESHOLD` is the canonical 250-char floor — make the boundary
    /// arithmetic explicit so a silent constant drift fails loudly here.
    #[test]
    fn thin_threshold_is_250() {
        assert_eq!(THIN_THRESHOLD, 250);
    }

    #[test]
    fn is_thin_true_for_empty_and_short_strings() {
        // Empty string: trimmed len 0 < 250 → THIN.
        // (Kills line-96 `is_thin → false`: a `false`-returner fails here.)
        assert!(is_thin(&json!("")));

        // Exactly THIN_THRESHOLD - 1 (249) characters → still THIN.
        // (Boundary for line-98 `<`: 249 < 250 holds; the `==` and `>`
        // mutants both make this case wrongly return `false`.)
        let just_under = "x".repeat(THIN_THRESHOLD - 1);
        assert_eq!(just_under.len(), 249);
        assert!(is_thin(&json!(just_under)));
    }

    #[test]
    fn is_thin_false_at_threshold_exactly() {
        // Exactly THIN_THRESHOLD (250) characters → NOT thin.
        // (Kills line-96 `is_thin → true`: a `true`-returner fails here.
        //  Kills line-98 `< → <=`: 250 <= 250 would wrongly report THIN.)
        let at_threshold = "y".repeat(THIN_THRESHOLD);
        assert_eq!(at_threshold.len(), 250);
        assert!(!is_thin(&json!(at_threshold)));
    }

    #[test]
    fn is_thin_true_for_non_string_values() {
        // A non-string value has no `as_str()` → `is_none_or` → THIN.
        // (Kills line-96 `is_thin → false` on the non-string path, which
        //  the string-only cases above cannot reach.)
        assert!(is_thin(&Value::Null));
        assert!(is_thin(&json!(42)));
        assert!(is_thin(&json!(true)));
        assert!(is_thin(&json!([1, 2, 3])));
    }

    #[test]
    fn is_thin_measures_trimmed_length_not_raw() {
        // Raw length ≥ 250 but TRIMMED length < 250 → THIN. Without the
        // `.trim()`, the raw 260-char length would clear the floor and
        // wrongly report NOT-thin; with it, the 240 visible chars stay
        // below 250. Also re-pins line-98 `<` on the trimmed metric.
        let padded = format!("{}{}{}", " ".repeat(10), "z".repeat(240), " ".repeat(10));
        assert_eq!(padded.len(), 260);
        assert_eq!(padded.trim().len(), 240);
        assert!(is_thin(&json!(padded)));
    }

    // ── article · the 3-stage cascade ─────────────────────────────────
    //
    // Reliability lever: `readability(body, base)` forwards `base` to
    // `dom_smoothie::Readability::new`, which returns
    // `Err(BadDocumentURL)` when `base` is NOT an absolute URL. Passing a
    // non-absolute base therefore makes Stage 2 deterministically ERROR —
    // no dependence on dom_smoothie's content scoring. With Stage 2 fixed
    // to `Err`, the Stage 3 boilerpipe fallback (our own deterministic
    // `blocks::boilerpipe_content`) drives the observable output.
    const NON_ABSOLUTE_BASE: Option<&str> = Some("not-an-absolute-url");

    /// Stage 1 — a real `<article>` body returns the rule-cascade markdown
    /// without ever consulting readability/boilerpipe. Anchors the happy
    /// path: the prose survives end-to-end as a Markdown string.
    #[test]
    fn stage1_rich_article_returns_markdown_prose() {
        let prose = "This is a genuine article body with plenty of running \
             words so that the rule cascade comfortably clears the THIN \
             floor and returns Markdown straight from stage one without \
             ever falling through to readability or the boilerpipe recall \
             floor underneath it all here."
            .to_string();
        let body =
            format!("<html><body><article><h1>Heading</h1><p>{prose}</p></article></body></html>");

        let out = article(&body, Some("https://example.com/")).expect("rich article extracts");
        let md = out.as_str().expect("article returns a Markdown string");
        assert!(
            md.contains("genuine article body"),
            "stage-1 markdown must carry the article prose, got: {md}"
        );
        assert!(
            md.trim().len() >= THIN_THRESHOLD,
            "stage-1 result is not thin"
        );
    }

    /// Stage 3 RICH — no semantic container (Stage 1 abstains) + a
    /// non-absolute base (Stage 2 errors) ⇒ the boilerpipe fallback owns
    /// the result, and a ≥250-char fallback is returned verbatim. Pins the
    /// `fallback.trim().len() >= THIN_THRESHOLD` decision (line 59) as the
    /// gate that emits the rich recall floor.
    #[test]
    fn stage3_rich_boilerpipe_fallback_is_returned() {
        // One bare <div> of long prose — no <article>/<main>/role/class, so
        // the rule cascade finds no zone and Stage 1 abstains.
        let prose = "the boilerpipe recall floor recovers this long running \
             paragraph of prose because the rule cascade abstains on this \
             markup poor div soup and readability is forced to error out by \
             the non absolute base url we deliberately pass into the article \
             cascade here so only the shallow text density extractor remains \
             standing to carry the body forward in full intact";
        let body = format!("<html><body><div>{prose}</div></body></html>");

        let out = article(&body, NON_ABSOLUTE_BASE).expect("boilerpipe fallback recovers prose");
        let md = out.as_str().expect("fallback is a string");
        assert!(
            md.contains("boilerpipe recall floor recovers"),
            "rich boilerpipe fallback must be returned, got: {md}"
        );
        assert!(
            md.trim().len() >= THIN_THRESHOLD,
            "fallback cleared the rich floor"
        );
    }

    /// Stage 3 THIN-but-non-empty — Stage 1 abstains, Stage 2 errors, and
    /// boilerpipe yields a SHORT (<250) yet NON-EMPTY string. The correct
    /// cascade still returns that short content (the `!fallback.is_empty()`
    /// arm, line 70). The `delete !` / guard→`false` mutants on line 70
    /// would instead drop to `other => other` and surface the Stage-2
    /// error — so this asserts `Ok` + the prose, killing those mutants.
    #[test]
    fn stage3_short_nonempty_boilerpipe_beats_the_error() {
        // ~11 running words in a lonely div → boilerpipe's degenerate
        // floor keeps it (non-empty) but it is far under 250 chars.
        let body = "<html><body>\
             <div>eleven plain running words sit inside this lonely div block</div>\
             </body></html>";

        let out =
            article(body, NON_ABSOLUTE_BASE).expect("short non-empty fallback wins over the error");
        let md = out.as_str().expect("fallback is a string");
        assert!(
            md.contains("eleven plain running words"),
            "short non-empty boilerpipe must be returned, not the error, got: {md}"
        );
        assert!(
            md.trim().len() < THIN_THRESHOLD,
            "this fallback is intentionally THIN"
        );
    }

    /// The `<noscript>` rescue — a JS-rendered shell whose server-side
    /// fallback carries the whole page. With scripting enabled (the HTML
    /// parser's default) the `<noscript>` subtree is RAW TEXT, so all
    /// three stages starve on the shell; the rescue re-parses that source
    /// and returns the prose. Without it the page extracts to nothing.
    #[test]
    fn noscript_fallback_rescues_a_starved_shell() {
        let prose = "the server rendered fallback carries the entire \
             discussion thread inside a noscript block because the page \
             itself is an empty javascript shell that renders nothing at \
             all for a parser and every dom walking extractor starves on \
             it unless the noscript source is parsed as the markup it is";
        let posts = format!("<p>{prose}</p>").repeat(4);
        let body = format!(
            "<html><body><div id=\"main-outlet\"></div>\
             <noscript><div id=\"content\">{posts}</div></noscript>\
             </body></html>"
        );

        let out = article(&body, Some("https://example.com/t/thread/1"))
            .expect("the noscript fallback carries the page");
        let md = out.as_str().expect("rescue returns a Markdown string");
        assert!(
            md.contains("server rendered fallback carries"),
            "the noscript source must be extracted, got: {md}"
        );
    }

    /// The rescue is GATED on a starved primary: a page with a real body
    /// AND a boilerplate `<noscript>` notice keeps its body. Pins that the
    /// rescue can never overwrite a rich extraction.
    #[test]
    fn noscript_never_overrides_a_rich_primary() {
        let prose = "this genuine article body is long enough to clear the \
             thin floor on its own so the cascade returns it directly and \
             the noscript notice underneath must never replace it no \
             matter how much markup that notice happens to carry with it \
             and the extra running words here exist only to put the body \
             comfortably over the two hundred and fifty character floor";
        let filler = "<p>enable javascript to view this site correctly</p>".repeat(40);
        let body = format!(
            "<html><body><article><p>{prose}</p></article>\
             <noscript><div>{filler}</div></noscript></body></html>"
        );

        let out = article(&body, Some("https://example.com/")).expect("rich article extracts");
        let md = out.as_str().expect("article returns a Markdown string");
        assert!(
            md.contains("genuine article body"),
            "the rich primary must survive, got: {md}"
        );
        assert!(
            !md.contains("enable javascript"),
            "the noscript notice must not leak into a rich result, got: {md}"
        );
    }

    /// All-thin terminus — an empty body: Stage 1 abstains, Stage 2 errors
    /// (non-absolute base), and boilerpipe returns "" (empty). With an
    /// EMPTY fallback, the correct cascade exhausts every rescue arm and
    /// surfaces the Stage-2 error verbatim (`other => other`, line 71).
    ///
    /// This single assertion kills three survivors at once:
    ///   • line 70 guard `→ true` / `delete !`  → would return `Ok("")`
    ///   • line 79 `readability → Ok(Default::default())` → readability
    ///     never errors, the `Null` default flows to line 66's
    ///     `0 >= 0` arm and returns `Ok(Null)`.
    /// All three replace the expected `Err` with an `Ok`, so `is_err()`
    /// distinguishes the real code from every one of them.
    #[test]
    fn empty_body_surfaces_the_readability_error() {
        let body = "<html><body></body></html>";
        let out = article(body, NON_ABSOLUTE_BASE);
        assert!(
            out.is_err(),
            "empty body + empty fallback must surface the stage-2 error, got: {out:?}"
        );
    }
}
