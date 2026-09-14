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

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
// The `std::process` / `std::thread` exemption below is deliberate and
// scoped: see the module doc — a persistent pipe, tokio unavailable here.
#[allow(clippy::disallowed_types)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use nika_kernel::command_sandbox::CommandSandbox;
use nika_kernel::process::ShellCommand;
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
}

impl CallOutcome {
    /// Build an outcome (INV-019 · the `#[non_exhaustive]` constructor).
    #[must_use]
    pub fn new(content: impl Into<String>, structured: Option<Value>, is_error: bool) -> Self {
        Self {
            content: content.into(),
            structured,
            is_error,
        }
    }

    /// Map a `tools/call` RESULT payload onto the outcome.
    #[must_use]
    pub fn from_result(result: &Value) -> Self {
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

/// The INV-011 guard for a std child (std's `Command` has no
/// `kill_on_drop` — that is a tokio method): dropping the session SIGKILLs
/// the server and reaps it, so a failed handshake, a drift refusal or a
/// finished run never leaks a running subprocess.
#[allow(clippy::disallowed_types)] // see the import-site exemption note
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill(); // SIGKILL · idempotent on an already-dead child
        let _ = self.0.wait(); // reap the zombie
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
        self.request("initialize", &params, self.timeout)?
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

    /// Wait for the reply carrying `want_id`, skipping notifications and
    /// stray ids (a server may interleave) — bounded by `timeout`.
    fn await_reply(&self, want_id: u64, timeout: Duration) -> Result<Value, PinError> {
        for _ in 0..16 {
            match self.rx.recv_timeout(timeout) {
                Ok(Line::Text(text)) => {
                    let msg: Value = serde_json::from_str(&text)
                        .map_err(|e| self.transport(format!("a reply is not JSON: {e}")))?;
                    if msg.get("id").and_then(Value::as_u64) == Some(want_id) {
                        return Ok(msg);
                    }
                }
                Ok(Line::Failed(why)) => return Err(self.transport(why)),
                Ok(Line::Eof) => {
                    return Err(
                        self.transport("the server closed the pipe before answering".to_owned())
                    );
                }
                Err(_) => {
                    return Err(self.transport(format!("no reply within {}s", timeout.as_secs())));
                }
            }
        }
        Err(self.transport("the server sent 16 messages without answering the request".to_owned()))
    }

    /// One request → `Ok(result)` or `Err(error)` (the JSON-RPC error
    /// object) inside a transport-level `Result`.
    fn request(
        &mut self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Result<Value, Value>, PinError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let reply = self.await_reply(id, timeout)?;
        if let Some(error) = reply.get("error") {
            return Ok(Err(error.clone()));
        }
        Ok(Ok(reply.get("result").cloned().unwrap_or(Value::Null)))
    }
}

impl McpSessionDyn for StdioSession {
    fn tools_list(&mut self) -> Result<Vec<McpToolDef>, PinError> {
        let result = self
            .request("tools/list", &json!({}), self.timeout)
            .map_err(|err| self.explain_death(err))?
            .map_err(|error| self.transport(format!("the server refused `tools/list`: {error}")))?;
        McpToolDef::from_list_value(
            &self.server,
            &result.get("tools").cloned().unwrap_or(Value::Null),
        )
    }

    fn tools_call(&mut self, tool: &str, arguments: &Value) -> Result<CallOutcome, PinError> {
        let params = json!({ "name": tool, "arguments": arguments });
        match self
            .request("tools/call", &params, self.call_timeout)
            .map_err(|err| self.explain_death(err))?
        {
            Ok(result) => Ok(CallOutcome::from_result(&result)),
            // A JSON-RPC error on tools/call is the TOOL's failure voice
            // (an unknown name · invalid params) — the model sees it.
            Err(error) => Ok(CallOutcome::new(
                format!("the server refused `tools/call` for `{tool}`: {error}"),
                None,
                true,
            )),
        }
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
    let spec = config.sandbox_spec(project_dir);
    let confined = sandbox
        .confine(&spec, inner)
        .map_err(|e| PinError::Sandbox {
            server: config.name.clone(),
            why: e.to_string(),
        })?;
    let mut cmd = Command::new(&confined.program);
    cmd.args(&confined.args);
    apply_env_scrub(&mut cmd);
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
    Ok((KillOnDrop(child), rx, tail))
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
}
