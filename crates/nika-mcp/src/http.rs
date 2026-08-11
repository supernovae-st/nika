// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The **Streamable HTTP** transport (#335) — the second pump over the
//! same pure dispatch the stdio pump feeds.
//!
//! MCP spec rev 2025-11-25 §Transports allows a deliberately minimal,
//! fully conformant shape for a read-only, stateless server, and this
//! is exactly that shape:
//!
//! - `POST` with a JSON-RPC **request** → `200 · application/json`, ONE
//!   response object (the spec's non-SSE branch — a synchronous audit
//!   tool never streams);
//! - `POST` with a **notification** (no `id`) → `202 Accepted`;
//! - `GET` (the server-push stream) → `405` (explicitly legal: a server
//!   MUST return SSE *or 405* — a read-only server has nothing to push);
//! - `DELETE` (session end) → `405` — no `Mcp-Session-Id` is ever
//!   assigned (a stateless server MAY simply not); one sent by a client
//!   is ignored;
//! - JSON-RPC **batches** are not accepted (removed from the spec in
//!   the 2025-06-18 revision).
//!
//! **Security posture** (the spec's own MUSTs + house sovereignty):
//! the `Origin` header is validated on every request (anti
//! DNS-rebinding — absent is allowed for non-browser clients, loopback
//! origins pass, anything else is `403`); the listener binds loopback
//! unless the operator widens it explicitly; `NIKA_MCP_TOKEN` arms
//! constant-time bearer auth; bodies require `Content-Length` (`411`
//! otherwise — every first-party MCP SDK sends sized bodies) and are
//! capped at the same 8 MiB trust-boundary ceiling the stdio pump
//! enforces (`413` over it).
//!
//! **Zero new dependencies** — hand-rolled HTTP/1.1 over
//! `std::net::TcpListener`, thread per connection, `Connection: close`
//! per response (correct and simple; keep-alive is a measured future
//! upgrade). The crate stays zero-async, zero-SDK — its stated
//! discipline. Production TLS is a reverse proxy's job (documented on
//! the CLI flag), never hand-rolled here.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use crate::{MAX_MSG_BYTES, McpError, protocol};

/// Header-section ceiling (start line + headers). Generous for real
/// clients; a bound because the socket is a trust boundary.
const MAX_HEAD_BYTES: usize = 16 * 1024;

/// Per-connection read deadline — a stalled client releases its thread.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// One parsed request — method · target path · lowercased header map ·
/// raw body bytes.
struct Request {
    method: String,
    target: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

/// A refusal the transport answers WITHOUT reaching the dispatch —
/// status + teaching body (the HTTP twin of the stdio `-32700` line).
#[derive(Debug)]
struct Refusal {
    status: u16,
    reason: &'static str,
    detail: String,
}

impl Refusal {
    fn new(status: u16, reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            reason,
            detail: detail.into(),
        }
    }
}

/// A bound-but-not-yet-serving MCP HTTP server. Two-step shape so the
/// CALLER owns the operator surface: it binds (learning the resolved
/// address — port 0 becomes real), prints its own banner through its
/// own print discipline, reads its own env for the bearer token, then
/// hands both to [`HttpServer::serve`]. This crate stays env-free and
/// print-free (the workspace purity lints are the enforcement).
pub struct HttpServer {
    listener: TcpListener,
}

impl HttpServer {
    /// Bind `bind:port` without serving yet.
    ///
    /// # Errors
    /// Returns [`McpError::Transport`] if the listener cannot bind.
    pub fn bind(bind: &str, port: u16) -> Result<Self, McpError> {
        Ok(Self {
            listener: TcpListener::bind((bind, port))?,
        })
    }

    /// The resolved local address (a `--port 0` request becomes real here).
    ///
    /// # Errors
    /// Returns [`McpError::Transport`] if the OS cannot report the address.
    pub fn addr(&self) -> Result<std::net::SocketAddr, McpError> {
        Ok(self.listener.local_addr()?)
    }

    /// Refuse a public bind with no bearer token (#822 · #890): a
    /// non-loopback address with no auth answers every request
    /// unauthenticated — refuse to START, naming both fixes. The caller
    /// reads its env and asks before [`Self::serve`]; loopback with no
    /// token stays convenient. Judges the RESOLVED address, so a
    /// `localhost` bind reads as the loopback it bound, never the
    /// spelling.
    ///
    /// # Errors
    /// [`McpError::Transport`] when the bound address is non-loopback and
    /// no token is held (the refusal names both fixes), or when the OS
    /// cannot report the listener's address.
    pub fn guard_bind_auth(&self, token: Option<&str>) -> Result<(), McpError> {
        let addr = self.addr()?;
        if bind_auth_ok(addr.ip(), token) {
            return Ok(());
        }
        Err(McpError::Transport(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to serve {addr} without auth — a non-loopback bind reaches the \
                 network unauthenticated; set NIKA_MCP_TOKEN (a bearer is required) or \
                 bind a loopback address"
            ),
        )))
    }

    /// Accept forever — SEQUENTIAL: one connection, one request, one
    /// response (`Connection: close`), then the next. An MCP host is one
    /// client issuing millisecond-scale audit calls; serializing them is
    /// correct and keeps this crate free of threading (the workspace is
    /// tokio-first and this transport is deliberately sync — a measured
    /// upgrade if a real host ever saturates it). A failed accept is
    /// skipped (transient), never fatal.
    pub fn serve(&self, token: Option<&str>) -> ! {
        loop {
            let Ok((stream, _)) = self.listener.accept() else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
            handle_conn(stream, token);
        }
    }
}

/// One connection = one request = one response, then close. Transport
/// write errors are swallowed — the client hung up; there is nobody
/// left to tell.
fn handle_conn(stream: TcpStream, token: Option<&str>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut out = stream;
    match parse_request(&mut reader) {
        Ok(req) => {
            let (status, reason, content_type, body) = respond(&req, token);
            let _ = write_response(&mut out, status, reason, content_type, body.as_bytes());
        }
        Err(refusal) => {
            let _ = write_response(
                &mut out,
                refusal.status,
                refusal.reason,
                "text/plain; charset=utf-8",
                refusal.detail.as_bytes(),
            );
        }
    }
}

/// Route one parsed request through the gates to the pure dispatch —
/// returns (status · reason · content-type · body).
fn respond(req: &Request, token: Option<&str>) -> (u16, &'static str, &'static str, String) {
    if !origin_allowed(req.headers.get("origin").map(String::as_str)) {
        return (
            403,
            "Forbidden",
            "text/plain; charset=utf-8",
            "origin not allowed — this endpoint serves local MCP clients, not browsers \
             (DNS-rebinding guard, MCP spec §security)"
                .to_owned(),
        );
    }
    if !auth_ok(req.headers.get("authorization").map(String::as_str), token) {
        return (
            401,
            "Unauthorized",
            "text/plain; charset=utf-8",
            "missing or wrong bearer — this server was started with NIKA_MCP_TOKEN set; \
             send `Authorization: Bearer <token>`"
                .to_owned(),
        );
    }
    if req.method != "POST" {
        // GET = the optional server-push stream (nothing to push, 405 is
        // the spec's sanctioned answer) · DELETE = session end (stateless).
        return (
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            format!(
                "{} unsupported — POST one JSON-RPC message to this endpoint \
                 (read-only server: no push stream, no sessions)",
                req.method
            ),
        );
    }
    if req.target != "/mcp" && req.target != "/" {
        return (
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            format!("no route {} — the MCP endpoint is /mcp", req.target),
        );
    }
    dispatch_body(&req.body)
}

/// Parse the body as ONE JSON-RPC message and dispatch it — the exact
/// contract the stdio pump applies per line, minus the framing.
fn dispatch_body(body: &[u8]) -> (u16, &'static str, &'static str, String) {
    let parsed = std::str::from_utf8(body)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).map_err(|e| e.to_string()));
    let msg = match parsed {
        Ok(msg) if msg.is_array() => {
            // Batches left the spec in 2025-06-18 — refuse loudly rather
            // than answering half a contract.
            let reply = serde_json::json!({
                "jsonrpc": "2.0", "id": serde_json::Value::Null,
                "error": { "code": -32600,
                    "message": "JSON-RPC batches are not part of MCP (removed 2025-06-18) — send one message per POST" }
            });
            return (400, "Bad Request", "application/json", reply.to_string());
        }
        Ok(msg) => msg,
        Err(e) => {
            let reply = serde_json::json!({
                "jsonrpc": "2.0", "id": serde_json::Value::Null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            });
            return (400, "Bad Request", "application/json", reply.to_string());
        }
    };
    match protocol::dispatch(&msg) {
        Some(reply) => (200, "OK", "application/json", reply.to_string()),
        // A notification produces no reply — 202 is the spec's word for it.
        None => (202, "Accepted", "application/json", String::new()),
    }
}

/// The DNS-rebinding gate: absent `Origin` (curl · SDK clients) passes;
/// loopback origins pass; everything else is refused. A local audit
/// server has no browser story, so there is no allowlist to configure.
fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(origin) = origin else { return true };
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let Some(rest) = rest else { return false };
    let host = rest.split(':').next().unwrap_or(rest);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// The bearer gate — armed only when the server holds a token.
/// Constant-time comparison: an early-exit mismatch would leak the
/// prefix length to a timing probe.
fn auth_ok(header: Option<&str>, token: Option<&str>) -> bool {
    let Some(token) = token else { return true };
    let Some(sent) = header.and_then(|h| h.strip_prefix("Bearer ")) else {
        return false;
    };
    let (a, b) = (sent.as_bytes(), token.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The bind×auth matrix, pure (#890): a held token makes any bind lawful;
/// loopback is lawful without one. Anything else refuses to start.
fn bind_auth_ok(ip: std::net::IpAddr, token: Option<&str>) -> bool {
    token.is_some() || ip.is_loopback()
}

/// Read one HTTP/1.1 request: start line + headers (bounded), then a
/// `Content-Length`-sized body (bounded). Chunked bodies are refused
/// with `411` — every first-party MCP SDK sends sized bodies, and a
/// sized read is the bounded one.
fn parse_request<R: BufRead>(reader: &mut R) -> Result<Request, Refusal> {
    let start = read_head_line(reader)?;
    let mut parts = start.split_ascii_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(Refusal::new(400, "Bad Request", "malformed request line"));
    };
    let (method, target) = (method.to_owned(), target.to_owned());

    let mut headers = BTreeMap::new();
    let mut head_budget = MAX_HEAD_BYTES.saturating_sub(start.len());
    loop {
        let line = read_head_line(reader)?;
        if line.is_empty() {
            break;
        }
        head_budget = head_budget.checked_sub(line.len()).ok_or_else(|| {
            Refusal::new(
                431,
                "Request Header Fields Too Large",
                "header section over 16 KiB",
            )
        })?;
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }

    let body = read_sized_body(reader, &method, &headers)?;
    Ok(Request {
        method,
        target,
        headers,
        body,
    })
}

/// One CRLF-terminated head line (start line or header), bounded.
fn read_head_line<R: BufRead>(reader: &mut R) -> Result<String, Refusal> {
    let mut buf = Vec::new();
    let n = reader
        .take(MAX_HEAD_BYTES as u64 + 1)
        .read_until(b'\n', &mut buf)
        .map_err(|e| Refusal::new(400, "Bad Request", format!("read failed: {e}")))?;
    if n == 0 || buf.last() != Some(&b'\n') {
        return Err(Refusal::new(400, "Bad Request", "truncated request head"));
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| Refusal::new(400, "Bad Request", "non-UTF-8 request head"))
}

/// The bounded body read — `Content-Length` required on POST (411
/// otherwise: chunked is the unbounded form), capped at the stdio
/// pump's own 8 MiB ceiling (413 over it). Non-POST methods carry none.
fn read_sized_body<R: BufRead>(
    reader: &mut R,
    method: &str,
    headers: &BTreeMap<String, String>,
) -> Result<Vec<u8>, Refusal> {
    if method != "POST" {
        return Ok(Vec::new());
    }
    if headers.contains_key("transfer-encoding") {
        return Err(Refusal::new(
            411,
            "Length Required",
            "chunked bodies are refused — send Content-Length (every MCP SDK does)",
        ));
    }
    let len: u64 = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| Refusal::new(411, "Length Required", "POST needs Content-Length"))?;
    if len > MAX_MSG_BYTES {
        return Err(Refusal::new(
            413,
            "Content Too Large",
            format!("body exceeds the {MAX_MSG_BYTES}-byte message ceiling"),
        ));
    }
    let mut body = vec![0u8; usize::try_from(len).unwrap_or(usize::MAX)];
    reader.read_exact(&mut body).map_err(|e| {
        Refusal::new(
            400,
            "Bad Request",
            format!("body shorter than declared: {e}"),
        )
    })?;
    Ok(body)
}

/// One HTTP/1.1 response, `Connection: close`, flushed.
fn write_response(
    out: &mut impl Write,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        out,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    out.write_all(body)?;
    out.flush()
}

#[cfg(test)]
// std::thread::spawn is workspace-disallowed in PRODUCTION (tokio-first) —
// these tests need a real OS thread to host the blocking accept loop, the
// exact thing the sync transport is. Test-only, deliberate.
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(raw: &str) -> Result<Request, Refusal> {
        parse_request(&mut Cursor::new(raw.as_bytes().to_vec()))
    }

    /// A well-formed sized POST parses: method · target · lowercased
    /// headers · exact body bytes.
    #[test]
    fn parses_a_sized_post() {
        let req = parse(
            "POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
        )
        .expect("parses");
        assert_eq!(req.method, "POST");
        assert_eq!(req.target, "/mcp");
        assert_eq!(req.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(req.body, b"{}");
    }

    /// The unbounded form is refused with the teaching 411 — chunked
    /// AND absent Content-Length alike.
    #[test]
    fn chunked_and_lengthless_posts_get_411() {
        for raw in [
            "POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST /mcp HTTP/1.1\r\nHost: x\r\n\r\n",
        ] {
            let refusal = parse(raw).err().expect("refused");
            assert_eq!(refusal.status, 411, "{}", refusal.detail);
        }
    }

    /// The stdio pump's 8 MiB ceiling holds on this transport too.
    #[test]
    fn oversize_body_gets_413() {
        let raw = format!(
            "POST /mcp HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_MSG_BYTES + 1
        );
        let refusal = parse(&raw).err().expect("refused");
        assert_eq!(refusal.status, 413);
    }

    /// The DNS-rebinding matrix: absent + loopback pass · foreign
    /// origins are refused (http and https alike).
    #[test]
    fn origin_gate_matrix() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("http://localhost")));
        assert!(origin_allowed(Some("http://localhost:6274")));
        assert!(origin_allowed(Some("https://127.0.0.1:8123")));
        assert!(!origin_allowed(Some("https://evil.example.com")));
        assert!(!origin_allowed(Some("http://localhost.evil.com")));
        assert!(!origin_allowed(Some("file://x")));
    }

    /// The bearer matrix: unarmed passes everything · armed requires
    /// the exact token in the Bearer form.
    #[test]
    fn auth_gate_matrix() {
        assert!(auth_ok(None, None));
        assert!(auth_ok(Some("Bearer whatever"), None));
        assert!(!auth_ok(None, Some("s3cret")));
        assert!(!auth_ok(Some("Bearer wrong"), Some("s3cret")));
        assert!(
            !auth_ok(Some("s3cret"), Some("s3cret")),
            "raw token without the Bearer form"
        );
        assert!(auth_ok(Some("Bearer s3cret"), Some("s3cret")));
    }

    /// The bind×auth matrix (#890): a token lawfuls any bind · loopback
    /// is lawful without one · a public or wildcard address without a
    /// token refuses to start.
    #[test]
    fn bind_auth_matrix() {
        use std::net::IpAddr;
        let v4_loop: IpAddr = "127.0.0.1".parse().expect("literal");
        let v6_loop: IpAddr = "::1".parse().expect("literal");
        let v4_public: IpAddr = "192.168.1.20".parse().expect("literal");
        let v4_wildcard: IpAddr = "0.0.0.0".parse().expect("literal");
        assert!(bind_auth_ok(v4_loop, None));
        assert!(bind_auth_ok(v6_loop, None));
        assert!(bind_auth_ok(v4_public, Some("s3cret")));
        assert!(bind_auth_ok(v4_wildcard, Some("s3cret")));
        assert!(!bind_auth_ok(v4_public, None));
        assert!(!bind_auth_ok(v4_wildcard, None));
    }

    /// The guard on a REAL loopback bind passes with or without a token
    /// (#890) — the refusal needs no fixture: the bind×auth matrix above
    /// decides it, and the resolved address here is the loopback kind.
    #[test]
    fn a_loopback_bind_passes_the_guard_without_a_token() {
        let server = HttpServer::bind("127.0.0.1", 0).expect("ephemeral bind");
        assert!(server.guard_bind_auth(None).is_ok());
        assert!(server.guard_bind_auth(Some("s3cret")).is_ok());
    }

    /// GET answers the spec-sanctioned 405 (no push stream to offer) —
    /// and the gates run BEFORE the method check (a foreign origin on a
    /// GET is still 403).
    #[test]
    fn get_is_405_and_origin_still_gates_it() {
        let req = parse("GET /mcp HTTP/1.1\r\nHost: x\r\n\r\n").expect("parses");
        let (status, ..) = respond(&req, None);
        assert_eq!(status, 405);
        let req =
            parse("GET /mcp HTTP/1.1\r\nOrigin: https://evil.example.com\r\n\r\n").expect("parses");
        let (status, ..) = respond(&req, None);
        assert_eq!(status, 403);
    }

    /// A JSON-RPC REQUEST body answers 200 + ONE application/json
    /// object; a NOTIFICATION answers 202 with an empty body; a BATCH
    /// answers 400 naming the 2025-06-18 removal.
    #[test]
    fn dispatch_shapes_request_notification_batch() {
        let (status, _, ct, body) = dispatch_body(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        assert_eq!((status, ct), (200, "application/json"));
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(v["id"], 1);

        let (status, _, _, body) =
            dispatch_body(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert_eq!(status, 202);
        assert!(body.is_empty());

        let (status, _, _, body) = dispatch_body(br"[]");
        assert_eq!(status, 400);
        assert!(body.contains("2025-06-18"), "{body}");
    }

    /// The full socket round-trip on an ephemeral port: initialize →
    /// tools/list through a REAL `TcpStream` — the exact bytes a client
    /// sends, the exact bytes it reads back.
    #[test]
    fn real_socket_round_trip() {
        let server = HttpServer::bind("127.0.0.1", 0).expect("ephemeral bind");
        let addr = server.addr().expect("addr");
        std::thread::spawn(move || server.serve(None));

        let reply = post(
            addr,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
        );
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert!(reply.contains("application/json"), "{reply}");
        assert!(reply.contains("protocolVersion"), "{reply}");

        let reply = post(addr, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        assert!(reply.contains("nika_check"), "{reply}");
    }

    /// The armed server refuses a tokenless client with the teaching
    /// 401 and accepts the exact bearer.
    #[test]
    fn real_socket_auth_round_trip() {
        let server = HttpServer::bind("127.0.0.1", 0).expect("ephemeral bind");
        let addr = server.addr().expect("addr");
        std::thread::spawn(move || server.serve(Some("s3cret")));

        let reply = post(addr, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");

        let reply = post_with(
            addr,
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            "Authorization: Bearer s3cret\r\n",
        );
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    }

    fn post(addr: std::net::SocketAddr, body: &str) -> String {
        post_with(addr, body, "")
    }

    fn post_with(addr: std::net::SocketAddr, body: &str, extra: &str) -> String {
        let mut s = TcpStream::connect(addr).expect("connect");
        write!(
            s,
            "POST /mcp HTTP/1.1\r\nHost: t\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("send");
        let mut reply = String::new();
        s.read_to_string(&mut reply).expect("read");
        reply
    }
}
