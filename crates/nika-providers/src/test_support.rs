// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

/// Canned-response http effect (one response/stream per instance · captures
/// every request it sees).
pub(crate) struct FakeHttp {
    json: Mutex<VecDeque<(u16, String)>>,
    stream: Mutex<Option<(u16, Vec<Bytes>)>>,
    captured: Mutex<Vec<HttpRequest>>,
}

impl FakeHttp {
    /// One JSON response with the given status.
    pub(crate) fn with_json(status: u16, body: &str) -> Arc<Self> {
        Arc::new(Self {
            json: Mutex::new(VecDeque::from([(status, body.to_owned())])),
            stream: Mutex::new(None),
            captured: Mutex::new(Vec::new()),
        })
    }

    /// One SSE stream, chopped into `chunk_size`-byte chunks (exercises the
    /// incremental parser across arbitrary boundaries).
    pub(crate) fn with_stream(status: u16, sse: &str, chunk_size: usize) -> Arc<Self> {
        let chunks = sse
            .as_bytes()
            .chunks(chunk_size.max(1))
            .map(Bytes::copy_from_slice)
            .collect();
        Arc::new(Self {
            json: Mutex::new(VecDeque::new()),
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
        let (status, body) = next.ok_or_else(|| HttpError::Other {
            reason: "FakeHttp: no canned response queued".to_owned(),
        })?;
        Ok(HttpResponse::new(
            status,
            BTreeMap::new(),
            Bytes::from(body),
            request.url,
        ))
    }

    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        self.record(&request);
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

struct ChunkStream(VecDeque<Result<Bytes, HttpError>>);

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
