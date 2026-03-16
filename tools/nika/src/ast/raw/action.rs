//! Raw task action variants (verbs).
//!
//! Each verb (infer, exec, fetch, invoke, agent) has its own params struct
//! with full span tracking for error reporting.

use indexmap::IndexMap;

use crate::source::{Span, Spanned};

/// The action a task performs - one of the 5 verbs.
#[derive(Debug, Clone)]
pub enum RawTaskAction {
    /// LLM inference: infer: { prompt: "..." }
    Infer(Spanned<RawInferAction>),

    /// Shell command: exec: { command: "..." }
    Exec(Spanned<RawExecAction>),

    /// HTTP fetch: fetch: { url: "..." }
    Fetch(Spanned<RawFetchAction>),

    /// MCP tool invocation: invoke: tool_name or invoke: { tool: name, params: {...} }
    Invoke(Spanned<RawInvokeAction>),

    /// Autonomous agent: agent: { prompt: "..." }
    Agent(Spanned<RawAgentAction>),
}

impl Default for RawTaskAction {
    fn default() -> Self {
        RawTaskAction::Infer(Spanned::dummy(RawInferAction::default()))
    }
}

impl RawTaskAction {
    /// Get the verb name as a string.
    pub fn verb_name(&self) -> &'static str {
        match self {
            RawTaskAction::Infer(_) => "infer",
            RawTaskAction::Exec(_) => "exec",
            RawTaskAction::Fetch(_) => "fetch",
            RawTaskAction::Invoke(_) => "invoke",
            RawTaskAction::Agent(_) => "agent",
        }
    }

    /// Get the span of the action.
    pub fn span(&self) -> Span {
        match self {
            RawTaskAction::Infer(a) => a.span,
            RawTaskAction::Exec(a) => a.span,
            RawTaskAction::Fetch(a) => a.span,
            RawTaskAction::Invoke(a) => a.span,
            RawTaskAction::Agent(a) => a.span,
        }
    }
}

/// Parameters for the `infer` verb (LLM inference).
#[derive(Debug, Clone, Default)]
pub struct RawInferAction {
    /// The prompt to send to the LLM
    pub prompt: Spanned<String>,

    /// System prompt override
    pub system: Option<Spanned<String>>,

    /// Temperature (0.0 - 2.0)
    pub temperature: Option<Spanned<f64>>,

    /// Maximum tokens to generate
    pub max_tokens: Option<Spanned<u32>>,

    /// Stop sequences
    pub stop: Option<Spanned<Vec<Spanned<String>>>>,

    /// Enable extended thinking (Claude)
    pub thinking: Option<Spanned<bool>>,

    /// Thinking budget tokens
    pub thinking_budget: Option<Spanned<u32>>,
}

/// Parameters for the `exec` verb (shell command execution).
#[derive(Debug, Clone, Default)]
pub struct RawExecAction {
    /// Command to execute (string or array)
    pub command: Spanned<String>,

    /// Run through shell (sh -c) - defaults to false for security
    pub shell: Option<Spanned<bool>>,

    /// Working directory
    pub working_dir: Option<Spanned<String>>,

    /// Environment variables
    pub env: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<Spanned<u64>>,

    /// Capture stdout
    pub capture_stdout: Option<Spanned<bool>>,

    /// Capture stderr
    pub capture_stderr: Option<Spanned<bool>>,
}

/// Parameters for the `fetch` verb (HTTP requests).
#[derive(Debug, Clone, Default)]
pub struct RawFetchAction {
    /// URL to fetch
    pub url: Spanned<String>,

    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: Option<Spanned<String>>,

    /// HTTP headers
    pub headers: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Request body (for POST/PUT)
    pub body: Option<Spanned<String>>,

    /// Request body as JSON
    pub json: Option<Spanned<serde_json::Value>>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<Spanned<u64>>,

    /// Follow redirects
    pub follow_redirects: Option<Spanned<bool>>,
}

/// Parameters for the `invoke` verb (MCP tool invocation).
#[derive(Debug, Clone, Default)]
pub struct RawInvokeAction {
    /// MCP tool name: "tool_name" or "server::tool_name"
    pub tool: Spanned<String>,

    /// Tool parameters (validated against MCP schema)
    pub params: Option<Spanned<serde_json::Value>>,

    /// Optional MCP server to use (if not in tool name)
    pub mcp: Option<Spanned<String>>,

    /// Timeout for tool execution
    pub timeout_ms: Option<Spanned<u64>>,
}

impl RawInvokeAction {
    /// Parse server and tool name from the tool field.
    /// Returns (server, tool_name) where server may be None.
    pub fn parse_tool_name(&self) -> (Option<&str>, &str) {
        let tool = &self.tool.value;
        if let Some((server, name)) = tool.split_once("::") {
            (Some(server), name)
        } else {
            (None, tool.as_str())
        }
    }
}

/// Parameters for the `agent` verb (autonomous agent execution).
#[derive(Debug, Clone, Default)]
pub struct RawAgentAction {
    /// The prompt for the agent to execute
    pub prompt: Spanned<String>,

    /// Available tools for the agent
    pub tools: Option<Spanned<Vec<Spanned<String>>>>,

    /// Maximum iterations before stopping
    pub max_iterations: Option<Spanned<u32>>,

    /// Maximum tokens per response
    pub max_tokens: Option<Spanned<u32>>,

    /// Agent definition reference (agents: section)
    pub from: Option<Spanned<String>>,

    /// Skills to inject into system prompt
    pub skills: Option<Spanned<Vec<Spanned<String>>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_action_verb_names() {
        let infer = RawTaskAction::Infer(Spanned::dummy(RawInferAction::default()));
        assert_eq!(infer.verb_name(), "infer");

        let exec = RawTaskAction::Exec(Spanned::dummy(RawExecAction::default()));
        assert_eq!(exec.verb_name(), "exec");

        let fetch = RawTaskAction::Fetch(Spanned::dummy(RawFetchAction::default()));
        assert_eq!(fetch.verb_name(), "fetch");

        let invoke = RawTaskAction::Invoke(Spanned::dummy(RawInvokeAction::default()));
        assert_eq!(invoke.verb_name(), "invoke");

        let agent = RawTaskAction::Agent(Spanned::dummy(RawAgentAction::default()));
        assert_eq!(agent.verb_name(), "agent");
    }

    #[test]
    fn test_invoke_parse_tool_name() {
        // Simple tool name
        let simple = RawInvokeAction {
            tool: Spanned::new("my_tool".to_string(), make_span(0, 7)),
            ..Default::default()
        };
        let (server, name) = simple.parse_tool_name();
        assert_eq!(server, None);
        assert_eq!(name, "my_tool");

        // Server-qualified name
        let qualified = RawInvokeAction {
            tool: Spanned::new("novanet::query".to_string(), make_span(0, 14)),
            ..Default::default()
        };
        let (server, name) = qualified.parse_tool_name();
        assert_eq!(server, Some("novanet"));
        assert_eq!(name, "query");
    }

    #[test]
    fn test_infer_action_fields() {
        let infer = RawInferAction {
            prompt: Spanned::new("Hello, world!".to_string(), make_span(0, 13)),
            temperature: Some(Spanned::new(0.7, make_span(20, 23))),
            max_tokens: Some(Spanned::new(1000, make_span(30, 34))),
            ..Default::default()
        };

        assert_eq!(infer.prompt.value, "Hello, world!");
        assert_eq!(infer.temperature.as_ref().unwrap().value, 0.7);
        assert_eq!(infer.max_tokens.as_ref().unwrap().value, 1000);
    }
}
