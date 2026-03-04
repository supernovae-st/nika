//! Analyzed task AST.
//!
//! Tasks with resolved references - TaskId instead of String.

use indexmap::IndexMap;

use crate::source::Span;
use super::ids::TaskId;

/// An analyzed task - validated and resolved.
///
/// All string references are replaced with interned IDs.
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

    /// Task-specific provider override
    pub provider: Option<String>,

    /// Task-specific model override
    pub model: Option<String>,

    /// Resolved use: references (alias → TaskId)
    pub use_refs: IndexMap<String, AnalyzedUseRef>,

    /// Resolved flow: dependencies (TaskIds)
    pub flow_deps: Vec<TaskId>,

    /// Output configuration
    pub output: Option<AnalyzedOutput>,

    /// Span of the task
    pub span: Span,
}

/// Resolved use reference.
#[derive(Debug, Clone)]
pub struct AnalyzedUseRef {
    /// Alias name
    pub alias: String,

    /// Target task ID (resolved)
    pub target: TaskId,

    /// Optional JSONPath for extracting specific data
    pub path: Option<String>,

    /// Span of the reference
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
    Agent(AnalyzedAgentAction),
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
    /// The prompt to send to the LLM
    pub prompt: String,

    /// System prompt override
    pub system: Option<String>,

    /// Temperature (validated: 0.0 - 2.0)
    pub temperature: Option<f64>,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// Stop sequences
    pub stop: Vec<String>,

    /// Enable extended thinking
    pub thinking: Option<bool>,

    /// Thinking budget tokens
    pub thinking_budget: Option<u32>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed exec action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedExecAction {
    /// Command to execute
    pub command: String,

    /// Run through shell
    pub shell: bool,

    /// Working directory
    pub working_dir: Option<String>,

    /// Environment variables
    pub env: IndexMap<String, String>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,

    /// Capture stdout
    pub capture_stdout: bool,

    /// Capture stderr
    pub capture_stderr: bool,

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
    pub timeout_ms: Option<u64>,

    /// Follow redirects
    pub follow_redirects: bool,

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

    /// Tool name
    pub tool: String,

    /// Tool parameters (validated against schema in v0.20)
    pub params: Option<serde_json::Value>,

    /// Timeout for tool execution
    pub timeout_ms: Option<u64>,

    /// Span of the action
    pub span: Span,
}

/// Analyzed agent action.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedAgentAction {
    /// The goal for the agent
    pub goal: String,

    /// Available tools
    pub tools: Vec<String>,

    /// Maximum iterations
    pub max_iterations: Option<u32>,

    /// Maximum tokens per response
    pub max_tokens: Option<u32>,

    /// Agent definition reference (resolved)
    pub from: Option<String>,

    /// Skills to inject
    pub skills: Vec<String>,

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

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
    fn test_analyzed_task_action_verb() {
        let infer = AnalyzedTaskAction::Infer(AnalyzedInferAction::default());
        assert_eq!(infer.verb_name(), "infer");

        let exec = AnalyzedTaskAction::Exec(AnalyzedExecAction::default());
        assert_eq!(exec.verb_name(), "exec");
    }

    #[test]
    fn test_analyzed_use_ref() {
        let use_ref = AnalyzedUseRef {
            alias: "data".to_string(),
            target: TaskId::new(1),
            path: Some("$.result".to_string()),
            span: make_span(0, 10),
        };

        assert_eq!(use_ref.alias, "data");
        assert_eq!(use_ref.target.index(), 1);
        assert_eq!(use_ref.path.as_deref(), Some("$.result"));
    }
}
