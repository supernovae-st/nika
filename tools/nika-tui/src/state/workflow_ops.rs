// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow operations for TuiState
//!
//! Contains workflow status queries (is_running, is_failed, is_success),
//! pause/debug controls, retry logic, and error dismissal.

use nika_engine::event::EventKind;

use super::types::{Breakpoint, Metrics};
use super::TuiState;

use crate::theme::{MissionPhase, TaskStatus};

impl TuiState {
    /// Toggle pause state
    ///
    /// P0 Fix: Uses workflow.paused as single source of truth
    pub fn toggle_pause(&mut self) {
        if !self.workflow.paused {
            // Pausing: save current phase before overwriting
            self.workflow.phase_before_pause = Some(self.workflow.phase);
            self.workflow.paused = true;
            self.workflow.phase = MissionPhase::Pause;
        } else {
            // Resuming: restore saved phase or fall back to heuristic
            self.workflow.paused = false;
            if let Some(phase) = self.workflow.phase_before_pause.take() {
                self.workflow.phase = phase;
            } else if self.current_task.is_some() {
                self.workflow.phase = MissionPhase::Orbital;
            } else {
                self.workflow.phase = MissionPhase::Countdown;
            }
        }
        self.dirty.progress = true;
        self.dirty.status = true;
    }

    /// Check if execution is paused (unified accessor)
    pub fn is_paused(&self) -> bool {
        self.workflow.paused
    }

    /// Check if a breakpoint should trigger
    pub fn should_break(&self, kind: &EventKind) -> bool {
        if self.breakpoints.is_empty() {
            return false;
        }

        match kind {
            EventKind::TaskStarted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::BeforeTask(task_id.to_string())),
            EventKind::TaskCompleted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string())),
            EventKind::TaskFailed { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string())),
            EventKind::McpInvoke { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string())),
            EventKind::AgentTurn {
                task_id,
                turn_index,
                ..
            } => self
                .breakpoints
                .contains(&Breakpoint::OnAgentTurn(task_id.to_string(), *turn_index)),
            _ => false,
        }
    }

    /// Check if a task has a breakpoint set (TIER 2.3)
    pub fn has_breakpoint(&self, task_id: &str) -> bool {
        self.breakpoints
            .contains(&Breakpoint::BeforeTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string()))
    }

    // ═══════════════════════════════════════════
    // RETRY SUPPORT (TIER 1.2)
    // ═══════════════════════════════════════════

    /// Check if the workflow is in a failed state (can be retried)
    pub fn is_failed(&self) -> bool {
        self.workflow.phase == MissionPhase::Abort || self.workflow.error_message.is_some()
    }

    /// Check if the workflow completed successfully
    pub fn is_success(&self) -> bool {
        self.workflow.phase == MissionPhase::MissionSuccess
    }

    /// Check if the workflow is still running
    pub fn is_running(&self) -> bool {
        matches!(
            self.workflow.phase,
            MissionPhase::Countdown
                | MissionPhase::Launch
                | MissionPhase::Orbital
                | MissionPhase::Rendezvous
        )
    }

    /// Reset state for retry - clears failed tasks and resets workflow phase
    ///
    /// Returns the list of task IDs that were reset (previously failed)
    pub fn reset_for_retry(&mut self) -> Vec<String> {
        let mut reset_tasks = Vec::new();

        // Reset workflow state
        self.workflow.phase = MissionPhase::Preflight;
        self.workflow.error_message = None;
        self.workflow.final_output = None;
        self.workflow.total_duration_ms = None;
        self.workflow.tasks_completed = 0;
        self.workflow.started_at = None;

        // Reset all non-Success tasks to pending (Running/Skipped/Failed from aborted runs)
        for (task_id, task) in &mut self.tasks {
            if !matches!(task.status, TaskStatus::Success) {
                task.status = TaskStatus::Pending;
                task.duration_ms = None;
                task.error = None;
                task.output = None;
                reset_tasks.push(task_id.clone());
            }
        }

        // Clear current task
        self.current_task = None;

        // Clear agent turns
        self.agent.turns.clear();

        // Reset metrics
        self.metrics = Metrics::default();

        // Clear MCP calls from previous run — new run starts fresh
        self.mcp.calls.clear();
        self.mcp.seq = 0;
        self.mcp.selected_idx = None;

        self.dirty.mark_all();

        reset_tasks
    }

    /// P3 Fix: Dismiss error message
    /// Clears the workflow error message without resetting the entire workflow state
    pub fn dismiss_error(&mut self) -> bool {
        if self.workflow.error_message.is_some() {
            self.workflow.error_message = None;
            self.dirty.progress = true;
            self.dirty.status = true;
            true
        } else {
            false
        }
    }

    /// Get current DAG version for cache invalidation
    ///
    /// Uses timeline_version since DAG changes track task changes.
    pub fn dag_version(&self) -> u32 {
        self.timeline_version
    }

    /// Clear all dirty flags after render completes
    ///
    /// Call this at the end of render_unified_frame() to reset
    /// dirty state for the next frame.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }
}
