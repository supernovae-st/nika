// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A default the candidate already carries needs no author round trip: picked natively, it is
//! bound by the person's line and the proposal shown waits on, unchanged, with its next choice or
//! its save decision. Another option still reaches the author, who revises.

use serde_json::Value;

use crate::peer::Kind;
use crate::scenarios::{PARIS, SEAT, ZONE};
use crate::{candidate, outcome_kind, proposal, provenance, run, waiting, work};

/// How many choices the proposal shown after `step` leaves open.
fn open_choices(step: &Value) -> usize {
    work(step)["open_choices"].as_array().map_or(0, Vec::len)
}

/// The words the outcome of `step` says.
fn said(step: &Value) -> &str {
    step["outcome"]["text"].as_str().unwrap_or_default()
}

/// The proposal shows the run model and the zone with their defaults; Enter on each keeps it,
/// bound by that line as `offered`, the proposal identity and bytes unchanged, and no author call
/// between the proposal and its save decision; « oui » then saves the bytes shown.
#[test]
fn defaults_picked_natively_reach_the_save_decision_with_no_author_call() {
    let journey = run("kept_defaults");
    let shown = journey.step(0);
    assert_eq!(outcome_kind(shown), "proposal", "{shown:#}");
    assert_eq!(open_choices(shown), 2, "{shown:#}");
    assert_eq!(
        journey.agent.len(),
        2,
        "the candidate and its proposal: no author call between the proposal and its save decision"
    );
    let model = journey.step(1);
    assert_eq!(outcome_kind(model), "facts", "{model:#}");
    assert!(
        said(model).starts_with("défaut gardé · Le modèle de cette Session"),
        "{model:#}"
    );
    assert_eq!(proposal(model), proposal(shown), "the same proposal waits");
    assert_eq!(waiting(model), "consent", "{model:#}");
    assert_eq!(open_choices(model), 1, "{model:#}");
    assert_eq!(provenance(model, SEAT), ("offered".into(), "u2".into()));
    let zone = journey.step(2);
    assert_eq!(outcome_kind(zone), "facts", "{zone:#}");
    assert!(said(zone).contains("« oui » l'applique"), "{zone:#}");
    assert_eq!(proposal(zone), proposal(shown), "the same proposal waits");
    assert_eq!(waiting(zone), "consent", "the save decision: {zone:#}");
    assert_eq!(open_choices(zone), 0, "{zone:#}");
    assert_eq!(provenance(zone, ZONE), ("offered".into(), "u3".into()));
    let reads = (journey.seen.iter()).filter(|seen| seen.kind == Kind::Pick);
    assert_eq!(reads.count(), 0, "a native pick needs no reading");
    let saved = journey.step(3);
    let workflow = work(saved)["saved"]["workflow"].as_str().expect("saved");
    let landed = journey.dir.path().join("project").join(workflow);
    let bytes = std::fs::read_to_string(landed).expect("the saved workflow");
    assert_eq!(bytes, candidate(shown), "the bytes shown are saved");
}

/// Another option than the default still sends the author to revise: the zone picked reaches
/// it, which proposes the schedule in that zone; the model's default picked on that revision is
/// then kept with no author.
#[test]
fn another_option_still_revises_through_the_author() {
    let journey = run("kept_defaults_other_zone");
    let picked = journey.step(1);
    assert_eq!(outcome_kind(picked), "proposal", "{picked:#}");
    assert_ne!(proposal(picked), proposal(journey.step(0)), "a revision");
    assert!(
        said(picked).contains(&format!("zone {PARIS}")),
        "{picked:#}"
    );
    assert_eq!(provenance(picked, PARIS), ("offered".into(), "u2".into()));
    assert_eq!(open_choices(picked), 1, "the model is still shown");
    assert_eq!(journey.agent.len(), 3, "the pick reached the author once");
    let model = journey.step(2);
    assert_eq!(outcome_kind(model), "facts", "{model:#}");
    assert_eq!(proposal(model), proposal(picked));
    assert_eq!(waiting(model), "consent", "{model:#}");
    assert_eq!(journey.agent.len(), 3, "the default reached no author");
}
