// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime types backing the TOML-driven `model_capabilities` resolver.
//!
//! These types are **crate-internal**. Workflow callers see only the final
//! [`ModelCapabilities`] struct returned by
//! [`crate::data::models::model_capabilities`].
//!
//! The tables themselves are emitted by `build.rs` into
//! `$OUT_DIR/model_capabilities.rs` as three `pub(crate) static` items:
//!   * `CAPABILITY_DEFAULTS: CapPatch` — baseline from `[defaults]`
//!   * `CAPABILITY_RULES: &[Rule]` — ordered rule list, first-match-wins
//!
//! Zero runtime allocation: every matcher is a slice of `&'static str`,
//! every comparison uses `eq_ignore_ascii_case` on the input slice.
//!
//! # Feature gate
//!
//! This module is only compiled when the `capabilities` feature is on — it
//! is useless without the generated rule table. The gate lives at the
//! `pub mod capabilities;` declaration in `types/mod.rs`.

use super::model::{ModelCapabilities, TokenLimitParam};

/// Partial capabilities — `Option<T>` per field so several patches can be
/// layered by `merge_with` before materialising into a concrete
/// [`ModelCapabilities`]. Emitted by `build.rs` for `[defaults]` and for
/// each rule's `caps`.
///
/// The field set mirrors [`ModelCapabilities`] 1:1; adding a field to the
/// public struct requires adding it here + in the merge + materialize paths
/// — Session 2b will grow this shape when it introduces modalities /
/// tokenizer / supported parameters.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CapPatch {
    /// Per-field override for [`ModelCapabilities::token_limit_param`].
    pub token_limit_param: Option<TokenLimitParam>,
    /// Per-field override for [`ModelCapabilities::supports_temperature`].
    pub supports_temperature: Option<bool>,
    /// Per-field override for [`ModelCapabilities::supports_stop_sequences`].
    pub supports_stop_sequences: Option<bool>,
    /// Per-field override for [`ModelCapabilities::reasoning`].
    pub reasoning: Option<bool>,
    /// Per-field override for [`ModelCapabilities::supports_vision`].
    pub supports_vision: Option<bool>,
}

impl CapPatch {
    /// Apply every `Some` field of `other` onto `self`.
    ///
    /// Contract: `other` wins field-by-field. `None` fields on `other` do
    /// not clear `self` — they are "no-op" slots. The algorithm is
    /// deliberately boring so the `const` version LLVM sees is the same
    /// as the runtime one.
    #[must_use]
    #[inline]
    pub(crate) const fn merge_with(mut self, other: Self) -> Self {
        if let Some(v) = other.token_limit_param {
            self.token_limit_param = Some(v);
        }
        if let Some(v) = other.supports_temperature {
            self.supports_temperature = Some(v);
        }
        if let Some(v) = other.supports_stop_sequences {
            self.supports_stop_sequences = Some(v);
        }
        if let Some(v) = other.reasoning {
            self.reasoning = Some(v);
        }
        if let Some(v) = other.supports_vision {
            self.supports_vision = Some(v);
        }
        self
    }

    /// Fill any still-unset field from [`ModelCapabilities::default`] and
    /// return the concrete struct.
    ///
    /// Called exactly once at the tail of `model_capabilities`.
    #[must_use]
    pub(crate) fn materialize(self) -> ModelCapabilities {
        let fallback = ModelCapabilities::default();
        ModelCapabilities {
            token_limit_param: self.token_limit_param.unwrap_or(fallback.token_limit_param),
            supports_temperature: self
                .supports_temperature
                .unwrap_or(fallback.supports_temperature),
            supports_stop_sequences: self
                .supports_stop_sequences
                .unwrap_or(fallback.supports_stop_sequences),
            reasoning: self.reasoning.unwrap_or(fallback.reasoning),
            supports_vision: self.supports_vision.unwrap_or(fallback.supports_vision),
        }
    }
}

/// Matcher — which `model` strings a rule applies to.
///
/// Four shapes:
///   * [`Matcher::Any`] — match any model (used for provider-wide fallback
///     rules like `anthropic-any-model`)
///   * [`Matcher::Exact`] — single-name exact match (case-insensitive ASCII)
///   * [`Matcher::ExactAny`] — exact match against any entry in the list
///   * [`Matcher::PrefixAny`] — model starts with any entry in the list
///
/// No regex variant in `@1.0` — keeps L0 zero-runtime-dep. If a future
/// provider ever needs regex, add a new variant and bump the schema.
#[derive(Debug)]
pub(crate) enum Matcher {
    /// Always matches.
    Any,
    /// Case-insensitive ASCII equality with `model`.
    Exact(&'static str),
    /// Case-insensitive ASCII equality with any entry.
    ExactAny(&'static [&'static str]),
    /// Model starts (case-insensitively, ASCII) with any entry.
    PrefixAny(&'static [&'static str]),
}

impl Matcher {
    /// Test whether `model` matches. Zero alloc.
    #[must_use]
    #[inline]
    pub(crate) fn matches(&self, model: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(m) => model.eq_ignore_ascii_case(m),
            Self::ExactAny(list) => list.iter().any(|m| model.eq_ignore_ascii_case(m)),
            Self::PrefixAny(list) => list.iter().any(|p| starts_with_ci_ascii(model, p)),
        }
    }
}

/// Case-insensitive ASCII prefix test. No allocation, no Unicode folding.
///
/// Equivalent to `model.to_ascii_lowercase().starts_with(&p.to_ascii_lowercase())`
/// but without the two heap allocations.
#[inline]
fn starts_with_ci_ascii(model: &str, prefix: &str) -> bool {
    let pb = prefix.as_bytes();
    let mb = model.as_bytes();
    mb.len() >= pb.len() && mb[..pb.len()].eq_ignore_ascii_case(pb)
}

/// One ordered rule from `data/model-capabilities.toml`.
///
/// Rules are scanned in file order; the first match wins and its `caps`
/// are merged into the accumulator via [`CapPatch::merge_with`].
///
/// The TOML `name` field is used at build time for uniqueness checks only
/// and is NOT carried into the runtime struct — it would be dead code per
/// the zero-`#[allow(dead_code)]` policy. Consult `model-capabilities.toml`
/// when debugging rule ordering; generated rule slices appear in file order.
#[derive(Debug)]
pub(crate) struct Rule {
    /// Canonical provider ids this rule applies to. Empty slice = any provider.
    pub providers: &'static [&'static str],
    /// Wire-protocol dialect scope. `None` = any dialect.
    pub api_dialect: Option<&'static str>,
    /// Model-name matcher.
    pub matcher: Matcher,
    /// Partial caps merged into the accumulator on match.
    pub caps: CapPatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_with_takes_some_fields_from_other() {
        let base = CapPatch {
            reasoning: Some(false),
            supports_vision: Some(true),
            ..CapPatch::default()
        };
        let patch = CapPatch {
            reasoning: Some(true),
            ..CapPatch::default()
        };
        let merged = base.merge_with(patch);
        assert_eq!(merged.reasoning, Some(true));
        assert_eq!(
            merged.supports_vision,
            Some(true),
            "fields not set on patch must be preserved from base",
        );
    }

    #[test]
    fn merge_with_none_does_not_clear_self() {
        let base = CapPatch {
            reasoning: Some(true),
            ..CapPatch::default()
        };
        let empty = CapPatch::default();
        let merged = base.merge_with(empty);
        assert_eq!(
            merged.reasoning,
            Some(true),
            "None on other must not reset fields on self",
        );
    }

    #[test]
    fn materialize_falls_back_to_default_for_unset_fields() {
        let caps = CapPatch::default().materialize();
        assert_eq!(caps, ModelCapabilities::default());
    }

    #[test]
    fn materialize_preserves_overridden_fields() {
        let caps = CapPatch {
            reasoning: Some(true),
            supports_vision: Some(false),
            ..CapPatch::default()
        }
        .materialize();
        assert!(caps.reasoning);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn matcher_any_matches_anything() {
        assert!(Matcher::Any.matches(""));
        assert!(Matcher::Any.matches("gpt-9999"));
    }

    #[test]
    fn matcher_exact_case_insensitive() {
        let m = Matcher::Exact("gpt-5");
        assert!(m.matches("gpt-5"));
        assert!(m.matches("GPT-5"));
        assert!(!m.matches("gpt-5x"));
        assert!(!m.matches("gpt-"));
    }

    #[test]
    fn matcher_exact_any_covers_every_entry() {
        let m = Matcher::ExactAny(&["o1", "o3", "o4"]);
        assert!(m.matches("o1"));
        assert!(m.matches("O3"));
        assert!(m.matches("o4"));
        assert!(!m.matches("o5"));
        assert!(!m.matches("o1-preview"));
    }

    #[test]
    fn matcher_prefix_any_is_case_insensitive_and_strict() {
        let m = Matcher::PrefixAny(&["o1-", "o3-"]);
        assert!(m.matches("o1-preview"));
        assert!(m.matches("O3-MINI"));
        assert!(!m.matches("o1"), "prefix requires the dash — bare base name must not match");
        assert!(!m.matches("x-o1-"));
    }

    #[test]
    fn starts_with_ci_ascii_bytes_agree_with_owning_lowercase() {
        for (model, prefix, expected) in [
            ("", "", true),
            ("anything", "", true),
            ("", "x", false),
            ("Claude-Opus", "claude", true),
            ("bedrock/claude-x", "claude", false),
            ("a", "ab", false),
        ] {
            assert_eq!(
                starts_with_ci_ascii(model, prefix),
                expected,
                "model={model:?} prefix={prefix:?}",
            );
        }
    }
}
