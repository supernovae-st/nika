// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F-P22 (NEP-0017) — the blame polarity of a failure: WHO a violated
//! bound is imputed to. This module names F-A5's two existing polarities
//! (« by the value » · « by the caller ») for the first time and adds the
//! third F-J2 × F-P22 introduces: a default the CONTRACT declares and the
//! normalization materializes is imputed « by the contract » — the
//! failure receipt names the faulty contract, never the value that
//! merely carried the default downstream.
//!
//! The closed vocabulary rides the wire kebab-case (`by-the-value` ·
//! `by-the-caller` · `by-the-contract`) so journals, receipts and
//! diagnostics spell the polarity with one voice.

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The closed blame-polarity vocabulary (F-A5 × F-J2 × F-P22 ·
/// kebab-case on the wire).
///
/// `#[non_exhaustive]`: a future law MAY add a polarity — consumers
/// match with a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum BlamePolarity {
    /// The VALUE itself violated the bound (F-A5 — e.g. a written
    /// literal the schema rejects).
    ByTheValue,
    /// The CALLER wrote the bound the failure tripped (F-A5 — e.g. the
    /// task's own `max_turns:` the loop exhausted).
    ByTheCaller,
    /// The CONTRACT declared the default the failure trips (F-P22 —
    /// e.g. spec 02-verbs.md §agent's `max_turns` default 10 applied on
    /// an absent key): neither the value nor the caller is at fault —
    /// the receipt names the contract that DECLARES the default.
    ByTheContract,
}

impl BlamePolarity {
    /// The wire form (the journal's and receipt's field values).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ByTheValue => "by-the-value",
            Self::ByTheCaller => "by-the-caller",
            Self::ByTheContract => "by-the-contract",
        }
    }
}

impl fmt::Display for BlamePolarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn the_three_wire_names_are_pinned() {
        // F-P22's positive acceptance: the third polarity EXISTS on the
        // wire next to F-A5's two — these spellings are the law's pin.
        assert_eq!(BlamePolarity::ByTheValue.as_str(), "by-the-value");
        assert_eq!(BlamePolarity::ByTheCaller.as_str(), "by-the-caller");
        assert_eq!(BlamePolarity::ByTheContract.as_str(), "by-the-contract");
    }

    #[test]
    fn display_is_the_kebab_wire_form() {
        for (polarity, wire) in [
            (BlamePolarity::ByTheValue, "by-the-value"),
            (BlamePolarity::ByTheCaller, "by-the-caller"),
            (BlamePolarity::ByTheContract, "by-the-contract"),
        ] {
            assert_eq!(polarity.to_string(), wire, "Display == as_str");
            assert_eq!(polarity.as_str(), wire);
        }
    }

    #[test]
    fn serde_round_trips_the_kebab_wire_form() {
        for (polarity, wire) in [
            (BlamePolarity::ByTheValue, "\"by-the-value\""),
            (BlamePolarity::ByTheCaller, "\"by-the-caller\""),
            (BlamePolarity::ByTheContract, "\"by-the-contract\""),
        ] {
            let json = serde_json::to_string(&polarity).expect("serialize");
            assert_eq!(json, wire, "{polarity}");
            let back: BlamePolarity = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, polarity);
        }
        // The vocabulary is CLOSED — an unknown polarity is rejected.
        assert!(serde_json::from_str::<BlamePolarity>("\"by-the-model\"").is_err());
    }
}
