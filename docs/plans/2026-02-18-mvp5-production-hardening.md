# MVP 5: Production Hardening - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Nika production-ready with robust error handling, retry logic, rate limiting, and OpenAI provider support.

**Architecture:** Resilience patterns (retry, circuit breaker, rate limiting) as composable middleware around provider and MCP calls.

**Tech Stack:** tokio, tower (middleware), backoff crate, metrics

**Prerequisites:** MVP 4 completed (real integration working)

---

## Task 1: Add Tool Calling to OpenAI Provider

**Files:**
- Modify: `nika-dev/tools/nika/src/provider/openai.rs`
- Modify: `nika-dev/tools/nika/src/provider/types.rs`

**Step 1: Write failing test**

```rust
// src/provider/openai.rs - add test
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_openai_chat_with_tools() {
        let provider = OpenAiProvider::new("gpt-4o".to_string());

        let tools = vec![
            ToolDefinition {
                name: "get_weather".to_string(),
                description: "Get current weather".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": { "type": "string" }
                    },
                    "required": ["location"]
                }),
            }
        ];

        let messages = vec![
            Message {
                role: Role::User,
                content: "What's the weather in Paris?".to_string(),
                tool_calls: None,
                tool_call_id: None,
            }
        ];

        let response = provider.chat(messages, Some(tools)).await.unwrap();

        // Should either have content or tool_calls
        assert!(response.content.is_some() || !response.tool_calls.is_empty());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_openai_chat_with_tools`
Expected: FAIL (chat method not implemented)

**Step 3: Implement OpenAI tool calling**

```rust
// src/provider/openai.rs
use crate::NikaError;
use super::types::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAiProvider {
    client: Client,
    model: String,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            model,
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Serialize, Deserialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn complete(&self, prompt: &str) -> Result<String, NikaError> {
        let messages = vec![Message {
            role: Role::User,
            content: prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = self.chat(messages, None).await?;
        Ok(response.content.unwrap_or_default())
    }

    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<ChatResponse, NikaError> {
        let openai_messages: Vec<OpenAiMessage> = messages.iter().map(|m| {
            OpenAiMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: Some(m.content.clone()),
                tool_calls: m.tool_calls.as_ref().map(|calls| {
                    calls.iter().map(|c| OpenAiToolCall {
                        id: c.id.clone(),
                        call_type: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: c.name.clone(),
                            arguments: serde_json::to_string(&c.arguments).unwrap(),
                        },
                    }).collect()
                }),
                tool_call_id: m.tool_call_id.clone(),
            }
        }).collect();

        let openai_tools = tools.map(|t| {
            t.iter().map(|tool| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunction {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.input_schema.clone(),
                },
            }).collect()
        });

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: openai_messages,
            tools: openai_tools,
            tool_choice: None,
        };

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| NikaError::ProviderError {
                provider: "openai".to_string(),
                source: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(NikaError::ProviderError {
                provider: "openai".to_string(),
                source: error_text,
            });
        }

        let openai_response: OpenAiResponse = response.json().await
            .map_err(|e| NikaError::ProviderError {
                provider: "openai".to_string(),
                source: e.to_string(),
            })?;

        let choice = &openai_response.choices[0];
        let tool_calls: Vec<ToolCall> = choice.message.tool_calls
            .as_ref()
            .map(|calls| {
                calls.iter().map(|c| ToolCall {
                    id: c.id.clone(),
                    name: c.function.name.clone(),
                    arguments: serde_json::from_str(&c.function.arguments)
                        .unwrap_or(serde_json::json!({})),
                }).collect()
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content: choice.message.content.clone(),
            tool_calls,
            stop_reason: match choice.finish_reason.as_str() {
                "stop" => StopReason::EndTurn,
                "tool_calls" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                _ => StopReason::EndTurn,
            },
            usage: Usage {
                input_tokens: openai_response.usage.prompt_tokens,
                output_tokens: openai_response.usage.completion_tokens,
            },
        })
    }
}
```

**Step 4: Run test**

Run: `cargo test test_openai_chat_with_tools`
Expected: PASS (with OPENAI_API_KEY set)

**Step 5: Commit**

```bash
git add src/provider/openai.rs
git commit -m "feat(provider): add tool calling to OpenAI provider

- Full OpenAI chat completions API with tools
- Function calling format conversion
- Usage tracking

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Create Retry Module with Exponential Backoff

**Files:**
- Create: `nika-dev/tools/nika/src/resilience/mod.rs`
- Create: `nika-dev/tools/nika/src/resilience/retry.rs`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Add backoff dependency**

```toml
# Cargo.toml
[dependencies]
backoff = { version = "0.4", features = ["tokio"] }
```

**Step 2: Write failing test**

```rust
// src/resilience/retry.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_succeeds_on_third_attempt() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = with_retry(
            RetryConfig::default(),
            || async {
                let count = attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 2 {
                    Err(RetryableError::Transient("temporary failure".to_string()))
                } else {
                    Ok("success")
                }
            },
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_gives_up_after_max_attempts() {
        let result = with_retry(
            RetryConfig { max_retries: 3, ..Default::default() },
            || async {
                Err::<(), _>(RetryableError::Transient("always fails".to_string()))
            },
        ).await;

        assert!(result.is_err());
    }
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test test_retry`
Expected: FAIL (module not implemented)

**Step 4: Implement retry module**

```rust
// src/resilience/mod.rs
pub mod retry;
pub mod circuit_breaker;
pub mod rate_limit;

pub use retry::{with_retry, RetryConfig, RetryableError};
pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use rate_limit::RateLimiter;
```

```rust
// src/resilience/retry.rs
use std::future::Future;
use std::time::Duration;
use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries (0 = no retries)
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_interval: Duration,
    /// Maximum delay between retries
    pub max_interval: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
    /// Add randomization to prevent thundering herd
    pub randomization_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_interval: Duration::from_millis(100),
            max_interval: Duration::from_secs(30),
            multiplier: 2.0,
            randomization_factor: 0.5,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RetryableError {
    #[error("Transient error (will retry): {0}")]
    Transient(String),
    #[error("Permanent error (no retry): {0}")]
    Permanent(String),
}

impl RetryableError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }

    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Permanent(_))
    }
}

/// Execute an async operation with exponential backoff retry
pub async fn with_retry<T, E, F, Fut>(
    config: RetryConfig,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let backoff = ExponentialBackoffBuilder::default()
        .with_initial_interval(config.initial_interval)
        .with_max_interval(config.max_interval)
        .with_multiplier(config.multiplier)
        .with_randomization_factor(config.randomization_factor)
        .with_max_elapsed_time(None)
        .build();

    let mut attempts = 0;
    let mut current_interval = config.initial_interval;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts > config.max_retries {
                    return Err(e);
                }

                // Check if error is permanent (for RetryableError)
                let error_str = format!("{:?}", e);
                if error_str.contains("Permanent") {
                    return Err(e);
                }

                tracing::warn!(
                    attempt = attempts,
                    max_retries = config.max_retries,
                    "Operation failed, retrying after {:?}",
                    current_interval
                );

                sleep(current_interval).await;

                // Calculate next interval with jitter
                let jitter = (rand::random::<f64>() - 0.5) * 2.0 * config.randomization_factor;
                current_interval = Duration::from_secs_f64(
                    (current_interval.as_secs_f64() * config.multiplier * (1.0 + jitter))
                        .min(config.max_interval.as_secs_f64())
                );
            }
        }
    }
}

/// Classify HTTP status codes for retry decisions
pub fn should_retry_http(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}
```

**Step 5: Run tests**

Run: `cargo test test_retry`
Expected: PASS

**Step 6: Commit**

```bash
git add src/resilience/ Cargo.toml
git commit -m "feat(resilience): add retry module with exponential backoff

- RetryConfig with configurable parameters
- Exponential backoff with jitter
- Transient vs permanent error distinction
- HTTP status code classification

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Add Circuit Breaker Pattern

**Files:**
- Create: `nika-dev/tools/nika/src/resilience/circuit_breaker.rs`

**Step 1: Write failing test**

```rust
// src/resilience/circuit_breaker.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_millis(100),
            half_open_max_calls: 1,
        });

        // Fail 3 times
        for _ in 0..3 {
            let _ = cb.call(|| async { Err::<(), _>("error") }).await;
        }

        assert_eq!(cb.state(), CircuitState::Open);

        // Should reject immediately
        let result = cb.call(|| async { Ok::<_, &str>(()) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[tokio::test]
    async fn test_circuit_recovers_after_timeout() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            reset_timeout: Duration::from_millis(50),
            half_open_max_calls: 1,
        });

        // Fail to open
        for _ in 0..2 {
            let _ = cb.call(|| async { Err::<(), _>("error") }).await;
        }

        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for reset
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should be half-open, allow one call
        let result = cb.call(|| async { Ok::<_, &str>("success") }).await;
        assert!(result.is_ok());
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_circuit`
Expected: FAIL

**Step 3: Implement circuit breaker**

```rust
// src/resilience/circuit_breaker.rs
use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Time to wait before trying again
    pub reset_timeout: Duration,
    /// Max calls allowed in half-open state
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open")]
    Open,
    #[error("Operation failed: {0}")]
    Inner(E),
}

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    state: RwLock<CircuitState>,
    last_failure_time: RwLock<Option<Instant>>,
    half_open_calls: AtomicU32,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            state: RwLock::new(CircuitState::Closed),
            last_failure_time: RwLock::new(None),
            half_open_calls: AtomicU32::new(0),
        }
    }

    pub fn state(&self) -> CircuitState {
        let mut state = self.state.write();

        // Check if we should transition from Open to HalfOpen
        if *state == CircuitState::Open {
            if let Some(last_failure) = *self.last_failure_time.read() {
                if last_failure.elapsed() >= self.config.reset_timeout {
                    *state = CircuitState::HalfOpen;
                    self.half_open_calls.store(0, Ordering::SeqCst);
                }
            }
        }

        *state
    }

    pub async fn call<T, E, F, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let current_state = self.state();

        match current_state {
            CircuitState::Open => {
                return Err(CircuitBreakerError::Open);
            }
            CircuitState::HalfOpen => {
                let calls = self.half_open_calls.fetch_add(1, Ordering::SeqCst);
                if calls >= self.config.half_open_max_calls {
                    return Err(CircuitBreakerError::Open);
                }
            }
            CircuitState::Closed => {}
        }

        match operation().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(e) => {
                self.on_failure();
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }

    fn on_success(&self) {
        let mut state = self.state.write();

        match *state {
            CircuitState::HalfOpen => {
                // Success in half-open: close circuit
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
                *state = CircuitState::Closed;
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    fn on_failure(&self) {
        let mut state = self.state.write();

        match *state {
            CircuitState::HalfOpen => {
                // Failure in half-open: open circuit again
                *self.last_failure_time.write() = Some(Instant::now());
                *state = CircuitState::Open;
            }
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    *self.last_failure_time.write() = Some(Instant::now());
                    *state = CircuitState::Open;
                }
            }
            _ => {}
        }
    }

    pub fn reset(&self) {
        *self.state.write() = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        *self.last_failure_time.write() = None;
    }
}
```

**Step 4: Run tests**

Run: `cargo test test_circuit`
Expected: PASS

**Step 5: Commit**

```bash
git add src/resilience/circuit_breaker.rs
git commit -m "feat(resilience): add circuit breaker pattern

- Three states: Closed, Open, HalfOpen
- Configurable failure threshold and reset timeout
- Automatic state transitions

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Implement Rate Limiting for Providers

**Files:**
- Create: `nika-dev/tools/nika/src/resilience/rate_limit.rs`

**Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 10,
        });

        for _ in 0..10 {
            assert!(limiter.acquire().await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 2,
        });

        // Use up burst
        limiter.acquire().await.unwrap();
        limiter.acquire().await.unwrap();

        // Third should wait or fail
        let start = Instant::now();
        limiter.acquire().await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(900)); // ~1 second wait
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_rate_limiter`
Expected: FAIL

**Step 3: Implement rate limiter (token bucket)**

```rust
// src/resilience/rate_limit.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    /// Burst size (max tokens at once)
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            burst_size: 10,
        }
    }
}

/// Token bucket rate limiter
pub struct RateLimiter {
    config: RateLimitConfig,
    state: Arc<Mutex<RateLimiterState>>,
}

struct RateLimiterState {
    tokens: f64,
    last_update: Instant,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(RateLimiterState {
                tokens: config.burst_size as f64,
                last_update: Instant::now(),
            })),
            config,
        }
    }

    /// Acquire a token, waiting if necessary
    pub async fn acquire(&self) -> Result<(), RateLimitError> {
        loop {
            let wait_time = {
                let mut state = self.state.lock();

                // Refill tokens based on elapsed time
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_update);
                let tokens_to_add = elapsed.as_secs_f64()
                    * (self.config.requests_per_minute as f64 / 60.0);

                state.tokens = (state.tokens + tokens_to_add)
                    .min(self.config.burst_size as f64);
                state.last_update = now;

                if state.tokens >= 1.0 {
                    state.tokens -= 1.0;
                    return Ok(());
                }

                // Calculate wait time for next token
                let tokens_needed = 1.0 - state.tokens;
                let seconds_per_token = 60.0 / self.config.requests_per_minute as f64;
                Duration::from_secs_f64(tokens_needed * seconds_per_token)
            };

            sleep(wait_time).await;
        }
    }

    /// Try to acquire a token without waiting
    pub fn try_acquire(&self) -> Result<(), RateLimitError> {
        let mut state = self.state.lock();

        // Refill tokens
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_update);
        let tokens_to_add = elapsed.as_secs_f64()
            * (self.config.requests_per_minute as f64 / 60.0);

        state.tokens = (state.tokens + tokens_to_add)
            .min(self.config.burst_size as f64);
        state.last_update = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            Ok(())
        } else {
            Err(RateLimitError::Exceeded)
        }
    }

    pub fn available_tokens(&self) -> f64 {
        let state = self.state.lock();
        state.tokens
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded")]
    Exceeded,
}
```

**Step 4: Run tests**

Run: `cargo test test_rate_limiter`
Expected: PASS

**Step 5: Commit**

```bash
git add src/resilience/rate_limit.rs
git commit -m "feat(resilience): add token bucket rate limiter

- Configurable requests per minute
- Burst capacity support
- Blocking acquire and non-blocking try_acquire

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Integrate Resilience into Providers

**Files:**
- Modify: `nika-dev/tools/nika/src/provider/claude.rs`
- Modify: `nika-dev/tools/nika/src/provider/openai.rs`

**Step 1: Add resilience to Claude provider**

```rust
// src/provider/claude.rs - add resilience
use crate::resilience::{with_retry, RetryConfig, CircuitBreaker, CircuitBreakerConfig, RateLimiter, RateLimitConfig};

pub struct ClaudeProvider {
    client: Client,
    model: String,
    api_key: String,
    retry_config: RetryConfig,
    circuit_breaker: CircuitBreaker,
    rate_limiter: RateLimiter,
}

impl ClaudeProvider {
    pub fn new(model: String) -> Self {
        Self {
            client: Client::new(),
            model,
            api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            retry_config: RetryConfig::default(),
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
            rate_limiter: RateLimiter::new(RateLimitConfig {
                requests_per_minute: 50, // Claude rate limit
                burst_size: 5,
            }),
        }
    }

    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for ClaudeProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<ChatResponse, NikaError> {
        // Rate limit
        self.rate_limiter.acquire().await
            .map_err(|_| NikaError::RateLimited { provider: "claude".to_string() })?;

        // Circuit breaker + retry
        let result = self.circuit_breaker.call(|| async {
            with_retry(self.retry_config.clone(), || async {
                self.chat_internal(messages.clone(), tools.clone()).await
            }).await
        }).await;

        match result {
            Ok(response) => Ok(response),
            Err(crate::resilience::CircuitBreakerError::Open) => {
                Err(NikaError::CircuitOpen { provider: "claude".to_string() })
            }
            Err(crate::resilience::CircuitBreakerError::Inner(e)) => Err(e),
        }
    }

    // ... rest of implementation
}
```

**Step 2: Add error variants**

```rust
// Add to src/error.rs
#[error("[NIKA-120] Rate limit exceeded for provider '{provider}'")]
RateLimited { provider: String },

#[error("[NIKA-121] Circuit breaker open for provider '{provider}'")]
CircuitOpen { provider: String },
```

**Step 3: Run tests**

Run: `cargo test provider`
Expected: PASS

**Step 4: Commit**

```bash
git add src/provider/ src/error.rs
git commit -m "feat(provider): integrate resilience patterns

- Rate limiting per provider
- Circuit breaker for failures
- Retry with exponential backoff
- New error codes NIKA-120, NIKA-121

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Add Performance Metrics Collection

**Files:**
- Create: `nika-dev/tools/nika/src/metrics/mod.rs`
- Create: `nika-dev/tools/nika/src/metrics/collector.rs`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Add metrics dependency**

```toml
[dependencies]
metrics = "0.22"
metrics-exporter-prometheus = { version = "0.13", optional = true }

[features]
metrics = ["metrics-exporter-prometheus"]
```

**Step 2: Create metrics collector**

```rust
// src/metrics/mod.rs
pub mod collector;
pub use collector::MetricsCollector;
```

```rust
// src/metrics/collector.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct WorkflowMetrics {
    pub total_duration: Duration,
    pub task_count: u32,
    pub successful_tasks: u32,
    pub failed_tasks: u32,
    pub total_tokens: u64,
    pub mcp_calls: u32,
    pub provider_calls: u32,
}

#[derive(Debug, Clone)]
pub struct TaskMetrics {
    pub name: String,
    pub duration: Duration,
    pub tokens_used: u64,
    pub tool_calls: u32,
    pub success: bool,
}

pub struct MetricsCollector {
    workflow_start: Instant,
    task_metrics: Arc<RwLock<Vec<TaskMetrics>>>,
    counters: Arc<RwLock<HashMap<String, u64>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            workflow_start: Instant::now(),
            task_metrics: Arc::new(RwLock::new(Vec::new())),
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn record_task(&self, metrics: TaskMetrics) {
        self.task_metrics.write().push(metrics);
    }

    pub fn increment(&self, name: &str, value: u64) {
        let mut counters = self.counters.write();
        *counters.entry(name.to_string()).or_insert(0) += value;
    }

    pub fn record_provider_call(&self, tokens: u64) {
        self.increment("provider_calls", 1);
        self.increment("total_tokens", tokens);
    }

    pub fn record_mcp_call(&self) {
        self.increment("mcp_calls", 1);
    }

    pub fn finalize(&self) -> WorkflowMetrics {
        let tasks = self.task_metrics.read();
        let counters = self.counters.read();

        WorkflowMetrics {
            total_duration: self.workflow_start.elapsed(),
            task_count: tasks.len() as u32,
            successful_tasks: tasks.iter().filter(|t| t.success).count() as u32,
            failed_tasks: tasks.iter().filter(|t| !t.success).count() as u32,
            total_tokens: *counters.get("total_tokens").unwrap_or(&0),
            mcp_calls: *counters.get("mcp_calls").unwrap_or(&0) as u32,
            provider_calls: *counters.get("provider_calls").unwrap_or(&0) as u32,
        }
    }

    pub fn summary(&self) -> String {
        let metrics = self.finalize();
        format!(
            "Workflow completed in {:?}\n\
             Tasks: {} total, {} succeeded, {} failed\n\
             Provider calls: {}, MCP calls: {}\n\
             Total tokens: {}",
            metrics.total_duration,
            metrics.task_count,
            metrics.successful_tasks,
            metrics.failed_tasks,
            metrics.provider_calls,
            metrics.mcp_calls,
            metrics.total_tokens
        )
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let collector = MetricsCollector::new();

        collector.record_task(TaskMetrics {
            name: "task1".to_string(),
            duration: Duration::from_millis(100),
            tokens_used: 500,
            tool_calls: 2,
            success: true,
        });

        collector.record_provider_call(500);
        collector.record_mcp_call();

        let metrics = collector.finalize();
        assert_eq!(metrics.task_count, 1);
        assert_eq!(metrics.successful_tasks, 1);
        assert_eq!(metrics.total_tokens, 500);
        assert_eq!(metrics.mcp_calls, 1);
    }
}
```

**Step 3: Run tests**

Run: `cargo test test_metrics`
Expected: PASS

**Step 4: Commit**

```bash
git add src/metrics/ Cargo.toml
git commit -m "feat(metrics): add performance metrics collection

- WorkflowMetrics summary struct
- TaskMetrics per-task tracking
- Counter-based aggregation
- Summary output formatting

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Create Benchmarks

**Files:**
- Create: `nika-dev/tools/nika/benches/workflow_bench.rs`
- Modify: `nika-dev/tools/nika/Cargo.toml`

**Step 1: Add criterion dependency**

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["async_tokio"] }

[[bench]]
name = "workflow_bench"
harness = false
```

**Step 2: Create benchmark**

```rust
// benches/workflow_bench.rs
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use nika::{Workflow, parse_workflow};
use std::time::Duration;

fn parse_workflow_benchmark(c: &mut Criterion) {
    let simple_workflow = r#"
name: simple
version: "1.0"
tasks:
  greet:
    exec: echo "Hello"
"#;

    let complex_workflow = r#"
name: complex
version: "1.0"
mcp:
  novanet:
    command: node
    args: ["server.js"]
tasks:
  fetch:
    invoke: novanet_generate
    params:
      entity: test
  process:
    depends_on: [fetch]
    infer: "Process the data"
    context: $fetch.result
  validate:
    depends_on: [process]
    exec: "echo 'done'"
"#;

    let mut group = c.benchmark_group("parse_workflow");

    group.bench_with_input(
        BenchmarkId::new("simple", "1 task"),
        simple_workflow,
        |b, workflow| {
            b.iter(|| parse_workflow(workflow).unwrap())
        }
    );

    group.bench_with_input(
        BenchmarkId::new("complex", "3 tasks + mcp"),
        complex_workflow,
        |b, workflow| {
            b.iter(|| parse_workflow(workflow).unwrap())
        }
    );

    group.finish();
}

fn dag_resolution_benchmark(c: &mut Criterion) {
    // Create workflows with varying dependency depths
    let mut group = c.benchmark_group("dag_resolution");

    for depth in [5, 10, 20] {
        let mut tasks = String::new();
        for i in 0..depth {
            if i == 0 {
                tasks.push_str(&format!(
                    "  task_{}: {{ exec: \"echo {}\" }}\n",
                    i, i
                ));
            } else {
                tasks.push_str(&format!(
                    "  task_{}: {{ depends_on: [task_{}], exec: \"echo {}\" }}\n",
                    i, i - 1, i
                ));
            }
        }

        let workflow = format!(
            "name: deep_dag\nversion: \"1.0\"\ntasks:\n{}",
            tasks
        );

        group.bench_with_input(
            BenchmarkId::new("linear_chain", depth),
            &workflow,
            |b, workflow| {
                let parsed = parse_workflow(workflow).unwrap();
                b.iter(|| nika::build_dag(&parsed).unwrap())
            }
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = parse_workflow_benchmark, dag_resolution_benchmark
}

criterion_main!(benches);
```

**Step 3: Run benchmarks**

Run: `cargo bench`
Expected: Benchmark results displayed

**Step 4: Commit**

```bash
git add benches/ Cargo.toml
git commit -m "bench: add workflow parsing and DAG resolution benchmarks

- Parse simple vs complex workflows
- DAG resolution with varying depths
- Criterion setup with async_tokio

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 5, you will have:

- OpenAI provider with full tool calling support
- Retry with exponential backoff and jitter
- Circuit breaker for cascade failure prevention
- Rate limiting per provider
- Performance metrics collection
- Benchmark baseline for regressions

**Total tasks:** 7
**New error codes:** NIKA-120, NIKA-121
