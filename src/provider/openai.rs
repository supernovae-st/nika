//! OpenAI provider using OpenAI API

use super::{Provider, OPENAI_DEFAULT_MODEL};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct OpenAIProvider {
    api_key: String,
    client: Client,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY not set")?;

        Ok(Self {
            api_key,
            client: Client::new(),
        })
    }

    /// Map model names to valid OpenAI models
    fn resolve_model(&self, model: &str) -> &'static str {
        // Direct OpenAI models - zero allocation exact matches
        if model.eq_ignore_ascii_case("gpt-4o") { return "gpt-4o"; }
        if model.eq_ignore_ascii_case("gpt-4o-mini") { return "gpt-4o-mini"; }
        if model.eq_ignore_ascii_case("gpt-4-turbo") { return "gpt-4-turbo"; }
        if model.eq_ignore_ascii_case("gpt-3.5-turbo") { return "gpt-3.5-turbo"; }
        if model.eq_ignore_ascii_case("o1") { return "o1"; }
        if model.eq_ignore_ascii_case("o1-mini") { return "o1-mini"; }
        if model.eq_ignore_ascii_case("o1-preview") { return "o1-preview"; }

        // Claude model fallback - single allocation for contains checks
        let model_lower = model.to_ascii_lowercase();

        // Claude Haiku → GPT-4o-mini (fast/cheap)
        if model_lower.contains("haiku") {
            return "gpt-4o-mini";
        }

        // Claude Sonnet/Opus → GPT-4o
        if model_lower.contains("sonnet") || model_lower.contains("opus") || model_lower.contains("claude") {
            return "gpt-4o";
        }

        // Default to gpt-4o
        "gpt-4o"
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn default_model(&self) -> &str {
        OPENAI_DEFAULT_MODEL
    }

    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let resolved_model = self.resolve_model(model);

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": resolved_model,
                "messages": [
                    { "role": "user", "content": prompt }
                ]
            }))
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, body);
        }

        let json: serde_json::Value = response.json().await?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .context("Invalid response format from OpenAI API")?;

        Ok(text.to_string())
    }
}
