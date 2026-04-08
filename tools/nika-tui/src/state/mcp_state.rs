// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP tracking state slice
//!
//! Extracted from TuiState to isolate MCP call tracking
//! and context assembly state.

use std::collections::VecDeque;

use super::types::{ContextAssembly, McpCall};

/// MCP-related state extracted from TuiState
///
/// Tracks MCP tool calls, sequence numbers, selection state,
/// and context assembly progress.
#[derive(Debug, Default)]
pub struct McpState {
    /// MCP call log (PERF: VecDeque for O(1) front eviction)
    pub calls: VecDeque<McpCall>,
    /// Next MCP call sequence number
    pub seq: usize,
    /// Selected MCP call index for Full JSON view
    pub selected_idx: Option<usize>,
    /// Current context assembly
    pub context_assembly: ContextAssembly,
}

impl McpState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Maximum number of MCP calls to retain
    const MAX_CALLS: usize = 200;

    /// Add an MCP call and return its sequence number
    pub fn add_call(&mut self, mut call: McpCall) -> usize {
        let seq = self.seq;
        call.seq = seq;
        self.seq += 1;
        if self.calls.len() >= Self::MAX_CALLS {
            self.calls.pop_front(); // O(1) vs Vec::remove(0) O(n)
            self.selected_idx = self.selected_idx.map(|idx| idx.saturating_sub(1));
        }
        self.calls.push_back(call);
        seq
    }

    /// Get the currently selected MCP call
    pub fn selected_call(&self) -> Option<&McpCall> {
        self.selected_idx.and_then(|idx| self.calls.get(idx))
    }

    /// Select next MCP call in the list
    pub fn select_next(&mut self) {
        if self.calls.is_empty() {
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) => (idx + 1).min(self.calls.len() - 1),
            None => 0,
        });
    }

    /// Select previous MCP call in the list
    pub fn select_prev(&mut self) {
        if self.calls.is_empty() {
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test McpCall with minimal boilerplate
    fn make_call(tool: &str, server: &str) -> McpCall {
        McpCall {
            call_id: String::new(),
            seq: 0,
            server: server.to_string(),
            tool: Some(tool.to_string()),
            resource: None,
            task_id: String::new(),
            completed: false,
            output_len: None,
            timestamp_ms: 0,
            params: None,
            response: None,
            is_error: false,
            duration_ms: None,
        }
    }

    #[test]
    fn test_mcp_state_default() {
        let mcp = McpState::new();
        assert!(mcp.calls.is_empty());
        assert_eq!(mcp.seq, 0);
        assert!(mcp.selected_idx.is_none());
    }

    #[test]
    fn test_mcp_state_add_call() {
        let mut mcp = McpState::new();
        let call = make_call("novanet_search", "novanet");
        let seq = mcp.add_call(call);
        assert_eq!(seq, 0);
        assert_eq!(mcp.calls.len(), 1);
        assert_eq!(mcp.seq, 1);
    }

    #[test]
    fn test_mcp_state_navigation() {
        let mut mcp = McpState::new();
        // Navigation on empty list is a no-op
        mcp.select_next();
        assert!(mcp.selected_idx.is_none());

        // Add some calls
        for i in 0..3 {
            mcp.add_call(make_call(&format!("tool_{}", i), "test"));
        }

        mcp.select_next();
        assert_eq!(mcp.selected_idx, Some(0));
        mcp.select_next();
        assert_eq!(mcp.selected_idx, Some(1));
        mcp.select_next();
        assert_eq!(mcp.selected_idx, Some(2));
        // Clamp at end
        mcp.select_next();
        assert_eq!(mcp.selected_idx, Some(2));

        mcp.select_prev();
        assert_eq!(mcp.selected_idx, Some(1));
        mcp.select_prev();
        assert_eq!(mcp.selected_idx, Some(0));
        // Clamp at start
        mcp.select_prev();
        assert_eq!(mcp.selected_idx, Some(0));
    }

    #[test]
    fn test_mcp_state_selected_call() {
        let mut mcp = McpState::new();
        assert!(mcp.selected_call().is_none());

        mcp.add_call(make_call("test_tool", "test"));
        mcp.selected_idx = Some(0);
        assert_eq!(
            mcp.selected_call().unwrap().tool.as_deref(),
            Some("test_tool")
        );
    }
}
