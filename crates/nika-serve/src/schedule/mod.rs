// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

mod model;
mod store;

pub use model::{
    MAX_API_SCHEDULES, MAX_ENCODED_SCHEDULE_BYTES, MAX_SCHEDULE_STORE_BYTES, ScheduleApplyOutcome,
    ScheduleApplyPrecondition, ScheduleStoreError,
};
pub(crate) use model::{ScheduleClaimEvidence, ScheduleDecisionRecord, ScheduleSlotAction};
pub(crate) use store::STATE_VERSION;
pub use store::ScheduleStore;

#[cfg(test)]
mod tests;
