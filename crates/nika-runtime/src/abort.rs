//! The wave boundary's abort — the budget's (NIKA-1704) and the operator's
//! (#1353): the unstarted tasks settle as cancelled, the run ends with its
//! terminal frame, the outcome says which.

use std::collections::BTreeMap;

use nika_dataflow::TaskRecord as DataflowTaskRecord;
use nika_event::EventKind;
use nika_schema::raw::RawWorkflow;

use nika_types::resource::Value as FieldValue;

use super::{EventSink, RunOutcome, Stamper, emit, i, ledger, record, s, terminal_cost_fields};

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
    let (kind, detail) = if operator {
        (
            EventKind::WorkflowCancelled,
            "cancelled by the operator — in-flight work completed and was counted · \
             unstarted tasks were cancelled"
                .to_owned(),
        )
    } else {
        (
            EventKind::WorkflowFailed,
            format!(
                "NIKA-1704 · run budget exceeded — spent ${:.6} of ${:.6} (--max-cost-usd) · \
                 in-flight work completed and was counted · unstarted tasks were cancelled",
                snapshot.spent_usd,
                snapshot.budget.unwrap_or(0.0),
            ),
        )
    };
    let mut fields = vec![("workflow", s(workflow_name)), ("detail", s(&detail))];
    fields.extend(terminal_cost_fields(snapshot));
    fields.extend(terminal_summary_fields(
        &records,
        if operator { "cancelled" } else { "failed" },
    ));
    emit(stamper, sink, kind, &fields);

    let mut outcome =
        RunOutcome::from_dataflow(false, records, BTreeMap::new()).with_ledger(snapshot);
    outcome.cache_hits = cache_hits;
    outcome.cancelled = operator;
    outcome
}

/// The run's summary on its terminal frame (#1247): `status` and the task
/// tally — total · ok · failed · recovered · skipped · cancelled — so a
/// machine consumer reads what the human card computes.
pub(super) fn terminal_summary_fields(
    records: &BTreeMap<String, DataflowTaskRecord>,
    status: &str,
) -> Vec<(&'static str, FieldValue)> {
    use nika_dataflow::{TaskStatus, TerminalCause};
    let count = |pick: &dyn Fn(&DataflowTaskRecord) -> bool| {
        i(i64::try_from(records.values().filter(|r| pick(r)).count()).unwrap_or(i64::MAX))
    };
    vec![
        ("status", s(status)),
        ("tasks_total", count(&|_| true)),
        ("tasks_ok", count(&|r| r.status == TaskStatus::Success)),
        ("tasks_failed", count(&|r| r.status == TaskStatus::Failure)),
        (
            "tasks_recovered",
            count(&|r| r.cause == TerminalCause::Recovered),
        ),
        ("tasks_skipped", count(&|r| r.status == TaskStatus::Skipped)),
        (
            "tasks_cancelled",
            count(&|r| r.status == TaskStatus::Cancelled),
        ),
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn record(
        status: nika_dataflow::TaskStatus,
        cause: nika_dataflow::TerminalCause,
    ) -> DataflowTaskRecord {
        DataflowTaskRecord::unran(status, cause)
    }

    /// The tally counts every record once by its status, and the recovered
    /// ones by their cause (a recovered task IS a success).
    #[test]
    fn the_summary_counts_the_records_by_status_and_recovery() {
        use nika_dataflow::{TaskStatus, TerminalCause};
        let mut records = BTreeMap::new();
        records.insert(
            "a".to_owned(),
            record(TaskStatus::Success, TerminalCause::Normal),
        );
        records.insert(
            "b".to_owned(),
            record(TaskStatus::Success, TerminalCause::Recovered),
        );
        records.insert(
            "c".to_owned(),
            record(TaskStatus::Failure, TerminalCause::VerbError),
        );
        records.insert(
            "d".to_owned(),
            record(TaskStatus::Skipped, TerminalCause::Gate),
        );
        records.insert(
            "e".to_owned(),
            record(TaskStatus::Cancelled, TerminalCause::Operator),
        );
        let fields = terminal_summary_fields(&records, "failed");
        let get = |key: &str| {
            fields
                .iter()
                .find(|(k, _)| *k == key)
                .map_or_else(|| panic!("{key} rides the frame"), |(_, v)| v.clone())
        };
        assert_eq!(get("status"), s("failed"));
        assert_eq!(get("tasks_total"), i(5));
        assert_eq!(get("tasks_ok"), i(2));
        assert_eq!(get("tasks_failed"), i(1));
        assert_eq!(get("tasks_recovered"), i(1));
        assert_eq!(get("tasks_skipped"), i(1));
        assert_eq!(get("tasks_cancelled"), i(1));
    }
}
