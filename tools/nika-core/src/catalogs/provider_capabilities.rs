// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-provider capability matrix consumed by `nika check` and runtime
//! defense-in-depth to reject user-requested capabilities that the selected
//! provider does not support.
//!
//! Before Track 2, capability mismatches were handled with `tracing::warn!()`
//! followed by a silent swap (extended_thinking) or silent drop
//! (stop_sequences, tool_choice). Users paid for one provider and got
//! another, or their generation never stopped at the marker they requested.
//! This catalog is the single source of truth wired into NIKA-120 at check
//! time.

/// Well-known canonical provider names. Kept in sync with
/// [`crate::catalogs::providers::KNOWN_PROVIDERS`] for the LLM subset.
const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic", "openai", "groq", "mistral", "deepseek", "gemini", "xai", "native",
];

/// Per-provider capability flags. Every bool answers the question:
/// "If the user sets this field on a task, does the provider actually honour
/// it, or does rig-core / the provider drop it silently?"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    /// Claude-style extended thinking / OpenAI o-series reasoning.
    pub extended_thinking: bool,
    /// Custom stop sequences (`stop_sequences: [...]`). Several providers
    /// accept the field at the API layer but rig-core drops it upstream.
    pub stop_sequences: bool,
    /// `tool_choice: required` — forces the model to emit a tool call.
    /// `auto` and `none` are universal; only `required` needs gating.
    pub tool_choice_required: bool,
    /// Multimodal image input.
    pub vision: bool,
    /// JSON Schema response_format (vs tool-based structured output).
    pub json_schema_response_format: bool,
    /// OpenAI o-series `reasoning_effort` knob.
    pub reasoning_effort: bool,
    /// Deterministic `seed`.
    pub seed: bool,
    /// `top_k` sampling.
    pub top_k: bool,
}

impl ProviderCapabilities {
    /// Empty capability set. Used for unknown providers (conservative).
    pub const fn empty() -> Self {
        Self {
            extended_thinking: false,
            stop_sequences: false,
            tool_choice_required: false,
            vision: false,
            json_schema_response_format: false,
            reasoning_effort: false,
            seed: false,
            top_k: false,
        }
    }

    /// Resolve a provider name (including aliases) to its capability matrix.
    ///
    /// Aliases handled: `claude` → anthropic, `gpt` → openai,
    /// `deep-seek` → deepseek, `grok` → xai, `google` → gemini, `local` →
    /// native. Unknown names return [`Self::empty`].
    pub fn for_provider(name: &str) -> Self {
        match name {
            "anthropic" | "claude" => Self {
                extended_thinking: true,
                stop_sequences: true,
                tool_choice_required: true,
                vision: true,
                json_schema_response_format: false,
                reasoning_effort: false,
                seed: false,
                top_k: true,
            },
            "openai" | "gpt" => Self {
                extended_thinking: true,
                stop_sequences: true,
                tool_choice_required: true,
                vision: true,
                json_schema_response_format: true,
                reasoning_effort: true,
                seed: true,
                top_k: false,
            },
            "groq" => Self {
                extended_thinking: false,
                stop_sequences: false,
                tool_choice_required: false,
                vision: true,
                json_schema_response_format: true,
                reasoning_effort: false,
                seed: true,
                top_k: false,
            },
            "mistral" => Self {
                extended_thinking: false,
                stop_sequences: false,
                tool_choice_required: false,
                vision: true,
                json_schema_response_format: true,
                reasoning_effort: false,
                seed: true,
                top_k: false,
            },
            "deepseek" | "deep-seek" => Self {
                extended_thinking: false,
                stop_sequences: true,
                tool_choice_required: false,
                vision: false,
                json_schema_response_format: true,
                reasoning_effort: false,
                seed: false,
                top_k: false,
            },
            "gemini" | "google" => Self {
                extended_thinking: false,
                stop_sequences: true,
                tool_choice_required: false,
                vision: true,
                json_schema_response_format: true,
                reasoning_effort: false,
                seed: true,
                top_k: true,
            },
            "xai" | "grok" => Self {
                extended_thinking: false,
                stop_sequences: false,
                tool_choice_required: false,
                vision: false,
                json_schema_response_format: false,
                reasoning_effort: false,
                seed: false,
                top_k: false,
            },
            "native" | "local" => Self {
                extended_thinking: false,
                stop_sequences: true,
                tool_choice_required: false,
                vision: false,
                json_schema_response_format: false,
                reasoning_effort: false,
                seed: true,
                top_k: true,
            },
            "mock" => Self {
                extended_thinking: true,
                stop_sequences: true,
                tool_choice_required: true,
                vision: true,
                json_schema_response_format: true,
                reasoning_effort: true,
                seed: true,
                top_k: true,
            },
            _ => Self::empty(),
        }
    }

    /// Dynamic lookup by capability name. Returns `false` for unknown names.
    pub fn supports(&self, capability: &str) -> bool {
        match capability {
            "extended_thinking" => self.extended_thinking,
            "stop_sequences" => self.stop_sequences,
            "tool_choice_required" => self.tool_choice_required,
            "vision" => self.vision,
            "json_schema_response_format" => self.json_schema_response_format,
            "reasoning_effort" => self.reasoning_effort,
            "seed" => self.seed,
            "top_k" => self.top_k,
            _ => false,
        }
    }

    /// List canonical provider names that support `capability`. Used to build
    /// the actionable help text in [`NIKA-120`](https://...) errors.
    ///
    /// Excludes aliases and `mock` (tests only).
    pub fn providers_supporting(capability: &str) -> Vec<&'static str> {
        KNOWN_PROVIDERS
            .iter()
            .copied()
            .filter(|p| Self::for_provider(p).supports(capability))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_supports_extended_thinking() {
        let caps = ProviderCapabilities::for_provider("anthropic");
        assert!(caps.extended_thinking);
        let alias = ProviderCapabilities::for_provider("claude");
        assert!(alias.extended_thinking);
        assert_eq!(caps, alias);
    }

    #[test]
    fn groq_does_not_support_extended_thinking_nor_stop() {
        let caps = ProviderCapabilities::for_provider("groq");
        assert!(!caps.extended_thinking);
        assert!(!caps.stop_sequences);
        assert!(!caps.tool_choice_required);
        assert!(caps.vision);
    }

    #[test]
    fn mistral_does_not_support_thinking_nor_stop_but_has_vision() {
        let caps = ProviderCapabilities::for_provider("mistral");
        assert!(!caps.extended_thinking);
        assert!(!caps.stop_sequences);
        assert!(!caps.tool_choice_required);
        assert!(caps.vision);
    }

    #[test]
    fn deepseek_has_stop_but_no_vision_or_thinking() {
        let caps = ProviderCapabilities::for_provider("deepseek");
        assert!(!caps.vision);
        assert!(!caps.extended_thinking);
        assert!(caps.stop_sequences);
        let alias = ProviderCapabilities::for_provider("deep-seek");
        assert_eq!(caps, alias);
    }

    #[test]
    fn xai_is_conservative_profile() {
        let caps = ProviderCapabilities::for_provider("xai");
        assert!(!caps.extended_thinking);
        assert!(!caps.stop_sequences);
        assert!(!caps.vision);
        let alias = ProviderCapabilities::for_provider("grok");
        assert_eq!(caps, alias);
    }

    #[test]
    fn mock_supports_everything() {
        let caps = ProviderCapabilities::for_provider("mock");
        assert!(caps.extended_thinking);
        assert!(caps.stop_sequences);
        assert!(caps.tool_choice_required);
        assert!(caps.vision);
        assert!(caps.json_schema_response_format);
        assert!(caps.reasoning_effort);
        assert!(caps.seed);
        assert!(caps.top_k);
    }

    #[test]
    fn unknown_provider_is_empty() {
        let caps = ProviderCapabilities::for_provider("not-a-real-provider");
        assert_eq!(caps, ProviderCapabilities::empty());
        assert!(!caps.extended_thinking);
        assert!(!caps.vision);
    }

    #[test]
    fn supports_string_lookup_matches_fields() {
        let caps = ProviderCapabilities::for_provider("anthropic");
        assert_eq!(caps.supports("extended_thinking"), caps.extended_thinking);
        assert_eq!(caps.supports("vision"), caps.vision);
        assert_eq!(caps.supports("tool_choice_required"), caps.tool_choice_required);
        assert!(!caps.supports("not-a-capability"));
    }

    #[test]
    fn providers_supporting_extended_thinking_is_anthropic_and_openai() {
        let list = ProviderCapabilities::providers_supporting("extended_thinking");
        assert!(list.contains(&"anthropic"));
        assert!(list.contains(&"openai"));
        assert!(!list.contains(&"groq"));
        assert!(!list.contains(&"mistral"));
        assert!(!list.contains(&"xai"));
        assert!(!list.contains(&"mock"), "mock must not leak into actionable help");
    }

    #[test]
    fn providers_supporting_vision_excludes_deepseek_and_xai() {
        let list = ProviderCapabilities::providers_supporting("vision");
        assert!(list.contains(&"anthropic"));
        assert!(list.contains(&"openai"));
        assert!(list.contains(&"groq"));
        assert!(list.contains(&"mistral"));
        assert!(list.contains(&"gemini"));
        assert!(!list.contains(&"deepseek"));
        assert!(!list.contains(&"xai"));
        assert!(!list.contains(&"native"));
    }

    #[test]
    fn empty_matches_default_false_for_every_capability() {
        let caps = ProviderCapabilities::empty();
        for cap in [
            "extended_thinking",
            "stop_sequences",
            "tool_choice_required",
            "vision",
            "json_schema_response_format",
            "reasoning_effort",
            "seed",
            "top_k",
        ] {
            assert!(!caps.supports(cap), "empty must say false for {cap}");
        }
    }
}
