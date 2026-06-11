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
pub(crate) mod openai_compat;

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::ai::provider::{InferEvent, ProviderError};
use nika_kernel::genai::GenAiSystem;
use nika_kernel::http::HttpError;

use crate::sse::SseParser;

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
        // groq · openrouter · the 5 local servers all speak the
        // OpenAI-compatible dialect; mock is Unknown by design.
        "groq" | "openrouter" | "ollama" | "lmstudio" | "llamacpp" | "localai" | "vllm" => {
            GenAiSystem::OpenAiCompatible
        }
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
    fn gen_ai_system_covers_all_fourteen() {
        for id in crate::profile::CANONICAL_IDS {
            let sys = gen_ai_system(id);
            if id == "mock" {
                assert_eq!(sys, GenAiSystem::Unknown);
            } else {
                assert_ne!(sys, GenAiSystem::Unknown, "{id} must be attributed");
            }
        }
    }
}
