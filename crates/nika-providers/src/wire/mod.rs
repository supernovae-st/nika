// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wire adapters — one module per protocol family.
//!
//! Shared here: HTTP→Provider error mapping, the SSE→`InferEvent` stream
//! wrapper (one state machine, per-wire `EventMapper`s), and the
//! `GenAiSystem` attribution table.

pub(crate) mod anthropic;
#[cfg(test)]
mod error_tests;
pub(crate) mod gemini;
pub(crate) mod json_mode;
pub(crate) mod mock;
mod mock_schema;
mod ollama;
pub(crate) mod openai_compat;
#[cfg(test)]
mod openai_compat_usage_tests;
mod openai_schema;
pub(crate) mod reasoning;
#[cfg(test)]
mod refusal_tests;

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::ai::provider::{InferEvent, ProviderError, ProviderHttpError};
use nika_kernel::genai::GenAiSystem;
use nika_kernel::http::HttpError;
use nika_kernel::secret::Secret;

use crate::sse::SseParser;

/// The total deadline of a BUFFERED provider call whose task declares no `timeout:`, local or
/// cloud alike: ten minutes, the provider transport's own bound on a connection that delivers
/// nothing (the provider client's idle-read guard). A buffered answer arrives whole at the end,
/// so no shorter implicit deadline can tell a slow legitimate call (a reasoning model, a long
/// prompt) from a stalled one: the former 30 s cloud default cut legitimate summaries at 30.0 s,
/// as the former 300 s local one cut slow local models. A task `timeout:` sets its own bound.
pub(crate) const BUFFERED_DEFAULT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(600);

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
/// (plumbed via `InferRequest::timeout`) when declared, else
/// [`BUFFERED_DEFAULT_TIMEOUT`] for every provider. STREAMING requests carry
/// only an EXPLICIT task timeout (else `None`): an SSE generation legitimately
/// outlives any fixed total budget — the http effect's idle-read guard reaps a
/// STALLED stream instead (`nika-http` streaming timeout semantics).
pub(crate) fn transport_deadline(
    req: &nika_kernel::ai::provider::InferRequest,
    stream: bool,
) -> Option<std::time::Duration> {
    if stream {
        return req.timeout;
    }
    Some(req.timeout.unwrap_or(BUFFERED_DEFAULT_TIMEOUT))
}

/// Transport-layer failure → provider error (no HTTP status yet).
pub(crate) fn map_http_err(e: &HttpError) -> ProviderError {
    match e {
        HttpError::Connection { reason } => ProviderError::Connection {
            reason: format!(
                "{reason}; check the provider endpoint and service, then use an authored \
                 retry.max_attempts with bounded backoff if appropriate. The provider may \
                 have generated or billed tokens before the interruption; missing usage \
                 is unknown, not zero. For an offline rehearsal, choose mock/echo."
            ),
        },
        HttpError::Timeout { .. } => ProviderError::Api {
            status: 408,
            message: format!(
                "{e}: the task's timeout: when it states one, else the {}s a buffered call \
                 is given, local or cloud (the provider transport also closes a connection \
                 silent that long). A buffered answer arrives whole at the end: to bound it \
                 otherwise, set timeout: on the task (next to infer:), e.g. timeout: 7m; a \
                 smaller non-reasoning model answers sooner. Streaming has an idle-read \
                 guard, not this total deadline.",
                BUFFERED_DEFAULT_TIMEOUT.as_secs(),
            ),
        },
        _ => ProviderError::Other {
            reason: e.to_string(),
        },
    }
}

/// Non-2xx on a streaming open: drain the (effect-capped) error body so the
/// provider's safe identifiers + retry-after (and its message, as the kernel
/// reduces it) survive into the same typed mapping as the non-streaming path.
pub(crate) async fn stream_status_error(
    resp: nika_kernel::http::HttpStreamResponse,
    key: Option<&Secret>,
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
        key,
    )
}

/// Non-2xx status + body → sanitized metadata. Do not retain raw bodies,
/// request identifiers, credentials, or arbitrary identifier-shaped strings:
/// the provider's own message reaches the person only as the kernel reduces it,
/// with `key` (the credential the call sent) withheld, and nothing classifies it.
pub(crate) fn status_error(
    status: u16,
    body: &[u8],
    retry_after: Option<&str>,
    key: Option<&Secret>,
) -> ProviderError {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok();
    let field = |name| value.as_ref()?.get("error")?.get(name)?.as_str();
    let top = |name| value.as_ref()?.get(name)?.as_str();
    // The Gemini API names its delay in the BODY (`google.rpc.RetryInfo`
    // · `error.details[].retryDelay = "39s"`), not in a header — the
    // backoff reads it through the same bounded parser as `Retry-After`.
    // Its `status` (`RESOURCE_EXHAUSTED`) stands in for a `type` only when
    // the body carries no `type`; the closed vocabulary still decides
    // what survives.
    let body_delay = value.as_ref().and_then(google_retry_delay);
    let retry_after = retry_after.or(body_delay.as_deref());
    // HTTP 402 is a billing refusal by status alone: filed under the existing
    // `credit_balance_exhausted` identifier. A provider that answers an
    // exhausted balance with a 400 and the reason in prose (Anthropic) stays
    // an ordinary 400 here: prose is never classified (the hostile-body law),
    // only relayed for the person to read; the infer verb names both readings.
    let code = if status == 402 {
        Some("credit_balance_exhausted")
    } else {
        field("code")
    };
    let details = ProviderHttpError::new(
        status,
        code,
        field("type").or_else(|| field("status")),
        retry_after,
    );
    // `error.message` (most wires), else a bare `error`, `message` or `detail` string.
    let message = field("message")
        .or_else(|| top("error"))
        .or_else(|| top("message"))
        .or_else(|| top("detail"));
    let withheld = key.map(Secret::expose);
    ProviderError::HttpResponse {
        details: match message {
            Some(message) => details.with_message(message, withheld.as_slice()),
            None => details,
        },
    }
}

/// `error.details[].retryDelay` in Google's `<seconds>s` form, as the
/// bare seconds the sanitized parser accepts.
fn google_retry_delay(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")?
        .get("details")?
        .as_array()?
        .iter()
        .find_map(|detail| {
            detail
                .get("retryDelay")?
                .as_str()?
                .strip_suffix('s')
                .map(str::to_owned)
        })
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
        "groq" | "openrouter" | "huggingface" | "nvidia" | "moonshot" | "scaleway" | "ollama"
        | "lmstudio" | "llamacpp" | "localai" | "vllm" => GenAiSystem::OpenAiCompatible,
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
        use nika_kernel::prelude::NikaErrorCode;
        for (status, code, transient) in [
            (401, 333, false),
            (403, 333, false),
            (404, 331, false),
            (429, 332, true),
            (500, 330, true),
            (400, 330, false),
        ] {
            let error = status_error(
                status,
                br#"{"error":{"message":"refused"}}"#,
                Some("2"),
                None,
            );
            assert_eq!(error.nika_code().num, code);
            assert_eq!(error.is_transient(), transient);
            let ProviderError::HttpResponse { details } = &error else {
                panic!("{error:?}")
            };
            assert_eq!(details.status(), status);
            assert_eq!(details.retry_after_ms(), Some(2000));
            let shown = error.to_string();
            assert!(shown.contains("the provider said: \"refused\""), "{shown}");
        }
        let auth = status_error(401, b"{}", None, None);
        assert!(auth.to_string().contains("does not probe present keys"));
    }

    /// HTTP 402 is a billing refusal by status: the credit class, no retry,
    /// the label names the top-up. A 400 that carries the reason only in prose
    /// (Anthropic's exhausted balance) stays an ordinary 400, since prose is
    /// never classified; the person reads the provider's own words, not the key.
    #[test]
    fn a_payment_required_status_is_billing_by_status_alone() {
        let body = br#"{"error":{"message":"Insufficient Balance"}}"#;
        let error = status_error(402, body, None, None);
        let ProviderError::HttpResponse { details } = &error else {
            panic!("{error:?}")
        };
        assert!(details.is_quota_exhausted(), "{error}");
        assert!(!error.is_transient(), "no retry pays a bill");
        let text = error.to_string();
        assert!(text.contains("quota exhausted (credit balance)"), "{text}");
        assert!(text.contains("top up"), "{text}");
        assert!(text.contains("said: \"Insufficient Balance\""), "{text}");
        let body = br#"{"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API with sk-ant-test."}}"#;
        let key = Secret::new("sk-ant-test");
        let plain = status_error(400, body, None, Some(&key));
        let ProviderError::HttpResponse { details } = &plain else {
            panic!("{plain:?}")
        };
        assert!(!details.is_quota_exhausted(), "prose is never classified");
        assert_eq!(
            details.message(),
            Some("Your credit balance is too low to access the Anthropic API with [withheld].")
        );
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
        for duration_ms in [30_000, 420_000, 600_000] {
            let err = map_http_err(&HttpError::Timeout { duration_ms });
            match &err {
                ProviderError::Api { status, message } => {
                    assert_eq!(*status, 408);
                    assert!(message.contains(&format!("{duration_ms}ms")), "{message}");
                    // The deadline is named for what it is: the task's own, else the one
                    // buffered default; no class default is claimed any more.
                    assert!(message.contains("the task's timeout: when it states one"));
                    assert!(message.contains("600s a buffered call"), "{message}");
                    assert!(!message.contains("30s cloud"), "{message}");
                    assert!(message.contains("timeout: 7m"), "{message}");
                    assert!(message.contains("next to infer:"), "{message}");
                    assert!(message.contains("smaller non-reasoning"), "{message}");
                }
                other => panic!("expected Api 408, got {other:?}"),
            }
            assert!(
                !err.is_transient(),
                "408 retains its existing terminal classification"
            );
        }
        let other = map_http_err(&HttpError::Connection {
            reason: "refused".into(),
        });
        assert!(matches!(other, ProviderError::Connection { .. }));
        assert!(other.is_transient());
    }

    pub(super) struct Q(pub(super) std::collections::VecDeque<Result<Bytes, HttpError>>);
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
        // the real 64 KiB one — metadata after the message must survive.
        let long = "x".repeat(2048);
        let body_json = format!(r#"{{"error":{{"message":"{long}","code":"server_error"}}}}"#);
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
        let err = stream_status_error(resp, None).await;
        match err {
            ProviderError::HttpResponse { details } => {
                assert_eq!(details.status(), 500);
                assert_eq!(
                    details.code(),
                    Some("server_error"),
                    "full JSON parsed: cap intact"
                );
                assert!(!details.to_string().contains(&long));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn transport_deadline_matrix() {
        use nika_kernel::ai::provider::{InferRequest, Message, Role};
        use std::time::Duration;

        let req = |t: Option<Duration>| {
            let mut r = InferRequest::new("m", vec![Message::text(Role::User, "q")]);
            r.timeout = t;
            r
        };

        // Buffered · no task budget → the one buffered default, local or cloud.
        assert_eq!(
            transport_deadline(&req(None), false),
            Some(BUFFERED_DEFAULT_TIMEOUT)
        );
        // Buffered · task budget → it wins.
        let budget = Some(Duration::from_secs(420));
        assert_eq!(transport_deadline(&req(budget), false), budget);
        // Streaming → explicit-only (None = idle guard governs).
        assert_eq!(transport_deadline(&req(None), true), None);
        assert_eq!(transport_deadline(&req(budget), true), budget);
        // The provider transport's own bound on a silent connection, never a shorter guess.
        assert_eq!(BUFFERED_DEFAULT_TIMEOUT, Duration::from_secs(600));
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

mod admission;

pub(crate) mod bounded_json;
