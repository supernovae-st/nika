// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal's source basis at its yes (C9 · F4), over the runtime's public doors: a
//! no-intelligence session (the reasoner, named `none`, counts any prompt it is handed), local
//! files and the deterministic compiler. No provider, no workflow execution, no paid request.
//! The public F4 diagnostic (`schema-drift-session`): a header column renamed between the
//! proposal and the yes still saved the stale workflow; compatible new rows must keep landing.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, Reply, SessionReasoner};
use crate::runtime::{SessionRuntime, TurnOutcome};

/// The public request of the F4 diagnostic.
const REQUEST: &str = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json";
/// The same request, said again over the renamed column.
const RESTATED: &str = "read ./data/input.csv, keep the rows where amount is over 250, write them to ./out/result.json";
const INPUT: &str =
    "id,amount_usd,status\nA001,10,paid\nA026,260,paid\nA027,270,open\nA030,300,open\n";
/// Where the session lands a candidate in a root without `workflows/`.
const LANDED: &str = "compiled-workflow.nika";

type Seen = Arc<Mutex<Vec<String>>>;

/// The no-intelligence reasoner, counted.
struct Player {
    seen: Seen,
}

impl SessionReasoner for Player {
    fn name(&self) -> String {
        "none".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.seen
            .lock()
            .expect("prompt record")
            .push(prompt.to_owned());
        Err(ReasonError::NoIntelligence)
    }
}

fn project(input: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(dir.path().join("data")).expect("data");
    std::fs::write(dir.path().join("data/input.csv"), input).expect("input");
    dir
}

fn open(root: &Path) -> (SessionRuntime, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let player = Player {
        seen: Arc::clone(&seen),
    };
    let intelligence = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: true,
        why: None,
    };
    (
        SessionRuntime::open(root, intelligence, Box::new(player)),
        seen,
    )
}

fn proposal(outcome: TurnOutcome) -> (ProposalId, String) {
    match outcome {
        TurnOutcome::Proposal { id, preview } => (id, preview),
        other => panic!("expected a proposal, got {other:?}"),
    }
}

fn refused(outcome: TurnOutcome) -> Refusal {
    match outcome {
        TurnOutcome::Refusal(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn facts(outcome: TurnOutcome) -> String {
    match outcome {
        TurnOutcome::Facts(text) => text,
        other => panic!("expected facts, got {other:?}"),
    }
}

/// The exact bytes the pending proposal would land.
fn pending_bytes(s: &SessionRuntime) -> String {
    s.pending
        .as_ref()
        .and_then(|set| set.changes.first())
        .expect("a pending proposal")
        .content()
        .to_owned()
}

/// Nothing of a consent reached the project: no workflow, no output, no consent record.
fn untouched(root: &Path) {
    assert!(!root.join(LANDED).exists(), "no workflow written");
    assert!(!root.join("out").exists(), "no output");
    assert!(
        !root.join(".nika/consents.ndjson").exists(),
        "no consent recorded"
    );
}

/// F4 · RED before C9: the yes saved the stale workflow. Now the renamed column withdraws the
/// proposal before anything lands, names the dependency, keeps the goal, asks no model, and a
/// late consent by its identity finds nothing to apply.
#[test]
fn a_column_renamed_before_the_yes_withdraws_the_proposal_with_no_effect() {
    let root = project(INPUT);
    let (mut s, seen) = open(root.path());
    let (id, _) = proposal(s.turn(REQUEST));
    std::fs::write(
        root.path().join("data/input.csv"),
        INPUT.replace("amount_usd", "amount"),
    )
    .expect("the header changed");
    let why = refused(s.consent("yes"));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text
            .contains("`./data/input.csv` no longer has `amount_usd`: it has id, amount, status"),
        "{}",
        why.text
    );
    assert!(why.text.contains("nothing was written"), "{}", why.text);
    untouched(root.path());
    assert!(s.pending_proposal().is_none(), "withdrawn, never pending");
    assert_eq!(s.intent.goal.as_deref(), Some(REQUEST), "the goal stays");
    let late = refused(s.consent_to(&id, "yes"));
    assert_eq!(late.class, RefusalClass::WrongState, "{}", late.text);
    untouched(root.path());
    assert!(seen.lock().expect("record").is_empty(), "no model asked");
}

/// New, removed or reordered rows, another column order and a new peek hash hold the basis:
/// the exact reviewed bytes land and the report says the sources were judged.
#[test]
fn compatible_data_keeps_the_basis_and_lands_the_reviewed_bytes() {
    let reordered =
        "id,amount_usd,status\nA030,300,open\nA031,700,paid\nA001,10,paid\nA027,270,open\n";
    let columns = "status,amount_usd,id\npaid,10,A001\npaid,260,A026\nopen,270,A027\n";
    for fresh in [reordered, columns] {
        let root = project(INPUT);
        let (mut s, seen) = open(root.path());
        proposal(s.turn(REQUEST));
        let bytes = pending_bytes(&s);
        std::fs::write(root.path().join("data/input.csv"), fresh).expect("fresh data");
        let report = facts(s.consent("yes"));
        assert!(report.contains("applied"), "{report}");
        assert!(
            report.contains(
                "sources judged again before writing: 2 recorded fact(s) of `./data/input.csv` hold"
            ),
            "{report}"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(LANDED)).expect("landed"),
            bytes
        );
        assert!(seen.lock().expect("record").is_empty());
    }
}

/// A source that is gone, or a value no longer a number, withdraws the proposal too.
#[test]
fn a_vanished_source_or_a_value_no_longer_a_number_withdraws_the_proposal() {
    let root = project(INPUT);
    let (mut s, _) = open(root.path());
    proposal(s.turn(REQUEST));
    std::fs::remove_file(root.path().join("data/input.csv")).expect("removed");
    let why = refused(s.consent("yes"));
    assert!(
        why.text
            .contains("`./data/input.csv` is absent now: `amount_usd` cannot be read from it"),
        "{}",
        why.text
    );
    untouched(root.path());

    let root = project(INPUT);
    let (mut s, _) = open(root.path());
    proposal(s.turn(REQUEST));
    let text = format!("{INPUT}A040,n/a,paid\n");
    std::fs::write(root.path().join("data/input.csv"), text).expect("a text value");
    let why = refused(s.consent("yes"));
    assert!(
        why.text.contains(
            "`amount_usd` in `./data/input.csv` now has sampled values that are not numbers (text 1)"
        ),
        "{}",
        why.text
    );
    untouched(root.path());
}

/// A consent naming another proposal is refused before any judgement; the one waiting is judged
/// by its own basis at its own yes.
#[test]
fn a_consent_with_another_identity_never_reaches_the_basis() {
    let root = project(INPUT);
    let (mut s, _) = open(root.path());
    let (id, _) = proposal(s.turn(REQUEST));
    let other = ProposalId::of("another proposal");
    let wrong = refused(s.consent_to(&other, "yes"));
    assert_eq!(wrong.class, RefusalClass::StaleRevision, "{}", wrong.text);
    assert_eq!(s.pending_proposal(), Some(id.clone()), "it still waits");
    untouched(root.path());
    std::fs::write(
        root.path().join("data/input.csv"),
        INPUT.replace("amount_usd", "total"),
    )
    .expect("renamed");
    assert_eq!(
        refused(s.consent_to(&id, "yes")).class,
        RefusalClass::StaleRevision
    );
    untouched(root.path());
}

/// After the withdrawal the request said again over the renamed column is grounded on the
/// project as it is: a new proposal whose program reads `amount`, and its yes lands it.
#[test]
fn the_request_said_again_over_the_renamed_column_lands() {
    let root = project(INPUT);
    let (mut s, seen) = open(root.path());
    proposal(s.turn(REQUEST));
    std::fs::write(
        root.path().join("data/input.csv"),
        INPUT.replace("amount_usd", "amount"),
    )
    .expect("renamed");
    refused(s.consent("yes"));
    let (_, preview) = proposal(s.turn(RESTATED));
    let bytes = pending_bytes(&s);
    assert!(
        bytes.contains(".amount |") && !bytes.contains("amount_usd"),
        "{bytes}"
    );
    // The composed compiler compares exact decimal keys; the threshold remains 250.
    assert!(
        bytes.contains(r#"> ("250" | dkey)"#),
        "the same strict threshold: {bytes}"
    );
    assert!(preview.contains(LANDED), "{preview}");
    let report = facts(s.consent("yes"));
    assert!(report.contains("applied"), "{report}");
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("landed"),
        bytes
    );
    assert!(seen.lock().expect("record").is_empty());
}

/// Control: renamed before the proposal, the compiler grounds the rule's word on the fresh
/// header and asks which observed column it means — the defect lived between proposal and yes.
#[test]
fn a_column_renamed_before_the_proposal_is_asked_by_the_compiler() {
    let root = project(&INPUT.replace("amount_usd", "amount"));
    let (mut s, _) = open(root.path());
    let out = s.turn(REQUEST);
    let TurnOutcome::Question { question, .. } = &out else {
        panic!("the compiler asks: {out:?}");
    };
    assert!(
        question.contains("amount_usd") && question.contains("amount"),
        "{question}"
    );
    assert!(s.pending_proposal().is_none());
    untouched(root.path());
}

/// A money-only amendment at the consent prompt is a new proposal identity over the same bytes:
/// the basis those bytes were built on still judges its yes, and the old identity applies nothing.
#[test]
fn an_amended_proposal_is_judged_by_the_basis_of_its_bytes() {
    let root = project(INPUT);
    let (mut s, _) = open(root.path());
    let (base, _) = proposal(s.turn(REQUEST));
    let bytes = pending_bytes(&s);
    let (amended, _) = proposal(s.consent("budget 0.10 USD"));
    assert_ne!(amended, base);
    assert_eq!(pending_bytes(&s), bytes, "the same bytes");
    std::fs::write(
        root.path().join("data/input.csv"),
        INPUT.replace("amount_usd", "amount"),
    )
    .expect("renamed");
    assert_eq!(
        refused(s.consent_to(&base, "yes")).class,
        RefusalClass::StaleRevision,
        "the old identity applies nothing"
    );
    let why = refused(s.consent_to(&amended, "yes"));
    assert!(
        why.text.contains("no longer has `amount_usd`"),
        "{}",
        why.text
    );
    untouched(root.path());
}

/// A proposal kept from a closed session carries no bound basis: at its yes, its request compiled
/// again without AI gives its exact bytes, so that basis is judged and it lands.
#[test]
fn a_kept_proposal_whose_request_gives_the_same_bytes_again_lands() {
    let root = project(INPUT);
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path());
    first.enable_history(home.path()).expect("history");
    proposal(first.turn(REQUEST));
    let bytes = pending_bytes(&first);
    drop(first);
    let (mut again, seen) = open(root.path());
    again.enable_history(home.path()).expect("history");
    proposal(again.turn("/restore"));
    let report = facts(again.consent("yes"));
    assert!(
        report.contains("source basis derived again before writing"),
        "{report}"
    );
    assert!(report.contains("2 recorded fact(s)"), "{report}");
    assert_eq!(
        std::fs::read_to_string(root.path().join(LANDED)).expect("landed"),
        bytes
    );
    assert!(seen.lock().expect("record").is_empty());
}

/// A kept proposal whose request no longer compiles to its bytes (its column was renamed while
/// the session was closed) reads a project file: it is withdrawn, with the request to say it again.
#[test]
fn a_kept_source_dependent_proposal_that_cannot_be_derived_again_is_withdrawn() {
    let root = project(INPUT);
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path());
    first.enable_history(home.path()).expect("history");
    proposal(first.turn(REQUEST));
    drop(first);
    std::fs::write(
        root.path().join("data/input.csv"),
        INPUT.replace("amount_usd", "amount"),
    )
    .expect("renamed while closed");
    let (mut again, _) = open(root.path());
    again.enable_history(home.path()).expect("history");
    proposal(again.turn("/restore"));
    let why = refused(again.consent("yes"));
    assert_eq!(why.class, RefusalClass::StaleRevision, "{}", why.text);
    assert!(
        why.text.contains("reads `./data/input.csv`")
            && why.text.contains("its sources cannot be judged"),
        "{}",
        why.text
    );
    untouched(root.path());
}

/// A proposal that reads no project file carries no source fact: it lands, and its report says
/// plainly that no source freshness was judged — never that it was verified.
#[test]
fn a_source_independent_proposal_lands_with_freshness_said_unjudged() {
    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let (mut first, _) = open(root.path());
    first.enable_history(home.path()).expect("history");
    proposal(first.turn("write \"bonjour\" to ./out/j.txt"));
    drop(first);
    let (mut again, _) = open(root.path());
    again.enable_history(home.path()).expect("history");
    proposal(again.turn("/restore"));
    let report = facts(again.consent("yes"));
    assert!(report.contains("applied"), "{report}");
    assert!(
        report.contains("it reads no project file") && !report.contains("hold"),
        "{report}"
    );
}
