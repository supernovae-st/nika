// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{JobReceipt, JobRecord, JobStatus};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateJobRequest {
    pub(crate) workflow: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobResponse<'a> {
    id: &'a str,
    status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JobErrorBody<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outputs: Option<&'a BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<&'a JobReceipt>,
}

#[derive(Debug, Serialize)]
struct JobErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

impl<'a> From<&'a JobRecord> for JobResponse<'a> {
    fn from(record: &'a JobRecord) -> Self {
        let error = matches!(record.status(), JobStatus::Failed)
            .then(|| record.error())
            .flatten()
            .map(|(code, message)| JobErrorBody { code, message });
        Self {
            id: record.id().as_str(),
            status: record.status(),
            execution_id: record.execution_id(),
            trace_id: record.trace_id(),
            error,
            outputs: record.outputs(),
            receipt: record.receipt(),
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
