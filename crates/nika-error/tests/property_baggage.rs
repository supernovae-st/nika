// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Property tests for `Baggage` — bounded insertion invariants.
//!
//! Wave 3 H8: Baggage is the W3C context-propagation carrier. It MUST
//! cap entries (`DoS` defence) and MUST NOT panic on any user-provided
//! input. These properties exercise both.

use nika_error::baggage::{Baggage, MAX_ENTRIES};
use proptest::prelude::*;

proptest! {
    #[test]
    fn baggage_max_entries_enforced(n in 0usize..200) {
        let mut b = Baggage::new();
        for i in 0..n {
            b.insert(format!("k{i}"), "v");
        }
        prop_assert!(b.len() <= MAX_ENTRIES);
    }

    #[test]
    fn baggage_insert_never_panics(
        keys in proptest::collection::vec("[a-z]{1,16}", 0..100),
        val_len in 0usize..100,
    ) {
        let mut b = Baggage::new();
        let val = "x".repeat(val_len);
        for k in keys {
            let _ = b.insert(k, val.clone());
        }
    }

    #[test]
    fn baggage_get_after_insert_returns_value(
        key in "[a-z]{1,16}",
        value in "[a-z]{1,32}",
    ) {
        let mut b = Baggage::new();
        if b.insert(key.clone(), value.clone()) {
            let entry = b.get(&key);
            prop_assert!(entry.is_some());
            prop_assert_eq!(&entry.unwrap().value, &value);
        }
    }
}
