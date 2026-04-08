// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Session Management for Chat View
//!
//! Contains session persistence, model management, and MCP server tracking.

use std::path::Path;
use std::time::Instant;

use chrono::Local;

use super::{ChatMessage, ChatSession, ChatView, McpServerInfo};
use crate::widgets::Provider;

// ═══════════════════════════════════════════════════════════════════════════════
// Session Persistence (HIGH 8)
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Save current session to file
    pub fn save_session(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let session = ChatSession::from_messages(&self.messages, &self.provider.model);
        session.save(path)
    }

    /// Load session from file
    pub fn load_session(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let session = ChatSession::load(path)?;

        // Clear current messages and reset ID counter
        self.messages.clear();
        self.thinking_collapsed.clear(); // Clear thinking state for new session
        self.message_id_counter = 0; // Reset counter for fresh session

        for msg in session.messages {
            let id = self.next_message_id();
            self.messages.push(ChatMessage {
                id,
                role: msg.role.into(),
                content: msg.content,
                timestamp: Local::now(), // Use current time since original is lost
                created_at: Instant::now(),
                execution: None,
                thinking: msg.thinking,
            });
        }

        // Update model if specified in session
        if !session.model.is_empty() {
            self.provider.set_model(&session.model);
        }

        Ok(())
    }

    /// Get default session file path
    pub fn default_session_path() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join(".nika")
            .join("chat_session.json")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Model Management
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Set the current model name
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.provider.set_model(model);
    }

    /// Get the cached provider (PERF: computed once when model changes, not every frame)
    pub fn provider(&self) -> Provider {
        self.provider.kind
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MCP Server Management
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Set MCP servers from workflow configuration
    ///
    /// Replaces the default "novanet" with actual configured servers.
    pub fn set_mcp_servers(&mut self, server_names: impl IntoIterator<Item = impl Into<String>>) {
        self.session_context.mcp_servers.clear();
        for name in server_names {
            self.session_context
                .mcp_servers
                .push(McpServerInfo::new(name.into()));
        }
    }

    /// Mark an MCP server as connected
    pub fn mark_mcp_server_connected(&mut self, server_name: &str) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.mark_connected();
        }
    }

    /// Mark an MCP server as errored
    pub fn mark_mcp_server_error(&mut self, server_name: &str) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.mark_error();
        }
    }

    /// Update MCP server status with ping result
    ///
    /// Updates the server's connection status and recorded latency.
    pub fn update_mcp_server_status(
        &mut self,
        server_name: &str,
        connected: bool,
        latency_ms: u64,
    ) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.update_from_ping(connected, latency_ms);
        }
    }
}
