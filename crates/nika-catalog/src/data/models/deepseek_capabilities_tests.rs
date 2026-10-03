// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::model_capabilities;
use crate::types::ParamFlag;
use crate::types::model::{ReasoningLevel, TokenLimitParam};

#[test]
fn current_deepseek_routes_reserve_room_for_default_thinking() {
    for provider in ["deepseek", "deep-seek"] {
        for model in ["deepseek-v4-pro", "deepseek-flash"] {
            let caps = model_capabilities(provider, model);
            assert!(caps.reasoning, "{provider}/{model}");
            assert!(
                !caps.supports_temperature,
                "ignored during default thinking"
            );
            assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        }
    }
}

#[test]
fn deepseek_rules_do_not_invent_gateway_or_future_model_capabilities() {
    for (provider, model) in [
        ("deepseek", "deepseek-future"),
        ("deepseek", "deepseek-chat"),
        ("openai", "deepseek-v4-pro"),
    ] {
        assert!(
            !model_capabilities(provider, model).reasoning,
            "{provider}/{model}"
        );
    }
}

/// The effort levels are each exact model's own evidence (R4 B16 · CALIBRATION-01): deepseek-v4-pro
/// and deepseek-flash each list low, high and max; old aliases, suffixed or unqualified Flash
/// names and gateways list none, whatever their name.
#[test]
fn only_the_exact_deepseek_models_document_effort_levels() {
    for provider in ["deepseek", "deep-seek"] {
        for model in ["deepseek-v4-pro", "deepseek-flash"] {
            let caps = model_capabilities(provider, model);
            assert_eq!(
                caps.reasoning_efforts,
                [
                    ReasoningLevel::Low,
                    ReasoningLevel::High,
                    ReasoningLevel::Max
                ],
                "{provider}/{model}"
            );
            assert!(
                caps.reasoning
                    && caps
                        .supported_parameters
                        .contains(&ParamFlag::ReasoningEffort),
                "{provider}/{model}"
            );
        }
    }
    for (provider, model) in [
        ("deepseek", "deepseek-chat"),
        ("deepseek", "deepseek-reasoner"),
        ("deepseek", "deepseek-v4-pro-0813"),
        ("deepseek", "deepseek-future"),
        ("deepseek", "deepseek-flash-0731"),
        ("deepseek", "deepseek-v4-flash"),
        ("deepseek", "deepseek-flash-latest"),
        ("scaleway", "deepseek-v4-flash-0731"),
        ("openai", "deepseek-v4-pro"),
        ("openai", "deepseek-flash"),
        ("openrouter", "deepseek/deepseek-v4-pro"),
        ("openrouter", "deepseek/deepseek-flash"),
        ("huggingface", "deepseek-ai/deepseek-v4-pro"),
    ] {
        let caps = model_capabilities(provider, model);
        assert!(caps.reasoning_efforts.is_empty(), "{provider}/{model}");
    }
}
