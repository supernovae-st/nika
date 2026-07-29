// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's teardown facts (F-P2 · LOT-1) — the seal's extended
//! `covers` inputs, folded from what the run honestly knows (split out
//! of `run/mod.rs` at the 1500-line wall).

use nika_check::CheckReport;
use nika_runtime::RunOutcome;
use nika_schema::raw::RawWorkflow;

/// The teardown facts with the F-P14 debt attended FIRST (NEP-0014):
/// on the failure lane the quarantine effect RUNS (the moves happen
/// before the seal) and its fold rides; everywhere else `attend` is a
/// no-op `None` and the key stays OUT (absent is honest). The one-call
/// composition keeps the three surface lanes under the fn-length
/// ratchet.
pub(super) fn attended_facts(
    wf: &RawWorkflow,
    report: &CheckReport,
    outcome: &RunOutcome,
    journal: Option<&std::path::Path>,
) -> nika_dap::seal::SealTeardown {
    teardown_fold(
        wf,
        report,
        outcome,
        super::quarantine::attend(wf, outcome, journal),
    )
}

/// The run's teardown facts for the seal's extended `covers` (F-P2 ·
/// LOT-1): the receipt inputs (proves · the check certificate · each
/// `assert:` judged at its honest level · the outcome word), the budgets
/// ρ consumed against the certificate's ceiling, the effects ε
/// exercised against the declared bound, and the failed run's
/// quarantine fold (F-P14 · pre-shaped by the `quarantine` module on
/// the failure lane — `None` everywhere else, a clean run attests
/// nothing). The seal folds only what the run honestly knows — an
/// unprojectable workflow keeps the receipt digest out (absent is
/// honest); the seal attests WHAT HAPPENED, it never promises the
/// future.
fn teardown_fold(
    wf: &RawWorkflow,
    report: &CheckReport,
    outcome: &RunOutcome,
    quarantine: Option<serde_json::Value>,
) -> nika_dap::seal::SealTeardown {
    let mut teardown = nika_dap::seal::SealTeardown::new();
    teardown.proves = nika_runtime::proof::ir::semantic_ir_hash(wf).map(|h| h.as_hex().to_owned());
    teardown.certificate = serde_json::to_value(&report.certificate).ok();
    // The judged-assertions fold mirrors the evidence pack's receipt
    // (`level(true)` — the journal IS the trace).
    teardown.assertions = wf
        .assert
        .iter()
        .map(|sp| {
            let property = sp.value.clone();
            let level = property.level(true);
            serde_json::json!({
                "assert": property.name(),
                "level": level.as_str(),
            })
        })
        .collect();
    teardown.outcome = Some(
        if outcome.paused.is_some() {
            "paused"
        } else if outcome.ok {
            "completed"
        } else {
            "failed"
        }
        .to_owned(),
    );
    // Budgets ρ — consumed vs the certificate's ceiling: `spent_usd`
    // rides ONLY when metered (the no-fake-zero law the terminal frame
    // already keeps); the ceiling is the certificate's parametric bound,
    // absent when any spender is unpriceable.
    let mut budgets = serde_json::Map::new();
    if let Some(spent) = outcome.total_cost_usd {
        budgets.insert("spent_usd".to_owned(), serde_json::json!(spent));
    }
    budgets.insert("priced_calls".to_owned(), outcome.priced_calls.into());
    budgets.insert("unpriced_calls".to_owned(), outcome.unpriced_calls.into());
    budgets.insert("budget_exceeded".to_owned(), outcome.budget_exceeded.into());
    if let Some(ceiling) = &report.certificate.usd_micros
        && let Ok(value) = serde_json::to_value(ceiling)
    {
        budgets.insert("ceiling".to_owned(), value);
    }
    teardown.budgets = Some(serde_json::Value::Object(budgets));
    // Effects ε — exercised vs declared: `exercised` counts the effect
    // tasks' settled ATTEMPTS (exec · invoke — the task-attempt grain:
    // a fan-out's elements fold into their task's attempts, a traverse
    // fetch's pages stay the bound's grain), `declared` is the
    // certificate's static effect-call bound, `escapes` the checker's
    // static escape count. Facts next to the ceiling — never a judgment.
    let exercised: u64 = wf
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.value.action,
                nika_schema::raw::RawAction::Exec(_) | nika_schema::raw::RawAction::Invoke(_)
            )
        })
        .map(|task| {
            outcome
                .records
                .get(task.value.id.value.as_str())
                .and_then(|record| record.attempts)
                .unwrap_or(0)
        })
        .map(u64::from)
        .sum();
    let mut effects = serde_json::Map::new();
    effects.insert("exercised".to_owned(), exercised.into());
    if let Ok(declared) = serde_json::to_value(&report.certificate.effect_calls) {
        effects.insert("declared".to_owned(), declared);
    }
    effects.insert(
        "escapes".to_owned(),
        u64::try_from(report.certificate.effects.escapes)
            .unwrap_or(u64::MAX)
            .into(),
    );
    teardown.effects = Some(serde_json::Value::Object(effects));
    // F-P14 · la dette du run: the failure lane's quarantine fold rides
    // verbatim (`None` keeps the key OUT — absent is honest).
    teardown.quarantine = quarantine;
    teardown
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use nika_runtime::{TaskRecord, TaskStatus, TerminalCause};

    use super::*;

    fn parsed(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// ρ · the no-fake-zero law: an unmetered run carries NO `spent_usd`
    /// key (absent is honest — a `0.0` nobody metered is not), while the
    /// call counters and the outcome word always ride.
    #[test]
    fn budgets_omit_spent_when_nothing_was_metered() {
        let wf = parsed(
            "nika: v1\nworkflow:\n  id: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_check::check(&wf);
        let outcome = RunOutcome::new(true, BTreeMap::new(), BTreeMap::new());
        let td = teardown_fold(&wf, &report, &outcome, None);
        let budgets = td.budgets.expect("the budgets fold rides");
        assert!(
            budgets.get("spent_usd").is_none(),
            "no-fake-zero: {budgets}"
        );
        assert_eq!(budgets["priced_calls"], 0);
        assert_eq!(budgets["budget_exceeded"], false);
        assert_eq!(td.outcome.as_deref(), Some("completed"));
    }

    /// ε · the attempt grain counts EFFECT tasks only (exec · invoke):
    /// an infer task's attempts never inflate `exercised` — and a
    /// metered spend DOES surface as `spent_usd`.
    #[test]
    fn effects_count_effect_task_attempts_only() {
        let wf = parsed(
            "nika: v1\nworkflow:\n  id: grain\nmodel: mock/echo\npermits: { exec: [\"echo\"] }\ntasks:\n  e:\n    exec: { command: [\"echo\", \"hi\"] }\n  i:\n    infer: { prompt: \"x\" }\n",
        );
        let report = nika_check::check(&wf);
        let mut records = BTreeMap::new();
        let mut exec_rec = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
        exec_rec.attempts = Some(3);
        records.insert("e".to_owned(), exec_rec);
        let mut infer_rec = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
        infer_rec.attempts = Some(5);
        records.insert("i".to_owned(), infer_rec);
        let mut outcome = RunOutcome::new(true, records, BTreeMap::new());
        outcome.total_cost_usd = Some(0.5);
        outcome.priced_calls = 1;
        let td = teardown_fold(&wf, &report, &outcome, None);
        let effects = td.effects.expect("the effects fold rides");
        assert_eq!(
            effects["exercised"], 3,
            "the infer attempts stay out of ε: {effects}"
        );
        let budgets = td.budgets.expect("the budgets fold rides");
        assert_eq!(budgets["spent_usd"], 0.5, "metered spend surfaces");
    }

    /// F-P14 · la dette du run: the failure lane's quarantine fold rides
    /// the teardown VERBATIM (the seal's `extend_covers` places it under
    /// `covers["quarantine"]` — the teardown only carries it).
    #[test]
    fn the_failure_lanes_quarantine_fold_rides_the_teardown() {
        let wf = parsed(
            "nika: v1\nworkflow:\n  id: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_check::check(&wf);
        let outcome = RunOutcome::new(false, BTreeMap::new(), BTreeMap::new());
        let fold = serde_json::json!({
            "dir": ".nika/quarantine/2026-07-29T13-40-01Z-a3f2",
            "outputs": [{ "path": "out.txt", "quarantined_to": ".nika/quarantine/2026-07-29T13-40-01Z-a3f2/out.txt" }],
        });
        let td = teardown_fold(&wf, &report, &outcome, Some(fold.clone()));
        assert_eq!(
            td.quarantine.as_ref(),
            Some(&fold),
            "the fold reaches the seal untouched"
        );
        assert_eq!(td.outcome.as_deref(), Some("failed"));
    }

    /// The no-fake-zero posture, F-P14 side: a clean run passes `None`
    /// and the teardown carries NO quarantine — the key stays OUT of
    /// the covers (a clean run attests nothing).
    #[test]
    fn a_clean_run_carries_no_quarantine() {
        let wf = parsed(
            "nika: v1\nworkflow:\n  id: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_check::check(&wf);
        let outcome = RunOutcome::new(true, BTreeMap::new(), BTreeMap::new());
        let td = teardown_fold(&wf, &report, &outcome, None);
        assert!(td.quarantine.is_none(), "absent is honest");
    }
}
