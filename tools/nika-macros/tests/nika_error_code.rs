// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for #[derive(NikaErrorCode)]
//!
//! Macro test fixtures deliberately hold fields and variants that
//! the runtime never reads — they exist to exercise the proc-macro's
//! code generation against every real-world shape. Applying
//! `#![allow(dead_code)]` at the test-file root keeps the fixtures
//! readable instead of peppering `#[allow]` on every field. (S16-swarm
//! clippy sweep; equivalent attribute lives on `event_task_id.rs`
//! for the same reason.)
#![allow(dead_code)]

use nika_macros::NikaErrorCode;

// ── Test enum covering all variant types ────────────────────────

#[derive(NikaErrorCode)]
enum TestError {
    #[nika_code("TEST-001")]
    Named { reason: String },

    #[nika_code("TEST-002")]
    Unit,

    #[nika_code("TEST-003")]
    Tuple(String),

    #[nika_code("TEST-004")]
    MultiField { a: String, b: u32 },
}

#[test]
fn code_named_variant() {
    let e = TestError::Named {
        reason: "x".into(),
    };
    assert_eq!(e.code(), "TEST-001");
}

#[test]
fn code_unit_variant() {
    let e = TestError::Unit;
    assert_eq!(e.code(), "TEST-002");
}

#[test]
fn code_tuple_variant() {
    let e = TestError::Tuple("x".into());
    assert_eq!(e.code(), "TEST-003");
}

#[test]
fn code_multi_field_variant() {
    let e = TestError::MultiField {
        a: "x".into(),
        b: 42,
    };
    assert_eq!(e.code(), "TEST-004");
}

// ── Delegation pattern ──────────────────────────────────────────

struct Inner;

impl Inner {
    fn code(&self) -> &'static str {
        "INNER-999"
    }
}

#[derive(NikaErrorCode)]
enum DelegatingError {
    #[nika_code("DEL-001")]
    Simple { msg: String },

    #[nika_code(delegate)]
    Delegated(Inner),
}

#[test]
fn code_delegated_variant() {
    let e = DelegatingError::Delegated(Inner);
    assert_eq!(e.code(), "INNER-999");
}

#[test]
fn code_simple_with_delegation_enum() {
    let e = DelegatingError::Simple { msg: "x".into() };
    assert_eq!(e.code(), "DEL-001");
}
