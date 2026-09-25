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
    max_output_tokens: u32,
    timeout_ms: u64,
    declared_tariff: Option<super::DeclaredTariff>,
}
impl UnknownCostChoice {
    /// Bind an explicit choice to one candidate, invocation and full request
    /// route. Every request has finite output/time bounds; retry is always zero
    /// and only one request may be in flight. Endpoint bytes are never shortened.
    /// # Errors
    /// Empty identity, credentials/query/fragment, zero or unrepresentable bounds.
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
            || !endpoint.starts_with("https://")
            || endpoint.contains(['?', '#', '@'])
            || endpoint.chars().any(char::is_whitespace)
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
            max_output_tokens,
            timeout_ms,
            declared_tariff: None,
        })
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
        policy.validate()?;
        let mut account = Self::new(Cost::zero())?;
        {
            let mut s = account.lock()?;
            s.unknown = Some(choice);
            s.overridden_defaults = policy.defaults;
        }
        account.1 = false;
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
        Ok(Self(self.0.clone(), true))
    }
    pub(crate) fn check_route(
        &self,
        provider: &str,
        model: &str,
        endpoint: &str,
    ) -> Result<(), ProviderError> {
        let s = self.lock()?;
        if let Some(c) = &s.unknown {
            if !self.1 || !c.matches(provider, model, endpoint) || s.status != AdmissionState::Open
            {
                return Err(denied(
                    "unknown-cost choice is unbound, closed, or names a different route/model",
                ));
            }
            Ok(())
        } else {
            Self::qualify(provider, model, endpoint).map(|_| ())
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
            || s.unknown_active
            || s.unknown_attempts.len() >= choice.max_requests as usize
        {
            return Err(s.refuse("unknown-cost request/output/concurrency bound exhausted"));
        }
        let id = s.unknown_attempts.len();
        s.unknown_active = true;
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
    pub(super) fn settle(&mut self, r: &InferResponse) -> Result<(), ProviderError> {
        if !self.sent || self.done {
            return Err(denied("attempt must be sent exactly once"));
        }
        let tariff = InferenceTariff::new(
            &self.choice.provider,
            &self.choice.model,
            &self.choice.endpoint,
        );
        let identity_ok = r.gen_ai.response_model.as_deref() == Some(self.choice.model.as_str());
        let mut estimate = tariff
            .filter(|t| {
                identity_ok
                    && r.usage_completeness == UsageCompleteness::Complete
                    && r.usage.input_tokens <= t.context_tokens
                    && r.usage.output_tokens <= u64::from(self.output)
            })
            .and_then(|t| {
                t.price(
                    r.usage.input_tokens,
                    r.usage.output_tokens,
                    r.usage.cache_read_tokens.unwrap_or(0),
                )
            });
        let mut native_estimate = tariff
            .filter(|t| {
                identity_ok
                    && r.usage_completeness == UsageCompleteness::Complete
                    && r.usage.input_tokens <= t.context_tokens
                    && r.usage.output_tokens <= u64::from(self.output)
            })
            .and_then(|t| {
                t.price_native(
                    r.usage.input_tokens,
                    r.usage.output_tokens,
                    r.usage.cache_read_tokens.unwrap_or(0),
                )
            });
        let mut currency = tariff.map(|t| t.currency.to_owned());
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
        let mut s = self.account.lock()?;
        let a = &mut s.unknown_attempts[self.id];
        a.usage = Some(r.usage.clone());
        a.response_model.clone_from(&r.gen_ai.response_model);
        a.request_id.clone_from(&r.request_id);
        a.estimated = estimate;
        a.native_estimated_nano = native_estimate;
        a.currency = currency;
        a.note = if estimate.is_some() {
            "complete usage priced; invoice unknown"
        } else {
            "completed; USD cost unknown"
        }
        .into();
        if !identity_ok || r.usage.output_tokens > u64::from(self.output) {
            s.status = AdmissionState::Uncertain;
            return Err(s.refuse("response model/output contradicts explicit unknown-cost choice"));
        }
        if let Some(cost) = estimate {
            s.estimated = super::add(s.estimated, cost)?;
        }
        s.unknown_active = false;
        self.done = true;
        Ok(())
    }
}
impl Drop for UnknownAttempt {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if let Ok(mut s) = self.account.lock() {
            s.unknown_active = false;
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
