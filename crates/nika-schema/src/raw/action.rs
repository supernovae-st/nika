// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw action types — the four verb actions in the Nika workflow model.
//!
//! `fetch` is NOT a verb (spec D-2026-05-22-N18 · 4 verbs absolute) — it is
//! the `nika:fetch` builtin reached via `invoke:`. The HTTP request shape it
//! used to carry now lives in the catalog/builtin layer, not the parser AST.

use crate::source::Spanned;

/// The action a task performs — one of four verbs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawAction {
    /// Call an LLM for inference.
    Infer(RawInferAction),
    /// Execute a shell command.
    Exec(RawExecAction),
    /// Invoke a builtin or MCP tool.
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
    /// Model override (`<provider>/<name>`).
    pub model: Option<Spanned<String>>,
}

impl RawInferAction {
    /// Create a new infer action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            system: None,
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
    /// Working directory.
    pub cwd: Option<Spanned<String>>,
    /// Environment variables.
    pub env: Vec<(Spanned<String>, Spanned<String>)>,
}

impl RawExecAction {
    /// Create a new exec action with the given command.
    #[must_use]
    pub fn new(command: Spanned<String>) -> Self {
        Self {
            command,
            cwd: None,
            env: Vec::new(),
        }
    }
}

/// Raw invoke action — builtin / MCP tool invocation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawInvokeAction {
    /// Tool name to invoke.
    pub tool: Option<Spanned<String>>,
    /// Resource URI to read.
    pub resource: Option<Spanned<String>>,
}

impl RawInvokeAction {
    /// Create a new empty invoke action.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool: None,
            resource: None,
        }
    }

    /// Create an invoke action with the target (tool or resource)
    /// already populated.
    #[must_use]
    pub fn with_target(tool: Option<Spanned<String>>, resource: Option<Spanned<String>>) -> Self {
        Self { tool, resource }
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
    /// System prompt.
    pub system: Option<Spanned<String>>,
    /// Model override (`<provider>/<name>`).
    pub model: Option<Spanned<String>>,
}

impl RawAgentAction {
    /// Create a new agent action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            tools: Vec::new(),
            system: None,
            model: None,
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
    }

    #[test]
    fn exec_action_new() {
        let a = RawExecAction::new(span_str("ls -la"));
        assert_eq!(a.command.value, "ls -la");
        assert!(a.env.is_empty());
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
    }

    #[test]
    fn raw_action_enum_variants() {
        let infer = RawAction::Infer(RawInferAction::new(span_str("test")));
        assert!(matches!(infer, RawAction::Infer(_)));

        let exec = RawAction::Exec(RawExecAction::new(span_str("echo")));
        assert!(matches!(exec, RawAction::Exec(_)));
    }
}
