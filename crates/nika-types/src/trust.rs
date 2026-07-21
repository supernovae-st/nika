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

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Trust level as a u8 lattice value. Higher = more trusted.
///
/// Predefined levels:
/// - `SANDBOXED` (10) — untrusted external data
/// - `UNTRUSTED` (50) — user input, third-party API responses
/// - `TRUSTED` (150) — verified sources
/// - `ELEVATED` (200) — admin-level operations
/// - `SYSTEM` (255) — internal engine operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

// ── The trust plane (descended from nika-schema at the C2 flag-day ─────
//
// Invocation source + builtin trust categories — provenance tracking over
// the DAG: `trust = min(all_input_trust_levels)` (conservative). Zero
// consumers in-tree today (seeded for the Shield plane); nika-schema's
// `trust` module re-exports this door so the public path is unchanged.

/// How the workflow was invoked — determines the trust floor for all inputs.
///
/// CLI inputs are trusted (user typed them). HTTP inputs from `nika serve`
/// are untrusted (arbitrary clients). This is the single most important
/// trust decision — it determines the trust floor for the entire DAG.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvocationSource {
    /// Direct CLI invocation: inputs = Trusted.
    Cli,
    /// HTTP via `nika serve`: inputs = Untrusted.
    Serve,
    /// Test/mock execution: inputs = Trusted.
    Test,
    /// Nested workflow via `nika:invoke`. Inputs inherit the caller's trust
    /// ceiling — propagates through arbitrary nesting.
    NestedRun {
        /// Trust ceiling inherited from the parent workflow.
        ceiling: TrustLevel,
    },
    /// Embedded SDK consumer with no explicit source. Fails closed:
    /// inputs default to Untrusted.
    #[default]
    Unknown,
}

impl InvocationSource {
    /// Create a new invocation source (defaults to `Unknown` / fail-closed).
    #[must_use]
    pub fn new() -> Self {
        Self::Unknown
    }

    /// Returns the trust floor for inputs based on invocation source.
    #[must_use]
    pub fn input_trust(&self) -> TrustLevel {
        match self {
            Self::Cli | Self::Test => TrustLevel::TRUSTED,
            Self::Serve | Self::Unknown => TrustLevel::UNTRUSTED,
            Self::NestedRun { ceiling } => *ceiling,
        }
    }
}

// ── Builtin trust categories ────────────────────────────────────────────
//
// Each `nika:*` builtin belongs to exactly one trust category. The output
// trust of a builtin invocation is determined by its category:
//
// - PROPAGATING: output trust = input trust — data transforms (in-process)
// - PURE:        output trust = Trusted always (no flow OR pure compute)
// - EXTERNAL:    output trust = Untrusted always (network/exec/side-effect I/O)
//
// Spec 22 has no REFERENCE-only builtins (media is DEFERRED to stdlib v0.x);
// PROPAGATING + REFERENCE were semantically equivalent in legacy trust.rs,
// so they consolidate into PROPAGATING here. REFERENCE re-instates when
// media builtins land (Phase v0.x · trigger-gated per LOCK-031).

/// Builtins that propagate input trust (data transforms · in-process · 7).
const TRUST_PROPAGATING_BUILTINS: &[&str] = &[
    "nika:convert",
    "nika:glob",
    "nika:grep",
    "nika:jq",
    "nika:json_diff",
    "nika:json_merge_patch",
    "nika:read",
];

/// Builtins with pure output (always Trusted, no flow OR pure compute · 10).
const TRUST_PURE_BUILTINS: &[&str] = &[
    "nika:assert",
    "nika:date",
    "nika:done",
    "nika:emit",
    "nika:hash",
    "nika:inspect",
    "nika:log",
    "nika:uuid",
    "nika:validate",
    "nika:wait",
];

/// Builtins with external I/O (always Untrusted output · 5).
const TRUST_EXTERNAL_BUILTINS: &[&str] = &[
    "nika:edit",
    "nika:fetch",
    "nika:notify",
    "nika:prompt",
    "nika:write",
];

/// Compute the output trust level for a builtin invocation.
///
/// **Fail-closed:** unknown `nika:*` tools default to
/// `merge(input, Untrusted)` to prevent trust laundering via
/// uncategorized builtins.
#[must_use]
pub fn builtin_output_trust(tool: &str, input_trust: TrustLevel) -> TrustLevel {
    if TRUST_PROPAGATING_BUILTINS.contains(&tool) {
        input_trust
    } else if TRUST_PURE_BUILTINS.contains(&tool) {
        TrustLevel::TRUSTED
    } else if TRUST_EXTERNAL_BUILTINS.contains(&tool) {
        TrustLevel::UNTRUSTED
    } else {
        // Unknown nika:* builtin → fail closed to untrusted
        input_trust.meet(TrustLevel::UNTRUSTED)
    }
}

/// Check if a tool is a known `nika:*` builtin in any trust category.
#[must_use]
pub fn is_categorized_builtin(tool: &str) -> bool {
    TRUST_PROPAGATING_BUILTINS.contains(&tool)
        || TRUST_PURE_BUILTINS.contains(&tool)
        || TRUST_EXTERNAL_BUILTINS.contains(&tool)
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
    fn from_str_maps_every_named_level() {
        // Each named arm must map to its specific level (deleting any arm would
        // make that string fall through to the numeric parse → Err, killing the
        // `delete match arm` mutants). Case-insensitive.
        use core::str::FromStr;
        assert_eq!(TrustLevel::from_str("system").unwrap(), TrustLevel::SYSTEM);
        assert_eq!(
            TrustLevel::from_str("elevated").unwrap(),
            TrustLevel::ELEVATED
        );
        assert_eq!(
            TrustLevel::from_str("TRUSTED").unwrap(),
            TrustLevel::TRUSTED
        );
        assert_eq!(
            TrustLevel::from_str("untrusted").unwrap(),
            TrustLevel::UNTRUSTED
        );
        assert_eq!(
            TrustLevel::from_str("Sandboxed").unwrap(),
            TrustLevel::SANDBOXED
        );
        // Numeric fallback still works; garbage errors.
        assert_eq!(TrustLevel::from_str("150").unwrap(), TrustLevel::TRUSTED);
        assert!(TrustLevel::from_str("nonsense").is_err());
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
    #[test]
    fn invocation_source_cli_is_trusted() {
        let src = InvocationSource::Cli;
        assert_eq!(src.input_trust(), TrustLevel::TRUSTED);
    }

    #[test]
    fn invocation_source_serve_is_untrusted() {
        let src = InvocationSource::Serve;
        assert_eq!(src.input_trust(), TrustLevel::UNTRUSTED);
    }

    #[test]
    fn invocation_source_unknown_fails_closed() {
        let src = InvocationSource::Unknown;
        assert_eq!(src.input_trust(), TrustLevel::UNTRUSTED);
    }

    #[test]
    fn invocation_source_nested_propagates_ceiling() {
        let src = InvocationSource::NestedRun {
            ceiling: TrustLevel::new(128),
        };
        assert_eq!(src.input_trust(), TrustLevel::new(128));
    }

    #[test]
    fn invocation_source_default_is_unknown() {
        assert_eq!(InvocationSource::default(), InvocationSource::Unknown);
    }

    #[test]
    fn propagating_builtin_preserves_trust() {
        assert_eq!(
            builtin_output_trust("nika:jq", TrustLevel::TRUSTED),
            TrustLevel::TRUSTED
        );
        assert_eq!(
            builtin_output_trust("nika:jq", TrustLevel::UNTRUSTED),
            TrustLevel::UNTRUSTED
        );
    }

    #[test]
    fn pure_builtin_always_trusted() {
        assert_eq!(
            builtin_output_trust("nika:wait", TrustLevel::UNTRUSTED),
            TrustLevel::TRUSTED
        );
    }

    #[test]
    fn external_builtin_always_untrusted() {
        assert_eq!(
            builtin_output_trust("nika:fetch", TrustLevel::TRUSTED),
            TrustLevel::UNTRUSTED
        );
    }

    #[test]
    fn unknown_builtin_fails_closed() {
        let result = builtin_output_trust("nika:nonexistent", TrustLevel::TRUSTED);
        // merge(Trusted, Untrusted) = Untrusted (min)
        assert_eq!(result, TrustLevel::UNTRUSTED);
    }

    #[test]
    fn legacy_builtins_unknown_post_d_n6() {
        // Builtins cut per D-2026-05-22-N6 + 2026-05-27 + ADR-086/087/088
        // Rams sweep must NOT be categorized (jq subsumes most · ADR-086
        // `csv_to_json` → universal `nika:convert` · ADR-087 `sleep` +
        // `wait_until` → unified `nika:wait` · ADR-088 4 introspection
        // builtins → unified `nika:inspect` with view: discriminator ·
        // the unknown-builtin fail-closed path applies).
        for legacy in [
            "nika:map",
            "nika:filter",
            "nika:json_merge",
            "nika:aggregate",
            "nika:enrich",
            "nika:group_by",
            "nika:pipeline",
            "nika:run",
            "nika:complete",
            "nika:noop",
            "nika:timestamp",
            "nika:template",
            "nika:format",
            "nika:encode",
            "nika:random",
            "nika:math",
            "nika:counter",
            "nika:env",
            "nika:exec",
            "nika:delete",
            "nika:publish",
            "nika:email",
            "nika:store",
            "nika:recall",
            "nika:embed",
            "nika:csv_to_json",
            "nika:sleep",
            "nika:wait_until",
            "nika:cost",
            "nika:records",
            "nika:dag_info",
            "nika:threads",
        ] {
            assert!(
                !is_categorized_builtin(legacy),
                "legacy builtin `{legacy}` must not be categorized post D-N6"
            );
        }
    }

    #[test]
    fn no_duplicate_categorization() {
        let categories: &[(&str, &[&str])] = &[
            ("PROPAGATING", TRUST_PROPAGATING_BUILTINS),
            ("PURE", TRUST_PURE_BUILTINS),
            ("EXTERNAL", TRUST_EXTERNAL_BUILTINS),
        ];

        for (a_name, a_list) in categories {
            for tool in *a_list {
                for (b_name, b_list) in categories {
                    if *a_name == *b_name {
                        continue;
                    }
                    assert!(
                        !b_list.contains(tool),
                        "{tool} appears in both {a_name} and {b_name}"
                    );
                }
            }
        }
    }

    #[test]
    fn category_totals_match_spec_22() {
        assert_eq!(
            TRUST_PROPAGATING_BUILTINS.len(),
            7,
            "expected 7 propagating builtins"
        );
        assert_eq!(TRUST_PURE_BUILTINS.len(), 10, "expected 10 pure builtins");
        assert_eq!(
            TRUST_EXTERNAL_BUILTINS.len(),
            5,
            "expected 5 external builtins"
        );
        let total = TRUST_PROPAGATING_BUILTINS.len()
            + TRUST_PURE_BUILTINS.len()
            + TRUST_EXTERNAL_BUILTINS.len();
        assert_eq!(total, 22, "trust categorization total must equal spec 22");
    }

    #[test]
    fn is_categorized_builtin_true_for_known() {
        assert!(is_categorized_builtin("nika:jq"));
        assert!(is_categorized_builtin("nika:fetch"));
        assert!(is_categorized_builtin("nika:wait"));
        assert!(is_categorized_builtin("nika:notify"));
        assert!(is_categorized_builtin("nika:validate"));
        assert!(is_categorized_builtin("nika:inspect"));
    }

    #[test]
    fn is_categorized_builtin_false_for_unknown() {
        assert!(!is_categorized_builtin("nika:nonexistent"));
        assert!(!is_categorized_builtin("custom:tool"));
    }
}
