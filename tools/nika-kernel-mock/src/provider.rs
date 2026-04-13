// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockProvider` — deterministic LLM responses with call recording.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use parking_lot::Mutex;

use nika_kernel::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, ProviderError,
    ProviderInfer, ProviderMeta, ProviderStream, StopReason, TokenUsage,
};

/// Programmable LLM provider for tests.
///
/// Enqueue responses with `enqueue_text` / `enqueue_response` / `enqueue_error`.
/// Inspect recorded requests with `captured_requests`.
#[derive(Clone)]
pub struct MockProvider {
    name: Arc<str>,
    responses: Arc<Mutex<VecDeque<Result<InferResponse, ProviderError>>>>,
    requests: Arc<Mutex<Vec<InferRequest>>>,
    supports_response_format: Arc<Mutex<bool>>,
}

impl MockProvider {
    /// Create a new mock provider with the given name.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
            supports_response_format: Arc::new(Mutex::new(false)),
        }
    }

    /// Enqueue a simple text response (builder).
    #[must_use]
    pub fn enqueue_text(self, text: impl Into<String>) -> Self {
        let resp = InferResponse::new(
            vec![ContentBlock::Text { text: text.into() }],
            TokenUsage::new(10, 5),
            StopReason::EndTurn,
        );
        self.responses.lock().push_back(Ok(resp));
        self
    }

    /// Enqueue a full response (builder).
    #[must_use]
    pub fn enqueue_response(self, response: InferResponse) -> Self {
        self.responses.lock().push_back(Ok(response));
        self
    }

    /// Enqueue an error (builder).
    #[must_use]
    pub fn enqueue_error(self, error: ProviderError) -> Self {
        self.responses.lock().push_back(Err(error));
        self
    }

    /// Set whether this provider supports `response_format` (builder).
    #[must_use]
    pub fn with_response_format_support(self, enabled: bool) -> Self {
        *self.supports_response_format.lock() = enabled;
        self
    }

    /// Get all recorded requests.
    #[must_use]
    pub fn captured_requests(&self) -> Vec<InferRequest> {
        self.requests.lock().clone()
    }

    fn pop_response(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().push(request);
        self.responses
            .lock()
            .pop_front()
            .unwrap_or_else(|| {
                Err(ProviderError::Other {
                    reason: "MockProvider: response queue exhausted".into(),
                })
            })
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new("mock")
    }
}

impl ProviderInfer for MockProvider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.pop_response(request)
    }
}

impl ProviderStream for MockProvider {
    async fn infer_stream(
        &self,
        request: InferRequest,
    ) -> Result<InferEventStream, ProviderError> {
        let response = self.pop_response(request)?;
        // Synthesize a stream from the canned response.
        let mut events = Vec::new();
        for block in &response.content {
            if let ContentBlock::Text { text } = block {
                events.push(Ok(InferEvent::Delta {
                    text: text.clone(),
                }));
            }
        }
        events.push(Ok(InferEvent::Usage(response.usage.clone())));
        events.push(Ok(InferEvent::Done {
            stop_reason: response.stop_reason.clone(),
            request_id: response.request_id.clone(),
            finish_reason_raw: response.finish_reason_raw.clone(),
        }));
        Ok(Box::pin(VecStream(events.into())))
    }
}

impl ProviderMeta for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn supports_response_format(&self) -> bool {
        *self.supports_response_format.lock()
    }
}

/// Simple stream adapter: yields items from a `VecDeque` in order.
struct VecStream<T>(VecDeque<T>);

impl<T: Unpin> futures_core::Stream for VecStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(self.0.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_core::Stream;

    // Helper to collect a stream to vec.
    async fn collect_stream<S: Stream + Unpin>(mut stream: S) -> Vec<S::Item> {
        let mut items = Vec::new();
        while let Some(item) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            items.push(item);
        }
        items
    }

    #[tokio::test]
    async fn enqueue_text_and_infer() {
        let provider = MockProvider::new("test").enqueue_text("hello world");
        let req = InferRequest::new("model", vec![]);
        let resp = provider.infer(req).await.unwrap();
        assert_eq!(resp.content.len(), 1);
        assert!(matches!(&resp.content[0], ContentBlock::Text { text } if text == "hello world"));
    }

    #[tokio::test]
    async fn fifo_ordering() {
        let provider = MockProvider::new("test")
            .enqueue_text("first")
            .enqueue_text("second");
        let r1 = provider.infer(InferRequest::new("m", vec![])).await.unwrap();
        let r2 = provider.infer(InferRequest::new("m", vec![])).await.unwrap();
        assert!(matches!(&r1.content[0], ContentBlock::Text { text } if text == "first"));
        assert!(matches!(&r2.content[0], ContentBlock::Text { text } if text == "second"));
    }

    #[tokio::test]
    async fn empty_queue_returns_error() {
        let provider = MockProvider::new("test");
        let result = provider.infer(InferRequest::new("m", vec![])).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn records_requests() {
        let provider = MockProvider::new("test").enqueue_text("x");
        let _ = provider.infer(InferRequest::new("claude-sonnet", vec![])).await;
        let captured = provider.captured_requests();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].model, "claude-sonnet");
    }

    #[tokio::test]
    async fn stream_synthesizes_events() {
        let provider = MockProvider::new("test").enqueue_text("hello");
        let stream = provider
            .infer_stream(InferRequest::new("m", vec![]))
            .await
            .unwrap();
        let events = collect_stream(stream).await;
        // Should have: Delta("hello"), Usage, Done
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], Ok(InferEvent::Delta { text }) if text == "hello"));
        assert!(matches!(&events[1], Ok(InferEvent::Usage(_))));
        assert!(matches!(&events[2], Ok(InferEvent::Done { .. })));
    }

    #[test]
    fn name_returns_configured_name() {
        let provider = MockProvider::new("anthropic");
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn supports_response_format_default_false() {
        let provider = MockProvider::new("test");
        assert!(!provider.supports_response_format());
    }

    #[test]
    fn supports_response_format_configurable() {
        let provider = MockProvider::new("test").with_response_format_support(true);
        assert!(provider.supports_response_format());
    }

    #[tokio::test]
    async fn enqueue_response_populates_queue() {
        let custom = InferResponse::new(
            vec![ContentBlock::Text { text: "custom content".into() }],
            TokenUsage::new(20, 10),
            StopReason::EndTurn,
        );
        let provider = MockProvider::new("test").enqueue_response(custom);
        let resp = provider.infer(InferRequest::new("m", vec![])).await.unwrap();
        assert_eq!(resp.content.len(), 1);
        assert!(matches!(&resp.content[0], ContentBlock::Text { text } if text == "custom content"));
        assert_eq!(resp.usage.input_tokens, 20);
        assert_eq!(resp.usage.output_tokens, 10);
    }

    #[tokio::test]
    async fn enqueue_error_populates_queue() {
        let provider = MockProvider::new("test")
            .enqueue_error(ProviderError::ModelNotFound { model: "gpt-9".into() });
        let result = provider.infer(InferRequest::new("m", vec![])).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::ModelNotFound { model } if model == "gpt-9"));
    }

    #[test]
    fn satisfies_provider_trait() {
        fn _accepts<T: nika_kernel::Provider>(_: &T) {}
        let p = MockProvider::new("test");
        _accepts(&p);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_provider_is_send_sync() {
        _assert_send_sync::<MockProvider>();
    }
}
