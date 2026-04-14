// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Embedding model catalog entry.
//!
//! Vector-store compatibility is a compile-time fact: voyage-3-large is
//! 1024 dimensions, period. Failing at runtime on a dim mismatch is the
//! worst UX — embed once with a wrong model and every vector is trash.
//! This catalog lets workflow authors pick an embedding that matches
//! their vector store BEFORE the first call.

/// Similarity metric the embedding is optimised for.
///
/// Pragmatic closed set — the only values that actually appear in
/// published embedding docs (2026-04 snapshot). Most vector stores
/// support all three at query time, so the field is a **preference**
/// hint, not a hard constraint.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Similarity {
    /// Angular cosine similarity — the default for L2-normalised outputs.
    Cosine,
    /// Dot-product similarity — equivalent to cosine when normalised.
    DotProduct,
    /// Squared L2 (Euclidean) distance.
    L2,
}

impl Similarity {
    /// Kebab-case wire identifier used in TOML and downstream tooling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cosine => "cosine",
            Self::DotProduct => "dot-product",
            Self::L2 => "l2",
        }
    }
}

/// An embedding model exposed by a provider, with vector-store metadata.
///
/// Generated into `$OUT_DIR/embeddings.rs` by the crate's `build.rs` from
/// `data/embeddings.toml`. Lookup is O(1) case-insensitive via
/// [`EMBEDDING_INDEX`].
// `Eq` is intentionally omitted — `input_per_million: f64` blocks it
// (NaN != NaN). Values are curated constants so this never bites in
// practice, but the derive would not compile.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Embedding {
    /// Canonical id, commonly `"<provider>/<model>"` (e.g. `"voyage/voyage-3-large"`).
    pub id: &'static str,
    /// Provider id (matches a row in `ALL_PROVIDERS` — e.g. `"voyage"`, `"openai"`).
    pub provider: &'static str,
    /// Wire identifier sent to the provider API (e.g. `"voyage-3-large"`).
    pub model: &'static str,
    /// Output vector dimensionality.
    pub dimensions: u32,
    /// Maximum tokens the embedder accepts in a single input.
    pub max_input_tokens: u32,
    /// Whether the provider returns L2-normalised vectors by default.
    pub normalized_by_default: bool,
    /// Similarity metric the embedding is optimised for.
    pub similarity: Similarity,
    /// USD per 1 million input tokens (output is fixed-size, not priced).
    pub input_per_million: f64,
    /// Short human-readable description.
    pub description: &'static str,
    /// Typed tags describing embedding capabilities and domain.
    /// Sorted + deduplicated (enforced by build.rs assertion).
    pub tags: &'static [crate::types::Tag],
    /// Community / vendor-specific tags not in the core [`Tag`] enum.
    pub extra_tags: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
        assert_copy_send_sync::<Similarity>();
        assert_copy_send_sync::<Embedding>();
    };

    #[test]
    fn similarity_roundtrip() {
        for s in [Similarity::Cosine, Similarity::DotProduct, Similarity::L2] {
            assert!(!s.as_str().is_empty());
        }
    }

    #[test]
    fn embedding_construction() {
        let e = Embedding {
            id: "voyage/voyage-3-large",
            provider: "voyage",
            model: "voyage-3-large",
            dimensions: 1024,
            max_input_tokens: 32_000,
            normalized_by_default: true,
            similarity: Similarity::Cosine,
            input_per_million: 0.18,
            description: "Voyage 3 large embedding.",
            tags: &[],
            extra_tags: &[],
        };
        assert_eq!(e.dimensions, 1024);
        assert_eq!(e.similarity.as_str(), "cosine");
    }
}
