//! Model capabilities catalog — single source of truth for model behavior.
//!
//! Every model-specific conditional in the codebase MUST read from here
//! instead of hardcoding `starts_with("o3")` or `is_reasoning_model()` checks.
//!
//! # Design
//!
//! Three orthogonal concerns, NOT a single "is_reasoning" flag:
//!
//! 1. **Token limit parameter** — `max_tokens` vs `max_completion_tokens`
//! 2. **Temperature support** — some models reject it with HTTP 400
//! 3. **Capabilities** — thinking, vision, stop sequences
//!
//! Provider-aware: `o3` on OpenAI → `max_completion_tokens`.
//! `o3-finetune` on a custom vLLM endpoint → `max_tokens` (safe default).

/// How to send the token limit to the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenLimitParam {
    /// Standard: `"max_tokens"` in JSON body
    MaxTokens,
    /// OpenAI reasoning models (o-series, gpt-5.x): `"max_completion_tokens"`
    MaxCompletionTokens,
}

/// Capabilities of a specific model on a specific provider.
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    /// Which JSON field to use for token limits.
    pub token_limit_param: TokenLimitParam,
    /// Model accepts the `temperature` parameter.
    pub supports_temperature: bool,
    /// Model accepts `stop` / `stop_sequences` parameter.
    pub supports_stop_sequences: bool,
    /// Model supports extended thinking (Claude) or reasoning_effort (OpenAI).
    pub supports_thinking: bool,
    /// Model supports image/vision input.
    pub supports_vision: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            token_limit_param: TokenLimitParam::MaxTokens,
            supports_temperature: true,
            supports_stop_sequences: true,
            supports_thinking: false,
            supports_vision: true,
        }
    }
}

/// Resolve capabilities for a given provider + model combination.
///
/// **Provider-aware:** The same model name gets different treatment depending
/// on the provider. `o3` on OpenAI gets `max_completion_tokens`, but
/// `o3-finetune` on a custom vLLM endpoint gets `max_tokens`.
///
/// This is the ONLY function that should know about model-specific quirks.
/// All callers in nika-engine should use this instead of hardcoded checks.
pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities {
    let lower = model.to_lowercase();
    let prov = provider.to_lowercase();

    // ── OpenAI-native providers ─────────────────────────────────────
    // Only apply reasoning model rules for actual OpenAI API endpoints.
    // Custom endpoints (vLLM, Ollama) should get safe defaults even if
    // the model name happens to match.
    let is_openai_api = matches!(prov.as_str(), "openai" | "gpt" | "openrouter");

    if is_openai_api {
        // o-series: max_completion_tokens + NO temperature
        if is_o_series(&lower) {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: false,
                supports_thinking: true,
                ..Default::default()
            };
        }

        // gpt-5.x: max_completion_tokens + YES temperature
        if is_gpt5(&lower) {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: true,
                supports_thinking: true,
                ..Default::default()
            };
        }
    }

    // ── Anthropic / Claude ──────────────────────────────────────────
    if prov == "anthropic" || prov == "claude" || lower.starts_with("claude") {
        return ModelCapabilities {
            supports_thinking: true,
            ..Default::default()
        };
    }

    // ── DeepSeek ────────────────────────────────────────────────────
    if prov == "deepseek" {
        if lower == "deepseek-reasoner" {
            return ModelCapabilities {
                // DeepSeek Reasoner uses standard max_tokens (NOT max_completion_tokens)
                supports_temperature: false,
                supports_vision: false,
                ..Default::default()
            };
        }
        return ModelCapabilities {
            supports_vision: false,
            ..Default::default()
        };
    }

    // ── xAI / Grok ─────────────────────────────────────────────────
    if prov == "xai" || prov == "grok" {
        if lower == "grok-4" {
            return ModelCapabilities {
                // grok-4 rejects stop sequences and presence/frequency penalty
                supports_stop_sequences: false,
                ..Default::default()
            };
        }
    }

    // ── Everything else: safe defaults ──────────────────────────────
    ModelCapabilities::default()
}

// ── Private helpers ─────────────────────────────────────────────────

fn is_o_series(lower: &str) -> bool {
    lower == "o1"
        || lower.starts_with("o1-")
        || lower == "o3"
        || lower.starts_with("o3-")
        || lower == "o4"
        || lower.starts_with("o4-")
}

fn is_gpt5(lower: &str) -> bool {
    lower == "gpt-5" || lower.starts_with("gpt-5-") || lower.starts_with("gpt-5.")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── OpenAI o-series ─────────────────────────────────────────────

    #[test]
    fn o3_needs_max_completion_tokens_and_no_temperature() {
        let caps = model_capabilities("openai", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
        assert!(caps.supports_thinking);
    }

    #[test]
    fn o4_mini_needs_max_completion_tokens() {
        let caps = model_capabilities("openai", "o4-mini");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
    }

    #[test]
    fn o1_pro_is_reasoning() {
        let caps = model_capabilities("openai", "o1-pro");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
    }

    // ── OpenAI gpt-5.x: max_completion_tokens BUT supports temperature ──

    #[test]
    fn gpt52_needs_max_completion_tokens_but_supports_temperature() {
        let caps = model_capabilities("openai", "gpt-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn gpt54_all_variants() {
        for model in ["gpt-5.4", "gpt-5.4-mini", "gpt-5.4-nano"] {
            let caps = model_capabilities("openai", model);
            assert_eq!(
                caps.token_limit_param,
                TokenLimitParam::MaxCompletionTokens,
                "{model} should use max_completion_tokens"
            );
            assert!(caps.supports_temperature, "{model} should support temperature");
        }
    }

    #[test]
    fn gpt5_dot_variants_future_proof() {
        for model in [
            "gpt-5.2-pro",
            "gpt-5.2-2025-12-11",
            "gpt-5.3",
            "gpt-5.10-turbo",
        ] {
            let caps = model_capabilities("openai", model);
            assert_eq!(
                caps.token_limit_param,
                TokenLimitParam::MaxCompletionTokens,
                "{model} should use max_completion_tokens"
            );
        }
    }

    // ── gpt-4.x is NOT reasoning ────────────────────────────────────

    #[test]
    fn gpt41_is_not_reasoning() {
        let caps = model_capabilities("openai", "gpt-4.1");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn gpt4o_is_standard() {
        let caps = model_capabilities("openai", "gpt-4o");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(!caps.supports_thinking);
    }

    // ── Anthropic Claude ────────────────────────────────────────────

    #[test]
    fn claude_supports_thinking() {
        let caps = model_capabilities("anthropic", "claude-sonnet-4-6");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(caps.supports_thinking);
    }

    #[test]
    fn claude_alias_provider() {
        let caps = model_capabilities("claude", "claude-opus-4-6");
        assert!(caps.supports_thinking);
    }

    // ── DeepSeek ────────────────────────────────────────────────────

    #[test]
    fn deepseek_reasoner_uses_max_tokens() {
        let caps = model_capabilities("deepseek", "deepseek-reasoner");
        // DeepSeek uses standard max_tokens, NOT max_completion_tokens
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(!caps.supports_temperature);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn deepseek_chat_no_vision() {
        let caps = model_capabilities("deepseek", "deepseek-chat");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(!caps.supports_vision);
    }

    // ── xAI / Grok ─────────────────────────────────────────────────

    #[test]
    fn grok4_rejects_stop_sequences() {
        let caps = model_capabilities("xai", "grok-4");
        assert!(!caps.supports_stop_sequences);
        assert!(caps.supports_temperature);
        assert!(caps.supports_vision);
    }

    #[test]
    fn grok3_supports_stop_sequences() {
        let caps = model_capabilities("xai", "grok-3");
        assert!(caps.supports_stop_sequences);
    }

    // ── Custom endpoints: safe defaults ─────────────────────────────

    #[test]
    fn unknown_model_gets_safe_defaults() {
        let caps = model_capabilities("openai", "future-model-99");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
        assert!(caps.supports_stop_sequences);
    }

    #[test]
    fn custom_endpoint_qwen_on_vllm() {
        let caps = model_capabilities("h100", "Qwen/Qwen3-8B");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn custom_endpoint_reasoning_model_name() {
        // o3-finetune on vLLM should NOT get max_completion_tokens
        // because the endpoint isn't OpenAI
        let caps = model_capabilities("h100", "o3-finetune");
        assert_eq!(
            caps.token_limit_param,
            TokenLimitParam::MaxTokens,
            "Custom endpoints should use max_tokens even for o3-like names"
        );
    }

    // ── OpenRouter proxies ──────────────────────────────────────────

    #[test]
    fn openrouter_o3_gets_openai_treatment() {
        let caps = model_capabilities("openrouter", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── Case insensitive ────────────────────────────────────────────

    #[test]
    fn case_insensitive() {
        let caps = model_capabilities("OpenAI", "GPT-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── Standard providers: safe defaults ───────────────────────────

    #[test]
    fn gemini_standard() {
        let caps = model_capabilities("gemini", "gemini-2.5-flash");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn mistral_standard() {
        let caps = model_capabilities("mistral", "mistral-large-latest");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn groq_standard() {
        let caps = model_capabilities("groq", "llama-3.3-70b-versatile");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }
}
