// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's settlement — ONE terminal truth, built once, read everywhere.
//!
//! The runtime folds a run's records and its ledger into a
//! [`RunSettlement`] exactly once, at the boundary that ends the run (the
//! normal close · the operator's cancellation · the budget stop · a human
//! gate). The settlement rides the terminal frame (`workflow_completed` ·
//! `workflow_failed` · `workflow_cancelled` · `workflow_paused`) as flat
//! fields ([`RunSettlement::fields`]) and is read back from that frame by
//! every door ([`RunSettlement::from_event`]): the CLI's `run_settled`
//! envelope, the resident's job status, `trace ls` and `trace outputs
//! --json`, the session, the SDK. No surface refolds the task frames to
//! learn what the run settled as (ADR-128 · one door).
//!
//! Vocabulary law: [`RunState`] owns the state words (`succeeded` ·
//! `failed` · `paused` · `cancelled`); [`EventKind`] owns the frame kinds
//! (`workflow_completed` …). A kind is an event name, never a state word;
//! the two are mapped in ONE place ([`RunState::terminal_kind`] ·
//! [`RunState::from_terminal_kind`]).
//!
//! Honesty laws carried here: `total_cost_usd` is absent when nothing was
//! metered (a `0.0` nobody metered is a lie · [`CostQualifier`] says what
//! the total covers); the task tally is absent on a frame that predates it
//! (an older journal) rather than zero; the cause is what the runtime knew
//! at the boundary, never a reader's inference.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nika_types::resource::{KeyValue, Value as FieldValue};

use crate::event::Event;
use crate::kind::EventKind;

/// The run's state at settlement — the ONE vocabulary every door speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum RunState {
    /// Every task settled and the typed outputs held (`workflow_completed`).
    Succeeded,
    /// A task failed, the budget stopped the run, an output broke its
    /// declared type, or the runtime refused before any task
    /// (`workflow_failed`).
    Failed,
    /// The run awaits a human's answer (`workflow_paused`) — an obligation,
    /// never a verdict; the same run resumes.
    Paused,
    /// The operator cancelled at a wave boundary (`workflow_cancelled`) — a
    /// decision, never a defect.
    Cancelled,
}

impl RunState {
    /// The state word (`succeeded` · `failed` · `paused` · `cancelled`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Paused => "paused",
            Self::Cancelled => "cancelled",
        }
    }

    /// The state a word names, if it is one of the four.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "paused" => Some(Self::Paused),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// The terminal frame kind this state closes the journal with.
    #[must_use]
    pub const fn terminal_kind(self) -> EventKind {
        match self {
            Self::Succeeded => EventKind::WorkflowCompleted,
            Self::Failed => EventKind::WorkflowFailed,
            Self::Paused => EventKind::WorkflowPaused,
            Self::Cancelled => EventKind::WorkflowCancelled,
        }
    }

    /// The state a terminal frame kind names; `None` for any other kind.
    #[must_use]
    pub const fn from_terminal_kind(kind: EventKind) -> Option<Self> {
        match kind {
            EventKind::WorkflowCompleted => Some(Self::Succeeded),
            EventKind::WorkflowFailed => Some(Self::Failed),
            EventKind::WorkflowPaused => Some(Self::Paused),
            EventKind::WorkflowCancelled => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the run can still move: a paused run resumes, the others
    /// are final.
    #[must_use]
    pub const fn is_final(self) -> bool {
        !matches!(self, Self::Paused)
    }
}

/// WHY the run settled as it did — what the runtime knew at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum RunCause {
    /// The run ran to its end.
    Normal,
    /// A blocking `nika:prompt` asked; the run paused for the answer.
    HumanGate,
    /// A task failed and no recovery settled it.
    TaskFailed,
    /// A typed `outputs:` value broke its declared type (NIKA-VAR-009).
    OutputContract,
    /// `--max-cost-usd` was crossed at a wave boundary (NIKA-1704); the
    /// unstarted tasks were cancelled.
    Budget,
    /// The operator cancelled at a wave boundary; in-flight work completed
    /// and was counted, the unstarted tasks were cancelled.
    Operator,
    /// The runtime refused the run before any task (no access path · an
    /// unsatisfied pin · a missing input).
    Refused,
}

impl RunCause {
    /// The cause word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::HumanGate => "human_gate",
            Self::TaskFailed => "task_failed",
            Self::OutputContract => "output_contract",
            Self::Budget => "budget",
            Self::Operator => "operator",
            Self::Refused => "refused",
        }
    }

    /// The cause a word names.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "normal" => Some(Self::Normal),
            "human_gate" => Some(Self::HumanGate),
            "task_failed" => Some(Self::TaskFailed),
            "output_contract" => Some(Self::OutputContract),
            "budget" => Some(Self::Budget),
            "operator" => Some(Self::Operator),
            "refused" => Some(Self::Refused),
            _ => None,
        }
    }

    /// The cause a state implies when a frame carries none (a journal
    /// written before the cause rode the frame): the least the state
    /// itself proves.
    #[must_use]
    pub const fn implied_by(state: RunState) -> Self {
        match state {
            RunState::Succeeded => Self::Normal,
            RunState::Failed => Self::TaskFailed,
            RunState::Paused => Self::HumanGate,
            RunState::Cancelled => Self::Operator,
        }
    }
}

/// What the spend total covers — unknown is never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum CostQualifier {
    /// Every metered leaf was priced: the total is the spend.
    Priced,
    /// Some leaves were priced, some were not: the total is a floor.
    PartiallyPriced,
    /// Every metered leaf was unpriced (local · mock · uncataloged ·
    /// provider silent): there is no total to state.
    Unpriced,
    /// No leaf metered anything (an exec-only run · a run that never
    /// reached a leaf).
    Unmetered,
}

impl CostQualifier {
    /// The qualifier word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Priced => "priced",
            Self::PartiallyPriced => "partially_priced",
            Self::Unpriced => "unpriced",
            Self::Unmetered => "unmetered",
        }
    }

    /// The qualifier a word names.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "priced" => Some(Self::Priced),
            "partially_priced" => Some(Self::PartiallyPriced),
            "unpriced" => Some(Self::Unpriced),
            "unmetered" => Some(Self::Unmetered),
            _ => None,
        }
    }

    /// The qualifier the two counts prove.
    #[must_use]
    pub const fn of(priced_calls: u32, unpriced_calls: u32) -> Self {
        match (priced_calls > 0, unpriced_calls > 0) {
            (true, false) => Self::Priced,
            (true, true) => Self::PartiallyPriced,
            (false, true) => Self::Unpriced,
            (false, false) => Self::Unmetered,
        }
    }
}

/// The task tally the settlement carries — every record counted once by
/// its status, the recovered ones by their cause, the never-started ones
/// (cancelled at the boundary by the operator or the budget) apart from
/// the upstream-cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct TaskTally {
    /// Every settled record.
    pub total: u32,
    /// Settled as a success (a recovered task IS a success).
    pub ok: u32,
    /// Settled as a failure.
    pub failed: u32,
    /// REPAIRS an `on_error` recovery made (the recovered rows are
    /// counted in `ok` too). A `for_each` row counts one per recovered
    /// ITEM, never one per row — the number a human card prints.
    pub recovered: u32,
    /// Skipped by a gate or `on_error: skip`.
    pub skipped: u32,
    /// Cancelled (upstream or at the boundary).
    pub cancelled: u32,
    /// Cancelled at the boundary without ever starting (counted in
    /// `cancelled` too).
    pub never_started: u32,
}

impl TaskTally {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total: 0,
            ok: 0,
            failed: 0,
            recovered: 0,
            skipped: 0,
            cancelled: 0,
            never_started: 0,
        }
    }
}

/// The run's spend as the ledger metered it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Spend {
    /// Σ of METERED spend · `None` when nothing was priced (absent is
    /// honest, a `0.0` nobody metered is not).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub total_cost_usd: Option<f64>,
    /// Leaf executions whose spend is in the total.
    pub priced_calls: u32,
    /// Leaf executions that carried an unpriced reason — spend NOT in the
    /// total.
    pub unpriced_calls: u32,
    /// What the total covers.
    pub qualifier: CostQualifier,
    /// The pricing snapshot the total was priced against (prices move; the
    /// settlement says WHICH prices billed this run).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub pricing_as_of: Option<String>,
    /// Spend per attribution key (`provider/model` · tool id), micro-USD
    /// rounded.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "BTreeMap::is_empty", default)
    )]
    pub by_source: BTreeMap<String, f64>,
}

impl Spend {
    /// Construct from the ledger's counts; the qualifier follows the counts.
    #[must_use]
    pub fn new(total_cost_usd: Option<f64>, priced_calls: u32, unpriced_calls: u32) -> Self {
        Self {
            total_cost_usd,
            priced_calls,
            unpriced_calls,
            qualifier: CostQualifier::of(priced_calls, unpriced_calls),
            pricing_as_of: None,
            by_source: BTreeMap::new(),
        }
    }

    /// Name the pricing snapshot the total was priced against.
    #[must_use]
    pub fn with_pricing_as_of(mut self, as_of: impl Into<String>) -> Self {
        self.pricing_as_of = Some(as_of.into());
        self
    }

    /// Attach the per-source attribution (micro-USD rounded here, once).
    #[must_use]
    pub fn with_by_source(mut self, by_source: BTreeMap<String, f64>) -> Self {
        self.by_source = by_source
            .into_iter()
            .map(|(k, v)| (k, (v * 1e6).round() / 1e6))
            .collect();
        self
    }
}

impl Default for Spend {
    fn default() -> Self {
        Self::new(None, 0, 0)
    }
}

/// The failure the settlement names (#1403): the first failed task's
/// code, message and id — or a run-level cause (the budget · an output
/// contract · a refusal) with no task.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct SettlementError {
    /// The refusal's code (`NIKA-BUILTIN-READ-001` · `NIKA-1704` · …).
    pub code: String,
    /// The refusal's message, as recorded.
    pub message: String,
    /// The task that failed · `None` for a run-level cause.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub task: Option<String>,
}

impl SettlementError {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>, task: Option<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            task,
        }
    }
}

/// The run's settlement — built once by the runtime, projected by every
/// door.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct RunSettlement {
    /// The state (the `status` word on every wire).
    #[cfg_attr(feature = "serde", serde(rename = "status"))]
    pub state: RunState,
    /// Why.
    pub cause: RunCause,
    /// The run's elapsed time on the kernel clock, when known.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub elapsed_ms: Option<u64>,
    /// The task tally · `None` on a frame that predates it (never zero).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub tasks: Option<TaskTally>,
    /// The spend.
    pub spend: Spend,
    /// The failure named, when there is one.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub error: Option<SettlementError>,
}

/// The flat field keys the settlement writes on a terminal frame.
mod key {
    pub(super) const STATUS: &str = "status";
    pub(super) const CAUSE: &str = "cause";
    pub(super) const ELAPSED_MS: &str = "elapsed_ms";
    pub(super) const TASKS_TOTAL: &str = "tasks_total";
    pub(super) const TASKS_OK: &str = "tasks_ok";
    pub(super) const TASKS_FAILED: &str = "tasks_failed";
    pub(super) const TASKS_RECOVERED: &str = "tasks_recovered";
    pub(super) const TASKS_SKIPPED: &str = "tasks_skipped";
    pub(super) const TASKS_CANCELLED: &str = "tasks_cancelled";
    pub(super) const TASKS_NEVER_STARTED: &str = "tasks_never_started";
    pub(super) const TOTAL_COST_USD: &str = "total_cost_usd";
    pub(super) const PRICED_CALLS: &str = "priced_calls";
    pub(super) const UNPRICED_CALLS: &str = "unpriced_calls";
    pub(super) const COST_QUALIFIER: &str = "cost_qualifier";
    pub(super) const PRICING_AS_OF: &str = "pricing_as_of";
    pub(super) const COST_BY_SOURCE: &str = "cost_by_source";
    pub(super) const ERROR_CODE: &str = "error_code";
    pub(super) const ERROR_MESSAGE: &str = "error_message";
    pub(super) const ERROR_TASK: &str = "error_task";
}

impl RunSettlement {
    /// Construct (INV-019): a state and its cause, nothing else known.
    #[must_use]
    pub fn new(state: RunState, cause: RunCause) -> Self {
        Self {
            state,
            cause,
            elapsed_ms: None,
            tasks: None,
            spend: Spend::default(),
            error: None,
        }
    }

    /// Stamp the elapsed time.
    #[must_use]
    pub const fn with_elapsed_ms(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_ms = Some(elapsed_ms);
        self
    }

    /// Carry the task tally.
    #[must_use]
    pub const fn with_tasks(mut self, tasks: TaskTally) -> Self {
        self.tasks = Some(tasks);
        self
    }

    /// Carry the spend.
    #[must_use]
    pub fn with_spend(mut self, spend: Spend) -> Self {
        self.spend = spend;
        self
    }

    /// Name the failure.
    #[must_use]
    pub fn with_error(mut self, error: Option<SettlementError>) -> Self {
        self.error = error;
        self
    }

    /// The frame kind this settlement closes the journal with.
    #[must_use]
    pub const fn terminal_kind(&self) -> EventKind {
        self.state.terminal_kind()
    }

    /// The settlement as the flat fields of its terminal frame — the ONE
    /// writer; [`Self::from_event`] is its inverse.
    #[must_use]
    pub fn fields(&self) -> Vec<KeyValue> {
        let mut out = vec![
            kv(key::STATUS, FieldValue::string(self.state.as_str())),
            kv(key::CAUSE, FieldValue::string(self.cause.as_str())),
        ];
        if let Some(ms) = self.elapsed_ms {
            out.push(kv(
                key::ELAPSED_MS,
                FieldValue::Int(i64::try_from(ms).unwrap_or(i64::MAX)),
            ));
        }
        if let Some(t) = &self.tasks {
            for (k, v) in [
                (key::TASKS_TOTAL, t.total),
                (key::TASKS_OK, t.ok),
                (key::TASKS_FAILED, t.failed),
                (key::TASKS_RECOVERED, t.recovered),
                (key::TASKS_SKIPPED, t.skipped),
                (key::TASKS_CANCELLED, t.cancelled),
                (key::TASKS_NEVER_STARTED, t.never_started),
            ] {
                out.push(kv(k, FieldValue::Int(i64::from(v))));
            }
        }
        // The totals ride iff something was priced (the no-fake-zero law);
        // the counts and the qualifier ride always.
        if let Some(total) = self.spend.total_cost_usd {
            out.push(kv(key::TOTAL_COST_USD, FieldValue::Float(total)));
        }
        if let Some(as_of) = &self.spend.pricing_as_of {
            out.push(kv(key::PRICING_AS_OF, FieldValue::string(as_of)));
        }
        if !self.spend.by_source.is_empty() {
            out.push(kv(
                key::COST_BY_SOURCE,
                FieldValue::string(json_object_of_floats(&self.spend.by_source)),
            ));
        }
        out.push(kv(
            key::PRICED_CALLS,
            FieldValue::Int(i64::from(self.spend.priced_calls)),
        ));
        out.push(kv(
            key::UNPRICED_CALLS,
            FieldValue::Int(i64::from(self.spend.unpriced_calls)),
        ));
        out.push(kv(
            key::COST_QUALIFIER,
            FieldValue::string(self.spend.qualifier.as_str()),
        ));
        if let Some(e) = &self.error {
            out.push(kv(key::ERROR_CODE, FieldValue::string(&e.code)));
            out.push(kv(key::ERROR_MESSAGE, FieldValue::string(&e.message)));
            if let Some(task) = &e.task {
                out.push(kv(key::ERROR_TASK, FieldValue::string(task)));
            }
        }
        out
    }

    /// Read the settlement back from a terminal frame — the ONE reader.
    /// `None` when the event is not a terminal frame. The kind names the
    /// state (a `status` word written by an older engine is ignored); the
    /// tally is `None` when the frame predates it; the cause falls back to
    /// what the state itself proves.
    #[must_use]
    pub fn from_event(event: &Event) -> Option<Self> {
        let state = RunState::from_terminal_kind(event.kind)?;
        let cause = event
            .str_field(key::CAUSE)
            .and_then(RunCause::parse)
            .unwrap_or_else(|| RunCause::implied_by(state));
        let mut settlement = Self::new(state, cause);
        settlement.elapsed_ms = event
            .int_field(key::ELAPSED_MS)
            .and_then(|ms| u64::try_from(ms).ok());
        settlement.tasks = event.int_field(key::TASKS_TOTAL).map(|total| {
            let count = |k: &str| u32::try_from(event.int_field(k).unwrap_or(0)).unwrap_or(0);
            let mut t = TaskTally::new();
            t.total = u32::try_from(total).unwrap_or(0);
            t.ok = count(key::TASKS_OK);
            t.failed = count(key::TASKS_FAILED);
            t.recovered = count(key::TASKS_RECOVERED);
            t.skipped = count(key::TASKS_SKIPPED);
            t.cancelled = count(key::TASKS_CANCELLED);
            t.never_started = count(key::TASKS_NEVER_STARTED);
            t
        });
        let count = |k: &str| u32::try_from(event.int_field(k).unwrap_or(0)).unwrap_or(0);
        let mut spend = Spend::new(
            event.float_field(key::TOTAL_COST_USD),
            count(key::PRICED_CALLS),
            count(key::UNPRICED_CALLS),
        );
        if let Some(as_of) = event.str_field(key::PRICING_AS_OF) {
            spend.pricing_as_of = Some(as_of.to_owned());
        }
        if let Some(text) = event.str_field(key::COST_BY_SOURCE) {
            spend.by_source = parse_json_object_of_floats(text);
        }
        settlement.spend = spend;
        settlement.error = event.str_field(key::ERROR_CODE).map(|code| {
            SettlementError::new(
                code,
                event.str_field(key::ERROR_MESSAGE).unwrap_or_default(),
                event.str_field(key::ERROR_TASK).map(str::to_owned),
            )
        });
        Some(settlement)
    }

    /// The last terminal frame of a journal, settled — `None` when the
    /// journal never reached one (a run in flight · a torn trace).
    #[must_use]
    pub fn from_events(events: &[Event]) -> Option<Self> {
        events.iter().rev().find_map(Self::from_event)
    }
}

fn kv(key: &'static str, value: FieldValue) -> KeyValue {
    KeyValue::new(key, value)
}

/// `{"key":0.001234,…}` — the attribution map as one JSON text on the frame
/// (keys are attribution ids: escaped for the two characters JSON needs).
fn json_object_of_floats(map: &BTreeMap<String, f64>) -> String {
    let mut out = String::from("{");
    for (i, (k, v)) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in k.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if c.is_control() => {
                    let _ = write!(out, "\\u{:04x}", u32::from(c));
                }
                c => out.push(c),
            }
        }
        out.push_str("\":");
        let _ = write!(out, "{v}");
    }
    out.push('}');
    out
}

/// The inverse of [`json_object_of_floats`] for the frames this engine
/// writes; anything else reads as empty (a reader never guesses).
fn parse_json_object_of_floats(text: &str) -> BTreeMap<String, f64> {
    #[cfg(feature = "serde")]
    {
        serde_json::from_str::<BTreeMap<String, f64>>(text).unwrap_or_default()
    }
    #[cfg(not(feature = "serde"))]
    {
        let _ = text;
        BTreeMap::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_types::id::EventId;
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    fn frame(kind: EventKind, fields: Vec<KeyValue>) -> Event {
        Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(7), kind).with_fields(fields)
    }

    #[test]
    fn the_state_words_roundtrip_and_map_to_their_kinds() {
        for state in [
            RunState::Succeeded,
            RunState::Failed,
            RunState::Paused,
            RunState::Cancelled,
        ] {
            assert_eq!(RunState::parse(state.as_str()), Some(state));
            assert_eq!(
                RunState::from_terminal_kind(state.terminal_kind()),
                Some(state)
            );
        }
        assert_eq!(RunState::parse("completed"), None, "a kind is not a state");
        assert_eq!(RunState::from_terminal_kind(EventKind::TaskCompleted), None);
        assert!(!RunState::Paused.is_final());
        assert!(RunState::Cancelled.is_final());
    }

    #[test]
    fn the_cause_and_qualifier_words_roundtrip() {
        for cause in [
            RunCause::Normal,
            RunCause::HumanGate,
            RunCause::TaskFailed,
            RunCause::OutputContract,
            RunCause::Budget,
            RunCause::Operator,
            RunCause::Refused,
        ] {
            assert_eq!(RunCause::parse(cause.as_str()), Some(cause));
        }
        for q in [
            CostQualifier::Priced,
            CostQualifier::PartiallyPriced,
            CostQualifier::Unpriced,
            CostQualifier::Unmetered,
        ] {
            assert_eq!(CostQualifier::parse(q.as_str()), Some(q));
        }
        assert_eq!(CostQualifier::of(3, 0), CostQualifier::Priced);
        assert_eq!(CostQualifier::of(1, 2), CostQualifier::PartiallyPriced);
        assert_eq!(CostQualifier::of(0, 2), CostQualifier::Unpriced);
        assert_eq!(CostQualifier::of(0, 0), CostQualifier::Unmetered);
    }

    #[test]
    fn the_frame_fields_roundtrip_through_the_one_reader() {
        let mut tally = TaskTally::new();
        tally.total = 5;
        tally.ok = 3;
        tally.failed = 1;
        tally.recovered = 1;
        tally.cancelled = 1;
        tally.never_started = 1;
        let mut by_source = BTreeMap::new();
        by_source.insert("mistral/mistral-small-latest".to_owned(), 0.012_345_678);
        let spend = Spend::new(Some(0.0123), 2, 1)
            .with_pricing_as_of("2026-09-01")
            .with_by_source(by_source);
        let settlement = RunSettlement::new(RunState::Failed, RunCause::TaskFailed)
            .with_elapsed_ms(1234)
            .with_tasks(tally)
            .with_spend(spend)
            .with_error(Some(SettlementError::new(
                "NIKA-EXEC-001",
                "boom",
                Some("b".to_owned()),
            )));
        let event = frame(settlement.terminal_kind(), settlement.fields());
        let back = RunSettlement::from_event(&event).expect("a terminal frame settles");
        assert_eq!(back, settlement);
        assert_eq!(back.spend.qualifier, CostQualifier::PartiallyPriced);
        assert_eq!(
            back.spend.by_source.get("mistral/mistral-small-latest"),
            Some(&0.012_346),
            "micro-USD rounding happens once, at attach"
        );
    }

    #[test]
    fn an_older_frame_settles_from_its_kind_with_no_tally() {
        // A journal written before the settlement rode the frame: `status`
        // carried the kind's word, no tally, no cause.
        let event = frame(
            EventKind::WorkflowCompleted,
            vec![kv("status", FieldValue::string("completed"))],
        );
        let back = RunSettlement::from_event(&event).expect("terminal");
        assert_eq!(back.state, RunState::Succeeded, "the kind names the state");
        assert_eq!(back.cause, RunCause::Normal, "the least the state proves");
        assert_eq!(back.tasks, None, "absent, never zero");
        assert_eq!(back.spend.qualifier, CostQualifier::Unmetered);
        assert_eq!(back.spend.total_cost_usd, None, "no fake zero");
    }

    #[test]
    fn a_non_terminal_frame_never_settles_and_a_journal_settles_on_its_last_terminal() {
        let started = frame(EventKind::WorkflowStarted, vec![]);
        assert_eq!(RunSettlement::from_event(&started), None);
        let paused = frame(EventKind::WorkflowPaused, vec![]);
        let done = frame(EventKind::WorkflowCompleted, vec![]);
        let journal = vec![started, paused, done];
        let back = RunSettlement::from_events(&journal).expect("the last terminal");
        assert_eq!(back.state, RunState::Succeeded);
        assert_eq!(RunSettlement::from_events(&[]), None);
    }

    #[test]
    fn a_run_level_error_carries_no_task() {
        let settlement = RunSettlement::new(RunState::Failed, RunCause::Budget).with_error(Some(
            SettlementError::new("NIKA-1704", "run budget exceeded", None),
        ));
        let event = frame(settlement.terminal_kind(), settlement.fields());
        let back = RunSettlement::from_event(&event).expect("terminal");
        assert_eq!(back.error.as_ref().and_then(|e| e.task.clone()), None);
        assert_eq!(back.cause, RunCause::Budget);
    }

    #[test]
    fn the_attribution_text_escapes_what_json_needs() {
        let mut map = BTreeMap::new();
        map.insert("a\"b\\c".to_owned(), 1.5);
        let text = json_object_of_floats(&map);
        assert_eq!(text, "{\"a\\\"b\\\\c\":1.5}");
        assert_eq!(parse_json_object_of_floats(&text), map);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_envelope_spells_status_and_skips_what_is_unknown() {
        let settlement = RunSettlement::new(RunState::Cancelled, RunCause::Operator);
        let value = serde_json::to_value(&settlement).expect("serialize");
        assert_eq!(value["status"], "cancelled");
        assert_eq!(value["cause"], "operator");
        assert!(value.get("tasks").is_none(), "absent, never zero");
        assert!(value.get("elapsed_ms").is_none());
        assert_eq!(value["spend"]["qualifier"], "unmetered");
        assert!(value["spend"].get("total_cost_usd").is_none());
        let back: RunSettlement = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, settlement);
    }
}
