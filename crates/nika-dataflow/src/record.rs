// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`TaskRecord`] — the per-task result record (spec `04-variables.md`
//! §task output reference) + the canonical value→string rendering
//! (spec 04 §value rendering · compact JSON · **sorted keys**).
//!
//! Defined-null reads (04 §branch-join unlock): every field of a
//! TERMINAL task resolves — absent values are [`Value::Null`],
//! deterministically (a skipped task's output · a non-failure's error).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nika_types::timestamp::Timestamp;
use serde_json::Value;

/// A task's terminal status (spec 03 §task states · the v1 wire set has
/// four states; the Rust enum stays forward-compatible under FCI-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskStatus {
    /// The task completed successfully (incl. `on_error: recover`).
    Success,
    /// The task failed after retries with no recovery.
    Failure,
    /// The gate evaluated false · or `on_error: skip` fired.
    Skipped,
    /// The default gate became unsatisfiable (an upstream failure or
    /// cancellation) — a decision, not a defect (spec 03).
    Cancelled,
}

impl TaskStatus {
    /// The spec wire word (`tasks.X.status` comparisons · spec 04).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        }
    }
}

/// WHY a task settled — the second axis of the Outcome IR (spec 13 ·
/// `Outcome = TerminalClass × Cause × Payload`). Closed set: a new
/// cause is a spec amendment with its transition row, never a boolean
/// beside the record. A cause never influences an edge — gates read the
/// CLASS alone (the W2 pass-sets are untouched).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TerminalCause {
    /// success — the verb completed.
    Normal,
    /// success — the task failed, its `on_error: recover` chain settled
    /// it (the record keeps `recovered_from`, the original error).
    Recovered,
    /// failure — the verb refused/errored and no retry remained
    /// (attempts = 1 when no `retry:`).
    VerbError,
    /// failure — the task's `timeout:` budget elapsed (spec 03 ·
    /// `NIKA-TIMEOUT-001`).
    Timeout,
    /// failure — every attempt failed and the policy admitted no more.
    RetryExhausted,
    /// skipped — the `when:` gate (or the default gate's decision)
    /// evaluated false; a decision, not a defect (`.error` reads
    /// defined-`null`).
    Gate,
    /// skipped — `on_error: skip` fired; the PRESERVED error rides
    /// `.error` (spec 03 §fields).
    ErrorSkip,
    /// cancelled — the default gate became unsatisfiable (an upstream
    /// failure/cancellation propagated).
    Upstream,
    /// cancelled — the run was cancelled from outside (user signal ·
    /// `NIKA-CANCEL-001`). No runtime settle path produces this today
    /// (the engine has no operator-cancellation seam yet); the row is
    /// table-law so the cause exists the day the seam lands.
    Operator,
    /// cancelled — the run crossed `--max-cost-usd` and this task was
    /// UNSTARTED when the cap hit (spec 05 · `NIKA-RUN-1704`).
    Budget,
}

impl TerminalCause {
    /// The spec wire word (`tasks.X.cause` comparisons · spec 13).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Recovered => "recovered",
            Self::VerbError => "verb_error",
            Self::Timeout => "timeout",
            Self::RetryExhausted => "retry_exhausted",
            Self::Gate => "gate",
            Self::ErrorSkip => "error_skip",
            Self::Upstream => "upstream",
            Self::Operator => "operator",
            Self::Budget => "budget",
        }
    }
}

/// The spec-13 transition law — `true` iff `(class, cause)` is a row of
/// the normative table (`canon.yaml` `outcome_transitions.legal`). This
/// is the engine's ONE Rust copy of that table, parity-tested against
/// the vendored pack (`nika_pack::outcome_transitions()`); any pair
/// outside it is an engine bug, never a state.
#[must_use]
pub const fn legal(class: TaskStatus, cause: TerminalCause) -> bool {
    matches!(
        (class, cause),
        (
            TaskStatus::Success,
            TerminalCause::Normal | TerminalCause::Recovered
        ) | (
            TaskStatus::Failure,
            TerminalCause::VerbError | TerminalCause::Timeout | TerminalCause::RetryExhausted
        ) | (
            TaskStatus::Skipped,
            TerminalCause::Gate | TerminalCause::ErrorSkip
        ) | (
            TaskStatus::Cancelled,
            TerminalCause::Upstream | TerminalCause::Operator | TerminalCause::Budget
        )
    )
}

/// The wire code a task's ONE `timeout:` budget emits when it elapses
/// (spec 03 §timeout).
///
/// SPEC-PLANE code (the canon is the spec 05 table · resolvable via
/// `nika_pack::error_codes()` · NOT a `nika_error::codes` registry
/// entry — that registry carries the engine-internal NIKA-1700 range).
/// Pinned against the embedded canon by
/// `emitted_spec_codes_resolve_in_the_embedded_canon` (nika-runtime).
///
/// It lives HERE, beside [`failure_cause`] — the triage that reads it is
/// the record's own business — and `nika-runtime`'s dispatch re-exports it
/// at the historical `crate::task::TIMEOUT_CODE` path so the producer and
/// the classifier keep naming ONE constant.
pub const TIMEOUT_CODE: &str = "NIKA-TIMEOUT-001";

/// The failure-cause triage (spec 13 · the three failure rows):
/// the task's ONE `timeout:` budget elapsed → `Timeout` (the
/// `NIKA-TIMEOUT-001` wire code has exactly one producer — the budget
/// race — and wins even when retries had been scheduled before the
/// budget struck: the elapsed budget IS the settling fact) · more than
/// one attempt was made → `RetryExhausted` · else `VerbError`
/// (attempts = 1 · no retry was scheduled, whether no `retry:` was
/// declared or the policy refused the error).
#[must_use]
pub fn failure_cause(error: &TaskErrorRecord, attempts: u32) -> TerminalCause {
    if error.code == TIMEOUT_CODE {
        TerminalCause::Timeout
    } else if attempts > 1 {
        TerminalCause::RetryExhausted
    } else {
        TerminalCause::VerbError
    }
}

/// The typed error payload readable at `tasks.X.error` (spec 05
/// §error structure · v0 carries the load-bearing trio).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TaskErrorRecord {
    /// The canonical wire code (`NIKA-…` · verb-owned or spec class).
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Retry eligibility class (spec 05 · `error.transient`).
    pub transient: bool,
}

impl TaskErrorRecord {
    /// Construct the public error payload without relying on its evolvable
    /// field layout (FCI-002).
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>, transient: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            transient,
        }
    }

    /// The record as a JSON value (the `${{ tasks.X.error }}` shape).
    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "code": self.code,
            "message": self.message,
            "transient": self.transient,
        })
    }
}

/// One task's result record — the `tasks.<id>` namespace (spec 04 ·
/// a CEL object, never a bare scalar).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TaskRecord {
    /// Terminal status (closed enum · spec 03).
    pub status: TaskStatus,
    /// WHY the task settled (spec 13 · the Outcome IR's second axis).
    /// Always a legal `(status, cause)` row of the normative table —
    /// asserted at construction and pinned by the parity tests.
    pub cause: TerminalCause,
    /// The verb's output (`Null` on skipped/cancelled/failure ·
    /// defined-null per spec 04).
    pub output: Value,
    /// The coarse runtime integrity label of the output (F-O1 PR-1 ·
    /// RS-06's `Integ` axis — `Integrity::Trusted` by default, so
    /// records settled before the label existed read clean). ADDITIVE:
    /// no gate consumes it yet (PR-2 is the re-gate), and it is NOT part
    /// of the `tasks.<id>` expression namespace (the closed field set in
    /// [`Self::field`] is unchanged — a spec amendment would name it).
    pub integrity: nika_cap::Integrity,
    /// Present iff `status == failure` — PLUS the `on_error: skip`
    /// state where status is `skipped` AND the original error stays
    /// readable (spec 05 · the one coexist state).
    pub error: Option<TaskErrorRecord>,
    /// Attempts made, counting every attempt including the settling one
    /// (spec 13 §payload · ≥ 1). Present exactly on the classes whose
    /// payload law carries it (`success` · `failure`); `None` on
    /// skipped/cancelled — the payload table is the closed record shape.
    pub attempts: Option<u32>,
    /// The ORIGINAL error an `on_error: recover` repaired — present iff
    /// `cause == Recovered` (spec 13 §payload · `recovered_from`).
    pub recovered_from: Option<TaskErrorRecord>,
    /// The `TaskStarted` stamp (None when the task never ran).
    pub started_at: Option<Timestamp>,
    /// The terminal-event stamp (None when the task never ran).
    pub ended_at: Option<Timestamp>,
    /// Clock-measured execution time (None when the task never ran).
    pub duration_ms: Option<u64>,
    /// `output:` named bindings (spec 04 §Output binding) — `<name>` →
    /// the jq result over the raw output. Populated only on a SUCCESS
    /// (a skipped/cancelled/failed task's bindings read defined-null per
    /// spec 04 · so this stays empty and [`Self::field`] falls to null).
    /// Reserved-name collisions (`output`/`status`/…) are a parse error
    /// (the checker · spec 04 §rules), so a binding never shadows a
    /// record field here.
    pub named: BTreeMap<String, Value>,
}

impl TaskRecord {
    /// A record for a task that never ran (skipped gate · cancelled) —
    /// also the base every settle path fills in. The `(status, cause)`
    /// pair MUST be a row of the spec-13 table (an illegal pair is an
    /// engine bug, never a state — asserted here, at the one gate).
    #[must_use]
    pub fn unran(status: TaskStatus, cause: TerminalCause) -> Self {
        debug_assert!(
            legal(status, cause),
            "({}, {}) is outside the spec-13 transition table — an engine bug, never a state",
            status.as_str(),
            cause.as_str(),
        );
        Self {
            status,
            cause,
            output: Value::Null,
            integrity: nika_cap::Integrity::trusted(),
            error: None,
            attempts: None,
            recovered_from: None,
            started_at: None,
            ended_at: None,
            duration_ms: None,
            named: BTreeMap::new(),
        }
    }

    /// Resolve one record field by name — the closed reserved field set
    /// (spec 04 §result record) PLUS the task's `output:` named bindings
    /// (spec 04 §Output binding · `tasks.X.<name>`). `None` = neither a
    /// reserved field NOR a known binding (the reference form is out of
    /// the v0 subset).
    #[must_use]
    pub fn field(&self, name: &str) -> Option<Value> {
        Some(match name {
            "output" => self.output.clone(),
            "status" => Value::String(self.status.as_str().to_owned()),
            // Terminal-observation field (spec 13 §one obvious way ·
            // same pass-set as `.status`) — WHY the task settled.
            "cause" => Value::String(self.cause.as_str().to_owned()),
            // Defined-null · spec 04: absent values are null, never errors.
            "error" => self
                .error
                .as_ref()
                .map_or(Value::Null, TaskErrorRecord::to_value),
            "started_at" => stamp_value(self.started_at),
            "ended_at" => stamp_value(self.ended_at),
            "duration_ms" => self
                .duration_ms
                .map_or(Value::Null, |ms| Value::Number(ms.into())),
            // A declared `output:` binding (`tasks.X.<name>`). The map
            // carries an entry for EVERY declared binding once settled
            // (the evaluated value on success · `Null` on a non-success
            // task · the defined-null read · spec 04). A name absent from
            // the map is therefore neither reserved nor declared → None.
            other => return self.named.get(other).cloned(),
        })
    }
}

fn stamp_value(ts: Option<Timestamp>) -> Value {
    ts.map_or(Value::Null, |t| Value::String(t.to_string()))
}

/// The Outcome IR of a settled record as ONE compact JSON text — the
/// `outcome` field every terminal task event carries under
/// `trace_format: 2` (spec 13 §trace): `{class, cause, payload}`, the
/// payload per the normative table (`canon.yaml`
/// `outcome_transitions.payload`). Keys are sorted (`serde_json` object
/// = `BTreeMap`) — deterministic bytes, the event-stream contract.
#[must_use]
pub fn outcome_json(record: &TaskRecord) -> String {
    debug_assert!(
        legal(record.status, record.cause),
        "({}, {}) is outside the spec-13 transition table — an engine bug, never a state",
        record.status.as_str(),
        record.cause.as_str(),
    );
    let mut payload = serde_json::Map::new();
    match record.status {
        TaskStatus::Success => {
            payload.insert("value".to_owned(), record.output.clone());
            payload.insert("attempts".to_owned(), attempts_value(record));
            // Present iff cause = recovered (the payload law's one `?`
            // field on success) — the ORIGINAL error, whole.
            if let Some(original) = &record.recovered_from {
                payload.insert("recovered_from".to_owned(), original.to_value());
            }
        }
        TaskStatus::Failure => {
            if let Some(error) = &record.error {
                payload.insert("error".to_owned(), error.to_value());
            }
            payload.insert("attempts".to_owned(), attempts_value(record));
        }
        TaskStatus::Skipped => {
            // Present iff cause = error_skip (the PRESERVED error) ·
            // ABSENT under gate (the defined-null law — the judge
            // refuses a non-null error on a decision-skip).
            if let Some(error) = &record.error {
                payload.insert("error".to_owned(), error.to_value());
            }
        }
        TaskStatus::Cancelled => {
            // `reason` = the cause, spelled (spec 13 §payload).
            payload.insert(
                "reason".to_owned(),
                Value::String(record.cause.as_str().to_owned()),
            );
        }
    }
    serde_json::json!({
        "class": record.status.as_str(),
        "cause": record.cause.as_str(),
        "payload": payload,
    })
    .to_string()
}

/// The record's `attempts` as a payload value. The settle paths always
/// stamp it on success/failure records (the classes whose payload law
/// requires it) — the floor of 1 is the defensive total-function shape,
/// never expected to fire (the debug assert + the per-row tests pin it).
fn attempts_value(record: &TaskRecord) -> Value {
    debug_assert!(
        record.attempts.is_some(),
        "a {} record must carry attempts (spec 13 §payload)",
        record.status.as_str(),
    );
    Value::Number(record.attempts.unwrap_or(1).into())
}

/// Render a value into a string position (spec 04 §value rendering) ·
/// scalars natural (`null` → `null`) · objects/arrays compact JSON
/// with **sorted keys** (deterministic · no insignificant whitespace).
#[must_use]
pub fn render_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(_) | Value::Object(_) => {
            let mut out = String::new();
            write_canonical(value, &mut out);
            out
        }
    }
}

/// Compact JSON writer with recursively sorted object keys — the ONE
/// deterministic object→string form (spec 04 · "object keys sorted ·
/// no insignificant whitespace").
fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            out.push('{');
            let sorted: BTreeMap<&String, &Value> = map.iter().collect();
            for (i, (key, val)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // Key serialization via serde_json (escaping correctness).
                let _ = write!(out, "{}", Value::String((*key).clone()));
                out.push(':');
                write_canonical(val, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        // Scalars: serde_json's canonical token (strings escaped+quoted —
        // INSIDE a JSON document a string is quoted · only the top-level
        // string-into-string-position case renders raw, in render_value).
        scalar => {
            let _ = write!(out, "{scalar}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn status_wire_words_match_spec_enum() {
        // Spec 04 · closed enum · `success | failure | skipped | cancelled`.
        assert_eq!(TaskStatus::Success.as_str(), "success");
        assert_eq!(TaskStatus::Failure.as_str(), "failure");
        assert_eq!(TaskStatus::Skipped.as_str(), "skipped");
        assert_eq!(TaskStatus::Cancelled.as_str(), "cancelled");
    }

    /// Every cause, for the exhaustive sweeps below (defining-crate
    /// match — adding a variant breaks this until extended, the kind.rs
    /// ALL-list discipline).
    const ALL_CAUSES: [TerminalCause; 10] = [
        TerminalCause::Normal,
        TerminalCause::Recovered,
        TerminalCause::VerbError,
        TerminalCause::Timeout,
        TerminalCause::RetryExhausted,
        TerminalCause::Gate,
        TerminalCause::ErrorSkip,
        TerminalCause::Upstream,
        TerminalCause::Operator,
        TerminalCause::Budget,
    ];

    const ALL_STATUSES: [TaskStatus; 4] = [
        TaskStatus::Success,
        TaskStatus::Failure,
        TaskStatus::Skipped,
        TaskStatus::Cancelled,
    ];

    #[test]
    fn cause_wire_words_are_snake_case_and_serde_agrees() {
        // FCI-003 discipline: the wire slug has TWO encoders — the serde
        // derive and as_str(). They must agree forever.
        for cause in ALL_CAUSES {
            let json = serde_json::to_value(cause).expect("serializes");
            assert_eq!(
                json.as_str().expect("a JSON string"),
                cause.as_str(),
                "wire-slug divergence for {cause:?}"
            );
            let back: TerminalCause = serde_json::from_value(json).expect("round-trips");
            assert_eq!(back, cause);
        }
        assert_eq!(TerminalCause::VerbError.as_str(), "verb_error");
        assert_eq!(TerminalCause::RetryExhausted.as_str(), "retry_exhausted");
        assert_eq!(TerminalCause::ErrorSkip.as_str(), "error_skip");
    }

    #[test]
    fn legal_admits_exactly_the_ten_rows() {
        // Spec 13 · the table is exactly 10 rows; the 30-pair
        // complement refuses (the exhaustive 4×10 sweep).
        let admitted: usize = ALL_STATUSES
            .iter()
            .flat_map(|&s| ALL_CAUSES.iter().map(move |&c| legal(s, c)))
            .filter(|&ok| ok)
            .count();
        assert_eq!(admitted, 10, "exactly the ten normative rows admit");
        // Spot the shape per class.
        assert!(legal(TaskStatus::Success, TerminalCause::Recovered));
        assert!(legal(TaskStatus::Failure, TerminalCause::Timeout));
        assert!(legal(TaskStatus::Skipped, TerminalCause::ErrorSkip));
        assert!(legal(TaskStatus::Cancelled, TerminalCause::Operator));
        assert!(!legal(TaskStatus::Success, TerminalCause::Gate));
        assert!(!legal(TaskStatus::Cancelled, TerminalCause::Timeout));
    }

    #[test]
    fn failure_cause_triage_matches_spec_13() {
        let err = |code: &str| TaskErrorRecord::new(code, "x", false);
        // The budget's wire code wins — even when retries had fired.
        let timeout = err(TIMEOUT_CODE);
        assert_eq!(failure_cause(&timeout, 1), TerminalCause::Timeout);
        assert_eq!(failure_cause(&timeout, 3), TerminalCause::Timeout);
        // Attempts > 1 = the policy admitted retries and they exhausted.
        assert_eq!(
            failure_cause(&err("NIKA-EXEC-001"), 2),
            TerminalCause::RetryExhausted
        );
        // One attempt = the verb refused and no retry remained.
        assert_eq!(
            failure_cause(&err("NIKA-EXEC-001"), 1),
            TerminalCause::VerbError
        );
    }

    #[test]
    fn defined_null_reads_never_error() {
        // Spec 04 §branch-join unlock: every field of a terminal task
        // resolves · absent = null.
        let rec = TaskRecord::unran(TaskStatus::Skipped, TerminalCause::Gate);
        for field in ["output", "error", "started_at", "ended_at", "duration_ms"] {
            assert_eq!(
                rec.field(field).expect("closed field set resolves"),
                Value::Null,
                "{field} of a never-ran task must be defined-null"
            );
        }
        assert_eq!(
            rec.field("status").expect("status resolves"),
            Value::String("skipped".to_owned())
        );
        // Spec 13 · `.cause` resolves on EVERY terminal record — the
        // terminal-observation twin of `.status`.
        assert_eq!(
            rec.field("cause").expect("cause resolves"),
            Value::String("gate".to_owned())
        );
        assert!(rec.field("nope").is_none(), "unknown field = out of subset");
    }

    #[test]
    fn record_projection_preserves_non_null_timestamps_and_named_values() {
        let mut rec = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
        rec.started_at = Some(Timestamp::from_unix_ms(1_700_000_000_123));
        rec.ended_at = Some(Timestamp::from_unix_ms(1_700_000_000_456));
        rec.duration_ms = Some(333);
        rec.named.insert("answer".to_owned(), serde_json::json!(42));

        assert_eq!(
            rec.field("started_at").expect("reserved field"),
            Value::String("2023-11-14T22:13:20.123Z".to_owned())
        );
        assert_eq!(
            rec.field("ended_at").expect("reserved field"),
            Value::String("2023-11-14T22:13:20.456Z".to_owned())
        );
        assert_eq!(rec.field("duration_ms"), Some(serde_json::json!(333)));
        assert_eq!(rec.field("answer"), Some(serde_json::json!(42)));
    }

    #[test]
    fn error_record_coexists_with_skipped_status() {
        // Spec 05 · `on_error: skip` = the ONE state where status is
        // skipped AND the original error stays readable.
        let mut rec = TaskRecord::unran(TaskStatus::Skipped, TerminalCause::ErrorSkip);
        rec.error = Some(TaskErrorRecord {
            code: "NIKA-1402".to_owned(),
            message: "boom".to_owned(),
            transient: false,
        });
        let err = rec.field("error").expect("resolves");
        assert_eq!(err["code"], "NIKA-1402");
        assert_eq!(err["transient"], false);
    }

    #[test]
    fn outcome_json_carries_the_payload_per_class() {
        let parse = |rec: &TaskRecord| -> Value {
            serde_json::from_str(&outcome_json(rec)).expect("outcome is one JSON document")
        };
        let boom = TaskErrorRecord {
            code: "NIKA-EXEC-001".to_owned(),
            message: "boom".to_owned(),
            transient: false,
        };

        // success(normal) · value + attempts · NO recovered_from.
        let mut ok = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
        ok.output = serde_json::json!({"n": 1});
        ok.attempts = Some(1);
        let o = parse(&ok);
        assert_eq!(o["class"], "success");
        assert_eq!(o["cause"], "normal");
        assert_eq!(o["payload"]["value"], serde_json::json!({"n": 1}));
        assert_eq!(o["payload"]["attempts"], 1);
        assert!(o["payload"].get("recovered_from").is_none());

        // success(recovered) · recovered_from = the ORIGINAL error.
        let mut rec = TaskRecord::unran(TaskStatus::Success, TerminalCause::Recovered);
        rec.output = Value::String("fallback".to_owned());
        rec.attempts = Some(1);
        rec.recovered_from = Some(boom.clone());
        let o = parse(&rec);
        assert_eq!(o["payload"]["recovered_from"]["code"], "NIKA-EXEC-001");

        // failure · error + attempts.
        let mut failed = TaskRecord::unran(TaskStatus::Failure, TerminalCause::RetryExhausted);
        failed.error = Some(boom.clone());
        failed.attempts = Some(3);
        let o = parse(&failed);
        assert_eq!(o["payload"]["error"]["code"], "NIKA-EXEC-001");
        assert_eq!(o["payload"]["attempts"], 3);

        // skipped(gate) · the error key is ABSENT (defined-null law).
        let gate = TaskRecord::unran(TaskStatus::Skipped, TerminalCause::Gate);
        let o = parse(&gate);
        assert!(o["payload"].get("error").is_none());

        // skipped(error_skip) · the PRESERVED error rides.
        let mut skip = TaskRecord::unran(TaskStatus::Skipped, TerminalCause::ErrorSkip);
        skip.error = Some(boom);
        let o = parse(&skip);
        assert_eq!(o["payload"]["error"]["message"], "boom");

        // cancelled · reason = the cause, spelled.
        let upstream = TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Upstream);
        assert_eq!(parse(&upstream)["payload"]["reason"], "upstream");
        let budget = TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Budget);
        assert_eq!(parse(&budget)["payload"]["reason"], "budget");
        let operator = TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Operator);
        assert_eq!(parse(&operator)["payload"]["reason"], "operator");
    }

    #[test]
    fn render_value_scalars_are_natural() {
        // Spec 04 §value rendering · scalars natural · null → "null".
        assert_eq!(render_value(&Value::String("raw text".into())), "raw text");
        assert_eq!(render_value(&Value::Null), "null");
        assert_eq!(render_value(&serde_json::json!(true)), "true");
        assert_eq!(render_value(&serde_json::json!(42)), "42");
    }

    #[test]
    fn render_value_objects_sort_keys_recursively() {
        // Spec 04 · deterministic compact JSON · keys sorted at EVERY depth.
        let v = serde_json::json!({"z": {"b": 2, "a": 1}, "a": [3, {"y": 0, "x": 9}]});
        assert_eq!(
            render_value(&v),
            r#"{"a":[3,{"x":9,"y":0}],"z":{"a":1,"b":2}}"#
        );
    }

    #[test]
    fn render_value_strings_inside_json_stay_quoted_and_escaped() {
        let v = serde_json::json!({"k": "a\"b"});
        assert_eq!(render_value(&v), r#"{"k":"a\"b"}"#);
    }
}
