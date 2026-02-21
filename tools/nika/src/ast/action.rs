//! Task Action Types - the 5 action verbs (v0.2)
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request
//! - `InvokeParams`: MCP tool call / resource read (v0.2)
//! - `AgentParams`: Agentic execution with tool calling (v0.2)
//!
//! ## Shorthand Syntax (v0.5.1)
//!
//! `infer:` and `exec:` support shorthand string syntax:
//! ```yaml
//! # Shorthand
//! infer: "Generate a headline"
//! exec: "echo hello"
//!
//! # Full form (equivalent)
//! infer:
//!   prompt: "Generate a headline"
//! exec:
//!   command: "echo hello"
//! ```

use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer};

use crate::ast::{AgentParams, InvokeParams};

/// Infer action - one-shot LLM call
///
/// Supports shorthand: `infer: "prompt"` or full form `infer: { prompt: "..." }`
#[derive(Debug, Clone)]
pub struct InferParams {
    pub prompt: String,
    /// Override provider for this task
    pub provider: Option<String>,
    /// Override model for this task
    pub model: Option<String>,
}

impl<'de> Deserialize<'de> for InferParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum InferParamsHelper {
            Short(String),
            Full {
                prompt: String,
                #[serde(default)]
                provider: Option<String>,
                #[serde(default)]
                model: Option<String>,
            },
        }

        match InferParamsHelper::deserialize(deserializer)? {
            InferParamsHelper::Short(prompt) => Ok(InferParams {
                prompt,
                provider: None,
                model: None,
            }),
            InferParamsHelper::Full {
                prompt,
                provider,
                model,
            } => Ok(InferParams {
                prompt,
                provider,
                model,
            }),
        }
    }
}

/// Exec action - shell command
///
/// Supports shorthand: `exec: "command"` or full form `exec: { command: "..." }`
#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: String,
}

impl<'de> Deserialize<'de> for ExecParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ExecParamsHelper {
            Short(String),
            Full { command: String },
        }

        match ExecParamsHelper::deserialize(deserializer)? {
            ExecParamsHelper::Short(command) => Ok(ExecParams { command }),
            ExecParamsHelper::Full { command } => Ok(ExecParams { command }),
        }
    }
}

/// Fetch action - HTTP request
#[derive(Debug, Clone, Deserialize)]
pub struct FetchParams {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: FxHashMap<String, String>,
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// The 5 task action types (v0.2)
///
/// Each variant corresponds to a YAML verb:
/// - `infer:` - LLM inference (one-shot)
/// - `exec:` - Shell command execution
/// - `fetch:` - HTTP request
/// - `invoke:` - MCP tool call or resource read
/// - `agent:` - Agentic execution with tool calling loop
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
    Invoke { invoke: InvokeParams },
    Agent { agent: AgentParams },
}

impl TaskAction {
    /// Get the verb name for this action (infer, exec, fetch, invoke, agent)
    pub fn verb_name(&self) -> &'static str {
        match self {
            TaskAction::Infer { .. } => "infer",
            TaskAction::Exec { .. } => "exec",
            TaskAction::Fetch { .. } => "fetch",
            TaskAction::Invoke { .. } => "invoke",
            TaskAction::Agent { .. } => "agent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // =========================================================================
    // InferParams Tests
    // =========================================================================

    #[test]
    fn test_infer_params_shorthand_deserialize() {
        let yaml = r#"
infer: "Generate a headline for QR Code AI"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Generate a headline for QR Code AI");
                assert!(infer.provider.is_none());
                assert!(infer.model.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_full_form_deserialize() {
        let yaml = r#"
infer:
  prompt: "Generate a headline"
  provider: claude
  model: claude-sonnet-4-20250514
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Generate a headline");
                assert_eq!(infer.provider, Some("claude".to_string()));
                assert_eq!(infer.model, Some("claude-sonnet-4-20250514".to_string()));
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_full_form_only_prompt() {
        let yaml = r#"
infer:
  prompt: "Generate a headline"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Generate a headline");
                assert!(infer.provider.is_none());
                assert!(infer.model.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_multiline_prompt_shorthand() {
        let yaml = r#"
infer: |
  Generate a comprehensive headline for QR Code AI.
  Include value proposition and key benefit.
  Keep under 100 characters.
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert!(infer.prompt.contains("Generate a comprehensive headline"));
                assert!(infer.prompt.contains("value proposition"));
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_with_provider_only() {
        let yaml = r#"
infer:
  prompt: "Test"
  provider: openai
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.provider, Some("openai".to_string()));
                assert!(infer.model.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_with_model_only() {
        let yaml = r#"
infer:
  prompt: "Test"
  model: gpt-4
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert!(infer.provider.is_none());
                assert_eq!(infer.model, Some("gpt-4".to_string()));
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    // =========================================================================
    // ExecParams Tests
    // =========================================================================

    #[test]
    fn test_exec_params_shorthand_deserialize() {
        let yaml = r#"
exec: "npm run build"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert_eq!(exec.command, "npm run build");
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_exec_params_full_form_deserialize() {
        let yaml = r#"
exec:
  command: "npm run build"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert_eq!(exec.command, "npm run build");
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_exec_params_complex_command() {
        let yaml = r#"
exec: "cargo test --lib -- --test-threads=1 --nocapture"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert!(exec.command.contains("cargo test"));
                assert!(exec.command.contains("--test-threads=1"));
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_exec_params_with_pipes_and_redirects() {
        let yaml = r#"
exec: "cat file.txt | grep pattern > output.txt"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert!(exec.command.contains("grep pattern"));
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    // =========================================================================
    // FetchParams Tests
    // =========================================================================

    #[test]
    fn test_fetch_params_minimal() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://api.example.com/data");
                assert_eq!(fetch.method, "GET");
                assert!(fetch.headers.is_empty());
                assert!(fetch.body.is_none());
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_with_method() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
  method: "POST"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.method, "POST");
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_default_method_get() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.method, "GET");
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_with_headers() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
  method: "GET"
  headers:
    Authorization: "Bearer token123"
    Content-Type: "application/json"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.headers.len(), 2);
                assert_eq!(
                    fetch.headers.get("Authorization"),
                    Some(&"Bearer token123".to_string())
                );
                assert_eq!(
                    fetch.headers.get("Content-Type"),
                    Some(&"application/json".to_string())
                );
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_with_body() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
  method: "POST"
  body: '{"key": "value"}'
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.body, Some(r#"{"key": "value"}"#.to_string()));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_complete() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/users"
  method: "POST"
  headers:
    Authorization: "Bearer token"
    Content-Type: "application/json"
  body: '{"name": "Alice"}'
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://api.example.com/users");
                assert_eq!(fetch.method, "POST");
                assert_eq!(fetch.headers.len(), 2);
                assert_eq!(fetch.body, Some(r#"{"name": "Alice"}"#.to_string()));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    // =========================================================================
    // InvokeParams Tests
    // =========================================================================

    #[test]
    fn test_invoke_params_tool_call() {
        let yaml = r#"
invoke:
  mcp: novanet
  tool: novanet_generate
  params:
    entity: qr-code
    locale: fr-FR
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp, "novanet");
                assert_eq!(invoke.tool, Some("novanet_generate".to_string()));
                assert_eq!(
                    invoke.params,
                    Some(json!({"entity": "qr-code", "locale": "fr-FR"}))
                );
                assert!(invoke.resource.is_none());
            }
            _ => panic!("Expected TaskAction::Invoke"),
        }
    }

    #[test]
    fn test_invoke_params_resource_read() {
        let yaml = r#"
invoke:
  mcp: novanet
  resource: entity://qr-code/fr-FR
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp, "novanet");
                assert!(invoke.tool.is_none());
                assert_eq!(invoke.resource, Some("entity://qr-code/fr-FR".to_string()));
                assert!(invoke.params.is_none());
            }
            _ => panic!("Expected TaskAction::Invoke"),
        }
    }

    #[test]
    fn test_invoke_params_tool_without_params() {
        let yaml = r#"
invoke:
  mcp: test_server
  tool: simple_tool
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp, "test_server");
                assert_eq!(invoke.tool, Some("simple_tool".to_string()));
                assert!(invoke.params.is_none());
            }
            _ => panic!("Expected TaskAction::Invoke"),
        }
    }

    // =========================================================================
    // AgentParams Tests
    // =========================================================================

    #[test]
    fn test_agent_params_minimal() {
        let yaml = r#"
agent:
  prompt: "Generate content for homepage"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.prompt, "Generate content for homepage");
                assert!(agent.system.is_none());
                assert!(agent.provider.is_none());
                assert!(agent.model.is_none());
                assert!(agent.mcp.is_empty());
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_with_mcp() {
        let yaml = r#"
agent:
  prompt: "Generate with MCP tools"
  mcp:
    - novanet
    - perplexity
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.mcp.len(), 2);
                assert!(agent.mcp.contains(&"novanet".to_string()));
                assert!(agent.mcp.contains(&"perplexity".to_string()));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_with_max_turns() {
        let yaml = r#"
agent:
  prompt: "Test prompt"
  max_turns: 5
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.max_turns, Some(5));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_with_stop_conditions() {
        let yaml = r#"
agent:
  prompt: "Test prompt"
  stop_conditions:
    - "GENERATION_COMPLETE"
    - "ERROR"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.stop_conditions.len(), 2);
                assert!(agent
                    .stop_conditions
                    .contains(&"GENERATION_COMPLETE".to_string()));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_with_extended_thinking() {
        let yaml = r#"
agent:
  prompt: "Test prompt"
  extended_thinking: true
  thinking_budget: 8192
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.extended_thinking, Some(true));
                assert_eq!(agent.thinking_budget, Some(8192));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_with_provider_and_model() {
        let yaml = r#"
agent:
  prompt: "Test prompt"
  provider: claude
  model: claude-sonnet-4-20250514
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.provider, Some("claude".to_string()));
                assert_eq!(agent.model, Some("claude-sonnet-4-20250514".to_string()));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_complete() {
        let yaml = r#"
agent:
  prompt: "Generate landing page content"
  system: "You are a web content expert"
  provider: claude
  model: claude-sonnet-4-20250514
  mcp:
    - novanet
  max_turns: 10
  token_budget: 10000
  stop_conditions:
    - "CONTENT_READY"
  scope: full
  extended_thinking: true
  thinking_budget: 4096
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.prompt, "Generate landing page content");
                assert_eq!(
                    agent.system,
                    Some("You are a web content expert".to_string())
                );
                assert_eq!(agent.provider, Some("claude".to_string()));
                assert_eq!(agent.model, Some("claude-sonnet-4-20250514".to_string()));
                assert_eq!(agent.mcp.len(), 1);
                assert_eq!(agent.max_turns, Some(10));
                assert_eq!(agent.token_budget, Some(10000));
                assert_eq!(agent.stop_conditions.len(), 1);
                assert_eq!(agent.scope, Some("full".to_string()));
                assert_eq!(agent.extended_thinking, Some(true));
                assert_eq!(agent.thinking_budget, Some(4096));
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    // =========================================================================
    // TaskAction::verb_name() Tests
    // =========================================================================

    #[test]
    fn test_verb_name_infer() {
        let action = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
            },
        };
        assert_eq!(action.verb_name(), "infer");
    }

    #[test]
    fn test_verb_name_exec() {
        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo test".to_string(),
            },
        };
        assert_eq!(action.verb_name(), "exec");
    }

    #[test]
    fn test_verb_name_fetch() {
        let action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "https://example.com".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
            },
        };
        assert_eq!(action.verb_name(), "fetch");
    }

    #[test]
    fn test_verb_name_invoke() {
        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: "test".to_string(),
                tool: Some("test_tool".to_string()),
                params: None,
                resource: None,
            },
        };
        assert_eq!(action.verb_name(), "invoke");
    }

    #[test]
    fn test_verb_name_agent() {
        let action = TaskAction::Agent {
            agent: AgentParams {
                prompt: "test".to_string(),
                ..Default::default()
            },
        };
        assert_eq!(action.verb_name(), "agent");
    }

    // =========================================================================
    // Mixed Action Type Tests
    // =========================================================================

    #[test]
    fn test_parse_multiple_action_types() {
        let infer_yaml = r#"infer: "test""#;
        let exec_yaml = r#"exec: "test""#;
        let fetch_yaml = r#"fetch: { url: "http://example.com" }"#;

        let infer_action: TaskAction = serde_yaml::from_str(infer_yaml).unwrap();
        let exec_action: TaskAction = serde_yaml::from_str(exec_yaml).unwrap();
        let fetch_action: TaskAction = serde_yaml::from_str(fetch_yaml).unwrap();

        assert_eq!(infer_action.verb_name(), "infer");
        assert_eq!(exec_action.verb_name(), "exec");
        assert_eq!(fetch_action.verb_name(), "fetch");
    }

    // =========================================================================
    // Edge Cases and Error Handling
    // =========================================================================

    #[test]
    fn test_infer_params_empty_prompt() {
        let yaml = r#"
infer: ""
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "");
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_exec_params_empty_command() {
        let yaml = r#"
exec: ""
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert_eq!(exec.command, "");
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_fetch_params_empty_headers() {
        let yaml = r#"
fetch:
  url: "https://example.com"
  headers: {}
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert!(fetch.headers.is_empty());
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_agent_params_empty_mcp_list() {
        let yaml = r#"
agent:
  prompt: "test"
  mcp: []
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert!(agent.mcp.is_empty());
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    #[test]
    fn test_agent_params_empty_stop_conditions() {
        let yaml = r#"
agent:
  prompt: "test"
  stop_conditions: []
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert!(agent.stop_conditions.is_empty());
            }
            _ => panic!("Expected TaskAction::Agent"),
        }
    }

    // =========================================================================
    // Special Characters and Unicode Tests
    // =========================================================================

    #[test]
    fn test_infer_params_special_characters() {
        let yaml = r#"
infer: "Generate content with special chars: !@#$%^&*()"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert!(infer.prompt.contains("!@#$%^&*()"));
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_unicode() {
        let yaml = r#"
infer: "Generate content en français: résumé, café, naïve"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert!(infer.prompt.contains("français"));
                assert!(infer.prompt.contains("résumé"));
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_fetch_params_url_with_query_string() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/search?q=rust&limit=10&offset=5"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert!(fetch.url.contains("search?q=rust"));
                assert!(fetch.url.contains("limit=10"));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    // =========================================================================
    // Cloning Tests
    // =========================================================================

    #[test]
    fn test_infer_action_clone() {
        let action = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: Some("claude".to_string()),
                model: Some("claude-sonnet-4-20250514".to_string()),
            },
        };
        let cloned = action.clone();
        assert_eq!(action.verb_name(), cloned.verb_name());
    }

    #[test]
    fn test_all_action_types_cloneable() {
        let infer = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
            },
        };
        let exec = TaskAction::Exec {
            exec: ExecParams {
                command: "echo".to_string(),
            },
        };
        let fetch = TaskAction::Fetch {
            fetch: FetchParams {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
            },
        };

        let _ = infer.clone();
        let _ = exec.clone();
        let _ = fetch.clone();
    }
}
