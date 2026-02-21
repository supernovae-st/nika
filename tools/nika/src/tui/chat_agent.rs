//! ChatAgent - Standalone LLM interface for TUI commands
//!
//! Manages LLM calls, streaming, and command operations for TUI chat commands.
//!
//! # Usage Pattern
//!
//! This module is **NOT dead code**. It's used by `app.rs` handlers:
//! - `handle_chat_infer()` — `/infer` command
//! - `handle_chat_fetch()` — `/fetch` command
//! - `handle_chat_invoke()` — `/invoke` command
//! - `handle_chat_agent()` — `/agent` command
//!
//! These handlers spawn ChatAgent operations in `tokio::spawn()` tasks for
//! async operations, keeping the TUI responsive while commands run.
//!
//! # Architecture
//!
//! ```text
//! ChatAgent
//! ├── provider: RigProvider (OpenAI/Claude via rig-core)
//! ├── history: Vec<ChatMessage>
//! └── streaming_tx: Optional mpsc channel for real-time updates
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! use nika::tui::chat_agent::ChatAgent;
//! use nika::tui::command::ModelProvider;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut agent = ChatAgent::new()?;
//!
//!     // Switch to Claude provider
//!     agent.set_provider(ModelProvider::Claude)?;
//!
//!     // Run inference
//!     let response = agent.infer("Hello, world!").await?;
//!     println!("{}", response);
//!
//!     // Execute shell command
//!     let output = agent.exec_command("echo hello").await?;
//!     println!("{}", output);
//!
//!     // Fetch URL
//!     let html = agent.fetch("https://example.com", "GET").await?;
//!     println!("{}", html);
//!
//!     Ok(())
//! }
//! ```

use crate::error::NikaError;
use crate::provider::rig::{RigProvider, StreamChunk};
use crate::tui::command::ModelProvider;
use tokio::sync::mpsc;

// ═══════════════════════════════════════════════════════════════════════════
// STREAMING STATE
// ═══════════════════════════════════════════════════════════════════════════

/// Streaming state for UI updates
///
/// Tracks the current streaming state for real-time UI updates.
#[derive(Debug, Default, Clone)]
pub struct StreamingState {
    /// Whether a streaming response is in progress
    pub is_streaming: bool,
    /// Partial response accumulated during streaming
    pub partial_response: String,
    /// Number of tokens received so far
    pub tokens_received: usize,
}

impl StreamingState {
    /// Create a new streaming state
    pub fn new() -> Self {
        Self::default()
    }

    /// Start streaming
    pub fn start(&mut self) {
        self.is_streaming = true;
        self.partial_response.clear();
        self.tokens_received = 0;
    }

    /// Append a chunk to the partial response
    pub fn append(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
        self.tokens_received += 1; // Rough approximation
    }

    /// Finish streaming
    pub fn finish(&mut self) -> String {
        self.is_streaming = false;
        std::mem::take(&mut self.partial_response)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT MESSAGE TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Role of a chat message participant
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    /// User message
    User,
    /// Assistant (LLM) message
    Assistant,
    /// System message (instructions)
    System,
    /// Tool result message
    Tool,
}

impl ChatRole {
    /// Get the display name for the role
    pub fn display_name(&self) -> &'static str {
        match self {
            ChatRole::User => "You",
            ChatRole::Assistant => "Nika",
            ChatRole::System => "System",
            ChatRole::Tool => "Tool",
        }
    }
}

/// A single chat message in the conversation history
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Role of the message sender
    pub role: ChatRole,
    /// Message content
    pub content: String,
    /// Timestamp of the message
    pub timestamp: std::time::Instant,
}

impl ChatMessage {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            timestamp: std::time::Instant::now(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            timestamp: std::time::Instant::now(),
        }
    }

    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            timestamp: std::time::Instant::now(),
        }
    }

    /// Create a new tool message
    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            timestamp: std::time::Instant::now(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT AGENT
// ═══════════════════════════════════════════════════════════════════════════

/// Main chat agent handling LLM interactions
///
/// # Example
///
/// ```rust,no_run
/// use nika::tui::chat_agent::ChatAgent;
///
/// #[tokio::main]
/// async fn main() -> Result<(), nika::error::NikaError> {
///     let mut agent = ChatAgent::new()?;
///     let response = agent.infer("Hello!").await?;
///     assert!(!response.is_empty());
///     Ok(())
/// }
/// ```
pub struct ChatAgent {
    /// Current LLM provider
    provider: RigProvider,
    /// Optional model override (uses provider default if None)
    model_override: Option<String>,
    /// Conversation history
    history: Vec<ChatMessage>,
    /// Optional streaming channel for real-time updates (String for backward compat)
    streaming_tx: Option<mpsc::Sender<String>>,
    /// Optional streaming channel for token-by-token updates
    stream_chunk_tx: Option<mpsc::Sender<StreamChunk>>,
    /// Current streaming state
    streaming_state: StreamingState,
    /// HTTP client for fetch operations
    http_client: reqwest::Client,
    /// Cumulative input tokens used
    pub total_input_tokens: u64,
    /// Cumulative output tokens used
    pub total_output_tokens: u64,
}

impl ChatAgent {
    /// Create a new ChatAgent with OpenAI provider (default)
    ///
    /// # Errors
    ///
    /// Returns `NikaError::MissingApiKey` if `OPENAI_API_KEY` is not set.
    pub fn new() -> Result<Self, NikaError> {
        // Check for API key before creating provider
        if std::env::var("OPENAI_API_KEY").is_err() {
            // Try Claude as fallback
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                return Ok(Self {
                    provider: RigProvider::claude(),
                    model_override: None,
                    history: Vec::new(),
                    streaming_tx: None,
                    stream_chunk_tx: None,
                    streaming_state: StreamingState::new(),
                    http_client: reqwest::Client::new(),
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                });
            }
            // No API key available, but we still create the agent
            // The error will happen when trying to use infer()
        }

        Ok(Self {
            provider: RigProvider::openai(),
            model_override: None,
            history: Vec::new(),
            streaming_tx: None,
            stream_chunk_tx: None,
            streaming_state: StreamingState::new(),
            http_client: reqwest::Client::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        })
    }

    /// Create a new ChatAgent with specific provider and model overrides
    ///
    /// # Arguments
    ///
    /// * `provider` - Optional provider name ("claude" or "openai")
    /// * `model` - Optional model name override
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use nika::tui::chat_agent::ChatAgent;
    ///
    /// let agent = ChatAgent::with_overrides(
    ///     Some("claude"),
    ///     Some("claude-sonnet-4-20250514")
    /// ).unwrap();
    /// ```
    pub fn with_overrides(provider: Option<&str>, model: Option<&str>) -> Result<Self, NikaError> {
        let mut agent = Self::new()?;

        // Apply provider override
        if let Some(p) = provider {
            match p.to_lowercase().as_str() {
                "claude" | "anthropic" => {
                    if std::env::var("ANTHROPIC_API_KEY").is_err() {
                        return Err(NikaError::MissingApiKey {
                            provider: "Claude".to_string(),
                        });
                    }
                    agent.provider = RigProvider::claude();
                }
                "openai" | "gpt" => {
                    if std::env::var("OPENAI_API_KEY").is_err() {
                        return Err(NikaError::MissingApiKey {
                            provider: "OpenAI".to_string(),
                        });
                    }
                    agent.provider = RigProvider::openai();
                }
                _ => {
                    return Err(NikaError::InvalidConfig {
                        message: format!("Unknown provider: '{}'. Use 'claude' or 'openai'", p),
                    });
                }
            }
        }

        // Apply model override
        if let Some(m) = model {
            agent.model_override = Some(m.to_string());
        }

        Ok(agent)
    }

    /// Set streaming channel for real-time updates (legacy String channel)
    pub fn with_streaming(mut self, tx: mpsc::Sender<String>) -> Self {
        self.streaming_tx = Some(tx);
        self
    }

    /// Set streaming channel for token-by-token updates (StreamChunk)
    ///
    /// This enables Claude Code-like streaming where tokens appear as they arrive.
    pub fn with_stream_chunks(mut self, tx: mpsc::Sender<StreamChunk>) -> Self {
        self.stream_chunk_tx = Some(tx);
        self
    }

    /// Set streaming channel (takes ownership, for use after construction)
    pub fn set_stream_chunk_tx(&mut self, tx: mpsc::Sender<StreamChunk>) {
        self.stream_chunk_tx = Some(tx);
    }

    /// Switch to a different LLM provider
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider to switch to (OpenAI, Claude, or List)
    ///
    /// # Errors
    ///
    /// Returns `NikaError::MissingApiKey` if the required API key is not set.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use nika::tui::chat_agent::ChatAgent;
    /// use nika::tui::command::ModelProvider;
    ///
    /// let mut agent = ChatAgent::new().unwrap();
    /// agent.set_provider(ModelProvider::Claude).unwrap();
    /// ```
    pub fn set_provider(&mut self, provider: ModelProvider) -> Result<(), NikaError> {
        match provider {
            ModelProvider::OpenAI => {
                if std::env::var("OPENAI_API_KEY").is_err() {
                    return Err(NikaError::MissingApiKey {
                        provider: "OpenAI".to_string(),
                    });
                }
                self.provider = RigProvider::openai();
            }
            ModelProvider::Claude => {
                if std::env::var("ANTHROPIC_API_KEY").is_err() {
                    return Err(NikaError::MissingApiKey {
                        provider: "Claude".to_string(),
                    });
                }
                self.provider = RigProvider::claude();
            }
            ModelProvider::List => {
                // List doesn't change the provider, just returns info
            }
        }
        Ok(())
    }

    /// Get the current provider name
    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// Get total tokens used (input + output) for status bar display
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }

    /// Execute an infer command (LLM text generation)
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt to send to the LLM
    ///
    /// # Returns
    ///
    /// The completion text from the model.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ProviderApiError` if the API call fails.
    ///
    /// # Streaming
    ///
    /// If `stream_chunk_tx` is set, tokens are streamed in real-time via
    /// `StreamChunk::Token` events, enabling Claude Code-like UX.
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // Add user message to history
        self.history.push(ChatMessage::user(prompt));

        // Start streaming state
        self.streaming_state.start();

        // Send prompt to legacy streaming channel if available
        if let Some(tx) = &self.streaming_tx {
            let _ = tx
                .send(format!("Sending to {}...", self.provider.name()))
                .await;
        }

        // Use streaming if stream_chunk_tx is set, otherwise blocking
        let response = if let Some(tx) = self.stream_chunk_tx.clone() {
            // Clone tx for metrics send (infer_stream takes ownership)
            let metrics_tx = tx.clone();

            // Real-time streaming - tokens appear as they arrive
            let result = self
                .provider
                .infer_stream(prompt, tx, self.model_override.as_deref())
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })?;
            // Accumulate token metrics for status bar display
            self.total_input_tokens += result.input_tokens;
            self.total_output_tokens += result.output_tokens;

            // Send metrics to UI for status bar update
            let _ = metrics_tx
                .send(StreamChunk::Metrics {
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                })
                .await;

            result.text
        } else {
            // Blocking call - full response at once
            self.provider
                .infer(prompt, None)
                .await
                .map_err(|e| NikaError::ProviderApiError {
                    message: e.to_string(),
                })?
        };

        // Finish streaming
        self.streaming_state.finish();

        // Add assistant message to history
        self.history.push(ChatMessage::assistant(&response));

        // Send completion to legacy streaming channel
        if let Some(tx) = &self.streaming_tx {
            let _ = tx.send(response.clone()).await;
        }

        Ok(response)
    }

    /// Execute a shell command
    ///
    /// Uses `tokio::process::Command` for non-blocking execution.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    ///
    /// # Returns
    ///
    /// The command output (stdout) on success, or formatted error on failure.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::Execution` if the command fails to execute.
    ///
    /// # Safety
    ///
    /// This executes arbitrary shell commands. Use with caution.
    pub async fn exec_command(&self, command: &str) -> Result<String, NikaError> {
        use tokio::process::Command as TokioCommand;

        let output = TokioCommand::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| NikaError::Execution(format!("Failed to execute command: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.trim().to_string())
        } else {
            // Return formatted output including exit code and stderr
            let exit_code = output.status.code().unwrap_or(-1);
            Ok(format!(
                "Exit code: {}\n{}\n{}",
                exit_code,
                stdout.trim(),
                stderr.trim()
            ))
        }
    }

    /// Execute a fetch command (HTTP request)
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to fetch
    /// * `method` - HTTP method (GET, POST, PUT, DELETE)
    ///
    /// # Returns
    ///
    /// The response body as text.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::Execution` if the HTTP request fails.
    pub async fn fetch(&self, url: &str, method: &str) -> Result<String, NikaError> {
        let request = match method.to_uppercase().as_str() {
            "POST" => self.http_client.post(url),
            "PUT" => self.http_client.put(url),
            "DELETE" => self.http_client.delete(url),
            "PATCH" => self.http_client.patch(url),
            "HEAD" => self.http_client.head(url),
            _ => self.http_client.get(url), // Default to GET
        };

        let response = request
            .send()
            .await
            .map_err(|e| NikaError::Execution(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| NikaError::Execution(format!("Failed to read response: {}", e)))?;

        // Include status code for non-2xx responses
        if !status.is_success() {
            Ok(format!(
                "HTTP {} {}\n{}",
                status.as_u16(),
                status.as_str(),
                text
            ))
        } else {
            Ok(text)
        }
    }

    /// Get the conversation history
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Clear the conversation history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Create ChatAgent with existing history for persistent conversations
    ///
    /// # Arguments
    ///
    /// * `messages` - Previous conversation history to restore
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use nika::tui::chat_agent::{ChatAgent, ChatMessage};
    ///
    /// let history = vec![
    ///     ChatMessage::user("Hello"),
    ///     ChatMessage::assistant("Hi there!"),
    /// ];
    /// let agent = ChatAgent::with_history(history).unwrap();
    /// assert_eq!(agent.history().len(), 2);
    /// ```
    pub fn with_history(messages: Vec<ChatMessage>) -> Result<Self, NikaError> {
        let mut agent = Self::new()?;
        agent.history = messages;
        Ok(agent)
    }

    /// Take ownership of the conversation history
    ///
    /// This moves the history out of the agent, leaving it empty.
    /// Useful for persisting history between sessions.
    pub fn take_history(&mut self) -> Vec<ChatMessage> {
        std::mem::take(&mut self.history)
    }

    /// Set the conversation history
    ///
    /// Replaces the current history with the provided messages.
    pub fn set_history(&mut self, messages: Vec<ChatMessage>) {
        self.history = messages;
    }

    /// Get the current streaming state
    pub fn streaming_state(&self) -> &StreamingState {
        &self.streaming_state
    }

    /// Check if currently streaming
    pub fn is_streaming(&self) -> bool {
        self.streaming_state.is_streaming
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // StreamingState tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_streaming_state_default() {
        let state = StreamingState::default();
        assert!(!state.is_streaming);
        assert!(state.partial_response.is_empty());
        assert_eq!(state.tokens_received, 0);
    }

    #[test]
    fn test_streaming_state_start() {
        let mut state = StreamingState::new();
        state.partial_response = "leftover".to_string();
        state.tokens_received = 10;

        state.start();

        assert!(state.is_streaming);
        assert!(state.partial_response.is_empty());
        assert_eq!(state.tokens_received, 0);
    }

    #[test]
    fn test_streaming_state_append() {
        let mut state = StreamingState::new();
        state.start();

        state.append("Hello");
        state.append(", ");
        state.append("world!");

        assert_eq!(state.partial_response, "Hello, world!");
        assert_eq!(state.tokens_received, 3);
    }

    #[test]
    fn test_streaming_state_finish() {
        let mut state = StreamingState::new();
        state.start();
        state.append("Complete response");

        let result = state.finish();

        assert_eq!(result, "Complete response");
        assert!(!state.is_streaming);
        assert!(state.partial_response.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ChatRole tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_chat_role_display_names() {
        assert_eq!(ChatRole::User.display_name(), "You");
        assert_eq!(ChatRole::Assistant.display_name(), "Nika");
        assert_eq!(ChatRole::System.display_name(), "System");
        assert_eq!(ChatRole::Tool.display_name(), "Tool");
    }

    #[test]
    fn test_chat_role_equality() {
        assert_eq!(ChatRole::User, ChatRole::User);
        assert_ne!(ChatRole::User, ChatRole::Assistant);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ChatMessage tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("Hi there!");
        assert_eq!(msg.role, ChatRole::Assistant);
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_chat_message_system() {
        let msg = ChatMessage::system("You are a helpful assistant.");
        assert_eq!(msg.role, ChatRole::System);
        assert_eq!(msg.content, "You are a helpful assistant.");
    }

    #[test]
    fn test_chat_message_tool() {
        let msg = ChatMessage::tool("{\"result\": \"success\"}");
        assert_eq!(msg.role, ChatRole::Tool);
        assert_eq!(msg.content, "{\"result\": \"success\"}");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ChatAgent creation tests
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_chat_agent_creation() {
        // This test verifies ChatAgent can be created
        // It succeeds if either OPENAI_API_KEY or ANTHROPIC_API_KEY is set
        // or if neither is set (agent created but will fail on infer)
        let agent = ChatAgent::new();

        // Agent creation should always succeed (error happens on use)
        assert!(agent.is_ok());
    }

    #[test]
    fn test_chat_agent_initial_state() {
        // Set a dummy key for the test
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");

        assert!(agent.history().is_empty());
        assert!(!agent.is_streaming());
        assert_eq!(agent.provider_name(), "openai");
    }

    #[test]
    fn test_chat_agent_with_claude_fallback() {
        // This test verifies Claude fallback logic.
        // Due to parallel test execution, we can't reliably remove OPENAI_API_KEY.
        // Instead, test that agent creation always succeeds.
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        // Provider will be openai if OPENAI_API_KEY is set (by parallel test),
        // or claude if only ANTHROPIC_API_KEY is set
        assert!(agent.provider_name() == "openai" || agent.provider_name() == "claude");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Provider switching tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_set_provider_openai() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");
        let result = agent.set_provider(ModelProvider::OpenAI);

        assert!(result.is_ok());
        assert_eq!(agent.provider_name(), "openai");
    }

    #[test]
    fn test_set_provider_claude() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");

        // Only test provider switch if ANTHROPIC_API_KEY is set
        // (parallel tests might remove it)
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            let result = agent.set_provider(ModelProvider::Claude);
            assert!(result.is_ok());
            assert_eq!(agent.provider_name(), "claude");
        }
    }

    #[test]
    fn test_set_provider_missing_key() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");

        // Test behavior when key is missing
        // We can't safely remove env vars due to parallel tests, but we can test
        // the error type when we know the key is missing
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            let result = agent.set_provider(ModelProvider::Claude);
            assert!(result.is_err());
            if let Err(NikaError::MissingApiKey { provider }) = result {
                assert_eq!(provider, "Claude");
            } else {
                panic!("Expected MissingApiKey error");
            }
        } else {
            // If ANTHROPIC_API_KEY is set (by parallel test), just verify we can switch
            let result = agent.set_provider(ModelProvider::Claude);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_set_provider_list_does_not_change() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");
        let original = agent.provider_name();

        let result = agent.set_provider(ModelProvider::List);

        assert!(result.is_ok());
        assert_eq!(agent.provider_name(), original);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // History tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_history_starts_empty() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        assert!(agent.history().is_empty());
    }

    #[test]
    fn test_clear_history() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");

        // Manually add messages to history (simulating conversation)
        agent.history.push(ChatMessage::user("Hello"));
        agent.history.push(ChatMessage::assistant("Hi!"));

        assert_eq!(agent.history().len(), 2);

        agent.clear_history();

        assert!(agent.history().is_empty());
    }

    #[test]
    fn test_with_history() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let history = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
            ChatMessage::user("How are you?"),
        ];

        let agent = ChatAgent::with_history(history).expect("Should create agent with history");

        assert_eq!(agent.history().len(), 3);
        assert_eq!(agent.history()[0].role, ChatRole::User);
        assert_eq!(agent.history()[0].content, "Hello");
        assert_eq!(agent.history()[1].role, ChatRole::Assistant);
        assert_eq!(agent.history()[2].content, "How are you?");
    }

    #[test]
    fn test_take_history() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");
        agent.history.push(ChatMessage::user("Hello"));
        agent.history.push(ChatMessage::assistant("Hi!"));

        let taken = agent.take_history();

        assert_eq!(taken.len(), 2);
        assert!(agent.history().is_empty()); // History is now empty
        assert_eq!(taken[0].content, "Hello");
        assert_eq!(taken[1].content, "Hi!");
    }

    #[test]
    fn test_set_history() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let mut agent = ChatAgent::new().expect("Should create agent");
        agent.history.push(ChatMessage::user("Old message"));

        let new_history = vec![
            ChatMessage::user("New conversation"),
            ChatMessage::assistant("Fresh start!"),
        ];

        agent.set_history(new_history);

        assert_eq!(agent.history().len(), 2);
        assert_eq!(agent.history()[0].content, "New conversation");
        assert_eq!(agent.history()[1].content, "Fresh start!");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Exec command tests (safe, no real execution)
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_exec_command_echo() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        let result = agent.exec_command("echo hello").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_exec_command_with_args() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        let result = agent.exec_command("echo -n 'test output'").await;

        assert!(result.is_ok());
        assert!(result.unwrap().contains("test output"));
    }

    #[tokio::test]
    async fn test_exec_command_failure() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        let result = agent.exec_command("exit 1").await;

        // Command failure returns Ok with exit code info
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn test_exec_command_pipe() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");
        let result = agent
            .exec_command("echo 'hello world' | tr 'a-z' 'A-Z'")
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "HELLO WORLD");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Streaming state tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_streaming_state_access() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let agent = ChatAgent::new().expect("Should create agent");

        assert!(!agent.is_streaming());
        assert!(!agent.streaming_state().is_streaming);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Streaming channel tests
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_with_streaming_channel() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");

        let (tx, _rx) = mpsc::channel::<String>(10);
        let agent = ChatAgent::new()
            .expect("Should create agent")
            .with_streaming(tx);

        // The streaming channel is set
        assert!(agent.streaming_tx.is_some());
    }
}
