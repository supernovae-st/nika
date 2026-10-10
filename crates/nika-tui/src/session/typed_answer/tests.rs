// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hermetic Live adapter proofs over the real deterministic Session, with no Run/provider.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use nika_session::QuestionId;
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
};
use nika_session::reasoner::NoReasoner;
use nika_session::work;

use crate::model::{Asked, Beat, Conversation, Kind, Turn, Waiting};
use crate::session::feed::{Gap, Seen};
use crate::session::{Live, Runners};

struct Room(PathBuf);
impl Room {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nika-typed-answer-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).expect("room");
        Self(root)
    }
}
impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn live(root: &Path) -> Live {
    let mut live = Live::new(
        root.to_path_buf(),
        IntelligenceCensus::empty(),
        Some(UserIntelligencePreference::new(
            IntelligenceKind::None,
            None,
        )),
        None,
        Box::new(|_| Box::new(NoReasoner)),
        Runners {
            run_once: Box::new(|_, _| panic!("no implicit Run")),
            run_resume: Box::new(|_, _, _, _| panic!("no implicit resume")),
            run_tapped: None,
        },
    );
    let _ = live.open();
    live
}

fn question(live: &mut Live) -> (String, QuestionId, Asked) {
    let turn = live.submit("aggregate-by-key");
    let (key, asked) = turn
        .beats
        .iter()
        .find_map(|beat| match beat {
            Beat::Wait(Waiting::QuestionDocument { key, asked }) => {
                Some((key.clone(), asked.clone()))
            }
            _ => None,
        })
        .expect("the actual typed currency question");
    let id = live
        .runtime
        .as_ref()
        .expect("runtime")
        .pending_question_id()
        .expect("waiting");
    let displayed = turn
        .beats
        .iter()
        .find_map(|beat| match beat {
            Beat::Say(committed) if committed.kind == Kind::Question => Some(committed),
            _ => None,
        })
        .expect("the actual question words");
    assert_eq!(displayed.question_witness(), Some(asked.witness.as_str()));
    (key, id, asked)
}

fn answer(live: &mut Live, text: &str, witness: &str) -> Turn {
    let (tx, _rx) = mpsc::channel();
    let (seen, _rx) = mpsc::sync_channel(16);
    live.answer_bound(
        text,
        witness,
        &tx,
        &Seen::new(seen, Arc::new(Gap::default())),
    )
}

fn snapshot(live: &Live) -> serde_json::Value {
    serde_json::to_value(live.runtime.as_ref().expect("runtime").work()).expect("work")
}

fn refused(turn: &Turn, text: &str) {
    assert!(turn.handoff.is_none(), "no effect handoff");
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::Say(c) if c.kind == Kind::Refusal)),
        "{turn:?}"
    );
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::NotTaken(t) if t == text)),
        "{turn:?}"
    );
}

#[test]
fn painting_lends_the_same_question_document_and_strong_identity() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (key, id, asked) = question(&mut live);
    let work = live.runtime.as_ref().expect("runtime").work();
    let q = work.question.expect("document");
    assert_eq!(q.key, key);
    assert_eq!(
        asked.witness,
        super::super::asked::witness(&id, live.question_epoch)
    );
    assert_eq!(asked.label, q.label);
    assert_eq!(asked.why, q.why);
    assert_eq!(asked.mandatory, q.mandatory);
    assert_eq!(asked.epoch, live.question_epoch);
    assert!(matches!(&live.shown, work::Waiting::Question { id: shown, .. } if *shown == id));
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

/// The question carries what the real Session keeps of its request, from the
/// same snapshot: the goal as kept. The Session keeps the question it asks as
/// its open question; beside that question it is not repeated.
#[test]
fn the_question_carries_the_request_the_session_keeps() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, _, asked) = question(&mut live);
    let request = live.runtime.as_ref().expect("runtime").work().request;
    assert_eq!(request.goal.as_deref(), Some("aggregate-by-key"));
    assert_eq!(request.unresolved, std::slice::from_ref(&asked.label));
    let retained = asked.retained.expect("the request is kept");
    assert_eq!(retained.goal, request.goal);
    assert!(retained.open.is_empty(), "{retained:?}");
}

#[test]
fn a_different_painted_witness_refuses_before_any_session_change_and_returns_exact_text() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, id, _) = question(&mut live);
    let before = snapshot(&live);
    let text = "my answer\nwith its second line  ";
    let turn = answer(&mut live, text, "not-the-painted-identity");
    refused(&turn, text);
    assert_eq!(snapshot(&live), before);
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        Some(id)
    );
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

fn answer_writes_no_file(room: &Path) {
    assert!(
        !room.join("aggregate-by-key.nika").exists(),
        "the actual proposed file is not saved"
    );
    assert!(
        !room.join("compiled-workflow.nika").exists(),
        "answer is not Save"
    );
}

#[test]
fn a_bound_answer_changes_the_draft_once_and_the_old_identity_never_starts_a_turn() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (key, id, asked) = question(&mut live);
    let turn = answer(&mut live, "EUR", &asked.witness);
    assert!(
        matches!(turn.beats.first(), Some(Beat::Say(c))
        if c.kind == Kind::Notice
        && c.text == "Answer taken · the currency code\n«EUR»\nas you typed it"),
        "the bound act must be reported before its compile outcome: {turn:?}"
    );
    assert!(
        !turn.beats.iter().any(|b| matches!(b, Beat::NotTaken(_))),
        "{turn:?}"
    );
    let runtime = live.runtime.as_ref().expect("runtime");
    let work = runtime.work();
    assert_eq!(
        work.answered,
        Some(work::Answered::new(
            id.clone(),
            work::AnswerAct::Bound {
                key: key.clone(),
                value: "EUR".into(),
                reading: work::ValueSource::AsTyped,
            }
        ))
    );
    assert_eq!(
        runtime.work(),
        work,
        "reading the snapshot again keeps the actual act"
    );
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::Say(c) if c.kind == Kind::Proposal)),
        "{turn:?}"
    );
    let candidate = runtime.candidate().expect("proposal");
    let source = candidate
        .set
        .changes
        .iter()
        .find(|c| c.is_workflow())
        .expect("workflow")
        .content();
    assert!(
        source.contains("currency: EUR"),
        "exact bound value: {source}"
    );
    assert!(work.request.unresolved.is_empty(), "{:?}", work.request);
    assert_eq!(work.saved, None, "binding never saves");
    assert_eq!(work.requested, None, "binding never requests Run");
    assert_eq!(work.run, None, "binding is not an observed Run");
    answer_writes_no_file(&room.0);
    assert_ne!(runtime.pending_question_id(), Some(id.clone()));
    let before = snapshot(&live);
    // A screen still carrying the strong old id reaches the runtime refusal, never submit.
    live.shown = work::Waiting::Question {
        key,
        id: id.clone(),
    };
    let again = answer(
        &mut live,
        "a fresh automation that must never start",
        &asked.witness,
    );
    refused(&again, "a fresh automation that must never start");
    assert_eq!(
        live.runtime.as_ref().expect("runtime").work().answered,
        Some(work::Answered::new(
            id.clone(),
            work::AnswerAct::Refused {
                class: nika_session::RefusalClass::AlreadyConsumed,
            }
        ))
    );
    let mut expected = before;
    expected["answered"] = serde_json::json!({
        "question": id.as_str(), "act": "refused", "class": "already_consumed",
    });
    assert_eq!(
        snapshot(&live),
        expected,
        "only the refused answer act changes; draft, consent and effects stay exact"
    );
    assert!(!again.beats.iter().any(|b| matches!(b, Beat::Say(c)
        if c.kind == Kind::Notice && c.text.starts_with("Answer taken"))));
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

#[test]
fn a_read_only_line_keeps_the_question_and_cancelling_never_claims_a_bound_value() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, id, asked) = question(&mut live);
    let help = answer(&mut live, "/help", &asked.witness);
    assert!(
        !help.beats.iter().any(|b| matches!(b, Beat::NotTaken(_))),
        "{help:?}"
    );
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        Some(id)
    );
    let cancelled = answer(&mut live, "cancel", &asked.witness);
    assert!(cancelled.handoff.is_none());
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        None
    );
    assert!(
        !live
            .runtime
            .as_ref()
            .expect("runtime")
            .work()
            .request
            .decisions
            .iter()
            .any(|s| s.contains("(answered "))
    );
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

/// A sentence that no chosen intelligence can read is not a bound value.
/// The question keeps its exact identity and the human keeps every answer byte.
#[test]
fn an_unread_answer_stays_unsent_instead_of_disappearing_from_the_draft() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, id, asked) = question(&mut live);
    let text = "use euro or dollars\nbut keep these words  ";
    let turn = answer(&mut live, text, &asked.witness);
    assert!(!turn.beats.iter().any(|b| matches!(b, Beat::Say(c)
        if c.kind == Kind::Notice && c.text.starts_with("Answer taken"))));
    let act = live
        .runtime
        .as_ref()
        .expect("runtime")
        .work()
        .answered
        .expect("waits act");
    assert_eq!(act.question, id);
    assert!(matches!(act.act, work::AnswerAct::Waits { .. }));
    assert!(turn.handoff.is_none(), "no effect handoff");
    assert!(
        turn.beats.iter().any(|b| matches!(b, Beat::Say(c)
        if c.kind == Kind::Question)),
        "{turn:?}"
    );
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        Some(id)
    );
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::NotTaken(t) if t == text)),
        "{turn:?}"
    );
    assert!(
        !live
            .runtime
            .as_ref()
            .expect("runtime")
            .work()
            .request
            .decisions
            .iter()
            .any(|s| s.contains("(answered "))
    );
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

/// Equal wire witnesses from two Sessions do not lend native ownership.
#[test]
fn a_painted_token_from_another_live_incarnation_never_binds_here() {
    let room = Room::new();
    let mut first = live(&room.0);
    let mut second = live(&room.0);
    let (_, first_id, first_asked) = question(&mut first);
    let (_, second_id, _) = question(&mut second);
    assert_eq!(first_id.as_str(), second_id.as_str(), "same wire witness");
    assert_ne!(first_id, second_id, "different Session incarnation");
    let before = snapshot(&second);
    let turn = answer(&mut second, "EUR", &first_asked.witness);
    refused(&turn, "EUR");
    assert_eq!(
        snapshot(&second),
        before,
        "no route or binding in this Session"
    );
    assert_eq!(
        second
            .runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        Some(second_id)
    );
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

#[test]
fn question_words_keep_exact_bytes_and_only_the_supported_snapshot_lends_a_witness() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (key, id, asked) = question(&mut live);
    let mut work = live.runtime.as_ref().expect("runtime").work();
    let words = "Choose a currency\r\nkeep every word and trailing space  ";
    let tagged = super::super::asked::question(&work, &key, words.to_owned(), live.question_epoch);
    assert_eq!(tagged.text, words);
    assert_eq!(tagged.question_witness(), Some(asked.witness.as_str()));
    let another =
        super::super::asked::question(&work, &key, words.to_owned(), live.question_epoch + 1);
    assert_eq!(another.text, words);
    assert_ne!(another.question_witness(), tagged.question_witness());
    for legacy_key in ["another-question", "run_cost", "unknown_cost"] {
        let legacy =
            super::super::asked::question(&work, legacy_key, words.to_owned(), live.question_epoch);
        assert_eq!(legacy.text, words);
        assert_eq!(legacy.question_witness(), None);
    }
    work.question.as_mut().expect("document").answer_type = "future";
    let unsupported =
        super::super::asked::question(&work, &key, words.to_owned(), live.question_epoch);
    assert_eq!(unsupported.text, words);
    assert_eq!(unsupported.question_witness(), None);
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_question_id(),
        Some(id)
    );
    assert!(!room.0.join("compiled-workflow.nika").exists());
}

#[test]
fn a_later_read_only_line_clears_the_prior_bound_act_without_saving_or_running() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, _, asked) = question(&mut live);
    let bound = answer(&mut live, "EUR", &asked.witness);
    assert!(matches!(bound.beats.first(), Some(Beat::Say(c))
        if c.kind == Kind::Notice && c.text.starts_with("Answer taken")));
    let before = live.runtime.as_ref().expect("runtime").work();
    assert!(matches!(before.answered.as_ref().map(|a| &a.act),
        Some(work::AnswerAct::Bound { value, .. }) if value == "EUR"));
    let help = live.submit("/help");
    let after = live.runtime.as_ref().expect("runtime").work();
    assert_eq!(after.answered, None);
    assert_eq!(after.candidate, before.candidate);
    assert_eq!(after.waiting, before.waiting);
    assert_eq!(after.saved, None);
    assert_eq!(after.requested, None);
    assert!(help.handoff.is_none());
    assert!(!help.beats.iter().any(|b| matches!(b, Beat::Say(c)
        if c.kind == Kind::Notice && c.text.starts_with("Answer taken"))));
    assert!(!room.0.join("aggregate-by-key.nika").exists());
}

/// The public invalid-money line expires the question before binding. It
/// must not return as an unsent answer under the now-free prompt. This is
/// the directive branch, not a private restored-budget injection.
#[test]
fn a_prebinding_money_refusal_expires_the_question_without_restoring_a_free_answer() {
    let room = Room::new();
    let mut live = live(&room.0);
    let (_, id, asked) = question(&mut live);
    let text = "Budget: -1 USD.";
    let turn = answer(&mut live, text, &asked.witness);
    assert!(turn.handoff.is_none());
    assert!(
        turn.beats
            .iter()
            .any(|b| matches!(b, Beat::Say(c) if c.kind == Kind::Refusal)),
        "{turn:?}"
    );
    assert!(
        !turn.beats.iter().any(|b| matches!(b, Beat::NotTaken(_))),
        "the expired question cannot restore an answer into Free: {turn:?}"
    );
    assert!(!turn.beats.iter().any(|b| matches!(b, Beat::Say(c)
        if c.kind == Kind::Notice && c.text.starts_with("Answer taken"))));
    assert!(matches!(turn.beats.last(), Some(Beat::Wait(Waiting::Free))));
    let runtime = live.runtime.as_ref().expect("runtime");
    let work = runtime.work();
    assert_eq!(runtime.pending_question_id(), None);
    assert_eq!(work.waiting, work::Waiting::Free);
    assert_eq!(
        work.answered,
        Some(work::Answered::new(
            id,
            work::AnswerAct::Refused {
                class: nika_session::RefusalClass::NotAllowed,
            }
        ))
    );
    assert_eq!(runtime.work(), work, "reread preserves this refusal act");
    assert_eq!(work.candidate, None);
    assert_eq!(work.saved, None);
    assert_eq!(work.requested, None);
    assert_eq!(work.run, None);
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    let before = snapshot(&live);
    let old = answer(&mut live, "EUR", &asked.witness);
    refused(&old, "EUR");
    assert_eq!(
        snapshot(&live),
        before,
        "an old painted reply starts no new turn"
    );
}
