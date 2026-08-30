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

#[derive(Serialize)]
struct RunSettlement<'a, T> {
    kind: &'static str,
    status: &'a str,
    outputs: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<LocalRunReceipt<'a>>,
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
    serde_json::to_writer(
        &mut *writer,
        &RunSettlement {
            kind: "run_settled",
            status,
            outputs,
            receipt,
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
pub fn write_local_run_settlement<T: Serialize>(
    writer: &mut impl Write,
    outcome: (bool, bool),
    outputs: &T,
    execution: nika_types::id::ExecutionId,
    snapshot_digest: &str,
    trace: Option<(&Path, &TraceProof)>,
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
    let status = if outcome.1 {
        "paused"
    } else if outcome.0 {
        "succeeded"
    } else {
        "failed"
    };
    write_run_settlement(writer, status, outputs, receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(value.get("receipt").is_none());
    }
}
