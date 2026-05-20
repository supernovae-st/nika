// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HREFLANG-001: merge `Link:` header hreflang entries into `extract: metadata` output.
//!
//! Extracted from `nika-engine/src/runtime/executor/fetch.rs` in S14-β.
//!
//! The engine's `fetch.rs` collects `Link:` headers during the retry loop,
//! then these helpers post-process the extracted JSON to merge any
//! `rel="alternate"; hreflang="..."` entries into the `hreflang` field of
//! the metadata object. Pure functions — no I/O, no engine coupling.
//!
//! `merge_link_hreflang` is generic over the error type `E` so it does not
//! couple to `NikaError`: the engine passes `Result<String, NikaError>`,
//! a future verb-crate call site could pass `Result<String, VerbFetchError>`
//! and the function works identically.

use nika_core::ast::extract::ExtractMode;

/// Merge `Link:` header hreflang entries into a JSON-string extract result.
///
/// Used by the `response:` handling path that serializes extract output as
/// a JSON string. Returns the input unchanged when:
/// - `extract_mode` is not `Metadata`
/// - `link_headers` is empty
/// - the input result is an error (passed through)
pub fn merge_link_hreflang<E>(
    result: Result<String, E>,
    extract_mode: Option<ExtractMode>,
    link_headers: &[String],
) -> Result<String, E> {
    if !matches!(extract_mode, Some(ExtractMode::Metadata)) || link_headers.is_empty() {
        return result;
    }
    let result_str = result?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result_str).unwrap_or_default();
    let link_hreflang = nika_extract::parse_link_header_hreflang(link_headers);
    if !link_hreflang.is_empty() {
        let hreflang = parsed.as_object_mut().and_then(|obj| {
            obj.entry("hreflang")
                .or_insert(serde_json::json!([]))
                .as_array_mut()
        });
        if let Some(arr) = hreflang {
            arr.extend(link_hreflang);
            dedup_hreflang(arr);
        }
    }
    Ok(parsed.to_string())
}

/// Merge `Link:` header hreflang entries into an already-parsed JSON Value.
///
/// Used by the `response: full` path that keeps extract output as a typed
/// `serde_json::Value` rather than serializing it as a string.
pub fn merge_link_hreflang_value(
    mut extracted: serde_json::Value,
    extract_mode: Option<ExtractMode>,
    link_headers: &[String],
) -> serde_json::Value {
    if !matches!(extract_mode, Some(ExtractMode::Metadata)) || link_headers.is_empty() {
        return extracted;
    }
    let link_hreflang = nika_extract::parse_link_header_hreflang(link_headers);
    if !link_hreflang.is_empty() {
        if let Some(obj) = extracted.as_object_mut() {
            let arr = obj.entry("hreflang").or_insert(serde_json::json!([]));
            if let Some(vec) = arr.as_array_mut() {
                vec.extend(link_hreflang);
                dedup_hreflang(vec);
            }
        }
    }
    extracted
}

/// Remove exact duplicate hreflang entries (same `lang` + same `href`).
///
/// Different URLs for the same lang are intentionally kept — that is a
/// site-side SEO error the user should be able to see in the output.
fn dedup_hreflang(entries: &mut Vec<serde_json::Value>) {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| {
        let key = (
            entry["lang"].as_str().unwrap_or_default().to_string(),
            entry["href"].as_str().unwrap_or_default().to_string(),
        );
        seen.insert(key)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_extract_mode_is_not_metadata() {
        let result: Result<String, std::io::Error> = Ok(r#"{"title":"x"}"#.to_string());
        let out = merge_link_hreflang(result, Some(ExtractMode::Markdown), &[]);
        assert_eq!(out.unwrap(), r#"{"title":"x"}"#);
    }

    #[test]
    fn passthrough_when_link_headers_empty() {
        let result: Result<String, std::io::Error> = Ok(r#"{"title":"x"}"#.to_string());
        let out = merge_link_hreflang(result, Some(ExtractMode::Metadata), &[]);
        assert_eq!(out.unwrap(), r#"{"title":"x"}"#);
    }

    #[test]
    fn error_is_passed_through() {
        let result: Result<String, std::io::Error> = Err(std::io::Error::other("upstream"));
        let out = merge_link_hreflang(result, Some(ExtractMode::Metadata), &["x".to_string()]);
        assert!(out.is_err());
    }

    #[test]
    fn dedup_removes_exact_duplicates_keeps_conflicts() {
        let mut entries = vec![
            serde_json::json!({"lang": "en", "href": "https://a.example/en"}),
            serde_json::json!({"lang": "en", "href": "https://a.example/en"}),
            serde_json::json!({"lang": "fr", "href": "https://a.example/fr"}),
            // Different URL, same lang — kept as SEO error signal.
            serde_json::json!({"lang": "en", "href": "https://a.example/en-US"}),
        ];
        dedup_hreflang(&mut entries);
        assert_eq!(entries.len(), 3);
    }
}
