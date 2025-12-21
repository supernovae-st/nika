//! Mock provider for testing
//!
//! Returns configurable responses without making real API calls.
//! Essential for unit tests and CI pipelines.

use super::{PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Mock provider that returns predefined responses
pub struct MockProvider {
    /// Queue of responses to return (FIFO)
    responses: Arc<Mutex<Vec<String>>>,
    /// Default response when queue is empty
    default_response: String,
    /// Track all requests made (for assertions)
    requests: Arc<Mutex<Vec<PromptRequest>>>,
}

impl MockProvider {
    /// Create a new mock provider with default echo behavior
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(vec![])),
            default_response: "Mock response".to_string(),
            requests: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Create with a queue of responses
    pub fn with_responses(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            default_response: "Mock response".to_string(),
            requests: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Set the default response when queue is empty
    pub fn with_default(mut self, response: impl Into<String>) -> Self {
        self.default_response = response.into();
        self
    }

    /// Add a response to the queue
    pub fn queue_response(&self, response: impl Into<String>) {
        self.responses.lock().unwrap().push(response.into());
    }

    /// Get all requests made to this provider
    pub fn get_requests(&self) -> Vec<PromptRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Get the last request made
    pub fn last_request(&self) -> Option<PromptRequest> {
        self.requests.lock().unwrap().last().cloned()
    }

    /// Clear all recorded requests
    pub fn clear_requests(&self) {
        self.requests.lock().unwrap().clear();
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        // Record the request
        self.requests.lock().unwrap().push(request.clone());

        // Get response from queue or use default
        let response_text = {
            let mut queue = self.responses.lock().unwrap();
            if queue.is_empty() {
                self.default_response.clone()
            } else {
                queue.remove(0)
            }
        };

        // Estimate token usage
        let usage = TokenUsage::estimate(request.prompt.len(), response_text.len());

        Ok(PromptResponse::success(response_text).with_usage(usage))
    }

    fn supports_tools(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        true // Mock is always available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_default_response() {
        let provider = MockProvider::new();
        let request = PromptRequest::new("Hello", "test-model");

        let response = provider.execute(request).unwrap();

        assert!(response.success);
        assert_eq!(response.content, "Mock response");
    }

    #[test]
    fn test_mock_queued_responses() {
        let provider = MockProvider::with_responses(vec![
            "First response".to_string(),
            "Second response".to_string(),
        ]);

        let req1 = PromptRequest::new("Hello", "test-model");
        let req2 = PromptRequest::new("World", "test-model");
        let req3 = PromptRequest::new("Extra", "test-model");

        let resp1 = provider.execute(req1).unwrap();
        let resp2 = provider.execute(req2).unwrap();
        let resp3 = provider.execute(req3).unwrap();

        assert_eq!(resp1.content, "First response");
        assert_eq!(resp2.content, "Second response");
        assert_eq!(resp3.content, "Mock response"); // Default after queue empty
    }

    #[test]
    fn test_mock_records_requests() {
        let provider = MockProvider::new();

        let req1 = PromptRequest::new("First prompt", "model-1");
        let req2 = PromptRequest::new("Second prompt", "model-2").isolated();

        provider.execute(req1).unwrap();
        provider.execute(req2).unwrap();

        let requests = provider.get_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].prompt, "First prompt");
        assert_eq!(requests[1].prompt, "Second prompt");
        assert!(requests[1].is_isolated);
    }

    #[test]
    fn test_mock_custom_default() {
        let provider = MockProvider::new().with_default("Custom default");

        let request = PromptRequest::new("Test", "model");
        let response = provider.execute(request).unwrap();

        assert_eq!(response.content, "Custom default");
    }

    #[test]
    fn test_mock_is_always_available() {
        let provider = MockProvider::new();
        assert!(provider.is_available());
    }

    #[test]
    fn test_mock_token_estimation() {
        let provider = MockProvider::new().with_default("Short");

        let request = PromptRequest::new("A longer prompt with more tokens", "model");
        let response = provider.execute(request).unwrap();

        assert!(response.usage.prompt_tokens > 0);
        assert!(response.usage.completion_tokens > 0);
        assert_eq!(
            response.usage.total_tokens,
            response.usage.prompt_tokens + response.usage.completion_tokens
        );
    }
}
