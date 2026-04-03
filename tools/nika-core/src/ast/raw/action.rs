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
    Fetch(Box<Spanned<RawFetchAction>>),

    /// MCP tool invocation: invoke: tool_name or invoke: { tool: name, params: {...} }
    Invoke(Spanned<RawInvokeAction>),

    /// Autonomous agent: agent: { prompt: "..." }
    Agent(Box<Spanned<RawAgentAction>>),
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
    /// The prompt to send to the LLM (optional when `content` is present)
    pub prompt: Spanned<String>,

    /// System prompt override
    pub system: Option<Spanned<String>>,

    /// Temperature (0.0 - 2.0)
    pub temperature: Option<Spanned<f64>>,

    /// Maximum tokens to generate
    pub max_tokens: Option<Spanned<u32>>,

    /// Enable extended thinking (Claude)
    pub extended_thinking: Option<Spanned<bool>>,

    /// Thinking budget tokens
    pub thinking_budget: Option<Spanned<u32>>,

    /// Multimodal content parts (text + images) for vision models
    pub content: Option<Spanned<Vec<crate::ast::content::RawContentPart>>>,

    /// Expected response format: text, json, markdown
    pub response_format: Option<Spanned<String>>,

    /// Guardrails for validating infer output
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
}

/// Parameters for the `exec` verb (shell command execution).
#[derive(Debug, Clone, Default)]
pub struct RawExecAction {
    /// Command to execute (string or array)
    pub command: Spanned<String>,

    /// Run through shell (sh -c) - defaults to false for security
    pub shell: Option<Spanned<bool>>,

    /// Working directory
    pub cwd: Option<Spanned<String>>,

    /// Environment variables
    pub env: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Timeout in milliseconds
    pub timeout_ms: Option<Spanned<u64>>,

    /// Maximum stdout size in bytes before truncation. Default: 50 MB.
    pub max_stdout: Option<Spanned<u64>>,
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

    /// Response mode: "full" (status + headers + body) or "binary" (CAS store)
    pub response: Option<Spanned<String>>,

    /// Extraction mode: markdown, article, text, selector, metadata, links, feed, jsonpath, llm_txt
    pub extract: Option<Spanned<String>>,

    /// CSS selector or JSONPath expression (used with extract: selector, text, jsonpath)
    pub selector: Option<Spanned<String>>,

    /// Enable cookie jar for session persistence across fetch tasks
    pub session: Option<Spanned<bool>>,

    /// Enable HTTP response caching with ETag / If-Modified-Since
    pub cache: Option<Spanned<bool>>,
}

/// Parameters for the `invoke` verb (MCP tool invocation).
#[derive(Debug, Clone, Default)]
pub struct RawInvokeAction {
    /// MCP tool name: "tool_name" or "server::tool_name"
    /// Required unless `resource` is set.
    pub tool: Option<Spanned<String>>,

    /// MCP resource URI (alternative to tool call)
    pub resource: Option<Spanned<String>>,

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
    /// Returns None if no tool is specified (resource-only invoke).
    pub fn parse_tool_name(&self) -> Option<(Option<&str>, &str)> {
        let tool = self.tool.as_ref()?;
        let tool_str = &tool.value;
        if let Some((server, name)) = tool_str.split_once("::") {
            Some((Some(server), name))
        } else {
            Some((None, tool_str.as_str()))
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

    /// Maximum turns before stopping
    pub max_turns: Option<Spanned<u32>>,

    /// Maximum tokens per response
    pub max_tokens: Option<Spanned<u32>>,

    /// Agent definition reference (agents: section)
    pub from: Option<Spanned<String>>,

    /// Skills to inject into system prompt
    pub skills: Option<Spanned<Vec<Spanned<String>>>>,

    /// Provider override (inside agent: block)
    pub provider: Option<Spanned<String>>,

    /// Model override (inside agent: block)
    pub model: Option<Spanned<String>>,

    /// MCP servers for tool access
    pub mcp: Option<Spanned<Vec<Spanned<String>>>>,

    /// Temperature for LLM sampling
    pub temperature: Option<Spanned<f64>>,

    /// Token budget for the agent
    pub token_budget: Option<Spanned<u32>>,

    /// System prompt (agent persona)
    pub system: Option<Spanned<String>>,

    /// Enable extended thinking (Claude)
    pub extended_thinking: Option<Spanned<bool>>,

    /// Thinking budget tokens
    pub thinking_budget: Option<Spanned<u32>>,

    /// Max spawn_agent recursion depth
    pub depth_limit: Option<Spanned<u32>>,

    /// Tool choice behavior: auto, required, none
    pub tool_choice: Option<Spanned<String>>,

    /// Sequences that stop generation (passed to LLM)
    pub stop_sequences: Option<Spanned<Vec<Spanned<String>>>>,

    /// Scope preset (full, minimal, debug)
    pub scope: Option<Spanned<String>>,
    /// Guardrails for validating agent outputs.
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
    /// Completion behavior configuration.
    pub completion: Option<crate::ast::completion::CompletionConfig>,
    /// Execution limits for cost control.
    pub limits: Option<crate::ast::limits::LimitsConfig>,
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

        let agent = RawTaskAction::Agent(Box::new(Spanned::dummy(RawAgentAction::default())));
        assert_eq!(agent.verb_name(), "agent");
    }

    #[test]
    fn test_invoke_parse_tool_name() {
        // Simple tool name
        let simple = RawInvokeAction {
            tool: Some(Spanned::new("my_tool".to_string(), make_span(0, 7))),
            ..Default::default()
        };
        let (server, name) = simple.parse_tool_name().unwrap();
        assert_eq!(server, None);
        assert_eq!(name, "my_tool");

        // Server-qualified name
        let qualified = RawInvokeAction {
            tool: Some(Spanned::new("novanet::query".to_string(), make_span(0, 14))),
            ..Default::default()
        };
        let (server, name) = qualified.parse_tool_name().unwrap();
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
