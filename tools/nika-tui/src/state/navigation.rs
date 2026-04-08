// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Navigation and interaction methods for TuiState
//!
//! Contains panel focus, MCP call navigation, filter/search,
//! status messages, clipboard copy, and agent icon helpers.

use super::types::{McpCall, MonitorPanel};
use super::TuiState;

use crate::theme::TaskStatus;

impl TuiState {
    // ═══════════════════════════════════════════
    // STATUS MESSAGE HELPERS
    // ═══════════════════════════════════════════

    /// Show an info status message
    pub fn status_info(&mut self, message: impl Into<String>) {
        self.status_messages.info(message);
    }

    /// Show a success status message
    pub fn status_success(&mut self, message: impl Into<String>) {
        self.status_messages.success(message);
    }

    /// Show a warning status message
    pub fn status_warning(&mut self, message: impl Into<String>) {
        self.status_messages.warning(message);
    }

    /// Show an error status message
    pub fn status_error(&mut self, message: impl Into<String>) {
        self.status_messages.error(message);
    }

    // ═══════════════════════════════════════════
    // AGENT ICON HELPERS
    // ═══════════════════════════════════════════

    /// Check if a task is a spawned subagent
    ///
    /// Returns true if the task_id appears as a child in spawned_agents.
    pub fn is_subagent(&self, task_id: &str) -> bool {
        self.agent
            .spawned_agents
            .iter()
            .any(|s| s.child_task_id == task_id)
    }

    /// Get the appropriate agent icon for a task
    ///
    /// Returns different icons for subagents vs parent agents.
    pub fn agent_icon(&self, task_id: &str) -> &'static str {
        if self.is_subagent(task_id) {
            ">" // Spawned subagent
        } else {
            ">>" // Parent agent
        }
    }

    // ═══════════════════════════════════════════
    // PANEL FOCUS
    // ═══════════════════════════════════════════

    /// Focus next panel
    pub fn focus_next(&mut self) {
        self.ui.focus = self.ui.focus.next();
    }

    /// Focus previous panel
    pub fn focus_prev(&mut self) {
        self.ui.focus = self.ui.focus.prev();
    }

    /// Focus specific panel by number (1-indexed)
    pub fn focus_panel(&mut self, num: u8) {
        self.ui.focus = match num {
            1 => MonitorPanel::Progress,
            2 => MonitorPanel::Dag,
            3 => MonitorPanel::NovaNet,
            4 => MonitorPanel::Agent,
            _ => self.ui.focus,
        };
    }

    /// Cycle tab in the currently focused panel
    pub fn cycle_tab(&mut self) {
        match self.ui.focus {
            MonitorPanel::Progress => self.ui.mission_tab = self.ui.mission_tab.next(),
            MonitorPanel::Dag => self.ui.dag_tab = self.ui.dag_tab.next(),
            MonitorPanel::NovaNet => self.ui.novanet_tab = self.ui.novanet_tab.next(),
            MonitorPanel::Agent => self.ui.reasoning_tab = self.ui.reasoning_tab.next(),
        }
    }

    // ═══════════════════════════════════════════
    // MCP NAVIGATION (TIER 1.3)
    // ═══════════════════════════════════════════

    /// Select previous MCP call
    pub fn select_prev_mcp(&mut self) {
        if self.mcp.calls.is_empty() {
            return;
        }

        self.mcp.selected_idx = match self.mcp.selected_idx {
            None => Some(self.mcp.calls.len().saturating_sub(1)), // Start from last
            Some(0) => Some(0),                                   // Stay at first
            Some(idx) => Some(idx - 1),
        };
    }

    /// Select next MCP call
    pub fn select_next_mcp(&mut self) {
        if self.mcp.calls.is_empty() {
            return;
        }

        let max_idx = self.mcp.calls.len().saturating_sub(1);
        self.mcp.selected_idx = match self.mcp.selected_idx {
            None => Some(0),                              // Start from first
            Some(idx) if idx >= max_idx => Some(max_idx), // Stay at last
            Some(idx) => Some(idx + 1),
        };
    }

    /// Select MCP call by index (for direct access)
    pub fn select_mcp(&mut self, idx: usize) {
        if idx < self.mcp.calls.len() {
            self.mcp.selected_idx = Some(idx);
        }
    }

    /// Get currently selected MCP call
    pub fn get_selected_mcp(&self) -> Option<&McpCall> {
        self.mcp
            .selected_idx
            .and_then(|idx| self.mcp.calls.get(idx))
    }

    /// Select first MCP call (g key - vim go to top)
    pub fn select_first_mcp(&mut self) {
        if !self.mcp.calls.is_empty() {
            self.mcp.selected_idx = Some(0);
        }
    }

    /// Select last MCP call (G key - vim go to bottom)
    pub fn select_last_mcp(&mut self) {
        if !self.mcp.calls.is_empty() {
            self.mcp.selected_idx = Some(self.mcp.calls.len().saturating_sub(1));
        }
    }

    // ═══════════════════════════════════════════
    // FILTER METHODS (TIER 1.5)
    // ═══════════════════════════════════════════

    /// Add character to filter query
    pub fn filter_push(&mut self, c: char) {
        self.filter_query.insert(self.filter_cursor, c);
        self.filter_cursor += c.len_utf8();
    }

    /// Remove character before cursor (backspace)
    pub fn filter_backspace(&mut self) {
        if self.filter_cursor > 0 {
            // Find the previous char boundary
            let prev = self.filter_query[..self.filter_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.filter_query.remove(prev);
            self.filter_cursor = prev;
        }
    }

    /// Remove character at cursor (delete)
    pub fn filter_delete(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            self.filter_query.remove(self.filter_cursor);
        }
    }

    /// Move cursor left
    pub fn filter_cursor_left(&mut self) {
        if self.filter_cursor > 0 {
            // Find the previous char boundary
            self.filter_cursor = self.filter_query[..self.filter_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right
    pub fn filter_cursor_right(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            // Advance past the current char
            self.filter_cursor = self.filter_query[self.filter_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.filter_cursor + i)
                .unwrap_or(self.filter_query.len());
        }
    }

    /// Clear filter query
    pub fn filter_clear(&mut self) {
        self.filter_query.clear();
        self.filter_cursor = 0;
    }

    /// Check if filter is active
    pub fn has_filter(&self) -> bool {
        !self.filter_query.is_empty()
    }

    /// Get filtered task IDs
    pub fn filtered_task_ids(&self) -> Vec<&String> {
        if self.filter_query.is_empty() {
            return self.task_order.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.task_order
            .iter()
            .filter(|id| {
                // Match task ID
                if id.to_lowercase().contains(&query) {
                    return true;
                }
                // Match task type
                if let Some(task) = self.tasks.get(*id) {
                    if let Some(task_type) = &task.task_type {
                        if task_type.to_lowercase().contains(&query) {
                            return true;
                        }
                    }
                }
                false
            })
            .collect()
    }

    /// Get filtered MCP calls
    pub fn filtered_mcp_calls(&self) -> Vec<&McpCall> {
        if self.filter_query.is_empty() {
            return self.mcp.calls.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.mcp
            .calls
            .iter()
            .filter(|call| {
                // Match server name
                if call.server.to_lowercase().contains(&query) {
                    return true;
                }
                // Match tool name
                if let Some(tool) = &call.tool {
                    if tool.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                // Match resource URI
                if let Some(resource) = &call.resource {
                    if resource.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                false
            })
            .collect()
    }

    // ═══════════════════════════════════════════
    // CLIPBOARD / COPY (TIER 2.1)
    // ═══════════════════════════════════════════

    /// Get content suitable for clipboard copy based on focused panel and current tab
    ///
    /// Returns the most relevant content for the current view:
    /// - Progress panel: Final output JSON or current task output
    /// - DAG panel: YAML content or task list
    /// - NovaNet panel: Selected MCP call (params + response)
    /// - Agent panel: Agent turns or thinking content
    pub fn get_copyable_content(&self) -> Option<String> {
        match self.ui.focus {
            MonitorPanel::Progress => {
                // Priority: final output > current task output > metrics summary
                if let Some(ref output) = self.workflow.final_output {
                    Some(serde_json::to_string_pretty(output.as_ref()).unwrap_or_default())
                } else if let Some(ref task_id) = self.current_task {
                    self.tasks.get(task_id).and_then(|task| {
                        task.output
                            .as_ref()
                            .map(|o| serde_json::to_string_pretty(o.as_ref()).unwrap_or_default())
                    })
                } else {
                    // Return metrics summary
                    Some(format!(
                        "Workflow: {}\nTasks: {}/{}\nTokens: {}\nMCP calls: {}",
                        self.workflow.path,
                        self.workflow.tasks_completed,
                        self.workflow.task_count,
                        self.metrics.total_tokens,
                        self.mcp.calls.len()
                    ))
                }
            }
            MonitorPanel::Dag => {
                // Return task list with statuses
                let mut lines = vec!["# DAG Tasks".to_string()];
                for task_id in &self.task_order {
                    if let Some(task) = self.tasks.get(task_id) {
                        let status = match task.status {
                            TaskStatus::Queued => "[.]",
                            TaskStatus::Pending => "[ ]",
                            TaskStatus::Running => "[~]",
                            TaskStatus::Success => "[x]",
                            TaskStatus::Failed => "[!]",
                            TaskStatus::Paused => "[-]",
                            TaskStatus::Skipped => "[/]",
                        };
                        let deps = if task.dependencies.is_empty() {
                            String::new()
                        } else {
                            format!(" -> {}", task.dependencies.join(", "))
                        };
                        lines.push(format!("{} {}{}", status, task_id, deps));
                    }
                }
                Some(lines.join("\n"))
            }
            MonitorPanel::NovaNet => {
                // Return selected MCP call or all calls
                if let Some(idx) = self.mcp.selected_idx {
                    self.mcp.calls.get(idx).map(|call| {
                        let mut content = format!(
                            "# MCP Call #{}: {}\n\n",
                            call.seq + 1,
                            call.tool.as_deref().unwrap_or("resource")
                        );
                        content.push_str("## Request\n");
                        if let Some(ref params) = call.params {
                            content.push_str(
                                &serde_json::to_string_pretty(params).unwrap_or_default(),
                            );
                        }
                        content.push_str("\n\n## Response\n");
                        if let Some(ref response) = call.response {
                            content.push_str(
                                &serde_json::to_string_pretty(response).unwrap_or_default(),
                            );
                        } else if !call.completed {
                            content.push_str("(pending...)");
                        }
                        content
                    })
                } else if !self.mcp.calls.is_empty() {
                    // Return summary of all MCP calls
                    let mut lines = vec!["# MCP Calls".to_string()];
                    for call in &self.mcp.calls {
                        let status = if call.completed { "[x]" } else { "[~]" };
                        let tool = call.tool.as_deref().unwrap_or("resource");
                        let duration = call
                            .duration_ms
                            .map(|d| format!(" {}ms", d))
                            .unwrap_or_default();
                        lines.push(format!(
                            "{} #{} {}:{}{}",
                            status,
                            call.seq + 1,
                            call.server,
                            tool,
                            duration
                        ));
                    }
                    Some(lines.join("\n"))
                } else {
                    None
                }
            }
            MonitorPanel::Agent => {
                // Return agent turns or thinking content
                if self.agent.turns.is_empty() {
                    return None;
                }

                let mut content = String::from("# Agent Turns\n\n");
                for turn in &self.agent.turns {
                    content.push_str(&format!("## Turn {}\n", turn.index + 1));
                    if let Some(ref thinking) = turn.thinking {
                        content.push_str("### Thinking\n");
                        content.push_str(thinking);
                        content.push_str("\n\n");
                    }
                    if let Some(ref response) = turn.response_text {
                        content.push_str("### Response\n");
                        content.push_str(response);
                        content.push_str("\n\n");
                    }
                    if !turn.tool_calls.is_empty() {
                        content.push_str("### Tool Calls\n");
                        for tool in &turn.tool_calls {
                            content.push_str(&format!("- {}\n", tool));
                        }
                        content.push('\n');
                    }
                }
                Some(content)
            }
        }
    }
}
