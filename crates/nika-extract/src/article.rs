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
    // Stage 1 — the rule cascade (page-type aware). The precise primary.
    let page_type = crate::page_type::classify(body, base);
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
