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
    /// Guard false — never ran, by design (`↷` · dim · a decision).
    Skipped,
    /// Cancelled (upstream failure · operator stop · `⊘ blocked` · dim
    /// · the path died upstream — never red, the defect is elsewhere).
    Cancelled,
    /// Awaiting a human answer (ADR-099 rider · `◇` · amber · the run
    /// paused durably at this gate — a state, never a defect).
    Paused,
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
    /// The task's output as ONE compact JSON text — the ADR-099 `output`
    /// trace field on `task_completed` / `task_cache_hit`. `None` when
    /// the frame carried none: a skip · a failure · an older engine's
    /// trace · the runtime's secret-drop (an output text leaking a
    /// resolved secret value never reaches the stream — ADR-099 §1).
    pub output_json: Option<String>,
    /// Per-task token usage (`tokens` on the terminal frame).
    pub tokens: Option<u64>,
    /// The FIRST `task_started` note (`infer · <model>` · `invoke ·
    /// nika:fetch` — the runtime's verb vocabulary), kept verbatim: the
    /// terminal note overwrites `note`, this survives for the per-task
    /// trace readers (`trace outputs`' verb column).
    pub started_note: Option<String>,
    /// The ADR-099 task-definition hash (`def_hash` · blake3 hex) when
    /// the terminal frame carried the checkpoint trio — the identity
    /// half `trace peek` surfaces beside the value.
    pub def_hash: Option<String>,
    /// The ADR-099 resolved-input hash (`input_hash` · blake3 hex).
    pub input_hash: Option<String>,
    /// The row reached Ok via a `task_cache_hit` rehydration (ADR-099
    /// `--resume`), never by running here — the render distinguishes
    /// `↷ cache hit (resume)` from a ran-to-green row.
    pub cached: bool,
    /// The task settled through an `on_error.recover` repair: a
    /// `task_recovered` frame preceded its terminal (D-2026-07-08-N4
    /// sequence · engine#313). The FACT survives to every settled
    /// surface (` · recovered`) — a repaired success must never render
    /// byte-identical to a clean one (#319).
    pub recovered: bool,
    /// The OBS-E non-fatal `warning` the terminal frame carried (#410 ·
    /// the thinking model that spent its budget and answered blank) —
    /// the task succeeded, but the console must say what the trace
    /// knows, or a green run silently feeds "" downstream.
    pub warning: Option<String>,
    /// A `for_each` fan-out's per-item terminals (#1276 · #1397): the
    /// `items` JSON array text the terminal frame carried · index · item ·
    /// status · code · message. `None` for every other row.
    pub items_json: Option<String>,
    /// Prompt tokens the provider metered (`tokens_in`) — INCLUDES the
    /// cache subsets, so the full-rate portion is the remainder. The
    /// terminal frame carries these beside `tokens` when the provider
    /// reported them; a reader recomputes `cost_usd` from them and the
    /// pinned price table. `None` is "not reported", never a zero.
    pub tokens_in: Option<u64>,
    /// Completion tokens the provider metered (`tokens_out`) — includes
    /// the reasoning subset.
    pub tokens_out: Option<u64>,
    /// Prompt tokens served from the provider's cache (`tokens_cache_read`
    /// · a subset of `tokens_in`, priced at the cache-read rate). The one
    /// number that tells a warm cache from a price change.
    pub tokens_cache_read: Option<u64>,
    /// Prompt tokens written to the provider's cache
    /// (`tokens_cache_write` · a subset of `tokens_in`).
    pub tokens_cache_write: Option<u64>,
    /// Reasoning/thinking tokens (`tokens_reasoning` · a subset of
    /// `tokens_out`).
    pub tokens_reasoning: Option<u64>,
    /// The born origin of the task's UNTRUSTED value (F-O1 · the
    /// terminal frame's `integrity_source`, present only when `integrity`
    /// reads `untrusted`): the ingress task that let the content in (a
    /// fetch · an exec · a recovered fallback) or the `inputs.<name>` the
    /// caller supplied. A trusted success renders as one; a task that
    /// drank untrusted content must say where it was born. It is NOT a
    /// repair marker: `recovered` is (the `task_recovered` frame). The two
    /// were conflated in prose once (#1444) and a wave of eight personas
    /// read « input from recovered » on runs that repaired nothing.
    pub integrity_source: Option<String>,
}

impl TaskRow {
    /// The metered call's split, in reading order, present meters only —
    /// the one reading order the card's totals row follows, and the one
    /// `trace peek` will read the day it prints the split, so a receipt
    /// read in prose and a receipt read by a machine name the same
    /// numbers in the same order.
    ///
    /// `input` includes the cache subsets and `output` includes
    /// `reasoning`, per the semantics the wires normalize to: a reader
    /// recomputes the full-rate portion as `input - cache_read -
    /// cache_write`. Empty when the frame carried no split (a mock or
    /// local seat, a tool task, an older engine's trace).
    #[must_use]
    pub fn meters(&self) -> Vec<(&'static str, u64)> {
        [
            ("input", self.tokens_in),
            ("cache_read", self.tokens_cache_read),
            ("cache_write", self.tokens_cache_write),
            ("output", self.tokens_out),
            ("reasoning", self.tokens_reasoning),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|n| (name, n)))
        .collect()
    }

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
    /// A sibling surface (the sink's living wire map) already draws the
    /// DAG — the frame's own wave-column line stands down.
    pub external_map: bool,
    /// Workflow name (from `workflow_started`).
    pub workflow: String,
    /// The statically-proven cost ceiling, if the workflow declared one.
    pub ceiling_usd: Option<f64>,
    /// The audit line: granted permits (joined display string).
    pub permits: Option<String>,
    /// Folded spend so far (sums `cost_usd` fields).
    pub cost_usd: f64,
    /// Calls whose spend is NOT in `cost_usd` (local · mock · uncataloged
    /// · provider silent) — folded live from per-task `cost_unpriced`
    /// fields, then OVERWRITTEN by the terminal frame's authoritative
    /// `unpriced_calls` (leaf-level: a fan-out counts its iterations).
    pub unpriced_calls: u64,
    /// Calls whose spend IS in `cost_usd` — folded live from per-task
    /// `cost_usd` fields, then OVERWRITTEN by the terminal frame's
    /// `priced_calls`. Zero, with no total, means nothing was metered: the
    /// card says the word, never a `$0.00` nobody metered (ADR-128).
    pub priced_calls: u64,
    /// Token-arrival samples for the sparkline.
    pub token_samples: Vec<u64>,
    /// Terminal verdict: `Some(true)` completed · `Some(false)` failed.
    pub verdict: Option<bool>,
    /// The run ended on `workflow_cancelled` (#1438): the operator's
    /// cancellation at a wave boundary · a verdict of its own (the card
    /// says so, never the failure card).
    pub cancelled: bool,
    /// The task a `workflow_paused` frame named (ADR-099 rider) — the
    /// run ended AWAITING, neither verdict applies (the paused card's
    /// key · `None` on every other run).
    pub paused_task: Option<String>,
    /// The paused prompt's `mode:` (`confirm` · `input` · `choice`) —
    /// the card's `--answer` shape. `None` when the frame omitted it
    /// (stdlib default is confirm).
    pub(crate) paused_mode: Option<String>,
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
    blocked_by: BTreeMap<String, String>,
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

    /// The upstream whose settle kept `task_id`'s gate closed.
    pub(crate) fn blocked_by(&self, task_id: &str) -> Option<&str> {
        self.blocked_by.get(task_id).map(String::as_str)
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

    /// How many REPAIRS the run made — feeds the verdict card and the
    /// final meter (the `(N unpriced)` honesty style: a non-zero count is
    /// never silent). A fan-out row stands for as many repairs as its
    /// item table records: the banner used to count ROWS while the task
    /// line counted ITEMS, and one screen said `1 recovered` above
    /// `2 recovered: …` (wave 3 · persona 10 · 2026-09-06).
    #[must_use]
    pub fn recovered_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.recovered)
            .map(recovered_items)
            .sum()
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

    /// FAILED rows · rides the final meter beside `recovered` (same honesty
    /// style — `done` counts every terminal state, so a failing run's meter
    /// read byte-identical to a clean one · caught live 2026-07-10 · #393).
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.state == TaskState::Failed)
            .count()
    }

    /// CANCELLED rows · rides the final meter beside `failed` (the same
    /// #393 honesty style): one root failure cancelling 22 downstream
    /// tasks used to read `23/23 done · 1 failed` — the fallout count
    /// stayed invisible and the wall of `⊘` rows had no summary voice.
    #[must_use]
    pub fn cancelled_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.state == TaskState::Cancelled)
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
                    let row = &mut self.rows[i];
                    row.started_ms = Some(ts);
                    if row.started_note.is_none() && !row.note.is_empty() {
                        row.started_note = Some(row.note.clone());
                    }
                }
            }
            EventKind::TaskCompleted => self.apply_task_completed(event, ts),
            // ADR-099 `--resume` — a rehydrated success: the row reads Ok
            // with the "cache hit" note the frame carries (VISIBLE, never
            // silent); zero duration/spend (the task never ran here). The
            // rehydrated output rides the frame — the shape tail and the
            // trace readers see the SAME value a live run would carry.
            EventKind::TaskCacheHit => {
                if let Some(i) = self.touch(event, TaskState::Ok) {
                    self.rows[i].ended_ms = Some(ts);
                    self.rows[i].cached = true;
                    self.keep_output(i, event);
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
            // D-2026-07-08-N4 (engine#313) — the repair frame: an
            // `on_error.recover` fallback stood in after a failed attempt;
            // the terminal `task_completed` follows it. The row stays
            // in-flight (Running) — only the FACT lands, so the settled
            // render can say ` · recovered` (#319).
            EventKind::TaskRecovered => {
                if let Some(i) = self.touch(event, TaskState::Running) {
                    self.rows[i].recovered = true;
                }
            }
            // §3.1 blocked `⊘` — a decision, not a defect (dim · never red).
            EventKind::TaskCancelled => self.cancel_row(event, ts),
            EventKind::WorkflowCompleted => {
                self.verdict = Some(true);
                self.absorb_terminal_cost(event);
            }
            EventKind::WorkflowCancelled => self.apply_workflow_cancelled(event),
            EventKind::WorkflowFailed => {
                self.verdict = Some(false);
                // A workflow-level reason (run-end NIKA-VAR-009) rides the
                // terminal frame's `detail` field, if present.
                self.workflow_detail = str_field(event, "detail").map(str::to_owned);
                self.absorb_terminal_cost(event);
            }
            // ADR-099 rider — the run paused on a human gate: no verdict
            // (neither success nor failure) · the gate's row turns `◇`
            // and the paused card names the awaiting task, so a live
            // frame AND a replayed trace both read honestly (the frame
            // used to stay mute — a paused run looked merely unfinished).
            EventKind::WorkflowPaused => {
                let task = str_field(event, "task").unwrap_or("a prompt").to_owned();
                self.workflow_detail = Some(format!("paused · awaiting an answer for `{task}`"));
                self.touch(event, TaskState::Paused);
                self.paused_task = Some(task);
                self.paused_mode = str_field(event, "mode").map(str::to_owned);
            }
            // Dispatch + checkpoint + cost/stream/permit kinds carry no
            // row state today. `#[non_exhaustive]` future kinds render
            // nothing rather than lying.
            _ => {}
        }
    }

    /// The operator's cancellation at a wave boundary (#1438): a decision,
    /// never a defect · the detail says what completed and what never
    /// started, the cost summary rides like every terminal.
    fn apply_workflow_cancelled(&mut self, event: &Event) {
        self.verdict = Some(false);
        self.cancelled = true;
        self.workflow_detail = str_field(event, "detail").map(str::to_owned);
        self.absorb_terminal_cost(event);
    }

    /// One `task_completed` frame — row terminal stamp · output · tokens
    /// · the live spend/unpriced fold (terminal-frame authoritative
    /// values overwrite these at run end).
    fn apply_task_completed(&mut self, event: &Event, ts: i64) {
        let usd = float_field(event, "cost_usd");
        if let Some(i) = self.touch(event, TaskState::Ok) {
            self.stamp_terminal(i, ts, event, usd);
            self.keep_output(i, event);
            if let Some(tokens) = int_field(event, "tokens") {
                self.rows[i].tokens = u64::try_from(tokens).ok();
            }
            // The OBS-E `warning` rider (#410) — kept on the row so the
            // final frame can speak it (the trace alone knowing is the
            // observability gap this closes).
            if let Some(warning) = str_field(event, "warning") {
                self.rows[i].warning = Some(warning.to_owned());
            }
        }
        // Live approximation (per-task) — the terminal frame's
        // leaf-level `unpriced_calls` overwrites it at run end.
        if str_field(event, "cost_unpriced").is_some() {
            self.unpriced_calls = self.unpriced_calls.saturating_add(1);
        }
        if let Some(usd) = usd {
            self.cost_usd += usd;
            self.priced_calls = self.priced_calls.saturating_add(1);
        }
        if let Some(tokens) = int_field(event, "tokens") {
            self.token_samples.push(u64::try_from(tokens).unwrap_or(0));
        }
    }

    /// Fold the terminal frame's AUTHORITATIVE cost summary over the
    /// live per-task approximation: `unpriced_calls` is leaf-level (a
    /// fan-out counts its iterations), and `total_cost_usd` also covers
    /// spend the task frames cannot carry (a billed attempt whose task
    /// later settled FAILED emits no `cost_usd` on `task_failed` — only
    /// the run total remembers it; ignoring it would be the exact
    /// partial-as-total lie this arc bans).
    fn absorb_terminal_cost(&mut self, event: &Event) {
        if let Some(n) = int_field(event, "unpriced_calls") {
            self.unpriced_calls = u64::try_from(n).unwrap_or(self.unpriced_calls);
        }
        if let Some(n) = int_field(event, "priced_calls") {
            self.priced_calls = u64::try_from(n).unwrap_or(self.priced_calls);
        }
        if let Some(total) = float_field(event, "total_cost_usd") {
            self.cost_usd = total;
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
        // The WHY reaches the operator: without this copy the per-row
        // failure card (render.rs) never fires — the journal carried
        // « NIKA-VAR-001 · supply it with --var … » while the live
        // render showed a mute ✖ (the user-sim finding · the
        // dead-on-the-wire class). Non-failed frames carry no detail.
        if let Some(d) = str_field(event, "detail") {
            d.clone_into(&mut row.detail);
        }
        // The metered call's split — carried by `task_completed` AND by
        // `task_failed` (a billed-then-failed attempt explains its own
        // `cost_usd` too). Absent meters stay absent: a provider that
        // reported nothing must not render as four honest zeroes.
        let meter =
            |event: &Event, key: &str| int_field(event, key).and_then(|n| u64::try_from(n).ok());
        row.tokens_in = meter(event, "tokens_in").or(row.tokens_in);
        row.tokens_out = meter(event, "tokens_out").or(row.tokens_out);
        row.tokens_cache_read = meter(event, "tokens_cache_read").or(row.tokens_cache_read);
        row.tokens_cache_write = meter(event, "tokens_cache_write").or(row.tokens_cache_write);
        row.tokens_reasoning = meter(event, "tokens_reasoning").or(row.tokens_reasoning);
        // #1276 · #1397 · a fan-out's item table survives to the readers.
        if let Some(items) = str_field(event, "items") {
            row.items_json = Some(items.to_owned());
        }
        // F-O1 · a task whose value is untrusted names its born origin on
        // every prose surface, not only in the JSON.
        if str_field(event, "integrity") == Some("untrusted")
            && let Some(source) = str_field(event, "integrity_source")
        {
            row.integrity_source = Some(source.to_owned());
        }
    }

    /// Keep the ADR-099 checkpoint trio (the `output` value as ONE
    /// compact JSON text + the `def_hash`/`input_hash` identity) when
    /// the frame carried it. A frame without it folds to `None` — the
    /// honest no-data arm every downstream summary respects (notably
    /// the runtime's secret-drop: a leaking output never rides the
    /// stream, so no preview can ever see it).
    fn keep_output(&mut self, i: usize, event: &Event) {
        let row = &mut self.rows[i];
        if let Some(text) = str_field(event, "output") {
            row.output_json = Some(text.to_owned());
        }
        if let Some(hash) = str_field(event, "def_hash") {
            row.def_hash = Some(hash.to_owned());
        }
        if let Some(hash) = str_field(event, "input_hash") {
            row.input_hash = Some(hash.to_owned());
        }
    }

    /// Settle a gate/cascade cancellation, keeping the gate's WHY.
    ///
    /// `blocked_by` names the upstream whose settle closed the edge. The
    /// runtime has always emitted it and this fold used to drop it, so
    /// the row could only echo the runtime's own note — which names no
    /// edge, no upstream and no outcome (#1198). Kept as the FACT here;
    /// the render reads the producer's outcome off the producer's own
    /// row.
    fn cancel_row(&mut self, event: &Event, ts: i64) {
        let culprit = str_field(event, "blocked_by").map(str::to_owned);
        if let Some(i) = self.touch(event, TaskState::Cancelled) {
            self.rows[i].ended_ms = Some(ts);
            let task_id = self.rows[i].id.clone();
            if let Some(culprit) = culprit {
                self.blocked_by.insert(task_id, culprit);
            } else {
                self.blocked_by.remove(&task_id);
            }
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
                output_json: None,
                tokens: None,
                tokens_in: None,
                tokens_out: None,
                tokens_cache_read: None,
                tokens_cache_write: None,
                tokens_reasoning: None,
                started_note: None,
                def_hash: None,
                input_hash: None,
                cached: false,
                recovered: false,
                warning: None,
                items_json: None,
                integrity_source: None,
            });
            let i = self.rows.len() - 1;
            self.index.insert(task_id.to_owned(), i);
            i
        };
        let row = &mut self.rows[idx];
        row.state = state;
        // D-2026-08-04-N1 · the structured `model` field is the carrier
        // — recorded fact first, render second.
        if let Some(m) = str_field(event, "model")
            && !m.is_empty()
        {
            row.model = Some(m.to_owned());
        }
        if let Some(note) = str_field(event, "note") {
            // Pre-access traces named the model ONLY inside the note
            // (`infer · <model>`) — the parse survives as a reader
            // leniency for those journals, never as the carrier: the
            // structured field above always wins.
            if row.model.is_none() {
                let model = note
                    .strip_prefix("infer · ")
                    .or_else(|| note.strip_prefix("agent · "));
                if let Some(m) = model
                    && !m.is_empty()
                {
                    row.model = Some(m.to_owned());
                }
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

/// A string field off an event (`None` when absent or non-string) —
/// shared with the sink layer (the plain narration + heartbeat riders
/// address rows by the same `task` field this fold reads).
#[must_use]
pub fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
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

/// The repairs one recovered row stands for: a fan-out terminal carries
/// one `items` entry per iteration with its own status, so a batch that
/// recovered two items is two repairs. A row without an item table, or
/// with one that does not parse, is one repair (never zero: the row IS
/// recovered).
fn recovered_items(row: &TaskRow) -> usize {
    let Some(items) = row.items_json.as_deref() else {
        return 1;
    };
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(items)
    else {
        return 1;
    };
    rows.iter()
        .filter(|r| r.get("status").and_then(serde_json::Value::as_str) == Some("recovered"))
        .count()
        .max(1)
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
        assert_eq!(view.failed_count(), 0);
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

    /// D-2026-08-04-N1 · the structured `model` field outranks the
    /// note-parse (recorded fact over render); a fieldless frame — a
    /// pre-access trace — still parses its note (reader leniency).
    #[test]
    fn structured_model_field_outranks_the_note_parse() {
        use nika_types::id::EventId;
        use nika_types::resource::KeyValue;
        use nika_types::timestamp::Timestamp;

        let ev = |seq: u128, kind: EventKind| {
            Event::new(
                EventId::new(uuid::Uuid::from_u128(seq)),
                Timestamp::from_unix_ms(u64::try_from(seq).unwrap_or(0)),
                kind,
            )
        };
        let mut view = RunView::new();
        // structured carrier present — the field wins over a diverging note
        view.apply(
            &ev(1, EventKind::TaskCompleted)
                .with_field(KeyValue::new("task", Value::String("a".to_owned())))
                .with_field(KeyValue::new(
                    "model",
                    Value::String("ollama/qwen3.5:4b".to_owned()),
                ))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · stale/render".to_owned()),
                )),
        );
        // pre-access trace — fieldless frame still parses the note
        view.apply(
            &ev(2, EventKind::TaskCompleted)
                .with_field(KeyValue::new("task", Value::String("b".to_owned())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · mock/echo".to_owned()),
                )),
        );
        let model_of = |id: &str| {
            view.rows()
                .iter()
                .find(|r| r.id == id)
                .and_then(|r| r.model.clone())
        };
        assert_eq!(model_of("a").as_deref(), Some("ollama/qwen3.5:4b"));
        assert_eq!(model_of("b").as_deref(), Some("mock/echo"));
    }

    /// ADR-099 — `workflow_paused` folds to an AWAITING view: the gate
    /// row turns `Paused`, `paused_task` names it, and neither verdict
    /// applies (a pause is a state, never a defect).
    #[test]
    fn demo_paused_folds_to_an_awaiting_view() {
        let mut view = RunView::new();
        for ev in demo::paused() {
            view.apply(&ev);
        }
        assert_eq!(view.verdict, None, "neither success nor failure");
        assert_eq!(view.paused_task.as_deref(), Some("summarize"));
        let gate = view
            .rows()
            .iter()
            .find(|r| r.id == "summarize")
            .expect("the gate row exists");
        assert_eq!(gate.state, TaskState::Paused);
        assert_eq!(view.failed_count(), 0);
        assert!(
            view.workflow_detail
                .as_deref()
                .is_some_and(|d| d.contains("awaiting an answer")),
            "{:?}",
            view.workflow_detail
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
        assert_eq!(view.failed_count(), 1, "the meter's failed counter");
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

    /// The fold keeps the ADR-099 per-task payload fields verbatim —
    /// `output` (compact JSON text) + `tokens` land on the row; a frame
    /// WITHOUT an `output` field folds to `None` (older engines · skips
    /// · the runtime's secret-drop, ADR-099 §1: an output text leaking a
    /// resolved secret never reaches the stream — so every preview
    /// surface inherits the invariant structurally).
    #[test]
    fn completed_rows_keep_output_text_and_tokens() {
        use nika_types::resource::{KeyValue, Value};
        let mut view = RunView::new();
        let task = |name: &str| KeyValue::new("task", Value::String(name.to_owned()));
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 10)
                .with_field(task("audit"))
                .with_field(KeyValue::new(
                    "output",
                    Value::String(r#"{"total":9}"#.to_owned()),
                ))
                .with_field(KeyValue::new("tokens", Value::Int(90))),
        );
        // The secret-drop / older-engine arm: no `output` field at all.
        view.apply(&demo::bare_event(EventKind::TaskCompleted, 20).with_field(task("bare")));
        // A cache hit rehydrates its output onto the row too (ADR-099).
        view.apply(
            &demo::bare_event(EventKind::TaskCacheHit, 30)
                .with_field(task("cached"))
                .with_field(KeyValue::new("output", Value::String("\"hi\"".to_owned()))),
        );
        assert_eq!(
            view.rows()[0].output_json.as_deref(),
            Some(r#"{"total":9}"#)
        );
        assert_eq!(view.rows()[0].tokens, Some(90));
        assert_eq!(view.rows()[1].output_json, None, "no field → None");
        assert_eq!(view.rows()[1].tokens, None);
        assert_eq!(view.rows()[2].output_json.as_deref(), Some("\"hi\""));
    }

    /// The FIRST started note (the verb vocabulary — `invoke ·
    /// nika:fetch`) survives the terminal-note overwrite — the trace
    /// readers' verb column depends on it.
    #[test]
    fn started_note_survives_the_terminal_overwrite() {
        let mut view = RunView::new();
        for ev in demo::success() {
            view.apply(&ev);
        }
        let fetch = &view.rows()[0];
        assert_eq!(fetch.started_note.as_deref(), Some("invoke · nika:fetch"));
        assert_eq!(fetch.note, "http 200 · 1.2s · 34 KB", "terminal note won");
        // A row that never started (skipped) keeps None.
        let skipped = view.rows().iter().find(|r| r.id == "notify_slack");
        assert_eq!(skipped.and_then(|r| r.started_note.as_deref()), None);
    }

    /// D-2026-07-08-N4 / #319 — the repair fact folds: a `task_recovered`
    /// frame BEFORE the terminal marks the row `recovered`, the row still
    /// settles Ok, and the view counts exactly the repaired rows. A clean
    /// sibling stays unmarked (the byte-identical trap this kills).
    #[test]
    fn recovered_frame_marks_the_row_and_the_count() {
        use nika_types::resource::{KeyValue, Value};
        let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));

        let mut view = RunView::new();
        view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task("fragile")));
        view.apply(
            &demo::bare_event(EventKind::TaskRecovered, 5)
                .with_field(task("fragile"))
                .with_field(KeyValue::new(
                    "code",
                    Value::String("NIKA-BUILTIN-READ-001".to_owned()),
                )),
        );
        // Mid-repair: the fact landed, the task is still in flight.
        assert!(view.rows()[0].recovered, "the repair fact folds");
        assert_eq!(view.rows()[0].state, TaskState::Running);

        view.apply(&demo::bare_event(EventKind::TaskCompleted, 10).with_field(task("fragile")));
        view.apply(&demo::bare_event(EventKind::TaskCompleted, 20).with_field(task("clean")));

        assert_eq!(view.rows()[0].state, TaskState::Ok, "settles Ok");
        assert!(view.rows()[0].recovered, "the fact survives the terminal");
        assert!(!view.rows()[1].recovered, "a clean row stays unmarked");
        assert_eq!(view.recovered_count(), 1);
    }

    /// A fan-out that recovered two of twelve items is TWO repairs on the
    /// banner, the same number its task line prints (wave 3 · persona 10).
    #[test]
    fn the_banner_counts_recovered_items_not_rows() {
        use nika_types::resource::KeyValue;
        let task = |id: &str| KeyValue::new("task", Value::String(id.to_owned()));
        let mut view = RunView::new();
        view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task("fan")));
        view.apply(
            &demo::bare_event(EventKind::TaskRecovered, 5)
                .with_field(task("fan"))
                .with_field(KeyValue::new(
                    "code",
                    Value::String("NIKA-BUILTIN-READ-001".to_owned()),
                )),
        );
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 10)
                .with_field(task("fan"))
                .with_field(KeyValue::new(
                    "items",
                    Value::String(
                        r#"[{"index":0,"status":"success"},{"index":7,"status":"recovered","code":"NIKA-BUILTIN-READ-001"},{"index":8,"status":"recovered"}]"#
                            .to_owned(),
                    ),
                )),
        );
        view.apply(&demo::bare_event(EventKind::TaskStarted, 11).with_field(task("solo")));
        view.apply(&demo::bare_event(EventKind::TaskRecovered, 12).with_field(task("solo")));
        view.apply(&demo::bare_event(EventKind::TaskCompleted, 13).with_field(task("solo")));
        assert_eq!(view.recovered_count(), 3, "two items + one plain row");
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

    /// #1438 · `workflow_cancelled` is a verdict of its own: the view says
    /// cancelled (never a bare failure), keeps the terminal's detail and
    /// absorbs its cost summary like the other terminals.
    #[test]
    fn workflow_cancelled_folds_to_a_cancelled_verdict() {
        use nika_types::resource::{KeyValue, Value};
        let mut view = RunView::new();
        view.apply(
            &demo::bare_event(EventKind::WorkflowCancelled, 10)
                .with_field(KeyValue::new(
                    "detail",
                    Value::String("cancelled by the operator".to_owned()),
                ))
                .with_field(KeyValue::new("unpriced_calls", Value::Int(2))),
        );
        assert!(view.cancelled, "the fold names the cancellation");
        assert_eq!(view.verdict, Some(false), "not a success");
        assert_eq!(
            view.workflow_detail.as_deref(),
            Some("cancelled by the operator")
        );
        assert_eq!(view.unpriced_calls, 2, "the terminal's summary rides");
    }

    /// #1444 · the fold keeps the lineage a terminal frame carries: the
    /// upstream whose recovered fallback fed this task.
    #[test]
    fn a_task_fed_by_a_recovered_fallback_keeps_its_source() {
        use nika_types::resource::{KeyValue, Value};
        let mut view = RunView::new();
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 10)
                .with_field(KeyValue::new("task", Value::String("c".to_owned())))
                .with_field(KeyValue::new(
                    "integrity",
                    Value::String("untrusted".to_owned()),
                ))
                .with_field(KeyValue::new(
                    "integrity_source",
                    Value::String("b".to_owned()),
                )),
        );
        assert_eq!(view.rows()[0].integrity_source.as_deref(), Some("b"));
        let mut clean = RunView::new();
        clean.apply(
            &demo::bare_event(EventKind::TaskCompleted, 10)
                .with_field(KeyValue::new("task", Value::String("c".to_owned()))),
        );
        assert!(
            clean.rows()[0].integrity_source.is_none(),
            "a clean row has no source"
        );
    }

    fn ev_at(kind: EventKind, ms: u64, fields: &[(&str, Value)]) -> Event {
        use nika_types::resource::KeyValue;
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, v.clone()));
        }
        e
    }

    fn s(v: &str) -> Value {
        Value::String(v.to_owned())
    }

    /// The metered split lands on the row from `task_completed`, in the
    /// ONE reading order every surface speaks. The measured probe: a
    /// 5015-token prompt (4992 of them served from the provider's cache)
    /// answered in one token — the frame that used to carry `tokens: 1`
    /// and nothing else.
    #[test]
    fn the_usage_split_lands_on_the_row_in_reading_order() {
        let mut view = RunView::new();
        view.apply(&ev_at(EventKind::TaskStarted, 100, &[("task", s("ask"))]));
        view.apply(&ev_at(
            EventKind::TaskCompleted,
            900,
            &[
                ("task", s("ask")),
                ("tokens", Value::Int(1)),
                ("tokens_in", Value::Int(5015)),
                ("tokens_out", Value::Int(1)),
                ("tokens_cache_read", Value::Int(4992)),
            ],
        ));
        let row = &view.rows()[0];
        assert_eq!(row.tokens, Some(1), "`tokens` keeps its meaning");
        assert_eq!(row.tokens_in, Some(5015));
        assert_eq!(row.tokens_cache_read, Some(4992));
        assert_eq!(
            row.tokens_cache_write, None,
            "an unreported meter stays absent, never a zero"
        );
        assert_eq!(
            row.meters(),
            vec![("input", 5015), ("cache_read", 4992), ("output", 1)],
            "input · cache_read · cache_write · output · reasoning, present only"
        );
    }

    /// A BILLED-then-failed attempt explains its own spend: the split
    /// rides `task_failed` too, so a failed frame carrying `cost_usd` is
    /// never a number without a receipt.
    #[test]
    fn a_billed_then_failed_frame_carries_its_own_receipt() {
        let mut view = RunView::new();
        view.apply(&ev_at(EventKind::TaskStarted, 100, &[("task", s("ask"))]));
        view.apply(&ev_at(
            EventKind::TaskFailed,
            900,
            &[
                ("task", s("ask")),
                ("detail", s("NIKA-INFER-004 · the model refused")),
                ("cost_usd", Value::Float(0.000_75)),
                ("tokens_in", Value::Int(5015)),
                ("tokens_out", Value::Int(0)),
            ],
        ));
        let row = &view.rows()[0];
        assert_eq!(row.state, TaskState::Failed);
        assert_eq!(row.tokens_in, Some(5015));
        assert_eq!(row.meters(), vec![("input", 5015), ("output", 0)]);
    }

    /// A mock seat reports no meters — and the row invents none.
    #[test]
    fn an_unmetered_frame_leaves_the_row_meterless() {
        let mut view = RunView::new();
        view.apply(&ev_at(EventKind::TaskStarted, 100, &[("task", s("ask"))]));
        view.apply(&ev_at(
            EventKind::TaskCompleted,
            900,
            &[("task", s("ask")), ("tokens", Value::Int(4))],
        ));
        assert!(view.rows()[0].meters().is_empty(), "no split, no meters");
    }
}
