// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task event handlers
//!
//! TaskScheduled, TaskStarted, TaskCompleted, TaskFailed, TaskSkipped

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::TuiState;
use crate::state::notification::Notification;
use crate::state::types::TaskState;
use crate::theme::{MissionPhase, TaskStatus};

impl TuiState {
    pub(super) fn on_task_scheduled(&mut self, task_id: &str, dependencies: &[Arc<str>]) {
        let deps: Vec<String> = dependencies.iter().map(|s| s.to_string()).collect();
        let task = TaskState::new(task_id.to_string(), deps);
        self.tasks.insert(task_id.to_string(), task);
        // Guard against duplicates on retry (task may be re-scheduled)
        if !self.task_order.contains(&task_id.to_string()) {
            self.task_order.push(task_id.to_string());
        }
        // TIER 4.1: Mark progress and dag dirty
        self.dirty.progress = true;
        self.dirty.dag = true;
        self.invalidate_timeline_cache();
    }

    pub(super) fn on_task_started(&mut self, task_id: &str, verb: &str, inputs: &Value) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Running;
            task.started_at = Some(Instant::now());
            task.input = Some(Arc::new(inputs.clone()));
            task.task_type = Some(verb.to_string());
        }
        self.current_task = Some(task_id.to_string());

        // Update phase — only from active execution phases (preserve Pause/Abort/MissionSuccess)
        if self.workflow.phase == MissionPhase::Countdown {
            self.workflow.phase = MissionPhase::Launch;
        } else if matches!(
            self.workflow.phase,
            MissionPhase::Launch | MissionPhase::Orbital | MissionPhase::Rendezvous
        ) {
            self.workflow.phase = MissionPhase::Orbital;
        }
        // TIER 4.1: Mark progress and dag dirty
        self.dirty.progress = true;
        self.dirty.dag = true;
        self.invalidate_timeline_cache();
        // TIER 4.4: Invalidate task cache on start (will need re-format later)
        self.json_cache.invalidate(&format!("task:{}", task_id));
    }

    pub(super) fn on_task_completed(
        &mut self,
        task_id: &str,
        output: &Arc<Value>,
        duration_ms: u64,
        timestamp_ms: u64,
    ) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Success;
            task.duration_ms = Some(duration_ms);
            task.output = Some(Arc::clone(output));
        }
        self.workflow.tasks_completed += 1;

        // TIER 3.4: Notify on slow tasks
        let duration_secs = duration_ms as f64 / 1000.0;
        if duration_ms > 30_000 {
            self.add_notification(Notification::alert(
                format!(
                    "Sloth mode! '{}' crawled in at {:.1}s",
                    task_id, duration_secs
                ),
                timestamp_ms,
            ));
        } else if duration_ms > 10_000 {
            self.add_notification(Notification::warning(
                format!("Taking its time... '{}' at {:.1}s", task_id, duration_secs),
                timestamp_ms,
            ));
        }

        // P1 Fix: Only clear agent state if this task was actually an agent task
        // Check task_type to avoid clearing during parallel workflows
        if let Some(task) = self.tasks.get(task_id) {
            if task.task_type.as_deref() == Some("agent") {
                self.agent.turns.clear();
                self.agent.streaming_buffer.clear();
                self.agent.max_turns = None;
            }
        }
        // TIER 4.1: Mark progress and dag dirty
        self.dirty.progress = true;
        self.dirty.dag = true;
        self.invalidate_timeline_cache();
        // TIER 4.4: Invalidate task cache on completion (new output)
        self.json_cache.invalidate(&format!("task:{}", task_id));
    }

    pub(super) fn on_task_failed(&mut self, task_id: &str, error: &str, duration_ms: u64) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Failed;
            task.duration_ms = Some(duration_ms);
            task.error = Some(error.to_string());
        }
        // Clear current_task so the UI stops highlighting this task as active
        if self.current_task.as_deref() == Some(task_id) {
            self.current_task = None;
        }
        // TIER 4.1: Mark progress, dag, and status dirty
        self.dirty.progress = true;
        self.dirty.dag = true;
        self.dirty.status = true;
        self.invalidate_timeline_cache();
    }

    pub(super) fn on_task_skipped(&mut self, task_id: &str, reason: &str) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Skipped;
            task.error = Some(format!("skipped: {}", reason));
        }
        self.dirty.progress = true;
        self.dirty.dag = true;
        self.dirty.status = true;
        self.invalidate_timeline_cache();
    }
}
