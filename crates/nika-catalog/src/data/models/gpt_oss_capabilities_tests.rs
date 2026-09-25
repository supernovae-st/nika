// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::model_capabilities;

#[test]
fn the_openai_gpt_oss_120b_name_records_its_reasoning_and_nothing_else() {
    let caps = model_capabilities("openai", "gpt-oss-120b");
    assert!(caps.reasoning);
    let defaults = model_capabilities("openai", "an-unlisted-model");
    assert!(!defaults.reasoning);
    assert_eq!(caps.token_limit_param, defaults.token_limit_param);
    assert_eq!(caps.supports_temperature, defaults.supports_temperature);
    assert_eq!(caps.json_mode, defaults.json_mode);
    assert_eq!(
        caps.max_output_tokens, None,
        "a route's output limit stays with its endpoint-bound admission tariff"
    );
}

#[test]
fn the_rule_does_not_reach_other_names_or_provider_ids() {
    for (provider, model) in [
        ("openai", "gpt-oss-120b-new"),
        ("openai", "gpt-oss-120b:fp4"),
        ("openai", "gpt-oss-20b"),
        ("scaleway", "gpt-oss-120b"),
    ] {
        assert!(
            !model_capabilities(provider, model).reasoning,
            "{provider}/{model}"
        );
    }
}
