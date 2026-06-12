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
            // First author's name (singular · matching `metadata` mode's
            // `author` and the first-link precedent above). Multi-author
            // entries exist but the normalized shape carries one; an
            // empty name is omitted (JSON absence over a blank string).
            if let Some(name) = entry
                .authors
                .into_iter()
                .map(|p| p.name)
                .find(|n| !n.trim().is_empty())
            {
                item.insert("author".to_owned(), name.into());
            }
            if let Some(summary) = entry.summary {
                item.insert("summary".to_owned(), summary.content.into());
            }
            // The FULL item body — RSS `<content:encoded>` · Atom
            // `<content>` · JSON Feed `content_html`/`content_text`. This
            // is the high-value field for content pipelines (the whole
            // post, not just the summary blurb); surfaced raw (the
            // caller runs it through `markdown`/`text` if they want it
            // cleaned · same raw-passthrough discipline as `summary`).
            // Bounded by the response body cap like everything else.
            if let Some(body) = entry.content.and_then(|c| c.body) {
                item.insert("content".to_owned(), body.into());
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

#[cfg(test)]
mod tests {
    use super::feed;

    // RSS 2.0 with the two Dublin-Core/content extensions a real feed
    // carries: `<dc:creator>` (author) + `<content:encoded>` (full body).
    const RSS_RICH: &str = r#"<?xml version="1.0"?>
<rss version="2.0"
     xmlns:content="http://purl.org/rss/1.0/modules/content/"
     xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>Demo Feed</title>
    <item>
      <title>First post</title>
      <link>https://example.com/post-1</link>
      <dc:creator>Jane Doe</dc:creator>
      <description>Short summary blurb</description>
      <content:encoded><![CDATA[<p>The <b>full</b> post body.</p>]]></content:encoded>
    </item>
  </channel>
</rss>"#;

    const ATOM_RICH: &str = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Demo</title>
  <entry>
    <title>Atom entry</title>
    <link href="https://example.com/atom-1"/>
    <author><name>John Smith</name></author>
    <summary>blurb</summary>
    <content type="html">&lt;p&gt;Atom full content&lt;/p&gt;</content>
  </entry>
</feed>"#;

    #[test]
    fn rss_surfaces_author_and_full_content_distinct_from_summary() {
        let out = feed(RSS_RICH).expect("rss parses");
        let item = &out["items"][0];
        assert_eq!(item["author"], "Jane Doe", "dc:creator → author: {out}");
        // content:encoded is the FULL body, NOT the summary — the two are
        // distinct fields and content carries the markup.
        let content = item["content"].as_str().unwrap_or_default();
        assert!(content.contains("full"), "content:encoded → content: {out}");
        assert_ne!(item["content"], item["summary"], "content ≠ summary: {out}");
    }

    #[test]
    fn atom_surfaces_author_and_content() {
        let out = feed(ATOM_RICH).expect("atom parses");
        let item = &out["items"][0];
        assert_eq!(item["author"], "John Smith", "atom author/name: {out}");
        assert!(
            item["content"]
                .as_str()
                .is_some_and(|c| c.contains("Atom full content")),
            "atom <content> → content: {out}"
        );
    }

    #[test]
    fn missing_author_and_content_are_omitted_not_null() {
        // A minimal item has neither — absence over null (stable shape).
        let bare = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>T</title>
  <item><title>x</title><link>https://e.test/1</link></item>
</channel></rss>"#;
        let out = feed(bare).expect("parses");
        let item = out["items"][0].as_object().expect("item object");
        assert!(!item.contains_key("author"), "no author key: {out}");
        assert!(!item.contains_key("content"), "no content key: {out}");
    }
}
