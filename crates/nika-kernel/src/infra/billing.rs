// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Billing sink trait for cost tracking.
//!
//! `BillingSink` is SEPARATE from `EventSink` because billing records are
//! never dropped by sampling. Telemetry can sample 10% of `ProviderResponded`
//! events, but every inference call must be metered for billing.
//!
//! This is a real day-1 trait (T4:A decision), not a stub.

use nika_error::NikaError;
use nika_error::cost::Cost;
use nika_error::id::{ModelId, ProviderId};

use nika_error::token_usage::TokenUsage;

/// Sink for billing records. Never dropped by sampling.
///
/// Production: database, analytics pipeline, Stripe metering.
/// Tests: `NullBillingSink` (no-op).
///
/// For telemetry events (may be sampled), see `EventSink`.
#[trait_variant::make(BillingSinkDyn: Send)]
pub trait BillingSink: Send + Sync + crate::sealed::Sealed {
    /// Record a billing event. Must never fail silently — errors
    /// should be surfaced to the caller for retry.
    ///
    /// CANCEL SAFETY: cancel-safe. The sink should write the record
    /// atomically (single DB insert or single analytics event). If the
    /// caller drops the future mid-await, the pending write either lands
    /// or is rolled back by the underlying transaction — no partial state.
    /// Impls MUST NOT split a single record into multiple awaits.
    async fn record(
        &self,
        cost: Cost,
        usage: &TokenUsage,
        provider: &ProviderId,
        model: &ModelId,
    ) -> Result<(), NikaError>;
}

#[cfg(test)]
mod tests {
    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    // BillingSink trait object cannot be assembled directly here without
    // an impl; the trait_variant Dyn companion verifies object-safety at
    // expansion time. nika-kernel-mock::NullBillingSink covers behavior.
}
