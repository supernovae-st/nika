// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 6 — the crate's two structural laws, property-tested:
//! the taxonomy is TOTAL (a router never errors) and the hash newtypes
//! are EXACT (a trust anchor never coerces).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nika_pck_manifest::{ArtifactKind, Blake3Hash, Sha256Hash};
use proptest::prelude::*;

proptest! {
    // ∀ strings: deserialization succeeds, and the wire form round-trips
    // byte-identical (core forms map to themselves; everything else rides
    // CustomTool verbatim).
    #[test]
    fn artifact_kind_is_total_and_round_trips(s in ".*") {
        let k: ArtifactKind = serde_json::from_value(serde_json::Value::String(s.clone()))
            .expect("taxonomy deserialization is TOTAL");
        let wire = serde_json::to_value(&k).unwrap();
        prop_assert_eq!(wire, serde_json::Value::String(s));
    }

    // ∀ 64-hex inputs (any case): accepted + canonicalized to lowercase,
    // and Display == serde output == the canonical form.
    #[test]
    fn hashes_accept_all_64_hex_and_canonicalize(bytes in prop::collection::vec(0u8..16, 64), upper in prop::collection::vec(any::<bool>(), 64)) {
        let s: String = bytes.iter().zip(upper).map(|(b, up)| {
            let c = char::from_digit(u32::from(*b), 16).unwrap();
            if up { c.to_ascii_uppercase() } else { c }
        }).collect();
        let h = Blake3Hash::new(&s).expect("64 hex chars always parse");
        prop_assert_eq!(h.as_hex(), s.to_ascii_lowercase());
        prop_assert_eq!(h.to_string(), s.to_ascii_lowercase());
    }

    // ∀ strings that are NOT 64 hex chars: rejected — by the constructor
    // AND by serde (the two entrances agree).
    #[test]
    fn hashes_reject_everything_else(s in ".*") {
        prop_assume!(!(s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())));
        prop_assert!(Sha256Hash::new(&s).is_err());
        let as_json = serde_json::Value::String(s);
        prop_assert!(serde_json::from_value::<Sha256Hash>(as_json).is_err());
    }
}
