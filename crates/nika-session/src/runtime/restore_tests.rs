// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/restore` over the session's own doors: the proposal kept from a closed session is
//! advertised (notice, help card, completion) only while it can be proposed again; it is
//! proposed again without a model call for a fresh review, lands only on a fresh yes, and
//! never answers, discards or approves what already waits. Scripted reasoning and local
//! files only: no provider, no run.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::draft::Restored;
use super::restore::{RESTORE_HELP, RESTORE_HINT};
use super::{HELP, SLASH_COMMANDS, SessionRuntime, TurnOutcome};
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};

/// An explicit intent the compiler settles at once: Ready, no question, no model.
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// An intent whose model the compiler must ask for: a question owns the next line.
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
/// Where the session lands a candidate in a root without `workflows/`.
const LANDED: &str = "compiled-workflow.nika";

type Seen = Arc<Mutex<Vec<String>>>;

/// A scripted reasoner that records every prompt it is asked.
struct Player {
    inner: ScriptedReasoner,
    seen: Seen,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        "restore-fixture".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.seen
            .lock()
            .expect("prompt record")
            .push(prompt.to_owned());
        self.inner.reason(prompt)
    }
}

fn open(root: &Path) -> (SessionRuntime, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let player = Player {
        inner: ScriptedReasoner::new(vec!["A reply.".to_owned()]),
        seen: Arc::clone(&seen),
    };
    let intelligence = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        model: None,
        locus: DataLocus::Local,
        ready: true,
        why: None,
    };
    (
        SessionRuntime::open(root, intelligence, Box::new(player)),
        seen,
    )
}

fn calls(seen: &Seen) -> usize {
    seen.lock().expect("prompt record").len()
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn proposed(outcome: TurnOutcome) -> (ProposalId, String) {
    match outcome {
        TurnOutcome::Proposal { id, preview } => (id, preview),
        other => panic!("expected a proposal, got {other:?}"),
    }
}

/// `COPY` proposed, the session closed while it waited, and a new session opened on the
/// same project and home: the reopened session, its prompt record, the restore notice and
/// the kept proposal's exact bytes.
fn reopened(root: &Path, home: &Path) -> (SessionRuntime, Seen, String, String) {
    std::fs::create_dir_all(root.join("notes")).expect("notes");
    std::fs::write(
        root.join("notes/brief.md"),
        "# Brief\n\nThe launch moves to October.\n",
    )
    .expect("brief");
    let (mut first, _) = open(root);
    first.enable_history(home).expect("fresh history");
    let _ = proposed(first.turn(COPY));
    let bytes = first
        .pending
        .as_ref()
        .and_then(|set| set.changes.first())
        .expect("the proposed file")
        .content()
        .to_owned();
    drop(first);
    let (mut resumed, seen) = open(root);
    let notice = resumed
        .enable_history(home)
        .expect("resume")
        .expect("a restore notice");
    (resumed, seen, notice, bytes)
}

/// The kept proposal is advertised where it exists, shown again with its saved request and
/// a fresh preview, and a French « non » discards it: nothing lands, no model is asked, and
/// the spent draft is no longer advertised.
#[test]
fn a_kept_proposal_is_found_reviewed_and_declined_without_a_model_call() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut s, seen, notice, _) = reopened(root.path(), home.path());
    assert!(notice.contains(RESTORE_HINT), "{notice}");
    assert!(notice.contains("not applied"), "{notice}");
    assert!(s.slash_commands().contains(&"/restore"));
    assert!(
        matches!(s.turn("/help"), TurnOutcome::Help(ref card) if card.contains(RESTORE_HELP)),
        "the help card names /restore while the draft can be proposed again"
    );
    let (_, preview) = proposed(s.turn("/restore"));
    assert!(
        preview.starts_with("your request from the last session: « ") && preview.contains(COPY),
        "the saved request comes first: {preview}"
    );
    assert!(preview.contains(LANDED), "a fresh preview: {preview}");
    assert!(s.pending_proposal().is_some(), "a fresh review waits");
    assert!(
        !root.path().join(LANDED).exists(),
        "nothing lands before a yes"
    );
    let _ = s.consent("non");
    assert!(s.pending_proposal().is_none(), "« non » discards it");
    assert!(!root.path().join(LANDED).exists(), "a no lands nothing");
    assert!(!s.slash_commands().contains(&"/restore"));
    assert!(!s.help_card().contains(RESTORE_HELP));
    assert_eq!(calls(&seen), 0, "restore, help and consent ask no model");
}

/// While the restored review waits, `/help` and `/status` answer from the session's own
/// facts and `/restore` is refused: the review stays pending, and only an explicit yes
/// lands the kept bytes, exactly.
#[test]
fn a_restored_proposal_lands_its_kept_bytes_only_on_an_explicit_yes() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut s, seen, _, bytes) = reopened(root.path(), home.path());
    let (id, _) = proposed(s.turn("/restore"));
    let TurnOutcome::Help(card) = s.consent("/help") else {
        panic!("help answers locally while the review waits");
    };
    assert!(card.contains("/show"), "{card}");
    assert_eq!(
        s.pending_proposal(),
        Some(id.clone()),
        "help keeps the review"
    );
    assert!(matches!(s.consent("/status"), TurnOutcome::Facts(_)));
    assert_eq!(s.pending_proposal(), Some(id.clone()), "status keeps it");
    let again = refused(s.consent("/restore"));
    assert_eq!(again.class, RefusalClass::WrongState, "{}", again.text);
    assert_eq!(s.pending_proposal(), Some(id.clone()), "restore keeps it");
    assert!(
        !root.path().join(LANDED).exists(),
        "nothing lands before a yes"
    );
    let _ = s.consent_to(&id, "yes");
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("landed"),
        bytes
    );
    assert_eq!(calls(&seen), 0, "no model call");
}

/// Without a kept draft `/restore` is not advertised, and typed anyway it is refused with a
/// way on; a draft kept by another engine version is not advertised either, and its
/// refusal says why while it stays kept unchanged.
#[test]
fn restore_is_neither_advertised_nor_granted_without_a_usable_draft() {
    let root = tempfile::tempdir().expect("project");
    let (mut s, seen) = open(root.path());
    assert_eq!(s.help_card(), HELP);
    assert_eq!(s.slash_commands(), SLASH_COMMANDS.to_vec());
    assert!(matches!(s.turn("/help"), TurnOutcome::Help(ref card) if !card.contains("/restore")));
    let none = refused(s.turn("/restore"));
    assert_eq!(none.class, RefusalClass::WrongState);
    assert!(
        none.text.contains("no kept draft to propose again")
            && none.text.contains("describe what you want instead"),
        "{}",
        none.text
    );
    s.restored_draft = Some(Restored::from_raw(serde_json::json!({ "schema": 2 })));
    assert_eq!(s.help_card(), HELP, "an unreadable draft is not advertised");
    assert!(!s.slash_commands().contains(&"/restore"));
    let unreadable = refused(s.turn("/restore"));
    assert_eq!(unreadable.class, RefusalClass::NotAllowed);
    assert!(
        unreadable.text.contains("cannot be used by this engine")
            && unreadable.text.contains("stays kept unchanged"),
        "{}",
        unreadable.text
    );
    assert!(s.restored_draft.is_some(), "it stays kept");
    assert!(s.pending_proposal().is_none());
    assert_eq!(calls(&seen), 0, "no model call");
}

/// A destination that appeared since the draft was proposed makes the kept draft
/// unavailable: it stays kept, `/restore` is no longer advertised, and typed anyway it is
/// refused with its reason and a way on. Nothing is overwritten and nothing waits.
#[test]
fn a_changed_project_refuses_restore_and_keeps_the_draft() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut s, seen, _, _) = reopened(root.path(), home.path());
    std::fs::write(root.path().join(LANDED), "nika: someone-else\n").expect("appeared");
    assert!(s.restored_draft.is_some(), "the draft stays kept");
    assert!(s.restored_draft_id().is_none(), "but it is not available");
    assert!(!s.slash_commands().contains(&"/restore"));
    assert!(!s.help_card().contains(RESTORE_HELP));
    let changed = refused(s.turn("/restore"));
    assert_eq!(changed.class, RefusalClass::NotAllowed);
    assert!(
        changed.text.contains("exists now")
            && changed.text.contains("state the request again instead"),
        "{}",
        changed.text
    );
    assert!(s.pending_proposal().is_none());
    assert!(s.restored_draft.is_some(), "the draft is still kept");
    assert!(!s.slash_commands().contains(&"/restore"));
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("theirs"),
        "nika: someone-else\n"
    );
    assert_eq!(calls(&seen), 0, "no model call");
}

/// `/restore` never answers or discards what already waits: beside an authoring question it
/// is refused, the question still owns the next line, and the draft stays kept.
#[test]
fn restore_never_answers_or_discards_what_already_waits() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut s, seen, _, _) = reopened(root.path(), home.path());
    assert!(
        matches!(s.turn(DRAFT), TurnOutcome::Question { .. }),
        "the model question waits"
    );
    let before = calls(&seen);
    let waits = refused(s.turn("/restore"));
    assert_eq!(waits.class, RefusalClass::WrongState);
    assert!(
        waits.text.contains("something already waits for you"),
        "{}",
        waits.text
    );
    assert!(s.pending_question().is_some(), "the question still waits");
    assert!(s.pending_proposal().is_none(), "nothing was proposed");
    assert!(s.restored_draft_id().is_some(), "the draft stays kept");
    assert_eq!(calls(&seen), before, "/restore asks no model");
}
