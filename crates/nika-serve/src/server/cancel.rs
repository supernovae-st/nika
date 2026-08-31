// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run-scoped cooperative cancellation authority.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use nika_types::cancel::CancelCtx;

use crate::JobId;

const CANCEL_POLL: Duration = Duration::from_millis(5);

/// One shared token per durable job.
///
/// A cancellation request may arrive before the queue worker registers. In
/// that case the cancelled token remains in the map and the later worker
/// inherits it. This closes the queued-to-running race without making the HTTP
/// adapter the owner of an execution task.
#[derive(Debug, Default)]
pub(super) struct ActiveCancellations {
    tokens: Mutex<BTreeMap<JobId, CancelCtx>>,
}

impl ActiveCancellations {
    pub(super) fn register(&self, id: &JobId) -> CancelCtx {
        let mut tokens = self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tokens.entry(id.clone()).or_default().clone()
    }

    pub(super) fn cancel(&self, id: &JobId) {
        let mut tokens = self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tokens.entry(id.clone()).or_default().cancel();
    }

    pub(super) fn retire(&self, id: &JobId) {
        let mut tokens = self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tokens.remove(id);
    }
}

pub(super) struct CancellationRegistration {
    active: std::sync::Arc<ActiveCancellations>,
    id: JobId,
}

impl CancellationRegistration {
    pub(super) fn new(active: std::sync::Arc<ActiveCancellations>, id: JobId) -> (Self, CancelCtx) {
        let token = active.register(&id);
        (Self { active, id }, token)
    }
}

impl Drop for CancellationRegistration {
    fn drop(&mut self) {
        self.active.retire(&self.id);
    }
}

pub(super) async fn cancelled(token: CancelCtx) {
    while !token.is_cancelled() {
        tokio::time::sleep(CANCEL_POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_before_registration_is_inherited() {
        let active = ActiveCancellations::default();
        let id = JobId::random();
        active.cancel(&id);
        assert!(active.register(&id).is_cancelled());
    }

    #[test]
    fn retirement_does_not_cancel_a_future_incarnation() {
        let active = ActiveCancellations::default();
        let id = JobId::random();
        active.cancel(&id);
        active.retire(&id);
        assert!(!active.register(&id).is_cancelled());
    }
}
