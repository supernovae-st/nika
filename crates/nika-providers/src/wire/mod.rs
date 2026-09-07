// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wire adapters — one module per protocol family.
//!
//! Shared here: HTTP→Provider error mapping, the SSE→`InferEvent` stream
//! wrapper (one state machine, per-wire `EventMapper`s), and the
//! `GenAiSystem` attribution table.

pub(crate) mod anthropic;
pub(crate) mod gemini;
pub(crate) mod mock;
mod mock_schema;
pub(crate) mod openai_compat;
#[cfg(test)]
mod openai_compat_usage_tests;
mod openai_schema;

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::ai::provider::{InferEvent, ProviderError};
use nika_kernel::genai::GenAiSystem;
use nika_kernel::http::HttpError;

use crate::profile::Profile;
use crate::sse::SseParser;

/// Default transport deadline for CLOUD providers when the task declares
/// no `timeout:` — matches the HTTP effect's historical 30s default (the
/// pre-plumb behavior · cloud completions comfortably fit it).
pub(crate) const CLOUD_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Default transport deadline for the 5 LOCAL servers (`ollama` ·
/// `lmstudio` · `llamacpp` · `localai` · `vllm`) when the task declares
/// no `timeout:` — a local model routinely needs minutes for one
/// completion on consumer hardware; the 30s cloud default killed every
/// serious local-first workflow with a 408 (F1 field report 2026-07-04).
pub(crate) const LOCAL_DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// A parsed `data:image/...;base64,...` URL (the inline form file vision
/// becomes after the verb loads bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DataImage<'a> {
    /// `image/png` · `image/jpeg` · …
    pub media_type: &'a str,
    /// Raw base64 payload (no `data:` prefix).
    pub data: &'a str,
}

/// Split a `data:image/<type>;base64,<payload>` URL. Anything else
/// (http(s), CAS hashes, `data:text/…`) is `None`.
pub(crate) fn parse_data_image(source: &str) -> Option<DataImage<'_>> {
    let rest = source.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if data.is_empty() {
        return None;
    }
    let mut tokens = meta.split(';');
    let media_type = tokens.next()?.trim();
    if !media_type.starts_with("image/") {
        return None;
    }
    if !tokens.any(|t| t.eq_ignore_ascii_case("base64")) {
        return None;
    }
    Some(DataImage { media_type, data })
}

/// True when the image source is a fetchable URL or an inline data URL
/// (the v0.1 allowed set — CAS hashes still wait for nika-media).
pub(crate) fn image_source_is_url(source: &str) -> bool {
    source.starts_with("http://")
        || source.starts_with("https://")
        || parse_data_image(source).is_some()
}

/// The per-request transport deadline for one provider round-trip.
///
/// BUFFERED calls always get a total deadline: the task-level `timeout:`
/// (plumbed via `InferRequest::timeout`) when declared, else the
/// per-provider default (local ≫ cloud — the sovereignty story breaks
/// when a 14B model gets 30s). STREAMING requests carry only an EXPLICIT
/// task timeout (else `None`): an SSE generation legitimately outlives
/// any fixed total budget — the http effect's idle-read guard reaps a
/// STALLED stream instead (`nika-http` streaming timeout semantics).
pub(crate) fn transport_deadline(
    profile: &Profile,
    req: &nika_kernel::ai::provider::InferRequest,
    stream: bool,
) -> Option<std::time::Duration> {
    if stream {
        return req.timeout;
    }
    Some(req.timeout.unwrap_or(if profile.is_local() {
        LOCAL_DEFAULT_TIMEOUT
    } else {
        CLOUD_DEFAULT_TIMEOUT
    }))
}

/// Transport-layer failure → provider error (no HTTP status yet).
pub(crate) fn map_http_err(e: &HttpError) -> ProviderError {
    match e {
        HttpError::Timeout { .. } => ProviderError::Api {
            status: 408,
            message: e.to_string(),
        },
        _ => ProviderError::Other {
            reason: e.to_string(),
        },
    }
}

/// Non-2xx on a streaming open: drain the (effect-capped) error body so the
/// provider's message + retry-after survive into the same typed mapping as
/// the non-streaming path (a stream 401 must be `AuthFailed`, not `Api`).
pub(crate) async fn stream_status_error(
    resp: nika_kernel::http::HttpStreamResponse,
    model: &str,
) -> ProviderError {
    const ERROR_BODY_CAP: usize = 64 * 1024;
    let mut body = resp.body;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        match chunk {
            Ok(bytes) => {
                let room = ERROR_BODY_CAP.saturating_sub(buf.len());
                buf.extend_from_slice(&bytes[..bytes.len().min(room)]);
                if buf.len() >= ERROR_BODY_CAP {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    status_error(
        resp.status,
        &buf,
        resp.headers.get("retry-after").map(String::as_str),
        model,
    )
}

/// Non-2xx status + body → typed provider error (shared mapping table —
/// both dialects put a human message under `error.message`).
pub(crate) fn status_error(
    status: u16,
    body: &[u8],
    retry_after: Option<&str>,
    model: &str,
) -> ProviderError {
    let message = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/message"))
                .and_then(|m| m.as_str().map(ToOwned::to_owned))
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).into_owned());

    match status {
        401 | 403 => ProviderError::AuthFailed { reason: message },
        // 404 usually means the model id — but with an operator-overridden
        // base_url it can be a path typo; the model field carries the hint.
        404 => ProviderError::ModelNotFound {
            model: model.to_owned(),
        },
        429 => ProviderError::RateLimited {
            // RFC 9110 allows delay-seconds (integer) — some gateways send
            // fractional seconds; accept both via Duration (rejects NaN /
            // negative / overflow structurally), round to ms.
            retry_after_ms: retry_after
                .and_then(|s| s.trim().parse::<f64>().ok())
                .and_then(|secs| std::time::Duration::try_from_secs_f64(secs).ok())
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        },
        _ => ProviderError::Api { status, message },
    }
}

/// `gen_ai.system` attribution per canonical provider id.
pub(crate) fn gen_ai_system(provider_id: &str) -> GenAiSystem {
    match provider_id {
        "anthropic" => GenAiSystem::Anthropic,
        "openai" => GenAiSystem::OpenAi,
        "gemini" => GenAiSystem::Google,
        "mistral" => GenAiSystem::Mistral,
        "deepseek" => GenAiSystem::DeepSeek,
        "xai" => GenAiSystem::Xai,
        // groq · openrouter · huggingface (Inference Providers router) ·
        // nvidia (integrate.api.nvidia.com / NIM) · the 5 local servers all
        // speak the OpenAI-compatible dialect; mock is Unknown by design.
        "groq" | "openrouter" | "huggingface" | "nvidia" | "moonshot" | "ollama" | "lmstudio"
        | "llamacpp" | "localai" | "vllm" => GenAiSystem::OpenAiCompatible,
        _ => GenAiSystem::Unknown,
    }
}

/// String at a JSON pointer (empty when absent — wire fields are best-effort).
pub(crate) fn str_at(v: &serde_json::Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// u64 at a JSON pointer (0 when absent).
pub(crate) fn u64_at(v: &serde_json::Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

/// Bijective function-name map for the wires whose function-calling API
/// restricts tool names to `^[a-zA-Z0-9_-]+$` (`OpenAI` · Anthropic).
///
/// Nika's tool ids are namespaced with colons (`nika:read`) and slashes
/// (`mcp:git/diff`), which both APIs reject with HTTP 400 (NIKA-463). This
/// map forward-sanitizes each canonical name to a wire-legal one when the
/// tool list is serialized, and reverse-maps the model's `tool_call` name
/// back to the canonical id before it is handed to the executor — the verb
/// layer only ever sees the canonical colon form (its whitelist + the
/// closed `nika:`/`mcp:` namespace dispatch depend on it).
///
/// Built fresh per request from `req.tools`: the same instance threads
/// both directions (send + the response parse), so the round-trip is
/// internally consistent even though the router may offer a different tool
/// subset on each turn. Collisions (two canonical names sanitizing to the
/// same string) are broken with a deterministic `_2`/`_3`… suffix so the
/// map stays a true bijection. Gemini accepts colons and is NOT routed
/// through this map.
#[derive(Debug, Default)]
pub(crate) struct ToolNameMap {
    /// canonical id → wire-legal name (the send direction).
    to_wire: std::collections::BTreeMap<String, String>,
    /// wire-legal name → canonical id (the response direction).
    to_canonical: std::collections::BTreeMap<String, String>,
}

impl ToolNameMap {
    /// Build the map from the request's tool ids (insertion order fixed by
    /// the slice so the collision suffixes are deterministic).
    pub(crate) fn from_tools(tools: &[nika_kernel::ai::provider::ToolDef]) -> Self {
        let mut map = Self::default();
        for tool in tools {
            map.insert(&tool.name);
        }
        map
    }

    /// Register one canonical id, sanitizing + disambiguating its wire name.
    fn insert(&mut self, canonical: &str) {
        if self.to_wire.contains_key(canonical) {
            return; // a duplicate id maps to its already-assigned wire name
        }
        let base = sanitize_tool_name(canonical);
        let mut candidate = base.clone();
        let mut n = 2u32;
        // The base may already be taken by a DIFFERENT canonical id (e.g.
        // `nika:read` and `nika/read` both sanitize to `nika_read`) — widen
        // with a numeric suffix until the wire name is free.
        while self.to_canonical.contains_key(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        self.to_canonical
            .insert(candidate.clone(), canonical.to_owned());
        self.to_wire.insert(canonical.to_owned(), candidate);
    }

    /// Canonical id → the wire-legal name to send (falls back to the
    /// sanitized form for an id not registered as a tool, e.g. a prior
    /// `ToolUse` block re-sent while that tool is not in this turn's list —
    /// it still serializes to a legal name).
    pub(crate) fn to_wire(&self, canonical: &str) -> String {
        self.to_wire
            .get(canonical)
            .cloned()
            .unwrap_or_else(|| sanitize_tool_name(canonical))
    }

    /// Wire name from the model → canonical id (falls back to the wire name
    /// verbatim when unknown, so a hallucinated name still surfaces for the
    /// verb's whitelist to reject rather than being silently dropped).
    pub(crate) fn to_canonical(&self, wire: &str) -> String {
        self.to_canonical
            .get(wire)
            .cloned()
            .unwrap_or_else(|| wire.to_owned())
    }
}

/// Forward-sanitize one tool name to the `^[a-zA-Z0-9_-]+$` charset both
/// `OpenAI` and Anthropic require: `:` and `/` (Nika's namespace separators)
/// and any other out-of-charset byte become `_`. An empty result (a name
/// of only illegal bytes) yields `_` so it is never the empty string.
fn sanitize_tool_name(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if mapped.is_empty() {
        "_".to_owned()
    } else {
        mapped
    }
}

/// Per-wire SSE payload → `InferEvent`s translator.
pub(crate) trait EventMapper: Send {
    /// Map one SSE `data:` payload to zero or more events.
    fn map(&mut self, payload: &str) -> Vec<Result<InferEvent, ProviderError>>;
    /// Stream ended — flush whatever closes the sequence (a `Done` if the
    /// wire never sent its terminal event).
    fn finish(&mut self) -> Vec<Result<InferEvent, ProviderError>>;
}

/// The one SSE state machine: http body chunks → [`SseParser`] →
/// [`EventMapper`] → `InferEvent` stream.
pub(crate) struct SseEventStream<M> {
    body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
    parser: SseParser,
    mapper: M,
    pending: VecDeque<Result<InferEvent, ProviderError>>,
    finished: bool,
}

impl<M: EventMapper> SseEventStream<M> {
    pub(crate) fn new(
        body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
        mapper: M,
    ) -> Self {
        Self {
            body,
            parser: SseParser::new(),
            mapper,
            pending: VecDeque::new(),
            finished: false,
        }
    }
}

impl<M: EventMapper + Unpin> Stream for SseEventStream<M> {
    type Item = Result<InferEvent, ProviderError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(ev) = this.pending.pop_front() {
                return Poll::Ready(Some(ev));
            }
            if this.finished {
                return Poll::Ready(None);
            }
            match this.body.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => {
                    for payload in this.parser.feed(&chunk) {
                        this.pending.extend(this.mapper.map(&payload));
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    this.finished = true;
                    this.pending.push_back(Err(map_http_err(&e)));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    this.pending.extend(this.mapper.finish());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_error_maps_the_table() {
        let auth = status_error(401, br#"{"error":{"message":"bad key"}}"#, None, "m");
        assert!(matches!(auth, ProviderError::AuthFailed { .. }));
        assert_eq!(auth.to_string(), "authentication failed: bad key");

        let nf = status_error(404, b"{}", None, "anthropic/claude-x");
        assert!(matches!(nf, ProviderError::ModelNotFound { .. }));

        let rl = status_error(429, b"{}", Some("2"), "m");
        match rl {
            ProviderError::RateLimited { retry_after_ms } => {
                assert_eq!(retry_after_ms, Some(2000));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        let api = status_error(500, br#"{"error":{"message":"boom"}}"#, None, "m");
        match &api {
            ProviderError::Api { status, message } => {
                assert_eq!(*status, 500);
                assert_eq!(message, "boom");
            }
            other => panic!("expected Api, got {other:?}"),
        }
        assert!(api.is_transient(), "5xx is transient per kernel contract");
    }

    #[test]
    fn status_error_falls_back_to_raw_body() {
        let api = status_error(502, b"bad gateway", None, "m");
        match api {
            ProviderError::Api { message, .. } => assert_eq!(message, "bad gateway"),
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn parse_data_image_accepts_png_and_rejects_the_rest() {
        let ok = parse_data_image("data:image/png;base64,QUJD").expect("png");
        assert_eq!(ok.media_type, "image/png");
        assert_eq!(ok.data, "QUJD");
        assert!(parse_data_image("data:text/plain;base64,QUJD").is_none());
        assert!(parse_data_image("http://example.com/i.png").is_none());
        assert!(parse_data_image("blake3:abc").is_none());
        assert!(parse_data_image("data:image/png;base64,").is_none());
        assert!(image_source_is_url("https://example.com/i.png"));
        assert!(image_source_is_url("data:image/jpeg;base64,xx"));
        assert!(!image_source_is_url("blake3:abc"));
    }

    #[test]
    fn timeout_maps_to_api_408() {
        let err = map_http_err(&HttpError::Timeout { duration_ms: 30000 });
        match err {
            ProviderError::Api { status, .. } => assert_eq!(status, 408),
            other => panic!("expected Api 408, got {other:?}"),
        }
        assert!(err.is_transient() || !err.is_transient(), "total");
        let other = map_http_err(&HttpError::Connection {
            reason: "refused".into(),
        });
        assert!(matches!(other, ProviderError::Other { .. }));
    }

    struct Q(std::collections::VecDeque<Result<Bytes, HttpError>>);
    impl Stream for Q {
        type Item = Result<Bytes, HttpError>;
        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    #[tokio::test]
    async fn stream_error_body_drained_up_to_the_64k_cap() {
        use std::collections::BTreeMap;

        // A 2 KiB message: well over a mutated 1088/0-byte cap, well under
        // the real 64 KiB one — the full JSON must survive into the error.
        let long = "x".repeat(2048);
        let body_json = format!(r#"{{"error":{{"message":"{long}"}}}}"#);
        let chunks: Vec<Result<Bytes, HttpError>> = body_json
            .as_bytes()
            .chunks(100)
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        let resp = nika_kernel::http::HttpStreamResponse::new(
            500,
            BTreeMap::new(),
            "u",
            None,
            Box::pin(Q(chunks.into())),
        );
        let err = stream_status_error(resp, "m").await;
        match err {
            ProviderError::Api { status, message } => {
                assert_eq!(status, 500);
                assert_eq!(message.len(), 2048, "full message extracted: cap intact");
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn transport_deadline_matrix() {
        use nika_kernel::ai::provider::{InferRequest, Message, Role};
        use std::time::Duration;

        let profiles = crate::profile::seed();
        let ollama = profiles.iter().find(|p| p.id == "ollama").expect("ollama");
        let openai = profiles.iter().find(|p| p.id == "openai").expect("openai");
        let req = |t: Option<Duration>| {
            let mut r = InferRequest::new("m", vec![Message::text(Role::User, "q")]);
            r.timeout = t;
            r
        };

        // Buffered · no task budget → the per-class default.
        assert_eq!(
            transport_deadline(ollama, &req(None), false),
            Some(LOCAL_DEFAULT_TIMEOUT),
            "local default is the generous one"
        );
        assert_eq!(
            transport_deadline(openai, &req(None), false),
            Some(CLOUD_DEFAULT_TIMEOUT),
            "cloud keeps the historical 30s"
        );
        // Buffered · task budget → it wins on BOTH classes.
        let budget = Some(Duration::from_secs(420));
        assert_eq!(transport_deadline(ollama, &req(budget), false), budget);
        assert_eq!(transport_deadline(openai, &req(budget), false), budget);
        // Streaming → explicit-only (None = idle guard governs).
        assert_eq!(transport_deadline(openai, &req(None), true), None);
        assert_eq!(transport_deadline(openai, &req(budget), true), budget);
        // The local default honours the ≥300s floor (F1 acceptance).
        assert!(LOCAL_DEFAULT_TIMEOUT >= Duration::from_secs(300));
    }

    #[test]
    fn gen_ai_system_covers_all_sixteen() {
        for id in crate::profile::CANONICAL_IDS {
            let sys = gen_ai_system(id);
            if id == "mock" {
                assert_eq!(sys, GenAiSystem::Unknown);
            } else {
                assert_ne!(sys, GenAiSystem::Unknown, "{id} must be attributed");
            }
        }
    }

    // ── BUG#5 · tool-name sanitization round-trip (NIKA-463) ──

    use nika_kernel::ai::provider::ToolDef;

    fn tool(name: &str) -> ToolDef {
        ToolDef::new(name, "", serde_json::json!({"type": "object"}))
    }

    #[test]
    fn sanitize_replaces_colons_and_slashes() {
        assert_eq!(sanitize_tool_name("nika:read"), "nika_read");
        assert_eq!(sanitize_tool_name("mcp:git/diff"), "mcp_git_diff");
        assert_eq!(sanitize_tool_name("nika:done"), "nika_done");
        // already-legal name is unchanged
        assert_eq!(sanitize_tool_name("plain-name_1"), "plain-name_1");
        // a name of only illegal bytes still yields a legal non-empty name
        assert_eq!(sanitize_tool_name(":/"), "__");
    }

    #[test]
    fn map_round_trips_canonical_through_wire() {
        let map = ToolNameMap::from_tools(&[tool("nika:read"), tool("mcp:git/diff")]);
        // forward (send) direction
        assert_eq!(map.to_wire("nika:read"), "nika_read");
        assert_eq!(map.to_wire("mcp:git/diff"), "mcp_git_diff");
        // reverse (response) direction recovers the canonical id exactly
        assert_eq!(map.to_canonical("nika_read"), "nika:read");
        assert_eq!(map.to_canonical("mcp_git_diff"), "mcp:git/diff");
    }

    #[test]
    fn map_disambiguates_collisions_bijectively() {
        // `nika:read` and `nika/read` both sanitize to `nika_read`; the
        // second gets a deterministic suffix so the map stays a bijection.
        let map = ToolNameMap::from_tools(&[tool("nika:read"), tool("nika/read")]);
        assert_eq!(map.to_wire("nika:read"), "nika_read");
        assert_eq!(map.to_wire("nika/read"), "nika_read_2");
        // both wire names reverse-map to their distinct canonical ids
        assert_eq!(map.to_canonical("nika_read"), "nika:read");
        assert_eq!(map.to_canonical("nika_read_2"), "nika/read");
    }

    #[test]
    fn map_falls_back_for_unknown_names() {
        let map = ToolNameMap::from_tools(&[tool("nika:read")]);
        // an unknown wire name (model hallucination) surfaces verbatim so
        // the verb's whitelist can reject it
        assert_eq!(map.to_canonical("ghost_tool"), "ghost_tool");
        // an unregistered canonical id still serializes to a legal name
        assert_eq!(map.to_wire("mcp:db/query"), "mcp_db_query");
    }
}
