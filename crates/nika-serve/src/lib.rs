// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Resident durable execution authority with optional authenticated HTTP.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod job;
pub mod schedule;
pub mod server;

pub use nika_cadence::ScheduleDecision;

pub use job::{
    Admission, ApprovalHistory, ApprovalHistoryError, EventPageLimit, IdempotencyKey, JobEvent,
    JobId, JobMutation, JobOrigin, JobReceipt, JobRecord, JobStatus, JobStore, JobStoreError,
    MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES, MAX_EVENT_BATCH_LEN, MAX_EVENT_PAGE_LEN,
    MAX_EVENT_PAYLOAD_BYTES, MAX_EXECUTION_SNAPSHOT_METADATA_BYTES,
    MAX_EXECUTION_SNAPSHOT_PATH_BYTES, MAX_JOB_SNAPSHOT_BYTES, RequestDigest, ServerIncarnation,
};
pub use schedule::{
    MAX_API_SCHEDULES, MAX_ENCODED_SCHEDULE_BYTES, MAX_SCHEDULE_STORE_BYTES, ScheduleApplyOutcome,
    ScheduleApplyPrecondition, ScheduleStore, ScheduleStoreError,
};
pub use server::{
    BoundServer, CredentialRefuse, ExecutionBackend, ExecutionDisposition, ExecutionOutcome,
    PreparedScheduledRun, ResidentAuthority, ResidentClock, ResidentConfig,
    ResidentExecutionCoordinator, ServerConfig, ServerError, ServerLimits, SystemResidentClock,
    serve_http,
};
