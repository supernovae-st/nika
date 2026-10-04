// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authority over a seat's requests. A repair count is not an authority (pack 94): the door
//! that seats a model also states how many requests it may be sent, and these counters hold
//! that number. [`Seat`] counts the invocations of any seat; [`Wire`] counts the physical
//! requests of a direct API seat where its bytes leave, so a transport retry or a
//! structured-output fallback inside one invocation is a request too. A seat whose own requests
//! cannot be observed (an agent harness) is bounded by its invocations alone. A request past the
//! authority is refused before any byte leaves, and every refusal is counted beside what was
//! sent, never over the core's own journal of attempts. [`worst_case`] is what a configuration
//! of this core can ask for, and [`Authority`] resolves a door's bound against what its caller
//! typed, before any request. The counters bound requests, never dollars.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use nika_kernel::ai::provider::{InferRequest, InferResponse, ProviderError, ProviderInferDyn};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use serde_json::{Value, json};

use crate::NativeMode;

/// The worst-case authoring requests a configuration can make, the verifier's included (R4 A11,
/// nv1b), so the review a caller signs bounds every request a compile sends. With `s` samples and
/// `r` repairs clamped to [`SAMPLES`] and [`REPAIRS`]:
/// - an edit's native revision: the candidate and its repairs, then the whole-request judgment
///   and its locate question (`verify::WHOLE_QUESTIONS`, 2): `3 + r`;
/// - the sketch door: the sketch, its fills and `r` repairs (a fill, or the graph reproposed),
///   each new candidate judged afresh: `2 + r + 2 (1 + r) = 4 + 3r`;
/// - COLD: each sample and its one evidence repair (`2s`), then `1 + r` verification attempts,
///   each asking at most `verify::CLAUSE_QUESTIONS` (8) clause questions, the whole-request
///   judgment (2) and one transform synthesis of at most `transform::MAX_CALLS` (2) questions;
///   each of the `r` verify repairs is one call, and the transform repairs share one allowance of
///   `r`: `2s + 12 (1 + r) + 2r = 2s + 14r + 12`;
/// - escalate: COLD, then the sketch door with one repair less (`3 + r`; none when `r` is 0);
/// - only: no request for a creation, which this core no longer authors as whole source.
#[must_use]
pub fn worst_case(strategy: NativeMode, samples: u32, repairs: u32, edit: bool) -> u32 {
    let samples = samples.clamp(*SAMPLES.start(), *SAMPLES.end());
    let repairs = repairs.clamp(*REPAIRS.start(), *REPAIRS.end());
    let count = |questions: usize| u32::try_from(questions).unwrap_or(u32::MAX);
    let judged = count(crate::cognition::WHOLE_QUESTIONS);
    let native = 1 + repairs + judged;
    let sketch = |repairs: u32| repairs + 2 + judged * (1 + repairs);
    let attempt = count(crate::cognition::CLAUSE_QUESTIONS)
        + judged
        + count(crate::cognition::TRANSFORM_QUESTIONS);
    let cold = 2 * samples + (1 + repairs) * attempt + 2 * repairs;
    match strategy {
        NativeMode::Off if edit => 0,
        NativeMode::Off => cold,
        _ if edit => native,
        NativeMode::Only => 0,
        NativeMode::Sketch => sketch(repairs),
        _ => cold + repairs.checked_sub(1).map_or(0, sketch),
    }
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

/// The requests an authority grants when its door names none: exactly one.
pub const DEFAULT_MAX_CALLS: u32 = 1;

/// The COLD samples the core runs; a typed count outside is refused, never clamped into another.
pub const SAMPLES: std::ops::RangeInclusive<u32> = 1..=5;

/// The native repairs the core runs; a typed count outside is refused, never clamped.
pub const REPAIRS: std::ops::RangeInclusive<u32> = 0..=5;

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

/// The authority one compile runs under, resolved before any request: its finite bound, whether
/// the caller named it, and what the configuration could ask for.
#[derive(Debug)]
pub struct Authority {
    max: u32,
    explicit: bool,
    configured: Value,
    door: Door,
}

impl Authority {
    /// The authority `max_calls` grants ([`DEFAULT_MAX_CALLS`] when absent) under the resolved
    /// strategy, or the refusal of a typed multiplicity it cannot honor (never a silent
    /// reduction). Defaults run within the authority, and the receipt states where they stopped.
    /// A typed value the strategy cannot apply (repairs under `off`, samples where no plan is
    /// sampled, a strategy's kind in an edit, which revises natively) changes no request: it is
    /// recorded as ignored, never refused.
    ///
    /// # Errors
    /// [`Refusal::Range`] for typed samples or repairs outside [`SAMPLES`] or [`REPAIRS`], or a
    /// grant of zero requests; [`Refusal::Multiplicity`] when typed repairs or samples raise the
    /// worst case above the defaults' and above the bound; [`Refusal::Strategy`] for a typed
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
        let max = max_calls.unwrap_or(DEFAULT_MAX_CALLS);
        let baseline = worst_case(strategy, 1, 0, edit);
        let needed = worst_case(strategy, samples.unwrap_or(1), repairs.unwrap_or(0), edit);
        if needed > baseline && needed > max {
            return Err(Refusal::Multiplicity {
                needed,
                authorized: max,
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
        if let Some(steps) = steps.filter(|_| max < least_requests(strategy)) {
            return Err(Refusal::Strategy { strategy, steps });
        }
        let ignored: Vec<&str> = [
            (
                "strategy",
                typed.strategy && edit && strategy != NativeMode::Off,
            ),
            (
                "samples",
                samples.is_some() && worst_case(strategy, 2, 0, edit) == baseline,
            ),
            (
                "repairs",
                repairs.is_some() && worst_case(strategy, 1, 1, edit) == baseline,
            ),
        ]
        .into_iter()
        .filter_map(|(value, ignored)| ignored.then_some(value))
        .collect();
        let (samples, repairs) = (samples.unwrap_or(1), repairs.unwrap_or(3));
        Ok(Self {
            max,
            explicit: max_calls.is_some(),
            configured: json!({
                "strategy": strategy.word(),
                "samples": samples,
                "repairs": repairs,
                "worst_case": worst_case(strategy, samples, repairs, edit),
                "ignored": ignored,
            }),
            door,
        })
    }

    /// The requests granted.
    #[must_use]
    pub const fn max_calls(&self) -> u32 {
        self.max
    }

    /// One counter under this authority's bound.
    #[must_use]
    pub fn envelope(&self) -> Arc<Envelope> {
        Arc::new(Envelope::new(self.max, self.door.remedy))
    }

    /// The receipt's account: the bound and its source, the invocations sent and refused, the
    /// wire requests sent and refused (one shape: `sent` and `refused` null with the reason in
    /// `unknown` for a seat that makes its own), what was configured.
    #[must_use]
    pub fn record(&self, invocations: &Envelope, requests: Option<&Envelope>) -> Value {
        json!({
            "max_calls": self.max,
            "source": if self.explicit { self.door.grant } else { "default: one request" },
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

/// One counter of an authority: at most `max` sent, every attempt past it refused and counted.
#[derive(Debug)]
pub struct Envelope {
    max: u32,
    remedy: &'static str,
    sent: AtomicU32,
    refused: AtomicU32,
}

impl Envelope {
    /// A counter of at most `max` requests; `remedy` tells the refused caller how to authorize
    /// more at its own door.
    #[must_use]
    pub const fn new(max: u32, remedy: &'static str) -> Self {
        Self {
            max,
            remedy,
            sent: AtomicU32::new(0),
            refused: AtomicU32::new(0),
        }
    }

    /// Takes one send, or records one refusal: atomic, so concurrent attempts never pass `max`.
    fn admit(&self) -> bool {
        let taken = self
            .sent
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |sent| {
                (sent < self.max).then_some(sent + 1)
            })
            .is_ok();
        if !taken {
            self.refused.fetch_add(1, Ordering::SeqCst);
        }
        taken
    }

    fn refusal(&self) -> String {
        format!(
            "the authoring authority is spent ({} of {} sent): this request was refused before any byte left; {}",
            self.sent.load(Ordering::SeqCst),
            self.max,
            self.remedy
        )
    }

    /// What was sent and refused, as a receipt states it.
    #[must_use]
    pub fn account(&self) -> Value {
        json!({
            "sent": self.sent.load(Ordering::SeqCst),
            "refused": self.refused.load(Ordering::SeqCst),
        })
    }
}

/// A direct API seat's transport: one wire attempt per POST, no redirect followed (a followed
/// redirect is another request carrying the same prompt, uncounted), and no POST past the
/// authority.
pub struct Wire<H> {
    inner: H,
    requests: Arc<Envelope>,
}

impl<H> Wire<H> {
    /// The transport `inner`, its POSTs counted by `requests`.
    #[must_use]
    pub const fn new(inner: H, requests: Arc<Envelope>) -> Self {
        Self { inner, requests }
    }
}

impl<H: HttpPostDyn + Send + Sync> HttpPostDyn for Wire<H> {
    fn supports_single_attempt(&self) -> bool {
        self.inner.supports_single_attempt()
    }

    async fn post(&self, mut request: HttpRequest) -> Result<HttpResponse, HttpError> {
        if !self.requests.admit() {
            return Err(HttpError::Other {
                reason: self.requests.refusal(),
            });
        }
        request.follow_redirects = false;
        self.inner.post(request).await
    }

    async fn send_streaming(
        &self,
        mut request: HttpRequest,
    ) -> Result<HttpStreamResponse, HttpError> {
        if !self.requests.admit() {
            return Err(HttpError::Other {
                reason: self.requests.refusal(),
            });
        }
        request.follow_redirects = false;
        self.inner.send_streaming(request).await
    }
}

/// A seat under the authority: its invocations counted and refused past it, and the model
/// identities its responses report, kept apart from the model the operator requested; a
/// response that reports no nonblank identity is counted, its identity unknown.
pub struct Seat<P> {
    inner: P,
    invocations: Arc<Envelope>,
    observed: Mutex<Vec<String>>,
    unreported: AtomicU32,
}

impl<P> Seat<P> {
    /// The seat `inner`, its invocations counted by `invocations`.
    #[must_use]
    pub const fn new(inner: P, invocations: Arc<Envelope>) -> Self {
        Self {
            inner,
            invocations,
            observed: Mutex::new(Vec::new()),
            unreported: AtomicU32::new(0),
        }
    }

    /// The responses whose model identity is unknown (absent or blank), never assumed to be the
    /// model requested.
    #[must_use]
    pub fn unreported(&self) -> u32 {
        self.unreported.load(Ordering::SeqCst)
    }

    /// The seat itself.
    #[must_use]
    pub const fn inner(&self) -> &P {
        &self.inner
    }

    /// The model identities the responses reported, each once, in order.
    #[must_use]
    pub fn observed(&self) -> Vec<String> {
        self.observed
            .lock()
            .map(|models| models.clone())
            .unwrap_or_default()
    }
}

impl<P: ProviderInferDyn + Send + Sync> ProviderInferDyn for Seat<P> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        // A local refusal before any request: typed as one, so the core knows nothing was used.
        if !self.invocations.admit() {
            return Err(ProviderError::AdmissionDenied {
                reason: self.invocations.refusal(),
            });
        }
        let response = self.inner.infer(request).await?;
        match (
            response.gen_ai.response_model.as_ref(),
            self.observed.lock(),
        ) {
            (Some(model), Ok(mut observed)) if !model.trim().is_empty() => {
                if !observed.contains(model) {
                    observed.push(model.clone());
                }
            }
            _ => {
                self.unreported.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::ai::provider::{ContentBlock, StopReason, TokenUsage};

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
        assert_eq!(worst_case(NativeMode::Escalate, 1, 3, false), 56 + 10);
        assert_eq!(worst_case(NativeMode::Escalate, 1, 0, false), 14);
        assert_eq!(worst_case(NativeMode::Escalate, 1, 1, false), 28 + 4);
        assert_eq!(worst_case(NativeMode::Only, 1, 0, false), 0);
        assert_eq!(worst_case(NativeMode::Only, 1, 3, false), 0);
        assert_eq!(worst_case(NativeMode::Sketch, 1, 0, false), 4);
        assert_eq!(worst_case(NativeMode::Sketch, 1, 3, false), 13);
        assert_eq!(worst_case(NativeMode::Off, 1, 0, false), 14);
        assert_eq!(worst_case(NativeMode::Off, 3, 5, false), 88);
        assert_eq!(worst_case(NativeMode::Escalate, 1, 3, true), 6);
        assert_eq!(worst_case(NativeMode::Only, 1, 3, true), 6);
        assert_eq!(worst_case(NativeMode::Off, 1, 3, true), 0);
        // Clamped as the policy clamps: five samples, five repairs.
        assert_eq!(worst_case(NativeMode::Escalate, 9, 9, false), 92 + 16);
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
    fn a_typed_multiplicity_is_refused_only_when_the_authority_cannot_honor_it() {
        use NativeMode::{Escalate, Only, Sketch};
        let nothing = Typed::new(false);
        // No grant: one request, and the defaults run within it.
        let default = resolve(None, Escalate, nothing).expect("one request");
        assert_eq!(default.max_calls(), 1);
        assert_eq!(default.configured["worst_case"], 66);
        // Typed repairs under escalate can need sixty-six: refused, naming what they need.
        let repairs = nothing.with_repairs(Some(3));
        let refused = resolve(None, Escalate, repairs).expect_err("sixty-six");
        let needed = Refusal::Multiplicity {
            needed: 66,
            authorized: 1,
            strategy: Escalate,
        };
        assert_eq!(refused, needed);
        assert!(resolve(Some(66), Escalate, repairs).is_ok());
        // Nothing extra typed: never refused for what the defaults would allow (a typed only
        // strategy needs its judgment, below).
        let none = nothing.with_strategy().with_repairs(Some(0));
        assert!(resolve(Some(2), Only, none).is_ok());
        assert!(resolve(None, Escalate, nothing.with_repairs(Some(0))).is_ok());
        // Samples: eighteen under escalate for three of them (no repair typed: the sketch door
        // after the plan has none left, so it sends nothing).
        let samples = nothing.with_samples(Some(3));
        let refused = resolve(None, Escalate, samples).expect_err("eighteen");
        assert!(matches!(refused, Refusal::Multiplicity { needed: 18, .. }));
        assert!(resolve(Some(18), Escalate, samples).is_ok());
        // A typed strategy is honored in full or refused: a judged READY takes two requests at
        // least, three for the sketch; the default runs in one. A typed `only` creates nothing,
        // so no grant is named to it: the core refuses the creation with its migration.
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
            assert!(resolve(None, strategy, nothing).is_ok());
        }
        // An edit revises natively: a typed escalate needs one request there.
        assert!(resolve(None, Escalate, Typed::new(true).with_strategy()).is_ok());
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
        assert_eq!(record["invocations"], json!({"sent": 0, "refused": 0}));
        let default = resolve(None, NativeMode::Escalate, Typed::new(false)).expect("one");
        let wire = default.envelope();
        let record = default.record(&default.envelope(), Some(&wire));
        assert_eq!(record["source"], "default: one request");
        assert_eq!(
            record["http_requests"],
            json!({"sent": 0, "refused": 0, "unknown": null})
        );
        assert_eq!(
            record["configured"],
            json!({"strategy": "escalate", "samples": 1, "repairs": 3, "worst_case": 66, "ignored": []})
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
        assert_eq!(
            escalate(Some(9), nine).err(),
            Some(range("samples", 9, 1, Some(5)))
        );
        let zero_samples = nothing.with_samples(Some(0));
        assert_eq!(
            escalate(Some(9), zero_samples).err(),
            Some(range("samples", 0, 1, Some(5)))
        );
        let six = nothing.with_repairs(Some(6));
        assert_eq!(
            escalate(Some(9), six).err(),
            Some(range("repairs", 6, 0, Some(5)))
        );
        // A grant of no request is refused before any request, never raised to one.
        assert_eq!(
            escalate(Some(0), nothing).err(),
            Some(range("max_calls", 0, 1, None))
        );
        // The core's policy clamps to these same ranges: a count inside them runs as typed.
        for count in 0..=7 {
            let policy =
                crate::AuthoringPolicy::new("mock/m", 1, std::time::Duration::from_secs(1))
                    .with_samples(count)
                    .with_repairs(count);
            assert_eq!(SAMPLES.contains(&count), policy.samples == count, "{count}");
            assert_eq!(REPAIRS.contains(&count), policy.repairs == count, "{count}");
        }
    }

    #[test]
    fn a_typed_value_the_strategy_cannot_apply_is_ignored_never_refused() {
        use NativeMode::{Escalate, Off, Only, Sketch};
        let nothing = Typed::new(false);
        let ignored = |authority: &Authority| authority.configured["ignored"].clone();
        // Repairs under off are the verifier's (R4 A11, nv1b): its verify repairs, their
        // syntheses and the transform repair allowance are counted, so typed repairs are honored
        // in full or refused, never ignored.
        let typed = nothing.with_strategy().with_repairs(Some(3));
        let refused = resolve(None, Off, typed).expect_err("fifty-six");
        assert!(matches!(refused, Refusal::Multiplicity { needed: 56, .. }));
        let off = resolve(Some(56), Off, typed).expect("off with its verifier repairs");
        assert_eq!(ignored(&off), json!([]));
        assert_eq!(off.configured["worst_case"], 56);
        // Samples change nothing where no plan is sampled.
        for strategy in [Only, Sketch] {
            let sampled = resolve(None, strategy, nothing.with_samples(Some(3))).expect("runs");
            assert_eq!(ignored(&sampled), json!(["samples"]), "{strategy:?}");
        }
        // An edit revises natively: neither a strategy's kind nor samples apply there.
        let edit = Typed::new(true).with_strategy().with_samples(Some(3));
        let revision = resolve(None, Sketch, edit).expect("one request");
        assert_eq!(ignored(&revision), json!(["strategy", "samples"]));
        // A value the strategy applies is never ignored, and is honored in full or refused.
        assert!(resolve(None, Off, nothing.with_samples(Some(2))).is_err());
        let repaired = resolve(Some(66), Escalate, nothing.with_repairs(Some(3))).expect("66");
        assert_eq!(ignored(&repaired), json!([]));
    }

    /// A transport that counts what reached it.
    struct Counting(AtomicU32);

    impl HttpPostDyn for Counting {
        fn supports_single_attempt(&self) -> bool {
            true
        }
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse::new(
                200,
                std::collections::BTreeMap::default(),
                Vec::new().into(),
                request.url,
            ))
        }
        async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(HttpError::Other {
                reason: "no stream here".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn the_wire_refuses_past_the_authority_before_the_transport_sees_it() {
        let requests = Arc::new(Envelope::new(2, REMEDY));
        let wire = Wire::new(Counting(AtomicU32::new(0)), Arc::clone(&requests));
        assert!(
            wire.post(HttpRequest::post("https://a.test/v1"))
                .await
                .is_ok()
        );
        assert!(
            wire.send_streaming(HttpRequest::post("https://a.test/v1"))
                .await
                .is_err()
        );
        let refused = wire.post(HttpRequest::post("https://a.test/v1")).await;
        let expected = "the authoring authority is spent (2 of 2 sent): this request was refused \
                        before any byte left; authorize more with --authoring-max-calls";
        assert!(matches!(refused, Err(HttpError::Other { ref reason }) if reason == expected));
        assert_eq!(
            wire.inner.0.load(Ordering::SeqCst),
            2,
            "the third never left"
        );
        assert_eq!(requests.account(), json!({"sent": 2, "refused": 1}));
        assert!(wire.supports_single_attempt());
    }

    /// A transport that keeps whether each request it received would follow a redirect.
    struct Recording(Mutex<Vec<bool>>);

    impl HttpPostDyn for Recording {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.0.lock().expect("log").push(request.follow_redirects);
            Ok(HttpResponse::new(
                307,
                std::collections::BTreeMap::default(),
                Vec::new().into(),
                request.url,
            ))
        }
        async fn send_streaming(
            &self,
            request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
            self.0.lock().expect("log").push(request.follow_redirects);
            Err(HttpError::Other {
                reason: "no stream here".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn the_wire_never_follows_a_redirect() {
        let requests = Arc::new(Envelope::new(3, REMEDY));
        let wire = Wire::new(Recording(Mutex::new(Vec::new())), Arc::clone(&requests));
        // A request asks to follow redirects by default; what reaches the transport never does.
        let request = HttpRequest::post("https://a.test/v1");
        assert!(request.follow_redirects);
        let answered = wire.post(request).await.expect("the 307 is the answer");
        assert_eq!(answered.status, 307);
        let _ = wire
            .send_streaming(HttpRequest::post("https://a.test/v1"))
            .await;
        assert_eq!(*wire.inner.0.lock().expect("log"), [false, false]);
        assert_eq!(requests.account(), json!({"sent": 2, "refused": 0}));
    }

    struct ReportedModel(Option<&'static str>);

    impl ProviderInferDyn for ReportedModel {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            let mut response =
                InferResponse::new(Vec::new(), TokenUsage::new(1, 1), StopReason::EndTurn);
            response.gen_ai.response_model = self.0.map(str::to_owned);
            Ok(response)
        }
    }

    #[tokio::test]
    async fn a_blank_reported_model_is_unknown_not_an_observed_identity() {
        for reported in [None, Some(""), Some(" "), Some("\t\n")] {
            let seat = Seat::new(ReportedModel(reported), Arc::new(Envelope::new(1, REMEDY)));
            let response = seat
                .infer(InferRequest::new("requested-model", Vec::new()))
                .await
                .expect("provider answer");
            assert_eq!(
                response.gen_ai.response_model.as_deref(),
                reported,
                "the provider response remains unchanged"
            );
            assert!(
                seat.observed().is_empty(),
                "blank is not model evidence: {reported:?}"
            );
            assert_eq!(seat.unreported(), 1);
        }
        let seat = Seat::new(
            ReportedModel(Some("actually-served")),
            Arc::new(Envelope::new(2, REMEDY)),
        );
        for _ in 0..2 {
            seat.infer(InferRequest::new("requested-model", Vec::new()))
                .await
                .expect("provider answer");
        }
        assert_eq!(seat.observed(), ["actually-served"]);
        assert_eq!(seat.unreported(), 0);
    }

    /// A seat that answers with one reported identity and counts its invocations.
    struct Answering(AtomicU32);

    impl ProviderInferDyn for Answering {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let mut response = InferResponse::new(
                vec![ContentBlock::Text { text: "{}".into() }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            );
            response.gen_ai.response_model = Some("served-model".to_owned());
            Ok(response)
        }
    }

    #[tokio::test]
    async fn a_seat_past_the_authority_is_refused_and_its_observed_identity_kept() {
        let invocations = Arc::new(Envelope::new(2, REMEDY));
        let seat = Seat::new(Answering(AtomicU32::new(0)), Arc::clone(&invocations));
        let request = || InferRequest::new("vllm/requested", Vec::new());
        // Five concurrent invocations under a ceiling of two: two reach the seat, three refused.
        let joined = tokio::join!(
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
        );
        let results = [joined.0, joined.1, joined.2, joined.3, joined.4];
        let answered = results.iter().filter(|result| result.is_ok()).count();
        assert_eq!((answered, seat.inner().0.load(Ordering::SeqCst)), (2, 2));
        // Each refusal is a local admission refusal, never a provider's answer.
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .all(|error| matches!(error, ProviderError::AdmissionDenied { .. }))
        );
        assert_eq!(invocations.account(), json!({"sent": 2, "refused": 3}));
        assert_eq!(seat.observed(), ["served-model"]);
        assert_eq!(seat.unreported(), 0);
    }

    /// A seat whose responses report no model identity.
    struct Unnamed;

    impl ProviderInferDyn for Unnamed {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            Ok(InferResponse::new(
                vec![ContentBlock::Text { text: "{}".into() }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            ))
        }
    }

    #[tokio::test]
    async fn a_response_without_a_model_is_counted_unknown_never_the_requested_one() {
        let seat = Seat::new(Unnamed, Arc::new(Envelope::new(2, REMEDY)));
        let request = || InferRequest::new("vllm/requested", Vec::new());
        assert!(seat.infer(request()).await.is_ok());
        assert!(seat.infer(request()).await.is_ok());
        assert!(seat.observed().is_empty());
        assert_eq!(seat.unreported(), 2);
    }
}
