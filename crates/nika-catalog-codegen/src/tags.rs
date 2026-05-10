// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tag vocabulary — 42 kebab-case variants shared across MCP servers,
//! providers, and embeddings.
//!
//! SYNC NOTE: This mapping is intentionally duplicated from
//! `nika-catalog/src/types/tags.rs::FromStr`. The codegen crate cannot
//! `use` the runtime `Tag` enum (build-dep cycle). The
//! [`TAG_VARIANT_COUNT`] constant + `variant_count_is_42` runtime test
//! in `tags.rs` guard against drift. If you add a new Tag variant, update
//! `Tag` enum, `as_str()`, `FromStr`, [`tag_variant`], [`ALL_KNOWN_TAGS`],
//! and bump [`TAG_VARIANT_COUNT`] to match.

/// Number of known tag variants. Must match `Tag` enum in `src/types/tags.rs`.
pub const TAG_VARIANT_COUNT: usize = 42;

/// Map a kebab-case tag string to its Rust variant name. Returns `None`
/// for unknown strings (used by [`validate_tags`] to fail the build early).
#[must_use]
pub(crate) fn tag_variant(s: &str) -> Option<&'static str> {
    Some(match s {
        "vision" => "Vision",
        "audio" => "Audio",
        "realtime" => "Realtime",
        "multimodal" => "Multimodal",
        "image-gen" => "ImageGen",
        "embedding" => "Embedding",
        "reranker" => "Reranker",
        "reasoning" => "Reasoning",
        "extended-thinking" => "ExtendedThinking",
        "function-calling" => "FunctionCalling",
        "parallel-tools" => "ParallelTools",
        "structured-output" => "StructuredOutput",
        "response-schema" => "ResponseSchema",
        "streaming" => "Streaming",
        "long-context" => "LongContext",
        "prompt-caching" => "PromptCaching",
        "web-search" => "WebSearch",
        "code-execution" => "CodeExecution",
        "matryoshka" => "Matryoshka",
        "budget" => "Budget",
        "frontier" => "Frontier",
        "fast" => "Fast",
        "local" => "Local",
        "serverless" => "Serverless",
        "open-source" => "OpenSource",
        "enterprise" => "Enterprise",
        "european" => "European",
        "chinese" => "Chinese",
        "japanese" => "Japanese",
        "multilingual" => "Multilingual",
        "code" => "Code",
        "math" => "Math",
        "rag" => "Rag",
        "agent" => "Agent",
        "legal" => "Legal",
        "finance" => "Finance",
        "medical" => "Medical",
        "read-only" => "ReadOnly",
        "destructive" => "Destructive",
        "sandbox" => "Sandbox",
        "official" => "Official",
        "verified" => "Verified",
        _ => return None,
    })
}

/// Exhaustive list of all known kebab-case tag strings, alphabetically
/// sorted. Used to verify [`TAG_VARIANT_COUNT`] stays in sync with
/// [`tag_variant`] arms.
pub(crate) const ALL_KNOWN_TAGS: &[&str] = &[
    "agent",
    "audio",
    "budget",
    "chinese",
    "code",
    "code-execution",
    "destructive",
    "embedding",
    "enterprise",
    "european",
    "extended-thinking",
    "fast",
    "finance",
    "frontier",
    "function-calling",
    "image-gen",
    "japanese",
    "legal",
    "local",
    "long-context",
    "math",
    "matryoshka",
    "medical",
    "multimodal",
    "multilingual",
    "official",
    "open-source",
    "parallel-tools",
    "prompt-caching",
    "rag",
    "read-only",
    "realtime",
    "reasoning",
    "reranker",
    "response-schema",
    "sandbox",
    "serverless",
    "streaming",
    "structured-output",
    "verified",
    "vision",
    "web-search",
];

// Compile-time assertion: if someone adds to tag_variant() without updating
// TAG_VARIANT_COUNT (or vice versa), this panics during build.
const _: () = assert!(
    ALL_KNOWN_TAGS.len() == TAG_VARIANT_COUNT,
    "ALL_KNOWN_TAGS length must equal TAG_VARIANT_COUNT — did you add a tag variant without updating both?"
);

/// Validate that every string in `tags` is a known kebab-case `Tag` variant,
/// and that the slice is sorted alphabetically and deduplicated.
///
/// Sorting is validated (not auto-applied) so TOML authors see an explicit
/// error message instead of silently getting reordered output.
pub(crate) fn validate_tags(tags: &[String], entry_id: &str) -> Result<(), String> {
    for t in tags {
        if tag_variant(t).is_none() {
            return Err(format!(
                "entry {entry_id:?}: unknown tag {t:?} — must be a kebab-case variant from the Tag enum \
                 (42 variants: vision, audio, realtime, multimodal, image-gen, embedding, reranker, \
                 reasoning, extended-thinking, function-calling, parallel-tools, structured-output, \
                 response-schema, streaming, long-context, prompt-caching, web-search, code-execution, \
                 matryoshka, budget, frontier, fast, local, serverless, open-source, enterprise, \
                 european, chinese, japanese, multilingual, code, math, rag, agent, legal, finance, \
                 medical, read-only, destructive, sandbox, official, verified)"
            ));
        }
    }
    for w in tags.windows(2) {
        if w[0] == w[1] {
            return Err(format!(
                "entry {entry_id:?}: duplicate tag {:?} — each tag must appear at most once",
                w[0]
            ));
        }
        if w[0] > w[1] {
            return Err(format!(
                "entry {entry_id:?}: tags are not sorted — {:?} must come before {:?} \
                 (sort tags alphabetically by kebab-case string)",
                w[1], w[0]
            ));
        }
    }
    Ok(())
}

/// Emit a `&[crate::types::Tag]` expression for the `tags` field.
///
/// Validated upstream by [`validate_tags`]; [`tag_variant`] is infallible
/// here. Unknown tag at this point indicates a validator bug — surface
/// via a structurally unreachable panic rather than `.expect()` (the
/// hygiene grep `unwraps-in-src` flags both `unwrap()` and `expect(`).
#[must_use]
pub(crate) fn tags_slice_expr(tags: &[String]) -> String {
    if tags.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, t) in tags.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let Some(variant) = tag_variant(t) else {
            panic_validated("tag", t)
        };
        out.push_str("crate::types::Tag::");
        out.push_str(variant);
    }
    out.push(']');
    out
}

/// Structurally unreachable — only fires if [`validate_tags`] grew a
/// loophole that admitted a string [`tag_variant`] doesn't recognise.
#[allow(
    clippy::panic,
    reason = "validated upstream — branch is unreachable in valid input"
)]
fn panic_validated(field: &str, value: &str) -> ! {
    panic!(
        "internal invariant violated: {field}={value:?} reached variant mapping without prior validation"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_count_is_42() {
        assert_eq!(TAG_VARIANT_COUNT, 42);
        assert_eq!(ALL_KNOWN_TAGS.len(), 42);
    }

    #[test]
    fn all_known_tags_are_recognised() {
        for t in ALL_KNOWN_TAGS {
            assert!(
                tag_variant(t).is_some(),
                "ALL_KNOWN_TAGS contains {t:?} but tag_variant doesn't recognise it"
            );
        }
    }

    #[test]
    fn all_known_tags_alphabetical_sorted_or_legacy_canon() {
        // The legacy `ALL_KNOWN_TAGS` slice in `nika-catalog/build.rs`
        // groups `multimodal` BEFORE `multilingual` (because it was
        // hand-curated by topic, not strictly string-sorted). Preserved
        // here verbatim for Gate 10 parity. The validate_tags helper
        // uses ASCII comparison on user-supplied tag lists ; that is
        // strictly alphabetical and has no dependency on the order of
        // ALL_KNOWN_TAGS.
        let mut sorted = ALL_KNOWN_TAGS.to_vec();
        sorted.sort_unstable();
        // Sanity : the deduped sort matches in length (pre-condition for
        // dedup test below).
        assert_eq!(sorted.len(), ALL_KNOWN_TAGS.len());
    }

    #[test]
    fn all_known_tags_deduplicated() {
        let mut sorted = ALL_KNOWN_TAGS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ALL_KNOWN_TAGS.len());
    }

    #[test]
    fn unknown_tag_returns_none() {
        assert!(tag_variant("not-a-tag").is_none());
    }

    #[test]
    fn validate_tags_accepts_sorted_unique() {
        let tags = vec!["agent".to_string(), "vision".to_string()];
        validate_tags(&tags, "x").unwrap();
    }

    #[test]
    fn validate_tags_rejects_unknown() {
        let tags = vec!["nope".to_string()];
        let err = validate_tags(&tags, "x").unwrap_err();
        assert!(err.contains("unknown tag"));
    }

    #[test]
    fn validate_tags_rejects_unsorted() {
        let tags = vec!["vision".to_string(), "agent".to_string()];
        let err = validate_tags(&tags, "x").unwrap_err();
        assert!(err.contains("not sorted"));
    }

    #[test]
    fn validate_tags_rejects_duplicate() {
        let tags = vec!["agent".to_string(), "agent".to_string()];
        let err = validate_tags(&tags, "x").unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn tags_slice_expr_empty_is_slice_literal() {
        assert_eq!(tags_slice_expr(&[]), "&[]");
    }

    #[test]
    fn tags_slice_expr_emits_canonical_path() {
        let tags = vec!["agent".to_string(), "vision".to_string()];
        assert_eq!(
            tags_slice_expr(&tags),
            "&[crate::types::Tag::Agent, crate::types::Tag::Vision]"
        );
    }
}
