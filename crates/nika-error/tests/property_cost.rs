// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Property tests for `Cost` — commutativity, identity, overflow safety.
//!
//! Wave 3 H8: the L0 `Cost` newtype carries every nano-USD billing
//! aggregation in the diamond. These laws back the stdlib-arithmetic
//! decision from commit 2 (P0-2) by checking:
//!
//! - `+` is commutative on safe ranges
//! - `Cost::zero()` is the additive identity
//! - `checked_add` / `checked_sub` / `saturating_mul` never panic

use nika_error::cost::Cost;
use proptest::prelude::*;

proptest! {
    #[test]
    fn cost_add_is_commutative(
        a in -1_000_000_000_000_i128..1_000_000_000_000,
        b in -1_000_000_000_000_i128..1_000_000_000_000,
    ) {
        let c1 = Cost::new(a) + Cost::new(b);
        let c2 = Cost::new(b) + Cost::new(a);
        prop_assert_eq!(c1, c2);
    }

    #[test]
    fn cost_zero_is_identity(
        a in -1_000_000_000_000_i128..1_000_000_000_000,
    ) {
        prop_assert_eq!(Cost::new(a) + Cost::zero(), Cost::new(a));
        prop_assert_eq!(Cost::zero() + Cost::new(a), Cost::new(a));
    }

    #[test]
    fn checked_add_never_panics(a: i128, b: i128) {
        let _ = Cost::new(a).checked_add(Cost::new(b));
    }

    #[test]
    fn checked_sub_never_panics(a: i128, b: i128) {
        let _ = Cost::new(a).checked_sub(Cost::new(b));
    }

    #[test]
    fn saturating_mul_never_panics(a: i128, scale: u64) {
        let _ = Cost::new(a).saturating_mul(scale);
    }

    #[test]
    fn checked_add_agrees_with_i128_checked_add(a: i128, b: i128) {
        let expected = a.checked_add(b).map(Cost::new);
        let actual = Cost::new(a).checked_add(Cost::new(b));
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn checked_sub_agrees_with_i128_checked_sub(a: i128, b: i128) {
        let expected = a.checked_sub(b).map(Cost::new);
        let actual = Cost::new(a).checked_sub(Cost::new(b));
        prop_assert_eq!(actual, expected);
    }
}
