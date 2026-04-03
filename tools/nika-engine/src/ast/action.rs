//! Task Action Types - the 5 action verbs
//!
//! Defines the task action variants:
//! - `InferParams`: One-shot LLM call
//! - `ExecParams`: Shell command execution
//! - `FetchParams`: HTTP request
//! - `InvokeParams`: MCP tool call / resource read
//! - `AgentParams`: Agentic execution with tool calling
//!
//! ## Shorthand Syntax
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

use nika_core::ProviderName;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ast::{AgentParams, InvokeParams};
use crate::error::NikaError;

/// Expected response format for infer tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    /// Plain text response (default)
    #[default]
    Text,
    /// JSON response
    Json,
    /// Markdown response
    Markdown,
}

/// Infer action - one-shot LLM call
///
/// Supports shorthand: `infer: "prompt"` or full form `infer: { prompt: "..." }`
///
/// ## LLM Control Options
/// - `temperature`: Control randomness (0.0-2.0, default varies by model)
/// - `max_tokens`: Limit output length
/// - `system`: System prompt to prepend
///
/// ## Extended Thinking
/// - `extended_thinking`: Enable Claude's extended thinking for deeper reasoning
/// - `thinking_budget`: Token budget for extended thinking (1024-65536, default 4096)
#[derive(Debug, Clone, Default)]
pub struct InferParams {
    pub prompt: String,
    /// Override provider for this task
    pub provider: Option<ProviderName>,
    /// Override model for this task
    pub model: Option<String>,
    /// Temperature for sampling (0.0 = deterministic, 2.0 = maximum randomness)
    pub temperature: Option<f64>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// System prompt to set context for the LLM
    pub system: Option<String>,
    /// Expected response format: json, text, markdown
    pub response_format: Option<ResponseFormat>,
    /// Enable extended thinking for deeper reasoning (Claude only)
    pub extended_thinking: Option<bool>,
    /// Token budget for extended thinking (1024-65536, default 4096, Claude only)
    pub thinking_budget: Option<u64>,
    /// Multimodal content parts for vision models (text + images)
    pub content: Option<Vec<crate::ast::content::ContentPart>>,
    /// Guardrails for validating infer output
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
    /// Base URL for OpenAI-compatible endpoint override.
    /// Resolved precedence: task base_url > workflow base_url > config endpoint > env var.
    pub base_url: Option<String>,
    /// Provider fallback chain: try providers in order until one succeeds.
    /// Set from `provider: [groq, anthropic]` or `routing.fallback`.
    pub provider_chain: Option<Vec<ProviderName>>,
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
                #[serde(default)]
                prompt: String,
                #[serde(default)]
                provider: Option<String>,
                #[serde(default)]
                model: Option<String>,
                #[serde(default)]
                temperature: Option<f64>,
                #[serde(default)]
                max_tokens: Option<u32>,
                #[serde(default)]
                system: Option<String>,
                #[serde(default)]
                response_format: Option<ResponseFormat>,
                #[serde(default)]
                extended_thinking: Option<bool>,
                #[serde(default)]
                thinking_budget: Option<u64>,
                #[serde(default)]
                content: Option<Vec<crate::ast::content::ContentPart>>,
                #[serde(default)]
                guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
            },
        }

        match InferParamsHelper::deserialize(deserializer)? {
            InferParamsHelper::Short(prompt) => Ok(InferParams {
                prompt,
                provider: None,
                model: None,
                temperature: None,
                max_tokens: None,
                system: None,
                response_format: None,
                extended_thinking: None,
                thinking_budget: None,
                content: None,
                guardrails: Vec::new(),
                base_url: None,
                provider_chain: None,
            }),
            InferParamsHelper::Full {
                prompt,
                provider,
                model,
                temperature,
                max_tokens,
                system,
                response_format,
                extended_thinking,
                thinking_budget,
                content,
                guardrails,
            } => Ok(InferParams {
                prompt,
                provider: provider.map(ProviderName::from),
                model,
                temperature,
                max_tokens,
                system,
                response_format,
                extended_thinking,
                thinking_budget,
                content,
                guardrails,
                base_url: None,
                provider_chain: None,
            }),
        }
    }
}

impl InferParams {
    /// Validate infer parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `prompt` is empty or whitespace-only
    /// - `temperature` is outside valid range (0.0..=2.0)
    /// - `extended_thinking` is true with non-Claude provider
    /// - `thinking_budget` is outside valid range (1024..=65536)
    ///
    pub fn validate(&self) -> Result<(), NikaError> {
        // Prompt can be empty when content is present (vision mode)
        let has_content = self.content.as_ref().is_some_and(|c| !c.is_empty());
        if self.prompt.trim().is_empty() && !has_content {
            return Err(NikaError::ValidationError {
                reason: "Infer requires 'prompt' or 'content' (neither provided)".into(),
            });
        }

        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(NikaError::ValidationError {
                    reason: format!("temperature must be between 0.0 and 2.0, got {}", temp),
                });
            }
        }

        // thinking_budget validation
        if let Some(budget) = self.thinking_budget {
            if !(1024..=65536).contains(&budget) {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "thinking_budget must be between 1024 and 65536, got {}",
                        budget
                    ),
                });
            }
        }

        Ok(())
    }

    /// Get effective thinking budget.
    ///
    /// Returns the configured `thinking_budget` if set, otherwise returns
    /// the default value (4096).
    pub fn effective_thinking_budget(&self) -> u64 {
        self.thinking_budget.unwrap_or(4096)
    }
}

/// Exec action - shell command
///
/// Supports shorthand: `exec: "command"` or full form `exec: { command: "...", shell: true }`
///
/// ## Shell Field
/// - `shell: None` (default) → Shell-free execution via shlex parsing
/// - `shell: Some(true)` → Execute via shell (opt-in for pipes, env vars)
/// - `shell: Some(false)` → Explicitly shell-free
#[derive(Debug, Clone)]
pub struct ExecParams {
    pub command: String,
    /// Whether to execute via shell. None means shell-free (default, secure).
    pub shell: Option<bool>,
    /// Timeout in seconds for command execution
    pub timeout: Option<u64>,
    /// Working directory for command execution
    pub cwd: Option<String>,
    /// Environment variables to pass to the command
    pub env: Option<FxHashMap<String, String>>,
    /// Maximum stdout size in bytes before truncation. Default: 50 MB.
    pub max_stdout: Option<u64>,
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
            Full {
                command: String,
                #[serde(default)]
                shell: Option<bool>,
                #[serde(default)]
                timeout: Option<u64>,
                #[serde(default)]
                cwd: Option<String>,
                #[serde(default)]
                env: Option<FxHashMap<String, String>>,
                #[serde(default)]
                max_stdout: Option<u64>,
            },
        }

        match ExecParamsHelper::deserialize(deserializer)? {
            ExecParamsHelper::Short(command) => Ok(ExecParams {
                command,
                shell: None,
                timeout: None,
                cwd: None,
                env: None,
                max_stdout: None,
            }),
            ExecParamsHelper::Full {
                command,
                shell,
                timeout,
                cwd,
                env,
                max_stdout,
            } => Ok(ExecParams {
                command,
                shell,
                timeout,
                cwd,
                env,
                max_stdout,
            }),
        }
    }
}

impl ExecParams {
    /// Validate exec parameters
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `command` is empty or whitespace-only
    /// - `timeout` is zero
    pub fn validate(&self) -> Result<(), NikaError> {
        if self.command.trim().is_empty() {
            return Err(NikaError::ValidationError {
                reason: "Exec command cannot be empty".into(),
            });
        }
        if self.timeout == Some(0) {
            return Err(NikaError::ValidationError {
                reason: "Exec timeout must be greater than 0".into(),
            });
        }
        Ok(())
    }
}

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial backoff in milliseconds (default: 1000)
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,

    /// Backoff multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

impl RetryConfig {
    /// Validate retry configuration.
    pub fn validate(&self) -> Result<(), NikaError> {
        if self.max_attempts == 0 {
            return Err(NikaError::ValidationError {
                reason: "retry.max_attempts must be at least 1".into(),
            });
        }
        Ok(())
    }
}

fn default_max_attempts() -> u32 {
    3
}

fn default_backoff_ms() -> u64 {
    1000
}

fn default_multiplier() -> f64 {
    2.0
}

/// Fetch action - HTTP request
///
/// ## JSON Body
/// - `json: { ... }` — Auto-serializes to JSON and sets Content-Type: application/json
/// - Mutually exclusive with `body` (json takes precedence)
///
/// ## Redirect Control
/// - `follow_redirects: true` (default) — Follows HTTP redirects up to 5 hops
/// - `follow_redirects: false` — Disables redirect following, returns 3xx response
#[derive(Debug, Clone, Deserialize)]
pub struct FetchParams {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: FxHashMap<String, String>,
    pub body: Option<String>,
    /// JSON body to auto-serialize
    /// If provided, serializes to string and sets Content-Type: application/json
    /// Takes precedence over `body` if both are specified
    #[serde(default)]
    pub json: Option<serde_json::Value>,
    /// Request timeout in seconds (matches JSON schema)
    pub timeout: Option<u64>,
    /// Optional retry configuration for failed requests
    #[serde(default)]
    pub retry: Option<RetryConfig>,
    /// Whether to follow HTTP redirects
    /// Defaults to true if not specified
    #[serde(default)]
    pub follow_redirects: Option<bool>,
    /// Response mode: full (status + headers + body JSON) or binary (CAS store)
    #[serde(default)]
    pub response: Option<nika_core::ast::extract::ResponseMode>,
    /// Extraction mode for post-processing
    #[serde(default)]
    pub extract: Option<nika_core::ast::extract::ExtractMode>,
    /// CSS selector or JSONPath expression (used with extract: selector, text, jsonpath)
    #[serde(default)]
    pub selector: Option<String>,
    /// Enable cookie jar for session persistence
    #[serde(default)]
    pub session: Option<bool>,
    /// Enable HTTP response caching
    #[serde(default)]
    pub cache: Option<bool>,
}

impl FetchParams {
    /// Validate fetch parameters
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - `url` is empty or whitespace-only
    /// - `method` is not a valid HTTP method
    /// - `timeout` is zero
    pub fn validate(&self) -> Result<(), NikaError> {
        if self.url.trim().is_empty() {
            return Err(NikaError::ValidationError {
                reason: "Fetch URL cannot be empty".into(),
            });
        }
        let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
        let method_upper = self.method.to_uppercase();
        if !valid_methods.contains(&method_upper.as_str()) {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Invalid HTTP method '{}', expected one of: {}",
                    self.method,
                    valid_methods.join(", ")
                ),
            });
        }
        if self.timeout == Some(0) {
            return Err(NikaError::ValidationError {
                reason: "Fetch timeout must be greater than 0".into(),
            });
        }
        // extract and response modes are validated at the type level (enums).
        if self.selector.is_some() && self.extract.is_none() {
            return Err(NikaError::ValidationError {
                reason: "fetch 'selector' requires 'extract' to be set".to_string(),
            });
        }
        if let (Some(resp), Some(ext)) = (&self.response, &self.extract) {
            use nika_core::ast::extract::{ExtractMode, ResponseMode};
            // Binary stores raw bytes — extract modes need text. Always incompatible.
            if *resp == ResponseMode::Binary {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "fetch cannot combine 'response: binary' with 'extract: {}' — binary stores raw bytes",
                        ext
                    ),
                });
            }
            // llm_txt makes its own sub-requests — it ignores the fetched body entirely.
            if *ext == ExtractMode::LlmTxt {
                return Err(NikaError::ValidationError {
                    reason: "fetch cannot combine 'response: full' with 'extract: llm_txt' — \
                             llm_txt performs its own requests"
                        .into(),
                });
            }
            // response: full + other extract modes: ALLOWED (ENG-015)
            // Output: { status, headers, url, elapsed_ms, body, extracted }
        }
        Ok(())
    }
}

fn default_method() -> String {
    "GET".to_string()
}

/// The 5 task action types
///
/// Each variant corresponds to a YAML verb:
/// - `infer:` - LLM inference (one-shot)
/// - `exec:` - Shell command execution
/// - `fetch:` - HTTP request
/// - `invoke:` - MCP tool call or resource read
/// - `agent:` - Agentic execution with tool calling loop
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)] // AgentParams is large due to CompletionConfig; boxing would be a breaking change
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
    use crate::serde_yaml;
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
  model: claude-sonnet-4-6
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Generate a headline");
                assert_eq!(infer.provider, Some(ProviderName::Anthropic));
                assert_eq!(infer.model, Some("claude-sonnet-4-6".to_string()));
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
                assert_eq!(infer.provider, Some(ProviderName::OpenAI));
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
    // InferParams Validation Tests
    // =========================================================================

    #[test]
    fn test_infer_params_validate_ok() {
        let params = InferParams {
            prompt: "Generate something".to_string(),
            temperature: Some(0.7),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_infer_params_validate_empty_prompt() {
        let params = InferParams {
            prompt: "".to_string(),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result);
        assert!(result.unwrap_err().to_string().contains("neither provided"));
    }

    #[test]
    fn test_infer_params_validate_whitespace_only_prompt() {
        let params = InferParams {
            prompt: "   \n\t  ".to_string(),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_err(), "Should fail but got: {:?}", result);
        assert!(result.unwrap_err().to_string().contains("neither provided"));
    }

    #[test]
    fn test_infer_params_validate_content_without_prompt_ok() {
        use crate::ast::content::{ContentPart, ImageDetail};
        let params = InferParams {
            prompt: "".to_string(),
            content: Some(vec![ContentPart::Image {
                source: "blake3:abc".to_string(),
                detail: ImageDetail::Auto,
            }]),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_infer_params_validate_content_and_prompt_ok() {
        use crate::ast::content::ContentPart;
        let params = InferParams {
            prompt: "Describe this image".to_string(),
            content: Some(vec![ContentPart::Text {
                text: "hello".to_string(),
            }]),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    }

    #[test]
    fn test_infer_params_validate_empty_content_vec_rejected() {
        let params = InferParams {
            prompt: "".to_string(),
            content: Some(vec![]),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("neither provided"));
    }

    #[test]
    fn test_infer_params_validate_temperature_too_low() {
        let params = InferParams {
            prompt: "Test".to_string(),
            temperature: Some(-0.1),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temperature"));
    }

    #[test]
    fn test_infer_params_validate_temperature_too_high() {
        let params = InferParams {
            prompt: "Test".to_string(),
            temperature: Some(2.5),
            ..Default::default()
        };
        let result = params.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temperature"));
    }

    #[test]
    fn test_infer_params_validate_temperature_boundary_valid() {
        // 0.0 and 2.0 are valid boundary values
        let params_min = InferParams {
            prompt: "Test".to_string(),
            temperature: Some(0.0),
            ..Default::default()
        };
        assert!(params_min.validate().is_ok());

        let params_max = InferParams {
            prompt: "Test".to_string(),
            temperature: Some(2.0),
            ..Default::default()
        };
        assert!(params_max.validate().is_ok());
    }

    // =========================================================================
    // InferParams LLM Control Options Tests
    // =========================================================================

    #[test]
    fn test_infer_params_with_temperature() {
        let yaml = r#"
infer:
  prompt: "Be creative"
  temperature: 0.9
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Be creative");
                assert_eq!(infer.temperature, Some(0.9));
                assert!(infer.max_tokens.is_none());
                assert!(infer.system.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_with_max_tokens() {
        let yaml = r#"
infer:
  prompt: "Short answer"
  max_tokens: 100
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Short answer");
                assert_eq!(infer.max_tokens, Some(100));
                assert!(infer.temperature.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_with_system_prompt() {
        let yaml = r#"
infer:
  prompt: "Explain quantum computing"
  system: "You are a physics professor explaining to undergraduates."
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Explain quantum computing");
                assert_eq!(
                    infer.system,
                    Some("You are a physics professor explaining to undergraduates.".to_string())
                );
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_full_llm_control() {
        let yaml = r#"
infer:
  prompt: "Write a haiku"
  provider: openai
  model: gpt-4o
  temperature: 0.7
  max_tokens: 50
  system: "You are a Japanese poetry master."
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Write a haiku");
                assert_eq!(infer.provider, Some(ProviderName::OpenAI));
                assert_eq!(infer.model, Some("gpt-4o".to_string()));
                assert_eq!(infer.temperature, Some(0.7));
                assert_eq!(infer.max_tokens, Some(50));
                assert_eq!(
                    infer.system,
                    Some("You are a Japanese poetry master.".to_string())
                );
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_shorthand_defaults_llm_options() {
        let yaml = r#"
infer: "Simple prompt"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.prompt, "Simple prompt");
                assert!(infer.temperature.is_none());
                assert!(infer.max_tokens.is_none());
                assert!(infer.system.is_none());
            }
            _ => panic!("Expected TaskAction::Infer"),
        }
    }

    #[test]
    fn test_infer_params_temperature_zero() {
        let yaml = r#"
infer:
  prompt: "Deterministic output"
  temperature: 0.0
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Infer { infer } => {
                assert_eq!(infer.temperature, Some(0.0));
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
    // ExecParams Shell Field Tests
    // =========================================================================

    #[test]
    fn test_exec_params_shell_field_default_none() {
        let yaml = r#"
exec:
  command: "echo hello"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert_eq!(exec.command, "echo hello");
                assert_eq!(exec.shell, None); // None means shell-free (default)
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_exec_params_shell_true_explicit() {
        let yaml = r#"
exec:
  command: "echo $HOME && ls | grep foo"
  shell: true
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert!(exec.command.contains("$HOME"));
                assert_eq!(exec.shell, Some(true));
            }
            _ => panic!("Expected TaskAction::Exec"),
        }
    }

    #[test]
    fn test_exec_params_shell_false_explicit() {
        let yaml = r#"
exec:
  command: "echo hello"
  shell: false
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Exec { exec } => {
                assert_eq!(exec.shell, Some(false));
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

    #[test]
    fn test_fetch_params_with_json() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/users"
  method: "POST"
  json:
    name: "Alice"
    age: 30
    active: true
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://api.example.com/users");
                assert_eq!(fetch.method, "POST");
                assert!(fetch.json.is_some());
                let json = fetch.json.unwrap();
                assert_eq!(json["name"], "Alice");
                assert_eq!(json["age"], 30);
                assert_eq!(json["active"], true);
                assert!(fetch.body.is_none()); // body not set, json is used
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_json_with_nested_objects() {
        let yaml = r#"
fetch:
  url: "https://api.example.com/data"
  method: "POST"
  json:
    user:
      name: "Bob"
      email: "bob@example.com"
    tags:
      - "admin"
      - "active"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                let json = fetch.json.unwrap();
                assert_eq!(json["user"]["name"], "Bob");
                assert_eq!(json["user"]["email"], "bob@example.com");
                assert_eq!(json["tags"][0], "admin");
                assert_eq!(json["tags"][1], "active");
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_follow_redirects_true() {
        let yaml = r#"
fetch:
  url: "https://example.com/redirect"
  follow_redirects: true
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://example.com/redirect");
                assert_eq!(fetch.follow_redirects, Some(true));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_follow_redirects_false() {
        let yaml = r#"
fetch:
  url: "https://example.com/redirect"
  follow_redirects: false
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://example.com/redirect");
                assert_eq!(fetch.follow_redirects, Some(false));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_follow_redirects_default_none() {
        let yaml = r#"
fetch:
  url: "https://example.com/api"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://example.com/api");
                assert!(fetch.follow_redirects.is_none()); // Defaults to None (meaning true)
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
  tool: novanet_context
  params:
    entity: qr-code
    locale: fr-FR
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Invoke { invoke } => {
                assert_eq!(invoke.mcp, Some("novanet".to_string()));
                assert_eq!(invoke.tool, Some("novanet_context".to_string()));
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
                assert_eq!(invoke.mcp, Some("novanet".to_string()));
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
                assert_eq!(invoke.mcp, Some("test_server".to_string()));
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
  model: claude-sonnet-4-6
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Agent { agent } => {
                assert_eq!(agent.provider, Some(ProviderName::Anthropic));
                assert_eq!(agent.model, Some("claude-sonnet-4-6".to_string()));
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
  model: claude-sonnet-4-6
  mcp:
    - novanet
  max_turns: 10
  token_budget: 10000
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
                assert_eq!(agent.provider, Some(ProviderName::Anthropic));
                assert_eq!(agent.model, Some("claude-sonnet-4-6".to_string()));
                assert_eq!(agent.mcp.len(), 1);
                assert_eq!(agent.max_turns, Some(10));
                assert_eq!(agent.token_budget, Some(10000));
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
                ..Default::default()
            },
        };
        assert_eq!(action.verb_name(), "infer");
    }

    #[test]
    fn test_verb_name_exec() {
        let action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo test".to_string(),
                shell: None,
                timeout: None,
                cwd: None,
                env: None,
                max_stdout: None,
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
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: None,
                selector: None,
                session: None,
                cache: None,
            },
        };
        assert_eq!(action.verb_name(), "fetch");
    }

    #[test]
    fn test_verb_name_invoke() {
        let action = TaskAction::Invoke {
            invoke: InvokeParams {
                mcp: Some("test".to_string()),
                tool: Some("test_tool".to_string()),
                params: None,
                resource: None,
                timeout: None,
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
                provider: Some(ProviderName::Anthropic),
                model: Some("claude-sonnet-4-6".to_string()),
                ..Default::default()
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
                ..Default::default()
            },
        };
        let exec = TaskAction::Exec {
            exec: ExecParams {
                command: "echo".to_string(),
                shell: None,
                timeout: None,
                cwd: None,
                env: None,
                max_stdout: None,
            },
        };
        let fetch = TaskAction::Fetch {
            fetch: FetchParams {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: None,
                selector: None,
                session: None,
                cache: None,
            },
        };

        let _ = infer.clone();
        let _ = exec.clone();
        let _ = fetch.clone();
    }

    // =========================================================================
    // FetchParams extract/selector validation
    // =========================================================================

    #[test]
    fn test_fetch_validate_valid_extract_modes() {
        use nika_core::ast::extract::ExtractMode;
        let valid_modes = [
            ExtractMode::Markdown,
            ExtractMode::Article,
            ExtractMode::Text,
            ExtractMode::Selector,
            ExtractMode::Metadata,
            ExtractMode::Links,
            ExtractMode::Jsonpath,
            ExtractMode::Feed,
            ExtractMode::LlmTxt,
        ];
        for mode in &valid_modes {
            let params = FetchParams {
                url: "https://example.com".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(*mode),
                selector: None,
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "extract mode '{}' should be valid",
                mode
            );
        }
    }

    // Invalid extract modes are now prevented at the type level (ExtractMode enum).
    // The analyzer catches invalid strings from YAML and emits a warning.

    #[test]
    fn test_fetch_validate_selector_without_extract() {
        let params = FetchParams {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
            response: None,
            extract: None,
            selector: Some("div.content".to_string()),
            session: None,
            cache: None,
        };
        let err = params.validate().unwrap_err();
        assert!(err.to_string().contains("selector"));
        assert!(err.to_string().contains("requires"));
    }

    #[test]
    fn test_fetch_validate_selector_with_extract() {
        let params = FetchParams {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
            response: None,
            extract: Some(nika_core::ast::extract::ExtractMode::Text),
            selector: Some("p.intro".to_string()),
            session: None,
            cache: None,
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_fetch_validate_no_extract_no_selector() {
        let params = FetchParams {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: FxHashMap::default(),
            body: None,
            json: None,
            timeout: None,
            retry: None,
            follow_redirects: None,
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_fetch_params_extract_deserialize() {
        let yaml = r#"
fetch:
  url: "https://example.com"
  extract: markdown
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(fetch.url, "https://example.com");
                assert_eq!(
                    fetch.extract,
                    Some(nika_core::ast::extract::ExtractMode::Markdown)
                );
                assert!(fetch.selector.is_none());
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_fetch_params_extract_with_selector_deserialize() {
        let yaml = r#"
fetch:
  url: "https://example.com"
  extract: selector
  selector: "div.content"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert_eq!(
                    fetch.extract,
                    Some(nika_core::ast::extract::ExtractMode::Selector)
                );
                assert_eq!(fetch.selector, Some("div.content".to_string()));
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    #[test]
    fn test_infer_extended_thinking_validates_ok() {
        let params = InferParams {
            prompt: "Deep reasoning".to_string(),
            extended_thinking: Some(true),
            thinking_budget: Some(8192),
            ..Default::default()
        };
        // extended_thinking on infer: verb should pass validation (no longer warns/errors)
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_infer_extended_thinking_budget_too_low() {
        let params = InferParams {
            prompt: "Deep reasoning".to_string(),
            extended_thinking: Some(true),
            thinking_budget: Some(512), // below 1024 minimum
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_fetch_params_no_extract_backward_compatible() {
        let yaml = r#"
fetch:
  url: "https://example.com"
"#;
        let action: TaskAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            TaskAction::Fetch { fetch } => {
                assert!(fetch.extract.is_none());
                assert!(fetch.selector.is_none());
            }
            _ => panic!("Expected TaskAction::Fetch"),
        }
    }

    // ═══════════════════════════════════════════
    // RetryConfig validation
    // ═══════════════════════════════════════════

    #[test]
    fn retry_max_attempts_zero_rejected() {
        let config = RetryConfig {
            max_attempts: 0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("max_attempts"),
            "should reject max_attempts=0: {err}"
        );
    }

    #[test]
    fn retry_max_attempts_positive_ok() {
        let config = RetryConfig {
            max_attempts: 1,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }
}
