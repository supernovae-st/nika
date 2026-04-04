//! SDK-owned types decoupled from internal crates.
//!
//! `Event` mirrors the SSE wire format (`#[serde(tag = "type")]`) so that
//! JSON from nika-serve deserializes directly into SDK types.

use std::pin::Pin;

use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::error::SdkError;

// ═══════════════════════════════════════════════════════════════════════════
// EVENTS
// ═══════════════════════════════════════════════════════════════════════════

/// Real-time event from a running workflow job.
///
/// Wire-compatible with `ServeEvent` in nika-serve — uses the same
/// `#[serde(tag = "type")]` discriminator so SSE `data:` payloads
/// deserialize directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "started")]
    Started { job_id: String },

    #[serde(rename = "task_start")]
    TaskStart {
        job_id: String,
        task_id: String,
        verb: String,
    },

    #[serde(rename = "task_complete")]
    TaskComplete {
        job_id: String,
        task_id: String,
        duration_ms: u64,
    },

    #[serde(rename = "task_failed")]
    TaskFailed {
        job_id: String,
        task_id: String,
        error: String,
        duration_ms: u64,
    },

    #[serde(rename = "artifact_written")]
    ArtifactWritten {
        job_id: String,
        task_id: String,
        path: String,
        size: u64,
    },

    #[serde(rename = "completed")]
    Completed {
        job_id: String,
        output: Option<String>,
    },

    #[serde(rename = "failed")]
    Failed {
        job_id: String,
        error: Option<String>,
    },

    #[serde(rename = "cancelled")]
    Cancelled { job_id: String },
}

impl Event {
    /// Whether this event terminates the stream.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

/// A boxed async stream of events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, SdkError>> + Send>>;

// ═══════════════════════════════════════════════════════════════════════════
// JOB STATUS
// ═══════════════════════════════════════════════════════════════════════════

/// Status of a workflow job.
///
/// Catches protocol drift at deserialization time — unknown statuses
/// from future server versions deserialize to `Unknown` rather than
/// failing outright.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    /// Forward-compat: unknown status strings from newer servers.
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Queued => write!(f, "queued"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB INFO
// ═══════════════════════════════════════════════════════════════════════════

/// Status of a job as returned by the `/v1/status/{id}` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobInfo {
    pub job_id: String,
    pub status: JobStatus,
    pub workflow: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
}

/// Final result of a completed job.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub job_id: String,
    pub output: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ARTIFACTS
// ═══════════════════════════════════════════════════════════════════════════

/// Metadata for a single artifact produced by a job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactInfo {
    pub name: String,
    pub size: u64,
    pub format: Option<String>,
    pub content_type: String,
    pub checksum: Option<String>,
}

/// List of artifacts for a job (as returned by the API).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ArtifactListResponse {
    #[allow(dead_code)]
    pub job_id: String,
    #[allow(dead_code)]
    pub count: usize,
    pub artifacts: Vec<ArtifactInfo>,
}

// ═══════════════════════════════════════════════════════════════════════════
// RUN OPTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Options for submitting a workflow run.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub inputs: Option<serde_json::Value>,
    pub resume_from: Option<String>,
}

impl RunOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_inputs(mut self, inputs: serde_json::Value) -> Self {
        self.inputs = Some(inputs);
        self
    }

    pub fn with_resume(mut self, job_id: impl Into<String>) -> Self {
        self.resume_from = Some(job_id.into());
        self
    }
}

/// Internal request payload for the Transport trait.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunRequest {
    pub workflow: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_from: Option<String>,
}

/// Response from POST /v1/run.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunResponse {
    pub job_id: String,
    #[allow(dead_code)]
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_started_roundtrip() {
        let event = Event::Started {
            job_id: "abc123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"started"#));
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn event_completed_with_output() {
        let event = Event::Completed {
            job_id: "x".into(),
            output: Some("result data".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn event_task_failed_roundtrip() {
        let event = Event::TaskFailed {
            job_id: "j".into(),
            task_id: "step1".into(),
            error: "timeout".into(),
            duration_ms: 500,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn event_is_terminal() {
        assert!(!Event::Started { job_id: "".into() }.is_terminal());
        assert!(!Event::TaskStart {
            job_id: "".into(),
            task_id: "".into(),
            verb: "".into(),
        }
        .is_terminal());
        assert!(Event::Completed {
            job_id: "".into(),
            output: None,
        }
        .is_terminal());
        assert!(Event::Failed {
            job_id: "".into(),
            error: None,
        }
        .is_terminal());
        assert!(Event::Cancelled { job_id: "".into() }.is_terminal());
    }

    #[test]
    fn deserialize_from_sse_wire_format() {
        // Simulates data from nika-serve SSE stream
        let wire = r#"{"type":"task_complete","job_id":"j1","task_id":"step1","duration_ms":1200}"#;
        let event: Event = serde_json::from_str(wire).unwrap();
        assert!(matches!(
            event,
            Event::TaskComplete {
                duration_ms: 1200,
                ..
            }
        ));
    }

    #[test]
    fn artifact_written_roundtrip() {
        let event = Event::ArtifactWritten {
            job_id: "j1".into(),
            task_id: "report".into(),
            path: "output/report.md".into(),
            size: 4096,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn job_info_deserialize() {
        let json = r#"{
            "job_id": "abc",
            "status": "running",
            "workflow": "test.nika.yaml",
            "created_at": "2026-04-02T12:00:00Z",
            "started_at": "2026-04-02T12:00:01Z",
            "completed_at": null,
            "exit_code": null,
            "output": null
        }"#;
        let info: JobInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.job_id, "abc");
        assert_eq!(info.status, JobStatus::Running);
        assert!(info.completed_at.is_none());
    }

    #[test]
    fn job_status_all_variants_roundtrip() {
        for (s, expected) in [
            ("\"pending\"", JobStatus::Pending),
            ("\"queued\"", JobStatus::Queued),
            ("\"running\"", JobStatus::Running),
            ("\"completed\"", JobStatus::Completed),
            ("\"failed\"", JobStatus::Failed),
            ("\"cancelled\"", JobStatus::Cancelled),
        ] {
            let status: JobStatus = serde_json::from_str(s).unwrap();
            assert_eq!(status, expected);
            let back = serde_json::to_string(&status).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn job_status_unknown_forward_compat() {
        let status: JobStatus = serde_json::from_str("\"paused\"").unwrap();
        assert_eq!(status, JobStatus::Unknown);
    }

    #[test]
    fn job_status_display() {
        assert_eq!(JobStatus::Running.to_string(), "running");
        assert_eq!(JobStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(JobStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn artifact_info_deserialize() {
        let json = r#"{
            "name": "report.md",
            "size": 4096,
            "format": "markdown",
            "content_type": "text/markdown",
            "checksum": "abc123"
        }"#;
        let info: ArtifactInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "report.md");
        assert_eq!(info.size, 4096);
    }

    #[test]
    fn run_options_fluent() {
        let opts = RunOptions::new()
            .with_inputs(serde_json::json!({"topic": "AI"}))
            .with_resume("prev-job-123");
        assert!(opts.inputs.is_some());
        assert_eq!(opts.resume_from.as_deref(), Some("prev-job-123"));
    }

    #[test]
    fn run_request_serializes_without_nones() {
        let req = RunRequest {
            workflow: "test.nika.yaml".into(),
            inputs: None,
            resume_from: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("inputs"));
        assert!(!json.contains("resume_from"));
    }
}
