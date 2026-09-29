// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The served teardown fold ([`super::SealTeardown::served`]) against a real
//! parsed workflow, its check report and typed settlements.

#![allow(clippy::expect_used)]

use nika_event::settlement::{RunState, Spend};

use super::{RunCause, RunSettlement, SealTeardown, workflow_hash};

/// C6 · a served teardown folds exactly what a service boundary carries:
/// the settlement's budgets (no fabricated `spent_usd` when nothing was
/// metered, none at all without a settlement), the caller's outcome word
/// and SDK binding, no memory fold without a store, no redacted keys.
#[test]
fn a_served_teardown_folds_the_settlement_and_keeps_redacted_keys_out() {
    let source = "nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let root = tempfile::tempdir().expect("an empty served root");
    let binding = serde_json::json!({"receipt_format": 1, "execution_id": "e"});
    let budgets = |mut expected: serde_json::Value| {
        if let Some(ceiling) = &report.certificate.usd_micros {
            expected["ceiling"] = serde_json::to_value(ceiling).expect("ceiling");
        }
        Some(expected)
    };
    let stopped = RunSettlement::new(RunState::Failed, RunCause::Budget).with_spend(Spend::new(
        Some(0.25),
        2,
        1,
    ));
    let fold = |outcome, settlement| {
        SealTeardown::served(
            &wf,
            &report,
            outcome,
            settlement,
            binding.clone(),
            root.path(),
        )
    };
    let teardown = fold("failed", Some(&stopped));
    let proves = nika_runtime::proof::ir::semantic_ir_hash(&wf).map(|h| h.as_hex().to_owned());
    assert!(proves.is_some());
    assert_eq!(teardown.proves, proves);
    assert_eq!(
        teardown.certificate,
        serde_json::to_value(&report.certificate).ok()
    );
    assert_eq!(teardown.outcome.as_deref(), Some("failed"));
    let spent = serde_json::json!({"spent_usd": 0.25, "priced_calls": 2, "unpriced_calls": 1, "budget_exceeded": true});
    assert_eq!(teardown.budgets, budgets(spent));
    assert_eq!(teardown.sdk_receipt.as_ref(), Some(&binding));
    assert!(teardown.memory.is_none() && teardown.memory_rejected.is_empty());
    assert!(teardown.effects.is_none() && teardown.quarantine.is_none());
    assert!(teardown.assertions.is_empty());
    let unmetered = RunSettlement::new(RunState::Succeeded, RunCause::Normal)
        .with_spend(Spend::new(None, 0, 0));
    let clean =
        serde_json::json!({"priced_calls": 0, "unpriced_calls": 0, "budget_exceeded": false});
    assert_eq!(fold("completed", Some(&unmetered)).budgets, budgets(clean));
    assert_eq!(fold("paused", None).budgets, None);
}

/// C6 · the seal's workflow hash is the per-task Merkle root of the parsed
/// workflow, and another workflow seals under another hash.
#[test]
fn the_seal_hash_is_the_workflows_per_task_merkle_root() {
    let parse = |source: &str| {
        nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    };
    let wf = parse("nika: w\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n");
    let root = nika_runtime::proof::ir::merkle_by_task(&wf).expect("a task tree has a root");
    assert_eq!(workflow_hash(&wf).as_deref(), Some(root.workflow.as_hex()));
    let other = parse("nika: w\ntasks:\n  b:\n    exec: { command: [\"echo\", \"ho\"] }\n");
    assert_ne!(workflow_hash(&other), workflow_hash(&wf));
}
