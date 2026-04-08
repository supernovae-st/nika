// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Action Parameters
//!
//! The `agent:` verb enables agentic execution with MCP tool access.
//! Unlike `infer:` (one-shot LLM call), `agent:` runs in a loop with
//! tool calling capabilities.
//!
//! # Example
//!
//! ```yaml
//! - agent:
//!     prompt: |
//!       Generate native content for the homepage hero block.
//!       Use @entity:qr-code-generator for the main concept.
//!     provider: claude
//!     model: claude-sonnet-4-6
//!     mcp:
//!       - novanet
//!     max_turns: 10
//! ```

use nika_core::ProviderName;
use serde::Deserialize;

use crate::ast::completion::CompletionConfig;
use crate::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// Tool Choice
// ═══════════════════════════════════════════════════════════════════════════

/// Tool choice behavior for agent execution
///
/// Controls when the agent uses tools during execution.
///
/// # YAML Syntax
/// ```yaml
/// agent:
///   prompt: "..."
///   tool_choice: auto    # auto | required | none
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    /// LLM decides when to use tools (default behavior)
    #[default]
    Auto,
    /// Must use at least one tool per turn
    Required,
    /// Never use tools (text-only response)
    None,
}

impl ToolChoice {
    /// Convert to rig-core compatible string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Required => "required",
            Self::None => "none",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Default maximum turns for agent loop
const DEFAULT_MAX_TURNS: u32 = 10;

/// Maximum allowed turns to prevent runaway agents
const MAX_ALLOWED_TURNS: u32 = 100;

/// Default thinking budget for extended thinking (4096 tokens)
const DEFAULT_THINKING_BUDGET: u64 = 4096;

/// Default depth limit for nested agent spawning (MVP 8 Phase 2)
const DEFAULT_DEPTH_LIMIT: u32 = 3;

/// Maximum allowed depth limit (prevent deep recursion)
const MAX_DEPTH_LIMIT: u32 = 10;

/// Parameters for the `agent:` verb
///
/// Enables agentic execution with MCP tool access. The agent runs
/// in a loop, calling tools as needed until a stop condition is met
/// or max_turns is reached.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentParams {
    /// System/user prompt for the agent
    pub prompt: String,

    /// System prompt (optional, sets the agent's behavior/persona)
    #[serde(default)]
    pub system: Option<String>,

    /// LLM provider override (defaults to workflow provider)
    #[serde(default)]
    pub provider: Option<ProviderName>,

    /// Model override (defaults to workflow model)
    #[serde(default)]
    pub model: Option<String>,

    /// MCP servers the agent can access for tool calling
    #[serde(default)]
    pub mcp: Vec<String>,

    /// Builtin tools the agent can use (nika:read, nika:write, etc.)
    #[serde(default)]
    pub tools: Vec<String>,

    /// Maximum agentic turns before stopping
    #[serde(default)]
    pub max_turns: Option<u32>,

    /// Token budget for the entire agent session
    /// Stops gracefully when budget is exceeded
    #[serde(default)]
    pub token_budget: Option<u32>,

    /// Sequences that stop generation (passed to LLM)
    #[serde(default)]
    pub stop_sequences: Vec<String>,

    /// Scope preset (full, minimal, debug)
    #[serde(default)]
    pub scope: Option<String>,

    /// Enable extended thinking for Claude models
    ///
    /// When enabled, the agent captures Claude's reasoning process
    /// in the `thinking` field of AgentTurn events. Only supported
    /// for Claude provider.
    #[serde(default)]
    pub extended_thinking: Option<bool>,

    /// Token budget for extended thinking (default: 4096)
    ///
    /// Controls how many tokens Claude can use for reasoning in
    /// extended thinking mode. Higher values allow deeper analysis
    /// but cost more. Only used when `extended_thinking: true`.
    #[serde(default)]
    pub thinking_budget: Option<u64>,

    /// Maximum depth for nested agent spawning (default: 3, max: 10)
    ///
    /// Controls how many levels of sub-agents can be spawned recursively.
    /// A depth_limit of 1 means no sub-agents can be spawned.
    /// Used by `spawn_agent` internal tool to prevent infinite recursion.
    #[serde(default)]
    pub depth_limit: Option<u32>,

    /// Tool choice behavior
    ///
    /// Controls when the agent uses tools:
    /// - `auto` (default): LLM decides when to use tools
    /// - `required`: Must use at least one tool per turn
    /// - `none`: Never use tools (text-only response)
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,

    /// Model temperature for response randomness
    ///
    /// Controls the randomness/creativity of responses:
    /// - 0.0: Deterministic, most focused
    /// - 1.0: Balanced creativity (typical default)
    /// - 2.0: Maximum creativity/randomness
    ///
    /// If not set, uses the provider's default.
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate per turn
    ///
    /// Required when using `extended_thinking: true` with Claude.
    /// Must be greater than `thinking_budget`. If not set with extended
    /// thinking enabled, defaults to `thinking_budget + 8192`.
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Skills to inject into the agent's system prompt
    ///
    /// List of skill aliases that will be loaded and prepended to
    /// the system prompt before each LLM call. Skills are resolved
    /// from the workflow's `skills:` map.
    #[serde(default)]
    pub skills: Option<Vec<String>>,

    /// Completion behavior configuration
    ///
    /// Controls how the agent signals task completion:
    /// - `mode: explicit` - Agent must call `nika:complete` tool (recommended)
    /// - `mode: natural` - Completes when agent has no more tool calls
    /// - `mode: pattern` - Completes on pattern match
    ///
    /// When not set, defaults to `natural` mode.
    #[serde(default)]
    pub completion: Option<CompletionConfig>,

    /// Guardrails for validating agent outputs
    ///
    /// Guardrails are checked after the agent signals completion.
    /// If any guardrail fails, the agent is asked to retry.
    #[serde(default)]
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,

    /// Execution limits for cost control
    ///
    /// Limits control resource consumption:
    /// - `max_turns`: Maximum agentic loop iterations
    /// - `max_tokens`: Total token budget (input + output)
    /// - `max_cost_usd`: Cost ceiling per execution
    /// - `max_duration_secs`: Wall-clock timeout
    ///
    /// When a limit is reached, the action specified in `on_limit_reached`
    /// is taken (complete_partial, fail, or escalate).
    #[serde(default)]
    pub limits: Option<crate::ast::limits::LimitsConfig>,

    /// Provider fallback chain: try providers in order until one succeeds.
    /// Set from `provider: [groq, anthropic]` or `routing.fallback`.
    #[serde(default)]
    pub provider_chain: Option<Vec<ProviderName>>,
}

impl AgentParams {
    /// Get effective max turns (with default).
    ///
    /// Returns the configured `max_turns` if set, otherwise returns
    /// the default value of 10.
    #[inline]
    pub fn effective_max_turns(&self) -> u32 {
        self.max_turns.unwrap_or(DEFAULT_MAX_TURNS)
    }

    /// Get effective token budget (with default).
    ///
    /// Returns the configured `token_budget` if set, otherwise returns
    /// `u32::MAX` (effectively unlimited).
    #[inline]
    pub fn effective_token_budget(&self) -> u32 {
        self.token_budget.unwrap_or(u32::MAX)
    }

    /// Get effective thinking budget (with default).
    ///
    /// Returns the configured `thinking_budget` if set, otherwise returns
    /// the default value of 4096 tokens.
    #[inline]
    pub fn effective_thinking_budget(&self) -> u64 {
        self.thinking_budget.unwrap_or(DEFAULT_THINKING_BUDGET)
    }

    /// Get effective depth limit for nested agent spawning (with default).
    ///
    /// Returns the configured `depth_limit` if set, otherwise returns
    /// the default value of 3.
    #[inline]
    pub fn effective_depth_limit(&self) -> u32 {
        self.depth_limit.unwrap_or(DEFAULT_DEPTH_LIMIT)
    }

    /// Get effective max tokens for LLM calls.
    ///
    /// When `extended_thinking` is enabled, Claude requires `max_tokens > thinking_budget`.
    /// If `max_tokens` is not set but extended thinking is enabled, this returns
    /// `thinking_budget + 8192` to ensure valid configuration.
    ///
    /// Returns `None` if neither max_tokens is set nor extended_thinking is enabled.
    #[inline]
    pub fn effective_max_tokens(&self) -> Option<u32> {
        if let Some(max_tokens) = self.max_tokens {
            Some(max_tokens)
        } else if self.extended_thinking.unwrap_or(false) {
            // Claude requires max_tokens > thinking_budget for extended thinking
            let thinking_budget = self.effective_thinking_budget() as u32;
            Some(thinking_budget + 8192)
        } else {
            None
        }
    }

    /// Get effective tool choice behavior.
    ///
    /// Returns the configured `tool_choice` if set, otherwise returns
    /// `ToolChoice::Auto` (LLM decides when to use tools).
    #[inline]
    pub fn effective_tool_choice(&self) -> ToolChoice {
        self.tool_choice.clone().unwrap_or_default()
    }

    /// Check if tool_choice was explicitly set by the user.
    ///
    /// Returns `true` only if the user explicitly specified `tool_choice` in YAML.
    /// Used to avoid redundant `.tool_choice(Auto)` calls when the default is sufficient.
    ///
    /// # Why This Matters
    ///
    /// Some providers handle `tool_choice` differently:
    /// - **Perplexity**: Logs warning, ignores tool_choice
    /// - **Cohere**: Rejects `Auto` explicitly (rig-core skips if Auto)
    /// - **Claude/OpenAI**: Full support
    ///
    /// By only setting tool_choice when explicit, we avoid:
    /// 1. Unnecessary API calls with default values
    /// 2. Provider-specific edge cases with Auto
    #[inline]
    pub fn has_explicit_tool_choice(&self) -> bool {
        self.tool_choice.is_some()
    }

    /// Get effective temperature.
    ///
    /// Returns the configured `temperature` if set, otherwise returns `None`
    /// to use the provider's default.
    #[inline]
    pub fn effective_temperature(&self) -> Option<f32> {
        self.temperature
    }

    /// Get effective completion configuration.
    ///
    /// Returns the completion config if set, otherwise None (natural mode).
    pub fn effective_completion(&self) -> Option<CompletionConfig> {
        self.completion.clone()
    }

    /// Generate system instruction for completion.
    ///
    /// Returns the auto-generated instruction to append to the system prompt,
    /// or empty string if no special completion instruction is needed.
    pub fn completion_system_instruction(&self) -> String {
        self.completion
            .as_ref()
            .map(|c| c.generate_system_instruction())
            .unwrap_or_default()
    }

    /// Validate agent parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `prompt` is empty
    /// - `max_turns` is 0 or exceeds 100
    /// - `token_budget` is 0
    pub fn validate(&self) -> Result<(), NikaError> {
        if self.prompt.is_empty() {
            return Err(NikaError::ValidationError {
                reason: "Agent prompt cannot be empty".into(),
            });
        }

        if let Some(max) = self.max_turns {
            if max == 0 {
                return Err(NikaError::ValidationError {
                    reason: "max_turns must be > 0".into(),
                });
            }
            if max > MAX_ALLOWED_TURNS {
                return Err(NikaError::ValidationError {
                    reason: format!("max_turns cannot exceed {}", MAX_ALLOWED_TURNS),
                });
            }
        }

        if let Some(budget) = self.token_budget {
            if budget == 0 {
                return Err(NikaError::ValidationError {
                    reason: "token_budget must be > 0".into(),
                });
            }
        }

        // Graceful degradation: extended_thinking with non-Claude provider
        if self.extended_thinking == Some(true) {
            if let Some(ref provider) = self.provider {
                if *provider != ProviderName::Anthropic {
                    tracing::warn!(
                        provider = %provider,
                        "extended_thinking: true ignored — only supported by Claude provider"
                    );
                }
            }
        }

        // Validate depth_limit (MVP 8 Phase 2)
        if let Some(depth) = self.depth_limit {
            if depth == 0 {
                return Err(NikaError::ValidationError {
                    reason: "depth_limit must be > 0".into(),
                });
            }
            if depth > MAX_DEPTH_LIMIT {
                return Err(NikaError::ValidationError {
                    reason: format!("depth_limit cannot exceed {}", MAX_DEPTH_LIMIT),
                });
            }
        }

        // Validate temperature
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(NikaError::ValidationError {
                    reason: format!("temperature must be between 0.0 and 2.0, got {}", temp),
                });
            }
        }

        // Validate completion config
        if let Some(ref completion) = self.completion {
            completion.validate()?;
        }

        // Validate limits config
        if let Some(ref limits) = self.limits {
            limits.validate()?;
        }

        Ok(())
    }

    /// Get effective limits configuration.
    ///
    /// Returns the limits config if set, or a default (no limits) otherwise.
    pub fn effective_limits(&self) -> crate::ast::limits::LimitsConfig {
        self.limits.clone().unwrap_or_default()
    }

    /// Check if any limits are configured.
    pub fn has_limits(&self) -> bool {
        self.limits
            .as_ref()
            .map(|l| l.has_limits())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn parse_agent_params_basic() {
        let yaml = r#"
prompt: "Test prompt"
provider: claude
model: claude-sonnet-4-6
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.prompt, "Test prompt");
        assert_eq!(params.provider, Some(ProviderName::Anthropic));
        assert_eq!(params.model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn parse_agent_params_mcp_list() {
        let yaml = r#"
prompt: "Test"
mcp:
  - novanet
  - filesystem
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.mcp, vec!["novanet", "filesystem"]);
    }

    #[test]
    fn effective_max_turns_default() {
        let params = AgentParams::default();
        assert_eq!(params.effective_max_turns(), DEFAULT_MAX_TURNS);
    }

    #[test]
    fn effective_max_turns_custom() {
        let params = AgentParams {
            max_turns: Some(20),
            ..Default::default()
        };
        assert_eq!(params.effective_max_turns(), 20);
    }

    #[test]
    fn validate_empty_prompt() {
        let params = AgentParams::default();
        let result = params.validate();
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    #[test]
    fn validate_zero_max_turns() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(0),
            ..Default::default()
        };
        let result = params.validate();
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    #[test]
    fn validate_excessive_max_turns() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(101),
            ..Default::default()
        };
        let result = params.validate();
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    #[test]
    fn validate_ok() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(50),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ========================================================================
    // Token Budget Tests
    // ========================================================================

    #[test]
    fn parse_token_budget() {
        let yaml = r#"
prompt: "Test"
token_budget: 100000
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.token_budget, Some(100000));
    }

    #[test]
    fn effective_token_budget_default() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        // Default should be unlimited (None -> max u32)
        assert_eq!(params.effective_token_budget(), u32::MAX);
    }

    #[test]
    fn effective_token_budget_custom() {
        let params = AgentParams {
            prompt: "test".to_string(),
            token_budget: Some(50000),
            ..Default::default()
        };
        assert_eq!(params.effective_token_budget(), 50000);
    }

    #[test]
    fn validate_zero_token_budget() {
        let params = AgentParams {
            prompt: "test".to_string(),
            token_budget: Some(0),
            ..Default::default()
        };
        let result = params.validate();
        assert!(
            result.is_err(),
            "Should fail but got: {:?}",
            result.unwrap()
        );
    }

    // ========================================================================
    // System Prompt Tests
    // ========================================================================

    #[test]
    fn parse_system_prompt() {
        let yaml = r#"
prompt: "User prompt"
system: "You are a helpful assistant."
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            params.system,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    fn system_prompt_defaults_to_none() {
        let params = AgentParams::default();
        assert!(params.system.is_none());
    }

    // ========================================================================
    // Extended Thinking Tests
    // ========================================================================

    #[test]
    fn parse_extended_thinking_true() {
        let yaml = r#"
prompt: "Test"
extended_thinking: true
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.extended_thinking, Some(true));
    }

    #[test]
    fn parse_extended_thinking_false() {
        let yaml = r#"
prompt: "Test"
extended_thinking: false
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.extended_thinking, Some(false));
    }

    #[test]
    fn extended_thinking_defaults_to_none() {
        let params = AgentParams::default();
        assert!(params.extended_thinking.is_none());
    }

    #[test]
    fn validate_extended_thinking_with_openai_degrades_gracefully() {
        // Previously returned Err; now it just warns and succeeds
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            provider: Some(ProviderName::OpenAI),
            ..Default::default()
        };
        assert!(
            params.validate().is_ok(),
            "should degrade gracefully, not crash"
        );
    }

    #[test]
    fn validate_extended_thinking_with_claude_ok() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            provider: Some(ProviderName::Anthropic),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn validate_extended_thinking_without_provider_ok() {
        // When provider is not specified, validation passes
        // (workflow provider will be used at runtime)
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            provider: None,
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ========================================================================
    // Thinking Budget Tests
    // ========================================================================

    #[test]
    fn parse_thinking_budget() {
        let yaml = r#"
prompt: "Test"
extended_thinking: true
thinking_budget: 8192
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.thinking_budget, Some(8192));
    }

    #[test]
    fn effective_thinking_budget_default() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        // Default should be 4096
        assert_eq!(params.effective_thinking_budget(), DEFAULT_THINKING_BUDGET);
        assert_eq!(params.effective_thinking_budget(), 4096);
    }

    #[test]
    fn effective_thinking_budget_custom() {
        let params = AgentParams {
            prompt: "test".to_string(),
            thinking_budget: Some(16384),
            ..Default::default()
        };
        assert_eq!(params.effective_thinking_budget(), 16384);
    }

    #[test]
    fn thinking_budget_defaults_to_none() {
        let params = AgentParams::default();
        assert!(params.thinking_budget.is_none());
    }

    // ========================================================================
    // MaxTokens Tests
    // ========================================================================

    #[test]
    fn effective_max_tokens_explicit() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_tokens: Some(16384),
            ..Default::default()
        };
        assert_eq!(params.effective_max_tokens(), Some(16384));
    }

    #[test]
    fn effective_max_tokens_with_extended_thinking() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            thinking_budget: Some(8192),
            max_tokens: None, // Not set explicitly
            ..Default::default()
        };
        // Should return thinking_budget + 8192
        assert_eq!(params.effective_max_tokens(), Some(8192 + 8192));
    }

    #[test]
    fn effective_max_tokens_explicit_overrides_auto() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            thinking_budget: Some(8192),
            max_tokens: Some(32768), // Explicitly set
            ..Default::default()
        };
        // Explicit value should be used
        assert_eq!(params.effective_max_tokens(), Some(32768));
    }

    #[test]
    fn effective_max_tokens_none_without_thinking() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: None,
            max_tokens: None,
            ..Default::default()
        };
        // Should return None (no override needed)
        assert_eq!(params.effective_max_tokens(), None);
    }

    // ========================================================================
    // ToolChoice Tests
    // ========================================================================

    #[test]
    fn test_parse_tool_choice_auto() {
        let yaml = r#"
prompt: "Test"
tool_choice: auto
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.tool_choice, Some(ToolChoice::Auto));
    }

    #[test]
    fn test_parse_tool_choice_required() {
        let yaml = r#"
prompt: "Test"
tool_choice: required
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.tool_choice, Some(ToolChoice::Required));
    }

    #[test]
    fn test_parse_tool_choice_none() {
        let yaml = r#"
prompt: "Test"
tool_choice: none
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.tool_choice, Some(ToolChoice::None));
    }

    #[test]
    fn test_tool_choice_default() {
        let params = AgentParams::default();
        assert!(params.tool_choice.is_none());
        assert_eq!(params.effective_tool_choice(), ToolChoice::Auto);
    }

    #[test]
    fn test_tool_choice_as_str() {
        assert_eq!(ToolChoice::Auto.as_str(), "auto");
        assert_eq!(ToolChoice::Required.as_str(), "required");
        assert_eq!(ToolChoice::None.as_str(), "none");
    }

    // ========================================================================
    // Temperature Tests
    // ========================================================================

    #[test]
    fn test_parse_temperature() {
        let yaml = r#"
prompt: "Test"
temperature: 0.7
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.temperature, Some(0.7));
    }

    #[test]
    fn test_temperature_default() {
        let params = AgentParams::default();
        assert!(params.temperature.is_none());
        assert_eq!(params.effective_temperature(), None);
    }

    #[test]
    fn test_temperature_validation_valid_range() {
        // Valid range: 0.0 to 2.0
        for temp in [0.0, 0.5, 1.0, 1.5, 2.0] {
            let params = AgentParams {
                prompt: "test".to_string(),
                temperature: Some(temp),
                ..Default::default()
            };
            assert!(
                params.validate().is_ok(),
                "temperature {} should be valid",
                temp
            );
        }
    }

    #[test]
    fn test_temperature_validation_too_low() {
        let params = AgentParams {
            prompt: "test".to_string(),
            temperature: Some(-0.1),
            ..Default::default()
        };
        let err = params.validate().unwrap_err();
        assert!(err
            .to_string()
            .contains("temperature must be between 0.0 and 2.0"));
    }

    #[test]
    fn test_temperature_validation_too_high() {
        let params = AgentParams {
            prompt: "test".to_string(),
            temperature: Some(2.1),
            ..Default::default()
        };
        let err = params.validate().unwrap_err();
        assert!(err
            .to_string()
            .contains("temperature must be between 0.0 and 2.0"));
    }

    #[test]
    fn test_effective_temperature_custom() {
        let params = AgentParams {
            prompt: "test".to_string(),
            temperature: Some(0.3),
            ..Default::default()
        };
        assert_eq!(params.effective_temperature(), Some(0.3));
    }

    #[test]
    fn test_combined_tool_choice_and_temperature() {
        let yaml = r#"
prompt: "Generate creative content"
tool_choice: required
temperature: 1.5
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.tool_choice, Some(ToolChoice::Required));
        assert_eq!(params.temperature, Some(1.5));
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // has_explicit_tool_choice Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_has_explicit_tool_choice_when_not_set() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        assert!(!params.has_explicit_tool_choice());
        // effective_tool_choice still returns Auto (default)
        assert_eq!(params.effective_tool_choice(), ToolChoice::Auto);
    }

    #[test]
    fn test_has_explicit_tool_choice_when_auto() {
        let params = AgentParams {
            prompt: "test".to_string(),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };
        // User explicitly set Auto, so has_explicit returns true
        assert!(params.has_explicit_tool_choice());
        assert_eq!(params.effective_tool_choice(), ToolChoice::Auto);
    }

    #[test]
    fn test_has_explicit_tool_choice_when_required() {
        let params = AgentParams {
            prompt: "test".to_string(),
            tool_choice: Some(ToolChoice::Required),
            ..Default::default()
        };
        assert!(params.has_explicit_tool_choice());
        assert_eq!(params.effective_tool_choice(), ToolChoice::Required);
    }

    #[test]
    fn test_has_explicit_tool_choice_when_none() {
        let params = AgentParams {
            prompt: "test".to_string(),
            tool_choice: Some(ToolChoice::None),
            ..Default::default()
        };
        assert!(params.has_explicit_tool_choice());
        assert_eq!(params.effective_tool_choice(), ToolChoice::None);
    }

    #[test]
    fn test_has_explicit_tool_choice_from_yaml_absent() {
        let yaml = r#"
prompt: "Test prompt"
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(!params.has_explicit_tool_choice());
    }

    #[test]
    fn test_has_explicit_tool_choice_from_yaml_present() {
        let yaml = r#"
prompt: "Test prompt"
tool_choice: none
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.has_explicit_tool_choice());
        assert_eq!(params.effective_tool_choice(), ToolChoice::None);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Completion Config Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_completion_explicit_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: explicit
  signal:
    fields:
      required: [result]
      optional: [confidence]
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.completion.is_some());

        let completion = params.completion.clone().unwrap();
        assert_eq!(completion.mode, crate::ast::CompletionMode::Explicit);
        assert!(completion.signal.is_some());
    }

    #[test]
    fn test_parse_completion_natural_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: natural
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let completion = params.completion.clone().unwrap();
        assert_eq!(completion.mode, crate::ast::CompletionMode::Natural);
    }

    #[test]
    fn test_parse_completion_pattern_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: pattern
  patterns:
    - value: "DONE"
      type: contains
    - value: "^COMPLETE:"
      type: regex
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let completion = params.completion.clone().unwrap();
        assert_eq!(completion.mode, crate::ast::CompletionMode::Pattern);
        assert_eq!(completion.patterns.len(), 2);
    }

    #[test]
    fn test_effective_completion_uses_completion_field() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: explicit
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

        let effective = params.effective_completion().unwrap();
        assert_eq!(effective.mode, crate::ast::CompletionMode::Explicit);
    }

    #[test]
    fn test_effective_completion_returns_none_when_empty() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };

        assert!(params.effective_completion().is_none());
    }

    #[test]
    fn test_completion_system_instruction_explicit_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: explicit
  signal:
    fields:
      required: [result, confidence]
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let instruction = params.completion_system_instruction();

        // Should contain instruction about nika:complete tool
        assert!(instruction.contains("nika:complete"));
        assert!(instruction.contains("result"));
        assert!(instruction.contains("confidence"));
    }

    #[test]
    fn test_completion_system_instruction_natural_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: natural
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let instruction = params.completion_system_instruction();

        // Natural mode should have empty instruction
        assert!(instruction.is_empty());
    }

    #[test]
    fn test_completion_system_instruction_pattern_mode() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: pattern
  patterns:
    - value: "TASK_DONE"
      type: exact
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let instruction = params.completion_system_instruction();

        // Pattern mode should have instruction about the pattern
        assert!(instruction.contains("TASK_DONE"));
    }

    #[test]
    fn test_completion_system_instruction_empty_when_none() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };

        let instruction = params.completion_system_instruction();
        assert!(instruction.is_empty());
    }

    #[test]
    fn test_validate_completion_config_valid() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: pattern
  patterns:
    - value: "^DONE$"
      type: regex
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_validate_completion_config_invalid_regex() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: pattern
  patterns:
    - value: "[invalid"
      type: regex
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let err = params.validate().unwrap_err();
        assert!(err.to_string().contains("Invalid regex pattern"));
    }

    #[test]
    fn test_completion_with_confidence_config() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: explicit
  confidence:
    threshold: 0.8
    on_low:
      action: escalate
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let completion = params.completion.clone().unwrap();

        let confidence = completion.confidence.clone().unwrap();
        assert!((confidence.threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(
            confidence.on_low.action,
            crate::ast::LowConfidenceAction::Escalate
        );
    }

    #[test]
    fn test_completion_with_instruction_config() {
        let yaml = r#"
prompt: "Test"
completion:
  mode: explicit
  instruction:
    tone: detailed
    lang: fr
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let completion = params.completion.clone().unwrap();

        let instruction_config = completion.instruction.clone().unwrap();
        assert_eq!(
            instruction_config.tone,
            crate::ast::completion::InstructionTone::Detailed
        );
        assert_eq!(instruction_config.lang, Some("fr".to_string()));
    }

    #[test]
    fn test_full_completion_config_yaml() {
        let yaml = r#"
prompt: "Generate content for QR Code AI"
provider: claude
model: claude-sonnet-4-6
mcp:
  - novanet
max_turns: 10
completion:
  mode: explicit
  signal:
    fields:
      required: [result]
      optional: [confidence, reasoning]
  confidence:
    threshold: 0.75
    on_low:
      action: retry
  instruction:
    tone: concise
    lang: en
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

        // Validate entire config
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        // Check completion
        let completion = params.completion.clone().unwrap();
        assert_eq!(completion.mode, crate::ast::CompletionMode::Explicit);

        // Check signal
        let signal = completion.signal.clone().unwrap();
        assert_eq!(signal.fields.required, vec!["result"]);
        assert_eq!(signal.fields.optional, vec!["confidence", "reasoning"]);

        // Check confidence
        let confidence = completion.confidence.clone().unwrap();
        assert!((confidence.threshold - 0.75).abs() < f64::EPSILON);

        // Check instruction
        let instruction = completion.instruction.clone().unwrap();
        assert_eq!(instruction.lang, Some("en".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Limits Config Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_limits_full() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 20
  max_tokens: 50000
  max_cost_usd: 2.00
  max_duration_secs: 300
  on_limit_reached:
    action: complete_partial
    save_progress: true
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.limits.is_some());

        let limits = params.limits.clone().unwrap();
        assert_eq!(limits.max_turns, 20);
        assert_eq!(limits.max_tokens, 50000);
        assert!((limits.max_cost_usd - 2.00).abs() < f64::EPSILON);
        assert_eq!(limits.max_duration_secs, 300);
        assert_eq!(
            limits.on_limit_reached.action,
            crate::ast::limits::LimitAction::CompletePartial
        );
        assert!(limits.on_limit_reached.save_progress);
    }

    #[test]
    fn test_parse_limits_partial() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 10
  max_cost_usd: 1.50
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let limits = params.limits.clone().unwrap();
        assert_eq!(limits.max_turns, 10);
        assert!((limits.max_cost_usd - 1.50).abs() < f64::EPSILON);
        assert_eq!(limits.max_tokens, 0); // default
        assert_eq!(limits.max_duration_secs, 0); // default
    }

    #[test]
    fn test_parse_limits_action_fail() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 5
  on_limit_reached:
    action: fail
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let limits = params.limits.clone().unwrap();
        assert_eq!(
            limits.on_limit_reached.action,
            crate::ast::limits::LimitAction::Fail
        );
    }

    #[test]
    fn test_parse_limits_action_escalate() {
        let yaml = r#"
prompt: "Test"
limits:
  max_cost_usd: 0.50
  on_limit_reached:
    action: escalate
    message: "Budget exceeded, needs approval"
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let limits = params.limits.clone().unwrap();
        assert_eq!(
            limits.on_limit_reached.action,
            crate::ast::limits::LimitAction::Escalate
        );
        assert_eq!(
            limits.on_limit_reached.message,
            Some("Budget exceeded, needs approval".to_string())
        );
    }

    #[test]
    fn test_limits_defaults_to_none() {
        let params = AgentParams::default();
        assert!(params.limits.is_none());
        assert!(!params.has_limits());
    }

    #[test]
    fn test_effective_limits_default() {
        let params = AgentParams {
            prompt: "test".to_string(),
            ..Default::default()
        };
        let limits = params.effective_limits();
        assert_eq!(limits.max_turns, 0);
        assert_eq!(limits.max_tokens, 0);
        assert!((limits.max_cost_usd - 0.0).abs() < f64::EPSILON);
        assert!(!limits.has_limits());
    }

    #[test]
    fn test_has_limits_true_when_configured() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 10
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(params.has_limits());
    }

    #[test]
    fn test_has_limits_false_when_all_zero() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 0
  max_tokens: 0
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert!(!params.has_limits());
    }

    #[test]
    fn test_validate_limits_negative_cost() {
        let yaml = r#"
prompt: "Test"
limits:
  max_cost_usd: -1.00
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let err = params.validate().unwrap_err();
        assert!(err.to_string().contains("max_cost_usd"));
        assert!(err.to_string().contains("non-negative"));
    }

    #[test]
    fn test_validate_limits_valid() {
        let yaml = r#"
prompt: "Test"
limits:
  max_turns: 20
  max_tokens: 50000
  max_cost_usd: 5.00
  max_duration_secs: 600
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_full_agent_config_with_limits() {
        let yaml = r#"
prompt: "Generate comprehensive research report"
provider: claude
model: claude-sonnet-4-6
mcp:
  - novanet
  - perplexity
max_turns: 20
completion:
  mode: explicit
  confidence:
    threshold: 0.8
guardrails:
  - type: length
    min_words: 500
limits:
  max_turns: 15
  max_tokens: 100000
  max_cost_usd: 3.00
  max_duration_secs: 600
  on_limit_reached:
    action: complete_partial
    save_progress: true
    message: "Research incomplete due to limits"
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(params.has_limits());

        let limits = params.effective_limits();
        assert_eq!(limits.max_turns, 15);
        assert_eq!(limits.max_tokens, 100000);
        assert!((limits.max_cost_usd - 3.00).abs() < f64::EPSILON);
    }
}
