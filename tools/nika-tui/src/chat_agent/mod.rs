// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
//! # Module Structure
//!
//! - `types` — `StreamingState`, `ChatRole`, `ChatMessage`
//! - `inference` — LLM inference with streaming support
//! - `commands` — Shell execution and HTTP fetch
//! - `mcp` — MCP tool invocation and agentic loops
//!
//! # Usage
//!
//! ```rust,no_run
//! use nika_tui::chat_agent::ChatAgent;
//! use nika_tui::command::ModelProvider;
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

mod commands;
mod inference;
mod mcp;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use types::{ChatMessage, ChatRole, StreamingState};

use crate::command::ModelProvider;
use nika_engine::error::NikaError;
use nika_engine::provider::rig::{RigProvider, StreamChunk};
use tokio::sync::mpsc;

// ═══════════════════════════════════════════════════════════════════════════
// CHAT AGENT
// ═══════════════════════════════════════════════════════════════════════════

/// Main chat agent handling LLM interactions
///
/// # Example
///
/// ```rust,no_run
/// use nika_tui::chat_agent::ChatAgent;
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
    pub(super) provider: RigProvider,
    /// Optional model override (uses provider default if None)
    pub(super) model_override: Option<String>,
    /// Conversation history
    pub(super) history: Vec<ChatMessage>,
    /// Optional streaming channel for real-time updates
    pub(super) streaming_tx: Option<mpsc::Sender<String>>,
    /// Optional streaming channel for token-by-token updates
    pub(super) stream_chunk_tx: Option<mpsc::Sender<StreamChunk>>,
    /// Current streaming state
    pub(super) streaming_state: StreamingState,
    /// HTTP client for fetch operations
    pub(super) http_client: reqwest::Client,
    /// Cumulative input tokens used
    pub total_input_tokens: u64,
    /// Cumulative output tokens used
    pub total_output_tokens: u64,
}

impl ChatAgent {
    /// Create a new ChatAgent with auto-detected provider
    ///
    /// Provider detection order:
    /// 1. ANTHROPIC_API_KEY → Claude
    /// 2. OPENAI_API_KEY → OpenAI
    /// 3. MISTRAL_API_KEY → Mistral
    /// 4. GROQ_API_KEY → Groq
    /// 5. DEEPSEEK_API_KEY → DeepSeek
    /// 6. GEMINI_API_KEY → Gemini
    pub fn new() -> Result<Self, NikaError> {
        // Use RigProvider::auto() for consistent detection
        // Return error if no API keys are set (instead of panicking on fallback)
        let provider = RigProvider::auto().ok_or_else(|| NikaError::MissingApiKey {
            provider: "any (ANTHROPIC_API_KEY, OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY, DEEPSEEK_API_KEY, or GEMINI_API_KEY)".to_string(),
        })?;

        Ok(Self {
            provider,
            model_override: None,
            history: Vec::new(),
            streaming_tx: None,
            stream_chunk_tx: None,
            streaming_state: StreamingState::new(),
            http_client: reqwest::Client::builder()
                .redirect(nika_engine::runtime::policy::ssrf_safe_redirect_policy(
                    vec![],
                    5,
                ))
                .build()
                .expect("HTTP client build with default TLS is infallible"),
            total_input_tokens: 0,
            total_output_tokens: 0,
        })
    }

    /// Create a new ChatAgent with specific provider and model overrides
    ///
    /// # Arguments
    ///
    /// * `provider` - Optional provider name (claude, openai, mistral, groq, deepseek, gemini)
    /// * `model` - Optional model name override
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use nika_tui::chat_agent::ChatAgent;
    ///
    /// let agent = ChatAgent::with_overrides(
    ///     Some("mistral"),
    ///     Some("mistral-large-latest")
    /// ).unwrap();
    /// ```
    pub fn with_overrides(provider: Option<&str>, model: Option<&str>) -> Result<Self, NikaError> {
        let mut agent = Self::new()?;

        // Apply provider override
        if let Some(p) = provider {
            agent.provider = RigProvider::from_name(p)?;
        }

        // Apply model override
        if let Some(m) = model {
            agent.model_override = Some(m.to_string());
        }

        Ok(agent)
    }

    /// Set streaming channel for real-time updates
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
    /// use nika_tui::chat_agent::ChatAgent;
    /// use nika_tui::command::ModelProvider;
    ///
    /// let mut agent = ChatAgent::new().unwrap();
    /// agent.set_provider(ModelProvider::Claude).unwrap();
    /// ```
    pub fn set_provider(&mut self, provider: ModelProvider) -> Result<(), NikaError> {
        if matches!(provider, ModelProvider::List) {
            return Ok(());
        }
        self.provider = RigProvider::from_name(provider.command_name())?;
        Ok(())
    }

    /// Get the current provider name
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Get the current model name (override or provider default)
    pub fn model_name(&self) -> &str {
        self.model_override
            .as_deref()
            .unwrap_or_else(|| self.provider.default_model())
    }

    /// Get total tokens used (input + output) for status bar display
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
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
    /// use nika_tui::chat_agent::{ChatAgent, ChatMessage};
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
