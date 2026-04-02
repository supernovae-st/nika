//! Mock transport for testing the Client→Transport→types path.
//!
//! `MockTransport` implements the sealed `Transport` trait and returns
//! canned responses. Used in unit tests to verify the full SDK flow
//! without any I/O.

#![cfg(test)]

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Mutex;

use crate::error::SdkError;
use crate::transport::Transport;
use crate::types::{ArtifactInfo, Event, EventStream, JobInfo, RunRequest};

/// Canned behavior for the mock.
#[derive(Clone)]
pub(crate) struct MockTransport {
    pub job_id: String,
    pub events: Arc<Vec<Event>>,
    pub artifacts: Arc<Vec<ArtifactInfo>>,
    pub status: Arc<Mutex<String>>,
    pub output: Arc<Mutex<Option<String>>>,
    pub fail_submit: bool,
}

impl Default for MockTransport {
    fn default() -> Self {
        Self {
            job_id: "mock-job-001".into(),
            events: Arc::new(vec![
                Event::Started {
                    job_id: "mock-job-001".into(),
                },
                Event::TaskStart {
                    job_id: "mock-job-001".into(),
                    task_id: "step1".into(),
                    verb: "infer".into(),
                },
                Event::TaskComplete {
                    job_id: "mock-job-001".into(),
                    task_id: "step1".into(),
                    duration_ms: 150,
                },
                Event::Completed {
                    job_id: "mock-job-001".into(),
                    output: Some("mock output".into()),
                },
            ]),
            artifacts: Arc::new(vec![ArtifactInfo {
                name: "report.md".into(),
                size: 1024,
                format: Some("markdown".into()),
                content_type: "text/markdown".into(),
                checksum: Some("abc123".into()),
            }]),
            status: Arc::new(Mutex::new("completed".into())),
            output: Arc::new(Mutex::new(Some("mock output".into()))),
            fail_submit: false,
        }
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn submit(&self, _req: &RunRequest) -> Result<String, SdkError> {
        if self.fail_submit {
            return Err(SdkError::InvalidWorkflow("mock failure".into()));
        }
        Ok(self.job_id.clone())
    }

    async fn status(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        if job_id != self.job_id {
            return Err(SdkError::NotFound(job_id.into()));
        }
        let status = self.status.lock().await.clone();
        let output = self.output.lock().await.clone();
        Ok(JobInfo {
            job_id: job_id.into(),
            status,
            workflow: "test.nika.yaml".into(),
            created_at: "2026-04-02T12:00:00Z".into(),
            started_at: Some("2026-04-02T12:00:01Z".into()),
            completed_at: None,
            exit_code: None,
            output,
        })
    }

    async fn cancel(&self, job_id: &str) -> Result<JobInfo, SdkError> {
        if job_id != self.job_id {
            return Err(SdkError::NotFound(job_id.into()));
        }
        *self.status.lock().await = "cancelled".into();
        Ok(JobInfo {
            job_id: job_id.into(),
            status: "cancelled".into(),
            workflow: "test.nika.yaml".into(),
            created_at: "2026-04-02T12:00:00Z".into(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
        })
    }

    async fn events(&self, job_id: &str) -> Result<EventStream, SdkError> {
        if job_id != self.job_id {
            return Err(SdkError::NotFound(job_id.into()));
        }
        let events = Arc::clone(&self.events);
        let stream = async_stream::stream! {
            for event in events.iter() {
                yield Ok(event.clone());
            }
        };
        Ok(Box::pin(stream))
    }

    async fn list_artifacts(&self, job_id: &str) -> Result<Vec<ArtifactInfo>, SdkError> {
        if job_id != self.job_id {
            return Err(SdkError::NotFound(job_id.into()));
        }
        Ok((*self.artifacts).clone())
    }

    async fn download_artifact(&self, job_id: &str, name: &str) -> Result<Bytes, SdkError> {
        if job_id != self.job_id {
            return Err(SdkError::NotFound(job_id.into()));
        }
        if self.artifacts.iter().any(|a| a.name == name) {
            Ok(Bytes::from("mock file content"))
        } else {
            Err(SdkError::NotFound(format!("artifact: {name}")))
        }
    }

    async fn health(&self) -> Result<bool, SdkError> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Client, Job};
    use crate::types::RunOptions;
    use futures_util::StreamExt;

    fn mock_client() -> Client {
        let transport = MockTransport::default();
        Client {
            transport: Arc::new(transport),
        }
    }

    fn mock_client_with(transport: MockTransport) -> Client {
        Client {
            transport: Arc::new(transport),
        }
    }

    #[tokio::test]
    async fn submit_returns_job() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        assert_eq!(job.job_id(), "mock-job-001");
    }

    #[tokio::test]
    async fn submit_with_inputs() {
        let client = mock_client();
        let opts = RunOptions::new().with_inputs(serde_json::json!({"topic": "AI"}));
        let job = client.submit("test.nika.yaml", opts).await.unwrap();
        assert_eq!(job.job_id(), "mock-job-001");
    }

    #[tokio::test]
    async fn submit_failure() {
        let transport = MockTransport {
            fail_submit: true,
            ..Default::default()
        };
        let client = mock_client_with(transport);
        let result = client.submit("bad.nika.yaml", RunOptions::new()).await;
        assert!(matches!(result, Err(SdkError::InvalidWorkflow(_))));
    }

    #[tokio::test]
    async fn job_status() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let info = job.status().await.unwrap();
        assert_eq!(info.status, "completed");
        assert_eq!(info.workflow, "test.nika.yaml");
    }

    #[tokio::test]
    async fn job_cancel() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let info = job.cancel().await.unwrap();
        assert_eq!(info.status, "cancelled");
    }

    #[tokio::test]
    async fn job_events_stream() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let mut stream = job.events().await.unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], Event::Started { .. }));
        assert!(matches!(events[1], Event::TaskStart { .. }));
        assert!(matches!(events[2], Event::TaskComplete { .. }));
        assert!(matches!(events[3], Event::Completed { .. }));
    }

    #[tokio::test]
    async fn job_wait_returns_result() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let result = job.wait().await.unwrap();
        assert_eq!(result.job_id, "mock-job-001");
        assert_eq!(result.output.as_deref(), Some("mock output"));
    }

    #[tokio::test]
    async fn job_wait_on_failure() {
        let transport = MockTransport {
            events: Arc::new(vec![
                Event::Started {
                    job_id: "mock-job-001".into(),
                },
                Event::Failed {
                    job_id: "mock-job-001".into(),
                    error: Some("DAG cycle".into()),
                },
            ]),
            ..Default::default()
        };
        let client = mock_client_with(transport);
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let result = job.wait().await;
        assert!(matches!(result, Err(SdkError::Engine { .. })));
    }

    #[tokio::test]
    async fn job_wait_on_cancel() {
        let transport = MockTransport {
            events: Arc::new(vec![
                Event::Started {
                    job_id: "mock-job-001".into(),
                },
                Event::Cancelled {
                    job_id: "mock-job-001".into(),
                },
            ]),
            ..Default::default()
        };
        let client = mock_client_with(transport);
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let result = job.wait().await;
        assert!(matches!(result, Err(SdkError::Engine { .. })));
    }

    #[tokio::test]
    async fn job_artifacts() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let artifacts = job.artifacts().await.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].name(), "report.md");
        assert_eq!(artifacts[0].size(), 1024);
        assert_eq!(artifacts[0].content_type(), "text/markdown");
    }

    #[tokio::test]
    async fn artifact_download() {
        let client = mock_client();
        let job = client.submit("test.nika.yaml", RunOptions::new()).await.unwrap();
        let artifacts = job.artifacts().await.unwrap();
        let data = artifacts[0].download().await.unwrap();
        assert_eq!(data.as_ref(), b"mock file content");
    }

    #[tokio::test]
    async fn health_check() {
        let client = mock_client();
        assert!(client.health().await.unwrap());
    }

    #[tokio::test]
    async fn status_not_found() {
        let client = mock_client();
        let job = Job {
            job_id: "nonexistent".into(),
            transport: Arc::new(MockTransport::default()),
        };
        let result = job.status().await;
        assert!(matches!(result, Err(SdkError::NotFound(_))));
    }

    #[tokio::test]
    async fn client_is_clone() {
        let client = mock_client();
        let client2 = client.clone();
        // Both should work
        assert!(client.health().await.unwrap());
        assert!(client2.health().await.unwrap());
    }
}
