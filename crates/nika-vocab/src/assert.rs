// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `assert:` — the author's obligations (spec `15-proof.md` §assert).
//!
//! A task or workflow declares assertions the engine JUDGES — distinct from
//! `nika:assert` (the single-condition fail-fast builtin). `assert:` is a
//! **closed vocabulary** of properties, each judged at an HONEST level:
//!
//! ```yaml
//! assert:
//!   - no_secret_egress                       # the flow laws of spec 10 hold
//!   - eventually: { task: deploy, state: success }
//!   - before: { first: gate, second: deploy }
//!   - bounded: { task: crawl, max_iterations: 100 }
//!   - resource: { cost_usd: { max: 5.00 } }
//! ```
//!
//! ## The three levels · claim ≤ evidence (normative)
//!
//! - [`AssertLevel::StaticProof`] — decidable at `nika check` on the graph/IR
//!   (an ordering law · a static bound). The strongest, claimable ONLY when
//!   the check genuinely decides it.
//! - [`AssertLevel::TraceVerified`] — decided by `nika trace verify` against a
//!   completed run's trace (spec 13). What only the trace can see is judged
//!   there, never optimistically promoted to `StaticProof`.
//! - [`AssertLevel::Unknown`] — honestly unresolved. Never dressed up.
//!
//! A [`AssertLevel::StaticProof`] claim the IR cannot decide is itself a
//! refusal ([`NIKA_ASSERT_001`]). Bounded/statistical assertions stay LAB
//! (calibrated research · never a shipped guarantee).
//!
//! ## The second evaluator
//!
//! [`AssertProperty::level`] and [`AssertProperty::check_claim`] mirror
//! `proof_core.assert_level` + `proof_core.check_assert_claim` — the eight
//! assert laws of `proof_core_selftest.py` are pinned in [`tests`].
//!
//! ## Honest scope (spec 15 · what this is)
//!
//! This module owns the CLASS-level leveling: which properties are statically
//! decidable at all (`before` · `bounded` · `no_secret_egress`) versus
//! trace-settled (`eventually` · `resource`), and the claim-≤-evidence guard.
//! Whether `nika check` GENUINELY decides a specific `before`/`bounded`
//! INSTANCE on the derived graph (and may thus honestly claim `StaticProof`
//! for it), and `nika trace verify` judging the trace-level assertions
//! against a real Outcome IR (spec 13), are the deeper integrations — named
//! owed, never simulated.

use serde_json::Value;

/// The mis-leveled-obligation refusal code (spec 15 · canon.yaml
/// `validation_error`).
pub const NIKA_ASSERT_001: &str = "NIKA-ASSERT-001";

/// The honest level an assertion is judged at (spec 15 · the reference's
/// `ASSERT_LEVELS`, weakest → strongest by [`AssertLevel::rank`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssertLevel {
    /// Honestly unresolved — no static check and no available trace settles it.
    Unknown,
    /// Decided by `nika trace verify` against a completed run's trace (13).
    TraceVerified,
    /// Decidable at `nika check` on the graph/IR — the strongest.
    StaticProof,
}

impl AssertLevel {
    /// The evidence rank (weakest 0 → strongest 2) — the reference's `rank`
    /// map. A claim outranking the achievable evidence is refused.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::TraceVerified => 1,
            Self::StaticProof => 2,
        }
    }

    /// The wire spelling (the reference's level strings).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::TraceVerified => "TraceVerified",
            Self::StaticProof => "StaticProof",
        }
    }
}

/// The closed vocabulary of `assert:` properties (spec 15). A property outside
/// the five is not a v1 assertion — [`AssertProperty::parse`] refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssertProperty {
    /// `no_secret_egress` — no secret reaches an unsanctioned sink (the flow
    /// laws of spec 10 hold across the whole run). Statically decidable.
    NoSecretEgress,
    /// `eventually: { task, state }` — the named task reaches the named
    /// terminal state (the Outcome of spec 13). Trace-settled.
    Eventually {
        /// The task that must reach a terminal state.
        task: String,
        /// The terminal state it must reach.
        state: String,
    },
    /// `before: { first, second }` — an ordering law on the derived graph
    /// (spec 03). Statically decidable.
    Before {
        /// The task that must run first.
        first: String,
        /// The task that must run after.
        second: String,
    },
    /// `bounded: { task, max_iterations }` — a `for_each`/agent loop stays
    /// under its cap. Statically decidable.
    Bounded {
        /// The bounded loop task.
        task: String,
        /// The iteration cap.
        max_iterations: u64,
    },
    /// `resource: { cost_usd: { max } }` — the symbolic certificate's cost
    /// bound (spec 05) holds. Trace-settled. The money rides as its authored
    /// string (no float field · the fixed-point discipline of spec 11/05).
    Resource {
        /// The `cost_usd.max` ceiling, as authored (float-free carrier).
        cost_usd_max: String,
    },
}

/// An `assert:` refusal (spec 15 · [`NIKA_ASSERT_001`]). A plain data carrier,
/// NOT a `thiserror` enum — it never crosses a `NikaErrorCode` boundary (the
/// `decide.rs` `DecideError` posture, so nika-vocab keeps its single
/// allowlisted `GoDurationError`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AssertRefusal {
    /// Always [`NIKA_ASSERT_001`].
    pub code: &'static str,
    /// The human-readable law that refused.
    pub message: String,
}

impl AssertRefusal {
    fn new(message: impl Into<String>) -> Self {
        Self {
            code: NIKA_ASSERT_001,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AssertRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl AssertProperty {
    /// The property name (the reference's `_property_name`) — the token that
    /// drives leveling.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NoSecretEgress => "no_secret_egress",
            Self::Eventually { .. } => "eventually",
            Self::Before { .. } => "before",
            Self::Bounded { .. } => "bounded",
            Self::Resource { .. } => "resource",
        }
    }

    /// Statically decidable at `nika check` (`before` · `bounded` ·
    /// `no_secret_egress`) — the reference's `STATIC_PROPERTIES`.
    #[must_use]
    const fn is_static(&self) -> bool {
        matches!(
            self,
            Self::NoSecretEgress | Self::Before { .. } | Self::Bounded { .. }
        )
    }

    /// The HONEST level this assertion can be judged at right now (spec 15 ·
    /// mirror of `proof_core.assert_level`): `StaticProof` iff statically
    /// decidable · else `TraceVerified` when a trace exists · else `Unknown`.
    /// Never optimistic — a trace property is never promoted to `StaticProof`.
    #[must_use]
    pub const fn level(&self, trace_available: bool) -> AssertLevel {
        if self.is_static() {
            AssertLevel::StaticProof
        } else if trace_available {
            AssertLevel::TraceVerified
        } else {
            AssertLevel::Unknown
        }
    }

    /// Check a CLAIMED level against the evidence (spec 15 · mirror of
    /// `proof_core.check_assert_claim`): a claim outranking what the evidence
    /// supports is a refusal ([`NIKA_ASSERT_001`] · claim ≤ evidence). A
    /// `StaticProof` claim on a trace-only property, or a `TraceVerified`
    /// claim with no trace, is refused. (The reference's "level not in the
    /// set" guard is here unrepresentable — [`AssertLevel`] is closed.)
    ///
    /// # Errors
    ///
    /// [`AssertRefusal`] ([`NIKA_ASSERT_001`]) when `claimed` outranks the
    /// achievable level.
    pub fn check_claim(
        &self,
        claimed: AssertLevel,
        trace_available: bool,
    ) -> Result<(), AssertRefusal> {
        let achievable = self.level(trace_available);
        if claimed.rank() > achievable.rank() {
            return Err(AssertRefusal::new(format!(
                "assert · {} claims {} but the evidence only supports {} \
                 (claim ≤ evidence · {NIKA_ASSERT_001})",
                self.name(),
                claimed.as_str(),
                achievable.as_str()
            )));
        }
        Ok(())
    }

    /// Parse an authored assertion — a bare string (`no_secret_egress`) OR a
    /// single-key map (`{ before: { … } }`) — into the closed vocabulary
    /// (spec 15 · the reference's `_property_name` + the closed-set gate). A
    /// property outside the five, or a malformed body, is a refusal
    /// ([`NIKA_ASSERT_001`]).
    ///
    /// # Errors
    ///
    /// [`AssertRefusal`] on an unknown property, a non-v1 shape, or a
    /// malformed body.
    pub fn parse(value: &Value) -> Result<Self, AssertRefusal> {
        match value {
            // The one param-less form is authored as a bare string.
            Value::String(s) if s == "no_secret_egress" => Ok(Self::NoSecretEgress),
            Value::String(other) => Err(AssertRefusal::new(format!(
                "assert · {other:?} is not a v1 assertion property \
                 (the only bare form is `no_secret_egress` · {NIKA_ASSERT_001})"
            ))),
            // The single-key map — its key is the property name (len==1
            // guarantees exactly one entry).
            Value::Object(map) if map.len() == 1 => match map.iter().next() {
                Some((name, body)) => Self::parse_keyed(name, body),
                None => Err(AssertRefusal::new(format!(
                    "assert · empty assertion map ({NIKA_ASSERT_001})"
                ))),
            },
            other => Err(AssertRefusal::new(format!(
                "assert · not a v1 assertion property (a bare string or a \
                 single-key map · got {other} · {NIKA_ASSERT_001})"
            ))),
        }
    }

    /// The single-key-map forms (`before`/`eventually`/`bounded`/`resource`)
    /// — an unknown key is the "not a v1 assertion property" refusal.
    fn parse_keyed(name: &str, body: &Value) -> Result<Self, AssertRefusal> {
        match name {
            "before" => Ok(Self::Before {
                first: string_field(body, name, "first")?,
                second: string_field(body, name, "second")?,
            }),
            "eventually" => Ok(Self::Eventually {
                task: string_field(body, name, "task")?,
                state: string_field(body, name, "state")?,
            }),
            "bounded" => Ok(Self::Bounded {
                task: string_field(body, name, "task")?,
                max_iterations: u64_field(body, name, "max_iterations")?,
            }),
            "resource" => Ok(Self::Resource {
                cost_usd_max: cost_max(body)?,
            }),
            other => Err(AssertRefusal::new(format!(
                "assert · {other:?} is not a v1 assertion property \
                 (the set is closed · {NIKA_ASSERT_001})"
            ))),
        }
    }
}

/// A required string field of an assertion body.
fn string_field(body: &Value, property: &str, field: &str) -> Result<String, AssertRefusal> {
    body.get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            AssertRefusal::new(format!(
                "assert.{property}.{field} · a string is required ({NIKA_ASSERT_001})"
            ))
        })
}

/// A required non-negative integer field (`max_iterations`).
fn u64_field(body: &Value, property: &str, field: &str) -> Result<u64, AssertRefusal> {
    body.get(field).and_then(Value::as_u64).ok_or_else(|| {
        AssertRefusal::new(format!(
            "assert.{property}.{field} · a non-negative integer is required ({NIKA_ASSERT_001})"
        ))
    })
}

/// The `resource.cost_usd.max` ceiling, carried as its authored numeric
/// spelling (float-free: the money rides as a string, never a float field).
fn cost_max(body: &Value) -> Result<String, AssertRefusal> {
    match body.get("cost_usd").and_then(|c| c.get("max")) {
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(AssertRefusal::new(format!(
            "assert.resource.cost_usd.max · a number is required ({NIKA_ASSERT_001})"
        ))),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_pass_by_value
)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prop(value: Value) -> AssertProperty {
        AssertProperty::parse(&value).expect("a v1 property")
    }

    // ── the eight reference assert laws (proof_core_selftest.py) ─────────

    #[test]
    fn a_static_property_levels_static_proof() {
        let before = prop(json!({"before": {"first": "a", "second": "b"}}));
        assert_eq!(before.level(false), AssertLevel::StaticProof);
    }

    #[test]
    fn no_secret_egress_is_static_proof() {
        assert_eq!(
            prop(json!("no_secret_egress")).level(false),
            AssertLevel::StaticProof
        );
    }

    #[test]
    fn a_trace_property_with_no_trace_is_unknown() {
        let ev = prop(json!({"eventually": {"task": "t", "state": "success"}}));
        assert_eq!(ev.level(false), AssertLevel::Unknown);
    }

    #[test]
    fn a_trace_property_with_a_trace_is_trace_verified() {
        let ev = prop(json!({"eventually": {"task": "t", "state": "success"}}));
        assert_eq!(ev.level(true), AssertLevel::TraceVerified);
    }

    #[test]
    fn claiming_static_proof_on_a_trace_only_property_is_refused() {
        let ev = prop(json!({"eventually": {"task": "t", "state": "success"}}));
        let err = ev
            .check_claim(AssertLevel::StaticProof, true)
            .expect_err("StaticProof on a trace-only property is refused");
        assert_eq!(err.code, NIKA_ASSERT_001);
    }

    #[test]
    fn claiming_trace_verified_with_no_trace_is_refused() {
        let res = prop(json!({"resource": {"cost_usd": {"max": 5}}}));
        let err = res
            .check_claim(AssertLevel::TraceVerified, false)
            .expect_err("TraceVerified with no trace is refused (Unknown only)");
        assert_eq!(err.code, NIKA_ASSERT_001);
    }

    #[test]
    fn an_honest_static_proof_claim_is_accepted() {
        let bounded = prop(json!({"bounded": {"task": "c", "max_iterations": 100}}));
        bounded
            .check_claim(AssertLevel::StaticProof, false)
            .expect("an honest StaticProof claim is accepted");
    }

    #[test]
    fn an_unknown_assertion_property_is_refused() {
        let err = AssertProperty::parse(&json!({"telepathy": {}}))
            .expect_err("an unknown property is refused");
        assert_eq!(err.code, NIKA_ASSERT_001);
        // A bare-string unknown is refused too.
        assert!(AssertProperty::parse(&json!("telepathy")).is_err());
    }

    // ── vocabulary shape · parse round-trips the five forms ─────────────

    #[test]
    fn the_five_forms_parse_to_their_names() {
        assert_eq!(prop(json!("no_secret_egress")).name(), "no_secret_egress");
        assert_eq!(
            prop(json!({"before": {"first": "a", "second": "b"}})).name(),
            "before"
        );
        assert_eq!(
            prop(json!({"eventually": {"task": "t", "state": "success"}})).name(),
            "eventually"
        );
        assert_eq!(
            prop(json!({"bounded": {"task": "c", "max_iterations": 3}})).name(),
            "bounded"
        );
        assert_eq!(
            prop(json!({"resource": {"cost_usd": {"max": 5}}})).name(),
            "resource"
        );
    }

    #[test]
    fn a_malformed_body_is_refused() {
        // before without `second`.
        assert!(AssertProperty::parse(&json!({"before": {"first": "a"}})).is_err());
        // bounded with a non-integer cap.
        assert!(
            AssertProperty::parse(&json!({"bounded": {"task": "c", "max_iterations": "x"}}))
                .is_err()
        );
        // resource without cost_usd.max.
        assert!(AssertProperty::parse(&json!({"resource": {}})).is_err());
        // a two-key map is not a v1 assertion shape.
        assert!(AssertProperty::parse(&json!({"before": {}, "after": {}})).is_err());
    }

    #[test]
    fn the_levels_rank_weakest_to_strongest() {
        assert!(AssertLevel::Unknown.rank() < AssertLevel::TraceVerified.rank());
        assert!(AssertLevel::TraceVerified.rank() < AssertLevel::StaticProof.rank());
    }
}
