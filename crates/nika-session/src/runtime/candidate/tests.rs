// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate a host draws is the proposal a consent answers, borrowed whole: its identity
//! is the consent's, its bytes are the ones a yes lands, a revision replaces it and leaves the
//! old identity stale, a revision's question sets it aside (never consentable), and reading it
//! changes nothing.

use super::super::tests::{COPY, COPY_DEST, ready, tree};
use super::super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::NoReasoner;

/// A session with no intelligence over a root holding the copy's source: the deterministic
/// compiler proposes COPY with no call.
fn session() -> (tempfile::TempDir, SessionRuntime) {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let none = ready(IntelligenceKind::None, DataLocus::None);
    let s = SessionRuntime::open(dir.path(), none, Box::new(NoReasoner));
    (dir, s)
}

/// Everything a reading must leave as it was: the proposal, its identity, the set aside one,
/// the decided one, the rehearsals' binding and the tree on disk.
fn state(s: &SessionRuntime, root: &Path) -> String {
    let mut tree: Vec<String> = walk(root);
    tree.sort();
    let proof = s
        .pending_proposal()
        .and_then(|id| s.rehearsals.lines_of(&id).map(str::to_owned));
    format!(
        "{:?}|{:?}|{:?}|{:?}|{proof:?}|{tree:?}",
        s.pending,
        s.pending_proposal(),
        s.revising.as_ref().map(|(set, _)| set),
        s.decided,
    )
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("dir").flatten() {
        let path = entry.path();
        out.push(path.display().to_string());
        if path.is_dir() {
            out.extend(walk(&path));
        }
    }
    out
}

#[test]
fn the_candidate_is_the_pending_proposal_borrowed_whole() {
    let (dir, mut s) = session();
    assert!(s.candidate().is_none(), "nothing proposed yet");
    let TurnOutcome::Proposal { id, preview } = s.turn(COPY) else {
        panic!("a proposal");
    };
    let before = state(&s, dir.path());
    let candidate = s.candidate().expect("the proposal is the candidate");
    assert_eq!(candidate.id, id, "the identity a consent names");
    assert_eq!(Some(&candidate.id), s.pending_proposal().as_ref());
    assert!(!candidate.aside);
    let pending = s.pending.as_ref().expect("pending");
    assert!(
        std::ptr::eq(candidate.set, pending),
        "borrowed, never copied"
    );
    assert_eq!(candidate.set.changes.len(), 1);
    assert_eq!(candidate.set.changes[0].path(), Path::new(COPY_DEST));
    assert!(candidate.set.changes[0].witness().is_none(), "a create");
    // The rehearsal words, when a proof binds this identity, are the preview's own; with none
    // bound the preview says that nothing ran, or why no rehearsal ran.
    match candidate.rehearsed {
        Some(words) => assert!(preview.contains(words), "{words}\n{preview}"),
        None => assert!(
            preview.contains(crate::review::NOTHING_RAN) || preview.contains("Rehearsal not run"),
            "{preview}"
        ),
    }
    drop(candidate);
    assert_eq!(state(&s, dir.path()), before, "reading changes nothing");
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing written");
}

#[test]
fn a_revision_replaces_the_candidate_and_its_old_identity_is_stale() {
    let (dir, mut s) = session();
    let TurnOutcome::Proposal { id: first, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    let bytes = s.candidate().expect("A").set.changes[0]
        .content()
        .to_owned();
    let TurnOutcome::Proposal { id: revised, .. } = s.consent("budget 0.10 USD") else {
        panic!("a money amendment is a new proposal");
    };
    let candidate = s.candidate().expect("B");
    assert_ne!(revised, first);
    assert_eq!(candidate.id, revised, "the candidate is B, never A");
    assert_eq!(
        candidate.set.changes[0].content(),
        bytes,
        "the same bytes under a new consent identity"
    );
    assert!(matches!(
        s.consent_to(&first, "yes"),
        TurnOutcome::Refusal(ref r) if r.class == RefusalClass::StaleRevision
    ));
    assert_eq!(s.candidate().map(|c| c.id), Some(revised.clone()));
    assert!(!dir.path().join(COPY_DEST).exists(), "A applied nothing");
    assert!(matches!(
        s.consent_to(&revised, "yes"),
        TurnOutcome::Facts(_)
    ));
    assert!(
        s.candidate().is_none(),
        "a saved candidate is no longer one"
    );
    assert!(dir.path().join(COPY_DEST).exists());
}

#[test]
fn a_discarded_or_dropped_proposal_leaves_no_candidate() {
    let (dir, mut s) = session();
    let TurnOutcome::Proposal { .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
    assert!(s.candidate().is_none());
    let TurnOutcome::Proposal { .. } = s.turn(COPY) else {
        panic!("a proposal again");
    };
    // Leaving at the consent prompt drops the proposal: a consent is the next line, never a
    // later session's (only a durable draft may propose it again, for a fresh consent).
    assert!(matches!(s.consent("/quit"), TurnOutcome::Quit));
    assert!(s.candidate().is_none());
    assert!(!dir.path().join(COPY_DEST).exists());
}

#[test]
fn a_revision_question_sets_the_candidate_aside_never_consentable() {
    let (dir, mut s) = session();
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    // The state a revision's question leaves (`settle_pending_revision`): the proposal it
    // revises waits aside while the round asks.
    let set = s.pending.take().expect("pending");
    s.revising = Some((set, None));
    s.authoring = Some(crate::authoring::AuthoringRound::new("revise"));
    let before = state(&s, dir.path());
    let candidate = s.candidate().expect("set aside, still shown");
    assert!(candidate.aside);
    assert_eq!(candidate.id, id, "the identity it waits again under");
    let aside = candidate.id.clone();
    assert!(s.pending_proposal().is_none(), "not consentable meanwhile");
    assert!(
        matches!(s.consent_to(&aside, "yes"), TurnOutcome::Refusal(ref r) if r.class == RefusalClass::WrongState),
        "an aside candidate applies nothing"
    );
    assert!(!dir.path().join(COPY_DEST).exists());
    assert_eq!(state(&s, dir.path()), before);
}
