// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authority over a seat's requests. A repair count is not an authority (pack 94): the door
//! may state an explicit request bound, and these counters enforce it. Without one they still
//! count every request. [`Seat`] counts the invocations of any seat; [`Wire`] counts the physical
//! requests of a direct API seat where its bytes leave, so a transport retry or a
//! structured-output fallback inside one invocation is a request too. A seat whose own requests
//! cannot be observed (an agent harness) is bounded by its invocations alone. A request past the
//! authority is refused before any byte leaves, and every refusal is counted beside what was
//! sent, never over the core's own journal of attempts. [`worst_case_of`] states what a
//! configuration can bound without its request, and [`Authority`] resolves a door's bound against what its caller
//! typed, before any request. The counters bound requests, never dollars.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::NativeMode;

/// The physical request counters a seat runs under, owned by the provider layer: [`Envelope`]
/// holds a bound (or none) and counts, [`Seat`] counts a seat's invocations, [`Wire`] a direct
/// API seat's requests. This authority resolves which bound they hold.
pub use nika_providers::authoring::requests::{Envelope, Seat, Wire};

/// Historical request estimate of the former capped compiler, retained for source compatibility.
/// It does not bound the current compiler: transform synthesis follows every unstated computation,
/// whose count is not known from these arguments. Use [`worst_case_of`] for current accounting;
/// an explicit allowance is enforced by the request counters, never by this historical estimate.
/// The former author-provider verifier was included (R4 A11, nv1b). A separately selected
/// decision seat keeps its own request observations. With `s` samples
/// formerly clamped to 1..=5 and `r` repairs as the policy holds them:
/// - an edit's native revision: the candidate and its repairs, then the whole-request judgment
///   and its locate question (`verify::WHOLE_QUESTIONS`, 2): `3 + r`;
/// - the sketch door: the sketch, its fills and `r` repairs (a fill, or the graph reproposed),
///   each new candidate judged afresh: `2 + r + 2 (1 + r) = 4 + 3r`;
/// - COLD: each sample and its one evidence repair (`2s`), then `1 + r` verification attempts,
///   each asking at most 8 clause questions, the whole-request judgment (2) and one transform
///   synthesis of at most 2 questions;
///   each of the `r` verify repairs is one call, and the transform repairs share one allowance of
///   `r`: `2s + 12 (1 + r) + 2r = 2s + 14r + 12`;
/// - escalate: COLD, then the sketch door with one repair less (`3r + 1`; none when `r` is 0);
/// - only: no request for a creation, which this core no longer authors as whole source.
///
/// Arithmetic saturates rather than overflows. These historical caps are not current limits.
#[must_use]
#[deprecated(note = "historical capped-engine estimate; use worst_case_of for current accounting")]
pub fn worst_case(strategy: NativeMode, samples: u32, repairs: u32, edit: bool) -> u32 {
    historical_worst_case(strategy, samples, repairs, edit)
}

fn historical_worst_case(strategy: NativeMode, samples: u32, repairs: u32, edit: bool) -> u32 {
    let samples = samples.clamp(1, 5);
    let count = |questions: usize| u32::try_from(questions).unwrap_or(u32::MAX);
    let judged = count(crate::cognition::WHOLE_QUESTIONS);
    let rounds = |repairs: u32| repairs.saturating_add(1);
    let native = rounds(repairs).saturating_add(judged);
    let sketch = |repairs: u32| {
        (repairs.saturating_add(2)).saturating_add(judged.saturating_mul(rounds(repairs)))
    };
    let attempt = 8_u32.saturating_add(judged).saturating_add(2);
    let cold = (samples.saturating_mul(2))
        .saturating_add(rounds(repairs).saturating_mul(attempt))
        .saturating_add(repairs.saturating_mul(2));
    match strategy {
        NativeMode::Off if edit => 0,
        NativeMode::Off => cold,
        _ if edit => native,
        NativeMode::Only => 0,
        NativeMode::Sketch => sketch(repairs),
        _ => cold.saturating_add(repairs.checked_sub(1).map_or(0, sketch)),
    }
}

/// The request-independent worst case of the current policy. COLD creation (`off` or
/// `escalate`) follows every unstated computation, so its request count is unknown here even
/// with an explicit repair limit. The sketch door's judgment asks each part of a doubted request
/// alone, the task a missing part points to, the extra-operation question and the questions over
/// a trial run: its count depends on the request too. Revisions also depend on the retained
/// representation and its link, fill and repeated judgment work, which these arguments do not
/// describe. A strategy with no repair limit has no finite worst case wherever repairs add
/// requests, or the finite estimate exceeds the count representation.
/// `None` never grants more requests: the explicit authority still refuses each attempt past
/// its bound.
#[must_use]
pub fn worst_case_of(
    strategy: NativeMode,
    samples: u32,
    repairs: Option<u32>,
    edit: bool,
) -> Option<u32> {
    // Neither the samples nor the repairs bound a request-dependent judgment: kept for callers.
    let _ = (samples, repairs);
    match (strategy, edit) {
        (NativeMode::Off, true) | (NativeMode::Only, false) => Some(0),
        _ => None,
    }
}

/// The requests an explicit source recovery reserves at least: the source and the whole-request
/// judgment per round (`1 + 2`), drawn from the same authority, never beside it. A doubted
/// judgment asks more (each part of the request alone, then the extra-operation question): the
/// counters refuse what the allowance cannot hold, never a silent cut. The rounds count as typed.
#[must_use]
pub fn recovery_requests(rounds: u32) -> u32 {
    let judged = u32::try_from(crate::cognition::WHOLE_QUESTIONS).unwrap_or(u32::MAX);
    rounds.saturating_mul(judged.saturating_add(1))
}

/// Whether a receipt's usage totals are complete, read from its calls' own results: each call
/// answered with its usage, or was refused by a local admission before any byte left (it used
/// none). A timeout, an answer without usage or a provider failure (which may have been
/// billed) leaves the totals partial: a lower bound, never a total.
#[must_use]
pub fn usage_complete(context: &[Value]) -> bool {
    context.iter().map(|entry| &entry["result"]).all(|result| {
        result["failure_kind"] == "admission_refused" || result["usage_reported"] == true
    })
}

/// The requests a typed strategy needs at least before its READY can be judged (nv1b): the plan,
/// then its judgment (2); the sketch, its fills, then their judgment (3). A strategy no typed
/// minimum names needs one; `only` creates nothing, so no grant of more requests can help it.
#[must_use]
pub const fn least_requests(strategy: NativeMode) -> u32 {
    match strategy {
        NativeMode::Sketch => 3,
        NativeMode::Escalate => 2,
        _ => 1,
    }
}

/// The single request a door that wants one by default grants explicitly (`Some(1)`). The core
/// itself grants no default bound: an authority whose door names none counts its requests and
/// refuses none.
pub const DEFAULT_MAX_CALLS: u32 = 1;

/// The COLD samples the core runs; a typed count outside is refused, never clamped into another.
pub const SAMPLES: std::ops::RangeInclusive<u32> = 1..=u32::MAX;

/// The native repairs the core runs: every typed count, as typed (the core clamps none; a policy
/// may also state no count). A typed count is refused only when the authority cannot honor it
/// ([`Refusal::Multiplicity`]).
pub const REPAIRS: std::ops::RangeInclusive<u32> = 0..=u32::MAX;

/// How a door names its authority to its caller: the spelling of the grant of more requests
/// (`--authoring-max-calls` at the CLI), and what a request refused past it is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Door {
    /// The grant's spelling at this door.
    pub grant: &'static str,
    /// The remedy a refused request carries.
    pub remedy: &'static str,
}

impl Door {
    /// A door that grants more requests through `grant` and tells a refused request `remedy`.
    #[must_use]
    pub const fn new(grant: &'static str, remedy: &'static str) -> Self {
        Self { grant, remedy }
    }
}

/// What the caller typed, beside what its door defaults: a typed multiplicity is honored in full
/// or refused before any request; a default runs within the authority, whose receipt states
/// where it stopped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Typed {
    /// Whether the caller named the strategy itself.
    pub strategy: bool,
    /// The COLD samples, when typed.
    pub samples: Option<u32>,
    /// The native repairs, when typed.
    pub repairs: Option<u32>,
    /// Whether the request revises a base rather than creating.
    pub edit: bool,
}

impl Typed {
    /// Nothing typed, for a creation or (`edit`) a revision.
    #[must_use]
    pub const fn new(edit: bool) -> Self {
        Self {
            strategy: false,
            samples: None,
            repairs: None,
            edit,
        }
    }

    /// The caller named the strategy.
    #[must_use]
    pub const fn with_strategy(mut self) -> Self {
        self.strategy = true;
        self
    }

    /// The COLD samples the caller typed, if any.
    #[must_use]
    pub const fn with_samples(mut self, samples: Option<u32>) -> Self {
        self.samples = samples;
        self
    }

    /// The native repairs the caller typed, if any.
    #[must_use]
    pub const fn with_repairs(mut self, repairs: Option<u32>) -> Self {
        self.repairs = repairs;
        self
    }
}

/// Why an authority refuses what its caller typed, before any request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// Typed repairs or samples can need `needed` requests under `strategy`: more than granted.
    Multiplicity {
        /// The requests the typed values can need.
        needed: u32,
        /// The requests the authority grants.
        authorized: u32,
        /// The resolved strategy.
        strategy: NativeMode,
    },
    /// A typed strategy granted fewer requests than it needs before its READY can be judged
    /// ([`least_requests`]; `steps` names them).
    Strategy {
        /// The typed strategy.
        strategy: NativeMode,
        /// What its requests are.
        steps: &'static str,
    },
    /// A typed count the core would run as another: out of its range, or a grant of no request.
    Range {
        /// Which count: `samples`, `repairs` or `max_calls`.
        name: &'static str,
        /// The count typed.
        typed: u32,
        /// The least the core runs.
        least: u32,
        /// The most the core runs, when bounded.
        most: Option<u32>,
    },
}

/// The first typed count the core would run as another: samples or repairs out of their range,
/// or a grant of no request at all.
fn out_of_range(
    max_calls: Option<u32>,
    samples: Option<u32>,
    repairs: Option<u32>,
) -> Option<Refusal> {
    let outside = |name, typed: Option<u32>, range: &std::ops::RangeInclusive<u32>| {
        typed
            .filter(|count| !range.contains(count))
            .map(|typed| Refusal::Range {
                name,
                typed,
                least: *range.start(),
                most: Some(*range.end()),
            })
    };
    outside("samples", samples, &SAMPLES)
        .or_else(|| outside("repairs", repairs, &REPAIRS))
        .or_else(|| {
            max_calls
                .filter(|max| *max == 0)
                .map(|typed| Refusal::Range {
                    name: "max_calls",
                    typed,
                    least: 1,
                    most: None,
                })
        })
}

/// The authority one compile runs under, resolved before any request: its bound (none when the
/// door names none), whether the caller named it, and what the configuration could ask for.
#[derive(Debug)]
pub struct Authority {
    max: Option<u32>,
    explicit: bool,
    configured: Value,
    door: Door,
}

impl Authority {
    /// The authority `max_calls` grants (no bound when absent: every request counted, none
    /// refused) under the resolved strategy, or the refusal of a typed multiplicity a typed bound
    /// cannot honor (never a silent reduction). Defaults run within the authority, and the
    /// receipt states where they stopped.
    /// A typed value the strategy cannot apply (repairs under `off`, samples where no plan is
    /// sampled, a strategy's kind in an edit, which revises natively) changes no request: it is
    /// recorded as ignored, never refused.
    ///
    /// # Errors
    /// [`Refusal::Range`] for typed samples outside [`SAMPLES`] or a grant of zero requests;
    /// [`Refusal::Multiplicity`] when a known finite request count under typed repairs exceeds
    /// both the defaults' and a typed bound. A request-dependent count remains unknown and is
    /// enforced by the counters; [`Refusal::Strategy`] for a typed
    /// escalate or sketch granted one request, outside an edit.
    pub fn resolve(
        max_calls: Option<u32>,
        strategy: NativeMode,
        typed: Typed,
        door: Door,
    ) -> Result<Self, Refusal> {
        let Typed {
            samples,
            repairs,
            edit,
            ..
        } = typed;
        if let Some(refusal) = out_of_range(max_calls, samples, repairs) {
            return Err(refusal);
        }
        let max = max_calls;
        let baseline = worst_case_of(strategy, 1, Some(0), edit);
        let needed = worst_case_of(
            strategy,
            samples.unwrap_or(1),
            Some(repairs.unwrap_or(0)),
            edit,
        );
        if let Some((needed, baseline)) = needed.zip(baseline)
            && let Some(authorized) = max.filter(|max| needed > baseline && needed > *max)
        {
            return Err(Refusal::Multiplicity {
                needed,
                authorized,
                strategy,
            });
        }
        // A typed strategy is honored in full or refused here: a READY is judged, so the plan or
        // the sketch and its fills are not enough alone (nv1b). A creation under `only` is refused
        // by the core with its migration, never by a grant it could buy.
        let steps = match strategy {
            _ if edit || !typed.strategy => None,
            NativeMode::Escalate => Some("the plan, then its judgment"),
            NativeMode::Sketch => Some("the sketch, its fills, then their judgment"),
            _ => None,
        };
        let short = max.is_some_and(|max| max < least_requests(strategy));
        if let Some(steps) = steps.filter(|_| short) {
            return Err(Refusal::Strategy { strategy, steps });
        }
        let ignored: Vec<&str> = [
            (
                "strategy",
                typed.strategy && edit && strategy != NativeMode::Off,
            ),
            (
                "samples",
                samples.is_some()
                    && (edit || matches!(strategy, NativeMode::Only | NativeMode::Sketch)),
            ),
            (
                "repairs",
                repairs.is_some()
                    && matches!(
                        (strategy, edit),
                        (NativeMode::Off, true) | (NativeMode::Only, false)
                    ),
            ),
        ]
        .into_iter()
        .filter_map(|(value, ignored)| ignored.then_some(value))
        .collect();
        // Untyped repairs are the core's own default: no count (null), so no finite worst case
        // wherever a repair buys a request (null).
        let samples = samples.unwrap_or(1);
        Ok(Self {
            max,
            explicit: max_calls.is_some(),
            configured: json!({
                "strategy": strategy.word(),
                "samples": samples,
                "repairs": repairs,
                "worst_case": worst_case_of(strategy, samples, repairs, edit),
                "ignored": ignored,
            }),
            door,
        })
    }

    /// The requests granted, `None` when the door named no bound.
    #[must_use]
    pub const fn max_calls(&self) -> Option<u32> {
        self.max
    }

    /// One counter under this authority's bound (uncapped when it names none).
    #[must_use]
    pub fn envelope(&self) -> Arc<Envelope> {
        Arc::new(match self.max {
            Some(max) => Envelope::new(max, self.door.remedy),
            None => Envelope::uncapped(self.door.remedy),
        })
    }

    /// The receipt's account: the bound and its source, the invocations sent and refused, the
    /// wire requests sent and refused (one shape: `sent` and `refused` null with the reason in
    /// `unknown` for a seat that makes its own), what was configured.
    #[must_use]
    pub fn record(&self, invocations: &Envelope, requests: Option<&Envelope>) -> Value {
        json!({
            "max_calls": self.max,
            "source": if self.explicit { self.door.grant } else { "default: no request bound" },
            "invocations": invocations.account(),
            "http_requests": requests.map_or_else(
                || json!({"sent": null, "refused": null, "unknown": "an ACP harness makes its own requests"}),
                |wire| {
                    let mut account = wire.account();
                    account["unknown"] = Value::Null;
                    account
                },
            ),
            "configured": self.configured,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMEDY: &str = "authorize more with --authoring-max-calls";

    #[test]
    fn the_worst_case_follows_the_resolved_strategy() {
        // Every request a configuration can send, the verifier's included (nv1b): an edit's
        // native candidate and its repairs, then its whole-request judgment and locate question
        // (3 + r); the sketch, its fills and r repairs, each new candidate judged afresh
        // (4 + 3r: a repair from the room or the judge reopens the graph or its fills); COLD's
        // samples and their evidence repairs (2s), then 1 + r verification attempts of at most 8
        // clause questions, the whole-request judgment and one synthesis of at most 2 transforms
        // each, with its r verify repairs and r transform repairs (2s + 14r + 12); escalate adds
        // the sketch door with one repair less (1 + 3r, none at r = 0). A creation under `only`
        // sends nothing. Before the verifier was counted, the default work was bounded at 2 + 1 + 3.
        assert_eq!(
            historical_worst_case(NativeMode::Escalate, 1, 3, false),
            56 + 10
        );
        assert_eq!(historical_worst_case(NativeMode::Escalate, 1, 0, false), 14);
        assert_eq!(
            historical_worst_case(NativeMode::Escalate, 1, 1, false),
            28 + 4
        );
        assert_eq!(historical_worst_case(NativeMode::Only, 1, 0, false), 0);
        assert_eq!(historical_worst_case(NativeMode::Only, 1, 3, false), 0);
        assert_eq!(historical_worst_case(NativeMode::Sketch, 1, 0, false), 4);
        assert_eq!(historical_worst_case(NativeMode::Sketch, 1, 3, false), 13);
        assert_eq!(historical_worst_case(NativeMode::Off, 1, 0, false), 14);
        assert_eq!(historical_worst_case(NativeMode::Off, 3, 5, false), 88);
        assert_eq!(historical_worst_case(NativeMode::Escalate, 1, 3, true), 6);
        assert_eq!(historical_worst_case(NativeMode::Only, 1, 3, true), 6);
        assert_eq!(historical_worst_case(NativeMode::Off, 1, 3, true), 0);
        // Samples clamped as the policy clamps them (five); repairs counted as typed (nine):
        // COLD 2·5 + 10·12 + 2·9, then the sketch door with eight repairs, 8 + 2 + 2·9.
        assert_eq!(
            historical_worst_case(NativeMode::Escalate, 9, 9, false),
            148 + 28
        );
        // An explicit source recovery: the source and its whole-request judgment per round,
        // none without the policy, every typed round counted.
        assert_eq!(super::recovery_requests(0), 0);
        assert_eq!(super::recovery_requests(1), 1 + 2);
        assert_eq!(super::recovery_requests(9), 9 * (1 + 2));
    }

    /// Totals are complete only when every call's usage is known (E10 P3-e): a local refusal
    /// used none; a timeout, an unreported usage or a provider failure leaves them partial.
    #[test]
    fn usage_totals_are_complete_only_when_every_call_is_known() {
        let reported = json!({"result": {"usage_reported": true,
            "input_tokens": 100, "output_tokens": 50}});
        let failed = |kind: &str| json!({"result": {"failure_kind": kind}});
        let refused = failed("admission_refused");
        assert!(usage_complete(&[reported.clone(), refused.clone()]));
        assert!(usage_complete(&[refused]));
        let unreported = json!({"result": {"usage_reported": false,
            "input_tokens": null, "output_tokens": null}});
        for partial in [failed("timeout"), failed("provider_error"), unreported] {
            assert!(
                !usage_complete(&[reported.clone(), partial.clone()]),
                "{partial}"
            );
        }
    }

    const DOOR: Door = Door::new("--authoring-max-calls", REMEDY);

    fn resolve(max: Option<u32>, strategy: NativeMode, typed: Typed) -> Result<Authority, Refusal> {
        Authority::resolve(max, strategy, typed, DOOR)
    }

    #[test]
    fn a_typed_multiplicity_is_refused_only_when_a_typed_bound_cannot_honor_it() {
        use NativeMode::{Escalate, Only, Sketch};
        let nothing = Typed::new(false);
        // No grant: no request bound, and the core's own default (no repair count) runs under
        // it, with no finite worst case.
        let default = resolve(None, Escalate, nothing).expect("no bound");
        assert_eq!(default.max_calls(), None);
        assert_eq!(default.configured["repairs"], Value::Null);
        assert_eq!(default.configured["worst_case"], Value::Null);
        // COLD's computation count is request-dependent even with typed repairs: the receipt
        // cannot promise the historical capped estimate. Its explicit authority remains exact.
        let repairs = nothing.with_repairs(Some(3));
        let honored = resolve(None, Escalate, repairs).expect("no bound to exceed");
        assert_eq!(honored.configured["worst_case"], Value::Null);
        let bounded = resolve(Some(1), Escalate, repairs).expect("dynamic work under one request");
        assert_eq!(bounded.max_calls(), Some(1));
        assert_eq!(bounded.configured["repairs"], 3);
        assert_eq!(bounded.configured["worst_case"], Value::Null);
        // The sketch door's judgment depends on the request (each part of a doubted request
        // asked alone): no finite count refuses typed repairs up front either, and the counters
        // refuse each request past the bound.
        let sketched = resolve(Some(1), Sketch, repairs).expect("dynamic judgment under one");
        assert_eq!(sketched.configured["worst_case"], Value::Null);
        // Nothing extra typed: never refused for what the defaults would allow (a typed only
        // strategy needs its judgment, below).
        let none = nothing.with_strategy().with_repairs(Some(0));
        assert!(resolve(Some(2), Only, none).is_ok());
        assert!(resolve(Some(1), Escalate, nothing.with_repairs(Some(0))).is_ok());
        // Samples do not make the computation count known. Their calls still share the
        // selected envelope; no preflight estimate silently raises that envelope.
        let samples = nothing.with_samples(Some(3));
        let account = resolve(Some(1), Escalate, samples).expect("one request remains the bound");
        assert_eq!(account.max_calls(), Some(1));
        assert_eq!(account.configured["samples"], 3);
        assert_eq!(account.configured["worst_case"], Value::Null);
        assert!(resolve(Some(18), Escalate, samples).is_ok());
        assert!(resolve(None, Escalate, samples).is_ok());
        // A typed strategy is honored in full or refused by a typed bound: a judged READY takes
        // two requests at least, three for the sketch. A typed `only` creates nothing, so no
        // grant is named to it: the core refuses the creation with its migration.
        let only = resolve(Some(1), Only, nothing.with_strategy()).expect("no grant to buy");
        assert_eq!(only.configured["worst_case"], 0);
        assert_eq!(least_requests(Only), 1);
        for (strategy, steps, least) in [
            (Escalate, "the plan, then its judgment", 2),
            (Sketch, "the sketch, its fills, then their judgment", 3),
        ] {
            let named = nothing.with_strategy();
            assert_eq!(least_requests(strategy), least);
            let refused = resolve(Some(least - 1), strategy, named).expect_err("too few");
            assert_eq!(refused, Refusal::Strategy { strategy, steps });
            assert!(resolve(Some(least), strategy, named).is_ok());
            assert!(
                resolve(None, strategy, named).is_ok(),
                "no bound: {strategy:?}"
            );
            assert!(resolve(None, strategy, nothing).is_ok());
        }
        // An edit revises natively: a typed escalate needs one request there.
        assert!(resolve(Some(1), Escalate, Typed::new(true).with_strategy()).is_ok());
    }

    #[test]
    fn the_account_states_its_source_and_an_opaque_seat_requests_as_unknown() {
        let granted = resolve(Some(2), NativeMode::Escalate, Typed::new(false)).expect("two");
        let invocations = granted.envelope();
        let record = granted.record(&invocations, None);
        // One shape either way: counts, or nulls with the reason they are unknown.
        let unknown = "an ACP harness makes its own requests";
        assert_eq!(
            record["http_requests"],
            json!({"sent": null, "refused": null, "unknown": unknown})
        );
        assert_eq!(record["source"], "--authoring-max-calls");
        assert_eq!(record["max_calls"], 2);
        assert_eq!(record["invocations"], json!({"sent": 0, "refused": 0}));
        let default = resolve(None, NativeMode::Escalate, Typed::new(false)).expect("no bound");
        let wire = default.envelope();
        let record = default.record(&default.envelope(), Some(&wire));
        assert_eq!(record["source"], "default: no request bound");
        assert_eq!(record["max_calls"], Value::Null);
        assert_eq!(
            record["http_requests"],
            json!({"sent": 0, "refused": 0, "unknown": null})
        );
        assert_eq!(
            record["configured"],
            json!({"strategy": "escalate", "samples": 1, "repairs": null, "worst_case": null, "ignored": []})
        );
    }

    #[test]
    fn a_typed_count_the_core_would_run_as_another_is_refused_never_clamped() {
        let nothing = Typed::new(false);
        let range = |name, typed, least, most| Refusal::Range {
            name,
            typed,
            least,
            most,
        };
        let escalate = |max, typed| resolve(max, NativeMode::Escalate, typed);
        let nine = nothing.with_samples(Some(9));
        let accepted = escalate(Some(9), nine).expect("nine samples under nine requests");
        assert_eq!(accepted.configured["samples"], 9);
        assert_eq!(accepted.max_calls(), Some(9));
        let zero_samples = nothing.with_samples(Some(0));
        assert_eq!(
            escalate(Some(9), zero_samples).err(),
            Some(range("samples", 0, 1, Some(u32::MAX)))
        );
        // Repairs are honored as typed, any count: no request-independent count refuses them,
        // and the counters bound what they spend.
        let six = nothing.with_repairs(Some(6));
        assert!(resolve(Some(9), NativeMode::Sketch, six).is_ok());
        assert!(escalate(None, nothing.with_repairs(Some(1000))).is_ok());
        // A grant of no request is refused before any request, never raised to one.
        assert_eq!(
            escalate(Some(0), nothing).err(),
            Some(range("max_calls", 0, 1, None))
        );
        // Both counts stay as typed. Zero samples is refused by the authority/core, not changed
        // silently into one; the policy's default repair count remains absent.
        let policy = || crate::AuthoringPolicy::new("mock/m", 1, std::time::Duration::from_secs(1));
        assert_eq!(policy().repairs, None);
        assert_eq!(
            policy().with_repairs(4).with_unbounded_repairs().repairs,
            None
        );
        for count in [0, 1, 5, 6, 7, 64] {
            let policy = policy().with_samples(count).with_repairs(count);
            assert_eq!(policy.samples, count, "{count}");
            assert_eq!(SAMPLES.contains(&count), count > 0, "{count}");
            assert!(REPAIRS.contains(&count), "{count}");
            assert_eq!(policy.repair_limit(), Some(count), "{count}");
        }
    }

    #[test]
    fn current_counts_are_unknown_for_dynamic_work_and_unbounded_repairs() {
        use NativeMode::{Escalate, Off, Only, Sketch};
        for strategy in [Escalate, Off, Only, Sketch] {
            for edit in [false, true] {
                let no_calls = matches!((strategy, edit), (Off, true) | (Only, false));
                let typed = no_calls.then_some(0);
                assert_eq!(worst_case_of(strategy, 5, Some(64), edit), typed);
                assert_eq!(
                    worst_case_of(strategy, 5, None, edit),
                    no_calls.then_some(0)
                );
            }
        }
        // The old API's historical estimate is preserved, never used as today's bound.
        assert_eq!(historical_worst_case(Escalate, 1, 3, false), 66);
        assert_eq!(historical_worst_case(Off, 1, 3, false), 56);
        assert_eq!(
            historical_worst_case(Escalate, 1, u32::MAX, false),
            u32::MAX
        );
        assert_eq!(worst_case_of(Escalate, 1, Some(0), false), None);
        assert_eq!(worst_case_of(Off, 1, Some(0), false), None);
        // The sketch door's judgment asks as many questions as a doubted request has parts.
        for repairs in [0, 1, 3, u32::MAX] {
            assert_eq!(worst_case_of(Sketch, 1, Some(repairs), false), None);
        }
        // The historical 3 + r revision estimate omits semantic links and repeated judgments.
        // Even zero repairs cannot make the retained representation known to this function.
        for strategy in [Escalate, Only, Sketch] {
            for repairs in [0, 1, 64, u32::MAX] {
                assert_eq!(worst_case_of(strategy, 1, Some(repairs), true), None);
            }
        }
        let authority = resolve(
            Some(1),
            Sketch,
            Typed::new(true).with_repairs(Some(u32::MAX)),
        )
        .expect("an unrepresentable estimate does not change the explicit grant");
        assert_eq!(authority.max_calls(), Some(1));
        assert!(authority.configured["worst_case"].is_null());
        assert_eq!(recovery_requests(2), 6);
        assert_eq!(recovery_requests(4), 12, "recovery rounds count as typed");
    }

    #[test]
    fn a_typed_value_the_strategy_cannot_apply_is_ignored_never_refused() {
        use NativeMode::{Escalate, Off, Only, Sketch};
        let nothing = Typed::new(false);
        let ignored = |authority: &Authority| authority.configured["ignored"].clone();
        // Repairs under off are the verifier's, never ignored just because their total request
        // count depends on the computations. The explicit request envelope remains unchanged.
        let typed = nothing.with_strategy().with_repairs(Some(3));
        let off = resolve(Some(1), Off, typed).expect("off with its verifier repairs");
        assert_eq!(ignored(&off), json!([]));
        assert_eq!(off.max_calls(), Some(1));
        assert_eq!(off.configured["worst_case"], Value::Null);
        // Samples change nothing where no plan is sampled.
        for strategy in [Only, Sketch] {
            let sampled = resolve(None, strategy, nothing.with_samples(Some(3))).expect("runs");
            assert_eq!(ignored(&sampled), json!(["samples"]), "{strategy:?}");
        }
        // An edit revises natively: neither a strategy's kind nor samples apply there.
        let edit = Typed::new(true).with_strategy().with_samples(Some(3));
        let revision = resolve(None, Sketch, edit).expect("one request");
        assert_eq!(ignored(&revision), json!(["strategy", "samples"]));
        // A value the strategy applies is never ignored when its request count is unknown.
        let sampled = resolve(Some(1), Off, nothing.with_samples(Some(2))).expect("one request");
        assert_eq!(ignored(&sampled), json!([]));
        assert_eq!(sampled.max_calls(), Some(1));
        let repaired = resolve(Some(66), Escalate, nothing.with_repairs(Some(3))).expect("66");
        assert_eq!(ignored(&repaired), json!([]));
    }
}
