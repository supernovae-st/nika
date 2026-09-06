// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal machine settlement for a local `nika run --json` stream.
//!
//! Progress events, declared outputs, and proof material are separate planes:
//! this envelope closes the stream without pretending to be a runtime event.
//! Its verdict IS the runtime's settlement (ADR-128): the `status`, the
//! `cause`, the task tally, the spend and the failure named ride the
//! envelope flattened, exactly as the journal's terminal frame carries them
//! — the CLI projects the settlement, it never refolds the run.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

pub use nika_event::settlement::{RunCause, RunSettlement, RunState, SettlementError};

/// Finalized journal facts carried out of the trace writer.
#[derive(Debug)]
pub struct TraceProof {
    pub head: String,
    pub len: usize,
    pub sealed: bool,
}

/// Whether the journal disclosure must name sensitive plaintext.
#[must_use]
pub fn sensitive_journey(report: &nika_check::CheckReport) -> bool {
    !matches!(
        report.data_journey.classification,
        nika_check::DataClassification::Internal
    )
}

/// The failure the run settled with, when any (#1403): the runtime named
/// it once, at the boundary — the first failed task's code, message and
/// id, or a run-level cause with no task.
#[must_use]
pub fn settlement_error(outcome: &nika_runtime::RunOutcome) -> Option<SettlementError> {
    outcome.settlement.error.clone()
}

/// The failed task's full record (the human card's autopsy line) when the
/// settlement names a task; `None` for a run-level cause or a clean run.
#[must_use]
pub fn first_failure(outcome: &nika_runtime::RunOutcome) -> Option<nika_runtime::TaskErrorRecord> {
    let task = outcome.settlement.error.as_ref()?.task.as_deref()?;
    outcome
        .records
        .get(task)
        .and_then(|record| record.error.clone())
}

/// Resolve checked wave indices into user-authored task identities.
#[must_use]
pub fn plan_waves(
    workflow: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
) -> Vec<Vec<String>> {
    report
        .waves
        .iter()
        .map(|wave| {
            wave.iter()
                .filter_map(|&index| {
                    workflow
                        .tasks
                        .get(index)
                        .map(|task| task.value.id.value.clone())
                })
                .collect()
        })
        .collect()
}

/// Proof locator minted only after the local trace is sealed and durable.
#[derive(Debug, Serialize)]
pub struct LocalRunReceipt<'a> {
    receipt_format: u8,
    execution_id: &'a str,
    trace_id: &'a str,
    snapshot_digest: &'a str,
    trace_path: &'a str,
    chain_head: &'a str,
    chain_len: usize,
    sealed: bool,
}

impl<'a> LocalRunReceipt<'a> {
    /// Bind a local trace locator to the exact admitted byte-world.
    #[must_use]
    pub const fn new(
        execution_id: &'a str,
        trace_id: &'a str,
        snapshot_digest: &'a str,
        trace_path: &'a str,
        chain_head: &'a str,
        chain_len: usize,
        sealed: bool,
    ) -> Self {
        Self {
            receipt_format: 1,
            execution_id,
            trace_id,
            snapshot_digest,
            trace_path,
            chain_head,
            chain_len,
            sealed,
        }
    }
}

/// Claims known before sealing and therefore attributable inside the seal.
#[must_use]
pub fn local_receipt_binding(
    execution: nika_types::id::ExecutionId,
    snapshot_digest: &str,
) -> serde_json::Value {
    serde_json::json!({
        "receipt_format": 1,
        "execution_id": execution.to_string(),
        "trace_id": nika_types::id::TraceId::from(execution).to_string(),
        "snapshot_digest": snapshot_digest,
    })
}

/// What the run left behind as evidence on the local door (ADR-129 · the
/// freeze audit): run state ≠ evidence state, and the evidence is SAID on
/// the settlement, never implied by an absent receipt.
#[derive(Debug, Clone, Copy)]
pub enum LocalEvidence<'a> {
    /// The journal at `path`, with its proof (sealed or not).
    Journal {
        /// The journal's path, as the receipt names it.
        path: &'a Path,
        /// The chain head, its length, whether the seal landed.
        proof: &'a TraceProof,
    },
    /// A journal was opened and its writer lane died after the run's
    /// effects: the run settled, the evidence is incomplete.
    Lost,
    /// No journal: the run opted out of the trace.
    None,
}

impl<'a> LocalEvidence<'a> {
    /// From the trace surface the run left: a path with its proof is the
    /// journal; a lane that died, or a path with no proof, is lost; nothing
    /// is none.
    #[must_use]
    pub const fn of(path: Option<&'a Path>, proof: Option<&'a TraceProof>, lost: bool) -> Self {
        match (path, proof) {
            (Some(path), Some(proof)) if !lost => Self::Journal { path, proof },
            (None, None) if !lost => Self::None,
            _ => Self::Lost,
        }
    }

    /// The word on the wire: `sealed` · `unsealed` · `lost` · `none`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Journal { proof, .. } if proof.sealed => "sealed",
            Self::Journal { .. } => "unsealed",
            Self::Lost => "lost",
            Self::None => "none",
        }
    }
}

/// The `run_settled` envelope: the settlement flattened (`status` · `cause`
/// · `elapsed_ms` · `tasks` · `spend` · `error`), then the locators the
/// runtime does not know — the execution identity, the evidence word, the
/// receipt, the lanes that served (One Door · wave 2b · the ONE lane-row
/// shape).
#[derive(Serialize)]
struct RunSettledFrame<'a, T> {
    kind: &'static str,
    #[serde(flatten)]
    settlement: &'a RunSettlement,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<nika_types::id::ExecutionId>,
    /// ADR-129 · what the run left as evidence (`sealed` · `unsealed` ·
    /// `lost` · `none`) — said, never implied by an absent receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<&'static str>,
    outputs: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<LocalRunReceipt<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_plan: Option<&'a [serde_json::Value]>,
}

/// Append exactly one terminal NDJSON document to a machine stream.
///
/// # Errors
/// Returns the writer or JSON serialization error without changing the run's
/// execution verdict.
pub fn write_run_settlement<T: Serialize>(
    writer: &mut impl Write,
    settlement: &RunSettlement,
    outputs: &T,
    receipt: Option<LocalRunReceipt<'_>>,
) -> std::io::Result<()> {
    serde_json::to_writer(
        &mut *writer,
        &RunSettledFrame {
            kind: "run_settled",
            settlement,
            execution: None,
            evidence: None,
            outputs,
            receipt,
            access_plan: None,
        },
    )
    .map_err(std::io::Error::from)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Project the typed local execution identity into its terminal envelope.
///
/// # Errors
/// Returns the writer or serialization error.
pub fn write_local_run_settlement<T: Serialize, W: Write>(
    writer: &mut nika_dap::journal::JsonSink<W>,
    settlement: &RunSettlement,
    outputs: &T,
    execution: nika_types::id::ExecutionId,
    snapshot_digest: &str,
    evidence: LocalEvidence<'_>,
    access_plan: Option<&[serde_json::Value]>,
) -> std::io::Result<()> {
    let execution_id = execution.to_string();
    let trace_id = nika_types::id::TraceId::from(execution).to_string();
    let journal = match evidence {
        LocalEvidence::Journal { path, proof } => Some((path.to_string_lossy(), proof)),
        LocalEvidence::Lost | LocalEvidence::None => None,
    };
    let receipt = journal.as_ref().map(|(path, proof)| {
        LocalRunReceipt::new(
            &execution_id,
            &trace_id,
            snapshot_digest,
            path,
            &proof.head,
            proof.len,
            proof.sealed,
        )
    });
    writer.write_record(&RunSettledFrame {
        kind: "run_settled",
        settlement,
        execution: Some(execution),
        evidence: Some(evidence.word()),
        outputs,
        receipt,
        access_plan,
    })
}

/// The settlement of a run the runtime REFUSED before any task (the launch
/// refusals the operator can fix: no access path · an unsatisfied pin · a
/// missing input): `failed` · `refused` · the code CI branches on (wave 2b
/// · the W1 gauntlet met a failed `run_settled` with no `error`). No task
/// ran, so the error names none.
#[must_use]
pub fn refusal_settlement(err: &nika_runtime::RuntimeError) -> RunSettlement {
    RunSettlement::new(RunState::Failed, RunCause::Refused).with_error(Some(SettlementError::new(
        err.spec_code(),
        err.to_string(),
        None,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// #1403 — the last line a machine reads is the whole verdict: a
    /// failed settlement names the code, the message and the task; a
    /// succeeded one carries no `error` key at all. ADR-128: the cause
    /// rides beside the status.
    #[test]
    fn a_failed_settlement_carries_its_cause_and_a_clean_one_does_not() {
        let failed = RunSettlement::new(RunState::Failed, RunCause::TaskFailed).with_error(Some(
            SettlementError::new(
                "NIKA-BUILTIN-READ-001",
                "no such file: ./missing.md",
                Some("brief".to_owned()),
            ),
        ));
        let mut out = Vec::new();
        write_run_settlement(&mut out, &failed, &json!({"result": null}), None).expect("writes");
        let value: serde_json::Value = serde_json::from_slice(&out).expect("one document");
        assert_eq!(value["kind"], "run_settled");
        assert_eq!(value["status"], "failed");
        assert_eq!(value["cause"], "task_failed");
        assert_eq!(value["error"]["code"], "NIKA-BUILTIN-READ-001");
        assert_eq!(value["error"]["message"], "no such file: ./missing.md");
        assert_eq!(value["error"]["task"], "brief");

        let clean = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        let mut out = Vec::new();
        write_run_settlement(&mut out, &clean, &json!({"result": 1}), None).expect("writes");
        let value: serde_json::Value = serde_json::from_slice(&out).expect("one document");
        assert_eq!(value["status"], "succeeded");
        assert!(
            value.get("error").is_none(),
            "no cause on a clean run: {value}"
        );
        assert_eq!(value["spend"]["qualifier"], "unmetered");
    }

    #[test]
    fn settlement_keeps_result_and_proof_planes_distinct() {
        let mut out = Vec::new();
        let receipt = LocalRunReceipt::new(
            "exe-1",
            "trace-1",
            "abc",
            ".nika/traces/exact.ndjson",
            "head",
            7,
            true,
        );
        let settlement = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        write_run_settlement(&mut out, &settlement, &json!({"answer": 42}), Some(receipt))
            .expect("settlement writes");
        let value: serde_json::Value =
            serde_json::from_slice(&out).expect("settlement is one JSON document");
        assert_eq!(value["kind"], "run_settled");
        assert!(value.get("execution").is_none());
        assert_eq!(value["outputs"], json!({"answer": 42}));
        assert_eq!(value["receipt"]["receipt_format"], 1);
        assert_eq!(value["receipt"]["chain_len"], 7);
        assert_eq!(out.last(), Some(&b'\n'));
    }

    #[test]
    fn trace_opt_out_never_invents_a_receipt() {
        let mut out = Vec::new();
        let paused = RunSettlement::new(RunState::Paused, RunCause::HumanGate);
        write_run_settlement(&mut out, &paused, &json!({}), None).expect("settlement writes");
        let value: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON");
        assert_eq!(value["status"], "paused");
        assert_eq!(value["cause"], "human_gate");
        assert!(value.get("execution").is_none());
        assert!(value.get("receipt").is_none());
    }

    #[test]
    fn local_settlement_projects_execution_even_without_a_trace() {
        let mut out = Vec::new();
        let execution = nika_types::id::ExecutionId::nil();
        let paused = RunSettlement::new(RunState::Paused, RunCause::HumanGate);
        {
            let mut writer = nika_dap::journal::JsonSink::new(&mut out);
            write_local_run_settlement(
                &mut writer,
                &paused,
                &json!({}),
                execution,
                "snapshot",
                LocalEvidence::None,
                None,
            )
            .expect("settlement writes");
        }
        let value: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON");
        assert_eq!(
            value["execution"],
            serde_json::to_value(execution).expect("execution serializes")
        );
        assert!(value.get("receipt").is_none());
        assert_eq!(value["evidence"], "none", "the opt-out is said");
    }

    /// ADR-129 · the freeze audit · a journal whose writer lane died after
    /// the run's effects: the run settled (its own state), the evidence is
    /// LOST — said on the frame, never implied by the absent receipt.
    #[test]
    fn a_lost_journal_is_said_never_implied() {
        let mut out = Vec::new();
        let execution = nika_types::id::ExecutionId::nil();
        let settlement = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        {
            let mut writer = nika_dap::journal::JsonSink::new(&mut out);
            write_local_run_settlement(
                &mut writer,
                &settlement,
                &json!({}),
                execution,
                "snapshot",
                LocalEvidence::of(Some(Path::new(".nika/traces/torn.ndjson")), None, true),
                None,
            )
            .expect("settlement writes");
        }
        let value: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON");
        assert_eq!(value["status"], "succeeded", "the run's own state");
        assert_eq!(value["evidence"], "lost", "{value}");
        assert!(
            value.get("receipt").is_none(),
            "no receipt for lost evidence"
        );
        let proof = TraceProof {
            head: "head".to_owned(),
            len: 1,
            sealed: false,
        };
        assert_eq!(
            LocalEvidence::of(Some(Path::new("j")), Some(&proof), false).word(),
            "unsealed"
        );
        assert_eq!(LocalEvidence::of(None, None, false).word(), "none");
        assert_eq!(LocalEvidence::of(None, None, true).word(), "lost");
    }

    #[test]
    fn journal_size_refusal_keeps_exact_outputs_and_successful_settlement() {
        use nika_dap::journal::{JsonSink, Tee, TraceFileSink};
        use nika_runtime::EventSink;

        let tmp = tempfile::tempdir().expect("tempdir");
        let outputs = json!({"answer": "🦋".repeat(nika_dap::chain::MAX_LINE_BYTES / 4 + 1)});
        let output_text = serde_json::to_string(&outputs).expect("output JSON");
        let event = nika_event::Event::new(
            nika_types::id::EventId::nil(),
            nika_types::timestamp::Timestamp::from_unix_ms(0),
            nika_event::EventKind::WorkflowCompleted,
        )
        .with_field(nika_types::resource::KeyValue::new(
            "output",
            nika_types::resource::Value::string(&output_text),
        ));
        let mut expected_primary = Vec::new();
        JsonSink::new(&mut expected_primary).emit(event.clone());
        let mut out = Vec::new();
        let mut tee = Tee::new(JsonSink::new(&mut out), TraceFileSink::new(tmp.path()));
        tee.emit(event.clone());
        let (mut primary, mut trace) = tee.into_parts();
        trace.finalize();
        let path = trace.path().expect("file was opened").to_path_buf();
        let failed = trace.into_error().is_some();
        assert!(failed, "the file journal refused the oversized event");
        assert_eq!(std::fs::metadata(&path).expect("journal").len(), 0);
        assert!(primary.error().is_none(), "the primary lane continues");
        let settlement = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        write_local_run_settlement(
            &mut primary,
            &settlement,
            &outputs,
            nika_types::id::ExecutionId::nil(),
            "snapshot",
            LocalEvidence::of(Some(&path), None, failed),
            None,
        )
        .expect("settlement still writes");
        assert!(primary.into_error().is_none());
        assert!(
            out.starts_with(&expected_primary),
            "the primary event is byte-identical"
        );
        let raw = std::str::from_utf8(&out).expect("UTF-8");
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 2, "one event and one settlement");
        let recovered: nika_event::Event = serde_json::from_str(lines[0]).expect("event");
        assert_eq!(recovered, event, "the output event is complete");
        let frame: serde_json::Value = serde_json::from_str(lines[1]).expect("settlement");
        assert_eq!(frame["kind"], "run_settled");
        assert_eq!(frame["status"], "succeeded");
        assert_eq!(frame["cause"], "normal");
        assert_eq!(frame["outputs"], outputs, "the exact result survives");
        assert_eq!(frame["evidence"], "lost");
        assert!(frame.get("receipt").is_none());
    }

    #[test]
    fn local_settlement_projects_the_same_execution_with_a_trace() {
        let mut out = Vec::new();
        let execution = nika_types::id::ExecutionId::nil();
        let proof = TraceProof {
            head: "head".to_owned(),
            len: 3,
            sealed: true,
        };
        let settlement = RunSettlement::new(RunState::Succeeded, RunCause::Normal);
        {
            let mut writer = nika_dap::journal::JsonSink::new(&mut out);
            write_local_run_settlement(
                &mut writer,
                &settlement,
                &json!({}),
                execution,
                "snapshot",
                LocalEvidence::Journal {
                    path: Path::new(".nika/traces/exact.ndjson"),
                    proof: &proof,
                },
                None,
            )
            .expect("settlement writes");
        }
        let value: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON");
        assert_eq!(
            value["execution"],
            serde_json::to_value(execution).expect("execution serializes")
        );
        assert_eq!(value["receipt"]["execution_id"], execution.to_string());
        assert_eq!(value["evidence"], "sealed", "{value}");
    }

    #[test]
    fn pre_seal_binding_carries_every_admission_identity_claim() {
        let execution = nika_types::id::ExecutionId::nil();
        let binding = local_receipt_binding(execution, "snapshot-abc");
        assert_eq!(binding["receipt_format"], 1);
        assert_eq!(binding["execution_id"], execution.to_string());
        assert_eq!(
            binding["trace_id"],
            nika_types::id::TraceId::from(execution).to_string()
        );
        assert_eq!(binding["snapshot_digest"], "snapshot-abc");
    }

    /// #1438 · ADR-128 · the outcome's booleans are projections of the ONE
    /// settlement the runtime built: a cancellation settles as `cancelled`
    /// (never the failure it is not), a pause keeps `ok`, and the failure
    /// named on the settlement is what `settlement_error` returns.
    #[test]
    fn the_outcome_projects_its_settlement() {
        let outcome = nika_runtime::RunOutcome::new(
            false,
            std::collections::BTreeMap::new(),
            std::collections::BTreeMap::new(),
        );
        assert_eq!(outcome.settlement.state, RunState::Failed);
        let cancelled =
            outcome.with_settlement(RunSettlement::new(RunState::Cancelled, RunCause::Operator));
        assert!(cancelled.cancelled && !cancelled.ok);
        assert_eq!(cancelled.settlement.state.as_str(), "cancelled");
        let paused =
            cancelled.with_settlement(RunSettlement::new(RunState::Paused, RunCause::HumanGate));
        assert!(
            paused.ok && !paused.cancelled,
            "a pause is a decision point, not a failure"
        );
        let budget = paused.with_settlement(
            RunSettlement::new(RunState::Failed, RunCause::Budget).with_error(Some(
                SettlementError::new("NIKA-1704", "run budget exceeded", None),
            )),
        );
        assert!(budget.budget_exceeded);
        let error = settlement_error(&budget).expect("the budget stop is named");
        assert_eq!((error.code.as_str(), error.task), ("NIKA-1704", None));
        assert!(
            first_failure(&budget).is_none(),
            "no task record for a run-level cause"
        );
    }
}
