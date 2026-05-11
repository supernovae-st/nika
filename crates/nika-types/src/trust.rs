// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `TrustLevel` — lattice-ordered trust for Nika Shield.
//!
//! Forms a bounded lattice with `meet` (min) and `join` (max) operations.
//! Used by the security layer to make capability decisions.
//!
//! ## Why u8 lattice, not enum?
//! Allows adding intermediate trust levels (e.g., 75, 125) without
//! breaking existing comparisons or match arms.

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// Trust level as a u8 lattice value. Higher = more trusted.
///
/// Predefined levels:
/// - `SANDBOXED` (10) — untrusted external data
/// - `UNTRUSTED` (50) — user input, third-party API responses
/// - `TRUSTED` (150) — verified sources
/// - `ELEVATED` (200) — admin-level operations
/// - `SYSTEM` (255) — internal engine operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TrustLevel {
    /// The trust value (0-255).
    pub level: u8,
}

impl TrustLevel {
    /// Sandboxed trust (10).
    pub const SANDBOXED: Self = Self { level: 10 };
    /// Untrusted (50).
    pub const UNTRUSTED: Self = Self { level: 50 };
    /// Trusted (150).
    pub const TRUSTED: Self = Self { level: 150 };
    /// Elevated (200).
    pub const ELEVATED: Self = Self { level: 200 };
    /// System (255).
    pub const SYSTEM: Self = Self { level: 255 };

    /// Create a custom trust level.
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self { level }
    }

    /// Whether this trust level is at least the given minimum.
    #[must_use]
    pub const fn is_at_least(&self, min: Self) -> bool {
        self.level >= min.level
    }

    /// Lattice meet (minimum / greatest lower bound).
    #[must_use]
    pub fn meet(self, other: Self) -> Self {
        Self {
            level: self.level.min(other.level),
        }
    }

    /// Lattice join (maximum / least upper bound).
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        Self {
            level: self.level.max(other.level),
        }
    }
}

// No `Default` impl: trust must be a deliberate construction at every
// call site. `TrustLevel::default()` previously returned `UNTRUSTED` (50),
// which sits ABOVE `SANDBOXED` (10) in the lattice — a silent inversion of
// safe-by-default for capability gates using `is_at_least(SANDBOXED)`.
// Removed in Wave 3 (P1-2, rust-security).

impl fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.level {
            255 => "system",
            200 => "elevated",
            150 => "trusted",
            50 => "untrusted",
            10 => "sandboxed",
            n => return write!(f, "trust({n})"),
        };
        f.write_str(name)
    }
}

/// Error parsing a trust level from string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseTrustError {
    /// The string that failed to parse.
    pub input: String,
}

impl fmt::Display for ParseTrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown trust level: '{}'", self.input)
    }
}

impl core::error::Error for ParseTrustError {}

impl FromStr for TrustLevel {
    type Err = ParseTrustError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "system" => Ok(Self::SYSTEM),
            "elevated" => Ok(Self::ELEVATED),
            "trusted" => Ok(Self::TRUSTED),
            "untrusted" => Ok(Self::UNTRUSTED),
            "sandboxed" => Ok(Self::SANDBOXED),
            _ => s
                .parse::<u8>()
                .map(Self::new)
                .map_err(|_| ParseTrustError { input: s.into() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_levels_ordered() {
        assert!(TrustLevel::SANDBOXED < TrustLevel::UNTRUSTED);
        assert!(TrustLevel::UNTRUSTED < TrustLevel::TRUSTED);
        assert!(TrustLevel::TRUSTED < TrustLevel::ELEVATED);
        assert!(TrustLevel::ELEVATED < TrustLevel::SYSTEM);
    }

    #[test]
    fn is_at_least() {
        assert!(TrustLevel::SYSTEM.is_at_least(TrustLevel::ELEVATED));
        assert!(TrustLevel::TRUSTED.is_at_least(TrustLevel::TRUSTED));
        assert!(!TrustLevel::UNTRUSTED.is_at_least(TrustLevel::TRUSTED));
    }

    #[test]
    fn meet_returns_minimum() {
        let result = TrustLevel::TRUSTED.meet(TrustLevel::UNTRUSTED);
        assert_eq!(result, TrustLevel::UNTRUSTED);
    }

    #[test]
    fn join_returns_maximum() {
        let result = TrustLevel::TRUSTED.join(TrustLevel::ELEVATED);
        assert_eq!(result, TrustLevel::ELEVATED);
    }

    #[test]
    fn meet_is_commutative() {
        let a = TrustLevel::TRUSTED;
        let b = TrustLevel::SANDBOXED;
        assert_eq!(a.meet(b), b.meet(a));
    }

    #[test]
    fn join_is_commutative() {
        let a = TrustLevel::UNTRUSTED;
        let b = TrustLevel::ELEVATED;
        assert_eq!(a.join(b), b.join(a));
    }

    #[test]
    fn meet_is_idempotent() {
        let a = TrustLevel::TRUSTED;
        assert_eq!(a.meet(a), a);
    }

    #[test]
    fn display_named_levels() {
        assert_eq!(TrustLevel::SYSTEM.to_string(), "system");
        assert_eq!(TrustLevel::ELEVATED.to_string(), "elevated");
        assert_eq!(TrustLevel::TRUSTED.to_string(), "trusted");
        assert_eq!(TrustLevel::UNTRUSTED.to_string(), "untrusted");
        assert_eq!(TrustLevel::SANDBOXED.to_string(), "sandboxed");
    }

    #[test]
    fn display_custom_level() {
        assert_eq!(TrustLevel::new(100).to_string(), "trust(100)");
    }

    #[test]
    fn from_str_named() {
        assert_eq!("system".parse::<TrustLevel>().unwrap(), TrustLevel::SYSTEM);
        assert_eq!(
            "TRUSTED".parse::<TrustLevel>().unwrap(),
            TrustLevel::TRUSTED
        );
    }

    #[test]
    fn from_str_numeric() {
        assert_eq!("100".parse::<TrustLevel>().unwrap(), TrustLevel::new(100));
    }

    #[test]
    fn from_str_invalid() {
        let err = "bogus".parse::<TrustLevel>().unwrap_err();
        assert_eq!(err.input, "bogus");
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn serde_roundtrip() {
        let t = TrustLevel::ELEVATED;
        let json = serde_json::to_string(&t).expect("serialize");
        let back: TrustLevel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(t, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn trust_level_is_send_sync() {
        _assert_send_sync::<TrustLevel>();
    }

    // ── Lattice invariants (proptest) ──────────────────────────────

    proptest::proptest! {
        /// Meet ≤ both operands (greatest lower bound).
        #[test]
        fn meet_is_lower_bound(a in 0u8..=255, b in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            let m = ta.meet(tb);
            proptest::prop_assert!(m <= ta);
            proptest::prop_assert!(m <= tb);
        }

        /// Join ≥ both operands (least upper bound).
        #[test]
        fn join_is_upper_bound(a in 0u8..=255, b in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            let j = ta.join(tb);
            proptest::prop_assert!(j >= ta);
            proptest::prop_assert!(j >= tb);
        }

        /// Meet + join idempotence: meet(a,a) == a, join(a,a) == a.
        #[test]
        fn meet_join_idempotent(a in 0u8..=255) {
            let ta = TrustLevel::new(a);
            proptest::prop_assert_eq!(ta.meet(ta), ta);
            proptest::prop_assert_eq!(ta.join(ta), ta);
        }

        /// Commutativity: meet(a,b) == meet(b,a); join(a,b) == join(b,a).
        #[test]
        fn meet_join_commutative(a in 0u8..=255, b in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            proptest::prop_assert_eq!(ta.meet(tb), tb.meet(ta));
            proptest::prop_assert_eq!(ta.join(tb), tb.join(ta));
        }

        /// Associativity of meet + join across three values.
        #[test]
        fn meet_join_associative(a in 0u8..=255, b in 0u8..=255, c in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            let tc = TrustLevel::new(c);
            proptest::prop_assert_eq!(ta.meet(tb).meet(tc), ta.meet(tb.meet(tc)));
            proptest::prop_assert_eq!(ta.join(tb).join(tc), ta.join(tb.join(tc)));
        }

        /// Absorption: meet(a, join(a,b)) == a  and  join(a, meet(a,b)) == a.
        #[test]
        fn meet_join_absorption(a in 0u8..=255, b in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            proptest::prop_assert_eq!(ta.meet(ta.join(tb)), ta);
            proptest::prop_assert_eq!(ta.join(ta.meet(tb)), ta);
        }

        /// is_at_least matches numeric comparison.
        #[test]
        fn is_at_least_matches_ord(a in 0u8..=255, b in 0u8..=255) {
            let ta = TrustLevel::new(a);
            let tb = TrustLevel::new(b);
            proptest::prop_assert_eq!(ta.is_at_least(tb), a >= b);
        }

        /// FromStr numeric roundtrip for every u8.
        #[test]
        fn from_str_numeric_roundtrip(a in 0u8..=255) {
            use alloc::string::ToString;
            let ta = TrustLevel::new(a);
            // Serialise as decimal number (not named form).
            let parsed: TrustLevel = a.to_string().parse().unwrap();
            proptest::prop_assert_eq!(parsed, ta);
        }
    }
}
