// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable state for the future Nika remote execution interface.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod job;

pub use job::{
    Admission, ApprovalHistory, ApprovalHistoryError, EventPageLimit, IdempotencyKey, JobEvent,
    JobId, JobMutation, JobRecord, JobStatus, JobStore, JobStoreError, MAX_EVENT_BATCH_LEN,
    MAX_EVENT_PAGE_LEN, MAX_EVENT_PAYLOAD_BYTES, MAX_JOB_SNAPSHOT_BYTES, RequestDigest,
    ServerIncarnation,
};
