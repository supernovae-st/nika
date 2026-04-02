//! Sealed transport trait — the extension point for SDK backends.
//!
//! `Transport` is `pub(crate)` so external crates cannot implement it.
//! The SDK ships two implementations:
//! - `RemoteTransport` (HTTP/SSE, feature `remote`)
//! - `EmbeddedTransport` (in-process Runner, feature `embedded`)

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::SdkError;
use crate::types::{ArtifactInfo, EventStream, JobInfo, RunRequest};

#[async_trait]
pub(crate) trait Transport: Send + Sync {
    /// Submit a workflow for execution. Returns the job ID.
    async fn submit(&self, req: &RunRequest) -> Result<String, SdkError>;

    /// Get current job status.
    async fn status(&self, job_id: &str) -> Result<JobInfo, SdkError>;

    /// Cancel a running job.
    async fn cancel(&self, job_id: &str) -> Result<JobInfo, SdkError>;

    /// Subscribe to real-time events for a job.
    async fn events(&self, job_id: &str) -> Result<EventStream, SdkError>;

    /// List artifacts produced by a job.
    async fn list_artifacts(&self, job_id: &str) -> Result<Vec<ArtifactInfo>, SdkError>;

    /// Download a single artifact by name.
    async fn download_artifact(&self, job_id: &str, name: &str) -> Result<Bytes, SdkError>;

    /// Check server/engine health.
    async fn health(&self) -> Result<bool, SdkError>;
}
