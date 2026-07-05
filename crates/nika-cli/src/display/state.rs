// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fold: `RunView` = a pure function of the event stream (spec §3).
//!
//! Consumes the REAL [`nika_event::Event`] taxonomy — the shipped kinds,
//! nothing invented (the census lives in nika-event's `ALL` slice · a
//! hand-typed count here rotted twice). Row states cover the full §3.1
//! table (pending/running/ok/failed/retrying/skipped/cancelled). Live
//! cost folds from `cost_usd` fields on completed tasks; per-chunk
//! `cost_incurred` ticks are a fold extension the runtime's cost meter
//! arrives with (consumer-signal gated). Every renderer (terminal ·
//! `--json` · SSE · webview) reads THIS state — one truth, N surfaces.

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
    /// An attempt failed · a retry is scheduled (§3.1 `↻` · yellow).
    Retrying,
    /// Guard false — never ran, by design.
    Skipped,
    /// Cancelled (upstream failure · operator stop · §3.1 `◼` · dim ·
    /// a decision, not a defect — never red).
    Cancelled,
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
    /// `task_started` stamp (unix ms) — feeds the live elapsed readout.
    pub started_ms: Option<i64>,
    /// Terminal stamp (unix ms) — completion · failure · skip · cancel.
    pub ended_ms: Option<i64>,
    /// The runtime-measured `duration_ms` field (the REAL wall time —
    /// event stamps are settle-time, this one is measured at dispatch).
    pub duration_ms: Option<u64>,
    /// Per-task spend (`cost_usd` on the terminal frame).
    pub cost_usd: Option<f64>,
    /// The model an inference note named (`infer · <model>` / `agent ·
    /// <model>` — the runtime's note vocabulary), kept once seen: the
    /// terminal note overwrites `note`, this survives for the verdict
    /// surface.
    pub model: Option<String>,
}

impl TaskRow {
    /// The task's best-known wall duration: the runtime-measured
    /// `duration_ms` when the stream carried it, else the stamp span.
    /// `None` for a task that never reached a terminal state.
    #[must_use]
    pub fn wall_ms(&self) -> Option<u64> {
        if let Some(d) = self.duration_ms {
            return Some(d);
        }
        let (start, end) = (self.started_ms?, self.ended_ms?);
        u64::try_from(end.saturating_sub(start)).ok()
    }
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
    /// A WORKFLOW-level failure reason carried on `workflow_failed` (e.g. a
    /// run-end NIKA-VAR-009 typed-output breach) — not tied to a task row.
    pub workflow_detail: Option<String>,
    /// Wall-clock span folded from event timestamps (ms).
    pub elapsed_ms: u64,
    /// Retry attempts observed across the run (`task_retrying` count).
    pub retries: u32,
    first_ts_ms: Option<i64>,
    last_ts_ms: Option<i64>,
    /// The static wave plan (task ids per wave · from the check report) —
    /// side information the run verb injects; the fold never derives it.
    plan_waves: Option<Vec<Vec<String>>>,
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

    /// The latest event stamp folded so far (unix ms) — "now" as the
    /// stream knows it (feeds the running row's live elapsed).
    #[must_use]
    pub fn last_ts_ms(&self) -> Option<i64> {
        self.last_ts_ms
    }

    /// Inject the static wave plan (task ids per wave · from the check
    /// report). Side information for the lane markers + the DAG-shape
    /// glyph — a replayed trace without it falls back to interval
    /// reconstruction.
    pub fn set_plan(&mut self, waves: Vec<Vec<String>>) {
        self.plan_waves = Some(waves);
    }

    /// The injected wave plan, when the caller provided one.
    #[must_use]
    pub fn plan(&self) -> Option<&[Vec<String>]> {
        self.plan_waves.as_deref()
    }

    /// How many rows reached a terminal state.
    #[must_use]
    pub fn done_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    TaskState::Ok | TaskState::Failed | TaskState::Skipped | TaskState::Cancelled
                )
            })
            .count()
    }

    /// Fold one event into the view (the ONLY mutation path).
    pub fn apply(&mut self, event: &Event) {
        let ts = event.timestamp.unix_ms();
        let first = *self.first_ts_ms.get_or_insert(ts);
        self.last_ts_ms = Some(ts);
        self.elapsed_ms = u64::try_from(ts.saturating_sub(first)).unwrap_or(0);

        match event.kind {
            EventKind::WorkflowStarted => {
                str_field(event, "workflow")
                    .unwrap_or("workflow")
                    .clone_into(&mut self.workflow);
                self.ceiling_usd = float_field(event, "ceiling_usd");
                self.permits = str_field(event, "permits").map(str::to_owned);
            }
            EventKind::TaskScheduled => {
                self.touch(event, TaskState::Pending);
            }
            EventKind::TaskStarted => {
                if let Some(i) = self.touch(event, TaskState::Running) {
                    self.rows[i].started_ms = Some(ts);
                }
            }
            EventKind::TaskCompleted => {
                let usd = float_field(event, "cost_usd");
                if let Some(i) = self.touch(event, TaskState::Ok) {
                    self.stamp_terminal(i, ts, event, usd);
                }
                if let Some(usd) = usd {
                    self.cost_usd += usd;
                }
                if let Some(tokens) = int_field(event, "tokens") {
                    self.token_samples.push(u64::try_from(tokens).unwrap_or(0));
                }
            }
            EventKind::TaskFailed => {
                let usd = float_field(event, "cost_usd");
                if let Some(i) = self.touch(event, TaskState::Failed) {
                    self.stamp_terminal(i, ts, event, usd);
                }
            }
            EventKind::TaskSkipped => {
                if let Some(i) = self.touch(event, TaskState::Skipped) {
                    self.rows[i].ended_ms = Some(ts);
                }
            }
            // §3.1 `↻` — the attempt failed · the TASK has not · the row
            // holds yellow until the terminal frame replaces it.
            EventKind::TaskRetrying => {
                self.retries = self.retries.saturating_add(1);
                self.touch(event, TaskState::Retrying);
            }
            // §3.1 `◼` — a decision, not a defect (dim · never red).
            EventKind::TaskCancelled => {
                if let Some(i) = self.touch(event, TaskState::Cancelled) {
                    self.rows[i].ended_ms = Some(ts);
                }
            }
            EventKind::WorkflowCompleted => self.verdict = Some(true),
            EventKind::WorkflowFailed => {
                self.verdict = Some(false);
                // A workflow-level reason (run-end NIKA-VAR-009) rides the
                // terminal frame's `detail` field, if present.
                self.workflow_detail = str_field(event, "detail").map(str::to_owned);
            }
            // Dispatch + checkpoint + cost/stream/permit kinds carry no
            // row state today. `#[non_exhaustive]` future kinds render
            // nothing rather than lying.
            _ => {}
        }
    }

    /// Stamp a ran-to-terminal row (completed · failed): the end stamp,
    /// the runtime-measured duration, the per-task spend.
    fn stamp_terminal(&mut self, i: usize, ts: i64, event: &Event, usd: Option<f64>) {
        let row = &mut self.rows[i];
        row.ended_ms = Some(ts);
        if let Some(d) = int_field(event, "duration_ms") {
            row.duration_ms = u64::try_from(d).ok();
        }
        if usd.is_some() {
            row.cost_usd = usd;
        }
    }

    /// Upsert the row a task event addresses, updating state + notes.
    /// Returns the row index so the caller can stamp kind-specific facts.
    fn touch(&mut self, event: &Event, state: TaskState) -> Option<usize> {
        let Some(task_id) = str_field(event, "task") else {
            return None; // a task event without a task field renders nothing
        };
        let idx = if let Some(&i) = self.index.get(task_id) {
            i
        } else {
            self.rows.push(TaskRow {
                id: task_id.to_owned(),
                state: TaskState::Pending,
                note: String::new(),
                detail: String::new(),
                started_ms: None,
                ended_ms: None,
                duration_ms: None,
                cost_usd: None,
                model: None,
            });
            let i = self.rows.len() - 1;
            self.index.insert(task_id.to_owned(), i);
            i
        };
        let row = &mut self.rows[idx];
        row.state = state;
        if let Some(note) = str_field(event, "note") {
            // The runtime's inference notes name the model (`infer ·
            // <model>`) — keep it once seen: the terminal note replaces
            // `note`, the verdict surface still wants the model.
            let model = note
                .strip_prefix("infer · ")
                .or_else(|| note.strip_prefix("agent · "));
            if let Some(m) = model
                && !m.is_empty()
            {
                row.model = Some(m.to_owned());
            }
            note.clone_into(&mut row.note);
        }
        if let Some(detail) = str_field(event, "detail") {
            detail.clone_into(&mut row.detail);
        }
        Some(idx)
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

    /// Each lifecycle kind owns a distinct fold transition — scheduled
    /// creates a Pending row, started flips it Running (deleting either
    /// match arm collapses states the renderer must distinguish).
    #[test]
    fn scheduled_then_started_walk_the_state_machine() {
        let mut view = RunView::new();
        view.apply(&demo::bare_event(EventKind::TaskScheduled, 10).with_field(
            nika_types::resource::KeyValue::new("task", Value::String("fetch_top".to_owned())),
        ));
        assert_eq!(view.rows().len(), 1, "scheduled creates the row");
        assert_eq!(view.rows()[0].state, TaskState::Pending);
        assert_eq!(view.done_count(), 0);

        view.apply(&demo::bare_event(EventKind::TaskStarted, 20).with_field(
            nika_types::resource::KeyValue::new("task", Value::String("fetch_top".to_owned())),
        ));
        assert_eq!(view.rows().len(), 1, "started upserts, never duplicates");
        assert_eq!(view.rows()[0].state, TaskState::Running);
    }

    /// The token sparkline folds EXACTLY the completed tasks that carry a
    /// `tokens` field — no invented samples, no dropped ones.
    #[test]
    fn token_samples_fold_exactly_the_reported_usage() {
        let mut view = RunView::new();
        for ev in demo::success() {
            view.apply(&ev);
        }
        // The storyboard reports usage on exactly one completion (710).
        assert_eq!(view.token_samples, vec![710]);
    }

    /// `ceiling_usd` accepts an integer-typed YAML value (`Value::Int`) —
    /// the float coercion arm is load-bearing, not decorative.
    #[test]
    fn ceiling_accepts_integer_values() {
        let mut view = RunView::new();
        view.apply(&demo::bare_event(EventKind::WorkflowStarted, 0).with_field(
            nika_types::resource::KeyValue::new("ceiling_usd", Value::Int(4)),
        ));
        assert_eq!(view.ceiling_usd, Some(4.0));
    }

    #[test]
    fn unknown_task_events_render_nothing_not_garbage() {
        let mut view = RunView::new();
        // A task_started with NO task field must not invent a row.
        let ev = demo::bare_event(EventKind::TaskStarted, 100);
        view.apply(&ev);
        assert!(view.rows().is_empty());
    }

    /// Terminal rows stamp start/end + spend, and the runtime-measured
    /// `duration_ms` field WINS over the stamp span (stamps are settle-
    /// time; the measurement is the wall truth).
    #[test]
    fn terminal_rows_stamp_time_and_spend() {
        use nika_types::resource::{KeyValue, Value};

        let mut view = RunView::new();
        for ev in demo::success() {
            view.apply(&ev);
        }
        let fetch = &view.rows()[0];
        assert_eq!(fetch.started_ms, Some(20));
        assert_eq!(fetch.ended_ms, Some(1200));
        assert_eq!(fetch.wall_ms(), Some(1180), "stamp-span fallback");
        let summarize = view
            .rows()
            .iter()
            .find(|r| r.id == "summarize")
            .expect("row");
        assert_eq!(summarize.cost_usd, Some(0.011), "per-task spend rides");

        // An explicit duration_ms field wins over the (settle-time) span.
        let mut measured = RunView::new();
        let task = || KeyValue::new("task", Value::String("t".to_owned()));
        measured.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task()));
        measured.apply(
            &demo::bare_event(EventKind::TaskCompleted, 5000)
                .with_field(task())
                .with_field(KeyValue::new("duration_ms", Value::Int(40))),
        );
        assert_eq!(measured.rows()[0].wall_ms(), Some(40), "measured wins");
        assert_eq!(measured.last_ts_ms(), Some(5000), "now = latest stamp");
    }

    /// The retry counter folds every `task_retrying` frame (feeds the
    /// verdict surface's `N retries`).
    #[test]
    fn retrying_frames_count_toward_the_retry_total() {
        let mut view = RunView::new();
        for ev in demo::retrying() {
            view.apply(&ev);
        }
        assert_eq!(view.retries, 1);
        let fresh = RunView::new();
        assert_eq!(fresh.retries, 0);
    }
}
