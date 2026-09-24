// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::model_capabilities;
use crate::types::model::TokenLimitParam;

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
