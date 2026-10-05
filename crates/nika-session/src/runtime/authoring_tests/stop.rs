// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Stop at the real proposal boundary, and after return but before host presentation.
use super::*;
use std::sync::Arc;

#[test]
fn a_ready_reading_loses_to_stop_before_it_can_commit_a_proposal() {
    let root = world();
    let mut session = open(root.path(), &[]);
    session.enable_continuous_preparation();
    session.intent.goal = Some("the previous durable intent".into());
    let stop = session.begin_preparation_turn();
    let callback = stop.clone();
    session.on_activity(Arc::new(move |activity| {
        if activity.phase == crate::activity::Phase::Understanding && activity.done {
            callback.cancel();
        }
    }));
    let result = session.turn(COPY);
    assert!(stop.is_cancelled(), "the compiler returned a real reading");
    assert!(
        matches!(result, TurnOutcome::Cancelled(_)),
        "Stop must win before proposal commit"
    );
    assert!(session.pending_proposal().is_none());
    assert_eq!(
        session.intent.goal.as_deref(),
        Some("the previous durable intent")
    );
    assert!(workflows(root.path()).is_empty());
    session.on_activity(Arc::new(|_| {}));
    let next = session.begin_preparation_turn();
    assert!(!next.is_cancelled());
    assert!(matches!(
        session.turn(COPY_FR),
        TurnOutcome::Proposal { .. }
    ));
}

#[test]
fn a_late_stop_withdraws_only_the_new_proposal_and_keeps_intent_and_history() {
    let root = world();
    let home = tempfile::tempdir().expect("history home");
    let mut session = open(root.path(), &[]);
    session.enable_continuous_preparation();
    session.enable_history(home.path()).expect("enable history");
    let stop = session.begin_preparation_turn();
    assert!(matches!(session.turn(COPY), TurnOutcome::Proposal { .. }));
    stop.cancel();
    assert!(session.withdraw_cancelled_preparation().is_some());
    assert!(session.pending_proposal().is_none());
    assert!(
        session.last_outcome.is_none(),
        "a withdrawn Ready is not current context"
    );
    assert_eq!(session.intent.goal.as_deref(), Some(COPY));
    assert!(session.withdraw_cancelled_preparation().is_none());
    assert!(workflows(root.path()).is_empty());
    drop(session);
    let mut restored = open(root.path(), &[]);
    restored.enable_continuous_preparation();
    restored
        .enable_history(home.path())
        .expect("restore history");
    restored.restore_state();
    assert_eq!(restored.intent.goal.as_deref(), Some(COPY));
    assert!(restored.pending_proposal().is_none());
}

#[test]
fn stop_after_status_preserves_an_earlier_proposal_and_run_remains_separate() {
    let root = world();
    let mut session = open(root.path(), &[]);
    session.enable_continuous_preparation();
    session.begin_preparation_turn();
    let TurnOutcome::Proposal { id, .. } = session.turn(COPY) else {
        panic!("proposal");
    };
    let stop = session.begin_preparation_turn();
    assert!(matches!(session.turn("/status"), TurnOutcome::Facts(_)));
    stop.cancel();
    assert!(session.withdraw_cancelled_preparation().is_none());
    assert_eq!(session.pending_proposal(), Some(id));
    session.begin_preparation_turn();
    assert!(matches!(session.consent("yes"), TurnOutcome::Facts(_)));
    let stop = session.begin_preparation_turn();
    stop.cancel();
    assert!(matches!(
        session.turn("run it"),
        TurnOutcome::RunRequested { .. }
    ));
    assert!(session.withdraw_cancelled_preparation().is_none());
}

#[test]
fn a_late_stop_discards_a_new_preparation_question_but_not_a_waiting_one() {
    let root = world();
    let mut session = open(root.path(), &[]);
    let stop = session.begin_preparation_turn();
    assert!(matches!(session.turn(DRAFT), TurnOutcome::Question { .. }));
    stop.cancel();
    assert!(session.withdraw_cancelled_preparation().is_some());
    assert!(session.pending_question().is_none());
    session.begin_preparation_turn();
    assert!(matches!(session.turn(DRAFT), TurnOutcome::Question { .. }));
    let stop = session.begin_preparation_turn();
    assert!(matches!(session.turn("/status"), TurnOutcome::Facts(_)));
    stop.cancel();
    assert!(session.withdraw_cancelled_preparation().is_none());
    assert_eq!(
        session.pending_question().map(|q| q.key.as_str()),
        Some("model")
    );
}

#[test]
fn withdrawal_keeps_each_unchanged_pending_field_independently() {
    for keep_proposal in [true, false] {
        let root = world();
        let mut session = open(root.path(), &[]);
        let mut other = open(root.path(), &[]);
        let (original, new) = if keep_proposal {
            (COPY, DRAFT)
        } else {
            (DRAFT, COPY)
        };
        session.turn(original);
        other.turn(new);
        let original_proposal = session.pending.clone();
        let original_question = session.authoring.clone();
        let original_reading = format!("{:?}", session.last_outcome);
        let unresolved = session.intent.unresolved.clone();
        let stop = session.begin_preparation_turn();
        // A late producer return changed only one pending field, before the host can paint it.
        if keep_proposal {
            session.authoring = other.authoring.take();
            session.intent.unresolved = other.intent.unresolved.clone();
        } else {
            session.pending = other.pending.take();
        }
        session.last_outcome = other.last_outcome.take();
        stop.cancel();
        assert!(session.withdraw_cancelled_preparation().is_some());
        assert!(
            session.pending == original_proposal,
            "the earlier proposal must remain exact"
        );
        assert!(
            session.authoring == original_question,
            "the earlier authoring round must remain exact"
        );
        assert!(
            session.intent.unresolved == unresolved,
            "the earlier unresolved obligations must remain exact"
        );
        assert!(
            format!("{:?}", session.last_outcome) == original_reading,
            "the earlier reading must remain exact"
        );
        assert!(workflows(root.path()).is_empty());
    }
}
