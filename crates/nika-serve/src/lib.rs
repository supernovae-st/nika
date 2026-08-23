// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable state and authenticated loopback HTTP for Nika remote execution.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod job;
pub mod server;

pub use job::{
    Admission, ApprovalHistory, ApprovalHistoryError, EventPageLimit, IdempotencyKey, JobEvent,
    JobId, JobMutation, JobRecord, JobStatus, JobStore, JobStoreError, MAX_EVENT_BATCH_LEN,
    MAX_EVENT_PAGE_LEN, MAX_EVENT_PAYLOAD_BYTES, MAX_JOB_SNAPSHOT_BYTES, RequestDigest,
    ServerIncarnation,
};
pub use server::{
    BoundServer, ExecutionBackend, ExecutionDisposition, ServerConfig, ServerError, ServerLimits,
    serve_http,
};
