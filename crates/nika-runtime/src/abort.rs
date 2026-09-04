//! The wave boundary's abort — the budget's (NIKA-1704) and the operator's
//! (#1353): the unstarted tasks settle as cancelled, the run ends with its
//! terminal frame, the settlement says which (ADR-128).

use std::collections::BTreeMap;

use nika_dataflow::TaskRecord as DataflowTaskRecord;
use nika_event::EventKind;
use nika_event::settlement::{RunCause, RunState, SettlementError};
use nika_schema::raw::RawWorkflow;

use super::{EventSink, RunOutcome, Stamper, emit, emit_terminal, ledger, record, s, settlement};

/// Settle every unstarted task as cancelled (the operator's cancellation when
/// `operator`, the budget otherwise), end the run with its terminal frame and
/// return the outcome. In-flight work completed and was counted before this.
#[allow(clippy::too_many_arguments)] // the wave boundary's own knob set
pub(super) fn abort_unran(
    wf: &RawWorkflow,
    workflow_name: &str,
    mut records: BTreeMap<String, DataflowTaskRecord>,
    cache_hits: Vec<String>,
    snapshot: &ledger::LedgerSnapshot,
    operator: bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> RunOutcome {
    let (cause, note) = if operator {
        (
            nika_dataflow::TerminalCause::Operator,
            "cancelled by the operator",
        )
    } else {
        (
            nika_dataflow::TerminalCause::Budget,
            "budget · --max-cost-usd reached",
        )
    };
    for task in &wf.tasks {
        let id = &task.value.id.value;
        if !records.contains_key(id) {
            // Spec 13 · cancelled/budget: this task was UNSTARTED when
            // the boundary hit (in-flight work completed and was counted).
            let record = DataflowTaskRecord::unran(nika_dataflow::TaskStatus::Cancelled, cause);
            emit(
                stamper,
                sink,
                EventKind::TaskCancelled,
                &[
                    ("task", s(id)),
                    ("note", s(note)),
                    ("outcome", s(&record::outcome_json(&record))),
                ],
            );
            records.insert(id.clone(), record);
        }
    }
    let (state, run_cause, detail, error) = if operator {
        (
            RunState::Cancelled,
            RunCause::Operator,
            "cancelled by the operator — in-flight work completed and was counted · \
             unstarted tasks were cancelled"
                .to_owned(),
            None,
        )
    } else {
        let detail = format!(
            "NIKA-1704 · run budget exceeded — spent ${:.6} of ${:.6} (--max-cost-usd) · \
             in-flight work completed and was counted · unstarted tasks were cancelled",
            snapshot.spent_usd,
            snapshot.budget.unwrap_or(0.0),
        );
        let error = SettlementError::new("NIKA-1704", detail.clone(), None);
        (RunState::Failed, RunCause::Budget, detail, Some(error))
    };
    let settlement = settlement::settle_run(&records, snapshot, state, run_cause, error);
    emit_terminal(
        stamper,
        sink,
        &[("workflow", s(workflow_name)), ("detail", s(&detail))],
        &settlement,
    );

    let mut outcome =
        RunOutcome::from_dataflow(false, records, BTreeMap::new()).with_settlement(settlement);
    outcome.cache_hits = cache_hits;
    outcome
}
