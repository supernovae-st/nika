// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Activity Tracking for Chat View
//!
//! Contains activity stack management, token/metrics updates, and command palette.

use std::time::Duration;

use super::{ActivityItem, ActivityTemp, ChatView, CurrentVerb, TurnMetrics};

// ═══════════════════════════════════════════════════════════════════════════════
// Activity tracking for /exec, /fetch, /agent commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Maximum activity items to retain (shared with task_boxes.rs)
const MAX_ACTIVITY_ITEMS: usize = 50;

impl ChatView {
    /// Add exec activity to the stack
    pub fn add_exec_activity(&mut self, command: &str) {
        let activity_id = format!("exec-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "exec"));
        Self::enforce_cap(&mut self.activity_items, MAX_ACTIVITY_ITEMS);
        tracing::debug!(command = %command, "Added exec activity");
    }

    /// Complete exec activity
    pub fn complete_exec_activity(&mut self) {
        self.transition_activity_to_warm("exec");
    }

    /// Add fetch activity to the stack
    pub fn add_fetch_activity(&mut self, url: &str, method: &str) {
        let activity_id = format!("fetch-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "fetch"));
        Self::enforce_cap(&mut self.activity_items, MAX_ACTIVITY_ITEMS);
        tracing::debug!(url = %url, method = %method, "Added fetch activity");
    }

    /// Complete fetch activity
    pub fn complete_fetch_activity(&mut self) {
        self.transition_activity_to_warm("fetch");
    }

    /// Add agent activity to the stack
    pub fn add_agent_activity(&mut self, goal: &str) {
        let activity_id = format!("agent-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "agent"));
        Self::enforce_cap(&mut self.activity_items, MAX_ACTIVITY_ITEMS);
        tracing::debug!(goal = %goal, "Added agent activity");
    }

    /// Complete agent activity
    pub fn complete_agent_activity(&mut self) {
        self.transition_activity_to_warm("agent");
    }

    /// Update session token usage
    pub fn update_tokens(&mut self, tokens_used: u64, cost: f64) {
        self.session_context.tokens_used = tokens_used;
        self.session_context.total_cost = cost;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Real-time Streaming Updates
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Update current verb during execution (for Mission Control display)
    pub fn set_current_verb(&mut self, verb: CurrentVerb) {
        self.current_verb = verb;
    }

    /// Update turn metrics during streaming (real-time token counts)
    pub fn update_turn_metrics(&mut self, input_tokens: u64, output_tokens: u64, cost_usd: f64) {
        // Compute deltas before updating turn metrics
        let input_delta = input_tokens.saturating_sub(self.turn_metrics.input_tokens);
        let output_delta = output_tokens.saturating_sub(self.turn_metrics.output_tokens);
        let cost_delta = cost_usd.max(0.0) - self.turn_metrics.cost_usd.max(0.0);

        // Update turn metrics
        self.turn_metrics.input_tokens = input_tokens;
        self.turn_metrics.output_tokens = output_tokens;
        self.turn_metrics.cost_usd = cost_usd;

        // Update session metrics with deltas
        self.session_metrics.input_tokens += input_delta;
        self.session_metrics.output_tokens += output_delta;
        self.session_metrics.cost_usd += cost_delta;
    }

    /// Increment turn metrics during streaming (delta updates)
    pub fn increment_output_tokens(&mut self, delta_tokens: u64) {
        self.turn_metrics.output_tokens += delta_tokens;
        self.session_metrics.output_tokens += delta_tokens;
    }

    /// Reset turn metrics for a new turn
    pub fn reset_turn_metrics(&mut self) {
        self.turn_metrics = TurnMetrics::default();
        self.current_verb = CurrentVerb::None;
    }

    /// Complete a turn (session metrics already updated via update_turn_metrics)
    pub fn complete_turn(&mut self) {
        // Session metrics are already updated incrementally via update_turn_metrics()
        // Just reset turn metrics for the next turn
        self.reset_turn_metrics();
        // Clear inline boxes and activities on turn completion
        self.inline_content.clear();
        self.activity_items.clear();
    }

    /// Toggle command palette visibility
    pub fn toggle_command_palette(&mut self) {
        self.command_palette.toggle();
    }

    /// Toggle Provider Modal visibility (⌘P)
    ///
    /// Returns true if the modal was opened (verification needed).
    pub fn toggle_provider_modal(&mut self) -> bool {
        let was_visible = self.provider_modal.visible;
        self.provider_modal.visible = !self.provider_modal.visible;
        // Return true if we just opened the modal (trigger verification)
        !was_visible && self.provider_modal.visible
    }

    /// Transition activity from hot to warm
    /// Use .rev() to find the LAST hot activity (most recent)
    /// Before: Only first concurrent op transitioned, others stuck forever
    pub(super) fn transition_activity_to_warm(&mut self, verb: &str) {
        if let Some(item) = self
            .activity_items
            .iter_mut()
            .rev() // Search from newest to oldest
            .find(|i| i.verb == verb && i.temp == ActivityTemp::Hot)
        {
            item.temp = ActivityTemp::Warm;
            item.duration = item.elapsed();
        }
    }

    /// Clear completed (warm) activities older than duration
    /// Also remove stuck Hot items that have been running too long
    pub fn clear_old_activities(&mut self, max_age_secs: u64) {
        let max_duration = Duration::from_secs(max_age_secs);

        self.activity_items.retain(|item| {
            match item.temp {
                ActivityTemp::Warm => {
                    // Remove warm items older than max_age
                    item.duration.map(|d| d < max_duration).unwrap_or(true)
                }
                ActivityTemp::Hot => {
                    // Remove stuck hot items (running > max_age = likely hung)
                    item.started
                        .map(|s| s.elapsed() < max_duration)
                        .unwrap_or(true)
                }
                ActivityTemp::Queued => true, // Keep queued items
            }
        });
    }
}
