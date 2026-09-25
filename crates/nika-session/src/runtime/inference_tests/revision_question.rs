// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A revision that asks: the change names the old destination beside the new one, so the
//! compiler proves the substitution structural but leaves the old path's disposition to the
//! human (`gap.1`). The question is answered through the Session's authoring round — the same
//! EDIT (exact base, change, original request) replayed from its recorded plan with zero calls —
//! into the revised proposal; a cancel restores the proposal it revised, exactly. While the
//! question waits no consent reaches either proposal. Loopback seat only; nothing runs.
use super::*;
use crate::turn::RoutingMethod;
use nika_onboard::compile::{CompileRequest, revise_intent};

/// The human's change at the consent prompt: it names the old destination it replaces.
const CHANGE: &str = "Change the destination from ./sortie.txt to ./revised.txt";

/// The seat's revision: the base with the destination replaced, the old path declared a gap.
fn revised() -> String {
    let mut reply: Value =
        serde_json::from_str(&native().replace("./sortie.txt", "./revised.txt")).expect("reply");
    reply["gaps"] = json!(["./sortie.txt is superseded by the requested ./revised.txt"]);
    reply.to_string()
}

/// The loopback seat: the original candidate, then the seat's revision. Any further request
/// would be counted.
fn seat(revision: &str) -> Peer {
    Peer::start(vec![(200, response(&native())), (200, response(revision))])
}

/// The door's classifier: the change is a MODIFY wherever it is said; any other line is an
/// ANSWER at a question, new work at idle, and undecided at the consent prompt.
struct Acts;

impl Acts {
    fn decide(context: &TurnContext, raw: &str) -> TurnDecision {
        let act = match context.phase {
            _ if raw.trim() == CHANGE => TurnAct::Modify,
            SessionPhase::QuestionPending => TurnAct::Answer,
            SessionPhase::Idle => TurnAct::NewWork,
            _ => TurnAct::Unknown,
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

impl TurnClassifier for Acts {
    fn classify_with_admission(
        &mut self,
        context: &TurnContext,
        raw: &str,
        _account: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        Self::decide(context, raw)
    }

    fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
        Self::decide(context, raw)
    }
}

/// A session over the loopback seat, its classifier and an explicit Session allowance (the
/// loopback substitution admits bounded calls only).
fn session(dir: &Path, home: Option<&Path>) -> SessionRuntime {
    let mut s = open(dir);
    if let Some(home) = home {
        s.enable_history(home).expect("history");
    }
    s.with_classifier(Box::new(Acts));
    s.admit_money("budget 2 USD", false).expect("allowance");
    s
}

/// What the original proposal was when the revision's question was asked: its identity, its
/// change set and the compiler's reading it came from (the Meaning view).
struct Original {
    id: crate::outcome::ProposalId,
    set: crate::change::ProjectChangeSet,
    reading: String,
}

/// A session with the original proposal waiting and the revision's question asked.
fn asked(dir: &Path, home: Option<&Path>) -> (SessionRuntime, Original) {
    let mut s = session(dir, home);
    let out = s.turn(WORK);
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("the original proposal: {out:?}");
    };
    let was = Original {
        id,
        set: s.pending.clone().expect("the original waits"),
        reading: format!("{:?}", s.last_outcome),
    };
    let out = s.consent(CHANGE);
    let TurnOutcome::Question { key, question } = &out else {
        panic!("the revision asks its question: {out:?}");
    };
    assert_eq!(key, "gap.1");
    assert!(question.contains("./sortie.txt"), "{question}");
    (s, was)
}

/// The world a test can observe: the project's files and their bytes.
fn files(root: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir(root)
        .expect("root")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_file())
        .map(|p| {
            let bytes = std::fs::read_to_string(&p).unwrap_or_default();
            (p.display().to_string(), bytes)
        })
        .collect();
    found.sort();
    found
}

/// The first change's exact bytes.
fn bytes_of(set: &crate::change::ProjectChangeSet) -> String {
    set.changes[0].content().to_owned()
}

#[test]
fn a_revision_question_is_answered_into_the_revised_proposal_saved_only() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, was) = asked(dir.path(), None);
    assert_eq!(peer.bodies().len(), 2, "one call authored, one revised");
    let before = files(dir.path());
    // The question owns the next line; the proposal it revises waits aside, never consentable.
    assert_eq!(s.phase(), SessionPhase::QuestionPending);
    assert_eq!(s.pending_proposal(), None);
    assert!(s.status_line().starts_with("Needs one answer"));
    // The round asks the compiler's EDIT: the exact base, the change, the original request and
    // the recorded native revision it replays.
    let request = s.authoring.as_ref().expect("the revision round").request();
    let edit = CompileRequest::edit(bytes_of(&was.set), CHANGE);
    assert_eq!(format!("{:?}", request.input), format!("{:?}", edit.input));
    assert_eq!(request.original_intent.as_ref(), Some(&was.set.goal));
    assert!(request.plan.is_some(), "replayed, never authored afresh");
    assert_eq!(
        revise_intent(&request),
        revise_intent(&edit.with_original_intent(&was.set.goal)),
        "the folded intent the recorded plan is keyed by"
    );
    // A consent names no proposal while the question waits: neither applies.
    assert!(matches!(s.consent("yes"), TurnOutcome::Refusal(_)));
    assert!(matches!(
        s.consent_to(&was.id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(files(dir.path()), before, "nothing written");
    // The compiler's own disposition binds `gap.1`; the revision replays into its proposal.
    let out = s.turn("drop");
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal: {out:?}");
    };
    assert_ne!(*id, was.id);
    assert_eq!(peer.bodies().len(), 2, "an answer replays: no call");
    assert!(s.pending_question().is_none());
    let revision = s.pending.clone().expect("the revised proposal waits");
    let bytes = bytes_of(&revision);
    assert!(bytes.contains("./revised.txt"), "{bytes}");
    assert!(!bytes.contains("./sortie.txt"), "{bytes}");
    // Save only: the old identity is stale, the revised bytes land, nothing runs.
    assert!(matches!(
        s.consent_to(&was.id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    let out = s.consent_to(id, "yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "{out:?}"
    );
    let saved = std::fs::read_to_string(dir.path().join(revision.changes[0].path()));
    assert_eq!(saved.expect("saved"), bytes);
    assert!(!dir.path().join("revised.txt").exists());
    assert!(!dir.path().join("sortie.txt").exists());
    assert_eq!(peer.bodies().len(), 2);
}

#[test]
fn a_cancelled_revision_question_restores_the_proposal_it_revised() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, was) = asked(dir.path(), None);
    let out = s.turn("cancel");
    let TurnOutcome::Held { id, preview } = &out else {
        panic!("the original waits again: {out:?}");
    };
    assert_eq!(*id, was.id);
    assert!(preview.contains("still waits"), "{preview}");
    assert_eq!(s.pending.as_ref(), Some(&was.set), "the exact proposal");
    assert_eq!(s.pending_proposal().as_ref(), Some(&was.id));
    assert_eq!(s.phase(), SessionPhase::ProposalPending);
    assert!(s.pending_question().is_none());
    assert_eq!(format!("{:?}", s.last_outcome), was.reading, "its reading");
    // The restored proposal is the one consented: its exact bytes land, nothing runs.
    let out = s.consent_to(&was.id, "yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "{out:?}"
    );
    let saved = std::fs::read_to_string(dir.path().join(was.set.changes[0].path()));
    assert_eq!(saved.expect("saved"), bytes_of(&was.set));
    assert!(!dir.path().join("sortie.txt").exists());
    assert_eq!(peer.bodies().len(), 2);
}

/// A `yes` at the question is its answer, never a consent: the revised proposal is proposed
/// for review and the proposal it revised is never saved.
#[test]
fn a_yes_at_the_revision_question_never_saves_the_old_proposal() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, was) = asked(dir.path(), None);
    let before = files(dir.path());
    let out = s.turn("yes");
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal, for review: {out:?}");
    };
    assert_ne!(*id, was.id);
    assert_ne!(s.pending.as_ref(), Some(&was.set));
    assert_eq!(files(dir.path()), before, "nothing written");
    assert!(!dir.path().join(was.set.changes[0].path()).exists());
}

/// While the question waits the kept draft is the proposal it revises (evidence, no
/// authority): a session reopened over the same history can propose it again.
#[test]
fn a_waiting_revision_keeps_the_proposal_it_revises_as_the_draft() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    let (s, was) = asked(dir.path(), Some(home.path()));
    drop(s);
    let mut resumed = open(dir.path());
    let notice = resumed.enable_history(home.path()).expect("history");
    let kept = was.id.to_string();
    assert!(
        notice.as_deref().is_some_and(|n| n.contains(&kept)),
        "{notice:?}"
    );
    assert_eq!(resumed.restored_draft_id(), Some(kept.as_str()));
    let out = resumed.restore_draft();
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let again = resumed.pending.clone().expect("proposed again");
    assert_eq!(bytes_of(&again), bytes_of(&was.set));
    assert_eq!(peer.bodies().len(), 2);
}

/// Each disposition is its own question of the same EDIT (`gap.1`, then `gap.2`), and a change
/// said at a revision's question is its answer: never a fresh request read again (no call).
#[test]
fn every_disposition_and_a_change_at_the_question_stay_in_the_revision() {
    let mut two: Value = serde_json::from_str(&revised()).expect("reply");
    two["gaps"]
        .as_array_mut()
        .expect("gaps")
        .push(json!("the copy keeps its exact bytes"));
    let peer = seat(&two.to_string());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, was) = asked(dir.path(), None);
    let out = s.turn("drop");
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "gap.2"),
        "{out:?}"
    );
    assert_eq!(s.pending_proposal(), None);
    let out = s.turn(CHANGE);
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal: {out:?}");
    };
    assert_ne!(*id, was.id);
    let bytes = bytes_of(s.pending.as_ref().expect("revised"));
    assert!(bytes.contains("./revised.txt"), "{bytes}");
    assert_eq!(
        peer.bodies().len(),
        2,
        "answers replay: nothing read afresh"
    );
}

/// A saved workflow's revision asks through the same EDIT round: the saved bytes are its base,
/// the answer proposes the revision beside them, and nothing is written before a consent.
#[test]
fn a_saved_workflows_revision_question_is_answered_through_its_edit() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = session(dir.path(), None);
    assert!(matches!(s.turn(WORK), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    let saved = s.last_workflow.clone().expect("saved");
    let base = std::fs::read_to_string(dir.path().join(&saved)).expect("base");
    let out = s.revise_saved(&saved, CHANGE);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "gap.1"),
        "{out:?}"
    );
    let request = s.authoring.as_ref().expect("the revision round").request();
    let edit = CompileRequest::edit(base.clone(), CHANGE);
    assert_eq!(format!("{:?}", request.input), format!("{:?}", edit.input));
    assert!(request.plan.is_some());
    let out = s.turn("drop");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let bytes = bytes_of(s.pending.as_ref().expect("revised"));
    assert!(bytes.contains("./revised.txt"), "{bytes}");
    let now = std::fs::read_to_string(dir.path().join(&saved)).expect("still saved");
    assert_eq!(now, base, "nothing written before a consent");
    assert_eq!(peer.bodies().len(), 2);
}

/// A refused spending line at the question expires what waits, the set-aside proposal with it:
/// nothing it set aside comes back later or stays kept as the draft.
#[test]
fn a_refused_budget_at_the_question_expires_the_proposal_it_set_aside() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    let (mut s, was) = asked(dir.path(), Some(home.path()));
    assert!(matches!(s.turn("budget=NaN"), TurnOutcome::Refusal(_)));
    assert!(s.pending_question().is_none() && s.pending_proposal().is_none());
    assert!(matches!(
        s.consent_to(&was.id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    drop(s);
    let mut resumed = open(dir.path());
    resumed.enable_history(home.path()).expect("history");
    assert_eq!(resumed.restored_draft_id(), None, "no draft outlives it");
    assert_eq!(peer.bodies().len(), 2);
}
