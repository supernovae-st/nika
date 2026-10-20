// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Replay the real journal without a provider, a workflow or restored authority.

#![allow(clippy::expect_used)]

use super::*;
use crate::ScriptedReasoner;
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::runtime::SessionRuntime;
use crate::runtime::history::{AuthorityState, EffectState, History, RunState};
use std::path::Path;

fn append(history: &mut History, operation: Operation, input: &str, pair: (&str, &str)) {
    history.begin(operation, input).expect("start");
    let mut state = history.state.clone();
    state.recent.push((pair.0.to_owned(), pair.1.to_owned()));
    if state.recent.len() > RECENT_TURNS {
        state.recent.remove(0);
    }
    history
        .complete(
            state,
            RunState::Idle,
            AuthorityState::None,
            "facts".to_owned(),
            EffectState::NoUncertaintyReported,
        )
        .expect("complete");
}

fn reopen(root: &Path, home: &Path) -> SessionRuntime {
    let intelligence = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    let mut session = SessionRuntime::open(
        root,
        intelligence,
        Box::new(ScriptedReasoner::new(Vec::new())),
    );
    session.enable_history(home).expect("reopen");
    assert!(session.pending_proposal().is_none());
    assert!(session.waiting_gate().is_none());
    session
}

#[test]
fn host_actions_never_become_human_but_identical_persons_words_stay_human() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let mut history = History::open(home.path(), root.path()).expect("history");
    for (operation, input, pair) in [
        (
            Operation::Turn,
            "save & run",
            ("save & run", "typed literally"),
        ),
        (
            Operation::Consent,
            "save & run",
            ("save & run", "(run requested)"),
        ),
        (Operation::Turn, "(run)", ("(run)", "typed literally")),
        (
            Operation::Observation,
            "(run observation)",
            ("(run)", "run observed"),
        ),
    ] {
        append(&mut history, operation, input, pair);
    }
    let retained = history.state.recent.clone();
    drop(history);
    for _ in 0..2 {
        let session = reopen(root.path(), home.path());
        assert_eq!(session.kept_turns(), retained);
        assert_eq!(
            (0..retained.len())
                .map(|i| session.kept_turn_is_human(i))
                .collect::<Vec<_>>(),
            [true, false, true, false]
        );
        assert!(!session.kept_turn_is_human(retained.len()));
    }
}

#[test]
fn interrupted_persons_input_keeps_its_speaker_while_recovery_is_host_context() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let mut history = History::open(home.path(), root.path()).expect("history");
    history
        .begin(Operation::Turn, "(run)")
        .expect("unfinished input");
    drop(history);
    for _ in 0..2 {
        let session = reopen(root.path(), home.path());
        assert_eq!(session.kept_turns().len(), 2);
        assert_eq!(session.kept_turns()[0].0, "(run)");
        assert_eq!(session.kept_turns()[1].0, "(recovery)");
        assert!(session.kept_turn_is_human(0));
        assert!(!session.kept_turn_is_human(1));
    }
}

#[test]
fn a_full_window_cannot_carry_old_human_labels_onto_host_occurrences() {
    let pair = ("(run)".to_owned(), "same text".to_owned());
    let repeated = vec![pair; RECENT_TURNS];
    let mut shown = Presentation {
        human: vec![true; RECENT_TURNS],
    };
    shown.completed(
        &repeated,
        &repeated,
        Some(&(Operation::Observation, "(run observation)".to_owned())),
    );
    assert_eq!(shown.human, vec![false; RECENT_TURNS]);
}

#[test]
fn truncation_keeps_exact_text_and_new_human_input_without_guessing_old_roles() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let mut history = History::open(home.path(), root.path()).expect("history");
    for index in 0..RECENT_TURNS {
        let input = format!("message {index}");
        append(&mut history, Operation::Turn, &input, (&input, "reply"));
    }
    append(
        &mut history,
        Operation::Observation,
        "observed",
        ("(run)", "finished"),
    );
    append(
        &mut history,
        Operation::Turn,
        "save & run",
        ("save & run", "literal input"),
    );
    let retained = history.state.recent.clone();
    drop(history);
    let session = reopen(root.path(), home.path());
    assert_eq!(session.kept_turns(), retained);
    assert_eq!(session.kept_turns().len(), RECENT_TURNS);
    assert!(!session.kept_turn_is_human(RECENT_TURNS - 2));
    assert!(session.kept_turn_is_human(RECENT_TURNS - 1));
}

#[test]
fn an_unbound_projection_is_neutral_even_when_its_text_matches_the_input() {
    let mut shown = Presentation { human: vec![true] };
    shown.completed(
        &[("old".to_owned(), "old reply".to_owned())],
        &[("replacement".to_owned(), "new reply".to_owned())],
        Some(&(Operation::Turn, "replacement".to_owned())),
    );
    assert_eq!(shown.human, [false]);
}
