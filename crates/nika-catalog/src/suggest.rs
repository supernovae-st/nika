// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fuzzy "did you mean?" suggestion across catalog namespaces.
//!
//! The engine occasionally receives a slightly wrong name — typo'd provider,
//! mis-cased MCP alias, renamed transform. Rather than returning a bare
//! `None`, the CLI surfaces the closest catalog entry so the user can
//! self-correct in one hop.
//!
//! Uses Jaro-Winkler similarity (`strsim::jaro_winkler`) with a 0.7 cutoff.
//! Jaro-Winkler weights matching prefixes — exactly what happens with
//! typos like `filesytem` → `filesystem` or `athropic` → `anthropic`.

#[cfg(any(
    feature = "mcp",
    feature = "providers",
    feature = "embeddings",
    feature = "builtins-transforms"
))]
use strsim::jaro_winkler;

#[cfg(feature = "builtins-transforms")]
use crate::data::builtins;
#[cfg(any(feature = "mcp", feature = "providers", feature = "embeddings"))]
use crate::data::generated;
#[cfg(feature = "builtins-transforms")]
use crate::data::transforms;

/// Minimum Jaro-Winkler score for a candidate to be considered a suggestion.
/// 0.7 is empirically the threshold between "typo" and "different word".
#[cfg(any(
    feature = "mcp",
    feature = "providers",
    feature = "embeddings",
    feature = "builtins-transforms"
))]
const MIN_SCORE: f64 = 0.7;

/// Maximum number of suggestions returned per query.
const MAX_SUGGESTIONS: usize = 5;

/// Which catalog a suggestion came from.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Namespace {
    /// LLM provider id or alias.
    Provider,
    /// MCP server id or alias.
    McpServer,
    /// Embedding model id.
    Embedding,
    /// Builtin tool name (e.g. `"nika:json_merge"`).
    Builtin,
    /// Pipe transform name (e.g. `"jq"`, `"upper"`).
    Transform,
}

/// A single suggestion with its source namespace and match score.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Suggestion {
    /// The catalog entry name (id or canonical form).
    pub name: &'static str,
    /// Which catalog the entry lives in.
    pub namespace: Namespace,
    /// Jaro-Winkler similarity, in `[0.0, 1.0]`. Higher = closer match.
    pub score: f64,
}

impl Suggestion {
    /// Explicit constructor — required because [`Suggestion`] is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(name: &'static str, namespace: Namespace, score: f64) -> Self {
        Self {
            name,
            namespace,
            score,
        }
    }
}

/// Find up to `MAX_SUGGESTIONS` (currently 5) catalog entries closest to
/// `query` across all namespaces. Sorted by score descending.
///
/// Returns an empty vec when no entry crosses `MIN_SCORE` (the
/// Jaro-Winkler threshold, currently 0.7). Query is compared
/// case-insensitively against canonical ids only — aliases are not
/// repeated because they already resolve to the same entry.
#[must_use]
#[cfg_attr(
    not(any(
        feature = "mcp",
        feature = "providers",
        feature = "embeddings",
        feature = "builtins-transforms"
    )),
    allow(unused_variables)
)]
pub fn suggest(query: &str) -> Vec<Suggestion> {
    let q = query.to_ascii_lowercase();
    let mut hits: Vec<Suggestion> = Vec::new();

    #[cfg(feature = "providers")]
    push_provider_hits(&q, &mut hits);
    #[cfg(feature = "mcp")]
    for srv in generated::ALL_MCP_SERVERS {
        let s = jaro_winkler(&q, &srv.id.to_ascii_lowercase());
        if s >= MIN_SCORE {
            hits.push(Suggestion {
                name: srv.id,
                namespace: Namespace::McpServer,
                score: s,
            });
        }
    }
    #[cfg(feature = "embeddings")]
    for emb in generated::ALL_EMBEDDINGS {
        let s = jaro_winkler(&q, &emb.id.to_ascii_lowercase());
        if s >= MIN_SCORE {
            hits.push(Suggestion {
                name: emb.id,
                namespace: Namespace::Embedding,
                score: s,
            });
        }
    }
    #[cfg(feature = "builtins-transforms")]
    for b in builtins::ALL_BUILTINS {
        let s = jaro_winkler(&q, &b.name.to_ascii_lowercase());
        if s >= MIN_SCORE {
            hits.push(Suggestion {
                name: b.name,
                namespace: Namespace::Builtin,
                score: s,
            });
        }
    }
    #[cfg(feature = "builtins-transforms")]
    for t in transforms::ALL_TRANSFORMS {
        let s = jaro_winkler(&q, &t.name.to_ascii_lowercase());
        if s >= MIN_SCORE {
            hits.push(Suggestion {
                name: t.name,
                namespace: Namespace::Transform,
                score: s,
            });
        }
    }

    // `f64::total_cmp` (the same SOTA form adopted in nika-bm25) gives a TOTAL,
    // deterministic order over the finite-by-construction jaro-winkler scores —
    // unlike `partial_cmp().unwrap_or(Equal)`, which would silently order any NaN
    // as Equal (non-deterministic) and is weaker under mutation. Ties break on the
    // name (ascending) so `truncate(MAX_SUGGESTIONS)` keeps a STABLE subset rather
    // than an insertion-order-dependent one.
    rank_hits(&mut hits);
    hits
}

#[cfg(feature = "providers")]
fn push_provider_hits(q: &str, hits: &mut Vec<Suggestion>) {
    for p in generated::ALL_PROVIDERS {
        consider(
            hits,
            p.id,
            Namespace::Provider,
            jaro_winkler(q, &p.id.to_ascii_lowercase()),
        );
        for alias in p.aliases {
            consider(
                hits,
                p.id,
                Namespace::Provider,
                jaro_winkler(q, &alias.to_ascii_lowercase()),
            );
        }
        // Model wire ids and nicknames suggest the PROVIDER that
        // serves them (B18 / issue 1306: `grok-3` is xAI, not groq).
        for m in p.models {
            for cand in [m.id, m.model] {
                let lower = cand.to_ascii_lowercase();
                let score = if lower == q {
                    1.0
                } else {
                    jaro_winkler(q, &lower)
                };
                consider(hits, p.id, Namespace::Provider, score);
            }
        }
    }
}

#[cfg(feature = "providers")]
fn consider(hits: &mut Vec<Suggestion>, name: &'static str, namespace: Namespace, score: f64) {
    if score >= MIN_SCORE {
        hits.push(Suggestion {
            name,
            namespace,
            score,
        });
    }
}

fn rank_hits(hits: &mut Vec<Suggestion>) {
    // `f64::total_cmp` (the same SOTA form adopted in nika-bm25) gives a TOTAL,
    // deterministic order over the finite-by-construction jaro-winkler scores —
    // unlike `partial_cmp().unwrap_or(Equal)`, which would silently order any NaN
    // as Equal (non-deterministic) and is weaker under mutation. Ties break on the
    // name (ascending) so `truncate(MAX_SUGGESTIONS)` keeps a STABLE subset rather
    // than an insertion-order-dependent one.
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.name.cmp(b.name)));
    let mut i = 0;
    while i < hits.len() {
        if hits[..i]
            .iter()
            .any(|h| h.name == hits[i].name && h.namespace == hits[i].namespace)
        {
            hits.remove(i);
        } else {
            i += 1;
        }
    }
    hits.truncate(MAX_SUGGESTIONS);
}

/// Scoped variant of [`suggest`] — only searches within one namespace.
///
/// Useful when the caller already knows the expected kind of name
/// (e.g. `nika provider set <typo>` should not propose MCP aliases).
#[must_use]
pub fn suggest_in(query: &str, namespace: Namespace) -> Vec<Suggestion> {
    suggest(query)
        .into_iter()
        .filter(|s| s.namespace == namespace)
        .collect()
}

// Suggestion tests reference specific catalog entries (filesystem, anthropic,
// gpt). They require the full catalog — gated to avoid false-fails under
// `--no-default-features --features minimal`.
#[cfg(all(test, feature = "mcp", feature = "providers"))]
mod tests {
    use super::*;

    #[test]
    fn filesystem_typo_resolves_to_filesystem() {
        let hits = suggest("filesytem");
        assert!(!hits.is_empty(), "expected suggestions for `filesytem`");
        assert_eq!(hits[0].name, "filesystem");
        assert_eq!(hits[0].namespace, Namespace::McpServer);
        assert!(hits[0].score > 0.9);
    }

    #[test]
    fn provider_typo_resolves_to_anthropic() {
        let hits = suggest("athropic");
        assert!(!hits.is_empty());
        let top = hits
            .iter()
            .find(|s| s.namespace == Namespace::Provider)
            .unwrap();
        assert_eq!(top.name, "anthropic");
    }

    #[test]
    fn nonsense_returns_empty() {
        // A string of the same character has essentially no Jaro-Winkler
        // similarity to any real catalog entry — the test doubles as a
        // smoke check on the threshold.
        let hits = suggest("qqqqqqqqqqqqqqqqqq");
        assert!(
            hits.is_empty(),
            "expected empty suggestions but got: {hits:?}",
        );
    }

    #[test]
    fn results_sorted_by_score_descending() {
        let hits = suggest("anthropic");
        for pair in hits.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    #[test]
    fn ordering_is_total_and_deterministic_on_ties() {
        // Equal scores must break on name (ascending) — the order is fully
        // determined, so repeated calls + truncation are reproducible.
        let proj =
            |hits: Vec<Suggestion>| hits.iter().map(|s| (s.name, s.score)).collect::<Vec<_>>();
        let a = proj(suggest("gpt"));
        let b = proj(suggest("gpt"));
        assert_eq!(a, b, "identical query yields identical ordering");
        for pair in a.windows(2) {
            // Mirror the production order via total_cmp (no float `==`): score
            // descending, and on an exact score tie, name ascending.
            let by_score = pair[0].1.total_cmp(&pair[1].1);
            assert!(
                by_score == std::cmp::Ordering::Greater
                    || (by_score == std::cmp::Ordering::Equal && pair[0].0 <= pair[1].0),
                "ties must break on name ascending"
            );
        }
    }

    #[test]
    fn capped_at_max_suggestions() {
        // "g" is a short query — multiple matches possible, capped at MAX.
        let hits = suggest("gpt");
        assert!(hits.len() <= MAX_SUGGESTIONS);
    }

    #[test]
    fn suggest_in_namespace_filters_correctly() {
        let hits = suggest_in("anthropic", Namespace::Provider);
        // Non-empty: a real query MUST return results (a `-> vec![]` mutant would
        // vacuously satisfy the `.all(...)` filter check below, so assert it
        // actually found something first).
        assert!(
            !hits.is_empty(),
            "suggest_in must return matches for a real query"
        );
        assert!(hits.iter().all(|s| s.namespace == Namespace::Provider));
    }

    #[test]
    fn case_insensitive_query() {
        let lower = suggest("FILESYSTEM");
        let upper = suggest("filesystem");
        assert_eq!(lower.len(), upper.len());
        assert_eq!(lower[0].name, upper[0].name);
    }

    /// B18 / issue 1306: a model wire id suggests the provider that
    /// serves it. `grok-3` is xAI; Jaro-Winkler against provider ids
    /// alone preferred `groq`.
    #[test]
    fn grok_3_suggests_xai_not_groq() {
        let hits = suggest("grok-3");
        assert!(
            !hits.is_empty(),
            "grok-3 must suggest its provider: {hits:?}"
        );
        assert_eq!(
            hits[0].name,
            "xai",
            "top hit must be xai, not groq: {:?}",
            hits.iter().map(|h| h.name).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].namespace, Namespace::Provider);
    }
}
