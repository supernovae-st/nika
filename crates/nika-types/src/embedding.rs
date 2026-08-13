// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Embedding shape — dim/dtype/metric/model trio.
//!
//! Reserved at v0.81 so that the Connectome can land embedding storage
//! without breaking-change migrations. The embedding landscape evolves
//! monthly (Matryoshka, 256/384/512/768/1024/1536/3072 dims; f32/f16/
//! bf16/i8/binary dtypes; cosine/dot-product/euclidean/hamming metrics)
//! so every field sits behind `#[non_exhaustive]`.
//!
//! The three axes — dim, dtype, metric — match what HNSW/BM25/RRF
//! backends need to decide index layout + distance kernel. `model_id`
//! is optional so catalog-free consumers (tests, tools) don't have to
//! materialise a provider name to construct a spec.
//!
//! ADR-029 captures the full rationale (Phase C).

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Element storage type for an embedding vector.
///
/// Matryoshka training + hardware tiers push providers towards ever-smaller
/// element types (binary for on-device retrieval, i8 for mobile, f16/bf16
/// for GPU inference, f32 for full-precision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum EmbeddingDtype {
    /// IEEE 754 binary32.
    F32,
    /// IEEE 754 binary16 — half precision.
    F16,
    /// bfloat16 — 8-bit exponent, 7-bit mantissa. Dominant on TPU/GPU.
    BF16,
    /// 8-bit signed integer quantisation.
    I8,
    /// 1-bit per dimension (bit-packed). Fastest, lowest-fidelity.
    Binary,
}

/// Vector distance metric for similarity search.
///
/// Choice dictates the index kernel (HNSW inner-product vs L2, BM25 tie,
/// Hamming popcount for binary vectors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum DistanceMetric {
    /// Cosine similarity (angle between vectors, magnitude-invariant).
    Cosine,
    /// Dot product (inner product, magnitude-sensitive).
    DotProduct,
    /// Euclidean distance (L2).
    Euclidean,
    /// Hamming distance (popcount of XOR). Binary dtype only.
    Hamming,
}

/// Full specification of an embedding shape.
///
/// Producers (provider responses, catalog defaults) emit `EmbeddingSpec`
/// so consumers (memory stores, similarity kernels) can reject shape
/// mismatches at ingress instead of silently producing garbage scores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct EmbeddingSpec {
    /// Dimensionality. Common values: 256, 384, 512, 768, 1024, 1536, 3072.
    /// `u16` caps at 65 535 which covers every known production embedding.
    pub dim: u16,
    /// Element storage.
    pub dtype: EmbeddingDtype,
    /// Distance metric used by index/search.
    pub metric: DistanceMetric,
    /// Optional provenance — e.g. `"openai-3-small"`, `"voyage-3-large"`.
    /// Not used for equality/hash by domain policy but kept for diagnostics.
    pub model_id: Option<alloc::string::String>,
}

impl EmbeddingSpec {
    /// New spec without `model_id` provenance.
    #[must_use]
    pub const fn new(dim: u16, dtype: EmbeddingDtype, metric: DistanceMetric) -> Self {
        Self {
            dim,
            dtype,
            metric,
            model_id: None,
        }
    }

    /// New spec with `model_id` provenance (owned string).
    #[must_use]
    pub fn with_model(
        dim: u16,
        dtype: EmbeddingDtype,
        metric: DistanceMetric,
        model_id: alloc::string::String,
    ) -> Self {
        Self {
            dim,
            dtype,
            metric,
            model_id: Some(model_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn new_default_model_id_is_none() {
        let spec = EmbeddingSpec::new(1536, EmbeddingDtype::F32, DistanceMetric::Cosine);
        assert_eq!(spec.dim, 1536);
        assert!(matches!(spec.dtype, EmbeddingDtype::F32));
        assert!(matches!(spec.metric, DistanceMetric::Cosine));
        assert!(spec.model_id.is_none());
    }

    #[test]
    fn with_model_sets_field() {
        let spec = EmbeddingSpec::with_model(
            3072,
            EmbeddingDtype::BF16,
            DistanceMetric::DotProduct,
            "openai-3-large".to_string(),
        );
        assert_eq!(spec.model_id.as_deref(), Some("openai-3-large"));
    }

    #[test]
    fn dtype_variants_distinct() {
        let variants = [
            EmbeddingDtype::F32,
            EmbeddingDtype::F16,
            EmbeddingDtype::BF16,
            EmbeddingDtype::I8,
            EmbeddingDtype::Binary,
        ];
        // Pairwise inequality.
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i != j {
                    assert_ne!(variants[i], variants[j]);
                }
            }
        }
    }

    #[test]
    fn metric_variants_distinct() {
        let variants = [
            DistanceMetric::Cosine,
            DistanceMetric::DotProduct,
            DistanceMetric::Euclidean,
            DistanceMetric::Hamming,
        ];
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i != j {
                    assert_ne!(variants[i], variants[j]);
                }
            }
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_spec_none_model() {
        let spec = EmbeddingSpec::new(768, EmbeddingDtype::F16, DistanceMetric::Euclidean);
        let json = serde_json::to_string(&spec).expect("ser");
        let back: EmbeddingSpec = serde_json::from_str(&json).expect("de");
        assert_eq!(spec, back);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_spec_with_model() {
        let spec = EmbeddingSpec::with_model(
            1536,
            EmbeddingDtype::Binary,
            DistanceMetric::Hamming,
            "voyage-3-large".to_string(),
        );
        let json = serde_json::to_string(&spec).expect("ser");
        let back: EmbeddingSpec = serde_json::from_str(&json).expect("de");
        assert_eq!(spec, back);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_dtype_uses_snake_case() {
        // serde snake_case rule splits at ASCII upper-lower transitions, so
        // `BF16` → `b_f16`. That's the wire-format contract we're locking in.
        let json = serde_json::to_string(&EmbeddingDtype::BF16).expect("ser");
        assert_eq!(json, "\"b_f16\"");
        let json = serde_json::to_string(&EmbeddingDtype::F32).expect("ser");
        assert_eq!(json, "\"f32\"");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_metric_uses_snake_case() {
        let json = serde_json::to_string(&DistanceMetric::DotProduct).expect("ser");
        assert_eq!(json, "\"dot_product\"");
    }
}
