//! ExecutorCompressorLlm — bridges TaskExecutor to CompressorLlm trait.
//!
//! Allows the RecordCompressor to use the workflow engine's existing
//! provider infrastructure for LLM-based record compression.

use crate::ast::{InferParams, TaskAction};
use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::runtime::executor::TaskExecutor;
use crate::runtime::record_compress::CompressorLlm;
use crate::store::RunContext;
use async_trait::async_trait;
use std::sync::Arc;

/// Implements CompressorLlm using the workflow executor's infer pipeline.
///
/// Resolves the cheapest available model (haiku > default model)
/// and runs a single infer call through the standard provider stack.
pub struct ExecutorCompressorLlm<'a> {
    executor: &'a TaskExecutor,
    model: String,
    provider: String,
}

impl<'a> ExecutorCompressorLlm<'a> {
    /// Create a compressor LLM using the executor's provider infrastructure.
    ///
    /// Tries to use a cheap model for compression:
    /// 1. claude-haiku-4-5 (if provider is anthropic)
    /// 2. gpt-4.1-mini (if provider is openai)
    /// 3. Falls back to the workflow's default model
    pub fn new(executor: &'a TaskExecutor, default_provider: &str, default_model: &str) -> Self {
        let (provider, model) = resolve_cheap_model(default_provider, default_model);
        Self {
            executor,
            model,
            provider,
        }
    }
}

#[async_trait]
impl CompressorLlm for ExecutorCompressorLlm<'_> {
    async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
        let task_id: Arc<str> = Arc::from("_record_compress");
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();

        let action = TaskAction::Infer {
            infer: InferParams {
                prompt: prompt.to_string(),
                provider: Some(self.provider.clone()),
                model: Some(self.model.clone()),
                temperature: Some(0.0), // Deterministic for compression
                max_tokens: Some(2000),
                ..Default::default()
            },
        };

        self.executor
            .execute(&task_id, &action, &bindings, &datastore, None)
            .await
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Resolve the cheapest model for record compression.
///
/// Preference: haiku for Anthropic, mini for OpenAI, else default.
fn resolve_cheap_model(provider: &str, default_model: &str) -> (String, String) {
    let lower = provider.to_lowercase();
    match lower.as_str() {
        "anthropic" | "claude" => (
            "anthropic".to_string(),
            "claude-haiku-4-5".to_string(),
        ),
        "openai" | "gpt" => (
            "openai".to_string(),
            "gpt-4.1-mini".to_string(),
        ),
        "mock" => ("mock".to_string(), "mock".to_string()),
        _ => (provider.to_string(), default_model.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cheap_model_anthropic() {
        let (p, m) = resolve_cheap_model("anthropic", "claude-sonnet-4-20250514");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-haiku-4-5");
    }

    #[test]
    fn test_resolve_cheap_model_openai() {
        let (p, m) = resolve_cheap_model("openai", "gpt-4o");
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-4.1-mini");
    }

    #[test]
    fn test_resolve_cheap_model_mock() {
        let (p, m) = resolve_cheap_model("mock", "test");
        assert_eq!(p, "mock");
        assert_eq!(m, "mock");
    }

    #[test]
    fn test_resolve_cheap_model_unknown_uses_default() {
        let (p, m) = resolve_cheap_model("groq", "mixtral-8x7b");
        assert_eq!(p, "groq");
        assert_eq!(m, "mixtral-8x7b");
    }
}
