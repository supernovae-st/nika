// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit, finite, invocation-bound unknown-cost admission. No permit or
//! execution grant is created here. Observations cannot restore authority.
use super::{AdmissionState, InferenceAdmission, ProviderError, denied};
use nika_catalog::admission::InferenceTariff;
use nika_kernel::ai::provider::{InferResponse, TokenUsage, UsageCompleteness};
use nika_types::cost::Cost;
use serde::Serialize;
use std::time::Duration;

/// A hard constraint must be observed absent before unknown spend is allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HardMonetaryCap {
    /// Explicitly observed absent; not an omitted or unreadable policy.
    Absent,
    /// A policy, machine or occurrence ceiling, including zero.
    Capped(Cost),
    /// Missing or unreadable policy; fails closed.
    Unknown,
}

/// Policy evidence supplied by the owning host. Invocation/project defaults
/// are overridable; hard caps and missing policy knowledge never are.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct UnknownCostPolicy {
    allowed: bool,
    hard: [HardMonetaryCap; 3],
    defaults: [Option<Cost>; 2],
}
impl UnknownCostPolicy {
    /// Preserve each source separately. No model answer can supply this evidence.
    #[must_use]
    pub fn new(
        allowed: bool,
        policy: HardMonetaryCap,
        machine: HardMonetaryCap,
        occurrence: HardMonetaryCap,
        invocation_default: Option<Cost>,
        project_default: Option<Cost>,
    ) -> Self {
        Self {
            allowed,
            hard: [policy, machine, occurrence],
            defaults: [invocation_default, project_default],
        }
    }
    pub(super) fn validate(&self) -> Result<(), ProviderError> {
        if !self.allowed || self.hard.iter().any(|c| *c != HardMonetaryCap::Absent) {
            return Err(denied(
                "unknown cost is unavailable: policy must permit it and policy/machine/occurrence caps must be explicitly absent",
            ));
        }
        if self.defaults.iter().flatten().any(|c| c.nano_usd < 0) {
            return Err(denied("negative invocation/project monetary default"));
        }
        Ok(())
    }
    /// [`Self::validate`] for one choice. A fresh review's choice is judged
    /// exactly as before. A choice admitted under a standing rule overrides
    /// nothing: every ceiling it is under, the invocation and project defaults
    /// included, must be absent or cover its declared worst case
    /// ([`Self::standing_refusal`]).
    pub(super) fn validate_for(&self, choice: &UnknownCostChoice) -> Result<(), ProviderError> {
        if choice.standing.is_none() {
            return self.validate();
        }
        self.standing_refusal(choice)
            .map_or(Ok(()), |why| Err(denied(why)))
    }
    /// Why a standing choice is not admitted under this evidence, in words a
    /// host shows; `None` when it is. The host must permit unknown cost and
    /// have observed every hard-cap layer. Then each ceiling the Run is under
    /// (a hard cap, an invocation or project default) either is absent or
    /// covers the choice's declared worst case: a rule that declares no price
    /// fits under no ceiling, and no ceiling is ever overridden.
    pub(super) fn standing_refusal(&self, choice: &UnknownCostChoice) -> Option<String> {
        if !self.allowed || self.hard.contains(&HardMonetaryCap::Unknown) {
            return Some(
                "unknown cost is unavailable: the host must permit it and have observed every policy, machine and occurrence cap".into(),
            );
        }
        let hard = self.hard.iter().filter_map(|cap| match cap {
            HardMonetaryCap::Capped(limit) => Some(*limit),
            _ => None,
        });
        let lowest = hard
            .chain(self.defaults.iter().flatten().copied())
            .min_by_key(|c| c.nano_usd)?;
        if lowest.nano_usd < 0 {
            return Some("negative monetary ceiling".into());
        }
        match choice.declared_worst_case() {
            Some(worst) if worst.nano_usd <= lowest.nano_usd => None,
            Some(worst) => Some(format!(
                "the declared worst case of this Run ({worst}: every request at its size and output bounds, at the declared rates) exceeds the {lowest} ceiling it runs under; nothing was sent"
            )),
            None => Some(format!(
                "a {lowest} ceiling binds this Run and its standing rule declares no price: an unknown price fits under no ceiling; nothing was sent"
            )),
        }
    }
}

/// Deliberate operator choice, not deserializable execution authority. All
/// identity fields and bounds are private and immutable after construction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct UnknownCostChoice {
    candidate: String,
    invocation: String,
    provider: String,
    model: String,
    endpoint: String,
    max_requests: u32,
    /// Requests in flight at once when widened past one; absent is the
    /// historical one-at-a-time choice, serialized byte for byte as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_in_flight: Option<u32>,
    /// The Run authored retries: only then does a received 429/503 answer its
    /// attempt; absent (false) keeps the historical bytes and Uncertain law.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    authored_retry: bool,
    max_output_tokens: u32,
    timeout_ms: u64,
    declared_tariff: Option<super::DeclaredTariff>,
    /// The largest request body, in bytes, when a standing rule bounds it
    /// below the 1 MiB of every admitted request; absent keeps the historical
    /// bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_request_bytes: Option<u32>,
    /// The operator's standing rule this choice was admitted under; absent,
    /// the choice is a fresh review's and keeps the historical bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    standing: Option<StandingGrant>,
}

/// What a choice records of the standing rule that admitted it: the rule's
/// identity, the Runs it admits per window and, when the rule declares a USD
/// price, the most this choice can cost under that declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct StandingGrant {
    policy: String,
    max_runs: u32,
    window_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    declared_worst_case_nano_usd: Option<NanoText>,
}

/// A nano-USD amount, written as the observation writes every amount: a
/// decimal string that keeps the whole `i128` range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NanoText(Cost);
impl Serialize for NanoText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0.nano_usd)
    }
}

impl UnknownCostChoice {
    /// Bind an explicit choice to one candidate, invocation and full request
    /// route. Every request has finite output/time bounds; no transport retry,
    /// and one request in flight unless [`Self::with_max_in_flight`] widens it.
    /// The endpoint must be canonical ([`crate::canonical_endpoint`]): its bytes
    /// are exactly what the transport dials and are never shortened or rewritten.
    /// # Errors
    /// Empty identity, a noncanonical endpoint (credentials, query, fragment, a
    /// form the URL parser would rewrite, an `@`), zero or unrepresentable bounds.
    #[allow(
        clippy::too_many_arguments,
        reason = "the immutable choice requires independent identity and finite bound axes"
    )]
    pub fn new(
        candidate: String,
        invocation: String,
        provider: String,
        model: String,
        endpoint: String,
        max_requests: u32,
        max_output_tokens: u32,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let timeout_ms =
            u64::try_from(timeout.as_millis()).map_err(|_| denied("timeout overflow"))?;
        if [&candidate, &invocation, &provider, &model]
            .iter()
            .any(|s| s.trim().is_empty())
            || !crate::canonical_endpoint(&endpoint)
            || endpoint.contains('@')
            || max_requests == 0
            || max_output_tokens == 0
            || timeout_ms == 0
        {
            return Err(denied(
                "unknown-cost choice requires exact identity and finite positive request/output/timeout bounds",
            ));
        }
        Ok(Self {
            candidate,
            invocation,
            provider,
            model,
            endpoint,
            max_requests,
            max_in_flight: None,
            authored_retry: false,
            max_output_tokens,
            timeout_ms,
            declared_tariff: None,
            max_request_bytes: None,
            standing: None,
        })
    }
    /// Mark this choice as admitted under the operator's standing `rule`,
    /// which must cover every bound it carries: the rule's request-size bound
    /// and, when it declares one, its USD price are attached, and the choice
    /// then records the rule's identity and the most it can cost under that
    /// declaration. A standing choice overrides no ceiling
    /// ([`UnknownCostPolicy`] judges it so).
    /// # Errors
    /// A bound of this choice above the rule's, or a declared price the
    /// tariff refuses.
    pub(super) fn under_standing(
        mut self,
        rule: &super::StandingRule,
    ) -> Result<Self, ProviderError> {
        if self.max_requests > rule.max_requests
            || self.max_output_tokens > rule.max_output_tokens
            || self.timeout() > rule.timeout
        {
            return Err(denied(
                "the choice exceeds the request, output or time bound of its standing rule",
            ));
        }
        self.max_request_bytes = rule.max_request_bytes;
        if let Some([input, output]) = rule.price {
            // No cache discount is declared: cached input costs the input rate.
            let tariff = super::DeclaredTariff::new(
                &self,
                self.provider.clone(),
                "USD".into(),
                super::TariffUnit::PerMillionTokens,
                [input, output, input],
                super::standing::DECLARED_PROVENANCE.into(),
                rule.id().into(),
            )?;
            self.declared_tariff = Some(tariff);
        }
        let worst = match &self.declared_tariff {
            Some(declared) => Some(
                self.worst_case(declared)
                    .ok_or_else(|| denied("the declared worst case is not representable"))?,
            ),
            None => None,
        };
        self.standing = Some(StandingGrant {
            policy: rule.id().into(),
            max_runs: rule.max_runs(),
            window_seconds: rule.window().as_secs(),
            declared_worst_case_nano_usd: worst.map(NanoText),
        });
        Ok(self)
    }
    /// Every request at the request-size and output bounds, at `declared`'s
    /// rates: a request's bytes bound its input tokens (each token spans at
    /// least one byte), so no admitted Run of this choice costs more under that
    /// declaration. `None` unless the declaration is in USD and the sum fits.
    fn worst_case(&self, declared: &super::DeclaredTariff) -> Option<Cost> {
        if declared.currency() != "USD" {
            return None;
        }
        declared
            .price_native(self.request_bytes(), u64::from(self.max_output_tokens), 0)?
            .checked_mul(i128::from(self.max_requests))
            .map(Cost::new)
    }
    /// The identity of the standing rule this choice was admitted under;
    /// `None` for a fresh review's choice.
    #[must_use]
    pub fn standing_policy(&self) -> Option<&str> {
        self.standing.as_ref().map(|grant| grant.policy.as_str())
    }
    /// The most this choice can cost under its standing rule's declared USD
    /// price; `None` when no rule admitted it or the rule declares no price.
    #[must_use]
    pub fn declared_worst_case(&self) -> Option<Cost> {
        let grant = self.standing.as_ref()?;
        grant.declared_worst_case_nano_usd.map(|nano| nano.0)
    }
    /// The Run authored retries inside the confirmed total: a 429 or 503
    /// received from the reserved endpoint answers its attempt (usage and USD
    /// cost unknown) instead of leaving the account Uncertain.
    #[must_use]
    pub fn with_authored_retry(mut self) -> Self {
        self.authored_retry = true;
        self
    }
    /// At most `n` of this choice's requests in flight at once, inside the same
    /// confirmed total; each reservation takes one of both atomically.
    /// # Errors
    /// Zero, or more than the confirmed total.
    pub fn with_max_in_flight(mut self, n: u32) -> Result<Self, ProviderError> {
        if n == 0 || n > self.max_requests {
            return Err(denied(
                "unknown-cost concurrency must be positive and within the confirmed total",
            ));
        }
        self.max_in_flight = (n > 1).then_some(n);
        Ok(self)
    }
    /// Exact selected adapter namespace.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    /// Exact selected wire model.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    /// Full selected request endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    /// The selected endpoint's origin ([`crate::route_origin`]): what a durable
    /// record names in its place. `None` when it has none to project.
    #[must_use]
    pub fn origin(&self) -> Option<String> {
        crate::route_origin(&self.endpoint)
    }
    /// Attach an explicitly declared estimate. This does not turn unknown-cost
    /// authorization into a dollar guarantee or permit a hard-cap override.
    /// # Errors
    /// Declaration names a different provider/model/endpoint.
    pub fn with_declared_tariff(
        mut self,
        tariff: super::DeclaredTariff,
    ) -> Result<Self, ProviderError> {
        if !tariff.matches(&self) {
            return Err(denied("declared tariff scope differs from choice"));
        }
        self.declared_tariff = Some(tariff);
        Ok(self)
    }
    pub(super) fn matches(&self, provider: &str, model: &str, endpoint: &str) -> bool {
        self.provider == provider && self.model == model && self.endpoint == endpoint
    }
    /// The request-size bound a declared worst case rests on, in bytes: the
    /// standing rule's own, or the 1 MiB of every admitted request.
    fn request_bytes(&self) -> u64 {
        self.max_request_bytes.map_or_else(
            || u64::try_from(super::BOUNDED_REQUEST_BYTES).unwrap_or(u64::MAX),
            u64::from,
        )
    }
    /// Whether a reply reporting `input_tokens` contradicts the bound this
    /// choice's declared worst case rests on (a request's bytes bound its
    /// tokens). A choice with no declared worst case rests on no such bound.
    fn exceeds_declared_input(&self, input_tokens: u64) -> bool {
        self.declared_worst_case().is_some() && input_tokens > self.request_bytes()
    }
    pub(super) fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

/// A physical attempt's evidence. Unknown cost remains `None`, even when a
/// native EUR tariff is known. The account's USD subtotal excludes that call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct UnknownAttemptReceipt {
    /// Account-local sequence number.
    pub id: usize,
    /// Exact consent and route identity, retained for review/recovery.
    pub choice: UnknownCostChoice,
    /// Source/currency/native rates for the exact route, without currency conversion.
    pub pricing: serde_json::Value,
    /// Whether transport was entered.
    pub sent: bool,
    /// The returned usage; absence never means zero usage.
    pub usage: Option<TokenUsage>,
    /// Known USD estimate, otherwise null. Never the provider invoice.
    pub estimated: Option<Cost>,
    /// Observed native-currency estimate, kept separate from the USD subtotal.
    pub native_estimated_nano: Option<i128>,
    /// Currency of the native estimate; absent if no tariff was declared/observed.
    pub currency: Option<String>,
    /// Exact observed provider response model.
    pub response_model: Option<String>,
    /// Provider request id when returned.
    pub request_id: Option<String>,
    /// Outcome including possibly billed failures.
    pub note: String,
}

impl InferenceAdmission {
    /// New explicit unknown-cost account. Numeric `new(Cost)` remains strict.
    /// The returned handle is unbound until `for_scope` verifies the host's
    /// actual candidate and invocation; the choice alone never starts a Run.
    /// # Errors
    /// Policy denies, is unknown, or carries any hard monetary ceiling.
    pub fn new_unknown(
        choice: UnknownCostChoice,
        policy: UnknownCostPolicy,
    ) -> Result<Self, ProviderError> {
        policy.validate_for(&choice)?;
        let mut account = Self::new(Cost::zero())?;
        {
            let mut s = account.lock()?;
            // A standing choice overrides no default: each one covers it.
            if choice.standing.is_none() {
                s.overridden_defaults = policy.defaults;
            }
            s.unknown = Some(choice);
        }
        account.1.bound = false;
        Ok(account)
    }
    /// Bind a handle to the candidate and invocation independently known by
    /// the caller. Clones share counters; binding cannot replenish allowances.
    /// # Errors
    /// Any identity mismatch, or unavailable account.
    pub fn for_scope(&self, candidate: &str, invocation: &str) -> Result<Self, ProviderError> {
        let s = self.lock()?;
        if s.unknown
            .as_ref()
            .is_some_and(|c| c.candidate != candidate || c.invocation != invocation)
        {
            return Err(denied(
                "unknown-cost choice belongs to a different candidate or invocation",
            ));
        }
        Ok(Self(
            self.0.clone(),
            super::scope::HandleScope {
                bound: true,
                ..self.1
            },
        ))
    }
    pub(crate) fn check_route(
        &self,
        provider: &str,
        model: &str,
        endpoint: &str,
    ) -> Result<(), ProviderError> {
        if !self.tracks_route(provider, model, endpoint) {
            return Err(denied(
                "route is outside this declared-free observation account",
            ));
        }
        let s = self.lock()?;
        if let Some(c) = &s.unknown {
            if !self.1.bound
                || !c.matches(provider, model, endpoint)
                || s.status != AdmissionState::Open
            {
                return Err(denied(
                    "unknown-cost choice is unbound, closed, or names a different route/model",
                ));
            }
            Ok(())
        } else {
            Self::qualify(provider, model, endpoint)?;
            if s.tariff_below_charge(provider, model, endpoint) {
                return Err(denied(super::TARIFF_BELOW_CHARGE));
            }
            Ok(())
        }
    }
    pub(crate) fn declared_tariff(&self) -> Result<Option<super::DeclaredTariff>, ProviderError> {
        Ok(self
            .lock()?
            .unknown
            .as_ref()
            .and_then(|c| c.declared_tariff.clone()))
    }

    pub(crate) fn request_timeout(&self) -> Result<Option<Duration>, ProviderError> {
        Ok(self
            .lock()?
            .unknown
            .as_ref()
            .map(UnknownCostChoice::timeout))
    }
    /// Refuse, before any reservation or byte, a request whose body is larger
    /// than the request-size bound of this account's standing rule. An account
    /// with no such bound admits every size the bounded transport does.
    pub(crate) fn check_request_bytes(&self, bytes: usize) -> Result<(), ProviderError> {
        let mut s = self.lock()?;
        let Some(bound) = s.unknown.as_ref().and_then(|c| c.max_request_bytes) else {
            return Ok(());
        };
        if u64::try_from(bytes).is_ok_and(|sent| sent <= u64::from(bound)) {
            return Ok(());
        }
        Err(s.refuse(&format!(
            "this {bytes}-byte request exceeds the {bound} bytes its standing cost rule admits per request; nothing was sent"
        )))
    }
    pub(super) fn reserve_unknown(
        &self,
        provider: &str,
        model: &str,
        endpoint: &str,
        output: u32,
    ) -> Result<Option<UnknownAttempt>, ProviderError> {
        self.check_route(provider, model, endpoint)?;
        let mut s = self.lock()?;
        let Some(choice) = s.unknown.clone() else {
            return Ok(None);
        };
        if output == 0
            || output > choice.max_output_tokens
            || s.unknown_in_flight >= choice.max_in_flight.unwrap_or(1)
            || s.unknown_attempts.len() >= choice.max_requests as usize
        {
            return Err(s.refuse("unknown-cost request/output/concurrency bound exhausted"));
        }
        let id = s.unknown_attempts.len();
        s.unknown_in_flight += 1;
        let pricing = choice.declared_tariff.as_ref().map_or_else(
            || {
                crate::retry::BillingRoute::new(provider.into(), model.into(), endpoint.into())
                    .map_or(serde_json::Value::Null, |r| r.observation())
            },
            super::DeclaredTariff::observation,
        );
        s.unknown_attempts.push(UnknownAttemptReceipt {
            id,
            choice: choice.clone(),
            pricing,
            sent: false,
            usage: None,
            estimated: None,
            native_estimated_nano: None,
            currency: None,
            response_model: None,
            request_id: None,
            note: "reserved; USD cost unknown".into(),
        });
        Ok(Some(UnknownAttempt {
            account: self.clone(),
            id,
            choice,
            output,
            sent: false,
            done: false,
        }))
    }
}

pub(crate) struct UnknownAttempt {
    account: InferenceAdmission,
    id: usize,
    choice: UnknownCostChoice,
    output: u32,
    sent: bool,
    done: bool,
}
impl UnknownAttempt {
    pub(super) fn sent(&mut self) -> Result<(), ProviderError> {
        let mut s = self.account.lock()?;
        if self.sent || self.done || s.status != AdmissionState::Open {
            return Err(s.refuse("unknown-cost attempt revoked or already sent"));
        }
        self.sent = true;
        s.unknown_attempts[self.id].sent = true;
        Ok(())
    }
    /// The provider's own account charge, reported with complete usage at its own
    /// seeded endpoint: it outranks every catalog or declared estimate.
    fn reported_charge(&self, r: &InferResponse, identity_ok: bool) -> Option<Cost> {
        r.cost.filter(|_| {
            identity_ok
                && r.usage_completeness == UsageCompleteness::Complete
                && r.usage.output_tokens <= u64::from(self.output)
                && crate::retry::official(
                    &self.choice.provider,
                    &self.choice.model,
                    &self.choice.endpoint,
                )
        })
    }
    /// Provenance of a settled reported charge on this choice's exact route.
    fn reported_pricing(&self) -> serde_json::Value {
        crate::retry::BillingRoute::new(
            self.choice.provider.clone(),
            self.choice.model.clone(),
            self.choice.endpoint.clone(),
        )
        .map_or(serde_json::Value::Null, |route| {
            crate::wire::admission::reported_charge_observation(&route)
        })
    }
    pub(super) fn settle(&mut self, r: &InferResponse) -> Result<(), ProviderError> {
        if !self.sent || self.done {
            return Err(denied("attempt must be sent exactly once"));
        }
        let tariff = InferenceTariff::new(
            &self.choice.provider,
            &self.choice.model,
            &self.choice.endpoint,
        );
        // With an exact tariff, a model it documents as served for the choice's model answers
        // too, as on every priced path; without one, only the choice's own model does.
        let identity_ok =
            r.gen_ai.response_model.as_deref().is_some_and(|model| {
                tariff.map_or(model == self.choice.model, |t| t.serves(model))
            });
        // A reply that reports more input tokens than the request-size bound contradicts what
        // a standing rule's declared worst case rests on: nothing is priced on it.
        let within_input = !self.choice.exceeds_declared_input(r.usage.input_tokens);
        let identity_ok = identity_ok && within_input;
        // A tariff prices a cache write only at the write rate it states.
        let priced = tariff
            .filter(|t| {
                identity_ok
                    && r.usage_completeness == UsageCompleteness::Complete
                    && r.usage.input_tokens <= t.context_tokens
                    && r.usage.output_tokens <= u64::from(self.output)
            })
            .zip(crate::retry::cache_writes(&r.usage));
        let split = (
            r.usage.input_tokens,
            r.usage.output_tokens,
            r.usage.cache_read_tokens.unwrap_or(0),
        );
        let mut estimate =
            priced.and_then(|(t, written)| t.price_written(split.0, split.1, split.2, written));
        let mut native_estimate = priced
            .and_then(|(t, written)| t.price_native_written(split.0, split.1, split.2, written));
        let mut currency = tariff.map(|t| t.currency.to_owned());
        let reported = self.reported_charge(r, identity_ok);
        if let Some(declared) = &self.choice.declared_tariff {
            native_estimate = (identity_ok
                && r.usage_completeness == UsageCompleteness::Complete
                && r.usage.output_tokens <= u64::from(self.output)
                && r.usage.cache_write_tokens.unwrap_or(0) == 0
                && r.usage.cache_creation_tokens.unwrap_or(0) == 0)
                .then(|| {
                    declared.price_native(
                        r.usage.input_tokens,
                        r.usage.output_tokens,
                        r.usage.cache_read_tokens.unwrap_or(0),
                    )
                })
                .flatten();
            currency = Some(declared.currency().into());
            estimate = native_estimate
                .filter(|_| declared.currency() == "USD")
                .map(Cost::new);
        }
        if let Some(charge) = reported {
            estimate = Some(charge);
            native_estimate = Some(charge.nano_usd);
            currency = Some("USD".into());
        }
        let pricing = reported.map(|_| self.reported_pricing());
        let mut s = self.account.lock()?;
        let a = &mut s.unknown_attempts[self.id];
        a.usage = Some(r.usage.clone());
        a.response_model.clone_from(&r.gen_ai.response_model);
        a.request_id.clone_from(&r.request_id);
        a.estimated = estimate;
        a.native_estimated_nano = native_estimate;
        a.currency = currency;
        if let Some(pricing) = pricing {
            a.pricing = pricing;
        }
        a.note = if estimate.is_some() {
            "complete usage priced; invoice unknown"
        } else {
            "completed; USD cost unknown"
        }
        .into();
        if !identity_ok || r.usage.output_tokens > u64::from(self.output) {
            s.status = AdmissionState::Uncertain;
            return Err(s.refuse(if within_input {
                "response model/output contradicts explicit unknown-cost choice"
            } else {
                "the reply reports more input tokens than the request-size bound of its standing rule: the rule's declared worst case no longer holds"
            }));
        }
        if let Some(cost) = estimate {
            s.estimated = super::add(s.estimated, cost)?;
        }
        s.unknown_in_flight = s.unknown_in_flight.saturating_sub(1);
        self.done = true;
        Ok(())
    }
    /// A 429 or 503 received from the reserved endpoint of a choice whose Run
    /// authored retries: the attempt is answered, its usage and USD cost stay
    /// unknown (never « not billed »). The account keeps its state, so only an
    /// authored retry inside the confirmed total may dispatch again, and only
    /// while it is Open. Any other status, endpoint or choice is left to
    /// `Drop`: Uncertain, no further dispatch.
    pub(super) fn answered(&mut self, status: u16, final_url: &str) {
        if !self.sent || self.done || !self.choice.authored_retry || !matches!(status, 429 | 503) {
            return;
        }
        if final_url != self.choice.endpoint {
            return;
        }
        let Ok(mut s) = self.account.lock() else {
            return;
        };
        s.unknown_attempts[self.id].note =
            format!("answered HTTP {status}; usage and USD cost unknown");
        s.unknown_in_flight = s.unknown_in_flight.saturating_sub(1);
        self.done = true;
    }
}
impl Drop for UnknownAttempt {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if let Ok(mut s) = self.account.lock() {
            s.unknown_in_flight = s.unknown_in_flight.saturating_sub(1);
            if self.sent {
                s.status = AdmissionState::Uncertain;
                s.unknown_attempts[self.id].note = "possibly billed; no automatic retry".into();
                s.refusal = Some(
                    "sent request ended without usable settlement; unknown charge retained".into(),
                );
            } else {
                s.unknown_attempts[self.id].note = "not dispatched".into();
            }
        }
    }
}
