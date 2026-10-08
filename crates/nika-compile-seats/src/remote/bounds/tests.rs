// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The remote round bounds where they now live: a caller narrows the operator's bounds and never
//! widens them (above a ceiling is refused, never clamped), and a replay token has one spelling.

use super::*;

const SECOND: Duration = Duration::from_secs(1);

fn operator() -> Bounds {
    let mut bounds = Bounds::route(4096, 1024, 30 * SECOND, "operator: test");
    bounds.max_calls = Some(8);
    bounds.repairs = Some(2);
    bounds.deadline = Some(600 * SECOND);
    bounds
}

fn asked(json: &str) -> Limits {
    serde_json::from_str(json).expect("limits")
}

#[test]
fn a_route_states_no_deadline_repair_or_request_count() {
    let route = Bounds::route(4096, 1024, 30 * SECOND, "operator: test");
    assert_eq!(
        (route.deadline, route.repairs, route.max_calls),
        (None, None, None)
    );
    assert_eq!((route.max_tokens, route.initial_tokens), (4096, 1024));
}

#[test]
fn a_caller_narrows_within_the_operators_bounds() {
    let narrowed = narrow(
        &asked(r#"{"max_calls": 3, "repairs": 0, "max_tokens": 512, "call_timeout_ms": 1000, "deadline_ms": 2000}"#),
        operator(),
    )
    .expect("within bounds");
    assert_eq!(narrowed.max_calls, Some(3));
    assert_eq!(
        narrowed.grant,
        "request: limits.max_calls within operator ceiling"
    );
    assert_eq!(narrowed.repairs, Some(0), "zero repairs is a narrowing");
    assert_eq!(narrowed.max_tokens, 512);
    assert_eq!(narrowed.call_timeout, SECOND);
    assert_eq!(narrowed.deadline, Some(2 * SECOND));
    assert_eq!(
        narrow(&asked("{}"), operator()),
        Some(operator()),
        "nothing asked"
    );
}

#[test]
fn above_or_zero_is_refused_never_clamped() {
    for refused in [
        r#"{"max_calls": 9}"#,
        r#"{"max_calls": 0}"#,
        r#"{"repairs": 3}"#,
        r#"{"max_tokens": 4097}"#,
        r#"{"max_tokens": 0}"#,
        r#"{"call_timeout_ms": 30001}"#,
        r#"{"call_timeout_ms": 0}"#,
        r#"{"deadline_ms": 600001}"#,
        r#"{"deadline_ms": 0}"#,
    ] {
        assert_eq!(narrow(&asked(refused), operator()), None, "{refused}");
    }
    let unbounded = Bounds::route(4096, 1024, 30 * SECOND, "operator: test");
    let narrowed = narrow(&asked(r#"{"max_calls": 50, "repairs": 9}"#), unbounded);
    assert_eq!(
        narrowed.map(|b| (b.max_calls, b.repairs)),
        Some((Some(50), Some(9))),
        "no operator ceiling: any positive count narrows"
    );
}

#[test]
fn limits_are_present_values_of_known_names() {
    assert!(serde_json::from_str::<Limits>(r#"{"max_calls": null}"#).is_err());
    assert!(serde_json::from_str::<Limits>(r#"{"budget": 1}"#).is_err());
    assert!(serde_json::from_str::<Limits>(r#"{"max_calls": -1}"#).is_err());
}

#[test]
fn a_replay_token_is_sixty_four_lowercase_hex_digits() {
    assert!(is_token(&"0a".repeat(32)));
    assert!(!is_token(&"0A".repeat(32)), "uppercase");
    assert!(!is_token(&"0a".repeat(31)), "short");
    assert!(!is_token(&"0g".repeat(32)), "not hex");
}
