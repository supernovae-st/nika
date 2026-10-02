// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native proof custody across a fresh EDIT. Compiler-result and room doubles isolate this
//! boundary; they do not claim that a model authored the fixture or that a Runtime ran it.

use super::*;
use crate::authoring::compile_deterministic;
use crate::change::ProjectChangeSet;
use crate::runtime::rehearsed::{ProofOrigin, selected};
use nika_onboard::compile::{CompileRequest, QuestionType, stated_destinations, stated_sources};

const DIRECT: &str = "Read ./in/source.txt and write it to ./out/copied.txt";
const CHANGE: &str = "Keep the text unchanged and revise the workflow description";

fn native_pending(
    root: &Path,
) -> (
    SessionRuntime,
    Arc<AtomicUsize>,
    ProposalId,
    ProjectChangeSet,
) {
    let (mut s, calls) = session(root, Mode::Passed);
    assert_eq!(stated_sources(DIRECT), vec!["./in/source.txt".to_owned()]);
    assert_eq!(
        stated_destinations(DIRECT),
        vec!["./out/copied.txt".to_owned()]
    );
    let round = AuthoringRound::new(DIRECT);
    assert!(
        s.rehearse_copy(&round)
            .expect("HARNESS_INVALID: NotCopy")
            .is_none()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    // These exact checked copy bytes are the compiler-result double for the oriented request.
    // The assertion above proves this request cannot be replaced by the closed Copy selector.
    let out = prepared(&s);
    let out = observed(&mut s, &round, out, false).expect("coherent passed native report");
    let (id, preview) = proposal(s.settle(round, Reading::Ready(out)));
    assert!(preview.contains("observed result of this candidate"));
    let (_, proof) = s.rehearsals.pending.as_ref().expect("typed proof");
    assert!(proof.origin == ProofOrigin::Native);
    assert!(
        s.basis.is_some(),
        "the old source basis must travel with its proof"
    );
    let set = s.pending.clone().expect("proposal");
    assert!(selected(&set, &proof.witness));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 1);
    (s, calls, id, set)
}

fn edit_round(set: &ProjectChangeSet) -> AuthoringRound {
    let mut round = AuthoringRound::new(format!("{} — {CHANGE}", set.goal));
    round.edit = Some((
        set.changes[0].content().to_owned(),
        CHANGE.into(),
        Some(set.goal.clone()),
    ));
    round
}

fn changed(s: &SessionRuntime) -> CompileOutcome {
    let mut out = prepared(s);
    // Distinct bytes exercise identity binding only, not behavioural diversity.
    out.candidate = out
        .candidate
        .map(|source| format!("# revised description\n{source}"));
    out
}

fn question(s: &SessionRuntime) -> CompileOutcome {
    let mut out = prepared(s);
    let skeleton = compile_deterministic(&CompileRequest::create("bounded-batch"))
        .expect("HARNESS_INVALID: question fixture");
    let mut q = skeleton
        .questions
        .first()
        .expect("HARNESS_INVALID: question")
        .clone();
    q.key = "gap.1".into();
    q.label = "Keep the previous description or drop it?".into();
    q.why = "The revised description leaves a clause to decide".into();
    q.answer_type = QuestionType::Text;
    q.options.clear();
    q.mandatory = true;
    out.status = CompileStatus::Incomplete;
    out.questions = vec![q];
    out.provenance.plan = None;
    out
}

fn ask(s: &mut SessionRuntime, set: ProjectChangeSet) {
    let expected = edit_round(&set).request();
    s.pending = None;
    let result = s.revise_pending_with(set, CHANGE, |this, request, _| {
        assert_eq!(
            format!("{:?}", request.input),
            format!("{:?}", expected.input)
        );
        assert_eq!(request.original_intent, expected.original_intent);
        Ok(question(this))
    });
    assert!(
        matches!(result, TurnOutcome::Question { ref key, .. } if key == "gap.1"),
        "{result:?}"
    );
    assert!(s.pending.is_none());
    assert!(s.rehearsals.pending.is_none());
    assert!(s.rehearsals.revising.is_some());
    assert!(s.basis.is_none());
}

#[test]
fn native_revision_gets_its_own_proof_and_rejects_old_consent() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    let round = edit_round(&set);
    let expected = round.request();
    s.pending = None;
    let result = s.revise_pending_with(set, CHANGE, |this, request, intent| {
        assert_eq!(
            format!("{:?}", request.input),
            format!("{:?}", expected.input)
        );
        assert_eq!(request.original_intent, expected.original_intent);
        assert_eq!(intent, round.effective_intent());
        let out = changed(this);
        observed(this, &round, out, false)
    });
    let (new, preview) = proposal(result);
    assert_ne!(old, new);
    assert!(preview.contains("observed result of this candidate"));
    let (proven, proof) = s.rehearsals.pending.as_ref().expect("new proof");
    assert_eq!(proven, &new);
    assert!(proof.origin == ProofOrigin::Native);
    assert!(selected(
        s.pending
            .as_ref()
            .expect("synthetic fixture must be available"),
        &proof.witness
    ));
    assert!(s.rehearsals.revising.is_none());
    assert_eq!(s.rehearsals.turn.attempts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        refused(s.consent_to(&old, "yes")).class,
        RefusalClass::StaleRevision
    );
    assert_eq!(s.pending_proposal(), Some(new));
    assert!(!root.path().join(LANDED).exists());
    assert!(!root.path().join("out/copied.txt").exists());
    assert_eq!(
        std::fs::read_to_string(root.path().join(SOURCE))
            .expect("synthetic fixture must be available"),
        USER
    );
}

#[test]
fn native_question_suspends_old_consent_and_cancel_restores_fresh_custody() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    let bytes = set.changes[0].content().to_owned();
    ask(&mut s, set);
    assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
    let result = s.turn("cancel");
    assert!(
        matches!(result, TurnOutcome::Held { ref id, .. } if id == &old),
        "{result:?}"
    );
    assert_eq!(s.pending_proposal(), Some(old.clone()));
    assert_eq!(pending_bytes(&s), bytes);
    assert!(s.rehearsed_pending(&old));
    assert!(s.rehearsals.revising.is_none());
    assert!(s.basis.is_some());
    assert_eq!(
        s.rehearsals.turn.attempts, 1,
        "cancel must not reset spending"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!root.path().join(LANDED).exists());
}

#[test]
fn native_answer_boundary_rehearses_fresh_bytes_and_keeps_the_account() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    ask(&mut s, set);
    // The normal answer turn expires transient state but preserves this revision's account.
    s.rehearsals.new_turn(true);
    let mut round = s.authoring.take().expect("EDIT question");
    assert!(round.edit.is_some());
    assert_eq!(round.answer_current("keep").as_deref(), Some("gap.1"));
    let out = changed(&s);
    let out = observed(&mut s, &round, out, false).expect("fresh answer report");
    let result = s.settle(round, Reading::Ready(out));
    let (new, _) = proposal(s.keep_revising(result));
    assert_ne!(old, new);
    assert!(s.rehearsed_pending(&new));
    assert!(s.rehearsals.revising.is_none());
    assert_eq!(s.rehearsals.turn.attempts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        refused(s.consent_to(&old, "yes")).class,
        RefusalClass::StaleRevision
    );
    assert!(!root.path().join(LANDED).exists());
}

#[test]
fn native_error_restores_old_proof_but_never_refunds_the_failed_dispatch() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    let round = edit_round(&set);
    s.pending = None;
    let result = s.revise_pending_with(set, CHANGE, |this, _, _| {
        let out = changed(this);
        observed(this, &round, out, true)
    });
    assert!(matches!(result, TurnOutcome::Refusal(_)));
    assert_eq!(s.pending_proposal(), Some(old.clone()));
    assert!(s.rehearsed_pending(&old));
    assert!(s.basis.is_some());
    assert_eq!(s.rehearsals.turn.attempts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn native_cancel_with_changed_originals_withdraws_instead_of_restoring() {
    for destination in [false, true] {
        let root = project();
        let (mut s, calls, old, set) = native_pending(root.path());
        ask(&mut s, set);
        if destination {
            write(root.path(), "out/copied.txt", "appeared");
        } else {
            write(root.path(), SOURCE, EDITED);
        }
        assert_eq!(refused(s.turn("cancel")).class, RefusalClass::StaleRevision);
        assert!(s.pending.is_none());
        assert!(s.rehearsals.pending.is_none());
        assert!(s.rehearsals.revising.is_none());
        assert!(s.basis.is_none());
        assert!(matches!(s.consent_to(&old, "yes"), TurnOutcome::Refusal(_)));
        assert_eq!(s.rehearsals.turn.attempts, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!root.path().join(LANDED).exists());
    }
}

#[test]
fn native_revision_without_new_live_evidence_cannot_borrow_the_old_proof() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    s.pending = None;
    // This double returns different static bytes with no host call. It can be source-only,
    // but the old proof must never follow those bytes.
    let result = s.revise_pending_with(set, CHANGE, |this, _, _| Ok(changed(this)));
    let (new, preview) = proposal(result);
    assert_ne!(old, new);
    assert!(!s.rehearsed_pending(&new));
    assert!(!preview.contains("observed result of this candidate"));
    assert!(s.rehearsals.revising.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.rehearsals.turn.attempts, 1);
}

#[test]
fn rejected_revision_binding_restores_the_old_basis_without_transferring_proof() {
    let root = project();
    let (mut s, calls, old, set) = native_pending(root.path());
    let bytes = set.changes[0].content().to_owned();
    let round = edit_round(&set);
    s.pending = None;
    let result = s.revise_pending_with(set, CHANGE, |this, _, _| {
        let out = changed(this);
        let mut out = observed(this, &round, out, false)?;
        // Deliberately corrupt a compiler-result double after its live report: the proposal
        // binder must reject it after bind_basis has seen the new request.
        out.candidate = out
            .candidate
            .map(|source| format!("# not witnessed\n{source}"));
        Ok(out)
    });
    assert!(matches!(result, TurnOutcome::Refusal(_)));
    assert_eq!(s.pending_proposal(), Some(old.clone()));
    assert_eq!(pending_bytes(&s), bytes);
    assert!(s.rehearsed_pending(&old));
    assert!(s.basis.is_some());
    assert!(s.rehearsals.revising.is_none());
    assert_eq!(s.rehearsals.turn.attempts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn copy_proof_still_holds_without_entering_the_native_edit_boundary() {
    let root = project();
    let (mut s, calls) = session(root.path(), Mode::Passed);
    let (id, _) = proposal(s.turn(INTENT));
    let (proven, proof) = s.rehearsals.pending.as_ref().expect("copy proof");
    assert_eq!(proven, &id);
    assert!(proof.origin == ProofOrigin::Copy);
    let before = calls.load(Ordering::SeqCst);
    let spent = s.rehearsals.turn;
    let set = s.pending.take().expect("copy proposal");
    let result = s.revise_pending_with(set, CHANGE, |_, _, _| panic!("Copy must remain held"));
    assert!(matches!(result, TurnOutcome::Held { id: ref held, .. } if held == &id));
    assert_eq!(s.pending_proposal(), Some(id));
    assert_eq!(s.rehearsals.turn, spent);
    assert_eq!(calls.load(Ordering::SeqCst), before);
}
