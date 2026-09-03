// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal machine settlement for a local `nika run --json` stream.
//!
//! Progress events, declared outputs, and proof material are separate planes:
//! this envelope closes the stream without pretending to be a runtime event.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

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

/// The failure a terminal `run_settled` frame carries (#1403): the
/// first failed task's code, message and id, so the LAST line a machine
/// reads is the whole verdict — a CI reader that does the natural thing
/// (`json.loads(last_line)`) learned « failed » and nothing else, the
/// cause living only on an earlier `task_failed` frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SettlementError {
    /// The refusal's code (`NIKA-BUILTIN-READ-001` · …).
    pub code: String,
    /// The refusal's message, as the task recorded it.
    pub message: String,
    /// The task that failed.
    pub task: String,
}

/// The first failed task's error with its task id, in the runtime's
/// stable record order — `None` when no task failed (a succeeded or
/// paused run · a run that never started a task).
#[must_use]
pub fn settlement_error(outcome: &nika_runtime::RunOutcome) -> Option<SettlementError> {
    outcome
        .records
        .iter()
        .find(|(_, record)| record.status == nika_runtime::TaskStatus::Failure)
        .and_then(|(task, record)| {
            record.error.as_ref().map(|e| SettlementError {
                code: e.code.clone(),
                message: e.message.clone(),
                task: task.clone(),
            })
        })
}

/// First failed task record, in the runtime's stable record order.
#[must_use]
pub fn first_failure(outcome: &nika_runtime::RunOutcome) -> Option<nika_runtime::TaskErrorRecord> {
    outcome
        .records
        .values()
        .find(|record| record.status == nika_runtime::TaskStatus::Failure)
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

#[derive(Serialize)]
struct RunSettlement<'a, T> {
    kind: &'static str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<nika_types::id::ExecutionId>,
    outputs: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<LocalRunReceipt<'a>>,
    /// The cause, when `status` is `failed` and a task failed (#1403).
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a SettlementError>,
    /// The lanes the run executed (One Door · wave 2b · the ONE lane-row
    /// shape) — the verdict frame answers « which path served ».
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
    status: &str,
    outputs: &T,
    receipt: Option<LocalRunReceipt<'_>>,
) -> std::io::Result<()> {
    write_run_settlement_bound(writer, status, outputs, None, receipt, None)
}

/// Append the terminal document of a FAILED run with its cause (#1403).
///
/// # Errors
/// Returns the writer or JSON serialization error without changing the run's
/// execution verdict.
pub fn write_failed_run_settlement<T: Serialize>(
    writer: &mut impl Write,
    outputs: &T,
    receipt: Option<LocalRunReceipt<'_>>,
    error: Option<&SettlementError>,
) -> std::io::Result<()> {
    write_run_settlement_bound(writer, "failed", outputs, None, receipt, error)
}

fn write_run_settlement_bound<T: Serialize>(
    writer: &mut impl Write,
    status: &str,
    outputs: &T,
    execution: Option<nika_types::id::ExecutionId>,
    receipt: Option<LocalRunReceipt<'_>>,
    error: Option<&SettlementError>,
) -> std::io::Result<()> {
    serde_json::to_writer(
        &mut *writer,
        &RunSettlement {
            kind: "run_settled",
            status,
            execution,
            outputs,
            receipt,
            error,
            access_plan: None,
        },
    )
    .map_err(std::io::Error::from)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// The `run_settled` status of a run the runtime returned (#1438): a pause
/// wins (the gate is unanswered), then the operator's cancellation (a
/// decision, never a failure), then the verdict.
#[must_use]
pub fn settled_status(outcome: &nika_runtime::RunOutcome) -> &'static str {
    if outcome.paused.is_some() {
        "paused"
    } else if outcome.cancelled {
        "cancelled"
    } else if outcome.ok {
        "succeeded"
    } else {
        "failed"
    }
}

/// Project the typed local execution identity into its terminal envelope.
///
/// # Errors
/// Returns the writer or serialization error.
pub fn write_local_run_settlement<T: Serialize, W: Write>(
    writer: &mut nika_dap::journal::JsonSink<W>,
    status: &str,
    outputs: &T,
    execution: nika_types::id::ExecutionId,
    snapshot_digest: &str,
    trace: Option<(&Path, &TraceProof)>,
    error: Option<&SettlementError>,
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
    writer.write_record(&RunSettlement {
        kind: "run_settled",
        status,
        execution: Some(execution),
        outputs,
        receipt,
        // A cause rides only a FAILED frame: a paused or succeeded run
        // has none to claim.
        error: error.filter(|_| status == "failed"),
        access_plan,
    })
}

/// The cause of a run the runtime REFUSED before any task (the launch
/// refusals the operator can fix: no access path · an unsatisfied pin ·
/// a missing input) — the verdict frame carries the code CI branches on
/// (wave 2b · the W1 gauntlet met a failed `run_settled` with no
/// `error`). `task` is `-`: no task ran.
#[must_use]
pub fn refusal_error(err: &nika_runtime::RuntimeError) -> SettlementError {
    SettlementError {
        code: err.spec_code(),
        message: err.to_string(),
        task: "-".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// #1403 — the last line a machine reads is the whole verdict: a
    /// failed settlement names the code, the message and the task; a
    /// succeeded one carries no `error` key at all.
    #[test]
    fn a_failed_settlement_carries_its_cause_and_a_clean_one_does_not() {
        let cause = SettlementError {
            code: "NIKA-BUILTIN-READ-001".to_owned(),
            message: "no such file: ./missing.md".to_owned(),
            task: "brief".to_owned(),
        };
        let mut out = Vec::new();
        write_failed_run_settlement(&mut out, &json!({"result": null}), None, Some(&cause))
            .expect("writes");
        let value: serde_json::Value = serde_json::from_slice(&out).expect("one document");
        assert_eq!(value["kind"], "run_settled");
        assert_eq!(value["status"], "failed");
        assert_eq!(value["error"]["code"], "NIKA-BUILTIN-READ-001");
        assert_eq!(value["error"]["message"], "no such file: ./missing.md");
        assert_eq!(value["error"]["task"], "brief");

        let mut clean = Vec::new();
        write_run_settlement(&mut clean, "succeeded", &json!({"result": 1}), None).expect("writes");
        let value: serde_json::Value = serde_json::from_slice(&clean).expect("one document");
        assert!(
            value.get("error").is_none(),
            "no cause on a clean run: {value}"
        );
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
        write_run_settlement(&mut out, "succeeded", &json!({"answer": 42}), Some(receipt))
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
        write_run_settlement(&mut out, "paused", &json!({}), None).expect("settlement writes");
        let value: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON");
        assert_eq!(value["status"], "paused");
        assert!(value.get("execution").is_none());
        assert!(value.get("receipt").is_none());
    }

    #[test]
    fn local_settlement_projects_execution_even_without_a_trace() {
        let mut out = Vec::new();
        let execution = nika_types::id::ExecutionId::nil();
        {
            let mut writer = nika_dap::journal::JsonSink::new(&mut out);
            write_local_run_settlement(
                &mut writer,
                "paused",
                &json!({}),
                execution,
                "snapshot",
                None,
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
        {
            let mut writer = nika_dap::journal::JsonSink::new(&mut out);
            write_local_run_settlement(
                &mut writer,
                "succeeded",
                &json!({}),
                execution,
                "snapshot",
                Some((Path::new(".nika/traces/exact.ndjson"), &proof)),
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

    /// #1438 · the operator's cancellation settles as `cancelled`, never as
    /// the failure it is not; a pause still wins.
    #[test]
    fn a_cancelled_outcome_settles_as_cancelled() {
        let mut outcome = nika_runtime::RunOutcome::new(
            false,
            std::collections::BTreeMap::new(),
            std::collections::BTreeMap::new(),
        );
        assert_eq!(settled_status(&outcome), "failed");
        outcome.cancelled = true;
        assert_eq!(settled_status(&outcome), "cancelled");
        outcome.cancelled = false;
        outcome.ok = true;
        assert_eq!(settled_status(&outcome), "succeeded");
    }
}
