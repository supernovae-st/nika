// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider and MCP connection event handlers
//!
//! ProviderCalled, ProviderResponded, McpInvoke, McpResponse,
//! McpConnected, McpError, McpRetry

use std::sync::Arc;

use nika_engine::event::FinishReason;

use super::{TuiState, MAX_HISTORY_ENTRIES};
use crate::state::notification::Notification;
use crate::state::types::McpCall;
use crate::theme::MissionPhase;

/// Default context window size for token threshold notifications
const CONTEXT_WINDOW_TOKENS: u64 = 100_000;

impl TuiState {
    pub(super) fn on_provider_called(
        &mut self,
        task_id: &str,
        provider: &str,
        model: &str,
        prompt_len: usize,
    ) {
        // Update task's provider info
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.provider = Some(provider.to_string());
            task.model = Some(model.to_string());
            task.prompt_len = Some(prompt_len);
        }

        // Update metrics
        self.metrics.provider_calls += 1;
        self.metrics.last_model = Some(model.to_string());

        // TIER 4.1: Mark progress dirty (for provider display)
        self.dirty.progress = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_provider_responded(
        &mut self,
        task_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cost_usd: f64,
        ttft_ms: &Option<u64>,
        finish_reason: &FinishReason,
        timestamp_ms: u64,
    ) {
        // Capture finish_reason on the task
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.finish_reason = Some(finish_reason.to_string());
        }

        self.metrics.input_tokens += input_tokens;
        self.metrics.output_tokens += output_tokens;
        self.metrics.cache_read_tokens += cache_read_tokens;
        self.metrics.total_tokens += input_tokens + output_tokens;
        self.metrics.cost_usd += cost_usd;
        if self.metrics.token_history.len() >= MAX_HISTORY_ENTRIES {
            self.metrics.token_history.pop_front();
        }
        self.metrics
            .token_history
            .push_back(input_tokens + output_tokens);
        if let Some(ttft) = ttft_ms {
            if self.metrics.latency_history.len() >= MAX_HISTORY_ENTRIES {
                self.metrics.latency_history.pop_front();
            }
            self.metrics.latency_history.push_back(*ttft);
            // Calculate tokens/sec from TTFT and push to velocity tracker
            // TTFT in ms, output_tokens is total - estimate avg rate
            let ttft_secs = (*ttft as f32).max(1.0) / 1000.0;
            let velocity = output_tokens as f32 / ttft_secs;
            // TokenVelocity is a ring buffer -- push() handles eviction
            self.metrics.token_velocity.push(velocity);
        } else if output_tokens > 0 {
            // Fallback: assume ~1 second if no TTFT, just track relative activity
            self.metrics.token_velocity.push(output_tokens as f32);
        }

        // TIER 3.4: Token usage progression with cosmic pirate emojis (fire-once per threshold)
        let pct = (self.metrics.total_tokens as f64 / CONTEXT_WINDOW_TOKENS as f64) * 100.0;

        // Independent ifs: all crossed thresholds fire when tokens jump multiple levels
        if pct > 95.0 && !self.metrics.notified_95pct {
            self.metrics.notified_95pct = true;
            self.add_notification(Notification::alert(
                format!(
                    "ABANDON SHIP! {:.0}% fuel ({}/{}k)",
                    pct,
                    self.metrics.total_tokens,
                    CONTEXT_WINDOW_TOKENS / 1000
                ),
                timestamp_ms,
            ));
        }
        if pct > 85.0 && !self.metrics.notified_85pct {
            self.metrics.notified_85pct = true;
            self.add_notification(Notification::alert(
                format!(
                    "Danger zone! {:.0}% fuel ({}/{}k)",
                    pct,
                    self.metrics.total_tokens,
                    CONTEXT_WINDOW_TOKENS / 1000
                ),
                timestamp_ms,
            ));
        }
        if pct > 70.0 && !self.metrics.notified_70pct {
            self.metrics.notified_70pct = true;
            self.add_notification(Notification::warning(
                format!(
                    "Getting spicy! {:.0}% fuel ({}/{}k)",
                    pct,
                    self.metrics.total_tokens,
                    CONTEXT_WINDOW_TOKENS / 1000
                ),
                timestamp_ms,
            ));
        }
        if pct > 50.0 && !self.metrics.notified_50pct {
            self.metrics.notified_50pct = true;
            self.add_notification(Notification::info(
                format!(
                    "Heating up... {:.0}% fuel ({}/{}k)",
                    pct,
                    self.metrics.total_tokens,
                    CONTEXT_WINDOW_TOKENS / 1000
                ),
                timestamp_ms,
            ));
        }
        // TIER 4.1: Mark progress dirty (for metrics display)
        self.dirty.progress = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_mcp_invoke(
        &mut self,
        task_id: &str,
        mcp_server: &str,
        tool: &Option<String>,
        resource: &Option<String>,
        call_id: &str,
        params: &Option<Arc<serde_json::Value>>,
        timestamp_ms: u64,
    ) {
        let call = McpCall {
            call_id: call_id.to_string(),
            seq: self.mcp.seq,
            server: mcp_server.to_string(),
            tool: tool.clone(),
            resource: resource.clone(),
            task_id: task_id.to_string(),
            completed: false,
            output_len: None,
            timestamp_ms,
            params: params.as_deref().cloned(),
            response: None,
            is_error: false,
            duration_ms: None,
        };
        self.mcp.add_call(call);

        // Update phase — only from active execution phases (preserve Pause/Abort/MissionSuccess)
        if matches!(
            self.workflow.phase,
            MissionPhase::Countdown | MissionPhase::Launch | MissionPhase::Orbital
        ) {
            self.workflow.phase = MissionPhase::Rendezvous;
        }

        // Track in metrics
        if let Some(ref tool_name) = tool {
            let entry = self.metrics.mcp_calls.entry(tool_name.clone()).or_insert(0);
            *entry += 1;
        }
        // TIER 4.1: Mark novanet panel dirty
        self.dirty.novanet = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_mcp_response(
        &mut self,
        output_len: usize,
        call_id: &str,
        duration_ms: u64,
        cached: bool,
        is_error: bool,
        response: &Option<Arc<serde_json::Value>>,
        timestamp_ms: u64,
    ) {
        // Track MCP cache hit/miss
        if cached {
            self.metrics.mcp_cache_hits += 1;
        } else {
            self.metrics.mcp_cache_misses += 1;
        }

        // Find and update the matching call by call_id (single pass)
        let tool_name = if let Some(call) = self.mcp.calls.iter_mut().find(|c| c.call_id == call_id)
        {
            let name = call.tool.clone();
            call.completed = true;
            call.output_len = Some(output_len);
            call.response = response.as_deref().cloned();
            call.is_error = is_error;
            call.duration_ms = Some(duration_ms);
            name
        } else {
            None
        };

        // Track MCP latency for sparkline
        if self.metrics.latency_history.len() >= MAX_HISTORY_ENTRIES {
            self.metrics.latency_history.pop_front();
        }
        self.metrics.latency_history.push_back(duration_ms);

        // Show MCP errors in status bar for user visibility
        if is_error {
            let tool_display = tool_name.as_deref().unwrap_or("MCP tool");
            let error_msg = response
                .as_ref()
                .map(|r| {
                    let s = r.to_string();
                    // Truncate long error messages for status bar (UTF-8 safe)
                    if s.len() > 80 {
                        let mut end = 77;
                        while end > 0 && !s.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &s[..end])
                    } else {
                        s
                    }
                })
                .unwrap_or_else(|| "unknown error".to_string());
            self.status_messages
                .error(format!("MCP '{}': {}", tool_display, error_msg));
        }

        // TIER 3.4: Notify on slow MCP responses (> 5s)
        if duration_ms > 5_000 {
            let duration_secs = duration_ms as f64 / 1000.0;
            let tool_display = tool_name.as_deref().unwrap_or("resource");
            self.add_notification(Notification::warning(
                format!(
                    "Tentacles reaching... '{}' at {:.1}s",
                    tool_display, duration_secs
                ),
                timestamp_ms,
            ));
        }

        // Return to orbital phase only from Rendezvous (preserve Pause/Abort/MissionSuccess)
        if self.workflow.phase == MissionPhase::Rendezvous {
            self.workflow.phase = MissionPhase::Orbital;
        }
        // TIER 4.1: Mark novanet panel dirty
        self.dirty.novanet = true;
        // TIER 4.4: Invalidate MCP call cache on response
        self.json_cache.invalidate(&format!("mcp:{}", call_id));
    }

    pub(super) fn on_mcp_connected(&mut self, server_name: &str, timestamp_ms: u64) {
        self.add_notification(Notification::success(
            format!("MCP server '{}' connected", server_name),
            timestamp_ms,
        ));
        self.dirty.status = true;
    }

    pub(super) fn on_mcp_error(&mut self, server_name: &str, error: &str, timestamp_ms: u64) {
        self.add_notification(Notification::error(
            format!("MCP '{}' error: {}", server_name, error),
            timestamp_ms,
        ));
        // Show in status bar for immediate user feedback
        self.status_messages
            .error(format!("MCP: {} disconnected", server_name));
        self.dirty.status = true;
    }

    pub(super) fn on_mcp_retry(
        &mut self,
        server_name: &str,
        operation: &str,
        attempt: u32,
        max_attempts: u32,
        error: &str,
        timestamp_ms: u64,
    ) {
        self.add_notification(Notification::warning(
            format!(
                "MCP '{}' retry {}/{} for '{}': {}",
                server_name, attempt, max_attempts, operation, error
            ),
            timestamp_ms,
        ));
        // Show retrying status in status bar
        self.status_messages.warning(format!(
            "MCP: {} retrying ({}/{})",
            server_name, attempt, max_attempts
        ));
        self.dirty.status = true;
    }
}
