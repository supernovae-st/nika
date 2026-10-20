// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Observed history can supply context for a fresh request, never restored Run authority.
use super::*;
use crate::run_view::KeptRun;

const RERUN: &str = "Relance le total, sans modifier le workflow.";

fn reopened() -> (tempfile::TempDir, tempfile::TempDir, SessionRuntime, Seen) {
    let (root, mut first, _) = session(WORKFLOW, TurnAct::RequestRun);
    let home = tempfile::tempdir().expect("home");
    first.last_workflow = None;
    first.intent.goal = None;
    first.enable_history(home.path()).expect("enable");
    assert!(matches!(
        first.natural_run_turn("Lance ./saved.nika tel quel."),
        Some(TurnOutcome::RunRequested { .. })
    ));
    let mut observed = KeptRun::new();
    observed.workflow_sha256 = Some(nika_event::source_id::sha256_hex(WORKFLOW.as_bytes()));
    let _ = first.observe_run_leg(0, None, observed);
    drop(first);
    let mut later = SessionRuntime::open(
        root.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    later.enable_history(home.path()).expect("reopen");
    let seen = Arc::new(Mutex::new(Vec::new()));
    later.with_classifier(Box::new(Reading {
        decision: TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model),
        seen: Arc::clone(&seen),
    }));
    (root, home, later, seen)
}

#[test]
fn a_reopened_run_without_save_supplies_context_only_for_a_fresh_rerun_request() {
    let (root, _home, mut runtime, seen) = reopened();
    assert!(runtime.last_workflow.is_none());
    assert!(runtime.consented.is_none());
    assert!(runtime.pending.is_none() && runtime.requested_run.is_none());
    assert!(seen.lock().expect("record").is_empty());
    assert!(!root.path().join("copy.txt").exists());
    let outcome = runtime.natural_run_turn(RERUN);
    let Some(TurnOutcome::RunRequested { run, .. }) = outcome else {
        panic!("the fresh request must admit the observed workflow: {outcome:?}");
    };
    assert_eq!(run.workflow, PathBuf::from("saved.nika"));
    assert_eq!(
        run.bytes.as_deref(),
        Some(&Witness::of(WORKFLOW.as_bytes()))
    );
    assert!(run.admits(WORKFLOW));
    assert!(run.vars.is_empty());
    assert!(runtime.consented.is_none() && runtime.pending.is_none());
    assert!(!root.path().join(".nika/consents.ndjson").exists());
    assert!(!root.path().join("copy.txt").exists());
    let calls = seen.lock().expect("record");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, RERUN);
    assert_eq!(calls[0].0.phase, SessionPhase::Idle);
    assert_eq!(
        calls[0].0.automation.as_deref(),
        Some(
            "The last observed run used the existing project workflow `saved.nika`; its current bytes match that observation. This is historical context, not a saved proposal or permission to run. Only a fresh request to run that unchanged workflow may reach Run admission."
        )
    );
}

#[test]
fn incomplete_escaping_or_changed_observations_do_not_supply_a_rerun_target() {
    for case in [
        "version",
        "missing_hash",
        "wrong_hash",
        "outside",
        "parent",
        "missing",
        "changed",
        "unsettled",
    ] {
        let (root, _home, mut runtime, seen) = reopened();
        let kept = runtime.kept_run.as_mut().expect("observed");
        match case {
            "version" => kept["version"] = serde_json::json!(2),
            "missing_hash" => {
                kept.as_object_mut()
                    .expect("object")
                    .remove("workflow_sha256");
            }
            "wrong_hash" => kept["workflow_sha256"] = serde_json::json!("unknown"),
            "outside" => kept["workflow"] = serde_json::json!("/outside/saved.nika"),
            "parent" => kept["workflow"] = serde_json::json!("../saved.nika"),
            "missing" => kept["workflow"] = serde_json::json!("missing.nika"),
            "changed" => std::fs::write(
                root.path().join("saved.nika"),
                WORKFLOW.replace("copy.txt", "other.txt"),
            )
            .expect("changed"),
            "unsettled" => kept["exit"] = serde_json::json!(4),
            _ => panic!("unexpected case"),
        }
        assert_eq!(runtime.natural_run_turn(RERUN), None, "{case}");
        assert!(seen.lock().expect("record").is_empty(), "{case}");
        assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
        assert!(runtime.requested_run.is_none() && runtime.run_inputs.is_none());
    }
}

#[test]
fn retained_run_context_never_promotes_modification_or_ambiguity_to_execution() {
    for act in [
        TurnAct::Modify,
        TurnAct::Mixed,
        TurnAct::Discuss,
        TurnAct::Cancel,
        TurnAct::Unknown,
        TurnAct::RequestRun,
    ] {
        let (_root, _home, mut runtime, seen) = reopened();
        let mut decision = TurnDecision::new(act, RoutingMethod::Model);
        if act == TurnAct::RequestRun {
            decision.secondary.push(TurnAct::Modify);
        }
        runtime.with_classifier(Box::new(Reading {
            decision,
            seen: Arc::clone(&seen),
        }));
        assert_eq!(
            runtime.natural_run_turn("Relance après avoir changé le total."),
            None
        );
        assert_eq!(seen.lock().expect("record").len(), 1);
        assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
        assert!(runtime.requested_run.is_none() && runtime.run_inputs.is_none());
    }
}

struct ChangesWhileReading(PathBuf);

impl TurnClassifier for ChangesWhileReading {
    fn classify(&mut self, _context: &TurnContext, _raw: &str) -> TurnDecision {
        std::fs::write(&self.0, WORKFLOW.replace("copy.txt", "other.txt")).expect("external edit");
        TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model)
    }
}

#[test]
fn admission_refuses_other_bytes_even_when_the_classifier_just_saw_the_observed_target() {
    let (root, _home, mut runtime, _) = reopened();
    runtime.with_classifier(Box::new(ChangesWhileReading(
        root.path().join("saved.nika"),
    )));
    let Some(TurnOutcome::Refusal(refusal)) = runtime.natural_run_turn(RERUN) else {
        panic!("admission must reject the changed checked bytes");
    };
    assert_eq!(refusal.class, RefusalClass::StaleRevision);
    assert!(refusal.text.contains("observed workflow changed"));
    assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
    assert!(runtime.requested_run.is_none() && runtime.run_inputs.is_none());
    assert!(!root.path().join("other.txt").exists());
}

#[cfg(unix)]
#[test]
fn a_linked_observed_workflow_never_supplies_a_target() {
    let (root, _home, mut runtime, seen) = reopened();
    std::fs::rename(
        root.path().join("saved.nika"),
        root.path().join("original.nika"),
    )
    .expect("move");
    std::os::unix::fs::symlink(
        root.path().join("original.nika"),
        root.path().join("saved.nika"),
    )
    .expect("link");
    assert_eq!(runtime.natural_run_turn(RERUN), None);
    assert!(seen.lock().expect("record").is_empty());
    assert!(runtime.requested_run.is_none());
}

#[test]
fn a_closed_run_it_line_uses_the_same_observed_bytes_without_a_classifier() {
    let (root, _home, mut runtime, seen) = reopened();
    assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
    let Some(TurnOutcome::RunRequested { run, .. }) = runtime.run_turn("run it") else {
        panic!("the closed fresh Run line can name the observed workflow");
    };
    assert_eq!(run.workflow, PathBuf::from("saved.nika"));
    assert_eq!(
        run.bytes.as_deref(),
        Some(&Witness::of(WORKFLOW.as_bytes()))
    );
    assert!(seen.lock().expect("record").is_empty());
    assert!(runtime.consented.is_none());
    assert!(!root.path().join("copy.txt").exists());
}

#[test]
fn a_closed_run_line_never_uses_changed_or_differently_named_observed_bytes() {
    for line in ["run it", "run missing.nika"] {
        let (root, _home, mut runtime, seen) = reopened();
        if line == "run it" {
            std::fs::write(
                root.path().join("saved.nika"),
                WORKFLOW.replace("copy.txt", "other.txt"),
            )
            .expect("changed");
        }
        assert!(matches!(
            runtime.run_turn(line),
            Some(TurnOutcome::Refusal(_))
        ));
        assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
        assert!(runtime.requested_run.is_none());
        assert!(seen.lock().expect("record").is_empty());
    }
}

#[test]
fn named_and_current_workflows_keep_precedence_over_observed_history() {
    for (line, expected, natural) in [
        ("Lance other.nika tel quel.", "other.nika", true),
        ("run other.nika", "other.nika", false),
        (RERUN, "current.nika", true),
        ("run it", "current.nika", false),
    ] {
        let (root, _home, mut runtime, _) = reopened();
        std::fs::write(root.path().join("current.nika"), WORKFLOW).expect("current");
        std::fs::write(root.path().join("other.nika"), WORKFLOW).expect("named");
        std::fs::write(root.path().join("saved.nika"), "not a workflow").expect("old changed");
        runtime.last_workflow = Some(PathBuf::from("current.nika"));
        let outcome = if natural {
            runtime.natural_run_turn(line)
        } else {
            runtime.run_turn(line)
        };
        let Some(TurnOutcome::RunRequested { run, .. }) = outcome else {
            panic!("the named/current workflow takes precedence: {outcome:?}");
        };
        assert_eq!(run.workflow, PathBuf::from(expected));
        assert!(run.admits(WORKFLOW));
    }
}

#[test]
fn a_nonplain_run_with_a_secondary_change_never_admits_the_observed_workflow() {
    let (root, _home, mut runtime, seen) = reopened();
    let mut decision = TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model);
    decision.secondary.push(TurnAct::Modify);
    runtime.with_classifier(Box::new(Reading {
        decision,
        seen: Arc::clone(&seen),
    }));
    assert_eq!(runtime.run_turn("run it after changing the output"), None);
    assert_eq!(seen.lock().expect("record").len(), 1);
    assert!(runtime.last_workflow.is_none() && runtime.consented.is_none());
    assert!(runtime.requested_run.is_none() && runtime.run_inputs.is_none());
    assert!(!root.path().join("copy.txt").exists());
}
