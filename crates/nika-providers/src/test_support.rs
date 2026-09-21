// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Test doubles — a canned-response http effect + stream collectors.
//!
//! The dividend of the kernel http seam: wire adapters are tested against
//! recorded fixtures with zero network and zero extra dev-dependencies.

use std::collections::{BTreeMap, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::ai::provider::{InferEvent, InferEventStream, ProviderError};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;

use crate::registry::{ProviderRegistry, ProvidersConfig, ResolvedProvider};

/// One canned JSON answer: status · body · response headers (lowercase
/// names, as the production client normalizes them).
pub(crate) type CannedJson = (u16, String, BTreeMap<String, String>);

/// One answer as a test writes it: status · body · headers.
pub(crate) type Answer<'a> = (u16, &'a str, &'a [(&'a str, &'a str)]);

/// Canned-response http effect (one response/stream per instance · captures
/// every request it sees).
pub(crate) struct FakeHttp {
    json: Mutex<VecDeque<CannedJson>>,
    stream: Mutex<Option<(u16, Vec<Bytes>)>>,
    captured: Mutex<Vec<HttpRequest>>,
}

impl FakeHttp {
    /// One JSON response with the given status.
    pub(crate) fn with_json(status: u16, body: &str) -> Arc<Self> {
        Self::with_sequence(&[(status, body, &[])])
    }

    /// Several JSON responses served in order, each with its own status
    /// and headers — the shape a transport-retry test needs (a 429 with
    /// `retry-after`, then the 200).
    pub(crate) fn with_sequence(answers: &[Answer<'_>]) -> Arc<Self> {
        let json = answers
            .iter()
            .map(|(status, body, headers)| {
                let headers = headers
                    .iter()
                    .map(|(k, v)| (k.to_ascii_lowercase(), (*v).to_owned()))
                    .collect();
                (*status, (*body).to_owned(), headers)
            })
            .collect();
        Arc::new(Self {
            json: Mutex::new(json),
            stream: Mutex::new(None),
            captured: Mutex::new(Vec::new()),
        })
    }

    /// One SSE stream, chopped into `chunk_size`-byte chunks (exercises the
    /// incremental parser across arbitrary boundaries).
    pub(crate) fn with_stream(status: u16, sse: &str, chunk_size: usize) -> Arc<Self> {
        Self::with_refusals_then_stream(&[], status, sse, chunk_size)
    }

    /// Non-2xx answers served on the streaming door first (each refuses
    /// the open with its status · body · headers), then the one SSE stream.
    pub(crate) fn with_refusals_then_stream(
        refusals: &[Answer<'_>],
        status: u16,
        sse: &str,
        chunk_size: usize,
    ) -> Arc<Self> {
        let chunks = sse
            .as_bytes()
            .chunks(chunk_size.max(1))
            .map(Bytes::copy_from_slice)
            .collect();
        let json = refusals
            .iter()
            .map(|(status, body, headers)| {
                let headers = headers
                    .iter()
                    .map(|(k, v)| (k.to_ascii_lowercase(), (*v).to_owned()))
                    .collect();
                (*status, (*body).to_owned(), headers)
            })
            .collect();
        Arc::new(Self {
            json: Mutex::new(json),
            stream: Mutex::new(Some((status, chunks))),
            captured: Mutex::new(Vec::new()),
        })
    }

    /// Every request this effect served, in order.
    pub(crate) fn captured(&self) -> Vec<HttpRequest> {
        self.captured.lock().map(|c| c.clone()).unwrap_or_default()
    }

    fn record(&self, request: &HttpRequest) {
        if let Ok(mut c) = self.captured.lock() {
            c.push(request.clone());
        }
    }
}

impl HttpPostDyn for FakeHttp {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.record(&request);
        let next = self.json.lock().ok().and_then(|mut q| q.pop_front());
        let (status, body, headers) = next.ok_or_else(|| HttpError::Other {
            reason: "FakeHttp: no canned response queued".to_owned(),
        })?;
        Ok(HttpResponse::new(
            status,
            headers,
            Bytes::from(body),
            request.url,
        ))
    }

    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        self.record(&request);
        // A queued NON-2xx JSON answer serves the streaming door too: the
        // open is refused with that status (the shape a 429 on an SSE open
        // takes), before the canned stream is reached.
        let refused = self.json.lock().ok().and_then(|mut q| {
            if q.front()
                .is_some_and(|(status, _, _)| !(200..300).contains(status))
            {
                q.pop_front()
            } else {
                None
            }
        });
        if let Some((status, body, headers)) = refused {
            let chunks: VecDeque<Result<Bytes, HttpError>> =
                VecDeque::from([Ok(Bytes::from(body))]);
            return Ok(HttpStreamResponse::new(
                status,
                headers,
                request.url,
                None,
                Box::pin(ChunkStream(chunks)),
            ));
        }
        let next = self.stream.lock().ok().and_then(|mut s| s.take());
        let (status, chunks) = next.ok_or_else(|| HttpError::Other {
            reason: "FakeHttp: no canned stream queued".to_owned(),
        })?;
        let body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>> =
            Box::pin(ChunkStream(chunks.into_iter().map(Ok).collect()));
        Ok(HttpStreamResponse::new(
            status,
            BTreeMap::new(),
            request.url,
            None,
            body,
        ))
    }
}

pub(crate) struct ChunkStream(pub(crate) VecDeque<Result<Bytes, HttpError>>);

impl Stream for ChunkStream {
    type Item = Result<Bytes, HttpError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front())
    }
}

/// Resolve a provider against a `FakeHttp` with an explicitly injected key
/// (the registry never reads env). `provider` is an id (default test model
/// appended) or a full `provider/model` string.
pub(crate) fn resolved_with(
    fake: &Arc<FakeHttp>,
    provider: &str,
    key: &str,
) -> ResolvedProvider<FakeHttp> {
    resolved_with_backoff(fake, provider, key, crate::retry::system_backoff())
}

/// A sleep seam that records every wait and never sleeps — the transport
/// backoff's test clock.
#[derive(Debug, Default)]
pub(crate) struct RecordingBackoff {
    waits: Mutex<Vec<std::time::Duration>>,
}

impl RecordingBackoff {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every wait the backoff asked for, in order.
    pub(crate) fn waits(&self) -> Vec<std::time::Duration> {
        self.waits.lock().map(|w| w.clone()).unwrap_or_default()
    }
}

impl crate::retry::Backoff for RecordingBackoff {
    fn sleep(
        &self,
        duration: std::time::Duration,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        if let Ok(mut waits) = self.waits.lock() {
            waits.push(duration);
        }
        Box::pin(std::future::ready(()))
    }
}

/// [`resolved_with`] over an injected backoff seam.
pub(crate) fn resolved_with_backoff(
    fake: &Arc<FakeHttp>,
    provider: &str,
    key: &str,
    backoff: Arc<dyn crate::retry::Backoff>,
) -> ResolvedProvider<FakeHttp> {
    let model = if provider.contains('/') {
        provider.to_owned()
    } else if provider == "anthropic" {
        "anthropic/claude-sonnet-4-20250514".to_owned()
    } else {
        format!("{provider}/test-model")
    };
    let provider_id = model.split('/').next().unwrap_or(provider).to_owned();
    let mut config = ProvidersConfig::new();
    if !key.is_empty() {
        config = config.with_key(provider_id, Secret::new(key));
    }
    ProviderRegistry::new(Arc::clone(fake), config)
        .with_backoff(backoff)
        .resolve(&model)
        .expect("test provider resolves")
}

/// Drain an `InferEventStream` to completion.
pub(crate) async fn collect(
    mut stream: InferEventStream,
) -> Vec<Result<InferEvent, ProviderError>> {
    let mut out = Vec::new();
    while let Some(ev) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
        out.push(ev);
    }
    out
}
