// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Proptest capabilities invariants of the model table — a child module of
//! `models`, carved out when the table crossed the 1500-line wall; the
//! `super::` paths below still name `models`' own items.

use super::model_capabilities;
use crate::types::Modality;
use proptest::prelude::*;

fn provider_name() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z-]{1,30}").expect("valid regex")
}

fn model_name() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-z0-9._/-]{0,50}").expect("valid regex")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Determinism: same input always produces the same output.
    #[test]
    fn deterministic(
        provider in provider_name(),
        model in model_name(),
    ) {
        let a = model_capabilities(&provider, &model);
        let b = model_capabilities(&provider, &model);
        prop_assert_eq!(a, b);
    }

    /// Case-insensitivity: mixed-case provider names resolve identically.
    #[test]
    fn case_insensitive_provider(
        provider in provider_name(),
        model in model_name(),
    ) {
        let lower = model_capabilities(&provider.to_ascii_lowercase(), &model);
        let upper = model_capabilities(&provider.to_ascii_uppercase(), &model);
        prop_assert_eq!(lower, upper);
    }

    /// Defaults contract: every resolved struct has at least Text input.
    #[test]
    fn always_has_text_input(
        provider in provider_name(),
        model in model_name(),
    ) {
        let caps = model_capabilities(&provider, &model);
        prop_assert!(
            caps.input_modalities.contains(&Modality::Text),
            "every model must support Text input: {provider}/{model}",
        );
    }

    /// Output modalities are non-empty (at least Text).
    #[test]
    fn always_has_output(
        provider in provider_name(),
        model in model_name(),
    ) {
        let caps = model_capabilities(&provider, &model);
        prop_assert!(
            !caps.output_modalities.is_empty(),
            "output_modalities must not be empty: {provider}/{model}",
        );
    }
}

/// Provider aliases resolve to the same capabilities.
#[test]
fn alias_equivalence() {
    let known_aliases = [
        ("claude", "anthropic"),
        ("gpt", "openai"),
        ("deep-seek", "deepseek"),
        ("grok", "xai"),
    ];
    for (alias, canonical) in known_aliases {
        let via_alias = model_capabilities(alias, "test-model");
        let via_canonical = model_capabilities(canonical, "test-model");
        assert_eq!(
            via_alias, via_canonical,
            "{alias} and {canonical} should resolve identically",
        );
    }
}
