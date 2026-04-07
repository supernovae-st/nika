//! Analyzed task AST.
//!
//! Tasks with resolved references - TaskId instead of String.
//!

use indexmap::IndexMap;

use super::ids::TaskId;
use crate::ast::artifact::ArtifactSpec;
use crate::ast::decompose::DecomposeSpec;
use crate::ast::logging::LogConfig;
use crate::ast::structured::StructuredOutputSpec;
use crate::ast::templatable::Templatable;
use crate::binding::WithSpec;
use crate::source::Span;

/// An analyzed task - validated and resolved.
///
/// All string references are replaced with interned IDs.
///
#[derive(Debug, Clone)]
pub struct AnalyzedTask {
    /// Task ID (interned)
    pub id: TaskId,

    /// Task name (for display/debugging)
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// The action this task performs
    pub action: AnalyzedTaskAction,

    /// Task-specific provider override (typed enum)
    pub provider: Option<crate::ProviderName>,

    /// Task-specific model override
    pub model: Option<String>,

    /// Agent preset reference from the workflow's `agents:` block
    pub preset: Option<String>,

    /// Parsed `with:` bindings (alias → WithEntry with source, transforms, defaults)
    ///
    /// Phase 2 parses raw `Spanned<String>` values via `parse_with_entry()`.
    /// Each entry has a `BindingPath` source, optional transforms, defaults, and type.
    pub with_spec: WithSpec,

    /// Explicit ordering dependencies: `depends_on: [task_id1, task_id2]`
    ///
    /// These are pure ordering edges — no data flows through them.
    /// Resolved from raw string task names to interned `TaskId`.
    pub depends_on: Vec<TaskId>,

    /// Implicit dependencies auto-extracted from `with:` bindings.
    ///
    /// When a `WithEntry` source references a task (e.g., `step1.data`),
    /// the analyzer extracts `step1` as an implicit dependency.
    /// These are used by the DAG builder alongside `depends_on`.
    pub implicit_deps: Vec<TaskId>,

    /// Output configuration
    pub output: Option<AnalyzedOutput>,

    /// For-each iteration configuration
    pub for_each: Option<AnalyzedForEach>,

    /// Retry configuration
    pub retry: Option<AnalyzedRetry>,

    /// On-error fallback configuration (fires after all retries exhausted)
    pub on_error: Option<AnalyzedOnError>,

    /// Decompose modifier for runtime DAG expansion
    pub decompose: Option<DecomposeSpec>,

    /// Standalone concurrency (used with decompose when no for_each)
    pub concurrency: Option<Templatable<u32>>,

    /// Standalone fail_fast (used with decompose when no for_each)
    pub fail_fast: Option<Templatable<bool>>,

    /// Artifact configuration for file persistence
    pub artifact: Option<ArtifactSpec>,

    /// Per-task log configuration
    pub log: Option<LogConfig>,

    /// Structured output specification (JSON schema enforcement)
    pub structured: Option<StructuredOutputSpec>,

    /// Record compression configuration
    pub record: Option<crate::ast::record::RecordSpec>,

    /// Context budget in tokens — limits total binding size passed to LLM
    pub context_budget: Option<Templatable<u32>>,

    /// Task-level routing override.
    pub routing: Option<crate::ast::routing::RoutingConfig>,

    /// Conditional execution expression.
    /// Template expression evaluated at runtime. If falsy, task is skipped.
    pub when: Option<String>,

    /// Nika Shield: trust elevation flag.
    /// When true, spotlighting and capability restrictions are bypassed for this task.
    /// Only valid value in YAML: `trust: elevated`.
    pub trust_elevated: bool,

    /// Span of the task
    pub span: Span,
}

/// The action a task performs (analyzed).
#[derive(Debug, Clone)]
pub enum AnalyzedTaskAction {
    /// LLM inference
    Infer(AnalyzedInferAction),

    /// Shell command execution
    Exec(AnalyzedExecAction),

    /// HTTP fetch
    Fetch(AnalyzedFetchAction),

    /// MCP tool invocation
    Invoke(AnalyzedInvokeAction),

    /// Autonomous agent
    Agent(Box<AnalyzedAgentAction>),
}

impl Default for AnalyzedTaskAction {
    fn default() -> Self {
        AnalyzedTaskAction::Infer(AnalyzedInferAction::default())
    }
}

impl AnalyzedTaskAction {
    /// Get the verb name.
    pub fn verb_name(&self) -> &'static str {
        match self {
            AnalyzedTaskAction::Infer(_) => "infer",
            AnalyzedTaskAction::Exec(_) => "exec",
            AnalyzedTaskAction::Fetch(_) => "fetch",
            AnalyzedTaskAction::Invoke(_) => "invoke",
            AnalyzedTaskAction::Agent(_) => "agent",
        }
    }
}

/// Analyzed infer action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedInferAction {
    /// The prompt to send to the LLM (may be empty when content is present)
    pub prompt: String,

    /// System prompt override
    pub system: Option<String>,

    /// Temperature (validated: 0.0 - 2.0, or template)
    pub temperature: Option<Templatable<f64>>,

    /// Maximum tokens to generate
    pub max_tokens: Option<Templatable<u32>>,

    /// Enable extended thinking
    pub extended_thinking: Option<Templatable<bool>>,

    /// Thinking budget tokens
    pub thinking_budget: Option<Templatable<u32>>,

    /// Multimodal content parts for vision (analyzed, spans stripped)
    pub content: Option<Vec<crate::ast::content::AnalyzedContentPart>>,

    /// Expected response format: text, json, markdown
    pub response_format: Option<String>,

    /// Guardrails for validating infer output
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed exec action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedExecAction {
    /// Command to execute
    pub command: String,

    /// Run through shell
    pub shell: Templatable<bool>,

    /// Working directory
    pub cwd: Option<String>,

    /// Environment variables
    pub env: IndexMap<String, String>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<Templatable<u64>>,

    /// Maximum stdout size in bytes before truncation. Default: 50 MB.
    pub max_stdout: Option<Templatable<u64>>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed fetch action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedFetchAction {
    /// URL to fetch
    pub url: String,

    /// HTTP method
    pub method: HttpMethod,

    /// HTTP headers
    pub headers: IndexMap<String, String>,

    /// Request body
    pub body: Option<String>,

    /// Request body as JSON
    pub json: Option<serde_json::Value>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<Templatable<u64>>,

    /// Follow redirects
    pub follow_redirects: Templatable<bool>,

    /// Response mode: full or binary
    pub response: Option<crate::ast::extract::ResponseMode>,

    /// Extraction mode for post-processing
    pub extract: Option<crate::ast::extract::ExtractMode>,

    /// CSS selector or JSONPath expression (used with extract)
    pub selector: Option<String>,

    /// Enable cookie jar for session persistence
    pub session: Templatable<bool>,

    /// Enable HTTP response caching
    pub cache: Templatable<bool>,

    /// Span of the action
    pub span: Span,
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    /// Parse an HTTP method string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }

    /// Get the method as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// Analyzed invoke action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedInvokeAction {
    /// MCP server name (None = first available)
    pub server: Option<String>,

    /// Tool name (empty string if resource-only invoke)
    pub tool: String,

    /// MCP resource URI (alternative to tool call)
    pub resource: Option<String>,

    /// Tool parameters
    pub params: Option<serde_json::Value>,

    /// Timeout for tool execution
    pub timeout_ms: Option<Templatable<u64>>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed agent action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedAgentAction {
    /// The prompt for the agent
    pub prompt: String,

    /// Available tools
    pub tools: Vec<String>,

    /// Maximum turns
    pub max_turns: Option<Templatable<u32>>,

    /// Maximum tokens per response
    pub max_tokens: Option<Templatable<u32>>,

    /// Agent definition reference (resolved)
    pub from: Option<String>,

    /// Skills to inject
    pub skills: Vec<String>,

    /// MCP servers for tool access
    pub mcp: Vec<String>,

    /// System prompt (agent persona)
    pub system: Option<String>,

    /// Agent-level provider override (preserved from agent: block)
    pub provider: Option<crate::ProviderName>,

    /// Agent-level model override (preserved from agent: block)
    pub model: Option<String>,

    /// Temperature for LLM sampling
    pub temperature: Option<Templatable<f64>>,

    /// Token budget for the agent
    pub token_budget: Option<Templatable<u32>>,

    /// Enable extended thinking (Claude)
    pub extended_thinking: Option<Templatable<bool>>,

    /// Thinking budget tokens
    pub thinking_budget: Option<Templatable<u32>>,

    /// Max spawn_agent recursion depth
    pub depth_limit: Option<Templatable<u32>>,

    /// Tool choice behavior: auto, required, none
    pub tool_choice: Option<String>,

    /// Sequences that stop generation (passed to LLM)
    pub stop_sequences: Vec<String>,

    /// Scope preset (full, minimal, debug)
    pub scope: Option<String>,
    /// Guardrails for validating agent outputs.
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
    /// Completion behavior configuration.
    pub completion: Option<crate::ast::completion::CompletionConfig>,
    /// Execution limits for cost control.
    pub limits: Option<crate::ast::limits::LimitsConfig>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed output configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedOutput {
    /// Output format
    pub format: OutputFormat,

    /// JSON Schema for validation (validated)
    pub schema: Option<serde_json::Value>,

    /// Schema reference: file path or named ref (from `schema_ref:` / `$ref:`)
    pub schema_ref: Option<String>,

    /// Maximum retries on validation failure
    pub max_retries: Option<Templatable<u32>>,

    /// Span of the output config
    pub span: Span,
}

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Yaml,
}

impl OutputFormat {
    /// Parse an output format string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "yaml" => Some(Self::Yaml),
            _ => None,
        }
    }

    /// Get the format as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }
}

/// Analyzed for-each iteration configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedForEach {
    /// Items expression (binding expression or serialized array)
    pub items: String,

    /// Loop variable name (default: "item")
    pub as_var: String,

    /// Maximum concurrency (None = unlimited)
    pub concurrency: Option<Templatable<u32>>,

    /// Fail fast on first error (default: true)
    pub fail_fast: Templatable<bool>,

    /// Span of the for_each config
    pub span: Span,
}

impl Default for AnalyzedForEach {
    fn default() -> Self {
        Self {
            items: String::new(),
            as_var: "item".to_string(),
            concurrency: None, // None = not specified, inherits from task/workflow level
            fail_fast: Templatable::Value(true),
            span: Span::dummy(),
        }
    }
}

impl AnalyzedForEach {
    /// Check if this is a binding expression.
    pub fn is_binding(&self) -> bool {
        self.items.starts_with("{{") || self.items.starts_with("$")
    }

    /// Check if items is a literal array.
    pub fn is_array(&self) -> bool {
        self.items.starts_with('[')
    }

    /// Parse items as a JSON array if it's a literal.
    pub fn parse_items(&self) -> Option<Vec<serde_json::Value>> {
        if self.is_array() {
            serde_json::from_str(&self.items).ok()
        } else {
            None
        }
    }
}

/// Analyzed retry configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedRetry {
    /// Maximum retry attempts (validated: 1-10)
    pub max_attempts: Templatable<u32>,

    /// Delay between retries in milliseconds (validated: 0-60000)
    pub delay_ms: Templatable<u64>,

    /// Exponential backoff multiplier (validated: 1.0-5.0)
    pub backoff: Option<Templatable<f64>>,

    /// Span of the retry config
    pub span: Span,
}

impl Default for AnalyzedRetry {
    fn default() -> Self {
        Self {
            max_attempts: Templatable::Value(3),
            delay_ms: Templatable::Value(1000),
            backoff: None,
            span: Span::dummy(),
        }
    }
}

/// On-error fallback configuration (analyzed).
#[derive(Debug, Clone)]
pub struct AnalyzedOnError {
    /// The recovery action to take.
    pub action: OnErrorAction,
    /// Span of the on_error: block (for diagnostics).
    pub span: Span,
}

/// What to do when a task fails after retries.
#[derive(Debug, Clone)]
pub enum OnErrorAction {
    /// Swallow the failure: store null as output, mark task succeeded.
    Ignore,
    /// Re-execute with a different provider (single attempt).
    RetryWithProvider { provider: crate::ProviderName },
    /// Execute a different task's action as the recovery path.
    Fallback { task_id: TaskId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_parse() {
        assert_eq!(HttpMethod::parse("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::parse("get"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::parse("POST"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::parse("UNKNOWN"), None);
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!(OutputFormat::parse("text"), Some(OutputFormat::Text));
        assert_eq!(OutputFormat::parse("JSON"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("yaml"), Some(OutputFormat::Yaml));
        assert_eq!(OutputFormat::parse("unknown"), None);
    }

    #[test]
    fn test_analyzed_task_provider_is_typed() {
        use crate::ProviderName;

        let task = AnalyzedTask {
            id: TaskId(0),
            name: "test".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: Some(ProviderName::OpenAI),
            model: Some("gpt-4o".to_string()),

            preset: None,
            with_spec: crate::binding::WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        };

        // Provider should be typed, enabling compile-time guarantees
        assert_eq!(task.provider, Some(ProviderName::OpenAI));
        assert!(task.provider.as_ref().unwrap().requires_api_key());
        assert!(task.provider.as_ref().unwrap().supports_vision());
    }

    #[test]
    fn test_analyzed_task_provider_native_no_vision() {
        use crate::ProviderName;

        // Native provider should not support vision
        let provider = ProviderName::Native;
        assert!(!provider.supports_vision());
        assert!(!provider.requires_api_key());
    }

    #[test]
    fn test_analyzed_task_action_verb() {
        let infer = AnalyzedTaskAction::Infer(AnalyzedInferAction::default());
        assert_eq!(infer.verb_name(), "infer");

        let exec = AnalyzedTaskAction::Exec(AnalyzedExecAction::default());
        assert_eq!(exec.verb_name(), "exec");
    }

    #[test]
    fn test_analyzed_task_with_spec() {
        use crate::binding::types::{BindingPath, BindingSource, PathSegment};
        use crate::binding::{WithEntry, WithSpec};

        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "data".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task("step1".into()),
                segments: vec![PathSegment::Field("result".into())],
            }),
        );

        assert_eq!(with_spec.len(), 1);
        let entry = with_spec.get("data").unwrap();
        assert_eq!(entry.task_id(), Some("step1"));
    }

    #[test]
    fn test_analyzed_for_each_default() {
        let for_each = AnalyzedForEach::default();
        assert_eq!(for_each.as_var, "item");
        assert_eq!(for_each.concurrency, None); // None = unspecified, inherits
        assert_eq!(for_each.fail_fast, Templatable::Value(true));
    }

    #[test]
    fn test_analyzed_for_each_is_binding() {
        let for_each = AnalyzedForEach {
            items: "{{with.items}}".to_string(),
            ..Default::default()
        };
        assert!(for_each.is_binding());

        let for_each = AnalyzedForEach {
            items: "$items".to_string(),
            ..Default::default()
        };
        assert!(for_each.is_binding());

        let for_each = AnalyzedForEach {
            items: r#"["a", "b", "c"]"#.to_string(),
            ..Default::default()
        };
        assert!(!for_each.is_binding());
    }

    #[test]
    fn test_analyzed_for_each_is_array() {
        let for_each = AnalyzedForEach {
            items: r#"["a", "b", "c"]"#.to_string(),
            ..Default::default()
        };
        assert!(for_each.is_array());

        let for_each = AnalyzedForEach {
            items: "{{with.items}}".to_string(),
            ..Default::default()
        };
        assert!(!for_each.is_array());
    }

    #[test]
    fn test_analyzed_for_each_parse_items() {
        let for_each = AnalyzedForEach {
            items: r#"["a", "b", "c"]"#.to_string(),
            ..Default::default()
        };
        let items = for_each.parse_items().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], serde_json::Value::String("a".to_string()));

        let for_each = AnalyzedForEach {
            items: "{{with.items}}".to_string(),
            ..Default::default()
        };
        assert!(for_each.parse_items().is_none());
    }

    #[test]
    fn test_analyzed_retry_default() {
        let retry = AnalyzedRetry::default();
        assert_eq!(retry.max_attempts, Templatable::Value(3));
        assert_eq!(retry.delay_ms, Templatable::Value(1000));
        assert!(retry.backoff.is_none());
    }
}
