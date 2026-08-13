// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The claims' proofs — each predicate against a case the studio lived.

use nika_tui_core::claims;
use nika_tui_core::derive;

use crate::common::load_parity;

mod common {
    use nika_tui_core::model::{Run, Workflow};
    pub(crate) fn load_parity(name: &str) -> (Workflow, Run, serde_json::Value) {
        let raw = std::fs::read_to_string(format!("tests/fixtures/{name}.json")).expect("fixture");
        let v: serde_json::Value = serde_json::from_str(&raw).expect("parses");
        (
            serde_json::from_value(v["workflow"].clone()).expect("workflow"),
            serde_json::from_value(v["run"].clone()).expect("run"),
            v["derived"].clone(),
        )
    }
}

/// `chain intact` — claimed on the recorded demo run, REFUSED on the
/// synthetic bench and on a trace that never ran.
#[test]
fn chain_intact_is_real_or_nothing() {
    let (_wf, run, _d) = load_parity("demo-ok");
    assert!(
        claims::may_claim_chain_intact(&run),
        "a recorded trace claims"
    );
    let (_wf, stress, _d) = load_parity("stress");
    assert!(
        !claims::may_claim_chain_intact(&stress),
        "the synthetic bench may not claim the chain"
    );
    let (_wf, never, _d) = load_parity("sonde-neverborn");
    assert!(
        !claims::may_claim_chain_intact(&never),
        "'never ran' is not a trace"
    );
}

/// The hole the predicate used to have: it denied two magic words instead
/// of requiring a receipt, so ANYTHING it had not enumerated claimed the
/// product's gravest line — including a run with nothing in it at all.
#[test]
fn a_run_with_no_receipt_claims_nothing() {
    let bare: nika_tui_core::model::Run = serde_json::from_value(serde_json::json!({
        "trace": "", "when": "", "output": "", "steps": [],
    }))
    .expect("run");
    assert!(
        !claims::may_claim_chain_intact(&bare),
        "no trace id = no receipt = no claim · this one used to pass"
    );

    // A word nobody enumerated is not evidence either — unless the receipt
    // is there, which is the whole point of asking for the receipt first.
    let odd: nika_tui_core::model::Run = serde_json::from_value(serde_json::json!({
        "trace": "", "when": "fabricated by hand", "output": "", "steps": [],
    }))
    .expect("run");
    assert!(!claims::may_claim_chain_intact(&odd), "still no receipt");

    // A real id beside a fabricated stamp is a CONTRADICTION, and a
    // contradiction refuses — the sentinel may land on either field.
    let contradictory: nika_tui_core::model::Run = serde_json::from_value(serde_json::json!({
        "trace": "01ff-real-looking-uuid", "when": "synthetic", "output": "", "steps": [],
    }))
    .expect("run");
    assert!(
        !claims::may_claim_chain_intact(&contradictory),
        "a real id does not launder a fabricated stamp"
    );
}

/// The journal fold leaves `when` blank while carrying a real
/// `workflow_started` uuid. That is a MISSING display field on a real run,
/// not a fabricated one — refusing here would have made the honest case
/// the failing case, which is how a fail-closed fix turns into a new bug.
///
/// The journal is built HERE, not read from the studio tree: a cross-repo
/// path would make this test skip itself in a public engine checkout, and a
/// skip is a green on a subset nobody named.
#[test]
fn a_folded_journal_still_claims_its_chain() {
    let journal = [
        r#"{"kind":"workflow_started","timestamp":1786401552473000000,"#.to_owned()
            + r#""id":{"uuid":"019fedd4-7859-7b31-b27f-896a6f3b01ff"},"#
            + r#""fields":[{"key":"workflow","value":"rapport-hebdo"}]}"#,
        r#"{"kind":"task_completed","timestamp":1786401553000000000,"#.to_owned()
            + r#""fields":[{"key":"task","value":"lire"},{"key":"duration_ms","value":120}]}"#,
    ]
    .join("\n");
    let run = nika_tui_core::ingress::run_from_journal(&journal).expect("folds");
    assert!(!run.trace.is_empty(), "the fold carries the receipt");
    assert!(run.when.is_empty(), "and leaves the display stamp blank");
    assert!(
        claims::may_claim_chain_intact(&run),
        "a real fold claims its chain · the receipt is what a verifier asks for"
    );
}

/// `check · clean` — only an answered-clean checker claims it; an
/// unanswered or refused one may not.
#[test]
fn check_clean_needs_an_answer() {
    assert!(claims::may_claim_check_clean(Some(true)));
    assert!(!claims::may_claim_check_clean(Some(false)));
    assert!(!claims::may_claim_check_clean(None));
}

/// `holds its wave on its own` — the real demo's bottleneck claims it;
/// the free bottleneck (nobody waits) may not, and a run without one
/// claims nothing.
#[test]
fn a_free_bottleneck_claims_nothing() {
    let (wf, run, _d) = load_parity("demo-ok");
    let ws = derive::waves(&wf);
    let neck = derive::bottleneck(&ws, &run);
    assert!(
        claims::may_claim_bottleneck(neck.as_ref()),
        "the recorded demo's neck holds its wave"
    );
    let free = nika_tui_core::derive::Neck::new("personne".to_owned(), 0.0, 0);
    assert!(
        !claims::may_claim_bottleneck(Some(&free)),
        "blocked 0 = nothing to claim"
    );
    assert!(!claims::may_claim_bottleneck(None));
}

/// `⟨simulated⟩` — always claimable, by construction.
#[test]
fn the_simulation_marker_is_unconditional() {
    assert!(claims::must_mark_simulated());
}
