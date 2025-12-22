//! Mock provider for testing
//!
//! Returns configurable responses without making real API calls.
//! Essential for unit tests and CI pipelines.
//!
//! # Features
//!
//! - **Response queue**: Return specific responses in order
//! - **Failure simulation**: Fail after N successful calls
//! - **Request tracking**: Inspect all requests made
//! - **Echo mode**: Default echoes prompt for template testing

use super::{PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Response behavior for the mock provider
#[derive(Clone)]
pub enum MockResponse {
    /// Return a successful response with this content
    Success(String),
    /// Return a failure response with this error message
    Failure(String),
}

impl From<&str> for MockResponse {
    fn from(s: &str) -> Self {
        MockResponse::Success(s.to_string())
    }
}

impl From<String> for MockResponse {
    fn from(s: String) -> Self {
        MockResponse::Success(s)
    }
}

/// Mock provider that returns predefined responses
pub struct MockProvider {
    /// Queue of responses to return (FIFO)
    responses: Arc<Mutex<Vec<MockResponse>>>,
    /// Default response when queue is empty (Arc for no-clone sharing)
    default_response: Arc<str>,
    /// Track all requests made (for assertions)
    requests: Arc<Mutex<Vec<PromptRequest>>>,
    /// Number of calls before automatic failure (0 = never fail)
    fail_after: AtomicUsize,
    /// Current call count
    call_count: AtomicUsize,
    /// Error message to use when failing (Arc for no-clone sharing)
    failure_message: Arc<str>,
}

impl MockProvider {
    /// Create a new mock provider with default echo behavior
    ///
    /// By default, MockProvider echoes the prompt prefixed with "[Mock]",
    /// which allows tests to verify template resolution.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(vec![])),
            default_response: Arc::from(""), // Empty means echo mode
            requests: Arc::new(Mutex::new(vec![])),
            fail_after: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
            failure_message: Arc::from("Mock failure"),
        }
    }

    /// Create with a queue of responses
    ///
    /// Falls back to echo mode when queue is empty.
    pub fn with_responses(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(
                responses.into_iter().map(MockResponse::Success).collect(),
            )),
            default_response: Arc::from(""), // Empty means echo mode
            requests: Arc::new(Mutex::new(vec![])),
            fail_after: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
            failure_message: Arc::from("Mock failure"),
        }
    }

    /// Create with a queue of MockResponse (can include failures)
    pub fn with_mock_responses(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            default_response: Arc::from(""),
            requests: Arc::new(Mutex::new(vec![])),
            fail_after: AtomicUsize::new(0),
            call_count: AtomicUsize::new(0),
            failure_message: Arc::from("Mock failure"),
        }
    }

    /// Set the default response when queue is empty
    pub fn with_default(mut self, response: impl Into<String>) -> Self {
        self.default_response = Arc::from(response.into().as_str());
        self
    }

    /// Configure to fail after N successful calls
    ///
    /// Useful for testing retry logic and error handling.
    pub fn with_failure_after(self, n: usize) -> Self {
        self.fail_after.store(n, Ordering::SeqCst);
        self
    }

    /// Set custom failure message
    pub fn with_failure_message(mut self, message: impl Into<String>) -> Self {
        self.failure_message = Arc::from(message.into().as_str());
        self
    }

    /// Add a response to the queue
    pub fn queue_response(&self, response: impl Into<String>) {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::Success(response.into()));
    }

    /// Add a failure response to the queue
    pub fn queue_failure(&self, error: impl Into<String>) {
        self.responses
            .lock()
            .unwrap()
            .push(MockResponse::Failure(error.into()));
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

    /// Get the current call count
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    /// Reset call count and recorded requests
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
        self.clear_requests();
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

        // Increment call count
        let current_count = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Check if we should fail after N calls
        let fail_after = self.fail_after.load(Ordering::SeqCst);
        if fail_after > 0 && current_count > fail_after {
            let usage = TokenUsage::estimate(request.prompt.len(), self.failure_message.len());
            return Ok(PromptResponse::failure(self.failure_message.as_ref()).with_usage(usage));
        }

        // Get response from queue, or use default/echo
        let mock_response = {
            let mut queue = self.responses.lock().unwrap();
            if !queue.is_empty() {
                queue.remove(0)
            } else if self.default_response.is_empty() {
                // Echo mode: return prompt prefixed with [Mock]
                MockResponse::Success(format!("[Mock] Executed prompt: {}", request.prompt))
            } else {
                MockResponse::Success(self.default_response.to_string())
            }
        };

        // Return based on response type
        match mock_response {
            MockResponse::Success(content) => {
                let usage = TokenUsage::estimate(request.prompt.len(), content.len());
                Ok(PromptResponse::success(content).with_usage(usage))
            }
            MockResponse::Failure(error) => {
                let usage = TokenUsage::estimate(request.prompt.len(), error.len());
                Ok(PromptResponse::failure(error).with_usage(usage))
            }
        }
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
        // By default, MockProvider echoes the prompt
        assert_eq!(response.content, "[Mock] Executed prompt: Hello");
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
        // Echo mode after queue is empty
        assert_eq!(resp3.content, "[Mock] Executed prompt: Extra");
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

    #[test]
    fn test_mock_failure_after_n_calls() {
        let provider = MockProvider::new()
            .with_default("Success")
            .with_failure_after(2)
            .with_failure_message("Simulated timeout");

        let req = PromptRequest::new("Test", "model");

        // First two should succeed
        let resp1 = provider.execute(req.clone()).unwrap();
        let resp2 = provider.execute(req.clone()).unwrap();
        assert!(resp1.success);
        assert!(resp2.success);

        // Third should fail
        let resp3 = provider.execute(req.clone()).unwrap();
        assert!(!resp3.success);
        assert_eq!(resp3.content, "Simulated timeout");

        // Fourth should also fail
        let resp4 = provider.execute(req).unwrap();
        assert!(!resp4.success);
    }

    #[test]
    fn test_mock_queue_failure_response() {
        let provider = MockProvider::with_mock_responses(vec![
            MockResponse::Success("First works".to_string()),
            MockResponse::Failure("Network error".to_string()),
            MockResponse::Success("Third works".to_string()),
        ]);

        let req = PromptRequest::new("Test", "model");

        let resp1 = provider.execute(req.clone()).unwrap();
        let resp2 = provider.execute(req.clone()).unwrap();
        let resp3 = provider.execute(req).unwrap();

        assert!(resp1.success);
        assert_eq!(resp1.content, "First works");

        assert!(!resp2.success);
        assert_eq!(resp2.content, "Network error");

        assert!(resp3.success);
        assert_eq!(resp3.content, "Third works");
    }

    #[test]
    fn test_mock_call_count() {
        let provider = MockProvider::new();

        assert_eq!(provider.call_count(), 0);

        let req = PromptRequest::new("Test", "model");
        provider.execute(req.clone()).unwrap();
        assert_eq!(provider.call_count(), 1);

        provider.execute(req.clone()).unwrap();
        provider.execute(req).unwrap();
        assert_eq!(provider.call_count(), 3);
    }

    #[test]
    fn test_mock_reset() {
        let provider = MockProvider::new();
        let req = PromptRequest::new("Test", "model");

        provider.execute(req.clone()).unwrap();
        provider.execute(req).unwrap();

        assert_eq!(provider.call_count(), 2);
        assert_eq!(provider.get_requests().len(), 2);

        provider.reset();

        assert_eq!(provider.call_count(), 0);
        assert_eq!(provider.get_requests().len(), 0);
    }

    #[test]
    fn test_mock_queue_failure_method() {
        let provider = MockProvider::new();

        provider.queue_response("Success response");
        provider.queue_failure("Error occurred");

        let req = PromptRequest::new("Test", "model");

        let resp1 = provider.execute(req.clone()).unwrap();
        let resp2 = provider.execute(req).unwrap();

        assert!(resp1.success);
        assert_eq!(resp1.content, "Success response");

        assert!(!resp2.success);
        assert_eq!(resp2.content, "Error occurred");
    }
}
