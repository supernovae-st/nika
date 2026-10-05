// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real Session → compiler → loopback: no outcome, plan or candidate injected into Session.
use super::*;
struct Work;
impl TurnClassifier for Work {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::NewWork, crate::turn::RoutingMethod::Model)
    }
}
fn live(root: &Path, home: &Path) -> SessionRuntime {
    let mut s = open(root);
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(Work));
    s.enable_history(home).unwrap();
    s
}
fn first_script() -> Vec<(u16, Value)> {
    let mut script: Vec<_> = semantic_create()
        .iter()
        .map(|s| (200, response(s)))
        .collect();
    script.push((
        400,
        json!({"error":{"message":"synthetic judge unavailable"}}),
    ));
    script
}
fn kept(s: &SessionRuntime) -> Value {
    assert!(s.pending_proposal().is_none());
    assert!(s.pending_question().is_none(), "no invented clarification");
    assert!(s.status_line().contains("Not ready"));
    s.authoring.as_ref().unwrap().continuation.clone().unwrap()
}
#[test]
fn same_intent_retries_only_the_judge_before_any_save_or_run() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    let plan = kept(&s);
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
    for line in ["save", "Save", "SAVE", "yes", "run"] {
        assert!(matches!(s.turn(line), TurnOutcome::Facts(_)));
        assert!(
            kept(&s) == plan,
            "Save and Run must preserve the complete unjudged plan"
        );
    }
    assert_eq!(peer.bodies().len(), CREATE_CALLS);
    let judge = Peer::start(vec![(200, response(JUDGE_APPROVES))]);
    let _again = test_transport::install(&judge.url);
    assert!(matches!(s.turn("RePrEnD"), TurnOutcome::Proposal { .. }));
    assert_eq!(judge.bodies().len(), 1, "no regenerated author calls");
    assert!(judge.bodies()[0].to_string().contains("unfaithful"));
    let source = s.candidate().unwrap().set.changes[0].content();
    assert_eq!(
        plan["final"]["candidate_sha256"],
        nika_event::source_id::sha256_hex(source.as_bytes())
    );
    assert!(!dir.path().join("compiled-workflow.nika").exists());
    assert!(!dir.path().join("sortie.txt").exists());
}
#[test]
fn close_restore_and_retry_preserve_exact_candidate_and_obligations() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let plan = {
        let mut s = live(dir.path(), home.path());
        assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
        kept(&s)
    };
    let mut s = live(dir.path(), home.path());
    assert!(s.pending_proposal().is_none());
    assert!(matches!(s.restore_round(), TurnOutcome::Facts(_)));
    assert!(
        kept(&s) == plan,
        "the exact record and obligations must survive close"
    );
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS,
        "restore never calls a model"
    );
    let judge = Peer::start(vec![(200, response(JUDGE_APPROVES))]);
    let _again = test_transport::install(&judge.url);
    assert!(matches!(s.turn("Réessaie"), TurnOutcome::Proposal { .. }));
    assert_eq!(judge.bodies().len(), 1);
    let source = s.candidate().unwrap().set.changes[0].content();
    assert_eq!(
        plan["final"]["candidate_sha256"],
        nika_event::source_id::sha256_hex(source.as_bytes())
    );
}
#[test]
fn a_correction_discards_the_old_candidate_and_authors_the_changed_intent() {
    let peer = Peer::start(first_script());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = live(dir.path(), home.path());
    assert!(matches!(s.turn(WORK), TurnOutcome::Facts(_)));
    let _old = kept(&s);
    let failed = Peer::start(vec![(
        400,
        json!({"error":{"message":"new author unavailable"}}),
    )]);
    let _new = test_transport::install(&failed.url);
    let out = s.turn("Utilise revised.txt au lieu de sortie.txt.");
    assert!(!matches!(
        out,
        TurnOutcome::Proposal { .. } | TurnOutcome::RunRequested { .. }
    ));
    assert!(!s.judgment_waits());
    assert!(s.pending_proposal().is_none());
    let bodies = failed.bodies();
    assert!(!bodies.is_empty());
    assert!(bodies[0].to_string().contains("revised.txt"));
    assert!(
        !bodies[0].to_string().contains("unfaithful"),
        "fresh author, not old judge"
    );
}
