// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde::{Deserialize, Serialize};

use crate::{JobRecord, JobStatus};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateJobRequest {
    pub(crate) workflow: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobResponse<'a> {
    id: &'a str,
    status: JobStatus,
}

impl<'a> From<&'a JobRecord> for JobResponse<'a> {
    fn from(record: &'a JobRecord) -> Self {
        Self {
            id: record.id().as_str(),
            status: record.status(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct JobStatusResponse {
    status: JobStatus,
}

impl From<&JobRecord> for JobStatusResponse {
    fn from(record: &JobRecord) -> Self {
        Self {
            status: record.status(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowListResponse {
    workflows: Vec<String>,
}

impl WorkflowListResponse {
    pub(crate) fn new(workflows: Vec<String>) -> Self {
        Self { workflows }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowMetadataResponse<'a> {
    workflow: &'a str,
}

impl<'a> WorkflowMetadataResponse<'a> {
    pub(crate) fn new(workflow: &'a str) -> Self {
        Self { workflow }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    status: &'static str,
    service: &'static str,
    #[serde(flatten)]
    identity: nika_runtime::EngineIdentity,
}

impl HealthResponse {
    pub(crate) fn current() -> Self {
        Self {
            status: "ok",
            service: "nika-serve",
            identity: *nika_runtime::engine_identity(),
        }
    }
}
