//! Claude provider using Anthropic API

use super::{Provider, CLAUDE_DEFAULT_MODEL};
use crate::util::{CONNECT_TIMEOUT, INFER_TIMEOUT};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct ClaudeProvider {
    api_key: String,
    client: Client,
}

impl ClaudeProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;

        let client = Client::builder()
            .timeout(INFER_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent("nika-cli/0.1")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { api_key, client })
    }

    /// Resolve model aliases to full Anthropic model IDs
    fn resolve_model<'a>(&self, model: &'a str) -> &'a str {
        // Sonnet variants
        if model.eq_ignore_ascii_case("claude-sonnet-4-5")
            || model.eq_ignore_ascii_case("claude-sonnet")
            || model.eq_ignore_ascii_case("sonnet")
        {
            "claude-sonnet-4-20250514"
        }
        // Opus variants
        else if model.eq_ignore_ascii_case("claude-opus-4")
            || model.eq_ignore_ascii_case("claude-opus")
            || model.eq_ignore_ascii_case("opus")
        {
            "claude-opus-4-20250514"
        }
        // Haiku variants
        else if model.eq_ignore_ascii_case("claude-haiku") || model.eq_ignore_ascii_case("haiku")
        {
            "claude-3-5-haiku-20241022"
        }
        // Pass through if already a full model ID (case-insensitive prefix check)
        else if model
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("claude-"))
        {
            model
        }
        // Default
        else {
            "claude-sonnet-4-20250514"
        }
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn default_model(&self) -> &str {
        CLAUDE_DEFAULT_MODEL
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "max_tokens": 4096,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, body);
        }

        let json: serde_json::Value = response.json().await?;
        let text = json["content"][0]["text"]
            .as_str()
            .context("Invalid response format from Claude API")?;

        Ok(text.to_string())
    }
}
