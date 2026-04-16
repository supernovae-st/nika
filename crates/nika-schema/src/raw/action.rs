// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw action types — the five verb actions in the Nika workflow model.

use crate::source::Spanned;
use crate::types::{ContentPart, Templatable};

/// The action a task performs — one of five verbs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawAction {
    /// Call an LLM for inference.
    Infer(RawInferAction),
    /// Execute a shell command.
    Exec(RawExecAction),
    /// Fetch from a URL.
    Fetch(Box<RawFetchAction>),
    /// Invoke an MCP tool or resource.
    Invoke(RawInvokeAction),
    /// Run an agent loop.
    Agent(Box<RawAgentAction>),
}

/// Raw infer action — LLM call configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawInferAction {
    /// The prompt text.
    pub prompt: Spanned<String>,
    /// System prompt.
    pub system: Option<Spanned<String>>,
    /// Temperature (0.0-2.0).
    pub temperature: Option<Spanned<Templatable<f64>>>,
    /// Maximum output tokens.
    pub max_tokens: Option<Spanned<Templatable<u32>>>,
    /// Enable extended thinking.
    pub extended_thinking: Option<Spanned<Templatable<bool>>>,
    /// Thinking token budget.
    pub thinking_budget: Option<Spanned<Templatable<u32>>>,
    /// Multi-modal content parts.
    pub content: Vec<ContentPart>,
    /// Response format hint.
    pub response_format: Option<Spanned<String>>,
    /// Provider override.
    pub provider: Option<Spanned<String>>,
    /// Model override.
    pub model: Option<Spanned<String>>,
}

impl RawInferAction {
    /// Create a new infer action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            system: None,
            temperature: None,
            max_tokens: None,
            extended_thinking: None,
            thinking_budget: None,
            content: Vec::new(),
            response_format: None,
            provider: None,
            model: None,
        }
    }
}

/// Raw exec action — shell command execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawExecAction {
    /// The command to execute.
    pub command: Spanned<String>,
    /// Whether to use a shell (default: true).
    pub shell: Option<Spanned<Templatable<bool>>>,
    /// Working directory.
    pub cwd: Option<Spanned<String>>,
    /// Environment variables.
    pub env: Vec<(Spanned<String>, Spanned<String>)>,
    /// Timeout in milliseconds.
    pub timeout_ms: Option<Spanned<Templatable<u64>>>,
    /// Maximum stdout bytes to capture.
    pub max_stdout: Option<Spanned<Templatable<u64>>>,
}

impl RawExecAction {
    /// Create a new exec action with the given command.
    #[must_use]
    pub fn new(command: Spanned<String>) -> Self {
        Self {
            command,
            shell: None,
            cwd: None,
            env: Vec::new(),
            timeout_ms: None,
            max_stdout: None,
        }
    }
}

/// Raw fetch action — HTTP request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawFetchAction {
    /// URL to fetch.
    pub url: Spanned<String>,
    /// HTTP method (GET, POST, etc.).
    pub method: Option<Spanned<String>>,
    /// HTTP headers.
    pub headers: Vec<(Spanned<String>, Spanned<String>)>,
    /// Request body (text).
    pub body: Option<Spanned<String>>,
    /// Request body (JSON).
    pub json: Option<Spanned<serde_json::Value>>,
    /// Timeout in milliseconds.
    pub timeout_ms: Option<Spanned<Templatable<u64>>>,
    /// Whether to follow redirects.
    pub follow_redirects: Option<Spanned<Templatable<bool>>>,
    /// Response mode hint.
    pub response: Option<Spanned<String>>,
    /// Extract mode hint.
    pub extract: Option<Spanned<String>>,
    /// CSS selector for extraction.
    pub selector: Option<Spanned<String>>,
    /// Whether to maintain a session.
    pub session: Option<Spanned<Templatable<bool>>>,
    /// Whether to cache the response.
    pub cache: Option<Spanned<Templatable<bool>>>,
}

impl RawFetchAction {
    /// Create a new fetch action for the given URL.
    #[must_use]
    pub fn new(url: Spanned<String>) -> Self {
        Self {
            url,
            method: None,
            headers: Vec::new(),
            body: None,
            json: None,
            timeout_ms: None,
            follow_redirects: None,
            response: None,
            extract: None,
            selector: None,
            session: None,
            cache: None,
        }
    }
}

/// Raw invoke action — MCP tool/resource invocation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawInvokeAction {
    /// Tool name to invoke.
    pub tool: Option<Spanned<String>>,
    /// Resource URI to read.
    pub resource: Option<Spanned<String>>,
    /// Parameters (JSON).
    pub params: Option<Spanned<serde_json::Value>>,
    /// MCP server alias.
    pub mcp: Option<Spanned<String>>,
    /// Timeout in milliseconds.
    pub timeout_ms: Option<Spanned<Templatable<u64>>>,
}

impl RawInvokeAction {
    /// Create a new empty invoke action.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool: None,
            resource: None,
            params: None,
            mcp: None,
            timeout_ms: None,
        }
    }

    /// Create an invoke action with the target (tool or resource)
    /// already populated. At least one of `tool` / `resource` must
    /// be `Some` for the action to be semantically valid, but this
    /// constructor does not enforce that — the parser does.
    #[must_use]
    pub fn with_target(tool: Option<Spanned<String>>, resource: Option<Spanned<String>>) -> Self {
        Self {
            tool,
            resource,
            params: None,
            mcp: None,
            timeout_ms: None,
        }
    }
}

impl Default for RawInvokeAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw agent action — agent loop configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawAgentAction {
    /// Agent prompt.
    pub prompt: Spanned<String>,
    /// Available tools.
    pub tools: Vec<Spanned<String>>,
    /// Maximum agent turns.
    pub max_turns: Option<Spanned<Templatable<u32>>>,
    /// Maximum tokens per turn.
    pub max_tokens: Option<Spanned<Templatable<u32>>>,
    /// Agent definition reference (from `agents:` section).
    pub from: Option<Spanned<String>>,
    /// Available skills.
    pub skills: Vec<Spanned<String>>,
    /// Provider override.
    pub provider: Option<Spanned<String>>,
    /// Model override.
    pub model: Option<Spanned<String>>,
    /// MCP server aliases.
    pub mcp: Vec<Spanned<String>>,
    /// Temperature.
    pub temperature: Option<Spanned<Templatable<f64>>>,
    /// Token budget for the agent.
    pub token_budget: Option<Spanned<Templatable<u32>>>,
    /// System prompt.
    pub system: Option<Spanned<String>>,
    /// Extended thinking.
    pub extended_thinking: Option<Spanned<Templatable<bool>>>,
    /// Thinking budget.
    pub thinking_budget: Option<Spanned<Templatable<u32>>>,
    /// Maximum recursion depth.
    pub depth_limit: Option<Spanned<Templatable<u32>>>,
    /// Tool choice constraint.
    pub tool_choice: Option<Spanned<String>>,
    /// Stop sequences.
    pub stop_sequences: Vec<Spanned<String>>,
    /// Agent scope.
    pub scope: Option<Spanned<String>>,
}

impl RawAgentAction {
    /// Create a new agent action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            tools: Vec::new(),
            max_turns: None,
            max_tokens: None,
            from: None,
            skills: Vec::new(),
            provider: None,
            model: None,
            mcp: Vec::new(),
            temperature: None,
            token_budget: None,
            system: None,
            extended_thinking: None,
            thinking_budget: None,
            depth_limit: None,
            tool_choice: None,
            stop_sequences: Vec::new(),
            scope: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    fn span_str(s: &str) -> Spanned<String> {
        Spanned {
            value: s.into(),
            span: Span::default(),
        }
    }

    #[test]
    fn infer_action_new() {
        let a = RawInferAction::new(span_str("Summarize this"));
        assert_eq!(a.prompt.value, "Summarize this");
        assert!(a.system.is_none());
        assert!(a.content.is_empty());
    }

    #[test]
    fn exec_action_new() {
        let a = RawExecAction::new(span_str("ls -la"));
        assert_eq!(a.command.value, "ls -la");
        assert!(a.env.is_empty());
    }

    #[test]
    fn fetch_action_new() {
        let a = RawFetchAction::new(span_str("https://api.example.com"));
        assert_eq!(a.url.value, "https://api.example.com");
        assert!(a.headers.is_empty());
    }

    #[test]
    fn invoke_action_new() {
        let a = RawInvokeAction::new();
        assert!(a.tool.is_none());
        assert!(a.resource.is_none());
    }

    #[test]
    fn agent_action_new() {
        let a = RawAgentAction::new(span_str("Research quantum computing"));
        assert_eq!(a.prompt.value, "Research quantum computing");
        assert!(a.tools.is_empty());
        assert!(a.mcp.is_empty());
    }

    #[test]
    fn raw_action_enum_variants() {
        let infer = RawAction::Infer(RawInferAction::new(span_str("test")));
        assert!(matches!(infer, RawAction::Infer(_)));

        let exec = RawAction::Exec(RawExecAction::new(span_str("echo")));
        assert!(matches!(exec, RawAction::Exec(_)));

        let fetch = RawAction::Fetch(Box::new(RawFetchAction::new(span_str("http://x"))));
        assert!(matches!(fetch, RawAction::Fetch(_)));
    }
}
