// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullAuditSink` — accepts everything, persists nowhere. Test-only.
//! `FailingAuditSink` — always returns `PersistFailed`. Drives error-path tests.

use nika_kernel::audit::{AuditRecord, AuditSink, AuditSinkError};

/// Test mock that accepts every record. NEVER use in production —
/// violates the "persist or fail" contract.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct NullAuditSink;

impl NullAuditSink {
    /// Construct a new mock.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl nika_kernel::sealed::Sealed for NullAuditSink {}

impl AuditSink for NullAuditSink {
    async fn audit(&self, _record: AuditRecord) -> Result<(), AuditSinkError> {
        Ok(())
    }
}

/// Test mock that always fails. Used to drive error-path tests.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct FailingAuditSink;

impl FailingAuditSink {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl nika_kernel::sealed::Sealed for FailingAuditSink {}

impl AuditSink for FailingAuditSink {
    async fn audit(&self, _record: AuditRecord) -> Result<(), AuditSinkError> {
        Err(AuditSinkError::PersistFailed {
            reason: "mock always fails".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_accepts_everything() {
        let sink = NullAuditSink::new();
        let r = AuditRecord::capability_override("t", "c", "r");
        assert!(sink.audit(r).await.is_ok());
    }

    #[tokio::test]
    async fn failing_always_fails() {
        let sink = FailingAuditSink::new();
        let r = AuditRecord::capability_override("t", "c", "r");
        assert!(sink.audit(r).await.is_err());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mocks_are_send_sync() {
        _assert_send_sync::<NullAuditSink>();
        _assert_send_sync::<FailingAuditSink>();
    }
}
