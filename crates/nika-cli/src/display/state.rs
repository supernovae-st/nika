// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fold: `RunView` = a pure function of the event stream (spec §3).
//!
//! Consumes the REAL [`nika_event::Event`] taxonomy — the 11 shipped kinds,
//! nothing invented. Live cost folds from `cost_usd` fields on completed
//! tasks today; per-chunk cost ticks arrive with the §3bis proposed
//! `cost_incurred` kind when the runtime (L3) ships its emitters. Every
//! renderer (terminal · `--json` · SSE · webview) reads THIS state — one
//! truth, N surfaces.

use std::collections::BTreeMap;

use nika_event::{Event, EventKind};
use nika_types::resource::Value;

/// What the stream has said about one task so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Scheduled, dependencies not yet satisfied or not yet started.
    Pending,
    /// Executing now (the animated line).
    Running,
    /// Reached success.
    Ok,
    /// Reached failure.
    Failed,
    /// Guard false or upstream failed — never ran, by design.
    Skipped,
}

/// One render row (insertion order = first-seen order = stable layout).
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// The task id from the workflow file.
    pub id: String,
    /// Current folded state.
    pub state: TaskState,
    /// The human note carried by the latest event (`note` field).
    pub note: String,
    /// Failure detail (`detail` field) — feeds the failure card.
    pub detail: String,
}

/// The folded view of one run — everything a frame needs, nothing more.
#[derive(Debug, Default)]
pub struct RunView {
    /// Workflow name (from `workflow_started`).
    pub workflow: String,
    /// The statically-proven cost ceiling, if the workflow declared one.
    pub ceiling_usd: Option<f64>,
    /// The audit line: granted permits (joined display string).
    pub permits: Option<String>,
    /// Folded spend so far (sums `cost_usd` fields).
    pub cost_usd: f64,
    /// Token-arrival samples for the sparkline.
    pub token_samples: Vec<u64>,
    /// Terminal verdict: `Some(true)` completed · `Some(false)` failed.
    pub verdict: Option<bool>,
    /// Wall-clock span folded from event timestamps (ms).
    pub elapsed_ms: u64,
    first_ts_ms: Option<i64>,
    rows: Vec<TaskRow>,
    index: BTreeMap<String, usize>,
}

impl RunView {
    /// Start an empty view (the fold's identity element).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The render rows, in stable first-seen order.
    #[must_use]
    pub fn rows(&self) -> &[TaskRow] {
        &self.rows
    }

    /// How many rows reached a terminal state.
    #[must_use]
    pub fn done_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    TaskState::Ok | TaskState::Failed | TaskState::Skipped
                )
            })
            .count()
    }

    /// Fold one event into the view (the ONLY mutation path).
    pub fn apply(&mut self, event: &Event) {
        let ts = event.timestamp.unix_ms();
        let first = *self.first_ts_ms.get_or_insert(ts);
        self.elapsed_ms = u64::try_from(ts.saturating_sub(first)).unwrap_or(0);

        match event.kind {
            EventKind::WorkflowStarted => {
                str_field(event, "workflow")
                    .unwrap_or("workflow")
                    .clone_into(&mut self.workflow);
                self.ceiling_usd = float_field(event, "ceiling_usd");
                self.permits = str_field(event, "permits").map(str::to_owned);
            }
            EventKind::TaskScheduled => self.touch(event, TaskState::Pending),
            EventKind::TaskStarted => self.touch(event, TaskState::Running),
            EventKind::TaskCompleted => {
                self.touch(event, TaskState::Ok);
                if let Some(usd) = float_field(event, "cost_usd") {
                    self.cost_usd += usd;
                }
                if let Some(tokens) = int_field(event, "tokens") {
                    self.token_samples.push(u64::try_from(tokens).unwrap_or(0));
                }
            }
            EventKind::TaskFailed => self.touch(event, TaskState::Failed),
            EventKind::TaskSkipped => self.touch(event, TaskState::Skipped),
            EventKind::WorkflowCompleted => self.verdict = Some(true),
            EventKind::WorkflowFailed => self.verdict = Some(false),
            // Dispatch + checkpoint kinds carry no row state today; the
            // §3bis taxonomy extension (cost/retry/permit kinds) lands with
            // the runtime emitters. `#[non_exhaustive]` future kinds render
            // nothing rather than lying.
            _ => {}
        }
    }

    /// Upsert the row a task event addresses, updating state + notes.
    fn touch(&mut self, event: &Event, state: TaskState) {
        let Some(task_id) = str_field(event, "task") else {
            return; // a task event without a task field renders nothing
        };
        let idx = if let Some(&i) = self.index.get(task_id) {
            i
        } else {
            self.rows.push(TaskRow {
                id: task_id.to_owned(),
                state: TaskState::Pending,
                note: String::new(),
                detail: String::new(),
            });
            let i = self.rows.len() - 1;
            self.index.insert(task_id.to_owned(), i);
            i
        };
        let row = &mut self.rows[idx];
        row.state = state;
        if let Some(note) = str_field(event, "note") {
            note.clone_into(&mut row.note);
        }
        if let Some(detail) = str_field(event, "detail") {
            detail.clone_into(&mut row.detail);
        }
    }
}

fn value_of<'a>(event: &'a Event, key: &str) -> Option<&'a Value> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .map(|kv| &kv.value)
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    match value_of(event, key) {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn float_field(event: &Event, key: &str) -> Option<f64> {
    match value_of(event, key) {
        Some(Value::Float(f)) => Some(*f),
        #[allow(clippy::cast_precision_loss)] // display-only magnitude
        Some(Value::Int(i)) => Some(*i as f64),
        _ => None,
    }
}

fn int_field(event: &Event, key: &str) -> Option<i64> {
    match value_of(event, key) {
        Some(Value::Int(i)) => Some(*i),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn demo_success_folds_to_the_storyboard_final_state() {
        let mut view = RunView::new();
        for ev in demo::success() {
            view.apply(&ev);
        }
        assert_eq!(view.workflow, "veille-news");
        assert_eq!(view.verdict, Some(true));
        assert_eq!(view.rows().len(), 5);
        assert_eq!(view.done_count(), 5);
        assert!((view.cost_usd - 0.011).abs() < 1e-9);
        assert_eq!(view.ceiling_usd, Some(0.04));
        let states: Vec<TaskState> = view.rows().iter().map(|r| r.state).collect();
        assert_eq!(
            states,
            [
                TaskState::Ok,
                TaskState::Ok,
                TaskState::Ok,
                TaskState::Ok,
                TaskState::Skipped
            ]
        );
    }

    #[test]
    fn demo_failure_folds_to_failed_verdict_with_detail() {
        let mut view = RunView::new();
        for ev in demo::failure() {
            view.apply(&ev);
        }
        assert_eq!(view.verdict, Some(false));
        let failed: Vec<&TaskRow> = view
            .rows()
            .iter()
            .filter(|r| r.state == TaskState::Failed)
            .collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].detail.contains("NIKA-431"));
    }

    #[test]
    fn fold_is_prefix_monotone_done_never_exceeds_rows() {
        // Property-lite: every prefix of the stream yields a consistent view.
        let events = demo::success();
        for cut in 0..=events.len() {
            let mut view = RunView::new();
            for ev in &events[..cut] {
                view.apply(ev);
            }
            assert!(view.done_count() <= view.rows().len());
            assert!(view.cost_usd >= 0.0);
        }
    }

    #[test]
    fn unknown_task_events_render_nothing_not_garbage() {
        let mut view = RunView::new();
        // A task_started with NO task field must not invent a row.
        let ev = demo::bare_event(EventKind::TaskStarted, 100);
        view.apply(&ev);
        assert!(view.rows().is_empty());
    }
}
