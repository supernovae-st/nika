// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Property tests for `TrustLevel` lattice laws.
//!
//! Wave 3 H8: the `TrustLevel` u8 lattice gates every Shield capability
//! decision. Commit 2 removed the (inverted) `Default`; these properties
//! confirm `meet` / `join` form a valid bounded lattice — required by
//! Nika Shield security semantics.

use nika_error::trust::TrustLevel;
use proptest::prelude::*;

proptest! {
    #[test]
    fn trust_meet_idempotent(l: u8) {
        let t = TrustLevel::new(l);
        prop_assert_eq!(t.meet(t), t);
    }

    #[test]
    fn trust_join_idempotent(l: u8) {
        let t = TrustLevel::new(l);
        prop_assert_eq!(t.join(t), t);
    }

    #[test]
    fn trust_meet_commutative(a: u8, b: u8) {
        let (a, b) = (TrustLevel::new(a), TrustLevel::new(b));
        prop_assert_eq!(a.meet(b), b.meet(a));
    }

    #[test]
    fn trust_join_commutative(a: u8, b: u8) {
        let (a, b) = (TrustLevel::new(a), TrustLevel::new(b));
        prop_assert_eq!(a.join(b), b.join(a));
    }

    #[test]
    fn trust_meet_associative(a: u8, b: u8, c: u8) {
        let (a, b, c) = (TrustLevel::new(a), TrustLevel::new(b), TrustLevel::new(c));
        prop_assert_eq!(a.meet(b).meet(c), a.meet(b.meet(c)));
    }

    #[test]
    fn trust_join_associative(a: u8, b: u8, c: u8) {
        let (a, b, c) = (TrustLevel::new(a), TrustLevel::new(b), TrustLevel::new(c));
        prop_assert_eq!(a.join(b).join(c), a.join(b.join(c)));
    }

    #[test]
    fn trust_absorption_meet_over_join(a: u8, b: u8) {
        let (a, b) = (TrustLevel::new(a), TrustLevel::new(b));
        prop_assert_eq!(a.meet(a.join(b)), a);
    }

    #[test]
    fn trust_absorption_join_over_meet(a: u8, b: u8) {
        let (a, b) = (TrustLevel::new(a), TrustLevel::new(b));
        prop_assert_eq!(a.join(a.meet(b)), a);
    }

    #[test]
    fn trust_serde_roundtrip(l: u8) {
        let t = TrustLevel::new(l);
        let json = serde_json::to_string(&t).expect("serialize");
        let back: TrustLevel = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(t, back);
    }

    #[test]
    fn trust_is_at_least_consistent_with_ord(a: u8, b: u8) {
        let (a, b) = (TrustLevel::new(a), TrustLevel::new(b));
        prop_assert_eq!(a.is_at_least(b), a >= b);
    }
}
