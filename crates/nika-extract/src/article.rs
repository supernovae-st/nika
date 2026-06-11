// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: article` — Readability extraction (`dom_smoothie`) composed
//! with the shared HTML→Markdown leg. The spec contract: "article body
//! only · Markdown string" — readability strips the chrome, htmd does
//! the formatting.

use nika_types::extract::ExtractMode;

use crate::ExtractError;

/// Only the universally-dead subtrees — readability already removed
/// nav/sidebars/ads; stripping more would eat article content.
const ARTICLE_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

pub(crate) fn article(body: &str, base: Option<&str>) -> Result<serde_json::Value, ExtractError> {
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
