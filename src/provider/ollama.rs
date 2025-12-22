//! Ollama provider for local LLM execution
//!
//! Executes prompts via Ollama's local API.
//! Requires Ollama to be running locally.
//!
//! # Status: STUB
//!
//! This is a stub implementation. The actual API call is not yet implemented.
//! Returns placeholder responses for testing provider selection.

use super::{Capabilities, PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;

/// Default Ollama API endpoint
const DEFAULT_HOST: &str = "http://localhost:11434";

/// Default model
const DEFAULT_MODEL: &str = "llama3.2";

/// Ollama provider for local LLM execution
pub struct OllamaProvider {
    /// Ollama API host
    host: String,
    /// Model to use
    model: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default settings
    pub fn new() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Set custom host
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Set model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::ollama()
    }

    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        // TODO: Implement actual Ollama API call
        // POST {host}/api/generate
        // {
        //   "model": "llama3.2",
        //   "prompt": "...",
        //   "stream": false
        // }

        tracing::warn!(
            provider = "ollama",
            host = %self.host,
            model = %self.model,
            "Ollama provider is a stub - returning placeholder response"
        );

        let model = if request.model.is_empty() {
            &self.model
        } else {
            &request.model
        };

        let content = format!(
            "[Ollama stub] Model: {}, Prompt: {}",
            model,
            &request.prompt[..request.prompt.len().min(50)]
        );

        let usage = TokenUsage::estimate(request.prompt.len(), content.len());
        Ok(PromptResponse::success(content).with_usage(usage))
    }

    fn is_available(&self) -> bool {
        // TODO: Check if Ollama is running
        // Could ping {host}/api/tags to verify
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_name() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_ollama_capabilities() {
        let provider = OllamaProvider::new();
        let caps = provider.capabilities();
        assert!(caps.tool_use);
        assert!(!caps.vision); // Most local models don't have vision
        assert_eq!(caps.max_context, 8_192);
    }

    #[test]
    fn test_ollama_with_host() {
        let provider = OllamaProvider::new().with_host("http://192.168.1.100:11434");
        assert_eq!(provider.host, "http://192.168.1.100:11434");
    }

    #[test]
    fn test_ollama_with_model() {
        let provider = OllamaProvider::new().with_model("mistral");
        assert_eq!(provider.model, "mistral");
    }

    #[test]
    fn test_ollama_default() {
        let provider = OllamaProvider::default();
        assert_eq!(provider.host, DEFAULT_HOST);
        assert_eq!(provider.model, DEFAULT_MODEL);
    }

    #[tokio::test]
    async fn test_ollama_execute_stub() {
        let provider = OllamaProvider::new();
        let request = PromptRequest::new("Hello world", "llama3.2");

        let response = provider.execute(request).await.unwrap();

        assert!(response.success);
        assert!(response.content.contains("[Ollama stub]"));
        assert!(response.content.contains("llama3.2"));
    }
}
