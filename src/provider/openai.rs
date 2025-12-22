//! OpenAI provider using the OpenAI API
//!
//! Executes prompts via OpenAI's Chat Completions API.
//! Requires `OPENAI_API_KEY` environment variable.

use super::{Capabilities, PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// OpenAI API endpoint
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI provider that uses the OpenAI API
pub struct OpenAIProvider {
    /// HTTP client
    client: reqwest::Client,
    /// API key
    api_key: String,
    /// Model to use (default: gpt-4o)
    model: String,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider
    ///
    /// Reads `OPENAI_API_KEY` from environment.
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "gpt-4o".to_string(),
        })
    }

    /// Create with a specific API key
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Build messages array from request
    fn build_messages(&self, request: &PromptRequest) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Add system prompt if present
        if let Some(ref system) = request.system_prompt {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }

        // Add conversation history (for agent: tasks, not isolated)
        if !request.is_isolated {
            for msg in &request.history {
                let role = match msg.role {
                    crate::runner::MessageRole::User => "user",
                    crate::runner::MessageRole::Assistant => "assistant",
                    crate::runner::MessageRole::System => "system",
                };
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: msg.content.to_string(),
                });
            }
        }

        // Add the main prompt as user message
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        messages
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::openai()
    }

    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        let messages = self.build_messages(&request);

        // Build request payload
        let payload = ChatCompletionRequest {
            model: if request.model.is_empty() {
                self.model.clone()
            } else {
                request.model.clone()
            },
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        tracing::debug!(
            provider = "openai",
            model = %payload.model,
            messages_count = payload.messages.len(),
            "Sending request to OpenAI API"
        );

        // Make API request
        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        // Check for errors
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(
                provider = "openai",
                status = %status,
                error = %error_text,
                "OpenAI API error"
            );
            return Ok(PromptResponse::failure(format!(
                "OpenAI API error ({}): {}",
                status, error_text
            )));
        }

        // Parse response
        let api_response: ChatCompletionResponse = response
            .json()
            .await
            .context("Failed to parse OpenAI API response")?;

        // Extract content from first choice
        let content = api_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        // Build token usage
        let usage = if let Some(u) = api_response.usage {
            TokenUsage::new(u.prompt_tokens, u.completion_tokens)
        } else {
            TokenUsage::estimate(request.prompt.len(), content.len())
        };

        tracing::debug!(
            provider = "openai",
            tokens = usage.total_tokens,
            "OpenAI API response received"
        );

        Ok(PromptResponse::success(content).with_usage(usage))
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ============================================================================
// API TYPES
// ============================================================================

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_name() {
        // Can't test new() without API key, use with_api_key
        let provider = OpenAIProvider::with_api_key("test-key");
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_capabilities() {
        let provider = OpenAIProvider::with_api_key("test-key");
        let caps = provider.capabilities();
        assert!(caps.tool_use);
        assert!(caps.vision);
        assert_eq!(caps.max_context, 128_000);
    }

    #[test]
    fn test_openai_with_model() {
        let provider = OpenAIProvider::with_api_key("test-key").with_model("gpt-4-turbo");
        assert_eq!(provider.model, "gpt-4-turbo");
    }

    #[test]
    fn test_build_messages_simple() {
        let provider = OpenAIProvider::with_api_key("test-key");
        let request = PromptRequest::new("Hello world", "gpt-4o");

        let messages = provider.build_messages(&request);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello world");
    }

    #[test]
    fn test_build_messages_with_system() {
        let provider = OpenAIProvider::with_api_key("test-key");
        let request = PromptRequest::new("Hello", "gpt-4o").with_system_prompt("You are helpful");

        let messages = provider.build_messages(&request);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "You are helpful");
        assert_eq!(messages[1].role, "user");
    }

    #[test]
    fn test_is_available() {
        let provider = OpenAIProvider::with_api_key("test-key");
        assert!(provider.is_available());

        let empty_provider = OpenAIProvider::with_api_key("");
        assert!(!empty_provider.is_available());
    }
}
