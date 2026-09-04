//! The run's settlement, built ONCE from the records and the ledger at the
//! boundary that ends the run (ADR-128 · one door): the normal close, the
//! operator's cancellation, the budget stop, a human gate. The type and
//! its wire live in `nika-event` (`RunSettlement`); this module is the
//! only place that folds records into it. Every door downstream projects
//! the settlement — none refolds the task frames.

use std::collections::BTreeMap;

use nika_dataflow::{TaskRecord as DataflowTaskRecord, TaskStatus, TerminalCause};
use nika_event::settlement::{
    RunCause, RunSettlement, RunState, SettlementError, Spend, TaskTally,
};

use nika_schema::raw::RawWorkflow;
use nika_schema::types::OutputDecl;
use serde_json::Value;

use super::ledger::LedgerSnapshot;
use super::{EventSink, Stamper, emit_terminal, json_type_name, s};
use nika_schema::types::type_expr_display;

/// Fold the records and the ledger's terminal snapshot into the run's
/// settlement.
pub(crate) fn settle_run(
    records: &BTreeMap<String, DataflowTaskRecord>,
    snap: &LedgerSnapshot,
    state: RunState,
    cause: RunCause,
    error: Option<SettlementError>,
) -> RunSettlement {
    let mut settlement = RunSettlement::new(state, cause)
        .with_tasks(tally(records))
        .with_spend(spend(snap))
        .with_error(error);
    if let Some(elapsed) = snap.elapsed {
        settlement =
            settlement.with_elapsed_ms(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
    }
    settlement
}

/// Every record counted once by its status, the recovered ones by their
/// cause (a recovered task IS a success), the never-started ones (cancelled
/// at the boundary by the operator or the budget) apart from the
/// upstream-cancelled.
pub(crate) fn tally(records: &BTreeMap<String, DataflowTaskRecord>) -> TaskTally {
    let count = |pick: &dyn Fn(&DataflowTaskRecord) -> bool| {
        u32::try_from(records.values().filter(|r| pick(r)).count()).unwrap_or(u32::MAX)
    };
    let mut t = TaskTally::new();
    t.total = count(&|_| true);
    t.ok = count(&|r| r.status == TaskStatus::Success);
    t.failed = count(&|r| r.status == TaskStatus::Failure);
    t.recovered = count(&|r| r.cause == TerminalCause::Recovered);
    t.skipped = count(&|r| r.status == TaskStatus::Skipped);
    t.cancelled = count(&|r| r.status == TaskStatus::Cancelled);
    t.never_started = count(&|r| {
        r.status == TaskStatus::Cancelled
            && matches!(r.cause, TerminalCause::Operator | TerminalCause::Budget)
    });
    t
}

/// The spend as the ledger metered it — the total rides iff at least one
/// leaf METERED real spend (a mock/local run stays total-free: absent is
/// honest, a `0.0` nobody metered is not); the counts and the qualifier
/// ride always.
fn spend(snap: &LedgerSnapshot) -> Spend {
    let mut spend = Spend::new(
        snap.any_priced.then_some(snap.spent_usd),
        snap.priced_calls,
        snap.unpriced_calls,
    );
    if snap.any_priced {
        // The snapshot identity the total was priced against — the
        // point-in-time honesty stamp (prices move; the trace says WHICH
        // prices billed this run).
        spend = spend
            .with_pricing_as_of(nika_catalog::pricing_snapshot().as_of)
            .with_by_source(snap.by_source.clone());
    }
    spend
}

/// The first failed task's error with its task id, in the runtime's stable
/// record order — `None` when no task failed.
pub(crate) fn first_failure(
    records: &BTreeMap<String, DataflowTaskRecord>,
) -> Option<SettlementError> {
    records
        .iter()
        .find(|(_, r)| r.status == TaskStatus::Failure)
        .and_then(|(task, r)| {
            r.error.as_ref().map(|e| {
                SettlementError::new(e.code.clone(), e.message.clone(), Some(task.clone()))
            })
        })
}

/// Resolve the run's verdict over the typed `outputs:` and emit the terminal
/// frame. Returns the final `ok` (false if a typed output broke its declared
/// `type:` · spec 01 rule 6 · NIKA-VAR-009). The reason rides the
/// `WorkflowFailed` frame as a WORKFLOW-level `detail` (not a phantom
/// `task_failed`) — the event model stays consistent (no orphan task event, no
/// spurious row) and `--json`/journal consumers see a valid terminal with the
/// code. Split out of `run()` to keep that function within its line budget.
#[allow(clippy::too_many_arguments)] // the terminal frame's parts
pub(crate) fn finalize_outputs(
    wf: &RawWorkflow,
    outputs: &BTreeMap<String, Value>,
    records: &BTreeMap<String, DataflowTaskRecord>,
    workflow_name: &str,
    mut ok: bool,
    snapshot: &LedgerSnapshot,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> (bool, RunSettlement) {
    // A typed output is checked only when every task otherwise settled (a task
    // failure is already the verdict · the output reference is then omitted).
    let violation = if ok {
        first_output_type_violation(wf, outputs)
    } else {
        None
    };
    if violation.is_some() {
        ok = false;
    }
    let mut fields = vec![("workflow", s(workflow_name))];
    // The settlement (ADR-128): the state, WHY, and the failure named —
    // built here, once, for every door downstream.
    let (state, cause, error) = if let Some(v) = &violation {
        let detail = format!(
            "NIKA-VAR-009 · output `{}` is {}, declared type: {}",
            v.name, v.actual, v.expected
        );
        fields.push(("detail", s(&detail)));
        (
            RunState::Failed,
            RunCause::OutputContract,
            Some(SettlementError::new("NIKA-VAR-009", detail, None)),
        )
    } else if ok {
        (RunState::Succeeded, RunCause::Normal, None)
    } else {
        (
            RunState::Failed,
            RunCause::TaskFailed,
            first_failure(records),
        )
    };
    let settlement = settle_run(records, snapshot, state, cause, error);
    emit_terminal(stamper, sink, &fields, &settlement);
    (ok, settlement)
}

/// One typed-`outputs:` contract violation (spec 01 rule 6 · NIKA-VAR-009).
pub(crate) struct OutputTypeViolation {
    pub(crate) name: String,
    pub(crate) expected: String,
    pub(crate) actual: &'static str,
}

/// The FIRST typed `outputs:` value whose resolved JSON value does not
/// inhabit its declared `type:` (spec 01 §engine-MUST rule 6 · NIKA-VAR-009
/// · the output half of the callable contract). Post-R3b the declared
/// type speaks the full `TypeExpr` and the judgment is the ONE type core's
/// `fits` (lenient exactly where the core is: a whole float like `42.0`
/// inhabits `integer` · a genuine cross-type mismatch refuses). An output
/// whose `${{ }}` reference no longer resolves is OMITTED by
/// [`resolve_outputs`] (spec §3 · not a type error); a `type:` that does
/// not parse is the check's refusal (`NIKA-TYPE-001` · audit-before-run
/// keeps it out of the run), never the run's. `None` ⇒ every typed output
/// honours its declared type.
pub(crate) fn first_output_type_violation(
    wf: &RawWorkflow,
    resolved: &BTreeMap<String, Value>,
) -> Option<OutputTypeViolation> {
    let named = std::collections::BTreeMap::new();
    let type_names = std::collections::BTreeSet::new();
    for (key, decl) in &wf.outputs {
        let OutputDecl::Typed {
            r#type: Some(ty), ..
        } = decl
        else {
            continue; // untyped output OR no declared type → nothing to check
        };
        let Some(value) = resolved.get(key.value.as_str()) else {
            continue; // unresolved → omitted upstream, not a type error
        };
        let Ok(declared) = nika_types::types::parse_type(&ty.value, &type_names, "outputs") else {
            continue; // the check owns this refusal (NIKA-TYPE-001)
        };
        if !nika_types::types::fits(value, &declared, &named) {
            return Some(OutputTypeViolation {
                name: key.value.clone(),
                expected: type_expr_display(&ty.value),
                actual: json_type_name(value),
            });
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_event::settlement::CostQualifier;

    fn record(status: TaskStatus, cause: TerminalCause) -> DataflowTaskRecord {
        DataflowTaskRecord::unran(status, cause)
    }

    fn snap(any_priced: bool, priced: u32, unpriced: u32) -> LedgerSnapshot {
        LedgerSnapshot {
            spent_usd: if any_priced { 0.25 } else { 0.0 },
            any_priced,
            priced_calls: priced,
            unpriced_calls: unpriced,
            budget: None,
            by_source: BTreeMap::new(),
            elapsed: Some(std::time::Duration::from_millis(1500)),
        }
    }

    /// The tally counts every record once by its status, the recovered
    /// ones by their cause (a recovered task IS a success), and the
    /// never-started ones apart from the upstream-cancelled.
    #[test]
    fn the_tally_counts_the_records_by_status_recovery_and_start() {
        let mut records = BTreeMap::new();
        records.insert(
            "a".into(),
            record(TaskStatus::Success, TerminalCause::Normal),
        );
        records.insert(
            "b".into(),
            record(TaskStatus::Success, TerminalCause::Recovered),
        );
        records.insert(
            "c".into(),
            record(TaskStatus::Failure, TerminalCause::VerbError),
        );
        records.insert("d".into(), record(TaskStatus::Skipped, TerminalCause::Gate));
        records.insert(
            "e".into(),
            record(TaskStatus::Cancelled, TerminalCause::Operator),
        );
        records.insert(
            "f".into(),
            record(TaskStatus::Cancelled, TerminalCause::Upstream),
        );
        records.insert(
            "g".into(),
            record(TaskStatus::Cancelled, TerminalCause::Budget),
        );
        let t = tally(&records);
        assert_eq!(
            (
                t.total,
                t.ok,
                t.failed,
                t.recovered,
                t.skipped,
                t.cancelled,
                t.never_started
            ),
            (7, 2, 1, 1, 1, 3, 2)
        );
    }

    /// Nothing metered → no total, an honest qualifier; something priced →
    /// the total, the pricing stamp, the attribution.
    #[test]
    fn the_spend_never_states_a_total_nobody_metered() {
        let records = BTreeMap::new();
        let unmetered = settle_run(
            &records,
            &snap(false, 0, 0),
            RunState::Succeeded,
            RunCause::Normal,
            None,
        );
        assert_eq!(unmetered.spend.total_cost_usd, None);
        assert_eq!(unmetered.spend.qualifier, CostQualifier::Unmetered);
        assert_eq!(unmetered.spend.pricing_as_of, None);
        assert_eq!(unmetered.elapsed_ms, Some(1500));
        let mixed = settle_run(
            &records,
            &snap(true, 2, 1),
            RunState::Succeeded,
            RunCause::Normal,
            None,
        );
        assert_eq!(mixed.spend.total_cost_usd, Some(0.25));
        assert_eq!(mixed.spend.qualifier, CostQualifier::PartiallyPriced);
        assert!(
            mixed.spend.pricing_as_of.is_some(),
            "which prices billed this run"
        );
    }

    /// The first failure in record order names its task; a run with no
    /// failed task names none.
    #[test]
    fn the_first_failure_names_its_task_in_record_order() {
        let mut records = BTreeMap::new();
        records.insert(
            "a".into(),
            record(TaskStatus::Success, TerminalCause::Normal),
        );
        let mut failed = record(TaskStatus::Failure, TerminalCause::VerbError);
        failed.error = Some(nika_dataflow::TaskErrorRecord::new(
            "NIKA-EXEC-001",
            "boom",
            false,
        ));
        records.insert("b".into(), failed);
        let e = first_failure(&records).expect("a failed task");
        assert_eq!(
            (e.code.as_str(), e.task.as_deref()),
            ("NIKA-EXEC-001", Some("b"))
        );
        records.remove("b");
        assert!(first_failure(&records).is_none());
    }
}
