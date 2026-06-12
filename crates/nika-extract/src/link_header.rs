// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RFC 8288 `Link:` header parsing — pure, total.
//!
//! `<https://ex.com/fr>; rel="alternate"; hreflang="fr", <…>; rel=next`
//! → typed entries. Commas INSIDE `<…>` are URL bytes, not separators
//! (query strings carry them); parameters are `;`-separated `key=value`
//! with optional quoting, keys case-insensitive. The builtin enriches
//! `mode: metadata` with the `alternates` (hreflang) entries.

/// One parsed `Link:` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LinkEntry {
    /// The target inside `<…>` (verbatim — resolution is the caller's).
    pub href: String,
    /// The `rel` parameter (lowercased · may carry multiple space-
    /// separated relation types per the RFC).
    pub rel: Option<String>,
    /// The `hreflang` parameter (as sent).
    pub hreflang: Option<String>,
    /// The `type` parameter (media type · as sent).
    pub media_type: Option<String>,
}

impl LinkEntry {
    /// True when `rel` contains the given relation type (the RFC allows
    /// `rel="alternate stylesheet"` — word membership, not equality).
    #[must_use]
    pub fn has_rel(&self, relation: &str) -> bool {
        self.rel
            .as_deref()
            .is_some_and(|r| r.split_ascii_whitespace().any(|w| w == relation))
    }
}

/// Parse one `Link:` header VALUE (which may carry several
/// comma-separated entries — the form proxies produce when combining).
/// Malformed segments are skipped, never an error: headers are
/// remote-controlled and a broken third entry must not void the first.
#[must_use]
pub fn parse_link_header(value: &str) -> Vec<LinkEntry> {
    split_top_level_commas(value)
        .into_iter()
        .filter_map(parse_entry)
        .collect()
}

/// Split on commas OUTSIDE `<…>` brackets (a URL may contain commas).
fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in value.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < value.len() {
        parts.push(&value[start..]);
    }
    parts
}

/// Parse one `<href>; key=value; …` segment.
fn parse_entry(segment: &str) -> Option<LinkEntry> {
    let segment = segment.trim();
    let open = segment.find('<')?;
    let close = segment[open..].find('>')? + open;
    let href = segment.get(open + 1..close)?.to_owned();
    if href.is_empty() {
        return None;
    }

    let mut entry = LinkEntry {
        href,
        rel: None,
        hreflang: None,
        media_type: None,
    };
    for param in segment[close + 1..].split(';') {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.trim().to_ascii_lowercase().as_str() {
            // First occurrence wins (the RFC says repeated `rel` MUST
            // be ignored).
            "rel" if entry.rel.is_none() => entry.rel = Some(value.to_ascii_lowercase()),
            "hreflang" if entry.hreflang.is_none() => entry.hreflang = Some(value.to_owned()),
            "type" if entry.media_type.is_none() => entry.media_type = Some(value.to_owned()),
            _ => {}
        }
    }
    Some(entry)
}

/// The `alternates` projection for `mode: metadata`: every
/// `rel=alternate` entry carrying an `hreflang`, as
/// `[{lang, href}, …]` (empty when none — the caller omits the key).
#[must_use]
pub fn alternates(entries: &[LinkEntry]) -> Vec<serde_json::Value> {
    entries
        .iter()
        .filter(|e| e.has_rel("alternate"))
        .filter_map(|e| {
            let lang = e.hreflang.as_deref()?;
            Some(serde_json::json!({ "lang": lang, "href": e.href }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_hreflang_cluster() {
        let header = r#"<https://ex.com/fr/>; rel="alternate"; hreflang="fr", <https://ex.com/de/>; rel=alternate; hreflang=de"#;
        let entries = parse_link_header(header);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].href, "https://ex.com/fr/");
        assert_eq!(entries[0].hreflang.as_deref(), Some("fr"));
        assert!(entries[0].has_rel("alternate"));
        // Unquoted parameter form parses identically.
        assert_eq!(entries[1].hreflang.as_deref(), Some("de"));

        let alts = alternates(&entries);
        assert_eq!(alts.len(), 2);
        assert_eq!(alts[1]["lang"], "de");
        assert_eq!(alts[1]["href"], "https://ex.com/de/");
    }

    #[test]
    fn commas_inside_brackets_are_url_bytes() {
        let header = r#"<https://ex.com/?ids=1,2,3>; rel="next", <https://ex.com/x>; rel="prev""#;
        let entries = parse_link_header(header);
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].href, "https://ex.com/?ids=1,2,3");
        assert!(entries[0].has_rel("next"));
        assert!(entries[1].has_rel("prev"));
    }

    #[test]
    fn multi_word_rel_is_word_matched_and_repeats_ignored() {
        let entries =
            parse_link_header(r#"<https://e.com/s.css>; rel="alternate stylesheet"; rel=next"#);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].has_rel("alternate"), "word membership");
        assert!(entries[0].has_rel("stylesheet"));
        assert!(!entries[0].has_rel("next"), "repeated rel ignored per RFC");
        assert!(!entries[0].has_rel("alt"), "no substring matching");
    }

    #[test]
    fn malformed_segments_are_skipped_not_fatal() {
        let header =
            "<https://good.example/>; rel=canonical, no-brackets-here; rel=next, <>; rel=empty";
        let entries = parse_link_header(header);
        assert_eq!(entries.len(), 1, "only the well-formed entry: {entries:?}");
        assert_eq!(entries[0].href, "https://good.example/");
    }

    #[test]
    fn type_parameter_lands_in_media_type() {
        let entries =
            parse_link_header(r#"<https://e.com/feed>; rel=alternate; type="application/rss+xml""#);
        assert_eq!(
            entries[0].media_type.as_deref(),
            Some("application/rss+xml")
        );
        assert!(
            alternates(&entries).is_empty(),
            "no hreflang → no alternate"
        );
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            /// Total over arbitrary header garbage — never panics.
            #[test]
            fn parser_is_total(header in ".{0,300}") {
                let _ = parse_link_header(&header);
            }
        }
    }
}
