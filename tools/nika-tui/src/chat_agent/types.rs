// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat message types and streaming state
//!
//! Core data types used throughout the chat agent module:
//! - `StreamingState` — Tracks real-time streaming progress for UI updates
//! - `ChatRole` — Message participant role (User, Assistant, System, Tool)
//! - `ChatMessage` — A single message in the conversation history

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
