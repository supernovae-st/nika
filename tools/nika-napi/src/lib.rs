//! Node.js bindings for the Nika SDK via napi-rs.
//!
//! Wraps `nika-sdk` remote transport only — no embedded mode.
//! Async Rust methods map to JavaScript Promises automatically.

use std::sync::Arc;

use futures_util::StreamExt;
use napi::bindgen_prelude::*;
use napi_derive::napi;

// ═══════════════════════════════════════════════════════════════════════════
// ERROR MAPPING
// ═══════════════════════════════════════════════════════════════════════════

fn sdk_err(e: nika_sdk::SdkError) -> napi::Error {
  napi::Error::from_reason(e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// RUN OPTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Options for submitting a workflow run.
#[napi(object)]
pub struct RunOptions {
  /// Workflow inputs as a JSON object.
  pub inputs: Option<serde_json::Value>,
  /// Resume from a previous job's checkpoints.
  pub resume_from: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// EVENT
// ═══════════════════════════════════════════════════════════════════════════

/// A real-time event from a running workflow job.
#[napi(object)]
pub struct NikaEvent {
  /// Event type: "started", "task_start", "task_complete", "task_failed",
  /// "artifact_written", "completed", "failed", "cancelled"
  pub r#type: String,
  pub job_id: String,
  pub task_id: Option<String>,
  pub verb: Option<String>,
  pub duration_ms: Option<f64>,
  pub error: Option<String>,
  pub path: Option<String>,
  pub size: Option<f64>,
  pub output: Option<String>,
}

impl From<nika_sdk::Event> for NikaEvent {
  fn from(event: nika_sdk::Event) -> Self {
    match event {
      nika_sdk::Event::Started { job_id } => NikaEvent {
        r#type: "started".into(),
        job_id,
        task_id: None,
        verb: None,
        duration_ms: None,
        error: None,
        path: None,
        size: None,
        output: None,
      },
      nika_sdk::Event::TaskStart {
        job_id,
        task_id,
        verb,
      } => NikaEvent {
        r#type: "task_start".into(),
        job_id,
        task_id: Some(task_id),
        verb: Some(verb),
        duration_ms: None,
        error: None,
        path: None,
        size: None,
        output: None,
      },
      nika_sdk::Event::TaskComplete {
        job_id,
        task_id,
        duration_ms,
      } => NikaEvent {
        r#type: "task_complete".into(),
        job_id,
        task_id: Some(task_id),
        verb: None,
        duration_ms: Some(duration_ms as f64),
        error: None,
        path: None,
        size: None,
        output: None,
      },
      nika_sdk::Event::TaskFailed {
        job_id,
        task_id,
        error,
        duration_ms,
      } => NikaEvent {
        r#type: "task_failed".into(),
        job_id,
        task_id: Some(task_id),
        verb: None,
        duration_ms: Some(duration_ms as f64),
        error: Some(error),
        path: None,
        size: None,
        output: None,
      },
      nika_sdk::Event::ArtifactWritten {
        job_id,
        task_id,
        path,
        size,
      } => NikaEvent {
        r#type: "artifact_written".into(),
        job_id,
        task_id: Some(task_id),
        verb: None,
        duration_ms: None,
        error: None,
        path: Some(path),
        size: Some(size as f64),
        output: None,
      },
      nika_sdk::Event::Completed { job_id, output } => NikaEvent {
        r#type: "completed".into(),
        job_id,
        task_id: None,
        verb: None,
        duration_ms: None,
        error: None,
        path: None,
        size: None,
        output,
      },
      nika_sdk::Event::Failed { job_id, error } => NikaEvent {
        r#type: "failed".into(),
        job_id,
        task_id: None,
        verb: None,
        duration_ms: None,
        error,
        path: None,
        size: None,
        output: None,
      },
      nika_sdk::Event::Cancelled { job_id } => NikaEvent {
        r#type: "cancelled".into(),
        job_id,
        task_id: None,
        verb: None,
        duration_ms: None,
        error: None,
        path: None,
        size: None,
        output: None,
      },
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB INFO
// ═══════════════════════════════════════════════════════════════════════════

/// Status information for a job.
#[napi(object)]
pub struct JobInfo {
  pub job_id: String,
  pub status: String,
  pub workflow: String,
  pub created_at: String,
  pub started_at: Option<String>,
  pub completed_at: Option<String>,
  pub exit_code: Option<i32>,
  pub output: Option<String>,
}

impl From<nika_sdk::JobInfo> for JobInfo {
  fn from(info: nika_sdk::JobInfo) -> Self {
    Self {
      job_id: info.job_id,
      status: info.status,
      workflow: info.workflow,
      created_at: info.created_at,
      started_at: info.started_at,
      completed_at: info.completed_at,
      exit_code: info.exit_code,
      output: info.output,
    }
  }
}

/// Final result of a completed job.
#[napi(object)]
pub struct JobResult {
  pub job_id: String,
  pub output: Option<String>,
}

/// Artifact metadata.
#[napi(object)]
pub struct ArtifactInfo {
  pub name: String,
  pub size: f64,
  pub content_type: String,
  pub checksum: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ARTIFACT
// ═══════════════════════════════════════════════════════════════════════════

/// A downloadable artifact from a completed job.
#[napi]
pub struct Artifact {
  inner: nika_sdk::Artifact,
}

#[napi]
impl Artifact {
  /// Artifact name (filename).
  #[napi(getter)]
  pub fn name(&self) -> String {
    self.inner.name().to_string()
  }

  /// Artifact size in bytes.
  #[napi(getter)]
  pub fn size(&self) -> f64 {
    self.inner.size() as f64
  }

  /// Content type (MIME).
  #[napi(getter)]
  pub fn content_type(&self) -> String {
    self.inner.content_type().to_string()
  }

  /// Checksum (if available).
  #[napi(getter)]
  pub fn checksum(&self) -> Option<String> {
    self.inner.checksum().map(|s| s.to_string())
  }

  /// Download the artifact content.
  #[napi]
  pub async fn download(&self) -> Result<Buffer> {
    let bytes = self.inner.download().await.map_err(sdk_err)?;
    Ok(bytes.to_vec().into())
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB
// ═══════════════════════════════════════════════════════════════════════════

/// Handle to a submitted workflow job.
#[napi]
pub struct Job {
  inner: Arc<nika_sdk::Job>,
}

#[napi]
impl Job {
  /// The unique job identifier.
  #[napi(getter)]
  pub fn job_id(&self) -> String {
    self.inner.job_id().to_string()
  }

  /// Get current job status.
  #[napi]
  pub async fn status(&self) -> Result<JobInfo> {
    let info = self.inner.status().await.map_err(sdk_err)?;
    Ok(info.into())
  }

  /// Cancel the job.
  #[napi]
  pub async fn cancel(&self) -> Result<JobInfo> {
    let info = self.inner.cancel().await.map_err(sdk_err)?;
    Ok(info.into())
  }

  /// Wait for the job to complete.
  #[napi]
  pub async fn wait(&self) -> Result<JobResult> {
    let result = self.inner.wait().await.map_err(sdk_err)?;
    Ok(JobResult {
      job_id: result.job_id,
      output: result.output,
    })
  }

  /// List artifacts produced by this job.
  #[napi]
  pub async fn artifacts(&self) -> Result<Vec<Artifact>> {
    let artifacts = self.inner.artifacts().await.map_err(sdk_err)?;
    Ok(artifacts.into_iter().map(|a| Artifact { inner: a }).collect())
  }

  /// Collect all events into an array (non-streaming).
  ///
  /// For streaming, use `Client.streamEvents(jobId)` instead.
  #[napi]
  pub async fn collect_events(&self) -> Result<Vec<NikaEvent>> {
    let mut stream = self.inner.events().await.map_err(sdk_err)?;
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
      let event = event.map_err(sdk_err)?;
      let terminal = event.is_terminal();
      events.push(NikaEvent::from(event));
      if terminal {
        break;
      }
    }
    Ok(events)
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLIENT
// ═══════════════════════════════════════════════════════════════════════════

/// Nika SDK client — connects to a remote `nika serve` instance.
///
/// ```js
/// const { Client } = require('@nika/sdk');
///
/// const client = new Client('http://localhost:3000', process.env.NIKA_TOKEN);
/// const job = await client.submit('pipeline.nika.yaml', { inputs: { topic: 'AI' } });
/// const result = await job.wait();
/// console.log(result.output);
/// ```
#[napi]
pub struct Client {
  inner: nika_sdk::Client,
}

#[napi]
impl Client {
  /// Create a client connected to a remote `nika serve` instance.
  #[napi(constructor)]
  pub fn new(url: String, token: String) -> Result<Self> {
    let inner = nika_sdk::Client::remote(url, token).map_err(sdk_err)?;
    Ok(Self { inner })
  }

  /// Submit a workflow for execution.
  #[napi]
  pub async fn submit(
    &self,
    workflow: String,
    options: Option<RunOptions>,
  ) -> Result<Job> {
    let opts = match options {
      Some(o) => {
        let mut run_opts = nika_sdk::RunOptions::new();
        if let Some(inputs) = o.inputs {
          run_opts = run_opts.with_inputs(inputs);
        }
        if let Some(resume) = o.resume_from {
          run_opts = run_opts.with_resume(resume);
        }
        run_opts
      }
      None => nika_sdk::RunOptions::new(),
    };

    let job = self.inner.submit(&workflow, opts).await.map_err(sdk_err)?;
    Ok(Job {
      inner: Arc::new(job),
    })
  }

  /// Check if the server is healthy.
  #[napi]
  pub async fn health(&self) -> Result<bool> {
    self.inner.health().await.map_err(sdk_err)
  }

  /// One-liner: submit + wait for completion.
  ///
  /// ```js
  /// const result = await client.run('pipeline.nika.yaml', { inputs: { topic: 'AI' } });
  /// console.log(result.output);
  /// ```
  #[napi]
  pub async fn run(
    &self,
    workflow: String,
    options: Option<RunOptions>,
  ) -> Result<JobResult> {
    let job = self.submit(workflow, options).await?;
    job.wait().await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn event_conversion_started() {
    let event = nika_sdk::Event::Started {
      job_id: "j1".into(),
    };
    let napi_event = NikaEvent::from(event);
    assert_eq!(napi_event.r#type, "started");
    assert_eq!(napi_event.job_id, "j1");
    assert!(napi_event.task_id.is_none());
  }

  #[test]
  fn event_conversion_task_complete() {
    let event = nika_sdk::Event::TaskComplete {
      job_id: "j1".into(),
      task_id: "step1".into(),
      duration_ms: 1200,
    };
    let napi_event = NikaEvent::from(event);
    assert_eq!(napi_event.r#type, "task_complete");
    assert_eq!(napi_event.task_id.as_deref(), Some("step1"));
    assert_eq!(napi_event.duration_ms, Some(1200.0));
  }

  #[test]
  fn event_conversion_completed() {
    let event = nika_sdk::Event::Completed {
      job_id: "j1".into(),
      output: Some("done".into()),
    };
    let napi_event = NikaEvent::from(event);
    assert_eq!(napi_event.r#type, "completed");
    assert_eq!(napi_event.output.as_deref(), Some("done"));
  }

  #[test]
  fn event_conversion_task_failed() {
    let event = nika_sdk::Event::TaskFailed {
      job_id: "j1".into(),
      task_id: "step2".into(),
      error: "timeout".into(),
      duration_ms: 30000,
    };
    let napi_event = NikaEvent::from(event);
    assert_eq!(napi_event.r#type, "task_failed");
    assert_eq!(napi_event.error.as_deref(), Some("timeout"));
    assert_eq!(napi_event.duration_ms, Some(30000.0));
  }

  #[test]
  fn event_conversion_artifact_written() {
    let event = nika_sdk::Event::ArtifactWritten {
      job_id: "j1".into(),
      task_id: "report".into(),
      path: "output/report.md".into(),
      size: 4096,
    };
    let napi_event = NikaEvent::from(event);
    assert_eq!(napi_event.r#type, "artifact_written");
    assert_eq!(napi_event.path.as_deref(), Some("output/report.md"));
    assert_eq!(napi_event.size, Some(4096.0));
  }

  #[test]
  fn event_conversion_all_variants() {
    let events = vec![
      nika_sdk::Event::Started { job_id: "j".into() },
      nika_sdk::Event::TaskStart {
        job_id: "j".into(),
        task_id: "t".into(),
        verb: "infer".into(),
      },
      nika_sdk::Event::TaskComplete {
        job_id: "j".into(),
        task_id: "t".into(),
        duration_ms: 100,
      },
      nika_sdk::Event::TaskFailed {
        job_id: "j".into(),
        task_id: "t".into(),
        error: "err".into(),
        duration_ms: 50,
      },
      nika_sdk::Event::ArtifactWritten {
        job_id: "j".into(),
        task_id: "t".into(),
        path: "p".into(),
        size: 1,
      },
      nika_sdk::Event::Completed {
        job_id: "j".into(),
        output: None,
      },
      nika_sdk::Event::Failed {
        job_id: "j".into(),
        error: None,
      },
      nika_sdk::Event::Cancelled { job_id: "j".into() },
    ];

    let types: Vec<String> = events
      .into_iter()
      .map(|e| NikaEvent::from(e).r#type)
      .collect();

    assert_eq!(
      types,
      vec![
        "started",
        "task_start",
        "task_complete",
        "task_failed",
        "artifact_written",
        "completed",
        "failed",
        "cancelled",
      ]
    );
  }
}
