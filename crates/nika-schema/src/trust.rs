// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trust system for Nika Shield — compile-time data provenance tracking.
//!
//! Every value flowing through a Nika DAG has a `TrustLevel` (from `nika-types`).
//! The propagation rule is conservative: `trust = min(all_input_trust_levels)`.
//!
//! This module adds schema-level trust primitives:
//! - [`InvocationSource`] — how the workflow was invoked (determines input trust)
//! - [`builtin_output_trust`] — output trust for each `nika:*` builtin tool
//!
//! Builtin set reconciled to spec 26 per `nika/spec/stdlib/builtins-v0.1.md`
//! (D-2026-05-22-N6 stdlib-collapse 42→26 + 2026-05-27 `json_merge` cut ·
//! jq subsumes legacy data transforms · media DEFERRED to stdlib v0.x).

// Re-export for convenience — consumers can use `nika_schema::trust::TrustLevel`.
pub use nika_error::TrustLevel;

// ── Invocation source ───────────────────────────────────────────────────────

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

// ── Builtin trust categories ────────────────────────────────────────────────
//
// Each `nika:*` builtin belongs to exactly one trust category. The output
// trust of a builtin invocation is determined by its category:
//
// - PROPAGATING: output trust = input trust — data transforms (in-process)
// - PURE:        output trust = Trusted always (no flow OR pure compute)
// - EXTERNAL:    output trust = Untrusted always (network/exec/side-effect I/O)
//
// Spec 26 has no REFERENCE-only builtins (media is DEFERRED to stdlib v0.x);
// PROPAGATING + REFERENCE were semantically equivalent in legacy trust.rs,
// so they consolidate into PROPAGATING here. REFERENCE re-instates when
// media builtins land (Phase v0.x · trigger-gated per LOCK-031).

/// Builtins that propagate input trust (data transforms · in-process · 7).
const TRUST_PROPAGATING_BUILTINS: &[&str] = &[
    "nika:csv_to_json",
    "nika:glob",
    "nika:grep",
    "nika:jq",
    "nika:json_diff",
    "nika:json_merge_patch",
    "nika:read",
];

/// Builtins with pure output (always Trusted, no flow OR pure compute · 14).
const TRUST_PURE_BUILTINS: &[&str] = &[
    "nika:assert",
    "nika:cost",
    "nika:dag_info",
    "nika:date",
    "nika:done",
    "nika:emit",
    "nika:hash",
    "nika:log",
    "nika:records",
    "nika:sleep",
    "nika:threads",
    "nika:uuid",
    "nika:validate",
    "nika:wait_until",
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
            builtin_output_trust("nika:sleep", TrustLevel::UNTRUSTED),
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
        // Builtins cut per D-2026-05-22-N6 must NOT be categorized (jq
        // subsumes them · the unknown-builtin fail-closed path applies).
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
    fn category_totals_match_spec_26() {
        assert_eq!(
            TRUST_PROPAGATING_BUILTINS.len(),
            7,
            "expected 7 propagating builtins"
        );
        assert_eq!(TRUST_PURE_BUILTINS.len(), 14, "expected 14 pure builtins");
        assert_eq!(
            TRUST_EXTERNAL_BUILTINS.len(),
            5,
            "expected 5 external builtins"
        );
        let total = TRUST_PROPAGATING_BUILTINS.len()
            + TRUST_PURE_BUILTINS.len()
            + TRUST_EXTERNAL_BUILTINS.len();
        assert_eq!(total, 26, "trust categorization total must equal spec 26");
    }

    #[test]
    fn is_categorized_builtin_true_for_known() {
        assert!(is_categorized_builtin("nika:jq"));
        assert!(is_categorized_builtin("nika:fetch"));
        assert!(is_categorized_builtin("nika:sleep"));
        assert!(is_categorized_builtin("nika:notify"));
        assert!(is_categorized_builtin("nika:validate"));
    }

    #[test]
    fn is_categorized_builtin_false_for_unknown() {
        assert!(!is_categorized_builtin("nika:nonexistent"));
        assert!(!is_categorized_builtin("custom:tool"));
    }
}
