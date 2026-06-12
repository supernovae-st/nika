// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: article` — a two-stage extraction cascade:
//!
//! 1. **Readability** (`dom_smoothie`) → the shared HTML→Markdown leg.
//!    The robust general case (best median F1 across the SIGIR'23
//!    benchmark, DOI 10.1145/3539618.3591920).
//! 2. **Boilerpipe Algorithm 2 fallback** (`blocks.rs` · WSDM 2010)
//!    when stage 1 errors OR yields THIN content (< the threshold):
//!    shallow text features need no `<p>`/class signals, so they
//!    recover prose on the div-soup pages where readability starves —
//!    the two failure modes are decorrelated (the cascade trick
//!    Trafilatura's benchmarks proved out, DOI
//!    10.18653/v1/2021.acl-demo.15).
//!
//! Spec contract unchanged: "article body only · Markdown string".

use nika_types::extract::ExtractMode;

use crate::ExtractError;

/// Only the universally-dead subtrees — readability already removed
/// nav/sidebars/ads; stripping more would eat article content.
const ARTICLE_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

/// Below this many characters the readability result counts as THIN
/// and the fallback fires (the survey's empirical floor: a real
/// article body is rarely shorter).
const THIN_THRESHOLD: usize = 250;

pub(crate) fn article(body: &str, base: Option<&str>) -> Result<serde_json::Value, ExtractError> {
    match readability(body, base) {
        Ok(value) if !is_thin(&value) => Ok(value),
        // Thin or failed → the decorrelated fallback. A page where BOTH
        // starve yields whatever boilerpipe's recall floor finds —
        // honest emptiness beats fabricated content.
        thin_or_err => {
            let fallback = crate::blocks::boilerpipe_content(body);
            if fallback.len() >= THIN_THRESHOLD {
                return Ok(serde_json::Value::String(fallback));
            }
            // Keep the RICHER of the two thin results (or the original
            // readability error when it produced nothing at all).
            match thin_or_err {
                Ok(value) if value.as_str().map_or(0, str::len) >= fallback.len() => Ok(value),
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
