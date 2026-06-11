// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: feed` — RSS · Atom · JSON Feed normalized to the spec's
//! documented object shape via `feed-rs`.

use crate::ExtractError;

pub(crate) fn feed(body: &str) -> Result<serde_json::Value, ExtractError> {
    let parsed = feed_rs::parser::parse(body.as_bytes()).map_err(|e| ExtractError::Feed {
        reason: e.to_string(),
    })?;

    let mut out = serde_json::Map::new();
    if let Some(title) = parsed.title {
        out.insert("title".to_owned(), title.content.into());
    }
    if let Some(description) = parsed.description {
        out.insert("description".to_owned(), description.content.into());
    }
    if let Some(link) = parsed.links.first() {
        out.insert("link".to_owned(), link.href.clone().into());
    }
    if let Some(updated) = parsed.updated {
        out.insert("updated".to_owned(), updated.to_rfc3339().into());
    }

    let items: Vec<serde_json::Value> = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let mut item = serde_json::Map::new();
            if let Some(title) = entry.title {
                item.insert("title".to_owned(), title.content.into());
            }
            if let Some(link) = entry.links.first() {
                item.insert("link".to_owned(), link.href.clone().into());
            }
            if let Some(summary) = entry.summary {
                item.insert("summary".to_owned(), summary.content.into());
            }
            if let Some(published) = entry.published {
                item.insert("published".to_owned(), published.to_rfc3339().into());
            }
            serde_json::Value::Object(item)
        })
        .collect();
    out.insert("items".to_owned(), serde_json::Value::Array(items));

    Ok(serde_json::Value::Object(out))
}
