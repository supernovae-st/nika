// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! « request human confirmation before writing » asks a human before the write, as « ask for
//! human confirmation before writing » always did (the reader missed the requested
//! approval, so the seat's confirmation was refused as invented). `request`, `seek` and
//! `solicit` are asking verbs of the named gate, whose shape is unchanged: an asking verb, an
//! approval word or a person, then a `before` or `until` connector that binds the effect after
//! it. A request for something that is no approval, a confirmation that is a thing and a waiver
//! stay no gate.
use nika_compile_reader::lexicon;
use nika_compile_reader::plan::{EffectPolicy, EffectVerb};

/// The effects the reading of `intent` holds for a human's yes.
fn human_first(intent: &str) -> Vec<EffectVerb> {
    let plan = lexicon::read(intent).plan;
    let held = plan
        .effects
        .iter()
        .filter(|e| e.policy == EffectPolicy::HumanFirst);
    held.map(|e| e.verb).collect()
}

#[test]
fn a_requested_confirmation_before_writing_holds_the_write() {
    for intent in [
        "Read ./data/readings.csv and sum celsius; otherwise display the total and request human confirmation before writing.",
        "Read ./data/readings.csv, sum celsius and request human confirmation before writing the total to ./out/total.json.",
    ] {
        assert_eq!(human_first(intent), [EffectVerb::Write], "{intent}");
    }
}

#[test]
fn seeking_or_soliciting_an_approval_holds_the_effect_it_binds() {
    for intent in [
        "Read ./data/readings.csv, draft a summary and seek my approval before writing it to ./out/summary.md.",
        "Read ./data/readings.csv, draft a summary and solicit my approval before writing it to ./out/summary.md.",
    ] {
        assert_eq!(human_first(intent), [EffectVerb::Write], "{intent}");
    }
}

#[test]
fn asking_a_human_to_approve_stays_a_gate() {
    let intent = "Read ./data/readings.csv, draft a summary and ask a human to approve before writing it to ./out/summary.md.";
    assert_eq!(human_first(intent), [EffectVerb::Write]);
}

#[test]
fn a_request_for_no_approval_and_a_confirmation_that_is_a_thing_hold_nothing() {
    for intent in [
        "Read ./data/readings.csv, request the file before writing the total to ./out/total.json.",
        "Read ./data/readings.csv and request a refund before the deadline.",
        "Read ./data/orders.csv and confirm the order number in the report written to ./out/report.json.",
        "Read ./data/orders.csv, then send the confirmation email to ops@example.test.",
    ] {
        assert_eq!(human_first(intent), Vec::<EffectVerb>::new(), "{intent}");
    }
}

#[test]
fn a_waived_request_stays_no_gate() {
    let intent = "Read ./data/readings.csv, sum celsius and write the total to ./out/total.json; no need to request confirmation before writing.";
    let reading = lexicon::read(intent);
    assert_eq!(human_first(intent), Vec::<EffectVerb>::new());
    let waived = |c: &String| c.contains("no need to request confirmation");
    assert!(
        reading.policy_clauses.iter().any(waived),
        "{:?}",
        reading.policy_clauses
    );
}
