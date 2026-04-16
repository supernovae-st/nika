// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null billing sink — discards all billing records.

use nika_error::NikaError;
use nika_error::cost::Cost;
use nika_error::id::{ModelId, ProviderId};
use nika_kernel::billing::BillingSink;
use nika_kernel::provider::TokenUsage;

/// No-op billing sink.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullBillingSink;

impl NullBillingSink {
    /// Create a new null billing sink.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

// Sealing: NullBillingSink lives in nika-kernel-mock (workspace-controlled),
// so it is allowed to participate in the sealed BillingSink lattice.
impl nika_kernel::sealed::Sealed for NullBillingSink {}

impl BillingSink for NullBillingSink {
    async fn record(
        &self,
        _cost: Cost,
        _usage: &TokenUsage,
        _provider: &ProviderId,
        _model: &ModelId,
    ) -> Result<(), NikaError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_succeeds() {
        let sink = NullBillingSink::new();
        sink.record(
            Cost::zero(),
            &TokenUsage::new(10, 5),
            &ProviderId::new("anthropic"),
            &ModelId::new("claude-sonnet-4"),
        )
        .await
        .unwrap();
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_billing_is_send_sync() {
        _assert_send_sync::<NullBillingSink>();
    }
}
