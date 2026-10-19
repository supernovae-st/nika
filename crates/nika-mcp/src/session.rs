// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The stdio **session** — one confined MCP server process on one
//! persistent bidirectional pipe: spawn (through the OS sandbox · scrubbed
//! environment) · handshake (`initialize` · `notifications/initialized`) ·
//! bounded request/reply · SIGKILL on drop (INV-011).
//!
//! Two seams ride on it. [`McpConnectDyn`] turns a registry entry into a
//! live [`McpSessionDyn`]; the pin flow ([`crate::client`]) and the runtime
//! dispatch ([`crate::dispatch`]) are generic over both, so tests drive them
//! with scripted sessions and no subprocess. The one production pair is
//! [`StdioConnector`] → [`StdioSession`].
//!
//! It is the deliberate second subprocess-spawn site in the engine (the
//! first is `nika-exec-runner`, the shell effect): an MCP stdio session is a
//! persistent pipe, a shape the one-shot `ShellRunDyn` seam cannot express
//! — and the async process seam is unavailable to this crate by dependency
//! law (tokio is not on `nika-mcp`'s wrapper list), so std threads drain the
//! child's pipes and every reply wait carries a timeout.
//!
//! The child's stderr is NOT discarded: a bounded tail rides every transport
//! failure, because a confined launcher that dies before the handshake
//! (`npx -y …` resolving outside the project tree · a missing binary behind
//! the sandbox wrapper) used to surface as a bare « closed the pipe » with
//! the cause invisible (#1376).

use std::collections::{BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
// The `std::process` / `std::thread` exemption below is deliberate and
// scoped: see the module doc — a persistent pipe, tokio unavailable here.
#[allow(clippy::disallowed_types)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use nika_exec_runner::{EgressProxy, proxy_env, stderr_journal_for};
use nika_kernel::CancelCtx;
use nika_kernel::command_sandbox::CommandSandbox;
use nika_kernel::process::{NetPolicy, ShellCommand};
use nika_kernel::tool_executor::StopOutcome;
use serde_json::{Value, json};

use crate::client::McpServerConfig;
use crate::pin::{McpToolDef, PinError};

/// The protocol revision this client requests (newest broadly-deployed —
/// the server's own negotiation echoes a supported choice).
const CLIENT_PROTOCOL_VERSION: &str = "2025-11-25";

/// The default per-reply ceiling for the handshake and `tools/list` — an
/// unresponsive server must never hang an operator command forever
/// (kill-on-drop still bounds the child).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// The default `tools/call` ceiling — a tool does real work (a query · a
/// fetch · a render), so its reply window is wider than the handshake's.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// How much of the child's stderr the session keeps (the tail) for the
/// transport error — enough for a launcher's last complaint, never a log.
const STDERR_TAIL_BYTES: usize = 4096;

/// How long a reply wait that can be stopped stays idle before it looks at
/// its stop again: a std channel cannot wake on the token's flag.
const STOP_POLL: Duration = Duration::from_millis(50);

/// How many stopped calls a session remembers, so that their late replies
/// are dropped without counting against a later wait's byte budget.
const ABANDONED_KEPT: usize = 64;

/// What one `tools/call` returned, mapped off the wire: the text view the
/// model reads, the typed value when the server sent `structuredContent`,
/// and the MCP `isError` flag (a TOOL failure is a successful reply — the
/// model sees it and adapts).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallOutcome {
    /// The text content blocks joined by newlines (a non-text block leaves
    /// a `[<type> content]` marker so nothing vanishes silently).
    pub content: String,
    /// The `structuredContent` value verbatim, when present.
    pub structured: Option<Value>,
    /// The server's `isError` flag.
    pub is_error: bool,
    /// Set when the run stopped waiting for the call: whether it was sent.
    pub stopped: Option<StopOutcome>,
}

impl CallOutcome {
    /// Build an outcome (INV-019 · the `#[non_exhaustive]` constructor).
    #[must_use]
    pub fn new(content: impl Into<String>, structured: Option<Value>, is_error: bool) -> Self {
        Self {
            content: content.into(),
            structured,
            is_error,
            stopped: None,
        }
    }

    /// A call the run stopped waiting for: a failed outcome saying `content`
    /// and whether the call was sent.
    #[must_use]
    pub fn stopped(content: impl Into<String>, outcome: StopOutcome) -> Self {
        Self {
            stopped: Some(outcome),
            ..Self::new(content, None, true)
        }
    }

    /// Map a `tools/call` RESULT payload onto the outcome. A result whose
    /// `resultType` is not `complete` (an `input_required` or a `task`, the
    /// 2026-07-28 shapes) asks for a continuation this client never declared
    /// it could give: a failed call, never an empty success. An
    /// absent `resultType` is a complete result, as every earlier revision
    /// writes it.
    #[must_use]
    pub fn from_result(result: &Value) -> Self {
        match result.get("resultType") {
            None => {}
            Some(Value::String(kind)) if kind == "complete" => {}
            Some(kind) => {
                return Self::new(
                    format!(
                        "the server answered `tools/call` with resultType {}, which this client \
                         cannot continue (it declared no capability for it): the call did not \
                         complete",
                        bounded(kind)
                    ),
                    None,
                    true,
                );
            }
        }
        let mut lines = Vec::new();
        if let Some(blocks) = result.get("content").and_then(Value::as_array) {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => lines.push(
                        block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    ),
                    Some(other) => lines.push(format!("[{other} content]")),
                    None => lines.push("[untyped content]".to_owned()),
                }
            }
        }
        Self {
            content: lines.join("\n"),
            structured: result.get("structuredContent").cloned(),
            is_error: result
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            stopped: None,
        }
    }
}

/// The session seam — one live server pipe. Synchronous by dependency law;
/// the runtime dispatch offloads a call to a worker thread.
pub trait McpSessionDyn: Send {
    /// One `tools/list` (the pin layer vets the answer).
    ///
    /// # Errors
    ///
    /// [`PinError::Transport`] when the pipe dies or times out ·
    /// [`PinError::Malformed`] when the payload is not vettable.
    fn tools_list(&mut self) -> Result<Vec<McpToolDef>, PinError>;

    /// One `tools/call` of `tool` over `arguments`. A JSON-RPC error reply
    /// is a TOOL failure (`is_error`), never a transport failure.
    ///
    /// # Errors
    ///
    /// [`PinError::Transport`] when the pipe dies or times out — the caller
    /// drops the session so the next call reconnects.
    fn tools_call(&mut self, tool: &str, arguments: &Value) -> Result<CallOutcome, PinError>;

    /// [`Self::tools_call`], watching `stop`. A session that can stop a
    /// call writes it only through [`CancelCtx::begin`], so a call stopped
    /// before it was written is never sent; once the run stops waiting for
    /// a call in flight, it tells the server to cancel it and answers a
    /// stopped outcome ([`CallOutcome::stopped`]), the pipe kept for the next
    /// call. This default cannot stop a call in flight: it makes the call to
    /// its end.
    ///
    /// # Errors
    ///
    /// As [`Self::tools_call`].
    fn tools_call_until(
        &mut self,
        tool: &str,
        arguments: &Value,
        _stop: &CancelCtx,
    ) -> Result<CallOutcome, PinError> {
        self.tools_call(tool, arguments)
    }

    /// `Some(note)` when the server runs WITHOUT OS confinement (the
    /// deliberate `noop` backend) — the one receipt a run must say out
    /// loud; a confined server answers `None`. Never printed by this crate:
    /// a worker thread writing `stderr` under a caller's own lock deadlocks
    /// (measured 2026-09-13 · the CLI run holds it).
    fn unconfined_note(&self) -> Option<String> {
        None
    }
}

/// The connector seam — how a registry entry becomes a session.
pub trait McpConnectDyn: Send + Sync {
    /// Spawn + handshake one server.
    ///
    /// # Errors
    ///
    /// [`PinError::Transport`] · [`PinError::Sandbox`] ·
    /// [`PinError::Unsupported`] — the spawn-side refusals.
    fn connect(&self, config: &McpServerConfig) -> Result<Box<dyn McpSessionDyn>, PinError>;
}

/// The production connector: every server spawns CONFINED by the platform
/// sandbox ([`crate::sandbox::platform_sandbox`]) anchored at the project
/// dir (the `.nika/` convention's root), with the scrubbed environment.
pub struct StdioConnector {
    /// The OS-confinement backend — ALWAYS present: there is no unsandboxed
    /// construction, so the unconfined fallback cannot exist as an accident
    /// (the deliberate `NoopSandbox` case is named loudly by the note).
    sandbox: Arc<dyn CommandSandbox>,
    project_dir: PathBuf,
    timeout: Duration,
    call_timeout: Duration,
}

impl StdioConnector {
    /// A connector anchored at `project_dir`, under the platform sandbox.
    #[must_use]
    pub fn new(project_dir: impl Into<PathBuf>) -> Self {
        Self {
            sandbox: crate::sandbox::platform_sandbox(),
            project_dir: project_dir.into(),
            timeout: DEFAULT_TIMEOUT,
            call_timeout: DEFAULT_CALL_TIMEOUT,
        }
    }

    /// Override the confinement backend (tests · a wiring layer carrying
    /// its own `CommandSandbox`).
    #[must_use]
    pub fn with_sandbox(mut self, sandbox: Arc<dyn CommandSandbox>) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Override the handshake / `tools/list` reply ceiling.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the `tools/call` reply ceiling.
    #[must_use]
    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    /// The one-line sandbox mode note — `sandboxed (seatbelt · net deny)`
    /// style — for `config`'s network arm (see [`crate::sandbox`]).
    #[must_use]
    pub fn sandbox_note(&self, config: &McpServerConfig) -> String {
        crate::sandbox::sandbox_note(self.sandbox.backend(), &config.network)
    }
}

impl McpConnectDyn for StdioConnector {
    fn connect(&self, config: &McpServerConfig) -> Result<Box<dyn McpSessionDyn>, PinError> {
        let session = StdioSession::open(
            config,
            &self.sandbox,
            &self.project_dir,
            self.timeout,
            self.call_timeout,
        )?;
        Ok(Box::new(session))
    }
}

/// What the stdout reader thread yields per line (or why it stopped).
enum Line {
    Text(String),
    Failed(String),
    Eof,
}

/// Why a reply wait ended without its reply.
enum Unanswered {
    /// The pipe failed: unreadable, closed, or too many bytes of other
    /// messages.
    Failed(PinError),
    /// The deadline (this long) passed: the server was told to cancel.
    TimedOut(Duration),
    /// The run stopped waiting: the server was told to cancel, and the pipe
    /// is still healthy.
    Stopped,
    /// The run stopped waiting before the request was written: nothing was
    /// sent.
    NotSent,
}

/// The INV-011 guard for a std child (std's `Command` has no
/// `kill_on_drop` — that is a tokio method): dropping the session SIGKILLs
/// the server and reaps it, so a failed handshake, a drift refusal or a
/// finished run never leaks a running subprocess. The server's loopback
/// egress proxy, when its `network` is an allowlist, rides along and
/// stops only once the child is killed and reaped.
#[allow(clippy::disallowed_types)] // see the import-site exemption note
struct KillOnDrop(Child, Option<EgressProxy>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill(); // SIGKILL · idempotent on an already-dead child
        let _ = self.0.wait(); // reap the zombie
        drop(self.1.take()); // then its egress proxy stops listening
    }
}

/// The bounded stderr tail — shared with the drain thread.
type StderrTail = Arc<Mutex<VecDeque<u8>>>;

/// One live, confined server: the pipe, the reader, the id counter.
pub struct StdioSession {
    server: String,
    child: KillOnDrop,
    rx: mpsc::Receiver<Line>,
    stderr: StderrTail,
    /// The confinement receipt (`sandboxed (seatbelt · net deny)`) — named
    /// in every transport failure so a sandbox kill is never a mystery.
    confinement: String,
    /// The receipt again, ONLY when the backend is the deliberate `noop`
    /// (the server runs unconfined) — what a run must say out loud.
    unconfined: Option<String>,
    next_id: u64,
    /// The ids of the calls the run stopped waiting for, whose late replies
    /// may still come (at most [`ABANDONED_KEPT`]).
    abandoned: BTreeSet<u64>,
    timeout: Duration,
    call_timeout: Duration,
}

impl StdioSession {
    /// Spawn `config` under `sandbox` anchored at `project_dir`, then
    /// handshake. A confine refusal starts NO process ([`PinError::Sandbox`]);
    /// a handshake failure carries the child's stderr tail and names the
    /// confinement ([`PinError::Transport`]).
    ///
    /// # Errors
    ///
    /// [`PinError::Unsupported`] for a `url` entry · [`PinError::Sandbox`] ·
    /// [`PinError::Transport`].
    pub fn open(
        config: &McpServerConfig,
        sandbox: &Arc<dyn CommandSandbox>,
        project_dir: &Path,
        timeout: Duration,
        call_timeout: Duration,
    ) -> Result<Self, PinError> {
        let confinement = crate::sandbox::sandbox_note(sandbox.backend(), &config.network);
        let unconfined = (sandbox.backend() == "noop").then(|| confinement.clone());
        let (child, rx, stderr) = spawn(config, sandbox, project_dir)?;
        let mut session = Self {
            server: config.name.clone(),
            child,
            rx,
            stderr,
            confinement,
            unconfined,
            next_id: 0,
            abandoned: BTreeSet::new(),
            timeout,
            call_timeout,
        };
        session
            .handshake()
            .map_err(|err| session.explain_death(err))?;
        Ok(session)
    }

    /// `initialize` → `notifications/initialized` (the MCP lifecycle MUST).
    fn handshake(&mut self) -> Result<(), PinError> {
        let params = json!({
            "protocolVersion": CLIENT_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "nika", "version": env!("CARGO_PKG_VERSION") },
        });
        self.request("initialize", &params, self.timeout, None)
            .map_err(|why| self.unanswered(why))?
            .map_err(|error| self.transport(format!("the server refused `initialize`: {error}")))?;
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
    }

    /// A transport failure after a successful spawn: attach the child's
    /// stderr tail and name the confinement — the launcher-died-under-the-
    /// sandbox class (#1376) reads its own cause instead of a bare EOF.
    fn explain_death(&self, err: PinError) -> PinError {
        let PinError::Transport { server, why } = err else {
            return err;
        };
        let mut why = format!("{why}\n  the server ran {}", self.confinement);
        match self.stderr_tail() {
            Some(tail) => {
                why.push_str("\n  its stderr said: ");
                why.push_str(&tail);
            }
            None => why.push_str(" · its stderr was silent"),
        }
        why.push_str(
            "\n  a launcher that resolves or installs outside the project tree (`npx -y …` · a global \
             cache) dies confined: install the server inside the project and point `command` \
             at what the tree contains",
        );
        PinError::Transport { server, why }
    }

    /// The child's stderr tail so far (lossy UTF-8 · whitespace-trimmed ·
    /// newlines folded), or `None` when it wrote nothing.
    fn stderr_tail(&self) -> Option<String> {
        let mut buf = self.stderr.lock().unwrap_or_else(PoisonError::into_inner);
        let text = String::from_utf8_lossy(buf.make_contiguous()).into_owned();
        let folded: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        (!folded.is_empty()).then(|| folded.join(" ⏎ "))
    }

    fn transport(&self, why: String) -> PinError {
        PinError::Transport {
            server: self.server.clone(),
            why,
        }
    }

    /// Write one JSON-RPC message (one compact line · flushed).
    fn send(&mut self, msg: &Value) -> Result<(), PinError> {
        let line = serde_json::to_string(msg).unwrap_or_default();
        let Some(stdin) = self.child.0.stdin.as_mut() else {
            return Err(self.transport("the child's stdin was not piped".to_owned()));
        };
        if let Err(e) = writeln!(stdin, "{line}").and_then(|()| stdin.flush()) {
            return Err(self.transport(format!("cannot write to the server: {e}")));
        }
        Ok(())
    }

    /// Wait for the reply carrying `want_id`, skipping notifications, the
    /// server's own requests and stray ids. Bounded by ONE deadline for the
    /// whole wait and by the bytes it skips (the line ceiling, the budget one
    /// reply has), never by a count of messages: a server that logs forty
    /// lines before answering is answered. The late reply to a call the run
    /// stopped waiting for is dropped without counting against that budget.
    /// A received reply wins over a stop: the stop is honoured only once the
    /// pipe stayed idle for a [`STOP_POLL`].
    fn await_reply(
        &mut self,
        want_id: u64,
        timeout: Duration,
        stop: Option<&CancelCtx>,
    ) -> Result<Value, Unanswered> {
        // An unrepresentable deadline waits without one, as `recv_timeout`
        // itself does.
        let deadline = Instant::now().checked_add(timeout);
        let mut skipped: u64 = 0;
        loop {
            let left = deadline.map_or(Duration::MAX, |at| {
                at.saturating_duration_since(Instant::now())
            });
            let wait = if stop.is_some() {
                left.min(STOP_POLL)
            } else {
                left
            };
            match self.rx.recv_timeout(wait) {
                Ok(Line::Text(text)) => {
                    let msg: Value = serde_json::from_str(&text).map_err(|e| {
                        Unanswered::Failed(self.transport(format!("a reply is not JSON: {e}")))
                    })?;
                    // A reply carries no `method`: a server's own request (a
                    // `ping`) numbers its id in its own space.
                    let reply_id = msg
                        .get("method")
                        .is_none()
                        .then(|| msg.get("id").and_then(Value::as_u64))
                        .flatten();
                    if reply_id == Some(want_id) {
                        return Ok(msg);
                    }
                    if reply_id.is_some_and(|id| self.abandoned.remove(&id)) {
                        continue;
                    }
                    skipped = skipped.saturating_add(u64::try_from(text.len()).unwrap_or(u64::MAX));
                    if skipped > crate::MAX_MSG_BYTES {
                        return Err(Unanswered::Failed(self.transport(format!(
                            "the server sent {skipped} bytes of other messages without answering \
                             the request"
                        ))));
                    }
                }
                Ok(Line::Failed(why)) => return Err(Unanswered::Failed(self.transport(why))),
                Ok(Line::Eof) => {
                    return Err(Unanswered::Failed(self.transport(
                        "the server closed the pipe before answering".to_owned(),
                    )));
                }
                // Nothing arrived for a whole slice: only now may a stop win.
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if stop.is_some_and(CancelCtx::is_cancelled) {
                        return Err(Unanswered::Stopped);
                    }
                    if wait >= left {
                        return Err(Unanswered::TimedOut(timeout));
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Unanswered::Failed(self.transport(
                        "the server's reader stopped before an answer".to_owned(),
                    )));
                }
            }
        }
    }

    /// One request → `Ok(result)` or `Err(error)` (the JSON-RPC error
    /// object), or why it went unanswered. With a `stop`, the request is
    /// written only through [`CancelCtx::begin`]: stopped before, it is never
    /// sent. A request that times out or that the run stopped waiting for is
    /// cancelled (`notifications/cancelled` with its id, the spec's
    /// cancellation), so the server stops acting on it; a stopped request
    /// whose cancel cannot be written leaves a dead pipe, a transport
    /// failure. `initialize` is never cancelled, as the spec requires.
    fn request(
        &mut self,
        method: &str,
        params: &Value,
        timeout: Duration,
        stop: Option<&CancelCtx>,
    ) -> Result<Result<Value, Value>, Unanswered> {
        if stop.is_some_and(|stop| !stop.begin()) {
            return Err(Unanswered::NotSent);
        }
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .map_err(Unanswered::Failed)?;
        let reply = match self.await_reply(id, timeout, stop) {
            Ok(reply) => reply,
            Err(why @ (Unanswered::Failed(_) | Unanswered::NotSent)) => return Err(why),
            Err(why) => {
                if method != "initialize" {
                    let stopped = matches!(why, Unanswered::Stopped);
                    let reason = if stopped {
                        "the run stopped waiting for it".to_owned()
                    } else {
                        format!("no reply within {}s", timeout.as_secs())
                    };
                    let cancel = self.send(&json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/cancelled",
                        "params": { "requestId": id, "reason": reason },
                    }));
                    if stopped {
                        // The session outlives a stop only when the server
                        // was told; a timed-out session closes anyway.
                        cancel.map_err(Unanswered::Failed)?;
                        self.abandon(id);
                    }
                }
                return Err(why);
            }
        };
        if let Some(error) = reply.get("error") {
            return Ok(Err(error.clone()));
        }
        Ok(Ok(reply.get("result").cloned().unwrap_or(Value::Null)))
    }

    /// Remember a call the run stopped waiting for, so its late reply is
    /// dropped without counting against a later wait; the oldest is
    /// forgotten past [`ABANDONED_KEPT`].
    fn abandon(&mut self, id: u64) {
        self.abandoned.insert(id);
        if self.abandoned.len() > ABANDONED_KEPT {
            let _ = self.abandoned.pop_first();
        }
    }

    /// A request that went unanswered, as the transport failure it is.
    fn unanswered(&self, why: Unanswered) -> PinError {
        match why {
            Unanswered::Failed(err) => err,
            Unanswered::TimedOut(timeout) => {
                self.transport(format!("no reply within {}s", timeout.as_secs()))
            }
            Unanswered::Stopped => self.transport("the run stopped waiting for a reply".to_owned()),
            Unanswered::NotSent => {
                self.transport("the run stopped waiting before the request was sent".to_owned())
            }
        }
    }

    /// One `tools/call`, watching `stop` when there is one.
    fn call_tool(
        &mut self,
        tool: &str,
        arguments: &Value,
        stop: Option<&CancelCtx>,
    ) -> Result<CallOutcome, PinError> {
        let params = json!({ "name": tool, "arguments": arguments });
        match self.request("tools/call", &params, self.call_timeout, stop) {
            Ok(Ok(result)) => Ok(CallOutcome::from_result(&result)),
            // A JSON-RPC error on tools/call is the TOOL's failure voice
            // (an unknown name · invalid params) — the model sees it.
            Ok(Err(error)) => Ok(CallOutcome::new(
                format!("the server refused `tools/call` for `{tool}`: {error}"),
                None,
                true,
            )),
            // The run stopped waiting: the server was told to cancel, and the
            // healthy pipe is kept for the next call.
            Err(Unanswered::Stopped) => Ok(CallOutcome::stopped(
                format!("the run stopped waiting for `{tool}`: the server was told to cancel it"),
                StopOutcome::Unknown,
            )),
            Err(Unanswered::NotSent) => Ok(CallOutcome::stopped(
                format!("the run stopped waiting for `{tool}` before it was sent"),
                StopOutcome::NotSent,
            )),
            Err(why) => Err(self.explain_death(self.unanswered(why))),
        }
    }
}

impl McpSessionDyn for StdioSession {
    fn tools_list(&mut self) -> Result<Vec<McpToolDef>, PinError> {
        let result = self
            .request("tools/list", &json!({}), self.timeout, None)
            .map_err(|why| self.explain_death(self.unanswered(why)))?
            .map_err(|error| self.transport(format!("the server refused `tools/list`: {error}")))?;
        McpToolDef::from_list_value(
            &self.server,
            &result.get("tools").cloned().unwrap_or(Value::Null),
        )
    }

    fn tools_call(&mut self, tool: &str, arguments: &Value) -> Result<CallOutcome, PinError> {
        self.call_tool(tool, arguments, None)
    }

    fn tools_call_until(
        &mut self,
        tool: &str,
        arguments: &Value,
        stop: &CancelCtx,
    ) -> Result<CallOutcome, PinError> {
        self.call_tool(tool, arguments, Some(stop))
    }

    fn unconfined_note(&self) -> Option<String> {
        self.unconfined.clone()
    }
}

/// Spawn the child + the two drain threads — through the OS sandbox FIRST
/// (fail-closed): the configured command is confined to the derived
/// boundary ([`McpServerConfig::sandbox_spec`]) and a confine refusal is
/// [`PinError::Sandbox`] with NO process started (never a silent unconfined
/// fallback).
#[allow(clippy::disallowed_types, clippy::disallowed_methods)] // import-site note: persistent pipe · tokio unavailable here
fn spawn(
    config: &McpServerConfig,
    sandbox: &Arc<dyn CommandSandbox>,
    project_dir: &Path,
) -> Result<(KillOnDrop, mpsc::Receiver<Line>, StderrTail), PinError> {
    let transport = |why: String| PinError::Transport {
        server: config.name.clone(),
        why,
    };
    if let Some(url) = &config.url {
        return Err(PinError::Unsupported {
            server: config.name.clone(),
            why: format!(
                "remote MCP transport ({url}) is not wired yet — only stdio (command/args) servers can be reached today"
            ),
        });
    }
    let command = config
        .command
        .as_deref()
        .ok_or_else(|| transport("no `command` configured".to_owned()))?;
    // OS confinement (ADR-095 Layer 6): the SAME seam the exec runner
    // uses — the confine transform is pure; a refusal is terminal.
    let mut inner = ShellCommand::new(command);
    inner.args.clone_from(&config.args);
    inner.cwd = Some(project_dir.to_path_buf());
    let mut spec = config.sandbox_spec(project_dir);
    let egress = start_egress(config, &mut spec.net)?;
    let confined = sandbox
        .confine(&spec, inner)
        .map_err(|e| PinError::Sandbox {
            server: config.name.clone(),
            why: e.to_string(),
        })?;
    let mut cmd = Command::new(&confined.program);
    cmd.args(&confined.args);
    apply_env_scrub(&mut cmd);
    if let Some(proxy) = &egress {
        // After the scrub: the boundary's env contract outranks any
        // ambient proxy value, and the child's only egress is the proxy.
        for (name, value) in proxy_env(proxy.port()) {
            cmd.env(name, value);
        }
    }
    if let Some(cwd) = &confined.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| transport(format!("cannot spawn `{command}`: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| transport("the child's stdout was not piped".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| transport("the child's stderr was not piped".to_owned()))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || pump_lines(stdout, &tx));
    let tail: StderrTail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_BYTES)));
    let sink = Arc::clone(&tail);
    std::thread::spawn(move || drain_tail(stderr, &sink));
    Ok((KillOnDrop(child, egress), rx, tail))
}

/// The network allowlist's enforcement half (ADR-095 Layer 6 · the exec
/// arm's contract): a loopback egress proxy serves exactly the declared
/// hosts, and its port scopes the jail's outbound fence. Without it the
/// macOS profile admits loopback only and the server cannot even resolve
/// a name. Any other `network` value starts nothing. A proxy that cannot
/// start refuses the spawn — never an allowlisted server without its
/// proxy.
fn start_egress(
    config: &McpServerConfig,
    net: &mut NetPolicy,
) -> Result<Option<EgressProxy>, PinError> {
    let NetPolicy::Allowlist(allowlist) = net else {
        return Ok(None);
    };
    // A refused host is the security event the exec arm journals too: one
    // queued stderr line that names this server's allowlist (the server
    // itself sees a refused CONNECT `403` or SOCKS reply).
    let journal = stderr_journal_for(format!(
        "the `{}` MCP server's network allowlist",
        config.name
    ));
    let proxy =
        EgressProxy::start(allowlist.hosts.clone(), journal).map_err(|e| PinError::Sandbox {
            server: config.name.clone(),
            why: format!("the loopback egress proxy for the network allowlist did not start: {e}"),
        })?;
    allowlist.proxy_port = Some(proxy.port());
    Ok(Some(proxy))
}

/// Scrub the child's environment: `env_clear` drops EVERY ambient value the
/// engine holds (provider API keys, session tokens, the whole
/// env-var-injection class), then ONLY the curated names a server
/// legitimately needs are re-admitted — the runner floor
/// ([`nika_kernel::process::RUNNER_FLOOR_ENV_VARS`]) minus the
/// [`DANGEROUS_ENV_VARS`](nika_kernel::process::DANGEROUS_ENV_VARS)
/// floor, which wins even over those. MCP config values reach the server
/// via argv, never via ambient env inheritance.
#[allow(clippy::disallowed_types, clippy::disallowed_methods)] // import-site exemption note · reading the operator's ambient env to re-admit a curated subset to the child is the spawn site's duty, not a secret lookup
fn apply_env_scrub(cmd: &mut Command) {
    // ONE composition, shared with the exec runner (`compose_child_env`) —
    // the MCP stdio child gets the STRICTEST call: no passthrough grants,
    // no authored map, so only the floor minus the dangerous names
    // survives, which is what this crate's own module doc promises.
    let env = nika_kernel::process::compose_child_env(
        |name| std::env::var(name).ok(),
        &[],
        &std::collections::BTreeMap::new(),
    );
    cmd.env_clear();
    for (name, value) in env {
        cmd.env(name, value);
    }
}

/// Drain the child's stdout into the channel, one bounded line at a time
/// (the same 8 MiB ceiling as the server pump — a runaway line is a
/// transport failure, never an unbounded allocation). The sender is owned
/// by the reader thread and borrowed here so a disconnected client simply
/// ends the loop.
fn pump_lines(stdout: impl Read, tx: &mpsc::Sender<Line>) {
    let mut reader = BufReader::new(stdout);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = match (&mut reader)
            .take(crate::MAX_MSG_BYTES + 1)
            .read_until(b'\n', &mut buf)
        {
            Ok(0) => {
                let _ = tx.send(Line::Eof);
                return;
            }
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(Line::Failed(format!("cannot read the server: {e}")));
                return;
            }
        };
        if buf.last() != Some(&b'\n') && n as u64 > crate::MAX_MSG_BYTES {
            let _ = tx.send(Line::Failed(format!(
                "a reply exceeds the {}-byte line ceiling",
                crate::MAX_MSG_BYTES
            )));
            return;
        }
        while matches!(buf.last(), Some(b'\n' | b'\r')) {
            buf.pop();
        }
        match String::from_utf8(buf.clone()) {
            Ok(text) => {
                if tx.send(Line::Text(text)).is_err() {
                    return; // the client went away — stop quietly
                }
            }
            Err(e) => {
                let _ = tx.send(Line::Failed(format!("a reply is not UTF-8: {e}")));
                return;
            }
        }
    }
}

/// A server-chosen value quoted into a message: JSON-encoded and bounded, so
/// it can neither pose as our own words nor flood the model's context.
fn bounded(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 64 {
        format!("{}…", text.chars().take(64).collect::<String>())
    } else {
        text
    }
}

/// Drain the child's stderr into the bounded tail (the last
/// [`STDERR_TAIL_BYTES`] survive · older bytes fall off the front). Ends on
/// EOF or a read failure — the child dying is exactly when the tail is read.
fn drain_tail(stderr: impl Read, tail: &StderrTail) {
    let mut reader = stderr;
    let mut chunk = [0u8; 512];
    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let mut buf = tail.lock().unwrap_or_else(PoisonError::into_inner);
        buf.extend(&chunk[..n]);
        while buf.len() > STDERR_TAIL_BYTES {
            buf.pop_front();
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_call_result_maps_text_blocks_structured_and_is_error() {
        let result = json!({
            "content": [
                {"type": "text", "text": "line one"},
                {"type": "image", "data": "…", "mimeType": "image/png"},
                {"type": "text", "text": "line two"},
                {"data": "?"}
            ],
            "structuredContent": {"who": "x"},
            "isError": false
        });
        let out = CallOutcome::from_result(&result);
        assert_eq!(
            out.content,
            "line one\n[image content]\nline two\n[untyped content]"
        );
        assert_eq!(out.structured, Some(json!({"who": "x"})));
        assert!(!out.is_error);
    }

    #[test]
    fn a_bare_error_result_is_a_tool_error_with_no_structured_value() {
        let result = json!({"content": [{"type": "text", "text": "boom"}], "isError": true});
        let out = CallOutcome::from_result(&result);
        assert_eq!(out.content, "boom");
        assert!(out.structured.is_none());
        assert!(out.is_error);
        // An empty / shapeless result is an empty success, never a panic.
        let empty = CallOutcome::from_result(&Value::Null);
        assert_eq!(empty, CallOutcome::new("", None, false));
    }

    #[test]
    fn the_stderr_tail_is_bounded_and_keeps_the_end() {
        let tail: StderrTail = Arc::new(Mutex::new(VecDeque::new()));
        let mut noise = vec![b'a'; STDERR_TAIL_BYTES * 3];
        noise.extend_from_slice(b"\nnpm ERR! last words\n");
        drain_tail(noise.as_slice(), &tail);
        let mut buf = tail.lock().unwrap();
        assert_eq!(
            buf.len(),
            STDERR_TAIL_BYTES,
            "the ring holds exactly the cap"
        );
        let text = String::from_utf8_lossy(buf.make_contiguous()).into_owned();
        assert!(text.ends_with("npm ERR! last words\n"), "{text}");
    }

    /// A result that is not `complete` asks for a continuation
    /// this client never declared it could give: a failed call, never an
    /// empty success. A `complete` result, or one with no `resultType` (every
    /// earlier revision), stays a success.
    #[test]
    fn a_result_that_is_not_complete_is_a_failed_call() {
        for result in [
            json!({"resultType": "input_required", "requestState": "x"}),
            json!({"resultType": "task", "task": {"taskId": "t"}}),
            json!({"resultType": 7}),
        ] {
            let out = CallOutcome::from_result(&result);
            assert!(out.is_error, "{result}");
            assert!(out.content.contains("did not complete"), "{}", out.content);
            assert!(out.structured.is_none(), "{result}");
        }
        let text = json!([{"type": "text", "text": "ok"}]);
        let complete = json!({"resultType": "complete", "content": text});
        assert_eq!(
            CallOutcome::from_result(&complete),
            CallOutcome::new("ok", None, false)
        );
        assert_eq!(
            CallOutcome::from_result(&json!({"content": text})),
            CallOutcome::new("ok", None, false)
        );
    }

    /// A confinement that runs the command as given: the session's own
    /// mechanics are under test here, not the sandbox.
    struct AsGiven;

    impl CommandSandbox for AsGiven {
        fn confine(
            &self,
            _spec: &nika_kernel::process::SandboxSpec,
            command: ShellCommand,
        ) -> Result<ShellCommand, nika_kernel::command_sandbox::CommandSandboxError> {
            Ok(command)
        }

        fn backend(&self) -> &'static str {
            "as-given"
        }
    }

    /// The handshake every scripted server speaks before its own lines.
    const HANDSHAKE: &str = r#"#!/bin/sh
read -r _init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"scripted","version":"0"}}}'
read -r _initialized
"#;

    fn tmp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nika-mcp-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    /// A stdio server scripted in `dir`: the handshake, then `body` (`$1` is
    /// `dir`).
    fn scripted(dir: &Path, body: &str) -> McpServerConfig {
        let script = dir.join("scripted-mcp.sh");
        std::fs::write(&script, [HANDSHAKE, body].concat()).expect("write the scripted server");
        McpServerConfig::stdio(
            "scripted",
            "/bin/sh",
            vec![
                script.to_string_lossy().into_owned(),
                dir.to_string_lossy().into_owned(),
            ],
        )
    }

    fn open(config: &McpServerConfig, dir: &Path, call_timeout: Duration) -> StdioSession {
        let sandbox: Arc<dyn CommandSandbox> = Arc::new(AsGiven);
        StdioSession::open(config, &sandbox, dir, Duration::from_secs(10), call_timeout)
            .expect("the scripted server opens")
    }

    /// A server that logs forty lines before it answers is
    /// answered. The wait is bounded by one deadline and by the bytes it
    /// skips, never by a count of sixteen messages.
    #[cfg(unix)]
    #[test]
    fn forty_notifications_before_the_reply_are_skipped_not_fatal() {
        let dir = tmp("chatty");
        let config = scripted(
            &dir,
            r#"read -r _call
i=0
while [ "$i" -lt 40 ]; do
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"working"}}'
  i=$((i + 1))
done
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"done"}]}}'
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(10));
        let outcome = session
            .tools_call("work", &json!({}))
            .expect("the reply after forty notifications");
        assert_eq!(outcome, CallOutcome::new("done", None, false));
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A call that times out is cancelled before the pipe closes:
    /// the server reads `notifications/cancelled` carrying the call's id, so it
    /// can stop acting on it.
    #[cfg(unix)]
    #[test]
    fn a_timed_out_call_is_cancelled_before_the_pipe_closes() {
        let dir = tmp("cancelled");
        let config = scripted(
            &dir,
            r#"read -r _call
read -r cancel
printf '%s\n' "$cancel" > "$1/after-call.txt"
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(1));
        let refused = session
            .tools_call("slow", &json!({}))
            .expect_err("the call times out");
        assert!(
            refused.to_string().contains("no reply within 1s"),
            "{refused}"
        );
        let cancel = recorded(&dir.join("after-call.txt"));
        // Only now does the pipe close (the session kills its server).
        drop(session);
        assert_eq!(cancel["method"], "notifications/cancelled");
        assert_eq!(cancel["params"]["requestId"], 2);
        assert_eq!(cancel["params"]["reason"], "no reply within 1s");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The run stops waiting for a call in flight (its awaiting
    /// future was dropped). The session tells the server to cancel it and
    /// answers at once instead of holding the pipe to the 60 s call timeout,
    /// and the pipe stays open: the next call on the same server is
    /// answered.
    #[cfg(unix)]
    #[test]
    fn a_stopped_call_is_cancelled_and_the_pipe_kept() {
        let dir = tmp("stopped");
        let config = scripted(
            &dir,
            r#"read -r _call
read -r cancel
printf '%s\n' "$cancel" > "$1/after-call.txt"
read -r _next
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"next"}]}}'
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(60));
        let stop = CancelCtx::new();
        let started = Instant::now();
        let outcome = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                stop.cancel();
            });
            session.tools_call_until("slow", &json!({}), &stop)
        })
        .expect("a stopped call is an outcome, the pipe still healthy");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "answered at the stop, not at the call timeout"
        );
        assert!(outcome.is_error);
        assert_eq!(outcome.stopped, Some(StopOutcome::Unknown), "it was sent");
        assert!(
            outcome
                .content
                .contains("the run stopped waiting for `slow`"),
            "{}",
            outcome.content
        );
        let cancel = recorded(&dir.join("after-call.txt"));
        assert_eq!(cancel["method"], "notifications/cancelled");
        assert_eq!(cancel["params"]["requestId"], 2);
        assert_eq!(cancel["params"]["reason"], "the run stopped waiting for it");
        let next = session
            .tools_call("next", &json!({}))
            .expect("the same pipe answers the next call");
        assert_eq!(next, CallOutcome::new("next", None, false));
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The operator's Stop reaches a call in flight through the child
    /// context the run gave it: the server is told to cancel, the stop came
    /// from the run's context, and the pipe stays for the next call.
    #[cfg(unix)]
    #[test]
    fn an_operator_stop_cancels_an_inflight_mcp_call() {
        let dir = tmp("operator");
        let config = scripted(
            &dir,
            r#"read -r _call
read -r cancel
printf '%s\n' "$cancel" > "$1/after-call.txt"
read -r _next
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"next"}]}}'
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(60));
        let run = CancelCtx::new();
        let stop = run.child();
        let started = Instant::now();
        let outcome = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                run.cancel();
            });
            session.tools_call_until("slow", &json!({}), &stop)
        })
        .expect("a stopped call is an outcome, the pipe still healthy");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "answered at the operator's stop, not at the call timeout"
        );
        assert_eq!(outcome.stopped, Some(StopOutcome::Unknown));
        let cancel = recorded(&dir.join("after-call.txt"));
        assert_eq!(cancel["method"], "notifications/cancelled");
        assert_eq!(cancel["params"]["requestId"], 2);
        let next = session
            .tools_call("next", &json!({}))
            .expect("the same pipe answers the next call");
        assert_eq!(next, CallOutcome::new("next", None, false));
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A call the run stopped waiting for before it was written is never
    /// sent: the first line the server reads after the handshake is the next
    /// call, which takes the id the stopped one never used.
    #[cfg(unix)]
    #[test]
    fn a_call_stopped_before_it_is_written_is_never_sent() {
        let dir = tmp("unsent");
        let config = scripted(
            &dir,
            r#"read -r first
printf '%s\n' "$first" > "$1/first-call.txt"
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"next"}]}}'
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(10));
        let stop = CancelCtx::new();
        stop.cancel();
        let outcome = session
            .tools_call_until("slow", &json!({}), &stop)
            .expect("a call stopped before it was written is an outcome");
        assert!(outcome.is_error);
        assert_eq!(outcome.stopped, Some(StopOutcome::NotSent));
        assert!(!stop.cancel_begun(), "the call never began");
        let next = session
            .tools_call("next", &json!({}))
            .expect("the untouched pipe answers the next call");
        assert_eq!(next, CallOutcome::new("next", None, false));
        let first = recorded(&dir.join("first-call.txt"));
        assert_eq!(
            first["params"]["name"], "next",
            "the stopped call never left"
        );
        assert_eq!(first["id"], 2);
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The late reply to a call the run stopped waiting for is neither
    /// taken for the next call's reply nor counted against the bytes the
    /// next wait may skip: an 8.3 MB late reply then a 100 kB notification
    /// cross the 8 MiB budget together, and the next call is still answered.
    #[cfg(unix)]
    #[test]
    fn the_late_reply_of_a_stopped_call_is_neither_taken_nor_counted() {
        let dir = tmp("late");
        let config = scripted(
            &dir,
            r#"read -r _call
read -r _cancel
read -r _next
printf '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"'
/usr/bin/head -c 8300000 /dev/zero | /usr/bin/tr '\0' a
printf '"}]}}\n'
printf '{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"'
/usr/bin/head -c 100000 /dev/zero | /usr/bin/tr '\0' b
printf '"}}\n'
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"next"}]}}'
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(60));
        let stop = CancelCtx::new();
        let outcome = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                stop.cancel();
            });
            session.tools_call_until("slow", &json!({}), &stop)
        })
        .expect("a stopped call is an outcome");
        assert_eq!(outcome.stopped, Some(StopOutcome::Unknown));
        let next = session
            .tools_call("next", &json!({}))
            .expect("the next call is answered, its budget untouched by the late reply");
        assert_eq!(next, CallOutcome::new("next", None, false));
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A message carrying a `method` is the server's own request, even when
    /// its id equals the call's: the call waits for its real reply.
    #[cfg(unix)]
    #[test]
    fn a_server_request_reusing_the_call_id_is_not_its_reply() {
        let dir = tmp("ping");
        let config = scripted(
            &dir,
            r#"read -r _call
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"ping"}'
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"real"}]}}'
exec sleep 30
"#,
        );
        let mut session = open(&config, &dir, Duration::from_secs(10));
        let outcome = session
            .tools_call("work", &json!({}))
            .expect("the real reply");
        assert_eq!(outcome, CallOutcome::new("real", None, false));
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A stopped call whose cancel cannot be written (the server closed its
    /// input) leaves no pipe worth keeping: the session answers a transport
    /// failure, so its caller drops it and the next call reconnects.
    #[cfg(unix)]
    #[test]
    fn a_stopped_call_whose_cancel_cannot_be_written_is_a_dead_session() {
        let dir = tmp("deaf");
        let config = scripted(
            &dir,
            r"read -r _call
exec 0<&-
exec sleep 30
",
        );
        let mut session = open(&config, &dir, Duration::from_secs(60));
        let stop = CancelCtx::new();
        let refused = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                stop.cancel();
            });
            session.tools_call_until("slow", &json!({}), &stop)
        })
        .expect_err("a pipe that cannot take the cancel is not kept");
        assert!(
            refused.to_string().contains("cannot write to the server"),
            "{refused}"
        );
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The one JSON line a scripted server wrote to `record`, once it is
    /// there (bounded at five seconds).
    fn recorded(record: &Path) -> Value {
        let mut seen = String::new();
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(record)
                && !text.trim().is_empty()
            {
                seen = text;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        serde_json::from_str(seen.trim()).expect("one JSON line, written within five seconds")
    }

    /// A network allowlist starts the server's loopback egress proxy and
    /// scopes the jail's fence to its port (without it the macOS profile
    /// admits loopback only: the server cannot even resolve a host);
    /// `deny` and `allow` start nothing and keep their arm.
    #[test]
    fn an_allowlist_starts_the_egress_proxy_and_scopes_the_fence() {
        let allowlist =
            nika_kernel::process::EgressAllowlist::new(vec!["api.example.com".to_owned()]);
        let config = McpServerConfig::stdio("probe", "probe-server", Vec::new())
            .with_network(NetPolicy::Allowlist(allowlist));
        let mut spec = config.sandbox_spec(Path::new("/tmp/project"));
        let proxy = start_egress(&config, &mut spec.net)
            .expect("the proxy starts on loopback")
            .expect("an allowlist starts a proxy");
        let NetPolicy::Allowlist(scoped) = &spec.net else {
            panic!("the arm stays an allowlist: {:?}", spec.net);
        };
        assert_eq!(
            scoped.proxy_port,
            Some(proxy.port()),
            "the fence names the proxy's port"
        );
        std::net::TcpStream::connect(("127.0.0.1", proxy.port())).expect("the proxy listens");
        let env = proxy_env(proxy.port());
        assert!(
            env.iter().any(|(name, value)| *name == "HTTPS_PROXY"
                && *value == format!("http://127.0.0.1:{}", proxy.port())),
            "the child is pointed at the proxy: {env:?}"
        );
        for net in [NetPolicy::Deny, NetPolicy::Allow] {
            let config = McpServerConfig::stdio("probe", "probe-server", Vec::new())
                .with_network(net.clone());
            let mut spec = config.sandbox_spec(Path::new("/tmp/project"));
            assert!(
                start_egress(&config, &mut spec.net)
                    .expect("no proxy to start")
                    .is_none()
            );
            assert_eq!(spec.net, net, "the arm is untouched");
        }
    }
}
