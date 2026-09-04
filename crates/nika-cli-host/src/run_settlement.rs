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

/// The `run_settled` envelope: the settlement flattened (`status` · `cause`
/// · `elapsed_ms` · `tasks` · `spend` · `error`), then the locators the
/// runtime does not know — the execution identity, the receipt, the lanes
/// that served (One Door · wave 2b · the ONE lane-row shape).
#[derive(Serialize)]
struct RunSettledFrame<'a, T> {
    kind: &'static str,
    #[serde(flatten)]
    settlement: &'a RunSettlement,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<nika_types::id::ExecutionId>,
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
    trace: Option<(&Path, &TraceProof)>,
    access_plan: Option<&[serde_json::Value]>,
) -> std::io::Result<()> {
    let execution_id = execution.to_string();
    let trace_id = nika_types::id::TraceId::from(execution).to_string();
    let trace_path = trace.map(|(path, _)| path.to_string_lossy());
    let receipt = trace.zip(trace_path.as_deref()).map(|((_, proof), path)| {
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
                None,
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
                Some((Path::new(".nika/traces/exact.ndjson"), &proof)),
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
