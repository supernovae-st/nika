// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The request controls a provider's API refuses on a model, as the catalog records them: a
//! sampling parameter other than its default, and a thinking budget. Anthropic's Messages API
//! answers both with HTTP 400 on a model whose capability rule takes no temperature or lists no
//! thinking-budget parameter while recording its reasoning (the Claude 5 models fix their
//! sampling and think adaptively). The `openai` provider's own API answers a temperature other
//! than the default the same way on a model whose rule takes none (the o-series, GPT-5, GPT-5
//! mini and GPT-5 nano). Another OpenAI-compatible route, `openrouter` included, may drop such
//! a control instead, so no refusal is claimed there.

use crate::data::generated::find_provider;
use crate::data::models::model_capabilities;
use crate::types::ParamFlag;
use crate::types::model::ReasoningFacts;

/// The wire model a catalog nickname names (`anthropic`/`sonnet` names `claude-sonnet-5-5`);
/// `None` for any other name, which is its own wire id.
#[must_use]
pub fn nickname_wire(provider: &str, name: &str) -> Option<&'static str> {
    find_provider(provider)
        .and_then(|row| row.models.iter().find(|m| m.id == name))
        .map(|m| m.model)
}

/// The wire model `provider`/`name` names: a nickname's model, any other name as given.
fn wire_model<'a>(provider: &str, name: &'a str) -> &'a str {
    nickname_wire(provider, name).unwrap_or(name)
}

/// Whether `provider` speaks Anthropic's Messages API, where a refused control is an error.
fn anthropic_route(provider: &str) -> bool {
    find_provider(provider).and_then(|row| row.api_dialect) == Some("anthropic")
}

/// Whether `provider` is the `openai` provider (its id or an alias), whose own API answers a
/// refused sampling control with an error rather than dropping it.
fn openai_route(provider: &str) -> bool {
    find_provider(provider).is_some_and(|row| row.id == "openai")
}

/// Whether the provider's API refuses a sampling parameter (`temperature`, `top_p`, `top_k`)
/// other than its default on `provider`/`name` with an error: an Anthropic route or the
/// `openai` provider whose model takes no temperature. `false` wherever the catalog records no
/// such refusal.
#[must_use]
pub fn refuses_sampling(provider: &str, name: &str) -> bool {
    (anthropic_route(provider) || openai_route(provider))
        && !model_capabilities(provider, wire_model(provider, name)).supports_temperature
}

/// Whether the provider's API refuses a thinking budget (`thinking: { budget_tokens }`) on
/// `provider`/`name` with an error: an Anthropic route whose model's rule records its reasoning
/// and lists no thinking-budget parameter (it thinks adaptively, steered by effort). `false`
/// wherever the catalog records no such refusal, an unrecorded model included.
#[must_use]
pub fn refuses_thinking_budget(provider: &str, name: &str) -> bool {
    if !anthropic_route(provider) {
        return false;
    }
    let caps = model_capabilities(provider, wire_model(provider, name));
    caps.reasoning
        && caps.reasoning_facts != ReasoningFacts::default()
        && !caps
            .supported_parameters
            .contains(&ParamFlag::ThinkingBudget)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Claude 5 models refuse both controls on Anthropic's API, under their wire id, a
    /// nickname that names them or the provider alias; earlier Claude models accept both.
    #[test]
    fn the_claude_5_models_refuse_sampling_and_a_thinking_budget() {
        for (provider, name) in [
            ("anthropic", "claude-sonnet-5-5"),
            ("anthropic", "claude-opus-5-5"),
            ("anthropic", "claude-haiku-5-5"),
            ("anthropic", "claude-fable-5-1"),
            ("anthropic", "claude-fable-5"),
            ("anthropic", "claude-opus-5"),
            ("anthropic", "claude-sonnet-5"),
            ("anthropic", "sonnet"),
            ("anthropic", "opus"),
            ("anthropic", "haiku"),
            ("claude", "claude-sonnet-5-5"),
        ] {
            assert!(refuses_sampling(provider, name), "{provider}/{name}");
            assert!(refuses_thinking_budget(provider, name), "{provider}/{name}");
        }
        for name in [
            "claude-sonnet-4-6",
            "claude-sonnet-4-5-20250929",
            "claude-haiku-4-5-20251001",
        ] {
            assert!(!refuses_sampling("anthropic", name), "{name}");
            assert!(!refuses_thinking_budget("anthropic", name), "{name}");
        }
    }

    /// The `openai` provider's own API answers a temperature other than the default with
    /// HTTP 400 on the reasoning seats that fix their sampling (measured on 0.123.2): the
    /// o-series and GPT-5, GPT-5 mini and GPT-5 nano. A model that takes a temperature, a later
    /// dot variant included, keeps it, and a compatible route that drops the control claims no
    /// refusal.
    #[test]
    fn openai_reasoning_seats_refuse_sampling_on_openai_only() {
        for name in [
            "gpt-5",
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-5-mini-2025-08-07",
            "o3",
        ] {
            assert!(refuses_sampling("openai", name), "openai/{name}");
            assert!(
                refuses_sampling("gpt", name),
                "the provider alias: gpt/{name}"
            );
            assert!(!refuses_thinking_budget("openai", name), "openai/{name}");
        }
        for (provider, name) in [
            ("openai", "gpt-4o"),
            ("openai", "gpt-5.1"),
            ("openai", "gpt-5-chat-latest"),
            ("openrouter", "o3"),
            ("openrouter", "gpt-5-mini"),
        ] {
            assert!(!refuses_sampling(provider, name), "{provider}/{name}");
        }
    }

    /// A model the catalog records no refusal for gets none: an unrecorded Anthropic name, a
    /// Claude model reached through another dialect, and an OpenAI-compatible route whose model
    /// takes no temperature (that route ignores it rather than refusing).
    #[test]
    fn no_refusal_is_claimed_where_the_catalog_records_none() {
        for (provider, name) in [
            ("anthropic", "some-future-model"),
            ("openrouter", "anthropic/claude-sonnet-5.5"),
            ("deepseek", "deepseek-v4-pro"),
            ("openrouter", "o3"),
            ("mock", "echo"),
        ] {
            assert!(!refuses_sampling(provider, name), "{provider}/{name}");
            assert!(
                !refuses_thinking_budget(provider, name),
                "{provider}/{name}"
            );
        }
        assert_eq!(
            nickname_wire("anthropic", "sonnet"),
            Some("claude-sonnet-5-5")
        );
        assert_eq!(nickname_wire("claude", "opus"), Some("claude-opus-5-5"));
        assert_eq!(nickname_wire("anthropic", "claude-x"), None);
        assert_eq!(nickname_wire("nowhere", "sonnet"), None);
        assert_eq!(wire_model("anthropic", "claude-x"), "claude-x");
    }
}
