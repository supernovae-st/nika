// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw action types — the 4 verbs of the Nika workflow model.
//!
//! Spec `02-verbs.md` · « A verb is a **distinct native execution model**
//! the engine itself implements — call a model, run a process, dispatch a
//! tool, drive an agentic loop. That is the whole operation space. »
//!
//! `fetch` is NOT a verb (D-2026-05-22-N18 · 4 verbs absolute) — it is the
//! `nika:fetch` builtin reached via `invoke:`. Each verb's field set below
//! is CLOSED per the spec field tables — strict mode rejects unknowns.

use crate::source::Spanned;
use crate::types::CaptureMode;

/// The action a task performs — one of the 4 verbs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawAction {
    /// `infer:` — single LLM call.
    Infer(Box<RawInferAction>),
    /// `exec:` — shell command with sandboxing.
    Exec(RawExecAction),
    /// `invoke:` — call a builtin tool OR an MCP tool.
    Invoke(RawInvokeAction),
    /// `agent:` — multi-turn agentic loop with tool calls.
    Agent(Box<RawAgentAction>),
}

impl RawAction {
    /// The verb key this action binds (`infer` · `exec` · `invoke` ·
    /// `agent`).
    #[must_use]
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Infer(_) => "infer",
            Self::Exec(_) => "exec",
            Self::Invoke(_) => "invoke",
            Self::Agent(_) => "agent",
        }
    }
}

/// Extended-thinking config — `infer.thinking:` (spec `02-verbs.md` §infer
/// · `{ enabled, budget_tokens }`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Thinking {
    /// Whether extended thinking is enabled.
    pub enabled: bool,
    /// Optional thinking-token budget.
    pub budget_tokens: Option<u32>,
}

impl Thinking {
    /// Create a thinking config.
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            budget_tokens: None,
        }
    }
}

/// One `infer.vision:` input — `{ source: file|url, path|url }`
/// (spec `02-verbs.md` §infer fields table).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VisionInput {
    /// `source: file` — a local image path.
    File {
        /// The image path.
        path: Spanned<String>,
    },
    /// `source: url` — a remote image URL.
    Url {
        /// The image URL.
        url: Spanned<String>,
    },
}

/// `infer:` — a single LLM call (spec `02-verbs.md` §infer).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawInferAction {
    /// `prompt:` — REQUIRED · user prompt (may carry `${{ }}`).
    pub prompt: Spanned<String>,
    /// `system:` — system prompt.
    pub system: Option<Spanned<String>>,
    /// `model:` — `<provider>/<name>` · ONE string · « there is no
    /// separate `provider:` field » (spec `01-envelope.md` §model).
    pub model: Option<Spanned<String>>,
    /// `temperature:` — sampling temperature (number 0-2).
    pub temperature: Option<Spanned<f64>>,
    /// `max_tokens:` — max output tokens.
    pub max_tokens: Option<Spanned<u32>>,
    /// `schema:` — JSON Schema · structured-output validation.
    pub schema: Option<Spanned<serde_json::Value>>,
    /// `thinking:` — extended thinking config.
    pub thinking: Option<Spanned<Thinking>>,
    /// `vision:` — image inputs.
    pub vision: Vec<Spanned<VisionInput>>,
}

impl RawInferAction {
    /// Create a new infer action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            system: None,
            model: None,
            temperature: None,
            max_tokens: None,
            schema: None,
            thinking: None,
            vision: Vec::new(),
        }
    }
}

/// The `command:` of an `exec:` task — a shell string OR an argv array
/// (spec `02-verbs.md` §exec).
///
/// - **`Shell`** (`command: "a | b"`) runs through `/bin/sh -c` — pipes and
///   redirects work, the shell blocklist applies.
/// - **`Argv`** (`command: ["prog", "arg"]`) runs through `execve` with NO
///   shell — the structural fix for command injection: an interpolated
///   value is ONE argv element no matter what it contains, no shell to
///   parse it. The one-obvious-way for any command carrying untrusted
///   values.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawCommand {
    /// `/bin/sh -c <string>` — the shell-feature path.
    Shell(Spanned<String>),
    /// `execve(argv)` — no shell. Non-empty by construction (the parser
    /// rejects an empty array).
    Argv(Vec<Spanned<String>>),
}

impl RawCommand {
    /// The leading program of the argv form — unambiguous, no shell-split
    /// heuristic. `None` for the shell form (its program is the caller's
    /// leading-token problem).
    #[must_use]
    pub fn argv_program(&self) -> Option<&str> {
        match self {
            Self::Argv(parts) => parts.first().map(|p| p.value.as_str()),
            Self::Shell(_) => None,
        }
    }

    /// The shell-string form, when this IS a shell command (`None` for
    /// argv) — for lints/checks that only apply to `/bin/sh -c`.
    #[must_use]
    pub fn shell_str(&self) -> Option<&str> {
        match self {
            Self::Shell(s) => Some(s.value.as_str()),
            Self::Argv(_) => None,
        }
    }

    /// Every literal text fragment of the command (the whole string for
    /// `Shell` · each element for `Argv`) — for the secret-leak / `${{ }}`
    /// scan, which must see interpolations in any argv element.
    #[must_use]
    pub fn text_fragments(&self) -> Vec<&str> {
        match self {
            Self::Shell(s) => vec![s.value.as_str()],
            Self::Argv(parts) => parts.iter().map(|p| p.value.as_str()).collect(),
        }
    }
}

/// `exec:` — shell command execution (spec `02-verbs.md` §exec).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawExecAction {
    /// `command:` — REQUIRED · a shell string OR an argv array.
    pub command: RawCommand,
    /// `cwd:` — working directory.
    pub cwd: Option<Spanned<String>>,
    /// `env:` — OS environment of **this subprocess** (≠ the envelope
    /// `env:` · spec `02-verbs.md` · « They are NOT auto-connected »).
    pub env: Vec<(Spanned<String>, Spanned<String>)>,
    /// `stdin:` — stdin data (may carry `${{ }}`).
    pub stdin: Option<Spanned<String>>,
    /// `capture:` — stdout (default) · stderr · combined · structured.
    pub capture: Option<Spanned<CaptureMode>>,
}

impl RawExecAction {
    /// Create a new exec action with a shell-string command.
    #[must_use]
    pub fn new(command: Spanned<String>) -> Self {
        Self::with_command(RawCommand::Shell(command))
    }

    /// Create a new exec action with an explicit [`RawCommand`].
    #[must_use]
    pub fn with_command(command: RawCommand) -> Self {
        Self {
            command,
            cwd: None,
            env: Vec::new(),
            stdin: None,
            capture: None,
        }
    }
}

/// `invoke:` — builtin / MCP tool call (spec `02-verbs.md` §invoke).
///
/// Tool reference grammar · `<namespace>:<path>` — « the **colon** marks
/// the namespace boundary (exactly once), the **slash** separates the
/// path within it. »
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawInvokeAction {
    /// `tool:` — REQUIRED · `nika:<path>` OR `mcp:<server>/<tool>`.
    pub tool: Spanned<String>,
    /// `args:` — tool arguments (schema is tool-specific).
    pub args: Option<Spanned<serde_json::Value>>,
}

impl RawInvokeAction {
    /// Create an invoke action for the given tool reference.
    #[must_use]
    pub fn new(tool: Spanned<String>) -> Self {
        Self { tool, args: None }
    }
}

/// `agent:` — multi-turn agentic loop (spec `02-verbs.md` §agent).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawAgentAction {
    /// `prompt:` — REQUIRED · initial user message.
    pub prompt: Spanned<String>,
    /// `system:` — system prompt.
    pub system: Option<Spanned<String>>,
    /// `model:` — `<provider>/<name>` override.
    pub model: Option<Spanned<String>>,
    /// `tools:` — whitelist · gitignore-style globs · **default-deny**
    /// (« if absent the agent gets NO tools · least-privilege »).
    pub tools: Vec<Spanned<String>>,
    /// `max_turns:` — loop limit · default 10 (spec field table).
    pub max_turns: Option<Spanned<u32>>,
    /// `max_tokens_total:` — cumulative token budget.
    pub max_tokens_total: Option<Spanned<u64>>,
    /// `temperature:` — sampling temperature (number 0-2).
    pub temperature: Option<Spanned<f64>>,
    /// `schema:` — JSON Schema · validates the **final message**.
    pub schema: Option<Spanned<serde_json::Value>>,
}

impl RawAgentAction {
    /// Create a new agent action with the given prompt.
    #[must_use]
    pub fn new(prompt: Spanned<String>) -> Self {
        Self {
            prompt,
            system: None,
            model: None,
            tools: Vec::new(),
            max_turns: None,
            max_tokens_total: None,
            temperature: None,
            schema: None,
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
        assert!(a.schema.is_none());
        assert!(a.vision.is_empty());
    }

    #[test]
    fn exec_action_new() {
        let a = RawExecAction::new(span_str("ls -la"));
        assert_eq!(a.command.shell_str(), Some("ls -la"));
        assert!(a.env.is_empty());
        assert!(a.capture.is_none());
    }

    #[test]
    fn invoke_action_new() {
        let a = RawInvokeAction::new(span_str("nika:fetch"));
        assert_eq!(a.tool.value, "nika:fetch");
        assert!(a.args.is_none());
    }

    #[test]
    fn agent_action_new() {
        let a = RawAgentAction::new(span_str("Research quantum computing"));
        assert_eq!(a.prompt.value, "Research quantum computing");
        assert!(a.tools.is_empty(), "tools default-deny · empty whitelist");
        assert!(a.max_turns.is_none());
    }

    #[test]
    fn verb_names() {
        assert_eq!(
            RawAction::Infer(Box::new(RawInferAction::new(span_str("x")))).verb(),
            "infer"
        );
        assert_eq!(
            RawAction::Exec(RawExecAction::new(span_str("x"))).verb(),
            "exec"
        );
        assert_eq!(
            RawAction::Invoke(RawInvokeAction::new(span_str("nika:read"))).verb(),
            "invoke"
        );
        assert_eq!(
            RawAction::Agent(Box::new(RawAgentAction::new(span_str("x")))).verb(),
            "agent"
        );
    }

    #[test]
    fn thinking_new() {
        let t = Thinking::new(true);
        assert!(t.enabled);
        assert!(t.budget_tokens.is_none());
    }
}
