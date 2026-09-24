// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Shared catalog admission; a reservation is not an invoice or Run consent.
use nika_catalog::admission::InferenceTariff;
use nika_kernel::ai::provider::{InferResponse, ProviderError, TokenUsage, UsageCompleteness};
use nika_types::cost::Cost;
use std::sync::{Arc, Mutex, MutexGuard};

mod declared;
mod unknown;
pub use declared::{DeclaredTariff, TariffUnit};
pub use unknown::{HardMonetaryCap, UnknownAttemptReceipt, UnknownCostChoice, UnknownCostPolicy};

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "admission/unknown_tests.rs"]
mod unknown_tests;

/// Whether another bounded inference may be admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdmissionState {
    /// Reservations can be requested against remaining allowance.
    Open,
    /// Consent/constraint changed; this account no longer admits calls.
    Closed,
    /// An attempt may have charged without complete usable evidence.
    Uncertain,
}
/// One physical provider request's accounting, including unsuccessful ones.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AttemptReceipt {
    /// Stable sequence number within the account.
    pub id: usize,
    /// Exact selected wire model.
    pub model: String,
    /// Exact endpoint, not a provider nickname.
    pub endpoint: String,
    /// Pricing observation, separate from the models.dev snapshot.
    pub tariff: InferenceTariff,
    /// Reserved worst-case catalog cost.
    pub reserved: Cost,
    /// Whether the request crossed the transport boundary.
    pub sent: bool,
    /// Complete or partial observed meters; absent stays absent.
    pub usage: Option<TokenUsage>,
    /// Catalog price of complete reported usage, never a billed amount.
    pub estimated: Option<Cost>,
    /// Provider response identity, never inferred from the request.
    pub response_model: Option<String>,
    /// Provider request id, when returned.
    pub request_id: Option<String>,
    /// Catalog price of observed tokens, including an over-bound observation.
    /// Only `estimated` attests accepted complete settlement.
    pub reported_estimate: Option<Cost>,
    /// Outcome, including unknown charge and pre-dispatch rejection.
    pub note: String,
}
/// A snapshot of actual local decisions. No field grants execution authority.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InferenceReceipt {
    /// Explicit unknown-cost scope, absent for strict numeric admission.
    pub unknown_cost: Option<UnknownCostChoice>,
    /// Sent calls excluded from the known USD subtotal, never priced as zero.
    pub unknown_calls: usize,
    /// Per-request unknown-cost evidence, retained independently of known costs.
    pub unknown_attempts: Vec<UnknownAttemptReceipt>,
    /// Invocation/project defaults explicitly superseded by this choice.
    pub overridden_defaults: [Option<Cost>; 2],
    /// Total authorized catalog allowance for this work, not a per-call gift.
    pub limit: Cost,
    /// Settled known USD subtotal; each receipt names catalog or declared provenance.
    pub estimated: Cost,
    /// Reservations for requests still in flight.
    pub active: Cost,
    /// Unreleased reservations whose charge is unknown.
    pub held_unknown: Cost,
    /// Unallocated allowance (zero when already over the new limit).
    pub available: Cost,
    /// No authoritative invoice is observed by this first slice.
    pub billed: Option<Cost>,
    /// Whether calls are open, revoked or uncertain.
    pub state: AdmissionState,
    /// Most recent refusal/uncertainty; no invented independent cap.
    pub refusal: Option<String>,
    /// Per-attempt provenance; includes failed/unknown attempts.
    pub attempts: Vec<AttemptReceipt>,
}
#[derive(Debug)]
struct State {
    unknown: Option<UnknownCostChoice>,
    unknown_active: bool,
    unknown_attempts: Vec<UnknownAttemptReceipt>,
    overridden_defaults: [Option<Cost>; 2],
    limit: Cost,
    estimated: Cost,
    active: Cost,
    held: Cost,
    status: AdmissionState,
    refusal: Option<String>,
    attempts: Vec<AttemptReceipt>,
}
/// Clones share the same atomic allowance across factories, repairs and revisions.
#[derive(Clone, Debug)]
pub struct InferenceAdmission(Arc<Mutex<State>>, bool);

pub(crate) fn denied(reason: impl Into<String>) -> ProviderError {
    ProviderError::AdmissionDenied {
        reason: reason.into(),
    }
}
fn add(a: Cost, b: Cost) -> Result<Cost, ProviderError> {
    a.nano_usd
        .checked_add(b.nano_usd)
        .map(Cost::new)
        .ok_or_else(|| denied("admission arithmetic overflow"))
}
impl State {
    fn committed(&self) -> Result<Cost, ProviderError> {
        add(add(self.estimated, self.active)?, self.held)
    }
    fn refuse(&mut self, reason: &str) -> ProviderError {
        self.refusal = Some(reason.to_owned());
        denied(reason)
    }
}
impl InferenceAdmission {
    /// Start one allowance. Zero stays zero; signed credits are not authority.
    /// # Errors
    /// Negative amounts are refused.
    pub fn new(limit: Cost) -> Result<Self, ProviderError> {
        if limit.nano_usd < 0 {
            return Err(denied("negative admission allowance"));
        }
        Ok(Self(
            Arc::new(Mutex::new(State {
                unknown: None,
                unknown_active: false,
                unknown_attempts: Vec::new(),
                overridden_defaults: [None, None],
                limit,
                estimated: Cost::zero(),
                active: Cost::zero(),
                held: Cost::zero(),
                status: AdmissionState::Open,
                refusal: None,
                attempts: Vec::new(),
            })),
            true,
        ))
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>, ProviderError> {
        self.0
            .lock()
            .map_err(|_| denied("admission account is unavailable"))
    }
    /// Change the TOTAL allowance; settled and outstanding charges survive.
    /// # Errors
    /// Invalid, uncertain, revoked or already committed allowances refuse.
    pub fn amend(&self, limit: Cost) -> Result<(), ProviderError> {
        if limit.nano_usd < 0 {
            self.close("negative allowance")?;
            return Err(denied("negative allowance"));
        }
        let mut s = self.lock()?;
        if s.unknown.is_some() {
            return Err(s.refuse(
                "unknown-cost scope cannot be amended or converted into numeric authority",
            ));
        }
        if s.status == AdmissionState::Uncertain {
            return Err(s.refuse("account is charge-unknown; no new allowance inferred"));
        }
        s.limit = limit;
        s.status = AdmissionState::Open;
        if s.committed()?.nano_usd > limit.nano_usd {
            s.status = AdmissionState::Closed;
            return Err(s.refuse("new allowance is below already committed catalog exposure"));
        }
        s.refusal = None;
        Ok(())
    }
    /// Revoke future admission. This cannot undo already dispatched requests.
    /// # Errors
    /// An unavailable lock fails closed.
    pub fn close(&self, reason: &str) -> Result<(), ProviderError> {
        let mut s = self.lock()?;
        if s.status != AdmissionState::Uncertain {
            s.status = AdmissionState::Closed;
        }
        s.refusal = Some(reason.to_owned());
        Ok(())
    }
    /// Read observations without refreshing allowance or granting consent.
    /// # Errors
    /// An unavailable lock fails closed.
    pub fn snapshot(&self) -> Result<InferenceReceipt, ProviderError> {
        let s = self.lock()?;
        Ok(InferenceReceipt {
            unknown_cost: s.unknown.clone(),
            unknown_calls: s
                .unknown_attempts
                .iter()
                .filter(|a| a.sent && a.estimated.is_none())
                .count()
                + s.attempts
                    .iter()
                    .filter(|a| a.sent && a.estimated.is_none())
                    .count(),
            unknown_attempts: s.unknown_attempts.clone(),
            overridden_defaults: s.overridden_defaults,
            limit: s.limit,
            estimated: s.estimated,
            active: s.active,
            held_unknown: s.held,
            available: Cost::new(
                s.limit
                    .nano_usd
                    .saturating_sub(s.committed()?.nano_usd)
                    .max(0),
            ),
            billed: None,
            state: s.status,
            refusal: s.refusal.clone(),
            attempts: s.attempts.clone(),
        })
    }
    /// Qualify the ACTUAL selected endpoint and exact model; never a gateway
    /// alias or an unpriced local/subscription lane. No network is performed.
    /// # Errors
    /// Unknown binding refuses before any call.
    pub fn qualify(
        provider: &str,
        model: &str,
        endpoint: &str,
    ) -> Result<InferenceTariff, ProviderError> {
        InferenceTariff::new(provider, model, endpoint)
            .filter(|t| t.currency == "USD")
            .ok_or_else(|| denied("selected endpoint/model has no qualified catalog admission tariff; billed cost is unknown"))
    }

    pub(crate) fn refuse(&self, reason: &str) -> ProviderError {
        match self.lock() {
            Ok(mut s) => s.refuse(reason),
            Err(e) => e,
        }
    }
    pub(crate) fn reserve(
        &self,
        provider: &str,
        model: &str,
        endpoint: &str,
        output: u32,
    ) -> Result<Attempt, ProviderError> {
        if let Some(attempt) = self.reserve_unknown(provider, model, endpoint, output)? {
            return Ok(Attempt::Unknown(attempt));
        }
        let tariff =
            Self::qualify(provider, model, endpoint).map_err(|e| self.refuse(&e.to_string()))?;
        let quote = tariff
            .reserve(output)
            .ok_or_else(|| self.refuse("missing or unsupported maximum output/thinking bound"))?;
        let mut s = self.lock()?;
        if s.status != AdmissionState::Open {
            return Err(s.refuse("account closed or charge unknown"));
        }
        let Ok(total) = s.committed().and_then(|committed| add(committed, quote)) else {
            return Err(s.refuse("admission arithmetic overflow"));
        };
        if s.limit.nano_usd == 0 || total.nano_usd > s.limit.nano_usd {
            return Err(
                s.refuse("remaining catalog allowance cannot cover the full-context reservation")
            );
        }
        s.active = add(s.active, quote)?;
        let id = s.attempts.len();
        s.attempts.push(AttemptReceipt {
            id,
            model: model.to_owned(),
            endpoint: endpoint.to_owned(),
            tariff,
            reserved: quote,
            sent: false,
            usage: None,
            estimated: None,
            response_model: None,
            request_id: None,
            reported_estimate: None,
            note: "reserved".into(),
        });
        s.refusal = None;
        Ok(Attempt::Priced(PricedAttempt {
            account: self.clone(),
            id,
            quote,
            tariff,
            output,
            sent: false,
            done: false,
        }))
    }
}
/// A dropped sent future keeps its full reservation as unknown charge.
pub(crate) enum Attempt {
    Priced(PricedAttempt),
    Unknown(unknown::UnknownAttempt),
}
impl Attempt {
    pub(crate) fn sent(&mut self) -> Result<(), ProviderError> {
        match self {
            Self::Priced(a) => a.sent(),
            Self::Unknown(a) => a.sent(),
        }
    }
    pub(crate) fn settle(&mut self, response: &InferResponse) -> Result<(), ProviderError> {
        match self {
            Self::Priced(a) => a.settle(response),
            Self::Unknown(a) => a.settle(response),
        }
    }
}
pub(crate) struct PricedAttempt {
    account: InferenceAdmission,
    id: usize,
    quote: Cost,
    tariff: InferenceTariff,
    output: u32,
    sent: bool,
    done: bool,
}
impl PricedAttempt {
    pub(crate) fn sent(&mut self) -> Result<(), ProviderError> {
        if self.sent || self.done {
            return Err(denied("attempt already dispatched"));
        }
        let mut s = self.account.lock()?;
        if s.status != AdmissionState::Open
            || s.committed()?.nano_usd > s.limit.nano_usd
            || s.limit.nano_usd == 0
        {
            return Err(s.refuse("allowance revoked or lowered before dispatch"));
        }
        self.sent = true;
        s.attempts[self.id].sent = true;
        Ok(())
    }
    pub(crate) fn settle(&mut self, response: &InferResponse) -> Result<(), ProviderError> {
        if self.done || !self.sent {
            return Err(denied(
                "attempt must be sent exactly once before settlement",
            ));
        }
        let u = &response.usage;
        let estimate = self.tariff.price(
            u.input_tokens,
            u.output_tokens,
            u.cache_read_tokens.unwrap_or(0),
        );
        let complete = response.usage_completeness == UsageCompleteness::Complete
            && response.gen_ai.response_model.as_deref() == Some(self.tariff.model)
            && u.input_tokens <= self.tariff.context_tokens
            && u.output_tokens <= u64::from(self.output)
            && estimate.is_some_and(|c| c.nano_usd <= self.quote.nano_usd);
        let mut s = self.account.lock()?;
        s.attempts[self.id].usage = Some(u.clone());
        s.attempts[self.id]
            .response_model
            .clone_from(&response.gen_ai.response_model);
        s.attempts[self.id]
            .request_id
            .clone_from(&response.request_id);
        s.attempts[self.id].reported_estimate = estimate;
        if !complete {
            s.status = AdmissionState::Uncertain;
            // Keep partial facts, but never convert them into a discount.
            return Err(
                s.refuse("incomplete or contradictory provider usage; charge remains unknown")
            );
        }
        let cost = estimate.ok_or_else(|| denied("unpriceable response"))?;
        let total = add(s.estimated, cost)?;
        s.active = Cost::new(s.active.nano_usd - self.quote.nano_usd);
        s.estimated = total;
        s.attempts[self.id].estimated = Some(cost);
        s.attempts[self.id].note =
            "complete usage priced at pinned catalog tariff; invoice unknown".into();
        self.done = true;
        Ok(())
    }
}
impl Drop for PricedAttempt {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if let Ok(mut s) = self.account.lock() {
            s.active = Cost::new(s.active.nano_usd - self.quote.nano_usd);
            if self.sent {
                // active + held already fitted the checked allowance.
                s.held = Cost::new(s.held.nano_usd + self.quote.nano_usd);
                s.status = AdmissionState::Uncertain;
                if s.refusal.is_none() {
                    s.refusal = Some(
                        "dispatched request ended without complete usage; charge unknown".into(),
                    );
                }
                s.attempts[self.id].note = "charge unknown; reservation retained".into();
            } else {
                s.attempts[self.id].note = "not dispatched; reservation released".into();
            }
        }
    }
}

#[cfg(test)]
#[path = "admission/wire_tests.rs"]
mod wire_tests;

impl InferenceReceipt {
    /// Durable observation for traces/recovery, NEVER restorable execution
    /// authority. Old receipts are not recomputed against the current catalog.
    /// Nano-currency amounts are decimal strings to preserve the full i128 range.
    #[must_use]
    pub fn observation(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "nika/inference-cost-observation@1",
            "known_subtotal_nano_usd": self.estimated.nano_usd.to_string(),
            "unknown_calls": self.unknown_calls,
            "unknown_cost": self.unknown_cost,
            "unknown_attempts": self.unknown_attempts.iter().map(|a| serde_json::json!({
                "id": a.id, "choice": a.choice, "pricing": a.pricing, "sent": a.sent,
                "usage": a.usage, "estimated_nano_usd": a.estimated.map(|c| c.nano_usd.to_string()),
                "native_estimated_nano": a.native_estimated_nano.map(|c| c.to_string()),
                "currency": a.currency, "response_model": a.response_model, "request_id": a.request_id, "note": a.note,
            })).collect::<Vec<_>>(),
            "overridden_defaults": self.overridden_defaults.map(|c| c.map(|c| c.nano_usd.to_string())),
            "limit_nano_usd": self.unknown_cost.is_none().then(|| self.limit.nano_usd.to_string()),
            "billed_nano_usd": self.billed.map(|c| c.nano_usd.to_string()),
            "state": format!("{:?}", self.state),
            "refusal": self.refusal,
            "attempts": self.attempts.iter().map(|a| serde_json::json!({
                "id": a.id, "model": a.model, "endpoint": a.endpoint, "sent": a.sent,
                "estimated_nano_usd": a.estimated.map(|c| c.nano_usd.to_string()),
                "reserved_nano_usd": a.reserved.nano_usd.to_string(), "usage": a.usage,
                "billing_provider": a.tariff.billing_provider, "currency": a.tariff.currency,
                "source": a.tariff.source, "as_of": a.tariff.as_of,
                "source_sha256": a.tariff.source_sha256, "note": a.note,
            })).collect::<Vec<_>>(),
        })
    }
}
