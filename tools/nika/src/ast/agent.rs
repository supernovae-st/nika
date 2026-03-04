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
//!     stop_conditions:
//!       - "GENERATION_COMPLETE"
//! ```

use serde::Deserialize;

// ═══════════════════════════════════════════════════════════════════════════
// Tool Choice (v0.8.0)
// ═══════════════════════════════════════════════════════════════════════════

/// Tool choice behavior for agent execution (v0.8.0)
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
    pub provider: Option<String>,

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

    /// Conditions that trigger early stop (if output contains any)
    #[serde(default)]
    pub stop_conditions: Vec<String>,

    /// Sequences that stop generation (passed to LLM)
    #[serde(default)]
    pub stop_sequences: Vec<String>,

    /// Scope preset (full, minimal, debug)
    #[serde(default)]
    pub scope: Option<String>,

    /// Enable extended thinking for Claude models (v0.4+)
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

    /// Tool choice behavior (v0.8.0)
    ///
    /// Controls when the agent uses tools:
    /// - `auto` (default): LLM decides when to use tools
    /// - `required`: Must use at least one tool per turn
    /// - `none`: Never use tools (text-only response)
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,

    /// Model temperature for response randomness (v0.8.0)
    ///
    /// Controls the randomness/creativity of responses:
    /// - 0.0: Deterministic, most focused
    /// - 1.0: Balanced creativity (typical default)
    /// - 2.0: Maximum creativity/randomness
    ///
    /// If not set, uses the provider's default.
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate per turn (v0.18.0)
    ///
    /// Required when using `extended_thinking: true` with Claude.
    /// Must be greater than `thinking_budget`. If not set with extended
    /// thinking enabled, defaults to `thinking_budget + 8192`.
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Skills to inject into the agent's system prompt (v0.15.4)
    ///
    /// List of skill aliases that will be loaded and prepended to
    /// the system prompt before each LLM call. Skills are resolved
    /// from the workflow's `skills:` map.
    #[serde(default)]
    pub skills: Option<Vec<String>>,
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

    /// Get effective tool choice behavior (v0.8.0).
    ///
    /// Returns the configured `tool_choice` if set, otherwise returns
    /// `ToolChoice::Auto` (LLM decides when to use tools).
    #[inline]
    pub fn effective_tool_choice(&self) -> ToolChoice {
        self.tool_choice.clone().unwrap_or_default()
    }

    /// Check if tool_choice was explicitly set by the user (v0.8.0).
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

    /// Get effective temperature (v0.8.0).
    ///
    /// Returns the configured `temperature` if set, otherwise returns `None`
    /// to use the provider's default.
    #[inline]
    pub fn effective_temperature(&self) -> Option<f32> {
        self.temperature
    }

    /// Check if a response triggers a stop condition.
    ///
    /// Returns `true` if the content contains any of the configured
    /// stop conditions (case-sensitive substring match).
    pub fn should_stop(&self, content: &str) -> bool {
        self.stop_conditions
            .iter()
            .any(|cond| content.contains(cond))
    }

    /// Validate agent parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `prompt` is empty
    /// - `max_turns` is 0 or exceeds 100
    /// - `token_budget` is 0
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.is_empty() {
            return Err("Agent prompt cannot be empty".to_string());
        }

        if let Some(max) = self.max_turns {
            if max == 0 {
                return Err("max_turns must be > 0".to_string());
            }
            if max > MAX_ALLOWED_TURNS {
                return Err(format!("max_turns cannot exceed {}", MAX_ALLOWED_TURNS));
            }
        }

        if let Some(budget) = self.token_budget {
            if budget == 0 {
                return Err("token_budget must be > 0".to_string());
            }
        }

        // Extended thinking is only supported for Claude
        if self.extended_thinking == Some(true) {
            if let Some(ref provider) = self.provider {
                if provider != "claude" {
                    return Err(format!(
                        "extended_thinking only supported for claude provider, got '{}'",
                        provider
                    ));
                }
            }
        }

        // Validate depth_limit (MVP 8 Phase 2)
        if let Some(depth) = self.depth_limit {
            if depth == 0 {
                return Err("depth_limit must be > 0".to_string());
            }
            if depth > MAX_DEPTH_LIMIT {
                return Err(format!("depth_limit cannot exceed {}", MAX_DEPTH_LIMIT));
            }
        }

        // Validate temperature (v0.8.0)
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(format!(
                    "temperature must be between 0.0 and 2.0, got {}",
                    temp
                ));
            }
        }

        Ok(())
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
        assert_eq!(params.provider, Some("claude".to_string()));
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
    fn should_stop_matches() {
        let params = AgentParams {
            prompt: "test".to_string(),
            stop_conditions: vec!["DONE".to_string()],
            ..Default::default()
        };
        assert!(params.should_stop("Task is DONE"));
        assert!(!params.should_stop("Still working"));
    }

    #[test]
    fn validate_empty_prompt() {
        let params = AgentParams::default();
        assert!(params.validate().is_err());
    }

    #[test]
    fn validate_zero_max_turns() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(0),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn validate_excessive_max_turns() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(101),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn validate_ok() {
        let params = AgentParams {
            prompt: "test".to_string(),
            max_turns: Some(50),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
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
        assert!(params.validate().is_err());
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
    // Extended Thinking Tests (v0.4+)
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
    fn validate_extended_thinking_with_openai_fails() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            provider: Some("openai".to_string()),
            ..Default::default()
        };
        let err = params.validate().unwrap_err();
        assert!(err.contains("extended_thinking only supported for claude"));
    }

    #[test]
    fn validate_extended_thinking_with_claude_ok() {
        let params = AgentParams {
            prompt: "test".to_string(),
            extended_thinking: Some(true),
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
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
        assert!(params.validate().is_ok());
    }

    // ========================================================================
    // Thinking Budget Tests (v0.4+)
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
    // MaxTokens Tests (v0.18.0)
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
    // ToolChoice Tests (v0.8.0)
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
    // Temperature Tests (v0.8.0)
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
        assert!(err.contains("temperature must be between 0.0 and 2.0"));
    }

    #[test]
    fn test_temperature_validation_too_high() {
        let params = AgentParams {
            prompt: "test".to_string(),
            temperature: Some(2.1),
            ..Default::default()
        };
        let err = params.validate().unwrap_err();
        assert!(err.contains("temperature must be between 0.0 and 2.0"));
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
        assert!(params.validate().is_ok());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // has_explicit_tool_choice Tests (v0.8.0 optimization)
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
}
