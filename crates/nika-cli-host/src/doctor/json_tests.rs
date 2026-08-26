// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `doctor --json` machine-lane tests (split from `tests.rs` at the
//! 1,500-LOC file law): the adoption token ladder and the R4 access
//! census lane.
#![allow(clippy::expect_used)]

use super::{AdoptionState, Finding, Level, render_json};
use nika_providers::census::AccessCensus;

fn findings() -> Vec<Finding> {
    vec![Finding {
        level: Level::Ok,
        label: "binary".to_owned(),
        detail: "v0".to_owned(),
        fix: None,
    }]
}

/// P0-21 — the machine lane carries the adoption rung next to the
/// findings: agents/CI branch on ONE state token, not on a parse of
/// the flat finding rows. R4 added the seat rung.
#[test]
fn doctor_json_serializes_the_adoption_state() {
    for (state, token) in [
        (AdoptionState::Installed, "installed"),
        (AdoptionState::LocalDetected, "local_detected"),
        (AdoptionState::LocalReachable, "local_reachable"),
        (AdoptionState::KeyPresent, "key_present"),
        (AdoptionState::SeatReady, "seat_ready"),
        (AdoptionState::RealReady, "real_ready"),
    ] {
        let json: serde_json::Value = serde_json::from_str(&render_json(
            &findings(),
            state,
            &[],
            &AccessCensus::default(),
        ))
        .expect("valid JSON");
        assert_eq!(json["adoption_state"], token, "{state:?}");
    }
}

/// R4 — the machine lane renders the CENSUS (one read, never
/// recomputed): every path with its class · custody · fix, the ready
/// seats, the best path. The pre-census fields stay verbatim.
#[test]
fn doctor_json_renders_the_access_census() {
    let census = AccessCensus::from_parts(
        &[],
        vec![nika_providers::census::SeatFact::new(
            "claude-code",
            vec!["anthropic".to_owned()],
            true,
            true,
            true,
        )],
    );
    let json: serde_json::Value = serde_json::from_str(&render_json(
        &findings(),
        AdoptionState::SeatReady,
        &[],
        &census,
    ))
    .expect("valid JSON");
    assert_eq!(json["access"]["seats_ready"][0], "claude-code");
    assert_eq!(json["access"]["best"], "claude-code");
    let path = &json["access"]["paths"][0];
    assert_eq!(path["id"], "claude-code");
    assert_eq!(path["class"], "harness");
    assert_eq!(path["configured"], true);
    // A ready seat teaches nothing — `fix` is null, never a wrapper id.
    assert_eq!(path["fix"], serde_json::Value::Null);
    // Mutation pin: an unready seat keeps its fix and leaves `best` null.
    let unready = AccessCensus::from_parts(
        &[],
        vec![nika_providers::census::SeatFact::new(
            "claude-code",
            vec!["anthropic".to_owned()],
            true,
            false,
            true,
        )],
    );
    let json: serde_json::Value = serde_json::from_str(&render_json(
        &findings(),
        AdoptionState::Installed,
        &[],
        &unready,
    ))
    .expect("valid JSON");
    assert!(
        json["access"]["seats_ready"]
            .as_array()
            .expect("array")
            .is_empty()
    );
    assert_eq!(json["access"]["best"], serde_json::Value::Null);
    assert!(
        json["access"]["paths"][0]["fix"]
            .as_str()
            .expect("the fix line")
            .contains("--access claude-code"),
        "{json:#}"
    );
}
