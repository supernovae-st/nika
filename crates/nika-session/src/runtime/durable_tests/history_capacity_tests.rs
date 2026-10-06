// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Conversation capacity is not a creation allowance. Large valid records remain
//! readable; integrity, exclusive ownership and recovery still govern every size.

use super::*;
use crate::runtime::history::{AuthorityState, EffectState, History, Operation, RunState, Saved};

#[test]
fn a_large_input_reaches_the_reasoner_and_reopens_without_another_call() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let input = format!("What is {}?", "violet comet ".repeat(6000).trim_end());
    assert!(input.len() > 64 * 1024);
    let (mut first, seen) = open(root.path(), &[ANSWER]);
    first.enable_history(home.path()).expect("fresh history");
    let outcome = first.turn(&input);
    assert!(matches!(outcome, TurnOutcome::Reply(_)), "{outcome:?}");
    let prompts = seen.lock().expect("prompts");
    assert_eq!(prompts.len(), 1);
    assert!(
        prompts[0].contains(&input),
        "the complete request reaches the seat"
    );
    drop(prompts);
    drop(first);

    let (mut resumed, seen) = open(root.path(), &[]);
    resumed.enable_history(home.path()).expect("reopen");
    assert!(
        resumed
            .kept_turns()
            .iter()
            .any(|(user, answer)| user == &input && answer == ANSWER)
    );
    assert!(seen.lock().expect("calls").is_empty());
    assert!(resumed.pending_proposal().is_none());
    assert!(!root.path().join(LANDED).exists());
}

fn append_reply(history: &mut History, input: &str, answer: &str) {
    history.begin(Operation::Turn, input).expect("start");
    history
        .complete(
            Saved {
                recent: vec![(input.to_owned(), answer.to_owned())],
                ..Saved::default()
            },
            RunState::Idle,
            AuthorityState::None,
            "reply".to_owned(),
            EffectState::NoUncertaintyReported,
        )
        .expect("complete");
}

#[test]
fn a_record_larger_than_one_mib_keeps_its_exact_dialogue() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let answer = "é".repeat(600_000);
    let mut history = History::open(home.path(), root.path()).expect("history");
    append_reply(&mut history, GOAL, &answer);
    drop(history);
    let journal = history_dir(home.path(), root.path()).join("events.ndjson");
    let before = std::fs::read(&journal).expect("journal");
    assert!(
        before
            .split(|byte| *byte == b'\n')
            .any(|line| line.len() > 1024 * 1024)
    );

    let (mut resumed, seen) = open(root.path(), &[]);
    resumed
        .enable_history(home.path())
        .expect("reopen large record");
    assert_eq!(resumed.kept_turns(), &[(GOAL.to_owned(), answer)]);
    assert!(seen.lock().expect("calls").is_empty());
    assert!(
        std::fs::read(&journal)
            .expect("preserved journal")
            .starts_with(&before)
    );
}

fn large_journal(home: &Path, root: &Path) -> PathBuf {
    let mut history = History::open(home, root).expect("history");
    // Each record fits the former record ceiling; their total exceeds the
    // former journal ceiling without fabricating invalid padding or events.
    let answer = "v".repeat(900 * 1024);
    for index in 0..20 {
        append_reply(&mut history, &format!("conversation {index}"), &answer);
    }
    append_reply(&mut history, GOAL, ANSWER);
    drop(history);
    let journal = history_dir(home, root).join("events.ndjson");
    assert!(std::fs::metadata(&journal).expect("journal metadata").len() > 16 * 1024 * 1024);
    journal
}

#[test]
fn a_large_journal_reopens_and_continues_without_reset_or_replay() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let journal = large_journal(home.path(), root.path());
    let before = std::fs::read(&journal).expect("original journal");
    let (mut resumed, seen) = open(root.path(), &["A second reply."]);
    let notice = resumed
        .enable_history(home.path())
        .expect("reopen")
        .expect("notice");
    assert!(notice.contains("conversation restored"), "{notice}");
    assert_eq!(
        resumed.kept_turns(),
        &[(GOAL.to_owned(), ANSWER.to_owned())]
    );
    assert!(
        seen.lock().expect("calls").is_empty(),
        "recovery calls no reasoner"
    );
    assert!(resumed.pending_proposal().is_none());
    assert!(resumed.waiting_gate().is_none());
    assert!(!root.path().join(LANDED).exists());
    assert!(matches!(resumed.turn(GOAL), TurnOutcome::Reply(_)));
    assert_eq!(
        seen.lock().expect("calls").len(),
        1,
        "only the new explicit turn reasons"
    );
    drop(resumed);
    assert!(
        std::fs::read(&journal)
            .expect("retained journal")
            .starts_with(&before)
    );

    let (mut again, seen) = open(root.path(), &[]);
    again
        .enable_history(home.path())
        .expect("reopen continued journal");
    assert_eq!(
        again.kept_turns().last(),
        Some(&(GOAL.to_owned(), "A second reply.".to_owned()))
    );
    assert!(seen.lock().expect("calls").is_empty());
}

#[test]
fn damaged_large_journals_and_empty_journals_remain_refused_without_rewrite() {
    let root = project();
    let home = tempfile::tempdir().expect("home");
    let journal = large_journal(home.path(), root.path());
    let valid = std::fs::read(&journal).expect("valid journal");
    let mut truncated = valid.clone();
    truncated.pop();
    let mut corrupted = valid;
    let goal = corrupted
        .windows(GOAL.len())
        .position(|bytes| bytes == GOAL.as_bytes())
        .expect("recorded goal");
    corrupted[goal] = b'X';
    for bytes in [truncated, corrupted, Vec::new()] {
        std::fs::write(&journal, &bytes).expect("damage fixture");
        let (mut resumed, seen) = open(root.path(), &[]);
        assert!(resumed.enable_history(home.path()).is_err());
        assert!(matches!(resumed.turn(GOAL), TurnOutcome::Refusal(_)));
        assert!(seen.lock().expect("calls").is_empty());
        assert_eq!(std::fs::read(&journal).expect("unchanged journal"), bytes);
    }
}
