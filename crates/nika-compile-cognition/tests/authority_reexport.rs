// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_compile_cognition::authority` after the module descended to the
//! size-cap member below the seats' doors, `nika_compile_seats` (2026-10-09 · ADR-152). This file
//! is an external consumer: it compiles against the cognition path, and that path names the very
//! same items — the same types (a value of one path IS a value of the other), the same functions
//! (their signatures carry the member's types) and the same constants — never copies.
#![allow(clippy::expect_used)]

use nika_compile::NativeMode;
use nika_compile_cognition::authority::{
    Authority, DEFAULT_MAX_CALLS, Door, REPAIRS, Refusal, SAMPLES, Typed, least_requests,
    recovery_requests, usage_complete, worst_case_of,
};
use nika_compile_seats as member;
use serde_json::json;

/// Each function of the cognition path, typed with the member's types: a cognition-side copy of
/// these items could not be assigned here.
const RESOLVE: fn(
    Option<u32>,
    NativeMode,
    member::authority::Typed,
    member::authority::Door,
) -> Result<member::authority::Authority, member::authority::Refusal> = Authority::resolve;
const WORST: fn(NativeMode, u32, Option<u32>, bool) -> Option<u32> =
    member::authority::worst_case_of;
const LEAST: fn(NativeMode) -> u32 = member::authority::least_requests;

#[test]
fn the_cognition_path_names_the_member_items() {
    let door: member::authority::Door = Door::new("--authoring-max-calls", "authorize more");
    let typed: member::authority::Typed = Typed::new(false);
    let granted = RESOLVE(Some(2), NativeMode::Escalate, typed, door).expect("two requests");
    assert_eq!(granted.max_calls(), Some(2));
    let refused: Result<Authority, Refusal> =
        member::authority::Authority::resolve(Some(0), NativeMode::Escalate, typed, door);
    assert!(matches!(
        refused,
        Err(member::authority::Refusal::Range {
            name: "max_calls",
            ..
        })
    ));
    assert_eq!(
        WORST(NativeMode::Off, 1, Some(0), true),
        worst_case_of(NativeMode::Off, 1, Some(0), true)
    );
    assert_eq!(
        LEAST(NativeMode::Sketch),
        least_requests(NativeMode::Sketch)
    );
    assert_eq!(
        recovery_requests(3),
        member::authority::recovery_requests(3)
    );
    assert!(usage_complete(&[
        json!({"result": {"usage_reported": true}})
    ]));
    assert_eq!(DEFAULT_MAX_CALLS, member::authority::DEFAULT_MAX_CALLS);
    assert_eq!(SAMPLES, member::authority::SAMPLES);
    assert_eq!(REPAIRS, member::authority::REPAIRS);
}

/// The counters the authority hands out are the provider layer's, whichever path names them.
#[test]
fn the_counters_are_the_provider_layer_ones() {
    let door = member::authority::Door::new("--authoring-max-calls", "authorize more");
    let granted = member::authority::Authority::resolve(
        Some(1),
        NativeMode::Escalate,
        member::authority::Typed::new(false),
        door,
    )
    .expect("one request");
    let envelope: std::sync::Arc<nika_compile_cognition::authority::Envelope> = granted.envelope();
    let record = granted.record(&envelope, None);
    assert_eq!(record["max_calls"], 1);
    assert_eq!(record["source"], "--authoring-max-calls");
}
