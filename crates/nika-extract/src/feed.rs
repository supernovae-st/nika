// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `mode: feed` — RSS · Atom · JSON Feed normalized to the spec's
//! documented object shape via `feed-rs`.
//!
//! BYTES in, not `&str`: feed-rs owns charset detection (the XML
//! prolog's `encoding=` + BOM). Pre-transcoding the body and handing
//! over a `&str` would leave a STALE prolog declaring the old charset
//! — feed-rs would re-decode the UTF-8 bytes as Latin-1 and mojibake
//! every non-ASCII char (review lens 2 · P3-7).

use crate::ExtractError;

/// Parse a feed from RAW response bytes (the builtin's entry — bypasses
/// the charset-aware text decode the other HTML modes use).
///
/// # Errors
///
/// [`ExtractError::Feed`] when the bytes are not a parseable RSS /
/// Atom / JSON feed.
pub fn feed_from_bytes(body: &[u8]) -> Result<serde_json::Value, ExtractError> {
    let parsed = feed_rs::parser::parse(body).map_err(|e| ExtractError::Feed {
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
            // feed-rs guarantees an id (Atom requires one; for RSS it
            // falls back to guid/link/title-hash) — always present.
            item.insert("id".to_owned(), entry.id.into());
            if let Some(title) = entry.title {
                item.insert("title".to_owned(), title.content.into());
            }
            // First link only: feed-rs orders rel=alternate first, and
            // multi-<link> channels/entries are vanishingly rare in the
            // wild — the spec shape wants ONE link per item.
            if let Some(link) = entry.links.first() {
                item.insert("link".to_owned(), link.href.clone().into());
            }
            if let Some(summary) = entry.summary {
                item.insert("summary".to_owned(), summary.content.into());
            }
            if let Some(published) = entry.published {
                item.insert("published".to_owned(), published.to_rfc3339().into());
            }
            if !entry.categories.is_empty() {
                let cats: Vec<serde_json::Value> = entry
                    .categories
                    .into_iter()
                    .map(|c| c.term.into())
                    .collect();
                item.insert("categories".to_owned(), serde_json::Value::Array(cats));
            }
            serde_json::Value::Object(item)
        })
        .collect();
    out.insert("items".to_owned(), serde_json::Value::Array(items));

    Ok(serde_json::Value::Object(out))
}

/// The `&str` path (`extract()` API uniformity) — delegates to the
/// bytes parser.
pub(crate) fn feed(body: &str) -> Result<serde_json::Value, ExtractError> {
    feed_from_bytes(body.as_bytes())
}
