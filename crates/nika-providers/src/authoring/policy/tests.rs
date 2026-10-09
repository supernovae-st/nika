// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
#[test]
fn exact_deepseek_route_gets_its_technical_capacity_not_a_gateway_alias() {
    for model in ["deepseek/deepseek-v4-pro", "deepseek/deepseek-flash"] {
        let direct = completion_bounds(model, false, ProvidersConfig::new());
        assert_eq!(
            (direct.initial_tokens, direct.max_tokens),
            (131_072, 393_216)
        );
        assert_eq!(direct.timeout, Duration::from_secs(600));
        let gateway = completion_bounds(
            model,
            false,
            ProvidersConfig::new()
                .with_base_url("deepseek", "https://gateway.invalid/v1/chat/completions"),
        );
        assert_eq!(
            (gateway.initial_tokens, gateway.max_tokens),
            (16_384, 32_768)
        );
    }
}
#[test]
fn gateway_tariff_is_used_for_limits_without_inventing_usd_or_a_larger_model() {
    let limits = completion_bounds(
        "openai/gpt-oss-120b",
        false,
        ProvidersConfig::new()
            .with_base_url("openai", "https://api.scaleway.ai/v1/chat/completions"),
    );
    assert_eq!(limits.max_tokens, 32_768);
    assert!(limits.initial_tokens <= limits.max_tokens);
    assert_eq!(
        completion_bounds("claude-code/opus", true, ProvidersConfig::new()).timeout,
        Duration::from_secs(600)
    );
    assert_eq!(
        legacy_completion_bounds(false).timeout,
        Duration::from_secs(180)
    );
}
#[test]
fn a_harness_route_is_named_by_its_runtime_never_by_an_api_or_a_retired_alias() {
    for model in ["codex/gpt-6", "claude-code/opus[1m]", "grok-build/default"] {
        assert!(harness_route(model), "{model}");
    }
    let others = [
        "openai/gpt-oss-120b",
        "deepseek/deepseek-v4-pro",
        "mock/authoring",
        "claude-agent-acp/default",
        "codex",
    ];
    for model in others {
        assert!(!harness_route(model), "{model}");
    }
}
