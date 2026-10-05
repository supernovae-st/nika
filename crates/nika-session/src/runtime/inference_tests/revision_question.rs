// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A revision that asks: the change offers two new destinations, so the compiler asks which
//! path the human means (`revision.path`). CREATE still uses plan, sketch, fills and judge;
//! EDIT answers typed clause links, never whole source. The answer replays the exact base and
//! links through the compiler's source substitution, then asks only the permitted round's judge.
//! Cancel restores the proposal it revised exactly. While a question waits no consent reaches
//! either proposal. Loopback seat only; nothing runs.
use super::*;
use crate::turn::RoutingMethod;
use nika_onboard::compile::{CompileRequest, revise_intent};

/// A real unresolved choice: either path is new, and no seat chooses between them.
const CHANGE: &str = "Change the destination from ./sortie.txt to ./revised.txt or ./reviewed.txt";
const TWO_OUTPUTS: &str = "Je veux que sortie.txt et secondaire.txt contiennent exactement les octets présents dans entree.txt.";
const TWO_CHOICES: &str = "Change one destination to ./revised.txt or ./reviewed.txt";

/// The typed links copy the exact clauses; the compiler writes the revised bytes.
fn links(original: &str, change: &str) -> String {
    json!({"supersedes": [{"replaces": original.trim_end_matches('.'),
        "by": change.trim_end_matches('.')}], "adds": [], "notes": "replace one destination"})
    .to_string()
}

fn revised() -> String {
    links(WORK, CHANGE)
}

/// A semantic CREATE of the copy into `destination` (plan, sketch, fills), then its judgment.
fn created(destination: &str) -> Vec<(u16, Value)> {
    let mut script: Vec<_> = (semantic_copy(destination).iter())
        .map(|t| (200, response(t)))
        .collect();
    script.push((200, response(JUDGE_APPROVES)));
    script
}

/// The loopback seat: semantic CREATE and its judge, typed revision links, then the judge of
/// the answered revision. Questions replay without another authoring call.
fn seat(revision: &str) -> Peer {
    let mut script = authored(response);
    script.extend([(200, response(revision)), (200, response(JUDGE_APPROVES))]);
    Peer::start(script)
}

/// Whether a request the seat received is the judge's closed choice (faithful · unfaithful).
fn judged(body: &Value) -> bool {
    body.to_string().contains("unfaithful")
}

/// The door's classifier: the change is a MODIFY wherever it is said; any other line is an
/// ANSWER at a question, new work at idle, and undecided at the consent prompt.
struct Acts;

impl Acts {
    fn decide(context: &TurnContext, raw: &str) -> TurnDecision {
        let act = match context.phase {
            _ if matches!(raw.trim(), CHANGE | TWO_CHOICES | "./revised.txt") => TurnAct::Modify,
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
    s.admit_money("budget 2 USD", false, false)
        .expect("allowance");
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
    assert_eq!(key, "revision.path");
    assert!(
        question.contains("./revised.txt") && question.contains("./reviewed.txt"),
        "{question}"
    );
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
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 1,
        "the semantic CREATE authored and judged, one revised"
    );
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
    // The chosen path binds the real revision question; the compiler writes the proposal.
    let out = s.turn("./revised.txt");
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal: {out:?}");
    };
    assert_ne!(*id, was.id);
    let bodies = peer.bodies();
    assert_eq!(
        bodies.len(),
        CREATE_CALLS + 2,
        "an answer replays: its one call is the round's judge"
    );
    let (revision, judge) = (&bodies[CREATE_CALLS], &bodies[CREATE_CALLS + 1]);
    assert!(judged(judge) && !judged(revision), "{judge:#}");
    assert!(s.pending_question().is_none());
    let revision = s.pending.clone().expect("the revised proposal waits");
    let bytes = bytes_of(&revision);
    assert!(bytes.contains("revised.txt"), "{bytes}");
    assert!(!bytes.contains("sortie.txt"), "{bytes}");
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
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
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
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 1);
}

/// A `yes` at a path choice is no offered path and no consent: the question stays open,
/// with the original proposal still set aside and never silently saved.
#[test]
fn a_yes_at_the_revision_question_never_saves_the_old_proposal() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, was) = asked(dir.path(), None);
    let before = files(dir.path());
    let out = s.turn("yes");
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    assert_eq!(s.pending_proposal(), None);
    assert_eq!(
        &s.revising.as_ref().expect("base stays set aside").0,
        &was.set
    );
    assert!(matches!(
        s.consent_to(&was.id, "yes"),
        TurnOutcome::Refusal(_)
    ));
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 1,
        "no judge or new authoring"
    );
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
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 1);
}

/// Two unresolved choices are two questions of the same EDIT: which original destination,
/// then which new path. Even a line the classifier would label MODIFY answers that question.
#[test]
fn every_destination_question_and_a_change_at_the_question_stay_in_the_revision() {
    let [_, raw_graph, fills] = semantic_copy("sortie.txt");
    let mut graph: Value = serde_json::from_str(&raw_graph).expect("graph");
    let mut second = graph["tasks"][1].clone();
    second["id"] = json!("write_second");
    second["writes"] = json!(["secondaire.txt"]);
    graph["tasks"].as_array_mut().expect("tasks").push(second);
    let plan = json!({"steps": [], "effects": [], "obligations": [], "constraints": [],
        "unknowns": [TWO_OUTPUTS.trim_end_matches('.')], "regions": [],
        "approval_bypass": {"present": false}});
    let replies = [
        plan.to_string(),
        graph.to_string(),
        fills,
        JUDGE_APPROVES.to_owned(),
        links(TWO_OUTPUTS, TWO_CHOICES),
        JUDGE_APPROVES.to_owned(),
    ];
    let peer = Peer::start(replies.iter().map(|r| (200, response(r))).collect());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = session(dir.path(), None);
    let TurnOutcome::Proposal { id: original, .. } = s.turn(TWO_OUTPUTS) else {
        panic!("the two-output base must be proposed");
    };
    let out = s.consent(TWO_CHOICES);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.destination"),
        "{out:?}"
    );
    let out = s.turn("sortie.txt");
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    assert_eq!(s.pending_proposal(), None);
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 1,
        "the first answer only replays"
    );
    let out = s.turn("./revised.txt");
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal: {out:?}");
    };
    assert_ne!(*id, original);
    let bytes = bytes_of(s.pending.as_ref().expect("revised"));
    assert!(
        bytes.contains("revised.txt") && bytes.contains("secondaire.txt"),
        "{bytes}"
    );
    assert!(!bytes.contains("sortie.txt"), "{bytes}");
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 2,
        "answers replay: nothing read afresh, the finishing round's judge alone"
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
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    let request = s.authoring.as_ref().expect("the revision round").request();
    let edit = CompileRequest::edit(base.clone(), CHANGE);
    assert_eq!(format!("{:?}", request.input), format!("{:?}", edit.input));
    assert!(request.plan.is_some());
    let out = s.turn("./revised.txt");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let bytes = bytes_of(s.pending.as_ref().expect("revised"));
    assert!(bytes.contains("revised.txt"), "{bytes}");
    let now = std::fs::read_to_string(dir.path().join(&saved)).expect("still saved");
    assert_eq!(now, base, "nothing written before a consent");
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
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
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 1);
}

// ── A saved workflow's revision keeps its target, its route and its money ─────────────────────
//
// The revision of a SAVED workflow is that file's next version: the same path, a witnessed
// update over the exact base the compiler revised, never a numbered twin beside it. Every seat
// here is the loopback peer; every Run is a simulated observation (`observe_run`), never an
// executed workflow. The scripted candidates isolate Session: they prove no semantic capability.

use crate::change::{ProjectChange, Witness};
use crate::money::MonetarySource;

/// A correction of the saved workflow said in French, with no amount (synthetic DEV words).
const CORRECTION: &str = "Remplace ./sortie.txt par ./revised.txt ou ./reviewed.txt";
/// A distinct new automation, said while the first one is saved (synthetic DEV words).
const OTHER_WORK: &str =
    "Je veux que copie.txt contienne exactement les octets présents dans entree.txt.";
/// A line the classifier cannot tell.
const AMBIGUOUS: &str = "Et pour le fichier de sortie, plutôt la version de la semaine.";
/// A second correction, said at the consent prompt of saved A's waiting update.
const FURTHER: &str =
    "Change the destination from ./revised.txt to ./final.txt or ./other-final.txt";
/// Work the deterministic reader cannot settle alone and the classifier cannot tell.
const UNDECIDED_WORK: &str = "Corrige le workflow enregistré : écris dans ./archive.txt au lieu de ./sortie.txt, garde tout le reste.";

/// The door's classifier for these witnesses: the correction is a MODIFY, the other work new
/// work, the ambiguous line UNKNOWN; at a question any line answers.
struct Revisions;

impl Revisions {
    fn decide(context: &TurnContext, raw: &str) -> TurnDecision {
        let act = match (context.phase, raw.trim()) {
            (SessionPhase::QuestionPending, _) => TurnAct::Answer,
            (_, line) if line == CORRECTION || line == CHANGE || line == FURTHER => TurnAct::Modify,
            (_, line) if line == OTHER_WORK || line == WORK => TurnAct::NewWork,
            _ => TurnAct::Unknown,
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

impl TurnClassifier for Revisions {
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

/// A session with workflow A proposed from `work` and saved; returns its path and bytes.
fn saved_a(dir: &Path, work: &str) -> (SessionRuntime, std::path::PathBuf, String) {
    saved_a_kept(dir, None, work)
}

/// [`saved_a`], its history kept under `home` when given (enabled before the first turn).
fn saved_a_kept(
    dir: &Path,
    home: Option<&Path>,
    work: &str,
) -> (SessionRuntime, std::path::PathBuf, String) {
    let mut s = open(dir);
    if let Some(home) = home {
        s.enable_history(home).expect("history");
    }
    s.with_classifier(Box::new(Revisions));
    s.admit_money("budget 2 USD", false, false)
        .expect("allowance");
    let out = s.turn(work);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "A: {out:?}");
    let out = s.consent("yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "save A: {out:?}"
    );
    let saved = s.last_workflow.clone().expect("A saved");
    let base = std::fs::read_to_string(dir.join(&saved)).expect("A bytes");
    (s, saved, base)
}

/// The saved file's revision asked through its EDIT and answered by the chosen path.
fn revised_through_question(s: &mut SessionRuntime, saved: &Path) -> TurnOutcome {
    let out = s.revise_saved(saved, CHANGE);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    s.turn("./revised.txt")
}

/// The workflow files in the project root.
fn nika_files(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(root)
        .expect("root")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_file())
        .map(|p| p.file_name().expect("name").to_string_lossy().into_owned())
        .filter(|n| {
            Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nika"))
        })
        .collect();
    found.sort();
    found
}

/// The waiting proposal is an update of `saved` over exactly `base`.
fn assert_updates(s: &SessionRuntime, saved: &Path, base: &str) {
    let set = s.pending.as_ref().expect("a proposal waits");
    match &set.changes[0] {
        ProjectChange::UpdateWorkflow { path, before, .. } => {
            assert_eq!(path, saved, "the saved file, never a twin");
            assert_eq!(
                *before,
                Witness::of(base.as_bytes()),
                "over the compiled base"
            );
        }
        other => panic!("a revision of the saved workflow must update it: {other:?}"),
    }
}

/// F-D1: the revision of the saved workflow updates that file over its exact base; nothing is
/// written before the consent, the consent saves the revised bytes in place, nothing runs.
#[test]
fn a_saved_workflows_revision_is_saved_in_place_and_runs_nothing() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = revised_through_question(&mut s, &saved);
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("the revised proposal: {out:?}");
    };
    assert_updates(&s, &saved, &base);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
    let revised = bytes_of(s.pending.as_ref().expect("revised"));
    let out = s.consent_to(id, "yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        revised
    );
    assert_eq!(
        nika_files(dir.path()).len(),
        1,
        "no twin: {:?}",
        nika_files(dir.path())
    );
    assert!(!dir.path().join("revised.txt").exists() && !dir.path().join("sortie.txt").exists());
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
}

/// F-D1 (question): the answered EDIT proposes the update of the saved file over its base.
#[test]
fn a_saved_workflows_answered_revision_updates_the_same_file() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = s.revise_saved(&saved, CHANGE);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    let out = s.turn("./revised.txt");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    assert_updates(&s, &saved, &base);
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 2);
}

/// F-D2: a base that moves while the revision's question waits is refused at the proposal —
/// no update over bytes the compiler never read, no twin created instead, nothing written.
#[test]
fn a_base_changed_during_the_revision_is_refused_never_proposed_beside_it() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = s.revise_saved(&saved, CHANGE);
    assert!(matches!(&out, TurnOutcome::Question { .. }), "{out:?}");
    let moved = format!("{base}# edited by hand\n");
    std::fs::write(dir.path().join(&saved), &moved).expect("hostile edit");
    let out = s.turn("./revised.txt");
    assert!(
        !matches!(out, TurnOutcome::Proposal { .. }),
        "a moved base is never proposed over: {out:?}"
    );
    assert!(s.pending.is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        moved
    );
    assert_eq!(
        nika_files(dir.path()).len(),
        1,
        "{:?}",
        nika_files(dir.path())
    );
}

/// F-D2: a base changed after the proposal is refused at the consent, nothing replaced.
#[test]
fn a_base_changed_after_the_revision_proposal_refuses_its_consent() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = revised_through_question(&mut s, &saved);
    let TurnOutcome::Proposal { id, .. } = &out else {
        panic!("{out:?}");
    };
    let moved = format!("{base}# edited by hand\n");
    std::fs::write(dir.path().join(&saved), &moved).expect("hostile edit");
    let out = s.consent_to(id, "yes");
    assert!(matches!(out, TurnOutcome::Refusal(_)), "stale: {out:?}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        moved
    );
    assert_eq!(
        nika_files(dir.path()).len(),
        1,
        "{:?}",
        nika_files(dir.path())
    );
}

/// F-R1: the correction said at Idle after Save and a (simulated) Run observation reaches the
/// classifier, routes MODIFY and revises the saved file through its EDIT — never a CREATE.
#[test]
fn a_correction_after_save_and_run_is_routed_as_the_saved_files_revision() {
    let peer = seat(&links(WORK, CORRECTION));
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let _ = s.observe_run(0, None);
    let routes = s.routes().len();
    let out = s.turn(CORRECTION);
    let consulted: Vec<_> = s.routes()[routes..].to_vec();
    assert!(
        consulted
            .iter()
            .any(|r| r.phase == SessionPhase::Idle && r.act == TurnAct::Modify),
        "the classifier read the correction: {consulted:?} · {out:?}"
    );
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    let out = s.turn("./revised.txt");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    assert_updates(&s, &saved, &base);
    let edit = peer.bodies()[CREATE_CALLS].to_string();
    assert!(
        edit.contains("sortie.txt"),
        "the EDIT carries the base: {edit}"
    );
}

/// F-R2: distinct new work said while A is saved keeps its own fresh destination (a CREATE).
#[test]
fn new_work_beside_a_saved_workflow_stays_a_fresh_creation() {
    let peer = Peer::start([created("sortie.txt"), created("copie.txt")].concat());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = s.turn(OTHER_WORK);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let set = s.pending.as_ref().expect("new work waits");
    assert!(
        matches!(&set.changes[0], ProjectChange::CreateWorkflow { path, .. } if *path != saved),
        "{:?}",
        set.changes[0]
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
}

/// What a turn must leave as it was: the goal, the reading the candidate came from, the saved
/// workflow it targets, and that nothing waits.
fn kept_state(s: &SessionRuntime) -> (Option<String>, String, Option<std::path::PathBuf>, bool) {
    let waits = s.pending.is_some() || s.authoring.is_some();
    (
        s.intent.goal.clone(),
        format!("{:?}", s.last_outcome),
        s.last_workflow.clone(),
        waits,
    )
}

/// F-R2: work the reader could not settle and the classifier cannot tell, said while A is saved:
/// the route keeps everything — no request at all, no goal, candidate or target changed, nothing
/// written — and says so.
#[test]
fn an_undecided_work_line_beside_a_saved_workflow_changes_nothing() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let calls = peer.bodies().len();
    let before = kept_state(&s);
    let out = s.turn(UNDECIDED_WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("nothing changed")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), calls, "no request");
    assert_eq!(kept_state(&s), before);
    assert!(
        s.routes()
            .last()
            .is_some_and(|r| r.phase == SessionPhase::Idle && r.act == TurnAct::Unknown)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
    assert_eq!(nika_files(dir.path()).len(), 1);
}

/// F-R2 (control): a line that reads as no work keeps today's conversational answer beside A —
/// never an authoring request, no goal, candidate or target changed, nothing written.
#[test]
fn a_conversational_line_beside_a_saved_workflow_keeps_its_answer() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let calls = peer.bodies().len();
    let before = kept_state(&s);
    let out = s.turn(AMBIGUOUS);
    assert!(!matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let authored = peer.bodies()[calls..]
        .iter()
        .any(|b| b.to_string().contains("candidate_lines"));
    assert!(!authored, "no authoring call: {out:?}");
    assert_eq!(kept_state(&s), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
    assert_eq!(nika_files(dir.path()).len(), 1);
}

/// A classifier whose label failed (a blank or transport failure): a decision, never a route.
struct FailedLabels;

impl TurnClassifier for FailedLabels {
    fn classify_with_admission(
        &mut self,
        _context: &TurnContext,
        _raw: &str,
        _account: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        TurnDecision::failed("the label came back blank")
    }

    fn classify(&mut self, _context: &TurnContext, _raw: &str) -> TurnDecision {
        TurnDecision::failed("the label came back blank")
    }
}

/// The undecided work line beside A, under `classifier` (`None`: no door and no factory, a
/// genuinely absent classifier): everything kept, nothing sent, the route recorded with `method`.
fn undecided_keeps_everything(classifier: Option<Box<dyn TurnClassifier>>, method: RoutingMethod) {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    s.classifier = classifier;
    if s.classifier.is_none() {
        s.factory = None;
    }
    let calls = peer.bodies().len();
    let before = kept_state(&s);
    let out = s.turn(UNDECIDED_WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("nothing changed")),
        "{out:?}"
    );
    assert_eq!(
        peer.bodies().len(),
        calls,
        "no request, no authoring fallback"
    );
    assert_eq!(kept_state(&s), before);
    assert!(s.routes().last().is_some_and(|r| r.method == method));
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
}

/// F-R2: a failed (blank or transport) label beside A keeps everything — never a creation.
#[test]
fn a_failed_label_beside_a_saved_workflow_changes_nothing() {
    undecided_keeps_everything(Some(Box::new(FailedLabels)), RoutingMethod::Failed);
}

/// F-R2: no classifier at all beside A keeps everything — never a creation.
#[test]
fn an_absent_classifier_beside_a_saved_workflow_changes_nothing() {
    undecided_keeps_everything(None, RoutingMethod::Fallback);
}

/// F-R2: a refusal an EARLIER turn left on the shared account is not this label's: a failed
/// label after it keeps everything (no admission card borrowed from that refusal, no request).
#[test]
fn a_refusal_left_by_an_earlier_turn_is_not_a_refused_label() {
    let peer = Peer::start([created("sortie.txt"), created("copie.txt")].concat());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    // A ceiling too small for any reservation: the next authoring call is refused before a send.
    s.admit_money("budget 0.01 USD", false, false)
        .expect("tiny allowance");
    let out = s.turn(OTHER_WORK);
    assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
    let left = s.inference_receipt().expect("receipt").expect("account");
    assert!(left.refusal.is_some(), "a refusal is left on the account");
    let calls = peer.bodies().len();
    s.with_classifier(Box::new(FailedLabels));
    let before = kept_state(&s);
    let out = s.turn(UNDECIDED_WORK);
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("nothing changed")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), calls);
    assert_eq!(kept_state(&s), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
}

/// F-M1: the saved workflow's explicit ceiling rides its revision (no amount said), bound to
/// the exact saved bytes; the shared account is neither replaced nor refilled.
#[test]
fn a_revision_without_an_amount_keeps_the_saved_workflows_ceiling() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, _) = saved_a(dir.path(), &format!("{WORK} budget 5 USD."));
    assert_eq!(
        s.monetary_decision().and_then(|d| d.effective_usd),
        Some(5.0)
    );
    let limit = s
        .inference_receipt()
        .expect("receipt")
        .expect("account")
        .limit;
    // The production order: the line's money is admitted before it is routed as a revision.
    s.admit_money(CHANGE, false, false).expect("no amount");
    let out = revised_through_question(&mut s, &saved);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let money = s.monetary_decision().expect("bound");
    assert_eq!(
        money.effective_usd,
        Some(5.0),
        "the saved ceiling: {money:?}"
    );
    assert_ne!(money.source, MonetarySource::SessionDefault, "{money:?}");
    let after = s.inference_receipt().expect("receipt").expect("account");
    assert_eq!(after.limit, limit, "the account is never refilled");
}

/// F-M1 (control): new work said beside the saved workflow keeps its own default.
#[test]
fn new_work_without_an_amount_keeps_its_own_default() {
    let peer = Peer::start([created("sortie.txt"), created("copie.txt")].concat());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, _, _) = saved_a(dir.path(), &format!("{WORK} budget 5 USD."));
    let out = s.turn(OTHER_WORK);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    let money = s.monetary_decision().expect("its own");
    assert_eq!(money.source, MonetarySource::SessionDefault, "{money:?}");
}

/// F-M1: a ceiling saved with other bytes than the ones the revision reads binds nothing: the
/// revision without an amount is refused before any call, never given a default instead.
#[test]
fn a_ceiling_saved_with_other_bytes_refuses_the_revision_before_any_call() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), &format!("{WORK} budget 5 USD."));
    let moved = format!("{base}# edited by hand\n");
    std::fs::write(dir.path().join(&saved), &moved).expect("hand edit");
    let calls = peer.bodies().len();
    let out = s.revise_saved(&saved, CHANGE);
    // The refusal speaks of this revision, never of running.
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("monetary decision")
            && r.text.contains("this revision cannot inherit its ceiling")
            && !r.text.contains("running")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), calls, "nothing sent");
    assert!(s.pending.is_none() && s.authoring.is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        moved
    );
    // The Run law keeps its own words over the same moved bytes: review before running.
    let run = s.turn("run it");
    assert!(
        matches!(&run, TurnOutcome::Refusal(r)
            if r.text.contains("prepare and review the revision before running")),
        "{run:?}"
    );
    assert_eq!(peer.bodies().len(), calls, "nothing sent");
}

/// F-M1: the saved ceiling binds a resolved identity, not a name: the same path now resolving
/// elsewhere (identical bytes) binds nothing, and the revision is refused before any call.
#[cfg(unix)]
#[test]
fn a_ceiling_saved_for_another_resolved_file_refuses_the_revision() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), &format!("{WORK} budget 5 USD."));
    std::fs::create_dir(dir.path().join("elsewhere")).expect("dir");
    std::fs::write(dir.path().join("elsewhere/copy.nika"), &base).expect("same bytes");
    std::fs::remove_file(dir.path().join(&saved)).expect("unlink");
    std::os::unix::fs::symlink(
        dir.path().join("elsewhere/copy.nika"),
        dir.path().join(&saved),
    )
    .expect("link");
    let calls = peer.bodies().len();
    let out = s.revise_saved(&saved, CHANGE);
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("monetary decision")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), calls, "nothing sent");
    assert!(s.pending.is_none() && s.authoring.is_none());
}

/// F-M1: an invalid amount in the revision refuses before any call; the saved file stays.
#[test]
fn an_invalid_amount_in_a_revision_refuses_before_any_call() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let calls = peer.bodies().len();
    let out = s.turn(&format!("{CORRECTION}. Budget NaN USD."));
    assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
    assert_eq!(peer.bodies().len(), calls);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
}

/// F-C1: the round a saved file's revision keeps names the file it revises — the base it read —
/// never whatever workflow a later turn selected; its continuation is that file's EDIT.
#[test]
fn a_kept_revision_round_names_the_file_it_revises() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    let out = s.revise_saved(&saved, CHANGE);
    assert!(matches!(&out, TurnOutcome::Question { .. }), "{out:?}");
    // A later selection (another saved workflow) must not retarget the waiting revision.
    s.last_workflow = Some(std::path::PathBuf::from("other.nika"));
    let kept = s.round_to_keep().expect("kept");
    let record = nika_onboard::compile::round::RoundReading::from_raw(kept);
    let record = record.record().expect("read");
    let edit = record.edit.as_ref().expect("an EDIT");
    assert_eq!(
        edit.path.as_ref().map(|p| p.text.as_str()),
        Some(saved.display().to_string().as_str()),
        "the revised file"
    );
    assert_eq!(edit.base.text, base);
}

/// F-C1: the kept round over a base changed before the reopen is refused, never re-created.
#[test]
fn a_kept_revision_round_over_a_changed_base_is_refused() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    let (mut s, saved, base) = saved_a_kept(dir.path(), Some(home.path()), WORK);
    let out = s.revise_saved(&saved, CHANGE);
    assert!(matches!(&out, TurnOutcome::Question { .. }), "{out:?}");
    // A recorded turn at the question keeps the round (the direct call above records none).
    let _ = s.turn("why");
    assert!(s.authoring.is_some(), "the question still waits");
    drop(s);
    std::fs::write(dir.path().join(&saved), format!("{base}# moved\n")).expect("moved");
    let mut resumed = session(dir.path(), Some(home.path()));
    let out = resumed.restore_round();
    assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
    assert!(resumed.pending.is_none() && resumed.authoring.is_none());
    assert_eq!(
        nika_files(dir.path()).len(),
        1,
        "{:?}",
        nika_files(dir.path())
    );
}

/// The second typed revision links the first revision's resolved clause to the new choice.
fn finalized() -> String {
    links(CHANGE, FURTHER)
}

/// A session over saved A whose revision through its question is proposed (an update of A),
/// then corrected again at its consent prompt by [`FURTHER`], whose disposition is asked.
fn corrected_update(s: &mut SessionRuntime, saved: &Path, base: &str) -> TurnOutcome {
    let out = revised_through_question(s, saved);
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    assert_updates(s, saved, base);
    let out = s.consent(FURTHER);
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    out
}

/// F-P2-1: a correction of the waiting proposal that updates saved A (a second MODIFY said at its
/// consent prompt) revises that proposal and keeps A's file and the witness of A's saved bytes,
/// through its question: exactly one `UpdateWorkflow` at A (never a twin), one `.nika` after its
/// own consent, and nothing runs.
#[test]
fn a_correction_of_a_saved_files_update_keeps_its_file_and_witness() {
    let mut script = authored(response);
    script.extend([
        (200, response(&revised())),
        (200, response(JUDGE_APPROVES)),
        (200, response(&finalized())),
        (200, response(JUDGE_APPROVES)),
    ]);
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let (mut s, saved, base) = saved_a(dir.path(), WORK);
    corrected_update(&mut s, &saved, &base);
    let out = s.turn("./final.txt");
    assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
    // The user-visible proof: an update of A over A's saved bytes, never a fresh twin.
    assert_updates(&s, &saved, &base);
    let set = s.pending.clone().expect("the corrected update waits");
    assert_eq!(set.changes.len(), 1, "{:?}", set.changes);
    let bytes = bytes_of(&set);
    assert!(
        bytes.contains("final.txt") && !bytes.contains("revised.txt"),
        "{bytes}"
    );
    let only = vec![saved.display().to_string()];
    assert_eq!(
        nika_files(dir.path()),
        only,
        "nothing written before its consent"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
    let out = s.consent("yes");
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("applied")),
        "{out:?}"
    );
    assert_eq!(nika_files(dir.path()), only, "one file, never a twin");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        bytes
    );
    for output in ["sortie.txt", "revised.txt", "final.txt"] {
        assert!(!dir.path().join(output).exists(), "nothing ran: {output}");
    }
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 4);
}

/// F-P2-1 (restore seam): the kept round of a correction of saved A's update, its question
/// waiting, reopened in a new session over the same history and continued by `/restore`. The
/// update it revises is proposed again and set aside; the restored round takes A's file and the
/// witness of A's saved bytes from it, and asks its question again with no call. Its answer is
/// held (no judge is permitted to the replay): the set-aside update of A waits again, A's file
/// untouched. This observes the restoration seam only; it claims no durable native continuation
/// to a proposal.
#[test]
fn a_restored_correction_of_a_saved_files_update_keeps_its_file_and_witness() {
    let mut script = authored(response);
    script.extend([
        (200, response(&revised())),
        (200, response(JUDGE_APPROVES)),
        (200, response(&finalized())),
        (200, response(JUDGE_APPROVES)),
    ]);
    let peer = Peer::start(script);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    let (mut s, saved, base) = saved_a_kept(dir.path(), Some(home.path()), WORK);
    corrected_update(&mut s, &saved, &base);
    let waiting_update = bytes_of(&s.revising.as_ref().expect("the update is set aside").0);
    let target = Some((saved.clone(), Witness::of(base.as_bytes())));
    assert_eq!(s.authoring.as_ref().and_then(|r| r.target.clone()), target);
    // A recorded turn at the question keeps the round (the direct call above records none).
    let _ = s.turn("why");
    drop(s);
    let mut resumed = open(dir.path());
    resumed.enable_history(home.path()).expect("history");
    resumed.with_classifier(Box::new(Revisions));
    let out = resumed.restore_kept();
    assert!(
        matches!(&out, TurnOutcome::Question { key, .. } if key == "revision.path"),
        "{out:?}"
    );
    assert_eq!(
        resumed.authoring.as_ref().and_then(|r| r.target.clone()),
        target,
        "the restored round revises A over A's saved bytes"
    );
    assert_eq!(
        peer.bodies().len(),
        CREATE_CALLS + 3,
        "the replay calls no one"
    );
    let out = resumed.turn("./final.txt");
    assert!(
        matches!(&out, TurnOutcome::Held { preview, .. }
            if preview.contains("only a judge this round can permit")),
        "{out:?}"
    );
    assert_eq!(peer.bodies().len(), CREATE_CALLS + 3, "no judge is asked");
    assert_updates(&resumed, &saved, &base);
    assert_eq!(
        bytes_of(resumed.pending.as_ref().expect("the update waits again")),
        waiting_update,
    );
    assert_eq!(nika_files(dir.path()), vec![saved.display().to_string()]);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(&saved)).expect("A"),
        base
    );
}

mod stop;
