// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tool server of one agent conversation: the session's own tools
//! ([`SessionTools`], `nika/author-tools@0`), served to the ACP agent that leads the
//! conversation, over MCP, under the name [`SERVER_NAME`] (the agent sees
//! `mcp__nika__<tool>`).
//!
//! The server exposes and relays; it never interprets a tool. `tools/list` answers the
//! session's definitions, `tools/call` hands a listed tool's call to the session (with the
//! tool-use id the client names in the request's `_meta`) and returns its reply as text, a
//! failed call as `isError: true` so the agent sees it. A name the session does not list never
//! reaches it.
//!
//! [`ToolServer`] is the Streamable HTTP transport of [`crate::HttpServer`] under its own law:
//! bound to `127.0.0.1` on an ephemeral port, its bearer minted for this conversation alone
//! (256 bits from the OS) and always required, the same origin gate, body bounds, deadlines and
//! connection cap, one request per connection. It serves connections side by side until its
//! [`Closer`] closes it, and hands the session one call at a time: a call holds the next call
//! for as long as its tool runs. For an agent that cannot mount an HTTP server, [`bridge`]
//! relays a stdio MCP session to the same server, line by line, with the same bearer.

use std::fmt::Write as _;
use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use serde_json::{Value, json};

use crate::{MAX_MSG_BYTES, McpError, http, protocol};

/// The name a conversation's tool server is mounted under.
pub const SERVER_NAME: &str = "nika";

/// Where Claude Code names its tool-use id in a call's `_meta`.
const TOOL_USE_ID: &str = "/_meta/claudecode~1toolUseId";

/// How long the bridge waits on each read of the server's reply.
const BRIDGE_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How long [`Closer::close`] tries to wake the accept loop.
const WAKE_TIMEOUT: Duration = Duration::from_secs(1);

/// Answer one MCP message for `tools`: `initialize`, `tools/list`, `tools/call` and `ping`;
/// a notification gets no reply, and a batch is refused (MCP removed them in 2025-06-18).
pub(crate) fn dispatch(tools: &dyn SessionTools, msg: &Value) -> Option<Value> {
    if msg.is_array() {
        let why = "invalid request: JSON-RPC batches are not part of MCP — send one message";
        return Some(protocol::err(&Value::Null, -32600, why));
    }
    let id = msg.get("id")?;
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        let why = "invalid request: `method` must be a string";
        return Some(protocol::err(id, -32600, why));
    };
    Some(match method {
        "initialize" => protocol::ok(id, initialize(msg.get("params"))),
        "tools/list" => {
            let listed: Vec<Value> = tools.tools().iter().map(listing).collect();
            protocol::ok(id, json!({ "tools": listed }))
        }
        "tools/call" => call(tools, id, msg.get("params")),
        "ping" => protocol::ok(id, json!({})),
        other => protocol::err(id, -32601, &format!("method not found: {other}")),
    })
}

fn initialize(params: Option<&Value>) -> Value {
    json!({
        "protocolVersion": protocol::negotiated(params),
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// One definition as an MCP tool.
fn listing(def: &ToolDef) -> Value {
    json!({
        "name": def.name,
        "description": def.description,
        "inputSchema": def.input_schema,
        "annotations": { "readOnlyHint": def.read_only },
    })
}

/// `tools/call`: a listed tool's call handed to the session, its reply returned as text.
fn call(tools: &dyn SessionTools, id: &Value, params: Option<&Value>) -> Value {
    let Some(name) = params.and_then(|p| p.get("name")).and_then(Value::as_str) else {
        let why = "invalid params: `tools/call` requires {name, arguments}";
        return protocol::err(id, -32602, why);
    };
    if !tools.tools().iter().any(|def| def.name == name) {
        let text = format!("no tool `{name}` is served in this conversation");
        return protocol::ok(id, protocol::tool_content(&text, true));
    }
    let arguments = params.and_then(|p| p.get("arguments")).cloned();
    let mut asked = ToolCall::new(name, arguments.unwrap_or_else(|| json!({})));
    let tool_use = params.and_then(|p| p.pointer(TOOL_USE_ID));
    if let Some(meta) = tool_use.and_then(Value::as_str) {
        asked = asked.with_meta(meta);
    }
    let reply = tools.call(asked);
    protocol::ok(id, protocol::tool_content(&reply.text, reply.is_error))
}

/// The session behind a turn lock: the server reads and answers connections side by side, and
/// the session receives one call at a time.
struct OneAtATime<'a> {
    tools: &'a dyn SessionTools,
    turn: Mutex<()>,
}

impl SessionTools for OneAtATime<'_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }

    /// A session that panics breaks its contract; the agent still reads an error reply and the
    /// server keeps serving, where the panic would otherwise end the connection unanswered and
    /// resurface when the server closes. The payload stays in the panic hook's report.
    fn call(&self, call: ToolCall) -> ToolReply {
        let name = call.name.clone();
        // The lock guards no state of its own: a poisoned one holds nothing to distrust.
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        std::panic::catch_unwind(AssertUnwindSafe(|| self.tools.call(call))).unwrap_or_else(|_| {
            ToolReply::error(format!("the session's `{name}` tool failed unexpectedly"))
        })
    }
}

/// One conversation's MCP tool server over Streamable HTTP: loopback, an ephemeral port and a
/// bearer minted for this conversation alone.
pub struct ToolServer {
    listener: TcpListener,
    bearer: String,
    tools: Arc<dyn SessionTools>,
    closed: Arc<AtomicBool>,
}

impl ToolServer {
    /// Bind `127.0.0.1` on an ephemeral port and mint this conversation's bearer; nothing is
    /// served before [`Self::serve`].
    ///
    /// # Errors
    /// [`McpError::Transport`] when the listener cannot bind or the OS gives no randomness.
    pub fn start(tools: Arc<dyn SessionTools>) -> Result<Self, McpError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        Ok(Self {
            listener,
            bearer: mint()?,
            tools,
            closed: Arc::default(),
        })
    }

    /// The endpoint an agent mounts: `http://127.0.0.1:<port>/mcp`.
    ///
    /// # Errors
    /// [`McpError::Transport`] when the OS cannot report the listener's address.
    pub fn url(&self) -> Result<String, McpError> {
        Ok(format!("http://{}/mcp", self.listener.local_addr()?))
    }

    /// The bearer every request must carry: hand it only to the agent that mounts the server.
    #[must_use]
    pub fn bearer(&self) -> &str {
        &self.bearer
    }

    /// The names of the tools the session lists now.
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.tools().into_iter().map(|def| def.name).collect()
    }

    /// The handle that ends [`Self::serve`]; dropping it ends it too.
    ///
    /// # Errors
    /// [`McpError::Transport`] when the OS cannot report the listener's address.
    pub fn closer(&self) -> Result<Closer, McpError> {
        Ok(Closer {
            addr: self.listener.local_addr()?,
            closed: Arc::clone(&self.closed),
        })
    }

    /// Serve until closed: one request and one response per connection, each connection on a
    /// thread of its own, at most eight at once, under the HTTP door's deadlines (a request has
    /// 30 s in all to arrive, its response 30 s to leave), so a slow or silent client holds one
    /// connection, never the server. The session receives one call at a time: a call holds the
    /// next call, not the next request, for as long as its tool runs. Blocking: run it on a
    /// thread of its own; it returns once closed and every connection in hand is answered. A
    /// failed accept is skipped, never fatal.
    pub fn serve(&self) {
        let session = OneAtATime {
            tools: &*self.tools,
            turn: Mutex::new(()),
        };
        let answer = |msg: &Value| dispatch(&session, msg);
        http::serve_bounded(
            || http::accept(&self.listener),
            Some(&self.bearer),
            &answer,
            &self.closed,
            http::Limits::SERVE,
        );
    }
}

impl std::fmt::Debug for ToolServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolServer")
            .field("addr", &self.listener.local_addr().ok())
            .field("bearer", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Ends a [`ToolServer::serve`] loop: no connection is taken after it, and `serve` returns once
/// the connections in hand are answered.
#[derive(Debug)]
pub struct Closer {
    addr: SocketAddr,
    closed: Arc<AtomicBool>,
}

impl Closer {
    /// Close the server; a later call does nothing. It never waits long: when the listener's
    /// backlog is full, the loop sees the close at the next connection it takes.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            // Wake an accept that waits for a connection.
            let _ = TcpStream::connect_timeout(&self.addr, WAKE_TIMEOUT);
        }
    }
}

impl Drop for Closer {
    fn drop(&mut self) {
        self.close();
    }
}

/// 256 bits from the OS, hex-encoded.
fn mint() -> Result<String, McpError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        std::io::Error::other(format!("no OS randomness for the bearer: {error}"))
    })?;
    let mut hex = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        // Writing into a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Relay a stdio MCP session (`input` → `output`, one JSON-RPC message per line) to a
/// conversation's tool server at `url` (`http://127.0.0.1:<port>/mcp`), each message posted
/// with `bearer`. A request's reply is written as one line; a notification gets none; a refusal
/// without a JSON-RPC body is answered as a JSON-RPC error naming the HTTP status. Ends at the
/// end of `input`.
///
/// # Errors
/// [`McpError::Transport`] when `url` is not a loopback HTTP endpoint, the server cannot be
/// reached, a line exceeds the message ceiling, or `input`/`output` fails.
pub fn bridge<R: BufRead, W: Write>(
    url: &str,
    bearer: &str,
    mut input: R,
    mut output: W,
) -> Result<(), McpError> {
    let (authority, path) = loopback(url)?;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut input)
            .take(MAX_MSG_BYTES + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        if line.last() != Some(&b'\n') && read as u64 > MAX_MSG_BYTES {
            let why = format!("a message exceeds the {MAX_MSG_BYTES}-byte line ceiling");
            return Err(McpError::Transport(std::io::Error::other(why)));
        }
        let body = line.trim_ascii();
        if body.is_empty() {
            continue;
        }
        let (status, reply) = post(authority, path, bearer, body)?;
        let answer = serde_json::from_slice::<Value>(&reply).ok();
        if let Some(answer) = answer.or_else(|| refused(status, body)) {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
}

/// The authority and path of a loopback `http://` endpoint, or the refusal.
fn loopback(url: &str) -> Result<(&str, &str), McpError> {
    let refuse = || {
        let why = format!("`{url}` is not a loopback http endpoint of a conversation");
        McpError::Transport(std::io::Error::other(why))
    };
    let rest = url.strip_prefix("http://").ok_or_else(refuse)?;
    let (authority, path) = rest.find('/').map_or((rest, "/"), |at| rest.split_at(at));
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    if matches!(host, "127.0.0.1" | "[::1]" | "localhost") {
        Ok((authority, path))
    } else {
        Err(refuse())
    }
}

/// One POST of `body`, answered `(status, body)`; the response is read to its close, bounded.
fn post(
    authority: &str,
    path: &str,
    bearer: &str,
    body: &[u8],
) -> Result<(u16, Vec<u8>), McpError> {
    let mut stream = TcpStream::connect(authority)?;
    stream.set_read_timeout(Some(BRIDGE_READ_TIMEOUT))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Accept: application/json\r\nAuthorization: Bearer {bearer}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    let mut response = Vec::new();
    (&mut stream)
        .take(MAX_MSG_BYTES + 64 * 1024)
        .read_to_end(&mut response)?;
    let split = (response.windows(4)).position(|w| w == b"\r\n\r\n");
    let (head, rest) = split.map_or((&response[..], &[][..]), |at| {
        (&response[..at], &response[at + 4..])
    });
    let status = (String::from_utf8_lossy(head).split_ascii_whitespace())
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Ok((status, rest.to_vec()))
}

/// The JSON-RPC error a request receives when the server answered it without a JSON-RPC body
/// (a refusal before the dispatch); a notification, or an accepted one, receives nothing.
fn refused(status: u16, request: &[u8]) -> Option<Value> {
    let parsed = serde_json::from_slice::<Value>(request).ok();
    let id = parsed.as_ref().and_then(|msg| msg.get("id"))?;
    let message = format!("the conversation's tool server refused the request (HTTP {status})");
    Some(protocol::err(id, -32603, &message))
}

#[cfg(test)]
mod tests;
