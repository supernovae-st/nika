// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoning facts recorded per route (2026-10-10) and the routes left unknown. The two rules
//! that carry facts for one provider stand before a broader rule and must keep its caps.

#![cfg(feature = "capabilities")]

use super::model_capabilities;
use crate::types::model::{ReasoningFacts, ThinkingOff};

fn facts(provider: &str, model: &str) -> ReasoningFacts {
    model_capabilities(provider, model).reasoning_facts
}

fn thinks(off: ThinkingOff) -> ReasoningFacts {
    ReasoningFacts::new(Some(true), Some(true), Some(off))
}

#[test]
fn recorded_routes_carry_their_facts() {
    for provider in ["deepseek", "deep-seek"] {
        for model in ["deepseek-flash", "deepseek-v4-pro"] {
            assert_eq!(
                facts(provider, model),
                thinks(ThinkingOff::NotReached),
                "{model}"
            );
        }
    }
    assert_eq!(
        facts("gemini", "gemini-2.5-flash"),
        thinks(ThinkingOff::BudgetZero)
    );
    assert_eq!(
        facts("gemini", "gemini-2.5-pro"),
        thinks(ThinkingOff::Never)
    );
    let claude = ReasoningFacts::new(Some(false), Some(true), Some(ThinkingOff::ByDefault));
    assert_eq!(facts("anthropic", "claude-sonnet-4-5-20250929"), claude);
    assert_eq!(facts("claude", "claude-opus-4-20250514"), claude);
}

#[test]
fn unrecorded_routes_stay_unknown() {
    for (provider, model) in [
        ("gemini", "gemini-2.5-flash-lite"),
        ("vertex", "gemini-2.5-flash"),
        ("openrouter", "anthropic/claude-sonnet-4-5-20250929"),
        ("bedrock", "anthropic.claude-sonnet-4-20250514-v1:0"),
        ("deepseek", "claude-sonnet-4-5-20250929"),
        ("deepseek", "deepseek-reasoner"),
        ("openai", "o3"),
    ] {
        assert_eq!(
            facts(provider, model),
            ReasoningFacts::default(),
            "{provider}/{model}"
        );
    }
}

#[test]
fn a_rule_that_carries_facts_keeps_the_caps_of_the_rule_it_stands_before() {
    let flash = model_capabilities("gemini", "gemini-2.5-flash");
    let broader = model_capabilities("vertex", "gemini-2.5-flash");
    assert_eq!(flash, broader.with_reasoning_facts(flash.reasoning_facts));
    let model = "claude-sonnet-4-5-20250929";
    let claude = model_capabilities("anthropic", model);
    let broader = model_capabilities("deepseek", model);
    assert_eq!(claude, broader.with_reasoning_facts(claude.reasoning_facts));
}
