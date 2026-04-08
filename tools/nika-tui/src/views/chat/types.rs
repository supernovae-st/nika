// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat View Types
//!
//! Core types for chat functionality.
//! This module contains message types, serialization, and mode enums.

use std::time::Instant;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use std::path::Path;

use crate::widgets::{InferStreamData, McpCallData, Provider, TaskBox};

// ═══════════════════════════════════════════════════════════════════════════════
// Message Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Nika,
    System,
    Tool,
}

/// A chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Stable unique ID for tracking (survives message list mutations)
    pub id: u64,
    pub role: MessageRole,
    pub content: String,
    /// Display timestamp (HH:MM) for better conversation tracking
    pub timestamp: DateTime<Local>,
    /// Instant for elapsed time calculations
    pub created_at: Instant,
    /// Optional inline execution result
    pub execution: Option<ExecutionResult>,
    /// Optional agent thinking/reasoning content
    /// Displayed inline when present (collapsible in UI)
    pub thinking: Option<String>,
}

/// Inline execution result in chat
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub tasks_completed: usize,
    pub tasks_total: usize,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Serializable Types (for session persistence)
// ═══════════════════════════════════════════════════════════════════════════════

/// Serializable message role for persistence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SerializableRole {
    User,
    Nika,
    System,
    Tool,
}

impl From<&MessageRole> for SerializableRole {
    fn from(role: &MessageRole) -> Self {
        match role {
            MessageRole::User => SerializableRole::User,
            MessageRole::Nika => SerializableRole::Nika,
            MessageRole::System => SerializableRole::System,
            MessageRole::Tool => SerializableRole::Tool,
        }
    }
}

impl From<SerializableRole> for MessageRole {
    fn from(role: SerializableRole) -> Self {
        match role {
            SerializableRole::User => MessageRole::User,
            SerializableRole::Nika => MessageRole::Nika,
            SerializableRole::System => MessageRole::System,
            SerializableRole::Tool => MessageRole::Tool,
        }
    }
}

/// Serializable message for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableMessage {
    pub role: SerializableRole,
    pub content: String,
    pub thinking: Option<String>,
}

impl From<&ChatMessage> for SerializableMessage {
    fn from(msg: &ChatMessage) -> Self {
        Self {
            role: (&msg.role).into(),
            content: msg.content.clone(),
            thinking: msg.thinking.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Inline Content Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Inline content that can appear in a message
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // TaskBox (352 bytes) is the common case; boxing would complicate pattern matching
pub enum InlineContent {
    /// MCP tool call with params and result
    McpCall(McpCallData),
    /// Streaming inference with token counter
    InferStream(InferStreamData),
    /// Task box for any verb
    Task(TaskBox),
}

// ═══════════════════════════════════════════════════════════════════════════════
// Active Provider (consolidated provider state)
// ═══════════════════════════════════════════════════════════════════════════════

/// Consolidated provider state — replaces 4 separate fields
///
/// Before: current_model, cached_provider, provider_name, current_provider_id
/// After: single struct, always in sync
#[derive(Debug, Clone)]
pub struct ActiveProvider {
    /// Provider ID for inference ("claude", "openai", "mistral", etc.)
    pub id: String,
    /// Display name ("Anthropic Claude", "OpenAI", etc.)
    pub name: String,
    /// Model name for inference ("claude-sonnet-4-6", "gpt-4o", etc.)
    pub model: String,
    /// Cached Provider enum (derived from model name)
    pub kind: Provider,
}

impl ActiveProvider {
    /// Create from provider ID, name, and model
    pub fn new(id: impl Into<String>, name: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let kind = Provider::from_model_name(&model);
        Self {
            id: id.into(),
            name: name.into(),
            model,
            kind,
        }
    }

    /// Update model (also refreshes cached kind)
    pub fn set_model(&mut self, model: impl Into<String>) {
        let model = model.into();
        self.kind = Provider::from_model_name(&model);
        self.model = model;
    }

    /// Check if no provider is configured
    pub fn is_none(&self) -> bool {
        self.id == "none"
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chat Mode
// ═══════════════════════════════════════════════════════════════════════════════

/// Inference mode for conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatMode {
    /// Simple inference mode (single completion, no tools)
    #[default]
    Infer,
    /// Agent mode with tool access (multi-turn, MCP tools)
    Agent,
}

impl ChatMode {
    /// Get display label for the mode
    pub fn label(&self) -> &'static str {
        match self {
            ChatMode::Infer => "Infer",
            ChatMode::Agent => "Agent",
        }
    }

    /// Get icon for the mode
    pub fn icon(&self) -> &'static str {
        match self {
            ChatMode::Infer => "⚡", // LLM generation
            ChatMode::Agent => "🐔", // Parent agent icon
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Serializable Chat Session (HIGH 8 - Persistent Sessions)
// ═══════════════════════════════════════════════════════════════════════════════

/// Serializable chat session for save/load
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub version: String,
    pub created_at: String,
    pub model: String,
    pub messages: Vec<SerializableMessage>,
}

impl ChatSession {
    /// Create session from messages and model
    pub fn from_messages(messages: &[ChatMessage], model: &str) -> Self {
        Self {
            version: "0.5.2".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            model: model.to_string(),
            messages: messages.iter().map(SerializableMessage::from).collect(),
        }
    }

    /// Save session to file using atomic write (temp+rename) for data integrity
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        use nika_engine::util::atomic_write;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        atomic_write(path, json.as_bytes())
    }

    /// Load session from file
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        use std::fs;
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MCP Retry Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Stores info about a failed MCP call for retry
#[derive(Debug, Clone)]
pub struct McpRetryInfo {
    /// Tool name that was called
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Parameters passed to the call
    pub params: serde_json::Value,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chat Message Selection Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Position of a rendered line for hit testing during mouse selection
/// Used to map screen coordinates to message content
#[derive(Debug, Clone)]
pub struct ChatLinePosition {
    /// Index of the message this line belongs to
    pub message_index: usize,
    /// Which line within the message (0 = first)
    pub line_in_message: usize,
    /// Y coordinate on screen
    pub screen_y: u16,
    /// Starting X coordinate (after prefix)
    pub start_x: u16,
    /// The actual text content of this line
    pub text: String,
}

/// Text selection state for chat messages
/// Tracks selection across multiple messages
#[derive(Debug, Clone)]
pub struct ChatMessageSelection {
    /// Starting position of selection
    pub start: ChatSelectionPos,
    /// Ending position of selection (current drag position)
    pub end: ChatSelectionPos,
}

/// Position within the chat for message-based selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatSelectionPos {
    /// Message index in the messages vec
    pub message_index: usize,
    /// Character offset within the message content
    pub char_offset: usize,
}

impl ChatMessageSelection {
    /// Create a new selection starting at the given position
    pub fn new(start: ChatSelectionPos) -> Self {
        Self { start, end: start }
    }

    /// Get the normalized selection (start <= end)
    pub fn normalized(&self) -> (ChatSelectionPos, ChatSelectionPos) {
        if self.start.message_index < self.end.message_index
            || (self.start.message_index == self.end.message_index
                && self.start.char_offset <= self.end.char_offset)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: ChatSelectionPos) -> bool {
        let (start, end) = self.normalized();

        if pos.message_index < start.message_index || pos.message_index > end.message_index {
            return false;
        }

        if pos.message_index == start.message_index && pos.message_index == end.message_index {
            // Same message: check char range
            pos.char_offset >= start.char_offset && pos.char_offset < end.char_offset
        } else if pos.message_index == start.message_index {
            // First message: from start.char_offset to end of message
            pos.char_offset >= start.char_offset
        } else if pos.message_index == end.message_index {
            // Last message: from 0 to end.char_offset
            pos.char_offset < end.char_offset
        } else {
            // Middle messages: fully selected
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_conversion() {
        let role = MessageRole::User;
        let serializable: SerializableRole = (&role).into();
        assert_eq!(serializable, SerializableRole::User);

        let back: MessageRole = serializable.into();
        assert_eq!(back, MessageRole::User);
    }

    #[test]
    fn test_chat_mode_label() {
        assert_eq!(ChatMode::Infer.label(), "Infer");
        assert_eq!(ChatMode::Agent.label(), "Agent");
    }

    #[test]
    fn test_chat_mode_icon() {
        assert_eq!(ChatMode::Infer.icon(), "⚡");
        assert_eq!(ChatMode::Agent.icon(), "🐔");
    }
}
