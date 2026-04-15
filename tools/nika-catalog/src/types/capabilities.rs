// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime types backing the TOML-driven `model_capabilities` resolver.
//!
//! These types are **crate-internal**. Workflow callers see only the final
//! [`ModelCapabilities`] struct returned by
//! [`crate::data::models::model_capabilities`].
//!
//! The tables themselves are emitted by `build.rs` into
//! `$OUT_DIR/model_capabilities.rs` as two `pub(crate) static` items:
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
use super::{Modality, ParamFlag, TokenizerFamily};

/// Partial capabilities — `Option<T>` per field so several patches can be
/// layered by `merge_with` before materialising into a concrete
/// [`ModelCapabilities`]. Emitted by `build.rs` for `[defaults]` and for
/// each rule's `caps`.
///
/// The field set mirrors [`ModelCapabilities`] 1:1 (10 fields after
/// Session 2b: original 5 + modalities + tokenizer + supported parameters +
/// system-message flag).
///
/// # `Copy` is MANDATORY
///
/// Every field is `Option<CopyType>` (including `Option<&'static [Copy]>`),
/// so the compiler can auto-derive `Copy`. Without it,
/// `CAPABILITY_DEFAULTS.merge_with(patch)` in a `const fn` context fails
/// with `error[E0507]: cannot move out of static item`. Confirmed by
/// rustc 1.91.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CapPatch {
    // ── existing 5 slots (Session 2a) ──────────────────────────────────
    /// Per-field override for [`ModelCapabilities::token_limit_param`].
    pub token_limit_param: Option<TokenLimitParam>,
    /// Per-field override for [`ModelCapabilities::supports_temperature`].
    pub supports_temperature: Option<bool>,
    /// Per-field override for [`ModelCapabilities::supports_stop_sequences`].
    pub supports_stop_sequences: Option<bool>,
    /// Per-field override for [`ModelCapabilities::reasoning`].
    pub reasoning: Option<bool>,
    // ── Session 2b additions (5 new slots) ─────────────────────────────
    /// Per-field override for [`ModelCapabilities::input_modalities`].
    pub input_modalities: Option<&'static [Modality]>,
    /// Per-field override for [`ModelCapabilities::output_modalities`].
    pub output_modalities: Option<&'static [Modality]>,
    /// Per-field override for [`ModelCapabilities::tokenizer`].
    ///
    /// A rule sets this to `Some(TokenizerFamily::X)` to attach a
    /// tokenizer to matching models; rules that do not touch tokenizer
    /// leave it `None` and fall through to `[defaults]`. There is no
    /// explicit "clear to None" semantic — models without a known
    /// tokenizer simply don't set this field in their matching rule.
    pub tokenizer: Option<TokenizerFamily>,
    /// Per-field override for [`ModelCapabilities::supported_parameters`].
    pub supported_parameters: Option<&'static [ParamFlag]>,
    /// Per-field override for [`ModelCapabilities::supports_system_messages`].
    pub supports_system_messages: Option<bool>,
}

impl CapPatch {
    /// Apply every `Some` field of `other` onto `self`.
    ///
    /// Contract: `other` wins field-by-field. `None` fields on `other` do
    /// not clear `self` — they are "no-op" slots. The algorithm is
    /// deliberately boring so the `const` version LLVM sees is the same
    /// as the runtime one.
    ///
    /// Expressed via a `macro_rules!` to keep the 10-field body readable.
    #[must_use]
    #[inline]
    pub(crate) const fn merge_with(mut self, other: Self) -> Self {
        macro_rules! merge_field {
            ($($field:ident),+ $(,)?) => {
                $(
                    if let Some(v) = other.$field {
                        self.$field = Some(v);
                    }
                )+
            };
        }
        merge_field!(
            token_limit_param,
            supports_temperature,
            supports_stop_sequences,
            reasoning,
            input_modalities,
            output_modalities,
            tokenizer,
            supported_parameters,
            supports_system_messages,
        );
        self
    }

    /// Fill any still-unset field from the supplied TOML-emitted `defaults`
    /// `CapPatch` (and, for truly-absent defaults, from a safe hard-coded
    /// fallback) and return the concrete struct.
    ///
    /// # P0-1 fix — single source of truth
    ///
    /// Before Session 2b, `materialize()` fell back to
    /// [`ModelCapabilities::default`]. Two independent sources of truth
    /// (Rust `Default` impl + TOML `[defaults]`) were asserted to agree
    /// only at test time — a silent divergence would have corrupted every
    /// unresolved model.
    ///
    /// After Session 2b, `materialize(&CAPABILITY_DEFAULTS)` draws every
    /// unset field from the TOML baseline. The hard-coded fallbacks below
    /// (`unwrap_or(…)`) only activate when a field is missing from BOTH
    /// the rule and `[defaults]` — a mis-authored TOML that would fail
    /// `validate_caps_patch` at build time.
    ///
    /// Called exactly once at the tail of `model_capabilities`.
    #[must_use]
    pub(crate) fn materialize(self, defaults: &Self) -> ModelCapabilities {
        ModelCapabilities {
            token_limit_param: self
                .token_limit_param
                .or(defaults.token_limit_param)
                .unwrap_or(TokenLimitParam::MaxTokens),
            supports_temperature: self
                .supports_temperature
                .or(defaults.supports_temperature)
                .unwrap_or(true),
            supports_stop_sequences: self
                .supports_stop_sequences
                .or(defaults.supports_stop_sequences)
                .unwrap_or(true),
            reasoning: self.reasoning.or(defaults.reasoning).unwrap_or(false),
            input_modalities: self
                .input_modalities
                .or(defaults.input_modalities)
                .unwrap_or(&[Modality::Text, Modality::Image]),
            output_modalities: self
                .output_modalities
                .or(defaults.output_modalities)
                .unwrap_or(&[Modality::Text]),
            // tokenizer: ModelCapabilities.tokenizer is already Option<T>,
            // so a missing patch+defaults collapse to None naturally.
            tokenizer: self.tokenizer.or(defaults.tokenizer),
            supported_parameters: self
                .supported_parameters
                .or(defaults.supported_parameters)
                .unwrap_or(&[]),
            supports_system_messages: self
                .supports_system_messages
                .or(defaults.supports_system_messages)
                .unwrap_or(true),
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

    fn toml_like_defaults() -> CapPatch {
        // Mirrors data/model-capabilities.toml [defaults] — used by unit
        // tests in this module to exercise materialize() independently of
        // the build-time generated CAPABILITY_DEFAULTS.
        CapPatch {
            token_limit_param: Some(TokenLimitParam::MaxTokens),
            supports_temperature: Some(true),
            supports_stop_sequences: Some(true),
            reasoning: Some(false),
            input_modalities: Some(&[Modality::Text, Modality::Image]),
            output_modalities: Some(&[Modality::Text]),
            tokenizer: None,
            supported_parameters: Some(&[]),
            supports_system_messages: Some(true),
        }
    }

    #[test]
    fn merge_with_takes_some_fields_from_other() {
        let base = CapPatch {
            reasoning: Some(false),
            supports_temperature: Some(true),
            ..CapPatch::default()
        };
        let patch = CapPatch {
            reasoning: Some(true),
            ..CapPatch::default()
        };
        let merged = base.merge_with(patch);
        assert_eq!(merged.reasoning, Some(true));
        assert_eq!(
            merged.supports_temperature,
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
    fn merge_with_covers_new_session_2b_fields() {
        // Regression guard: if a future author forgets to add a field to
        // the merge_field! invocation, this test catches it.
        const IMG: &[Modality] = &[Modality::Text, Modality::Image];
        const PARAMS: &[ParamFlag] = &[ParamFlag::StructuredOutputNative];
        let base = CapPatch::default();
        let patch = CapPatch {
            input_modalities: Some(IMG),
            output_modalities: Some(&[Modality::Text]),
            tokenizer: Some(TokenizerFamily::O200k),
            supported_parameters: Some(PARAMS),
            supports_system_messages: Some(false),
            ..CapPatch::default()
        };
        let merged = base.merge_with(patch);
        assert_eq!(merged.input_modalities, Some(IMG));
        assert_eq!(merged.tokenizer, Some(TokenizerFamily::O200k));
        assert_eq!(merged.supported_parameters, Some(PARAMS));
        assert_eq!(merged.supports_system_messages, Some(false));
    }

    #[test]
    fn materialize_uses_defaults_for_unset_fields() {
        let defaults = toml_like_defaults();
        let caps = CapPatch::default().materialize(&defaults);
        assert_eq!(caps, ModelCapabilities::default());
    }

    #[test]
    fn materialize_preserves_overridden_fields() {
        let defaults = toml_like_defaults();
        let caps = CapPatch {
            reasoning: Some(true),
            input_modalities: Some(&[Modality::Text]),
            ..CapPatch::default()
        }
        .materialize(&defaults);
        assert!(caps.reasoning);
        assert!(!caps.input_modalities.contains(&Modality::Image));
        assert_eq!(caps.input_modalities, &[Modality::Text]);
    }

    #[test]
    fn materialize_defaults_agree_with_rust_default_impl() {
        // P0-1 guard: the TOML-shaped defaults must produce the same
        // ModelCapabilities as the Rust Default impl. A future edit to
        // one and not the other is caught here.
        let defaults = toml_like_defaults();
        assert_eq!(
            CapPatch::default().materialize(&defaults),
            ModelCapabilities::default(),
        );
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
