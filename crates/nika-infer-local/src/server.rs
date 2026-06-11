// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The v1 sidecar HTTP server (ADR-093) — `tiny_http`, blocking, two routes.
//!
//! `POST /v1/chat/completions` over any [`BackendDyn`] + `GET /health` for
//! the future supervisor (ADR-091 isolation). The workload is sequential by
//! design (one multi-second generation dominates every request), so a
//! blocking thread-per-server loop is the honest shape — an async stack
//! could only add dependency surface around a blocking core.
//!
//! Errors leave as the `OpenAI`-compat error JSON
//! (`{"error": {"message", "type"}}`) so the engine-side wire client
//! (`nika-providers` `OpenAiCompat`) applies its existing mapping unchanged.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::backend::BackendDyn;
use crate::error::InferLocalError;
use crate::protocol::ChatRequest;

/// Request-body ceiling — far above any realistic chat payload, far below
/// a memory-pressure vector. Oversized bodies are refused with `413`.
const MAX_BODY_BYTES: usize = 32 << 20;

/// The one inference route (the `OpenAI`-compat path the wire client calls).
const COMPLETIONS_PATH: &str = "/v1/chat/completions";

/// Liveness probe for the supervisor layer.
const HEALTH_PATH: &str = "/health";

/// A running sidecar server: its bound address + the worker to join.
///
/// Dropping the handle shuts the server down (unblocks the accept loop and
/// joins the worker), so tests and the supervisor cannot leak a listener.
// No `Debug` derive: `tiny_http::Server` doesn't implement it.
pub struct ServerHandle {
    addr: SocketAddr,
    server: Arc<tiny_http::Server>,
    worker: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// The address the server actually bound (port 0 resolves here).
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop accepting requests and join the worker thread.
    ///
    /// Equivalent to dropping the handle; the method exists so call sites
    /// can shut down explicitly and by name.
    pub fn shutdown(self) {
        // Drop runs the teardown.
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            // A panicked worker already aborted the process in tests; at
            // runtime the supervisor restarts the child — nothing to do.
            let _ = worker.join();
        }
    }
}

/// Serve `backend` on `addr` (use port `0` for an ephemeral port).
///
/// Returns once the listener is bound; requests are handled on a dedicated
/// worker thread until the handle is dropped. Generations run one at a time
/// per process (the v1 queue discipline — concurrent requests serialize).
///
/// # Errors
///
/// [`InferLocalError::Backend`] when the address cannot be bound or the
/// single-threaded tokio bridge cannot be built.
pub fn serve<B>(backend: Arc<B>, addr: SocketAddr) -> Result<ServerHandle, InferLocalError>
where
    B: BackendDyn + Send + Sync + 'static,
{
    let server = tiny_http::Server::http(addr).map_err(|e| InferLocalError::Backend {
        reason: format!("bind {addr}: {e}"),
    })?;
    let server = Arc::new(server);
    let bound = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| InferLocalError::Backend {
            reason: "server bound a non-IP address".to_owned(),
        })?;

    // The async→sync bridge: BackendDyn futures are driven to completion on
    // a current-thread runtime owned by the worker (zero executor threads).
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| InferLocalError::Backend {
            reason: format!("tokio bridge: {e}"),
        })?;

    // A dedicated OS thread, NOT `tokio::spawn`: the accept loop blocks for
    // the server's whole life and would pin a tokio worker forever. `Builder`
    // (named + fallible) is the deliberate shape here — the disallowed bare
    // `std::thread::spawn` stays banned for ad-hoc concurrency.
    let accept = Arc::clone(&server);
    let worker = std::thread::Builder::new()
        .name("nika-infer-local-server".to_owned())
        .spawn(move || {
            for request in accept.incoming_requests() {
                route(request, backend.as_ref(), &runtime);
            }
        })
        .map_err(|e| InferLocalError::Backend {
            reason: format!("server worker spawn: {e}"),
        })?;

    Ok(ServerHandle {
        addr: bound,
        server,
        worker: Some(worker),
    })
}

/// Dispatch one request to its route and always respond exactly once.
fn route<B: BackendDyn>(
    mut request: tiny_http::Request,
    backend: &B,
    rt: &tokio::runtime::Runtime,
) {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned();
    let response = match (request.method(), path.as_str()) {
        (tiny_http::Method::Get, HEALTH_PATH) => {
            json_response(200, r#"{"status":"ok"}"#.to_owned())
        }
        (tiny_http::Method::Post, COMPLETIONS_PATH) => completions(&mut request, backend, rt),
        (_, COMPLETIONS_PATH | HEALTH_PATH) => {
            error_response(405, "invalid_request_error", "method not allowed")
        }
        _ => error_response(404, "invalid_request_error", "unknown route"),
    };
    // A send failure means the client hung up — nothing left to tell it.
    let _ = request.respond(response);
}

/// `POST /v1/chat/completions` — parse, generate, serialize.
fn completions<B: BackendDyn>(
    request: &mut tiny_http::Request,
    backend: &B,
    rt: &tokio::runtime::Runtime,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut body = Vec::new();
    // `take(MAX + 1)` lets the cap check distinguish "at the limit" (fine)
    // from "past it" (refused) without ever buffering more than MAX + 1.
    let cap = u64::try_from(MAX_BODY_BYTES).unwrap_or(u64::MAX);
    if let Err(e) = request.as_reader().take(cap + 1).read_to_end(&mut body) {
        return error_response(400, "invalid_request_error", &format!("body read: {e}"));
    }
    if body.len() > MAX_BODY_BYTES {
        return error_response(413, "invalid_request_error", "request body too large");
    }

    let chat: ChatRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => return error_response(400, "invalid_request_error", &format!("bad JSON: {e}")),
    };
    if chat.stream {
        // Honest refusal beats a client hanging for SSE frames that never
        // come — streaming is the ADR-093 follow-up, not silently ignored.
        return error_response(
            400,
            "invalid_request_error",
            "streaming is not supported by the v1 sidecar",
        );
    }

    match rt.block_on(backend.generate(&chat)) {
        Ok(reply) => match serde_json::to_string(&reply) {
            Ok(json) => json_response(200, json),
            Err(e) => error_response(500, "server_error", &format!("serialize: {e}")),
        },
        Err(err) => {
            let (status, kind) = status_for(&err);
            error_response(status, kind, &err.to_string())
        }
    }
}

/// Map a backend failure to its `OpenAI`-compat status + error type.
fn status_for(err: &InferLocalError) -> (u16, &'static str) {
    match err {
        InferLocalError::InvalidRequest { .. } | InferLocalError::ContextOverflow { .. } => {
            (400, "invalid_request_error")
        }
        InferLocalError::ModelNotFound { .. } => (404, "invalid_request_error"),
        InferLocalError::Timeout { .. } => (504, "server_error"),
        // OutOfMemory · Backend · future variants (`#[non_exhaustive]`).
        _ => (500, "server_error"),
    }
}

/// Build the `OpenAI`-compat error body.
fn error_response(
    status: u16,
    kind: &str,
    message: &str,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::json!({ "error": { "message": message, "type": kind } });
    json_response(status, body.to_string())
}

/// A JSON response with the right content type.
fn json_response(status: u16, body: String) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut response = tiny_http::Response::from_string(body).with_status_code(status);
    if let Ok(header) =
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
    {
        response = response.with_header(header);
    }
    response
}
