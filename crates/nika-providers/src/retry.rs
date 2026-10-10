// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the transport did for a provider call, and why it never re-sends one.
//!
//! A provider call is sent once. The money admission counts the requests a call may send (the
//! author's `retry:` and the schema re-asks, never a transport re-send), so the transport never
//! re-sends on its own: a 429 (rate limited), a 503 (unavailable) or a 529 (Anthropic
//! `overloaded_error`) ends the call with a typed transient rejection, which carries the delay
//! the provider asked for when it named one ([`transient_rejection`]). The author's `retry:`
//! (allowed after a received 429 or 503) or the Session decides, by its own counted policy. An
//! absent usage report remains an unknown charge; status alone does not establish zero billing.
//! The [`TransportReport`] says what was sent: every dispatch of the call, one round-trip.

mod billing;
#[cfg(test)]
mod billing_tests;
pub(crate) use billing::record;

use std::time::Duration;

use nika_kernel::ai::provider::ProviderError;

/// Observed physical billing route, separate from the model's publisher.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct BillingRoute {
    /// Selected adapter namespace.
    pub provider: String,
    /// Exact selected wire model.
    pub model: String,
    /// Full final request endpoint, including its path.
    pub endpoint: String,
}
impl BillingRoute {
    /// Record an observed route, never a model-prefix inference. A URL carrying
    /// userinfo/query/fragment is not safe to journal; absence stays unpriced
    /// instead of stripping identity and applying a different route's tariff.
    #[must_use]
    pub fn new(provider: String, model: String, endpoint: String) -> Option<Self> {
        if endpoint.contains(['?', '#', '@']) || endpoint.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            provider,
            model,
            endpoint,
        })
    }
    /// The endpoint's origin ([`crate::route_origin`]): what a durable record
    /// names in its place. `None` when it has none to project.
    #[must_use]
    pub fn origin(&self) -> Option<String> {
        crate::route_origin(&self.endpoint)
    }
    /// Exact dated tariff if known. Non-USD is never converted here.
    #[must_use]
    pub fn tariff(&self) -> Option<nika_catalog::admission::InferenceTariff> {
        nika_catalog::admission::InferenceTariff::new(&self.provider, &self.model, &self.endpoint)
    }
    /// Provenance for a new receipt. Empty document hashes become null.
    #[must_use]
    pub fn observation(&self) -> serde_json::Value {
        let tariff = self.tariff();
        serde_json::json!({
            "route": self,
            "billing_provider": tariff.map(|t| t.billing_provider),
            "currency": tariff.map(|t| t.currency),
            "source": tariff.map(|t| t.source),
            "route_source": tariff.map(|t| t.route_source),
            "limits_source": tariff.map(|t| t.limits_source),
            "as_of": tariff.map(|t| t.as_of),
            "source_sha256": tariff.and_then(|t| (!t.source_sha256.is_empty()).then_some(t.source_sha256)),
            "unit": "nano_currency_per_token",
            "input_rate": tariff.and_then(|t| t.price_native(1, 0, 0)),
            "output_rate": tariff.and_then(|t| t.price_native(0, 1, 0)),
            "cached_rate": tariff.and_then(|t| t.price_native(1, 0, 1)),
            "table_schema": "nika/inference-admission@1.1",
            "usd_conversion": null,
            "kind": if tariff.is_some() { "catalog_estimate_not_invoice" } else { "unknown" },
        })
    }
}

/// What the transport did for one logical call: how many round-trips it
/// sent (one per call: it never re-sends), how long it waited between them
/// and which statuses it waited on (none: kept so receipts read one shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransportReport {
    /// Per-dispatch observations; mixed routes and failed calls remain separate.
    pub inference_calls: Vec<nika_types::cost::InferenceCall>,
    /// Present only when every absorbed round-trip reported the same full route.
    pub billing_route: Option<BillingRoute>,
    /// Round-trips sent (1 = the call answered first time).
    pub attempts: u32,
    /// Total wait between round-trips: zero, since the transport never re-sends.
    pub waited: Duration,
    /// The HTTP status of every answer that was retried, in order: empty, since the
    /// transport never re-sends.
    pub statuses: Vec<u16>,
}

impl TransportReport {
    /// A report before any round-trip (INV-019 · `new()` on every
    /// `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&mut self, call: Option<nika_types::cost::InferenceCall>) {
        let Some(call) = call else {
            return;
        };
        let route = call.route.as_ref().and_then(|r| {
            BillingRoute::new(r.provider.clone(), r.model.clone(), r.endpoint.clone())
        });
        if self.inference_calls.is_empty() {
            self.billing_route = route;
        } else if self.billing_route != route {
            self.billing_route = None;
        }
        self.inference_calls.push(call);
        self.attempts = u32::try_from(self.inference_calls.len()).unwrap_or(u32::MAX);
    }

    /// Whether at least one round-trip was retried.
    #[must_use]
    pub fn retried(&self) -> bool {
        !self.statuses.is_empty()
    }

    /// Fold another logical call's report in (a verb that sends several
    /// round-trips per task sums them — the receipt reads the task total).
    pub fn absorb(&mut self, other: &Self) {
        self.inference_calls
            .extend_from_slice(&other.inference_calls);
        if self.attempts == 0 {
            self.billing_route.clone_from(&other.billing_route);
        } else if self.billing_route != other.billing_route {
            self.billing_route = None;
        }
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.waited = self.waited.saturating_add(other.waited);
        self.statuses.extend_from_slice(&other.statuses);
    }

    /// One line for a receipt, `None` when nothing was retried:
    /// `retried 2× on HTTP 429 · 429 (waited 3.0 s)`.
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        if !self.retried() {
            return None;
        }
        let statuses = self
            .statuses
            .iter()
            .map(|s| format!("HTTP {s}"))
            .collect::<Vec<_>>()
            .join(" · ");
        Some(format!(
            "retried {}× on {statuses} (waited {:.1} s)",
            self.statuses.len(),
            self.waited.as_secs_f64()
        ))
    }
}

/// A provider's transient rejection a caller may act on explicitly: a 429 · 503 · 529 the
/// transport did not re-send, with the delay the provider asked for when it named one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransientRejection {
    /// The HTTP status: 429, 503 or 529.
    pub status: u16,
    /// The provider's `Retry-After` (or Gemini's `retryDelay`), when it named one.
    pub retry_after: Option<Duration>,
}

impl TransientRejection {
    /// A rejection (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub const fn new(status: u16, retry_after: Option<Duration>) -> Self {
        Self {
            status,
            retry_after,
        }
    }
}

/// The transient rejection `err` carries, through its observations: a 429 · 503 · 529 the
/// provider may answer later. `None` for anything else, an exhausted quota included (waiting
/// does not refill credit), and for a timeout or a dropped connection (the seat may have sampled
/// and billed).
#[must_use]
pub fn transient_rejection(err: &ProviderError) -> Option<TransientRejection> {
    let (status, retry_after_ms) = match err.unobserved() {
        ProviderError::HttpResponse { details } if details.is_transient() => {
            (details.status(), details.retry_after_ms())
        }
        ProviderError::RateLimited { retry_after_ms } => (429, *retry_after_ms),
        ProviderError::Api { status, .. } => (*status, None),
        _ => return None,
    };
    matches!(status, 429 | 503 | 529)
        .then(|| TransientRejection::new(status, retry_after_ms.map(Duration::from_millis)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::ai::provider::ProviderHttpError;

    fn http(status: u16, code: Option<&str>, retry_after: Option<&str>) -> ProviderError {
        ProviderError::HttpResponse {
            details: ProviderHttpError::new(status, code, None, retry_after),
        }
    }

    #[test]
    fn a_429_503_or_529_is_a_transient_rejection_with_the_delay_it_named() {
        let rejection = |err: &ProviderError| transient_rejection(err);
        assert_eq!(
            rejection(&http(429, None, None)),
            Some(TransientRejection::new(429, None))
        );
        assert_eq!(
            rejection(&http(503, None, Some("2"))),
            Some(TransientRejection::new(503, Some(Duration::from_secs(2))))
        );
        assert_eq!(
            rejection(&http(529, Some("overloaded_error"), Some("2.5"))),
            Some(TransientRejection::new(
                529,
                Some(Duration::from_millis(2500))
            ))
        );
        // Any delay is the caller's to weigh: the layer caps nothing it never waits on.
        assert_eq!(
            rejection(&http(429, None, Some("45"))),
            Some(TransientRejection::new(429, Some(Duration::from_secs(45))))
        );
        // A date-form header names no delay the layer can compute.
        assert_eq!(
            rejection(&http(429, None, Some("Sun, 06 Nov 1994 08:49:37 GMT"))),
            Some(TransientRejection::new(429, None))
        );
    }

    #[test]
    fn the_rest_of_5xx_a_wrong_request_or_a_dead_key_is_no_transient_rejection() {
        for status in [400, 401, 403, 404, 408, 422, 500, 502, 504, 520] {
            assert_eq!(
                transient_rejection(&http(status, None, None)),
                None,
                "{status}"
            );
        }
    }

    #[test]
    fn exhausted_quota_is_terminal_even_with_a_retry_after() {
        for code in ["insufficient_quota", "credit_balance_exhausted"] {
            assert_eq!(
                transient_rejection(&http(429, Some(code), Some("1"))),
                None,
                "{code}: waiting does not refill credit"
            );
        }
    }

    #[test]
    fn the_typed_variants_follow_the_same_table() {
        assert_eq!(
            transient_rejection(&ProviderError::RateLimited {
                retry_after_ms: Some(1500)
            }),
            Some(TransientRejection::new(
                429,
                Some(Duration::from_millis(1500))
            ))
        );
        let api = |status| ProviderError::Api {
            status,
            message: "m".to_owned(),
        };
        assert_eq!(
            transient_rejection(&api(503)),
            Some(TransientRejection::new(503, None))
        );
        assert_eq!(
            transient_rejection(&api(408)),
            None,
            "a timeout keeps its verdict"
        );
        assert_eq!(transient_rejection(&api(500)), None);
        for err in [
            ProviderError::Connection {
                reason: "reset".to_owned(),
            },
            ProviderError::Other {
                reason: "x".to_owned(),
            },
        ] {
            assert_eq!(transient_rejection(&err), None, "{err}");
        }
    }

    #[test]
    fn the_report_sums_and_summarizes() {
        let mut report = TransportReport::new();
        assert!(!report.retried());
        assert_eq!(report.summary(), None);
        report.attempts = 3;
        report.waited = Duration::from_millis(3000);
        report.statuses = vec![429, 429];
        let mut other = TransportReport::new();
        other.attempts = 1;
        report.absorb(&other);
        assert_eq!(report.attempts, 4);
        assert_eq!(
            report.summary().as_deref(),
            Some("retried 2× on HTTP 429 · HTTP 429 (waited 3.0 s)")
        );
    }
}
