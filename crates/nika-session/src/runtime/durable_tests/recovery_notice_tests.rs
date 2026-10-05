// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Recovery notices are historical positions, not a new latest failure on every open.
//! No model, process, workflow or provider is invoked: only the native history owner and
//! the Session restore door consume disposable files and scripted reasoning.
use super::*;
use crate::runtime::history::{AuthorityState, EffectState, History, Operation, RunState};

fn notes(history: &History) -> usize {
    history
        .state
        .recent
        .iter()
        .filter(|(user, text)| user == "(recovery)" && text.contains("uncertain result"))
        .count()
}

fn completed(
    history: &mut History,
    operation: Operation,
    run: RunState,
    effect: EffectState,
    line: (&str, &str),
    outcome: &str,
) {
    history.begin(operation, line.0).expect("start");
    let mut state = history.state.clone();
    state.recent.push((line.0.to_owned(), line.1.to_owned()));
    if state.recent.len() > crate::runtime::RECENT_TURNS {
        state.recent.remove(0);
    }
    history
        .complete(state, run, AuthorityState::None, outcome.to_owned(), effect)
        .expect("complete");
}

fn crashed(home: &Path, root: &Path) -> History {
    let mut first = History::open(home, root).expect("new history");
    first
        .begin(Operation::Turn, "create the report. Budget: 2 USD.")
        .expect("started turn");
    drop(first);
    let recovered = History::open(home, root).expect("recover interruption");
    assert!(recovered.uncertain);
    assert!(recovered.monetary_seen);
    assert_eq!(notes(&recovered), 1);
    recovered
}

#[test]
fn successful_later_work_keeps_the_old_notice_in_place_on_every_reopen() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let mut history = crashed(home.path(), root.path());
    let money = serde_json::json!({"schema": "opaque-test-cost", "held": "unknown"});
    history.state.inference_checkpoint = Some(money.clone());
    history
        .state
        .decisions
        .push("earlier charge remains unknown".to_owned());
    completed(
        &mut history,
        Operation::Observation,
        RunState::Idle,
        EffectState::NoUncertaintyReported,
        ("(run)", "run observed · exit 0 · succeeded"),
        "facts",
    );
    let retained = history.state.recent.clone();
    drop(history);
    for _ in 0..2 {
        let history = History::open(home.path(), root.path()).expect("reopen after success");
        assert!(
            history.state.recent == retained,
            "no new latest recovery card"
        );
        assert_eq!(notes(&history), 1);
        assert!(
            history.uncertain,
            "unrelated success settles no earlier effect"
        );
        assert!(history.monetary_seen);
        assert!(
            history.state.inference_checkpoint.as_ref() == Some(&money),
            "retained cost evidence is unchanged"
        );
        assert!(
            history.state.decisions == ["earlier charge remains unknown"],
            "historical money decision is unchanged"
        );
        assert!(matches!(history.authority, AuthorityState::None));
        assert!(history.run == RunState::Idle);
    }
    let (mut session, seen) = open(root.path(), &[]);
    let notice = session
        .enable_history(home.path())
        .expect("restore")
        .expect("notice");
    assert!(
        notice.contains("historical unresolved operation"),
        "the notice distinguishes historical uncertainty"
    );
    assert!(
        notice.contains("later success does not reconcile it"),
        "the notice distinguishes historical uncertainty"
    );
    assert!(
        notice.contains("uncertain"),
        "uncertainty remains disclosed"
    );
    assert!(
        session.kept_turns() == retained,
        "restore preserves the retained conversation"
    );
    assert!(
        seen.lock().expect("calls").is_empty(),
        "restore calls no reasoner"
    );
}

#[test]
fn an_unreported_effect_survives_a_success_before_the_first_recovery() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let mut history = History::open(home.path(), root.path()).expect("new history");
    completed(
        &mut history,
        Operation::Consent,
        RunState::Idle,
        EffectState::Unknown,
        ("yes", "write may have happened"),
        "io_refusal",
    );
    completed(
        &mut history,
        Operation::Turn,
        RunState::Idle,
        EffectState::NoUncertaintyReported,
        ("hello", "a later clean reply"),
        "reply",
    );
    drop(history);
    let history = History::open(home.path(), root.path()).expect("first recovery");
    assert!(history.uncertain);
    assert_eq!(
        notes(&history),
        1,
        "first notification was never acknowledged"
    );
    let retained = history.state.recent.clone();
    drop(history);
    let history = History::open(home.path(), root.path()).expect("again");
    assert!(
        history.state.recent == retained,
        "restore does not add or reorder retained turns"
    );
}

#[test]
fn evicting_the_notice_from_the_recent_ring_does_not_make_it_new_again() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let mut history = crashed(home.path(), root.path());
    for index in 0..crate::runtime::RECENT_TURNS {
        completed(
            &mut history,
            Operation::Turn,
            RunState::Idle,
            EffectState::NoUncertaintyReported,
            (&format!("later turn {index}"), "completed"),
            "reply",
        );
    }
    assert_eq!(notes(&history), 0, "ordinary bounded-context eviction");
    let retained = history.state.recent.clone();
    drop(history);
    let mut history = History::open(home.path(), root.path()).expect("reopen full ring");
    assert!(
        history.state.recent == retained,
        "restore does not add or reorder retained turns"
    );
    assert!(
        history.uncertain,
        "journal uncertainty outlives the recent window"
    );
    history
        .begin(Operation::Consent, "yes")
        .expect("another interruption");
    drop(history);
    let history = History::open(home.path(), root.path()).expect("new recovery");
    assert_eq!(
        notes(&history),
        1,
        "a distinct unfinished effect is newly reported"
    );
    assert!(
        history.state.recent.len() == crate::runtime::RECENT_TURNS,
        "the retained window stays bounded"
    );
    assert!(history.uncertain);
}

#[test]
fn a_second_unknown_effect_is_reported_without_clearing_the_first() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let mut history = crashed(home.path(), root.path());
    completed(
        &mut history,
        Operation::Consent,
        RunState::Idle,
        EffectState::Unknown,
        ("yes", "another effect unknown"),
        "io_refusal",
    );
    drop(history);
    let history = History::open(home.path(), root.path()).expect("new unknown effect");
    assert_eq!(notes(&history), 2);
    assert!(history.uncertain);
}

#[test]
fn an_unobserved_run_is_not_reported_again_until_a_new_request_is_unobserved() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let mut history = History::open(home.path(), root.path()).expect("new history");
    completed(
        &mut history,
        Operation::Run,
        RunState::AwaitingObservation,
        EffectState::NoUncertaintyReported,
        ("run", "requested"),
        "run_requested",
    );
    drop(history);
    let mut history = History::open(home.path(), root.path()).expect("unobserved run");
    assert_eq!(notes(&history), 1);
    completed(
        &mut history,
        Operation::Turn,
        RunState::AwaitingObservation,
        EffectState::NoUncertaintyReported,
        ("hello", "completed"),
        "reply",
    );
    let retained = history.state.recent.clone();
    drop(history);
    let mut history = History::open(home.path(), root.path()).expect("same unobserved run");
    assert!(
        history.state.recent == retained,
        "restore does not add or reorder retained turns"
    );
    assert!(history.uncertain);
    assert!(history.run == RunState::AwaitingObservation);
    completed(
        &mut history,
        Operation::Run,
        RunState::AwaitingObservation,
        EffectState::NoUncertaintyReported,
        ("run another", "requested"),
        "run_requested",
    );
    drop(history);
    let history = History::open(home.path(), root.path()).expect("second unobserved run");
    assert_eq!(notes(&history), 2);
    assert!(history.uncertain);
}
