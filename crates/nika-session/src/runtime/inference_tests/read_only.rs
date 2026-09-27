// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Session routing truth on the real Session → registry → HTTP path, the selected route's
//! cost unknown (the black-box audit's live seat, 2026-09-27): a slash typo (BUG-U6) and the
//! read-only asks (S02 · S05b) never stage the one-time cost review and never send a byte; a
//! declined review cancels its own call only — the proposal, the question and their
//! identities wait again unchanged, and only the proposal's own `no` discards it.
use super::unknown_cost::{asked, open_unknown, unpriced_response};
use super::*;

/// An intent whose model the compiler asks for, deterministically (zero calls).
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";

/// The unknown-cost session over a project holding the notes the intents read. Any provider
/// call would reach the loopback peer and be counted.
struct Live {
    dir: tempfile::TempDir,
    peer: Peer,
    _transport: test_transport::Installed,
    s: SessionRuntime,
}

fn live() -> Live {
    let peer = Peer::start(vec![(200, unpriced_response("unexpected"))]);
    let transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let s = open_unknown(dir.path());
    Live {
        dir,
        peer,
        _transport: transport,
        s,
    }
}

/// The proposal's exact bytes.
fn bytes(s: &SessionRuntime) -> Vec<String> {
    s.pending.as_ref().map_or_else(Vec::new, |set| {
        set.changes.iter().map(|c| c.content().to_owned()).collect()
    })
}

/// BUG-U6: `/bogus` was read as open language and staged the fresh authoring cost decision;
/// approving it would have spent tokens on a typo. Now it is refused as a command, before any
/// review: nothing is staged, sent or recorded as a goal — and open language on the same route
/// still asks its review first (the control).
#[test]
fn a_slash_typo_never_stages_a_cost_review_or_sends_a_byte() {
    let mut l = live();
    for line in ["/bogus", "/bogus now", "/Status", "/"] {
        let out = l.s.turn(line);
        assert!(
            matches!(&out, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState),
            "{line}: {out:?}"
        );
        assert!(!l.s.waiting_cost_choice(), "{line} staged a cost review");
    }
    assert!(l.peer.bodies().is_empty(), "a typo reached the provider");
    assert!(l.s.inference_receipt().expect("receipt").is_none());
    assert!(l.s.intent.goal.is_none(), "a typo became the goal");
    asked(&l.s.turn("hello"));
    assert!(l.peer.bodies().is_empty());
}

/// S05b and S02 at the consent prompt: « what happened? », `why` and a command are answered
/// from the machine's state — no review is staged, nothing is sent, the proposal keeps its
/// identity and bytes.
#[test]
fn read_only_asks_at_a_proposal_stage_no_review_and_send_nothing() {
    let mut l = live();
    let TurnOutcome::Proposal { id, .. } = l.s.turn(COPY_INTENT) else {
        panic!("a deterministic proposal, no review");
    };
    let proposed = bytes(&l.s);
    for line in [
        "what happened?",
        "why",
        "explain",
        "/bogus",
        "/last",
        "/cancel",
    ] {
        let out = l.s.consent(line);
        assert!(
            !l.s.waiting_cost_choice(),
            "{line} staged a review: {out:?}"
        );
        assert_eq!(
            l.s.pending_proposal().as_ref(),
            Some(&id),
            "{line}: {out:?}"
        );
        assert_eq!(bytes(&l.s), proposed, "{line}");
    }
    assert!(l.peer.bodies().is_empty());
    assert!(
        matches!(l.s.consent_to(&id, "yes"), TurnOutcome::Facts(ref t) if t.contains("applied"))
    );
}

/// S05b: an open question at the proposal needs the route, so its one-time review is asked; the
/// door's interruption (`cancel`) or a typed `no` declines THAT call — the proposal waits again
/// with the same identity and bytes. Only the proposal's own `no` discards it (the targeted
/// cancellation), after which its identity answers nothing.
#[test]
fn a_declined_review_cancels_its_call_and_the_proposal_waits_unchanged() {
    let mut l = live();
    let TurnOutcome::Proposal { id, .. } = l.s.turn(COPY_INTENT) else {
        panic!("a deterministic proposal");
    };
    let proposed = bytes(&l.s);
    for decline in ["cancel", "no", "non, pas maintenant"] {
        asked(&l.s.consent("can you explain what it writes?"));
        let out = l.s.turn(decline);
        assert!(
            matches!(&out, TurnOutcome::Held { id: held, preview }
                if held == &id && preview.contains("cancelled; nothing sent") && preview.contains("still waits")),
            "{decline}: {out:?}"
        );
        assert!(!l.s.waiting_cost_choice());
        assert_eq!(l.s.pending_proposal().as_ref(), Some(&id), "{decline}");
        assert_eq!(bytes(&l.s), proposed, "{decline}");
    }
    assert!(
        l.peer.bodies().is_empty(),
        "a declined review sent a request"
    );
    assert!(matches!(l.s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    assert!(l.s.pending_proposal().is_none());
    assert!(matches!(
        l.s.consent_to(&id, "yes"),
        TurnOutcome::Refusal(ref r) if r.class == RefusalClass::AlreadyConsumed
    ));
    assert!(!l.dir.path().join(COPY_DEST).exists());
}

/// S02: at an open question the read-only asks and a command stage no review (the review used to
/// come before `why` was even read); open language asks its review, and declining it keeps the
/// question with the same identity — the round is never reset. (A skeleton's question carries no
/// replayed plan, so its open language needs the route and its review.)
#[test]
fn a_declined_review_at_a_question_keeps_the_question_and_its_identity() {
    let mut l = live();
    let TurnOutcome::Question { key, .. } = l.s.turn(SKELETON) else {
        panic!("the skeleton's question, no review");
    };
    assert!(
        l.s.authoring
            .as_ref()
            .is_some_and(|r| r.continuation.is_none())
    );
    let question = l.s.pending_question_id().expect("waits");
    for line in ["why", "what happened?", "/bogus", "/cancel please"] {
        let out = l.s.turn(line);
        assert!(
            !l.s.waiting_cost_choice(),
            "{line} staged a review: {out:?}"
        );
        assert_eq!(l.s.pending_question_id(), Some(question.clone()), "{line}");
    }
    asked(&l.s.turn("Use euros, the code EUR."));
    let out = l.s.turn("no");
    assert!(
        matches!(&out, TurnOutcome::Question { key: asked_key, question }
            if asked_key == &key && question.contains("cancelled; nothing sent")),
        "{out:?}"
    );
    assert_eq!(l.s.pending_question_id(), Some(question));
    assert_eq!(
        l.s.intent.goal.as_deref(),
        Some(SKELETON),
        "the goal was reset"
    );
    assert!(l.peer.bodies().is_empty());
    // The model question of a replayed plan: its read-only asks bind nothing either.
    let mut l = live();
    let TurnOutcome::Question { .. } = l.s.turn(DRAFT) else {
        panic!("the model question");
    };
    let question = l.s.pending_question_id().expect("waits");
    for line in ["what happened?", "/bogus", "hmm, which one"] {
        let _ = l.s.turn(line);
        assert_eq!(l.s.pending_question_id(), Some(question.clone()), "{line}");
    }
    assert!(l.peer.bodies().is_empty());
}

/// An exact skeleton: a deterministic question round with no replayed plan.
const SKELETON: &str = "aggregate-by-key";

/// A deterministic Ready intent over the project's notes (no review, no call).
const COPY_INTENT: &str = "Read ./notes/brief.md and write it to ./out/copy.md";

/// Where that proposal lands.
const COPY_DEST: &str = "compiled-workflow.nika";
