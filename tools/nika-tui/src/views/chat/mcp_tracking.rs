// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Call Tracking for Chat View
//!
//! Contains methods for tracking MCP tool calls and their lifecycle.

use std::time::Instant;

use chrono::Local;

use super::{
    ActivityItem, ChatMessage, ChatView, InlineContent, McpCallData, McpCallStatus, McpRetryInfo,
    McpStatus, MessageRole,
};
use nika_engine::runtime::chat_workflow::Role as WorkflowRole;

// ═══════════════════════════════════════════════════════════════════════════════
// MCP Call Tracking
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Add an MCP call to the inline content
    pub fn add_mcp_call(&mut self, tool: &str, server: &str, params: &str) {
        let data = McpCallData::new(tool, server).with_params(params);
        self.inline_content.push(InlineContent::McpCall(data));
        Self::enforce_cap(&mut self.inline_content, 100);

        // Add to activity stack as hot
        self.activity_items.push(ActivityItem::hot(
            format!("mcp-{}", self.inline_content.len()),
            "invoke",
        ));
        Self::enforce_cap(&mut self.activity_items, 50);

        // Update MCP server status to hot
        if let Some(server_info) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server)
        {
            server_info.status = McpStatus::Hot;
            server_info.last_call = Some(Instant::now());
        }

        // Auto-scroll when MCP call appears
        self.auto_scroll_to_bottom();
    }

    /// Complete an MCP call with result
    pub fn complete_mcp_call(&mut self, result: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            data.result = Some(result.to_string());
            data.status = McpCallStatus::Success;
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("invoke");
        // Auto-scroll when MCP result arrives
        self.auto_scroll_to_bottom();
    }

    /// Fail an MCP call with error
    ///
    /// Also saves the failed call for retry with Ctrl+R
    pub fn fail_mcp_call(&mut self, error: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            self.last_failed_mcp = Some(McpRetryInfo {
                tool: data.tool.clone(),
                server: data.server.clone(),
                params: serde_json::from_str(&data.params).unwrap_or(serde_json::Value::Null),
            });

            data.error = Some(error.to_string());
            data.status = McpCallStatus::Failed;
        }
        // Auto-scroll when MCP error appears
        self.auto_scroll_to_bottom();
    }

    /// Add a tool message
    pub fn add_tool_message(&mut self, content: String) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Tool,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        // Sync DAG when messages change
        self.maybe_sync_dag();

        // Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Tool);
    }
}
