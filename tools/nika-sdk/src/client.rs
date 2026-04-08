// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public API: `Client`, `Job`, and `Artifact`.
//!
//! `Client` is the entry point — create one via `Client::remote()` or
//! `Client::embedded()`, then submit workflows and interact with jobs.

use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt;

use crate::error::SdkError;
use crate::transport::Transport;
use crate::types::{ArtifactInfo, Event, EventStream, JobInfo, JobResult, RunOptions, RunRequest};

/// Nika SDK client.
///
/// Wraps a transport backend (remote HTTP or embedded engine) behind
/// `Arc<dyn Transport>`, making `Client` cheaply clonable.
///
/// # Examples
///
/// ```rust,no_run
/// use nika_sdk::{Client, RunOptions};
///
/// # async fn example() -> Result<(), nika_sdk::SdkError> {
/// let client = Client::remote("http://localhost:3000", "my-token")?;
///
/// let job = client.submit("pipeline.nika.yaml", RunOptions::new()).await?;
/// let result = job.wait().await?;
/// println!("Output: {:?}", result.output);
/// # Ok(())
/// # }
/// ```
pub struct Client {
    pub(crate) transport: Arc<dyn Transport>,
}

impl Client {
    /// Create a client connected to a remote `nika serve` instance.
    #[cfg(feature = "remote")]
    pub fn remote(url: impl Into<String>, token: impl Into<String>) -> Result<Self, SdkError> {
        let transport = crate::remote::RemoteTransport::new(url, token)?;
        Ok(Self {
            transport: Arc::new(transport),
        })
    }

    /// Create a client using the embedded engine (in-process execution).
    #[cfg(feature = "embedded")]
    pub fn embedded() -> Result<Self, SdkError> {
        let transport = crate::embedded::EmbeddedTransport::new()?;
        Ok(Self {
            transport: Arc::new(transport),
        })
    }

    /// Submit a workflow for execution.
    ///
    /// Returns a `Job` handle for tracking progress, streaming events,
    /// and retrieving results.
    pub async fn submit(&self, workflow: &str, options: RunOptions) -> Result<Job, SdkError> {
        let req = RunRequest {
            workflow: workflow.to_string(),
            inputs: options.inputs,
            resume_from: options.resume_from,
        };
        let job_id = self.transport.submit(&req).await?;
        Ok(Job {
            job_id,
            transport: Arc::clone(&self.transport),
        })
    }

    /// Check if the server/engine is healthy.
    pub async fn health(&self) -> Result<bool, SdkError> {
        self.transport.health().await
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
        }
    }
}

/// Handle to a submitted workflow job.
///
/// Provides methods to poll status, stream events, wait for completion,
/// and access artifacts.
pub struct Job {
    pub(crate) job_id: String,
    pub(crate) transport: Arc<dyn Transport>,
}

impl Job {
    /// The unique job identifier.
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    /// Get current job status.
    pub async fn status(&self) -> Result<JobInfo, SdkError> {
        self.transport.status(&self.job_id).await
    }

    /// Cancel the job.
    pub async fn cancel(&self) -> Result<JobInfo, SdkError> {
        self.transport.cancel(&self.job_id).await
    }

    /// Subscribe to real-time events.
    pub async fn events(&self) -> Result<EventStream, SdkError> {
        self.transport.events(&self.job_id).await
    }

    /// List artifacts produced by this job.
    pub async fn artifacts(&self) -> Result<Vec<Artifact>, SdkError> {
        let infos = self.transport.list_artifacts(&self.job_id).await?;
        Ok(infos
            .into_iter()
            .map(|info| Artifact {
                job_id: self.job_id.clone(),
                info,
                transport: Arc::clone(&self.transport),
            })
            .collect())
    }

    /// Wait for the job to complete by consuming the event stream.
    ///
    /// Returns the final result on success, or an error if the job
    /// fails or is cancelled.
    #[allow(clippy::wildcard_enum_match_arm)]
    pub async fn wait(&self) -> Result<JobResult, SdkError> {
        let mut stream = self.events().await?;
        while let Some(event) = stream.next().await {
            match event? {
                Event::Completed { output, .. } => {
                    return Ok(JobResult {
                        job_id: self.job_id.clone(),
                        output,
                    });
                }
                Event::Failed { error, .. } => {
                    let msg = error.unwrap_or_default();
                    let code = if msg.starts_with("NIKA-") {
                        msg.split_whitespace()
                            .next()
                            .map(|s| s.trim_end_matches(':').to_string())
                    } else {
                        None
                    };
                    return Err(SdkError::Engine { message: msg, code });
                }
                Event::Cancelled { .. } => {
                    return Err(SdkError::Cancelled);
                }
                _ => continue,
            }
        }
        Err(SdkError::StreamClosed)
    }
}

/// A downloadable artifact from a completed job.
pub struct Artifact {
    job_id: String,
    info: ArtifactInfo,
    transport: Arc<dyn Transport>,
}

impl Artifact {
    /// Artifact name (filename).
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Artifact size in bytes.
    pub fn size(&self) -> u64 {
        self.info.size
    }

    /// Content type (MIME).
    pub fn content_type(&self) -> &str {
        &self.info.content_type
    }

    /// Checksum (if available).
    pub fn checksum(&self) -> Option<&str> {
        self.info.checksum.as_deref()
    }

    /// Artifact metadata.
    pub fn info(&self) -> &ArtifactInfo {
        &self.info
    }

    /// Download the artifact content.
    pub async fn download(&self) -> Result<Bytes, SdkError> {
        self.transport
            .download_artifact(&self.job_id, &self.info.name)
            .await
    }
}
