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
    let neck = derive::bottleneck(&wf, &run);
    assert!(
        claims::may_claim_bottleneck(neck.as_ref()),
        "the recorded demo's neck holds its wave"
    );
    let free = nika_tui_core::derive::Neck {
        id: "personne".to_owned(),
        idle_total: 0.0,
        blocked: 0,
    };
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
