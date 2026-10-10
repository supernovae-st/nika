// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A persistent ACP conversation: one agent session kept open across a Session's turns,
//! whose only tools are Nika's own — a session's tools served over MCP — under the audited
//! conversation profile of its adapter (claude-agent-acp 0.81.1, codex-acp 1.13.1).
//!
//! `session/new` runs once per conversation, then one `session/prompt` per user entry; the
//! agent keeps the history and runs its own loop until it answers, with no turn limit. The
//! profile closes everything else before the first prompt: no built-in tools, no settings,
//! plugins, skills or agents, nothing persisted, and exactly one MCP server, Nika's
//! ([`ToolOffer`]). Claude Code mounts it from `session/new`; Codex from its own configuration,
//! read back at spawn with every other server disabled.
//!
//! The transport follows the agent's capability and is recorded: HTTP when the agent
//! advertises `mcpCapabilities.http` and Nika offers an HTTP endpoint, else Nika's stdio
//! bridge when offered; otherwise no conversation opens, and nothing falls back to the
//! one-shot path.
//!
//! Nika answers every `session/request_permission` itself: `allow_once` only for one of the
//! offered tools on Nika's server, a rejection for everything else (the agent continues
//! without it), each decision reported to the turn and recorded. `allow_always` is never
//! chosen. Stop sends `session/cancel` once per turn and records it. Each turn ends with the
//! activity record of the one-shot calls (first thought and answer, last update kind), where
//! tool frames are expected activity; while a tool call is open, the turn's silence allowance
//! waits for it (the tool's time is Nika's own, bounded by the Session and its Stop).

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_core::Stream;
use nika_kernel::ai::harness::{HarnessError, HarnessSelection};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::authoring::acp::{Profile, codex, refusal};
use crate::wire::{PermissionOptionIn, PermissionResult};

/// Nika's tool server as one conversation is offered it: the MCP server name, the tools it
/// serves, and the endpoints it can be reached at.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolOffer {
    /// The MCP server name (`nika`: the agent sees `mcp__nika__<tool>`).
    pub name: String,
    /// The names of the tools it serves.
    pub tools: Vec<String>,
    /// Its HTTP endpoint and bearer, when offered.
    pub http: Option<HttpMount>,
    /// Its stdio bridge, when offered.
    pub stdio: Option<StdioMount>,
}

impl ToolOffer {
    /// Construct (INV-019): a server reachable at no endpoint yet.
    #[must_use]
    pub fn new(name: impl Into<String>, tools: Vec<String>) -> Self {
        Self {
            name: name.into(),
            tools,
            http: None,
            stdio: None,
        }
    }

    /// Offer the server's HTTP endpoint (`http://127.0.0.1:<port>/mcp`) and the bearer it
    /// requires.
    #[must_use]
    pub fn with_http(mut self, url: impl Into<String>, bearer: impl Into<String>) -> Self {
        self.http = Some(HttpMount {
            url: url.into(),
            bearer: bearer.into(),
        });
        self
    }

    /// Offer the server through a stdio bridge the agent starts: `command`, `args` and the
    /// environment it needs.
    #[must_use]
    pub fn with_stdio(
        mut self,
        command: impl Into<String>,
        args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> Self {
        self.stdio = Some(StdioMount {
            command: command.into(),
            args,
            env,
        });
        self
    }
}

impl std::fmt::Debug for ToolOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolOffer")
            .field("name", &self.name)
            .field("tools", &self.tools)
            .field("http", &self.http.as_ref().map(|_| "<endpoint and bearer>"))
            .field("stdio", &self.stdio.as_ref().map(|mount| &mount.command))
            .finish()
    }
}

/// Where Nika's tool server answers over HTTP. Its bearer is a secret: it reaches only the
/// agent that mounts the server, never a record or a debug rendering.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpMount {
    url: String,
    bearer: String,
}

/// How an agent starts a stdio bridge to Nika's tool server.
#[derive(Clone, PartialEq, Eq)]
pub struct StdioMount {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

/// How the agent reaches Nika's tool server in one conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Transport {
    /// MCP over Streamable HTTP, on loopback, with the conversation's bearer.
    Http,
    /// MCP over the stdio of a bridge the agent starts.
    Stdio,
}

impl Transport {
    /// The record's word for the transport.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Stdio => "stdio",
        }
    }
}

/// What one conversation asks of its agent: a model and a reasoning effort, each verbatim and
/// applied before the first prompt (or refused, never dropped), and the silence a turn may keep.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConversationSetup {
    /// The model to select, verbatim; `None` keeps the agent's own.
    pub requested_model: Option<String>,
    /// The reasoning effort to apply, the route's native value; `None` keeps the agent's own.
    pub requested_effort: Option<String>,
    /// How long a turn may go without a frame of its agent working while no tool call is open,
    /// and each write's bound.
    pub allowance: Duration,
}

impl ConversationSetup {
    /// Construct (INV-019): the agent's own model and effort, the generic transport allowance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            requested_model: None,
            requested_effort: None,
            allowance: Duration::from_secs(crate::IDLE_TIMEOUT_SECS),
        }
    }

    /// Select this model, verbatim.
    #[must_use]
    pub fn with_requested_model(mut self, model: impl Into<String>) -> Self {
        self.requested_model = Some(model.into());
        self
    }

    /// Apply this native reasoning effort.
    #[must_use]
    pub fn with_requested_effort(mut self, effort: impl Into<String>) -> Self {
        self.requested_effort = Some(effort.into());
        self
    }

    /// Allow a turn this much silence.
    #[must_use]
    pub fn with_allowance(mut self, allowance: Duration) -> Self {
        self.allowance = allowance;
        self
    }
}

impl Default for ConversationSetup {
    fn default() -> Self {
        Self::new()
    }
}

/// How a turn ended, in Nika's closed words for the agent's stop reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnEnd {
    /// The agent answered and ended its turn.
    EndTurn,
    /// The agent reached its token limit.
    MaxTokens,
    /// The agent reached its limit of model requests in one turn.
    MaxTurnRequests,
    /// The agent declined to continue.
    Refusal,
    /// The turn was called off (after Stop, the agent's answer to `session/cancel`).
    Cancelled,
    /// A stop reason this client does not know, or none.
    Other,
}

impl TurnEnd {
    pub(crate) fn of(stop: Option<&str>) -> Self {
        match stop {
            Some("end_turn") => Self::EndTurn,
            Some("max_tokens") => Self::MaxTokens,
            Some("max_turn_requests") => Self::MaxTurnRequests,
            Some("refusal") => Self::Refusal,
            Some("cancelled") => Self::Cancelled,
            _ => Self::Other,
        }
    }

    /// The record's word for the end.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::MaxTurnRequests => "max_turn_requests",
            Self::Refusal => "refusal",
            Self::Cancelled => "cancelled",
            Self::Other => "other",
        }
    }
}

/// One observed beat of a turn.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TurnEvent {
    /// A chunk of the agent's answer text.
    Answer {
        /// The text delta.
        text: String,
    },
    /// A chunk of the agent's summarized thinking.
    Thought {
        /// The text delta.
        text: String,
    },
    /// The agent's tool call began or changed.
    Tool {
        /// The agent's id of the call.
        tool_call_id: String,
        /// Its status as the agent reported it (`pending`, `in_progress`, `completed`,
        /// `failed`), when it did.
        status: Option<String>,
        /// The offered tool it names, when it is one of Nika's.
        nika_tool: Option<String>,
    },
    /// Nika answered a permission the agent asked.
    Permission {
        /// The tool the agent named, when it named one plainly.
        tool: Option<String>,
        /// The offered tool it is, when it is one of Nika's.
        nika_tool: Option<String>,
        /// Allowed once (one of Nika's tools), or rejected.
        allowed: bool,
    },
    /// The turn ended: the terminal beat.
    Ended {
        /// How.
        end: TurnEnd,
        /// The turn's record: its permissions, its Stop, its bounds and its activity.
        record: Value,
    },
}

/// The beats of one turn, in order, ending with [`TurnEvent::Ended`]; an error ends the turn
/// (and, for a transport failure or a timed out turn, the conversation).
#[derive(Debug)]
pub struct TurnStream(mpsc::Receiver<Result<TurnEvent, HarnessError>>);

impl TurnStream {
    /// The turn's next beat; `None` once the turn is over.
    pub async fn next_beat(&mut self) -> Option<Result<TurnEvent, HarnessError>> {
        self.0.recv().await
    }
}

impl Stream for TurnStream {
    type Item = Result<TurnEvent, HarnessError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

/// What the conversation's driver receives from its handle.
pub(crate) enum Command {
    /// Send this user entry as the next turn, its beats to `events`.
    Prompt {
        text: String,
        events: mpsc::Sender<Result<TurnEvent, HarnessError>>,
    },
    /// Stop the turn in flight.
    Stop,
}

/// One open conversation. Dropping it ends the conversation: the driver stops, the adapter
/// process and its scratch directory go with it.
#[derive(Debug)]
pub struct Conversation {
    commands: mpsc::UnboundedSender<Command>,
    transport: Transport,
    record: Value,
}

impl Conversation {
    pub(crate) const fn new(
        commands: mpsc::UnboundedSender<Command>,
        transport: Transport,
        record: Value,
    ) -> Self {
        Self {
            commands,
            transport,
            record,
        }
    }

    /// How the agent reaches Nika's tool server.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }

    /// The opening record: the profile, the transport chosen and why, the server mounted, the
    /// selection applied. No endpoint or secret is in it.
    #[must_use]
    pub const fn record(&self) -> &Value {
        &self.record
    }

    /// Send `text` as the next user entry. One turn runs at a time: a prompt sent while a turn
    /// is in flight receives an error, and that turn goes on.
    ///
    /// # Errors
    /// The conversation has ended (its transport failed, a turn timed out, or its agent left).
    pub fn prompt(&self, text: impl Into<String>) -> Result<TurnStream, HarnessError> {
        let (events, beats) = mpsc::channel(64);
        let text = text.into();
        (self.commands.send(Command::Prompt { text, events })).map_err(|_| ended())?;
        Ok(TurnStream(beats))
    }

    /// Stop the turn in flight: `session/cancel` is sent once per turn; the turn then ends with
    /// [`TurnEnd::Cancelled`] when the agent answers it. Without a turn in flight, nothing.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

pub(crate) fn ended() -> HarnessError {
    HarnessError::Session {
        reason: "the ACP conversation has ended; open a new one".to_owned(),
    }
}

/// The transport `profile`'s agent mounts `offer` over: HTTP when the agent can mount it
/// (`http`: Claude Code's advertised `mcpCapabilities.http`; Codex's configuration speaks
/// Streamable HTTP) and it is offered, else the stdio bridge when offered.
///
/// # Errors
/// No offered endpoint is one the agent can mount: no conversation opens, no fallback.
pub(crate) fn choose(http: bool, offer: &ToolOffer) -> Result<Transport, HarnessError> {
    match (http && offer.http.is_some(), offer.stdio.is_some()) {
        (true, _) => Ok(Transport::Http),
        (false, true) => Ok(Transport::Stdio),
        (false, false) if offer.http.is_some() => Err(refusal(
            "ACP conversation refused: the agent does not advertise HTTP MCP \
             (`mcpCapabilities.http`) and Nika serves its session tools over HTTP only (no \
             stdio bridge is offered: `nika mcp --session` does not exist yet); no conversation \
             was opened; no fallback",
        )),
        (false, false) => Err(refusal(
            "ACP conversation refused: no Nika tool server was offered; no conversation was \
             opened; no fallback",
        )),
    }
}

/// Whether Claude Code's `initialize` answer advertises HTTP MCP servers.
pub(crate) fn advertises_http(init: &Value) -> bool {
    init.pointer("/agentCapabilities/mcpCapabilities/http")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The `mcpServers` of `session/new`: exactly Nika's server over `transport` for Claude Code;
/// none for Codex, which mounts it from its configuration.
pub(crate) fn mounts(offer: &ToolOffer, transport: Transport, profile: Profile) -> Vec<Value> {
    if profile == Profile::Codex {
        return Vec::new();
    }
    let mount = match (transport, &offer.http, &offer.stdio) {
        (Transport::Http, Some(http), _) => json!({"type": "http", "name": offer.name,
            "url": http.url,
            "headers": [{"name": "Authorization", "value": format!("Bearer {}", http.bearer)}]}),
        (Transport::Stdio, _, Some(stdio)) => json!({"name": offer.name,
            "command": stdio.command, "args": stdio.args,
            "env": (stdio.env.iter()).map(|(name, value)| json!({"name": name, "value": value}))
                .collect::<Vec<_>>()}),
        _ => return Vec::new(),
    };
    vec![mount]
}

/// The server Codex's configuration mounts for `offer` over `transport` (`mcp_servers.<name>`),
/// and the environment it reads: the bearer rides an environment variable the configuration
/// names, never the configuration or a command line.
pub(crate) fn codex_mount(
    offer: &ToolOffer,
    transport: Transport,
) -> (Value, Vec<(String, String)>) {
    match (transport, &offer.http, &offer.stdio) {
        (Transport::Http, Some(http), _) => (
            json!({"url": http.url, "bearer_token_env_var": codex::BEARER_ENV}),
            vec![(codex::BEARER_ENV.to_owned(), http.bearer.clone())],
        ),
        (_, _, Some(stdio)) => {
            let env: serde_json::Map<String, Value> = (stdio.env.iter())
                .map(|(name, value)| (name.clone(), Value::String(value.clone())))
                .collect();
            let server = json!({"command": stdio.command, "args": stdio.args, "env": env});
            (server, Vec::new())
        }
        _ => (Value::Null, Vec::new()),
    }
}

/// The tool a tool call names, as the agent named it: Claude Code's `_meta.claudeCode.toolName`,
/// else the call's own `name`, when it is a plain word.
pub(crate) fn asked_tool(tool_call: &Value) -> Option<String> {
    let named = (tool_call.pointer("/_meta/claudeCode/toolName"))
        .or_else(|| tool_call.get("name"))
        .and_then(Value::as_str)?;
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
    (!named.is_empty() && named.len() <= 128 && named.chars().all(plain)).then(|| named.to_owned())
}

/// The offered tool a tool call names on Nika's server (`mcp__<name>__<tool>`, the server
/// Claude Code names beside it agreeing), or `None`: every other call is someone else's.
pub(crate) fn nika_tool(tool_call: &Value, offer: &ToolOffer) -> Option<String> {
    let server = tool_call.pointer("/_meta/claudeCode/mcpServer/name");
    if server.is_some_and(|server| server.as_str() != Some(offer.name.as_str())) {
        return None;
    }
    let asked = asked_tool(tool_call)?;
    let tool = (asked.strip_prefix("mcp__"))
        .and_then(|rest| rest.strip_prefix(offer.name.as_str()))
        .and_then(|rest| rest.strip_prefix("__"))?;
    offer.tools.iter().find(|served| *served == tool).cloned()
}

/// The answer to a permission: `allow_once` when allowed, else `reject_once`, so the agent
/// goes on without the tool; `cancelled` when the option is not offered (fail-closed, never an
/// `allow_always` nor a remembered rejection).
pub(crate) fn answer(allowed: bool, options: &[PermissionOptionIn]) -> PermissionResult {
    let kind = if allowed { "allow_once" } else { "reject_once" };
    (options.iter().find(|option| option.kind == kind))
        .map_or_else(PermissionResult::cancelled, |option| {
            PermissionResult::selected(&option.option_id)
        })
}

/// The opening record: closed facts, no endpoint, bearer or adapter text.
pub(crate) fn opening(
    init: &Value,
    offer: &ToolOffer,
    transport: Transport,
    profile: Profile,
    selection: (&HarnessSelection, Option<&str>),
) -> Value {
    let advertised = (profile == Profile::ClaudeCode).then(|| {
        let flag = |kind: &str| {
            let pointer = format!("/agentCapabilities/mcpCapabilities/{kind}");
            init.pointer(&pointer).and_then(Value::as_bool) == Some(true)
        };
        json!({"http": flag("http"), "sse": flag("sse")})
    });
    let mounted_by = match profile {
        Profile::ClaudeCode => "session/new",
        Profile::Codex => "codex configuration, read back at spawn",
    };
    let offered: Vec<&str> = [
        ("http", offer.http.is_some()),
        ("stdio", offer.stdio.is_some()),
    ]
    .into_iter()
    .filter_map(|(word, offered)| offered.then_some(word))
    .collect();
    let load = init
        .pointer("/agentCapabilities/loadSession")
        .and_then(Value::as_bool);
    let (selection, model) = selection;
    json!({"profile": "conversation", "adapter_version": profile.version(),
        "transport": {"chosen": transport.as_str(), "mounted_by": mounted_by,
            "advertised": advertised, "offered": offered},
        "server": {"name": offer.name, "tools": offer.tools.len()},
        "load_session": load,
        "configured_model": model, "effort_option": selection.effort_option,
        "transmitted_effort": selection.transmitted_effort,
        "configured_effort": selection.configured_effort})
}

#[cfg(test)]
mod tests;
