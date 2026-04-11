// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MockProvider — programmable LLM provider for verb-crate unit tests.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use nika_kernel::provider::{
    ContentBlock, InferEvent, InferRequest, InferResponse, InferStream, ModelCapabilities,
    Provider, ProviderError, StopReason, TokenUsage,
};

/// Mock provider with a programmable response queue.
///
/// Responses are consumed in FIFO order. When the queue is empty,
/// returns [`ProviderError::Other`] with reason `"mock queue empty"`.
///
/// Records every `InferRequest` received so tests can assert on the
/// exact payload sent by the verb crate.
#[derive(Clone, Default)]
pub struct MockProvider {
    name: Arc<str>,
    responses: Arc<Mutex<VecDeque<Result<InferResponse, ProviderError>>>>,
    requests: Arc<Mutex<Vec<InferRequest>>>,
    supports_response_format: bool,
    capabilities: Option<ModelCapabilities>,
}

impl MockProvider {
    /// Create a new mock provider with the given name.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
            supports_response_format: false,
            capabilities: None,
        }
    }

    /// Enqueue a successful text-only response.
    pub fn enqueue_text(&self, text: impl Into<String>) {
        let response = InferResponse::new(
            vec![ContentBlock::Text { text: text.into() }],
            TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                ..Default::default()
            },
            StopReason::EndTurn,
        );
        self.responses.lock().push_back(Ok(response));
    }

    /// Enqueue a full `InferResponse` (for tests that need specific
    /// token counts, stop reasons, or content blocks).
    pub fn enqueue_response(&self, response: InferResponse) {
        self.responses.lock().push_back(Ok(response));
    }

    /// Enqueue an error.
    pub fn enqueue_error(&self, error: ProviderError) {
        self.responses.lock().push_back(Err(error));
    }

    /// Return a snapshot of every request seen so far.
    pub fn captured_requests(&self) -> Vec<InferRequest> {
        self.requests.lock().clone()
    }

    /// Toggle the `supports_response_format` capability.
    pub fn with_response_format_support(mut self, enabled: bool) -> Self {
        self.supports_response_format = enabled;
        self
    }

    /// Set the capabilities returned by `capabilities(_)`.
    pub fn with_capabilities(mut self, caps: ModelCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self, _model: &str) -> Option<ModelCapabilities> {
        self.capabilities.clone()
    }

    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().push(request);
        self.responses
            .lock()
            .pop_front()
            .unwrap_or_else(|| {
                Err(ProviderError::Other {
                    reason: "mock queue empty".to_string(),
                })
            })
    }

    async fn infer_stream(&self, request: InferRequest) -> Result<InferStream, ProviderError> {
        self.requests.lock().push(request);
        let maybe_response = self.responses.lock().pop_front();
        match maybe_response {
            Some(Ok(response)) => {
                // Synthesize a stream from the static response: one Delta
                // per text block, one Usage event, one Done event.
                let mut events: Vec<Result<InferEvent, ProviderError>> = Vec::new();
                for block in &response.content {
                    if let ContentBlock::Text { text } = block {
                        events.push(Ok(InferEvent::Delta(text.clone())));
                    }
                }
                events.push(Ok(InferEvent::Usage(response.usage.clone())));
                events.push(Ok(InferEvent::Done {
                    stop_reason: response.stop_reason.clone(),
                    request_id: response.request_id.clone(),
                    // W16-B3: propagate the finish_reason_raw the caller
                    // set on the enqueued InferResponse so tests can drive
                    // the verb-crate mapping through the streaming path
                    // symmetrically with the non-streaming path.
                    finish_reason_raw: response.finish_reason_raw.clone(),
                }));
                Ok(Box::pin(futures_util::stream::iter(events)))
            }
            Some(Err(e)) => Err(e),
            None => Err(ProviderError::Other {
                reason: "mock queue empty".to_string(),
            }),
        }
    }

    fn supports_response_format(&self) -> bool {
        self.supports_response_format
    }
}

// S16-swarm clippy sweep: same rationale as nika-kernel::builtin's
// test module — panic-helper `_ => panic!(…)` wildcards deliberately
// assert "anything BUT the target variant is a failure".
#[cfg(test)]
#[allow(clippy::wildcard_enum_match_arm)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_returns_enqueued_text() {
        let mock = MockProvider::new("test");
        mock.enqueue_text("hello world");

        let request = InferRequest {
            model: "test-model".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: Vec::new(),
            thinking_budget: None,
            extra: Default::default(),
        };

        let response = mock.infer(request).await.unwrap();
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello world"),
            other => panic!("expected Text, got {other:?}"),
        }
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[tokio::test]
    async fn mock_provider_records_requests() {
        let mock = MockProvider::new("test");
        mock.enqueue_text("ok");

        let request = InferRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: Vec::new(),
            temperature: Some(0.5),
            max_tokens: Some(1000),
            tools: Vec::new(),
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: Vec::new(),
            thinking_budget: None,
            extra: Default::default(),
        };

        mock.infer(request).await.unwrap();
        let captured = mock.captured_requests();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].model, "claude-sonnet-4-6");
        assert_eq!(captured[0].temperature, Some(0.5));
    }

    #[tokio::test]
    async fn mock_provider_returns_error_when_empty() {
        let mock = MockProvider::new("test");
        let request = InferRequest {
            model: "x".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: Vec::new(),
            thinking_budget: None,
            extra: Default::default(),
        };

        let result = mock.infer(request).await;
        assert!(matches!(result, Err(ProviderError::Other { .. })));
    }

    #[test]
    fn mock_provider_supports_response_format_toggle() {
        let mock = MockProvider::new("test");
        assert!(!mock.supports_response_format());

        let mock = mock.with_response_format_support(true);
        assert!(mock.supports_response_format());
    }

    /// **S14-α** — stream's terminal `Done` event must carry the struct-variant
    /// fields (`stop_reason`, `request_id`, `finish_reason_raw`) so the verb
    /// crate can thread them into `ProviderResponded` without reaching into
    /// provider concretes.
    ///
    /// When a response was enqueued with a `request_id` set, the mock must
    /// propagate it through the synthesized `Done` event verbatim.
    #[tokio::test]
    async fn mock_provider_stream_done_carries_request_id() {
        use futures_util::StreamExt;

        let mock = MockProvider::new("test");
        let mut response = InferResponse::new(
            vec![ContentBlock::Text { text: "hi".to_string() }],
            TokenUsage {
                input_tokens: 3,
                output_tokens: 2,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            StopReason::EndTurn,
        );
        response.request_id = Some("req_mock_42".to_string());
        mock.enqueue_response(response);

        let request = InferRequest {
            model: "test-model".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            tool_choice: Default::default(),
            response_format: Default::default(),
            stop_sequences: Vec::new(),
            thinking_budget: None,
            extra: Default::default(),
        };

        let stream = mock.infer_stream(request).await.unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;
        let done = events
            .iter()
            .find_map(|ev| match ev {
                Ok(InferEvent::Done { stop_reason, request_id, finish_reason_raw }) => {
                    Some((stop_reason.clone(), request_id.clone(), finish_reason_raw.clone()))
                }
                _ => None,
            })
            .expect("stream must terminate with a Done event");

        assert!(matches!(done.0, StopReason::EndTurn));
        assert_eq!(done.1.as_deref(), Some("req_mock_42"));
        assert_eq!(done.2, None);
    }
}
