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
//!     model: claude-sonnet-4
//!     mcp:
//!       - novanet
//!     max_turns: 10
//!     stop_conditions:
//!       - "GENERATION_COMPLETE"
//! ```

use serde::Deserialize;

/// Default maximum turns for agent loop
const DEFAULT_MAX_TURNS: u32 = 10;

/// Maximum allowed turns to prevent runaway agents
const MAX_ALLOWED_TURNS: u32 = 100;

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

    /// Scope preset (full, minimal, debug)
    #[serde(default)]
    pub scope: Option<String>,
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_params_basic() {
        let yaml = r#"
prompt: "Test prompt"
provider: claude
model: claude-sonnet-4
"#;
        let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.prompt, "Test prompt");
        assert_eq!(params.provider, Some("claude".to_string()));
        assert_eq!(params.model, Some("claude-sonnet-4".to_string()));
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
        assert_eq!(params.system, Some("You are a helpful assistant.".to_string()));
    }

    #[test]
    fn system_prompt_defaults_to_none() {
        let params = AgentParams::default();
        assert!(params.system.is_none());
    }
}
