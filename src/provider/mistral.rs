//! Mistral AI provider
//!
//! Executes prompts via Mistral AI's Chat API.
//! Requires `MISTRAL_API_KEY` environment variable.
//!
//! # Status: STUB
//!
//! This is a stub implementation. The actual API call is not yet implemented.
//! Returns placeholder responses for testing provider selection.

use super::{Capabilities, PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Mistral API endpoint
#[allow(dead_code)]
const MISTRAL_API_URL: &str = "https://api.mistral.ai/v1/chat/completions";

/// Default model
const DEFAULT_MODEL: &str = "mistral-large-latest";

/// Mistral AI provider
pub struct MistralProvider {
    /// API key
    api_key: String,
    /// Model to use
    model: String,
}

impl MistralProvider {
    /// Create a new Mistral provider
    ///
    /// Reads `MISTRAL_API_KEY` from environment.
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("MISTRAL_API_KEY")
            .context("MISTRAL_API_KEY environment variable not set")?;

        Ok(Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
        })
    }

    /// Create with a specific API key
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[async_trait]
impl Provider for MistralProvider {
    fn name(&self) -> &str {
        "mistral"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::mistral()
    }

    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        // TODO: Implement actual Mistral API call
        // POST https://api.mistral.ai/v1/chat/completions
        // {
        //   "model": "mistral-large-latest",
        //   "messages": [{"role": "user", "content": "..."}]
        // }

        tracing::warn!(
            provider = "mistral",
            model = %self.model,
            "Mistral provider is a stub - returning placeholder response"
        );

        let model = if request.model.is_empty() {
            &self.model
        } else {
            &request.model
        };

        let content = format!(
            "[Mistral stub] Model: {}, Prompt: {}",
            model,
            &request.prompt[..request.prompt.len().min(50)]
        );

        let usage = TokenUsage::estimate(request.prompt.len(), content.len());
        Ok(PromptResponse::success(content).with_usage(usage))
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mistral_provider_name() {
        let provider = MistralProvider::with_api_key("test-key");
        assert_eq!(provider.name(), "mistral");
    }

    #[test]
    fn test_mistral_capabilities() {
        let provider = MistralProvider::with_api_key("test-key");
        let caps = provider.capabilities();
        assert!(caps.tool_use);
        assert!(caps.vision); // Pixtral models have vision
        assert_eq!(caps.max_context, 128_000);
    }

    #[test]
    fn test_mistral_with_model() {
        let provider = MistralProvider::with_api_key("test-key").with_model("mistral-small");
        assert_eq!(provider.model, "mistral-small");
    }

    #[test]
    fn test_is_available() {
        let provider = MistralProvider::with_api_key("test-key");
        assert!(provider.is_available());

        let empty_provider = MistralProvider::with_api_key("");
        assert!(!empty_provider.is_available());
    }

    #[tokio::test]
    async fn test_mistral_execute_stub() {
        let provider = MistralProvider::with_api_key("test-key");
        let request = PromptRequest::new("Hello world", "mistral-large");

        let response = provider.execute(request).await.unwrap();

        assert!(response.success);
        assert!(response.content.contains("[Mistral stub]"));
    }
}
