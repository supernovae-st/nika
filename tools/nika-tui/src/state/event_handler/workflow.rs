// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow event handlers
//!
//! WorkflowStarted, WorkflowCompleted, WorkflowFailed, WorkflowAborted,
//! WorkflowPaused, WorkflowResumed

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::TuiState;
use crate::state::notification::Notification;
use crate::theme::{MissionPhase, TaskStatus};

impl TuiState {
    pub(super) fn on_workflow_started(&mut self, task_count: usize, generation_id: &str) {
        self.workflow.task_count = task_count;
        self.workflow.phase = MissionPhase::Countdown;
        self.workflow.started_at = Some(Instant::now());
        self.workflow.generation_id = Some(generation_id.to_string());
        // TIER 4.1: Mark all panels dirty on workflow start
        self.dirty.mark_all();
        // TIER 4.4: Clear JSON cache on workflow start
        self.json_cache.clear();
    }

    pub(super) fn on_workflow_completed(
        &mut self,
        final_output: &Arc<Value>,
        total_duration_ms: u64,
        timestamp_ms: u64,
    ) {
        self.workflow.phase = MissionPhase::MissionSuccess;
        self.workflow.final_output = Some(Arc::clone(final_output));
        self.workflow.total_duration_ms = Some(total_duration_ms);
        self.current_task = None;

        // TIER 3.4: Add success notification
        let duration_secs = total_duration_ms as f64 / 1000.0;
        self.add_notification(Notification::success(
            format!(
                "Magnificent! Warped through in {:.1}s ({}/{} tasks)",
                duration_secs, self.workflow.tasks_completed, self.workflow.task_count
            ),
            timestamp_ms,
        ));
        // TIER 4.1: Mark progress and status dirty
        self.dirty.progress = true;
        self.dirty.status = true;
    }

    pub(super) fn on_workflow_failed(&mut self, error: &str, timestamp_ms: u64) {
        self.workflow.phase = MissionPhase::Abort;
        self.workflow.error_message = Some(error.to_string());
        self.current_task = None;

        // Kill any still-Running tasks — they will never receive a TaskFailed event
        for task in self.tasks.values_mut() {
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Failed;
                if task.error.is_none() {
                    task.error = Some(error.to_string());
                }
            }
        }

        // TIER 3.4: Add error notification
        self.add_notification(Notification::error(
            format!("RAWR! Mission failed: {}", error),
            timestamp_ms,
        ));
        // TIER 4.1: Mark progress, status, and notifications dirty
        self.dirty.progress = true;
        self.dirty.status = true;
        self.dirty.notifications = true;
    }

    pub(super) fn on_workflow_aborted(
        &mut self,
        reason: &str,
        duration_ms: u64,
        running_tasks: &[Arc<str>],
        timestamp_ms: u64,
    ) {
        self.workflow.phase = MissionPhase::Abort;
        self.workflow.error_message = Some(format!("Aborted: {}", reason));
        self.workflow.total_duration_ms = Some(duration_ms);
        self.current_task = None;

        // Mark interrupted running tasks as Skipped so they stop spinning
        for task_id in running_tasks {
            if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                if task.status == TaskStatus::Running {
                    task.status = TaskStatus::Skipped;
                }
            }
        }

        // TIER 3.4: Add abort notification
        let task_info = if running_tasks.is_empty() {
            String::new()
        } else {
            format!(" ({} tasks interrupted)", running_tasks.len())
        };
        self.add_notification(Notification::warning(
            format!("Mission aborted: {}{}", reason, task_info),
            timestamp_ms,
        ));
        // Mark all relevant panels dirty
        self.dirty.progress = true;
        self.dirty.status = true;
        self.dirty.notifications = true;
    }

    pub(super) fn on_workflow_paused(&mut self, timestamp_ms: u64) {
        // P2 Fix: Save phase before pausing for proper restoration
        self.workflow.phase_before_pause = Some(self.workflow.phase);
        self.workflow.paused = true;
        self.workflow.phase = MissionPhase::Pause;
        self.add_notification(Notification::warning(
            "Mission paused - press SPACE to resume",
            timestamp_ms,
        ));
        self.dirty.progress = true;
        self.dirty.status = true;
    }

    pub(super) fn on_workflow_resumed(&mut self, timestamp_ms: u64) {
        self.workflow.paused = false;
        // P2 Fix: Restore saved phase, or infer from current state
        if let Some(phase) = self.workflow.phase_before_pause.take() {
            self.workflow.phase = phase;
        } else if self.current_task.is_some() {
            self.workflow.phase = MissionPhase::Orbital;
        } else {
            self.workflow.phase = MissionPhase::Countdown;
        }
        self.add_notification(Notification::info(
            "Mission resumed - engines back online!",
            timestamp_ms,
        ));
        self.dirty.progress = true;
        self.dirty.status = true;
    }
}
