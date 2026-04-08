// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Embedded transport — in-process workflow execution via `nika-engine`.
//!
//! Gated behind `#[cfg(feature = "embedded")]`.
//! Runs workflows directly using `Runner` without a separate server.

#![cfg(feature = "embedded")]

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex;

use crate::error::SdkError;
use crate::transport::Transport;
use crate::types::{ArtifactInfo, Event, EventStream, JobInfo, JobStatus, RunRequest};

/// State for a running or completed embedded job.
struct JobState {
    status: JobStatus,
    workflow: String,
    output: Option<String>,
    cancel_token: tokio_util::sync::CancellationToken,
    /// Broadcast sender for engine events — subscribers get real task events.
    event_tx: Option<tokio::sync::broadcast::Sender<Event>>,
    /// Handle to the monitor task — aborted on cancel to stop the workflow.
    monitor_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Maximum number of jobs retained in the embedded job map.
/// Once exceeded, completed/failed/cancelled jobs are reaped (oldest first).
const MAX_JOBS: usize = 1024;

pub(crate) struct EmbeddedTransport {
    jobs: Arc<Mutex<std::collections::HashMap<String, JobState>>>,
}

impl EmbeddedTransport {
    pub fn new() -> Result<Self, SdkError> {
        Ok(Self {
            jobs: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }
}

/// Reap terminal jobs when the map exceeds `MAX_JOBS`.
fn reap_completed_jobs(jobs: &mut std::collections::HashMap<String, JobState>) {
    if jobs.len() <= MAX_JOBS {
        return;
    }
    let terminal: Vec<String> = jobs
        .iter()
        .filter(|(_, s)| {
            matches!(
                s.status,
                JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
            )
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in terminal {
        jobs.remove(&id);
        if jobs.len() <= MAX_JOBS / 2 {
            break; // reap down to half capacity
        }
    }
}

#[async_trait]
impl Transport for EmbeddedTransport {
    async fn submit(&self, req: &RunRequest) -> Result<String, SdkError> {
        // Fail fast: resume not supported in embedded mode
        if req.resume_from.is_some() {
            return Err(SdkError::Engine {
                message: "resume not supported in embedded mode".into(),
                code: None,
            });
        }

        let job_id = uuid::Uuid::new_v4().to_string();

        // Read and parse workflow
        let yaml = tokio::fs::read_to_string(&req.workflow)
            .await
            .map_err(|e| SdkError::InvalidWorkflow(format!("{}: {e}", req.workflow)))?;

        let base_path = PathBuf::from(&req.workflow)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let analyzed = nika_engine::ast::parse_analyzed_with_includes(&yaml, &base_path)
            .map_err(|e| SdkError::InvalidWorkflow(e.to_string()))?;

        // Set up event broadcasting
        let (event_log, event_rx) = nika_event::EventLog::new_with_broadcast();
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let mut runner = nika_engine::runtime::Runner::with_event_log(analyzed, event_log)
            .map_err(|e| SdkError::Engine {
                message: e.to_string(),
                code: None,
            })?
            .quiet() // REQUIRED: makes Runner Send
            .with_base_path(base_path)
            .with_cancel_token(cancel_token.clone());

        // Inject workflow inputs
        if let Some(ref inputs) = req.inputs {
            if let Some(map) = inputs.as_object() {
                for (key, value) in map {
                    runner = runner.with_initial_context(key, value.clone());
                }
            }
        }

        // Create SDK event broadcast channel
        let (sdk_event_tx, _) = tokio::sync::broadcast::channel::<Event>(256);

        // Track the job (reap old terminal jobs if needed)
        {
            let mut jobs = self.jobs.lock().await;
            reap_completed_jobs(&mut jobs);
            jobs.insert(
                job_id.clone(),
                JobState {
                    status: JobStatus::Running,
                    workflow: req.workflow.clone(),
                    output: None,
                    cancel_token,
                    event_tx: Some(sdk_event_tx.clone()),
                    monitor_handle: None, // set after spawning
                },
            );
        }

        // Forward engine events → SDK events
        let fwd_tx = sdk_event_tx;
        let fwd_jid = job_id.clone();
        tokio::spawn(async move {
            let mut rx = event_rx;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        #[allow(clippy::wildcard_enum_match_arm)]
                        let sdk_event = match &event.kind {
                            nika_event::EventKind::TaskStarted { task_id, verb, .. } => {
                                Some(Event::TaskStart {
                                    job_id: fwd_jid.clone(),
                                    task_id: task_id.to_string(),
                                    verb: verb.to_string(),
                                })
                            }
                            nika_event::EventKind::TaskCompleted {
                                task_id,
                                duration_ms,
                                ..
                            } => Some(Event::TaskComplete {
                                job_id: fwd_jid.clone(),
                                task_id: task_id.to_string(),
                                duration_ms: *duration_ms,
                            }),
                            nika_event::EventKind::TaskFailed {
                                task_id,
                                error,
                                duration_ms,
                                ..
                            } => Some(Event::TaskFailed {
                                job_id: fwd_jid.clone(),
                                task_id: task_id.to_string(),
                                error: error.clone(),
                                duration_ms: *duration_ms,
                            }),
                            nika_event::EventKind::ArtifactWritten {
                                task_id,
                                path,
                                size,
                                ..
                            } => Some(Event::ArtifactWritten {
                                job_id: fwd_jid.clone(),
                                task_id: task_id.to_string(),
                                path: path.clone(),
                                size: *size,
                            }),
                            _ => None,
                        };
                        if let Some(ev) = sdk_event {
                            let _ = fwd_tx.send(ev);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Run in background task — capture JoinHandle to detect panics
        let jobs = Arc::clone(&self.jobs);
        let jid = job_id.clone();
        let run_handle = tokio::spawn(async move { runner.run().await });

        // Monitor the run handle in a separate task
        let monitor_jobs = Arc::clone(&jobs);
        let monitor_jid = jid.clone();
        let monitor_handle = tokio::spawn(async move {
            match run_handle.await {
                Ok(Ok(output)) => {
                    let mut jobs = monitor_jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&monitor_jid) {
                        state.status = JobStatus::Completed;
                        state.output = Some(output);
                    }
                }
                Ok(Err(e)) => {
                    let mut jobs = monitor_jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&monitor_jid) {
                        state.status = JobStatus::Failed;
                        state.output = Some(e.to_string());
                    }
                }
                Err(join_err) => {
                    // Panic or cancellation in the spawned task
                    let mut jobs = monitor_jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&monitor_jid) {
                        state.status = JobStatus::Failed;
                        state.output = Some(format!("internal error: {join_err}"));
                    }
                }
            }
        });

        // Store the monitor handle so cancel() can abort it
        {
            let mut jobs = jobs.lock().await;
            if let Some(state) = jobs.get_mut(&jid) {
                state.monitor_handle = Some(monitor_handle);
            }
        }

        Ok(job_id)
    }

    async fn status(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        let jobs = self.jobs.lock().await;
        let state = jobs
            .get(job_id)
            .ok_or_else(|| SdkError::NotFound(job_id.into()))?;

        Ok(JobInfo {
            job_id: job_id.into(),
            status: state.status.clone(),
            workflow: state.workflow.clone(),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: state.output.clone(),
        })
    }

    async fn cancel(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        let mut jobs = self.jobs.lock().await;
        let state = jobs
            .get_mut(job_id)
            .ok_or_else(|| SdkError::NotFound(job_id.into()))?;
        state.cancel_token.cancel(); // signal the Runner to stop
        if let Some(handle) = state.monitor_handle.take() {
            handle.abort(); // abort the monitor task (which holds the run handle)
        }
        state.status = JobStatus::Cancelled;

        Ok(JobInfo {
            job_id: job_id.into(),
            status: JobStatus::Cancelled,
            workflow: state.workflow.clone(),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
        })
    }

    async fn events(&self, job_id: &str) -> Result<EventStream, SdkError> {
        let jobs = Arc::clone(&self.jobs);
        let jid = job_id.to_string();

        // Subscribe to SDK event broadcast + verify job exists
        let mut event_rx = {
            let jobs = jobs.lock().await;
            let state = jobs
                .get(&jid)
                .ok_or_else(|| SdkError::NotFound(jid.clone()))?;
            state
                .event_tx
                .as_ref()
                .map(|tx| tx.subscribe())
                .ok_or_else(|| SdkError::StreamClosed)?
        };

        let stream = async_stream::stream! {
            yield Ok(Event::Started { job_id: jid.clone() });

            loop {
                // Try to receive a real task event with timeout
                match tokio::time::timeout(
                    std::time::Duration::from_millis(200),
                    event_rx.recv(),
                ).await {
                    Ok(Ok(event)) => {
                        yield Ok(event);
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                        // Engine finished — check terminal state
                    }
                    Err(_timeout) => {
                        // No event in 200ms — check terminal state
                    }
                }

                // Check for terminal state
                let terminal_event = {
                    let jobs = jobs.lock().await;
                    if let Some(state) = jobs.get(&jid) {
                        #[allow(clippy::wildcard_enum_match_arm)]
                        match state.status {
                            JobStatus::Completed => Some(Ok(Event::Completed {
                                job_id: jid.clone(),
                                output: state.output.clone(),
                            })),
                            JobStatus::Failed => Some(Ok(Event::Failed {
                                job_id: jid.clone(),
                                error: state.output.clone(),
                            })),
                            JobStatus::Cancelled => Some(Ok(Event::Cancelled {
                                job_id: jid.clone(),
                            })),
                            _ => None,
                        }
                    } else {
                        Some(Err(SdkError::NotFound(jid.clone())))
                    }
                };

                if let Some(event) = terminal_event {
                    yield event;
                    return;
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn list_artifacts(&self, job_id: &str) -> Result<Vec<ArtifactInfo>, SdkError> {
        let jobs = self.jobs.lock().await;
        if !jobs.contains_key(job_id) {
            return Err(SdkError::NotFound(job_id.into()));
        }
        Err(SdkError::Engine {
            message: "artifact listing not supported in embedded mode — use remote transport"
                .into(),
            code: None,
        })
    }

    async fn download_artifact(&self, job_id: &str, name: &str) -> Result<Bytes, SdkError> {
        Err(SdkError::NotFound(format!(
            "artifact {name} for job {job_id} (embedded mode)"
        )))
    }

    async fn health(&self) -> Result<bool, SdkError> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_transport_succeeds() {
        let transport = EmbeddedTransport::new();
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn health_always_true() {
        let transport = EmbeddedTransport::new().unwrap();
        assert!(transport.health().await.unwrap());
    }

    #[tokio::test]
    async fn status_unknown_job_returns_not_found() {
        let transport = EmbeddedTransport::new().unwrap();
        let err = transport.status("nonexistent-job").await.unwrap_err();
        assert!(matches!(err, SdkError::NotFound(_)));
    }

    #[tokio::test]
    async fn cancel_unknown_job_returns_not_found() {
        let transport = EmbeddedTransport::new().unwrap();
        let err = transport.cancel("nonexistent-job").await.unwrap_err();
        assert!(matches!(err, SdkError::NotFound(_)));
    }

    #[tokio::test]
    async fn events_unknown_job_returns_not_found() {
        let transport = EmbeddedTransport::new().unwrap();
        let result = transport.events("nonexistent-job").await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, SdkError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_artifacts_unknown_job_returns_not_found() {
        let transport = EmbeddedTransport::new().unwrap();
        let err = transport
            .list_artifacts("nonexistent-job")
            .await
            .unwrap_err();
        assert!(matches!(err, SdkError::NotFound(_)));
    }

    #[tokio::test]
    async fn download_artifact_returns_not_found() {
        let transport = EmbeddedTransport::new().unwrap();
        let err = transport
            .download_artifact("job1", "report.md")
            .await
            .unwrap_err();
        assert!(matches!(err, SdkError::NotFound(_)));
    }

    #[tokio::test]
    async fn submit_resume_rejected_before_file_read() {
        let transport = EmbeddedTransport::new().unwrap();
        let req = RunRequest {
            workflow: "nonexistent.nika.yaml".into(),
            inputs: None,
            resume_from: Some("prev-job".into()),
        };
        let err = transport.submit(&req).await.unwrap_err();
        assert!(matches!(err, SdkError::Engine { .. }));
        if let SdkError::Engine { message, .. } = &err {
            assert!(
                message.contains("resume not supported"),
                "expected resume error, got: {message}"
            );
        }
    }

    #[tokio::test]
    async fn submit_invalid_workflow_path() {
        let transport = EmbeddedTransport::new().unwrap();
        let req = RunRequest {
            workflow: "/nonexistent/path/workflow.nika.yaml".into(),
            inputs: None,
            resume_from: None,
        };
        let err = transport.submit(&req).await.unwrap_err();
        assert!(matches!(err, SdkError::InvalidWorkflow(_)));
    }

    #[tokio::test]
    async fn list_artifacts_known_job_returns_engine_error() {
        let transport = EmbeddedTransport::new().unwrap();
        // Manually insert a job to test artifact listing
        {
            let mut jobs = transport.jobs.lock().await;
            let cancel_token = tokio_util::sync::CancellationToken::new();
            jobs.insert(
                "test-job".into(),
                JobState {
                    status: JobStatus::Completed,
                    workflow: "test.nika.yaml".into(),
                    output: Some("done".into()),
                    cancel_token,
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }
        let err = transport.list_artifacts("test-job").await.unwrap_err();
        assert!(matches!(err, SdkError::Engine { .. }));
    }

    #[tokio::test]
    async fn cancel_sets_cancelled_status() {
        let transport = EmbeddedTransport::new().unwrap();
        let cancel_token = tokio_util::sync::CancellationToken::new();
        {
            let mut jobs = transport.jobs.lock().await;
            jobs.insert(
                "cancel-me".into(),
                JobState {
                    status: JobStatus::Running,
                    workflow: "test.nika.yaml".into(),
                    output: None,
                    cancel_token: cancel_token.clone(),
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }

        let info = transport.cancel("cancel-me").await.unwrap();
        assert_eq!(info.status, JobStatus::Cancelled);
        assert!(cancel_token.is_cancelled());
    }

    #[tokio::test]
    async fn status_returns_job_info() {
        let transport = EmbeddedTransport::new().unwrap();
        {
            let mut jobs = transport.jobs.lock().await;
            jobs.insert(
                "info-job".into(),
                JobState {
                    status: JobStatus::Running,
                    workflow: "pipeline.nika.yaml".into(),
                    output: None,
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }

        let info = transport.status("info-job").await.unwrap();
        assert_eq!(info.job_id, "info-job");
        assert_eq!(info.status, JobStatus::Running);
        assert_eq!(info.workflow, "pipeline.nika.yaml");
    }

    #[test]
    fn reap_noop_below_threshold() {
        let mut jobs = std::collections::HashMap::new();
        for i in 0..MAX_JOBS {
            jobs.insert(
                format!("job-{i}"),
                JobState {
                    status: JobStatus::Completed,
                    workflow: "test.nika.yaml".into(),
                    output: None,
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }
        assert_eq!(jobs.len(), MAX_JOBS);
        reap_completed_jobs(&mut jobs);
        // At threshold, no reaping
        assert_eq!(jobs.len(), MAX_JOBS);
    }

    #[test]
    fn reap_removes_terminal_above_threshold() {
        let mut jobs = std::collections::HashMap::new();
        // Fill to MAX_JOBS + 1 with completed jobs
        for i in 0..=MAX_JOBS {
            jobs.insert(
                format!("job-{i}"),
                JobState {
                    status: JobStatus::Completed,
                    workflow: "test.nika.yaml".into(),
                    output: None,
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }
        assert_eq!(jobs.len(), MAX_JOBS + 1);
        reap_completed_jobs(&mut jobs);
        // Reaped down to MAX_JOBS / 2
        assert!(jobs.len() <= MAX_JOBS / 2);
    }

    #[test]
    fn reap_preserves_running_jobs() {
        let mut jobs = std::collections::HashMap::new();
        // All running — reap cannot remove any
        for i in 0..=MAX_JOBS {
            jobs.insert(
                format!("job-{i}"),
                JobState {
                    status: JobStatus::Running,
                    workflow: "test.nika.yaml".into(),
                    output: None,
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    event_tx: None,
                    monitor_handle: None,
                },
            );
        }
        reap_completed_jobs(&mut jobs);
        // Nothing removed — all running
        assert_eq!(jobs.len(), MAX_JOBS + 1);
    }
}
