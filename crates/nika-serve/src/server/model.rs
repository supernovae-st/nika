// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::{JobReceipt, JobRecord, JobStatus};

#[derive(Debug, Serialize)]
pub(crate) struct SnapshotValidationAck<'a> {
    status: &'static str,
    snapshot_digest: &'a str,
    root: &'a str,
    units: usize,
}

impl<'a> SnapshotValidationAck<'a> {
    pub(crate) fn accepted(snapshot: &'a nika_execution::ExecutionSnapshot) -> Self {
        Self {
            status: "accepted",
            snapshot_digest: snapshot.digest(),
            root: snapshot.root(),
            units: snapshot.len(),
        }
    }
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
    identity: HttpAdapterIdentity,
}

const HTTP_ADAPTER_CAPABILITIES: &[&str] = &["check", "executionSnapshot", "eventStream"];

#[derive(Debug, Serialize)]
struct HttpAdapterIdentity {
    engine_version: &'static str,
    build_sha: &'static str,
    spec_sha: &'static str,
    api_version: &'static str,
    #[serde(rename = "engineVersion")]
    engine_version_sdk: &'static str,
    #[serde(rename = "buildSha")]
    build_sha_sdk: &'static str,
    #[serde(rename = "specSha")]
    spec_sha_sdk: &'static str,
    #[serde(rename = "machineProtocolVersion")]
    machine_protocol_version: u32,
    #[serde(rename = "snapshotFormatVersion")]
    snapshot_format_version: u32,
    #[serde(rename = "checkReportVersion")]
    check_report_version: u32,
    #[serde(rename = "eventFormatVersion")]
    event_format_version: u32,
    #[serde(rename = "traceFormatVersion")]
    trace_format_version: u32,
    #[serde(rename = "supportedCapabilities")]
    supported_capabilities: &'static [&'static str],
}

impl HttpAdapterIdentity {
    fn current() -> Self {
        let identity = nika_runtime::engine_identity();
        Self {
            engine_version: identity.engine_version(),
            build_sha: identity.build_sha(),
            spec_sha: identity.spec_sha(),
            api_version: identity.api_version(),
            engine_version_sdk: identity.engine_version(),
            build_sha_sdk: identity.build_sha(),
            spec_sha_sdk: identity.spec_sha(),
            machine_protocol_version: identity.machine_protocol_version(),
            snapshot_format_version: identity.snapshot_format_version(),
            check_report_version: identity.check_report_version(),
            event_format_version: identity.event_format_version(),
            trace_format_version: identity.trace_format_version(),
            supported_capabilities: HTTP_ADAPTER_CAPABILITIES,
        }
    }
}

impl HealthResponse {
    pub(crate) fn current() -> Self {
        Self {
            status: "ok",
            service: "nika-serve",
            identity: HttpAdapterIdentity::current(),
        }
    }
}
