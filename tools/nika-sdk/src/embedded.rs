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
use crate::types::{ArtifactInfo, Event, EventStream, JobInfo, RunRequest};

/// State for a running or completed embedded job.
struct JobState {
    status: String,
    workflow: String,
    output: Option<String>,
}

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

#[async_trait]
impl Transport for EmbeddedTransport {
    async fn submit(&self, req: &RunRequest) -> Result<String, SdkError> {
        let job_id = uuid::Uuid::new_v4().to_string();

        // Read and parse workflow
        let yaml = tokio::fs::read_to_string(&req.workflow)
            .await
            .map_err(|e| SdkError::InvalidWorkflow(format!("{}: {e}", req.workflow)))?;

        let base_path = PathBuf::from(&req.workflow)
            .parent()
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
            .with_cancel_token(cancel_token);

        // Resume not supported in embedded mode yet
        if req.resume_from.is_some() {
            return Err(SdkError::Engine {
                message: "resume not supported in embedded mode".into(),
                code: None,
            });
        }

        // Inject workflow inputs
        if let Some(ref inputs) = req.inputs {
            if let Some(map) = inputs.as_object() {
                for (key, value) in map {
                    runner = runner.with_initial_context(key, value.clone());
                }
            }
        }

        // Track the job
        {
            let mut jobs = self.jobs.lock().await;
            jobs.insert(
                job_id.clone(),
                JobState {
                    status: "running".into(),
                    workflow: req.workflow.clone(),
                    output: None,
                },
            );
        }

        // Run in background task — capture JoinHandle to detect panics
        let jobs = Arc::clone(&self.jobs);
        let jid = job_id.clone();
        let handle = tokio::spawn(async move {
            let _event_rx = event_rx; // keep receiver alive during execution
            runner.run().await
        });

        // Monitor the handle in a separate task
        tokio::spawn(async move {
            match handle.await {
                Ok(Ok(output)) => {
                    let mut jobs = jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&jid) {
                        state.status = "completed".into();
                        state.output = Some(output);
                    }
                }
                Ok(Err(e)) => {
                    let mut jobs = jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&jid) {
                        state.status = "failed".into();
                        state.output = Some(e.to_string());
                    }
                }
                Err(join_err) => {
                    // Panic or cancellation in the spawned task
                    let mut jobs = jobs.lock().await;
                    if let Some(state) = jobs.get_mut(&jid) {
                        state.status = "failed".into();
                        state.output = Some(format!("internal error: {join_err}"));
                    }
                }
            }
        });

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
        state.status = "cancelled".into();

        Ok(JobInfo {
            job_id: job_id.into(),
            status: "cancelled".into(),
            workflow: state.workflow.clone(),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
        })
    }

    async fn events(&self, job_id: &str) -> Result<EventStream, SdkError> {
        // For embedded mode, poll job status until terminal
        let jobs = Arc::clone(&self.jobs);
        let jid = job_id.to_string();

        // Verify job exists
        {
            let jobs = jobs.lock().await;
            if !jobs.contains_key(&jid) {
                return Err(SdkError::NotFound(jid));
            }
        }

        let stream = async_stream::stream! {
            yield Ok(Event::Started { job_id: jid.clone() });

            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                // Read state and drop the lock BEFORE yielding
                let terminal_event = {
                    let jobs = jobs.lock().await;
                    if let Some(state) = jobs.get(&jid) {
                        match state.status.as_str() {
                            "completed" => Some(Ok(Event::Completed {
                                job_id: jid.clone(),
                                output: state.output.clone(),
                            })),
                            "failed" => Some(Ok(Event::Failed {
                                job_id: jid.clone(),
                                error: state.output.clone(),
                            })),
                            "cancelled" => Some(Ok(Event::Cancelled {
                                job_id: jid.clone(),
                            })),
                            _ => None,
                        }
                    } else {
                        Some(Err(SdkError::NotFound(jid.clone())))
                    }
                }; // MutexGuard dropped here

                match terminal_event {
                    Some(event) => {
                        yield event;
                        return;
                    }
                    None => continue,
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
        // Embedded mode doesn't track artifacts in v0.61
        Ok(Vec::new())
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
