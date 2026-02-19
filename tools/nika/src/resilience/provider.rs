//! Resilient Provider Wrapper
//!
//! Wraps any LLM provider with resilience patterns:
//! 1. Rate limiting (prevent overwhelming the service)
//! 2. Circuit breaker (fail fast when service is down)
//! 3. Retry with exponential backoff (handle transient failures)
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::provider::{create_provider, Provider};
//! use nika::resilience::{ResilientProvider, ResilientProviderConfig};
//!
//! let base_provider = create_provider("openai")?;
//! let config = ResilientProviderConfig::default();
//! let provider = ResilientProvider::new(base_provider, config);
//!
//! // Now all calls have resilience built-in
//! let response = provider.infer("Hello", "gpt-4o").await?;
//! ```

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use crate::provider::{ChatResponse, Message, Provider, ToolDefinition};
use crate::resilience::{
    CircuitBreaker, CircuitBreakerConfig, Metrics, MetricsSnapshot, RateLimiter, RateLimiterConfig,
    RetryConfig, RetryPolicy,
};

/// Configuration for resilient provider
#[derive(Debug, Clone)]
pub struct ResilientProviderConfig {
    /// Retry configuration
    pub retry: RetryConfig,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
    /// Rate limiter configuration
    pub rate_limiter: RateLimiterConfig,
    /// Whether to enable rate limiting
    pub enable_rate_limiting: bool,
    /// Whether to enable circuit breaker
    pub enable_circuit_breaker: bool,
    /// Whether to enable retry
    pub enable_retry: bool,
}

impl Default for ResilientProviderConfig {
    fn default() -> Self {
        Self {
            retry: RetryConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            rate_limiter: RateLimiterConfig::default(),
            enable_rate_limiting: true,
            enable_circuit_breaker: true,
            enable_retry: true,
        }
    }
}

impl ResilientProviderConfig {
    /// Create a minimal config with just retries enabled
    pub fn retry_only() -> Self {
        Self {
            enable_rate_limiting: false,
            enable_circuit_breaker: false,
            enable_retry: true,
            ..Default::default()
        }
    }

    /// Create a config with no resilience patterns (passthrough)
    pub fn none() -> Self {
        Self {
            enable_rate_limiting: false,
            enable_circuit_breaker: false,
            enable_retry: false,
            ..Default::default()
        }
    }

    /// Set retry configuration
    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry = config;
        self
    }

    /// Set circuit breaker configuration
    pub fn with_circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = config;
        self
    }

    /// Set rate limiter configuration
    pub fn with_rate_limiter(mut self, config: RateLimiterConfig) -> Self {
        self.rate_limiter = config;
        self
    }

    /// Enable or disable rate limiting
    pub fn rate_limiting(mut self, enabled: bool) -> Self {
        self.enable_rate_limiting = enabled;
        self
    }

    /// Enable or disable circuit breaker
    pub fn circuit_breaker(mut self, enabled: bool) -> Self {
        self.enable_circuit_breaker = enabled;
        self
    }

    /// Enable or disable retry
    pub fn retry(mut self, enabled: bool) -> Self {
        self.enable_retry = enabled;
        self
    }
}

/// Provider wrapper with resilience patterns
pub struct ResilientProvider {
    inner: Arc<dyn Provider>,
    config: ResilientProviderConfig,
    rate_limiter: RateLimiter,
    circuit_breaker: CircuitBreaker,
    retry_policy: RetryPolicy,
    metrics: Metrics,
}

impl ResilientProvider {
    /// Create a new resilient provider wrapping the given provider
    pub fn new(provider: Box<dyn Provider>, config: ResilientProviderConfig) -> Self {
        let provider_name = provider.name().to_string();

        Self {
            inner: Arc::from(provider),
            rate_limiter: RateLimiter::new(
                format!("{}-rate-limiter", provider_name),
                config.rate_limiter.clone(),
            ),
            circuit_breaker: CircuitBreaker::new(
                format!("{}-circuit-breaker", provider_name),
                config.circuit_breaker.clone(),
            ),
            retry_policy: RetryPolicy::new(config.retry.clone()),
            metrics: Metrics::new(format!("{}-metrics", provider_name)),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_defaults(provider: Box<dyn Provider>) -> Self {
        Self::new(provider, ResilientProviderConfig::default())
    }

    /// Get the underlying provider name
    pub fn inner_name(&self) -> &str {
        self.inner.name()
    }

    /// Get current circuit breaker state
    pub fn circuit_state(&self) -> crate::resilience::CircuitState {
        self.circuit_breaker.state()
    }

    /// Get available rate limit tokens
    pub fn available_tokens(&self) -> f64 {
        self.rate_limiter.available_tokens()
    }

    /// Reset circuit breaker (for admin/testing)
    pub fn reset_circuit_breaker(&self) {
        self.circuit_breaker.reset();
    }

    /// Reset rate limiter (for admin/testing)
    pub fn reset_rate_limiter(&self) {
        self.rate_limiter.reset();
    }

    /// Get a snapshot of current metrics
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Reset metrics (for admin/testing)
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }
}

#[async_trait]
impl Provider for ResilientProvider {
    async fn infer(&self, prompt: &str, model: &str) -> Result<String> {
        let start = Instant::now();

        // 1. Rate limiting
        if self.config.enable_rate_limiting {
            if let Err(e) = self.rate_limiter.acquire().await {
                self.metrics.record_rate_limit();
                return Err(e);
            }
        }

        // 2. Circuit breaker + retry
        let inner = self.inner.clone();
        let prompt = prompt.to_string();
        let model = model.to_string();

        let operation = || {
            let inner = inner.clone();
            let prompt = prompt.clone();
            let model = model.clone();
            async move { inner.infer(&prompt, &model).await }
        };

        // Convert NikaError result to anyhow::Result for Provider trait
        let convert = |r: crate::error::Result<String>| -> Result<String> {
            r.map_err(|e| anyhow::anyhow!("{}", e))
        };

        // Correct order: retry wraps circuit breaker
        // - Each retry attempt goes through circuit breaker independently
        // - Circuit breaker sees each failure (accurate tracking)
        // - If circuit opens mid-retry, subsequent attempts fail fast
        let result = if self.config.enable_retry {
            if self.config.enable_circuit_breaker {
                convert(
                    self.retry_policy
                        .execute(|| self.circuit_breaker.execute(operation))
                        .await,
                )
            } else {
                convert(self.retry_policy.execute(operation).await)
            }
        } else if self.config.enable_circuit_breaker {
            convert(self.circuit_breaker.execute(operation).await)
        } else {
            operation().await
        };

        // Record metrics
        let latency = start.elapsed();
        match &result {
            Ok(_) => self.metrics.record_success(latency),
            Err(_) => self.metrics.record_failure(latency),
        }

        result
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        model: &str,
    ) -> Result<ChatResponse> {
        let start = Instant::now();

        // 1. Rate limiting
        if self.config.enable_rate_limiting {
            if let Err(e) = self.rate_limiter.acquire().await {
                self.metrics.record_rate_limit();
                return Err(e);
            }
        }

        // 2. Circuit breaker + retry
        let inner = self.inner.clone();
        let messages = messages.to_vec();
        let tools = tools.map(|t| t.to_vec());
        let model = model.to_string();

        let operation = || {
            let inner = inner.clone();
            let messages = messages.clone();
            let tools = tools.clone();
            let model = model.clone();
            async move { inner.chat(&messages, tools.as_deref(), &model).await }
        };

        // Convert NikaError result to anyhow::Result for Provider trait
        let convert = |r: crate::error::Result<ChatResponse>| -> Result<ChatResponse> {
            r.map_err(|e| anyhow::anyhow!("{}", e))
        };

        // Correct order: retry wraps circuit breaker
        // - Each retry attempt goes through circuit breaker independently
        // - Circuit breaker sees each failure (accurate tracking)
        // - If circuit opens mid-retry, subsequent attempts fail fast
        let result = if self.config.enable_retry {
            if self.config.enable_circuit_breaker {
                convert(
                    self.retry_policy
                        .execute(|| self.circuit_breaker.execute(operation))
                        .await,
                )
            } else {
                convert(self.retry_policy.execute(operation).await)
            }
        } else if self.config.enable_circuit_breaker {
            convert(self.circuit_breaker.execute(operation).await)
        } else {
            operation().await
        };

        // Record metrics
        let latency = start.elapsed();
        match &result {
            Ok(_) => self.metrics.record_success(latency),
            Err(_) => self.metrics.record_failure(latency),
        }

        result
    }

    fn default_model(&self) -> &str {
        self.inner.default_model()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

impl std::fmt::Debug for ResilientProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResilientProvider")
            .field("inner", &self.inner.name())
            .field("circuit_state", &self.circuit_breaker.state())
            .field("available_tokens", &self.rate_limiter.available_tokens())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Message, MessageContent, MessageRole, MockProvider, StopReason, Usage};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[test]
    fn test_resilient_provider_config_default() {
        let config = ResilientProviderConfig::default();
        assert!(config.enable_rate_limiting);
        assert!(config.enable_circuit_breaker);
        assert!(config.enable_retry);
    }

    #[test]
    fn test_resilient_provider_config_retry_only() {
        let config = ResilientProviderConfig::retry_only();
        assert!(!config.enable_rate_limiting);
        assert!(!config.enable_circuit_breaker);
        assert!(config.enable_retry);
    }

    #[test]
    fn test_resilient_provider_config_none() {
        let config = ResilientProviderConfig::none();
        assert!(!config.enable_rate_limiting);
        assert!(!config.enable_circuit_breaker);
        assert!(!config.enable_retry);
    }

    #[tokio::test]
    async fn test_resilient_provider_infer_success() {
        let mock = Box::new(MockProvider);
        let provider = ResilientProvider::with_defaults(mock);

        let result = provider.infer("Hello", "mock-v1").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Mock response");
    }

    #[tokio::test]
    async fn test_resilient_provider_chat_success() {
        let mock = Box::new(MockProvider);
        let provider = ResilientProvider::with_defaults(mock);

        let messages = vec![Message {
            role: MessageRole::User,
            content: MessageContent::Text("Hello".to_string()),
        }];

        let result = provider.chat(&messages, None, "mock-v1").await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(
            response.content,
            MessageContent::Text("Mock response".to_string())
        );
    }

    #[tokio::test]
    async fn test_resilient_provider_delegates_model_methods() {
        let mock = Box::new(MockProvider);
        let provider = ResilientProvider::with_defaults(mock);

        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.default_model(), "mock-v1");
        assert_eq!(provider.model(), "mock-v1");
    }

    #[tokio::test]
    async fn test_resilient_provider_rate_limiting() {
        let mock = Box::new(MockProvider);
        let config =
            ResilientProviderConfig::default().with_rate_limiter(RateLimiterConfig::new(1000.0, 5)); // 5 burst

        let provider = ResilientProvider::new(mock, config);

        // Should allow first 5 requests (burst)
        for i in 0..5 {
            let result = provider.infer("Hello", "mock-v1").await;
            assert!(result.is_ok(), "Request {} should succeed", i);
        }

        // Check tokens are depleted
        let available = provider.available_tokens();
        assert!(
            available < 1.0,
            "Expected depleted tokens, got {}",
            available
        );
    }

    #[tokio::test]
    async fn test_resilient_provider_circuit_breaker_integration() {
        // Create a failing provider
        struct FailingProvider {
            call_count: AtomicU32,
        }

        #[async_trait]
        impl Provider for FailingProvider {
            async fn infer(&self, _prompt: &str, _model: &str) -> Result<String> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("Always fails"))
            }

            async fn chat(
                &self,
                _messages: &[Message],
                _tools: Option<&[ToolDefinition]>,
                _model: &str,
            ) -> Result<ChatResponse> {
                Err(anyhow::anyhow!("Always fails"))
            }

            fn default_model(&self) -> &str {
                "fail-v1"
            }

            fn name(&self) -> &str {
                "failing"
            }
        }

        let failing = Box::new(FailingProvider {
            call_count: AtomicU32::new(0),
        });

        let config = ResilientProviderConfig::default()
            .rate_limiting(false) // Disable for this test
            .retry(false) // Disable retry for faster test
            .with_circuit_breaker(CircuitBreakerConfig::default().with_failure_threshold(3));

        let provider = ResilientProvider::new(failing, config);

        // First 3 calls should hit the actual provider
        for _ in 0..3 {
            let _ = provider.infer("Hello", "fail-v1").await;
        }

        // Circuit should now be open
        assert_eq!(
            provider.circuit_state(),
            crate::resilience::CircuitState::Open
        );

        // Next call should fail fast (not reach the provider)
        let result = provider.infer("Hello", "fail-v1").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circuit breaker open"));
    }

    #[tokio::test]
    async fn test_resilient_provider_retry_integration() {
        // Create a provider that fails then succeeds
        struct FlakeyProvider {
            call_count: AtomicU32,
            fail_until: u32,
        }

        #[async_trait]
        impl Provider for FlakeyProvider {
            async fn infer(&self, _prompt: &str, _model: &str) -> Result<String> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count <= self.fail_until {
                    Err(anyhow::anyhow!("temporary failure"))
                } else {
                    Ok("success after retries".to_string())
                }
            }

            async fn chat(
                &self,
                _messages: &[Message],
                _tools: Option<&[ToolDefinition]>,
                _model: &str,
            ) -> Result<ChatResponse> {
                Ok(ChatResponse {
                    content: MessageContent::Text("success".to_string()),
                    tool_calls: vec![],
                    stop_reason: StopReason::EndTurn,
                    usage: Usage::new(10, 10),
                })
            }

            fn default_model(&self) -> &str {
                "flakey-v1"
            }

            fn name(&self) -> &str {
                "flakey"
            }
        }

        let flakey = Box::new(FlakeyProvider {
            call_count: AtomicU32::new(0),
            fail_until: 2, // Fail first 2 calls
        });

        let config = ResilientProviderConfig::retry_only().with_retry(
            RetryConfig::default()
                .with_max_retries(3)
                .with_initial_delay(Duration::from_millis(1))
                .with_jitter(0.0),
        );

        let provider = ResilientProvider::new(flakey, config);

        // Should eventually succeed after retries
        let result = provider.infer("Hello", "flakey-v1").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success after retries");
    }

    #[test]
    fn test_resilient_provider_debug() {
        let mock = Box::new(MockProvider);
        let provider = ResilientProvider::with_defaults(mock);

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("mock"));
        assert!(debug_str.contains("Closed")); // Circuit state
    }

    #[tokio::test]
    async fn test_resilient_provider_reset_methods() {
        let mock = Box::new(MockProvider);
        let config =
            ResilientProviderConfig::default().with_rate_limiter(RateLimiterConfig::new(1.0, 1)); // Very limited

        let provider = ResilientProvider::new(mock, config);

        // Exhaust rate limiter
        provider.rate_limiter.try_acquire();
        assert!(provider.available_tokens() < 1.0);

        // Reset should restore
        provider.reset_rate_limiter();
        assert!(provider.available_tokens() >= 1.0);

        // Force circuit breaker open
        provider.circuit_breaker.force_open();
        assert_eq!(
            provider.circuit_state(),
            crate::resilience::CircuitState::Open
        );

        // Reset should close
        provider.reset_circuit_breaker();
        assert_eq!(
            provider.circuit_state(),
            crate::resilience::CircuitState::Closed
        );
    }
}
