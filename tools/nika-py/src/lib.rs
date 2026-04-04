//! Python bindings for the Nika SDK via PyO3.
//!
//! Wraps `nika-sdk` remote transport only.
//! Provides both sync and async APIs for Python consumers.

use std::sync::{Arc, OnceLock};

use futures_util::StreamExt;
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

// ═══════════════════════════════════════════════════════════════════════════
// TOKIO RUNTIME
// ═══════════════════════════════════════════════════════════════════════════

/// Global Tokio runtime — shared across all SDK calls.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create Tokio runtime")
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR MAPPING
// ═══════════════════════════════════════════════════════════════════════════

/// Map SdkError to appropriate Python exception.
fn sdk_err(e: nika_sdk::SdkError) -> PyErr {
    match e {
        nika_sdk::SdkError::InvalidUrl(msg) => PyValueError::new_err(msg),
        nika_sdk::SdkError::InvalidWorkflow(msg) => PyValueError::new_err(msg),
        nika_sdk::SdkError::Connection(msg) => PyConnectionError::new_err(msg),
        nika_sdk::SdkError::Timeout(d) => {
            PyTimeoutError::new_err(format!("request timeout after {d:?}"))
        }
        nika_sdk::SdkError::Unauthorized => PyConnectionError::new_err("authentication failed"),
        nika_sdk::SdkError::Cancelled => NikaError::new_err("job cancelled"),
        nika_sdk::SdkError::NotFound(msg) => NikaError::new_err(format!("not found: {msg}")),
        nika_sdk::SdkError::QueueFull => NikaError::new_err("server queue full"),
        nika_sdk::SdkError::RateLimited => NikaError::new_err("rate limited"),
        nika_sdk::SdkError::StreamClosed => NikaError::new_err("event stream closed unexpectedly"),
        nika_sdk::SdkError::EventParse(msg) => NikaError::new_err(format!("event parse: {msg}")),
        nika_sdk::SdkError::Http {
            status, message, ..
        } => NikaError::new_err(format!("HTTP {status}: {message}")),
        nika_sdk::SdkError::Engine { message, code } => {
            let msg = match code {
                Some(c) => format!("[{c}] {message}"),
                None => message,
            };
            NikaError::new_err(msg)
        }
        nika_sdk::SdkError::Other(e) => PyRuntimeError::new_err(e.to_string()),
    }
}

// Custom exception
pyo3::create_exception!(nika_sdk, NikaError, pyo3::exceptions::PyException);

// ═══════════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Status information for a job.
#[pyclass(frozen, eq)]
#[derive(Debug, Clone, PartialEq)]
pub struct JobInfo {
    #[pyo3(get)]
    pub job_id: String,
    #[pyo3(get)]
    pub status: String,
    #[pyo3(get)]
    pub workflow: String,
    #[pyo3(get)]
    pub created_at: String,
    #[pyo3(get)]
    pub started_at: Option<String>,
    #[pyo3(get)]
    pub completed_at: Option<String>,
    #[pyo3(get)]
    pub exit_code: Option<i32>,
    #[pyo3(get)]
    pub output: Option<String>,
}

impl From<nika_sdk::JobInfo> for JobInfo {
    fn from(info: nika_sdk::JobInfo) -> Self {
        Self {
            job_id: info.job_id,
            status: info.status.to_string(),
            workflow: info.workflow,
            created_at: info.created_at,
            started_at: info.started_at,
            completed_at: info.completed_at,
            exit_code: info.exit_code,
            output: info.output,
        }
    }
}

#[pymethods]
impl JobInfo {
    fn __repr__(&self) -> String {
        format!(
            "JobInfo(job_id={:?}, status={:?})",
            self.job_id, self.status
        )
    }
}

/// Final result of a completed job.
#[pyclass(frozen, eq)]
#[derive(Debug, Clone, PartialEq)]
pub struct JobResult {
    #[pyo3(get)]
    pub job_id: String,
    #[pyo3(get)]
    pub output: Option<String>,
}

#[pymethods]
impl JobResult {
    fn __repr__(&self) -> String {
        format!(
            "JobResult(job_id={:?}, output={:?})",
            self.job_id, self.output
        )
    }
}

/// Artifact metadata.
#[pyclass(frozen, eq)]
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub size: u64,
    #[pyo3(get)]
    pub format: Option<String>,
    #[pyo3(get)]
    pub content_type: String,
    #[pyo3(get)]
    pub checksum: Option<String>,
}

#[pymethods]
impl ArtifactInfo {
    fn __repr__(&self) -> String {
        format!("ArtifactInfo(name={:?}, size={})", self.name, self.size)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB
// ═══════════════════════════════════════════════════════════════════════════

/// Handle to a submitted workflow job.
#[pyclass]
pub struct Job {
    inner: Arc<nika_sdk::Job>,
}

#[pymethods]
impl Job {
    /// The unique job identifier.
    #[getter]
    fn job_id(&self) -> String {
        self.inner.job_id().to_string()
    }

    /// Get current job status (sync).
    fn status(&self, py: Python<'_>) -> PyResult<JobInfo> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(|| {
            runtime()
                .block_on(inner.status())
                .map(JobInfo::from)
                .map_err(sdk_err)
        })
    }

    /// Cancel the job (sync).
    fn cancel(&self, py: Python<'_>) -> PyResult<JobInfo> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(|| {
            runtime()
                .block_on(inner.cancel())
                .map(JobInfo::from)
                .map_err(sdk_err)
        })
    }

    /// Wait for job completion (sync, blocking).
    fn wait(&self, py: Python<'_>) -> PyResult<JobResult> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(|| {
            let result = runtime().block_on(inner.wait()).map_err(sdk_err)?;
            Ok(JobResult {
                job_id: result.job_id,
                output: result.output,
            })
        })
    }

    /// List artifacts produced by this job (sync).
    fn artifacts(&self, py: Python<'_>) -> PyResult<Vec<ArtifactInfo>> {
        let inner = Arc::clone(&self.inner);
        py.allow_threads(|| {
            let artifacts = runtime().block_on(inner.artifacts()).map_err(sdk_err)?;
            Ok(artifacts
                .into_iter()
                .map(|a| ArtifactInfo {
                    name: a.name().to_string(),
                    size: a.size(),
                    format: a.info().format.clone(),
                    content_type: a.content_type().to_string(),
                    checksum: a.checksum().map(|s| s.to_string()),
                })
                .collect())
        })
    }

    /// Download an artifact by name (sync, returns bytes).
    fn download_artifact(&self, py: Python<'_>, name: &str) -> PyResult<Vec<u8>> {
        let inner = Arc::clone(&self.inner);
        let name = name.to_string();
        py.allow_threads(|| {
            let artifacts = runtime().block_on(inner.artifacts()).map_err(sdk_err)?;
            let artifact = artifacts
                .into_iter()
                .find(|a| a.name() == name)
                .ok_or_else(|| NikaError::new_err(format!("artifact not found: {name}")))?;
            let data = runtime().block_on(artifact.download()).map_err(sdk_err)?;
            Ok(data.to_vec())
        })
    }

    /// Collect all events (sync, returns list of dicts).
    fn events(&self, py: Python<'_>) -> PyResult<PyObject> {
        let inner = Arc::clone(&self.inner);
        // Collect Rust data with GIL released, then convert to Python objects
        let events: Vec<nika_sdk::Event> = py.allow_threads(|| {
            runtime().block_on(async {
                let mut stream = inner.events().await.map_err(sdk_err)?;
                let mut events = Vec::new();
                while let Some(event) = stream.next().await {
                    let event = event.map_err(sdk_err)?;
                    let terminal = event.is_terminal();
                    events.push(event);
                    if terminal {
                        break;
                    }
                }
                Ok::<_, PyErr>(events)
            })
        })?;
        // Convert to Python dicts with GIL held (no with_gil needed — we already have py)
        let list: Vec<PyObject> = events.into_iter().map(|e| event_to_dict(py, e)).collect();
        Ok(list.into_pyobject(py)?.into())
    }

    /// Stream events as a blocking iterator.
    ///
    /// ```python
    /// for event in job.stream_events():
    ///     print(event["type"], event.get("task_id"))
    /// ```
    fn stream_events(&self, _py: Python<'_>) -> PyResult<EventStream> {
        let inner = Arc::clone(&self.inner);
        let (tx, rx) = std::sync::mpsc::sync_channel(64);

        // Spawn background task to forward events
        let rt = runtime();
        rt.spawn(async move {
            match inner.events().await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(e) => {
                                let terminal = e.is_terminal();
                                if tx.send(Ok(e)).is_err() {
                                    break;
                                }
                                if terminal {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e.to_string()));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });

        Ok(EventStream {
            receiver: std::sync::Mutex::new(Some(rx)),
        })
    }

    fn __repr__(&self) -> String {
        format!("Job(job_id={:?})", self.inner.job_id())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EVENT STREAM (sync iterator — GIL released during recv)
// ═══════════════════════════════════════════════════════════════════════════

/// Blocking iterator over job events — use with `for event in stream`.
///
/// Releases the GIL while waiting for the next event, so other Python
/// threads can run concurrently.
#[pyclass]
pub struct EventStream {
    receiver: std::sync::Mutex<
        Option<std::sync::mpsc::Receiver<std::result::Result<nika_sdk::Event, String>>>,
    >,
}

#[pymethods]
impl EventStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        // Take receiver out of Mutex, drop the guard before allow_threads
        let rx = {
            let mut guard = self
                .receiver
                .lock()
                .map_err(|_| NikaError::new_err("lock poisoned"))?;
            match guard.take() {
                Some(rx) => rx,
                None => return Ok(None), // StopIteration
            }
        }; // MutexGuard dropped here

        // Release GIL while blocking on recv — move rx into closure and return it
        let (result, rx) = py.allow_threads(move || {
            let result = rx.recv();
            (result, rx)
        });

        match result {
            Ok(Ok(event)) => {
                // Put receiver back for next iteration
                let mut guard = self
                    .receiver
                    .lock()
                    .map_err(|_| NikaError::new_err("lock poisoned"))?;
                *guard = Some(rx);
                Ok(Some(event_to_dict(py, event)))
            }
            Ok(Err(e)) => Err(NikaError::new_err(e)),
            Err(_) => Ok(None), // channel closed — StopIteration
        }
    }

    fn __repr__(&self) -> String {
        "EventStream(...)".to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLIENT
// ═══════════════════════════════════════════════════════════════════════════

/// Nika SDK client — connects to a remote `nika serve` instance.
///
/// ```python
/// from nika_sdk import Client
///
/// client = Client(base_url="http://localhost:3000", token="my-token")
/// result = client.run_sync("pipeline.nika.yaml", inputs={"topic": "AI"})
/// print(result.output)
/// ```
#[pyclass]
pub struct Client {
    inner: nika_sdk::Client,
}

#[pymethods]
impl Client {
    /// Create a client connected to a remote `nika serve` instance.
    #[new]
    #[pyo3(signature = (base_url, token=None))]
    fn new(base_url: String, token: Option<String>) -> PyResult<Self> {
        let inner =
            nika_sdk::Client::remote(base_url, token.unwrap_or_default()).map_err(sdk_err)?;
        Ok(Self { inner })
    }

    /// Submit a workflow for execution (sync).
    #[pyo3(signature = (workflow, inputs=None, resume_from=None))]
    fn submit(
        &self,
        py: Python<'_>,
        workflow: &str,
        inputs: Option<&Bound<'_, PyDict>>,
        resume_from: Option<String>,
    ) -> PyResult<Job> {
        let mut opts = nika_sdk::RunOptions::new();
        if let Some(inputs_dict) = inputs {
            let value: serde_json::Value = pythonize::depythonize(inputs_dict)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            opts = opts.with_inputs(value);
        }
        if let Some(resume) = resume_from {
            opts = opts.with_resume(resume);
        }

        let inner = self.inner.clone();
        let workflow = workflow.to_string();
        let job = py.allow_threads(|| {
            runtime()
                .block_on(inner.submit(&workflow, opts))
                .map_err(sdk_err)
        })?;
        Ok(Job {
            inner: Arc::new(job),
        })
    }

    /// One-liner: submit + wait for completion (sync, blocking).
    #[pyo3(signature = (workflow, inputs=None))]
    fn run_sync(
        &self,
        py: Python<'_>,
        workflow: &str,
        inputs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<JobResult> {
        let job = self.submit(py, workflow, inputs, None)?;
        job.wait(py)
    }

    /// Check if the server is healthy (sync).
    fn health(&self, py: Python<'_>) -> PyResult<bool> {
        let inner = self.inner.clone();
        py.allow_threads(|| runtime().block_on(inner.health()).map_err(sdk_err))
    }

    fn __repr__(&self) -> String {
        "Client(connected)".to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Convert an SDK Event to a Python dict.
fn event_to_dict(py: Python<'_>, event: nika_sdk::Event) -> PyObject {
    let dict = PyDict::new(py);
    match event {
        nika_sdk::Event::Started { job_id } => {
            let _ = dict.set_item("type", "started");
            let _ = dict.set_item("job_id", job_id);
        }
        nika_sdk::Event::TaskStart {
            job_id,
            task_id,
            verb,
        } => {
            let _ = dict.set_item("type", "task_start");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("task_id", task_id);
            let _ = dict.set_item("verb", verb);
        }
        nika_sdk::Event::TaskComplete {
            job_id,
            task_id,
            duration_ms,
        } => {
            let _ = dict.set_item("type", "task_complete");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("task_id", task_id);
            let _ = dict.set_item("duration_ms", duration_ms);
        }
        nika_sdk::Event::TaskFailed {
            job_id,
            task_id,
            error,
            duration_ms,
        } => {
            let _ = dict.set_item("type", "task_failed");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("task_id", task_id);
            let _ = dict.set_item("error", error);
            let _ = dict.set_item("duration_ms", duration_ms);
        }
        nika_sdk::Event::ArtifactWritten {
            job_id,
            task_id,
            path,
            size,
        } => {
            let _ = dict.set_item("type", "artifact_written");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("task_id", task_id);
            let _ = dict.set_item("path", path);
            let _ = dict.set_item("size", size);
        }
        nika_sdk::Event::Completed { job_id, output } => {
            let _ = dict.set_item("type", "completed");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("output", output);
        }
        nika_sdk::Event::Failed { job_id, error } => {
            let _ = dict.set_item("type", "failed");
            let _ = dict.set_item("job_id", job_id);
            let _ = dict.set_item("error", error);
        }
        nika_sdk::Event::Cancelled { job_id } => {
            let _ = dict.set_item("type", "cancelled");
            let _ = dict.set_item("job_id", job_id);
        }
    }
    dict.into()
}

// ═══════════════════════════════════════════════════════════════════════════
// MODULE
// ═══════════════════════════════════════════════════════════════════════════

/// Nika SDK Python module.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    m.add_class::<Job>()?;
    m.add_class::<JobInfo>()?;
    m.add_class::<JobResult>()?;
    m.add_class::<ArtifactInfo>()?;
    m.add_class::<EventStream>()?;
    m.add("NikaError", m.py().get_type::<NikaError>())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_to_dict_started() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let event = nika_sdk::Event::Started {
                job_id: "j1".into(),
            };
            let dict_obj = event_to_dict(py, event);
            let dict = dict_obj.downcast_bound::<PyDict>(py).unwrap();
            assert_eq!(
                dict.get_item("type")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "started"
            );
            assert_eq!(
                dict.get_item("job_id")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "j1"
            );
        });
    }

    #[test]
    fn event_to_dict_task_complete() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let event = nika_sdk::Event::TaskComplete {
                job_id: "j1".into(),
                task_id: "step1".into(),
                duration_ms: 1200,
            };
            let dict_obj = event_to_dict(py, event);
            let dict = dict_obj.downcast_bound::<PyDict>(py).unwrap();
            assert_eq!(
                dict.get_item("type")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "task_complete"
            );
            assert_eq!(
                dict.get_item("duration_ms")
                    .unwrap()
                    .unwrap()
                    .extract::<u64>()
                    .unwrap(),
                1200
            );
        });
    }

    #[test]
    fn event_to_dict_all_variants() {
        pyo3::prepare_freethreaded_python();
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

        Python::with_gil(|py| {
            let types: Vec<String> = events
                .into_iter()
                .map(|e| {
                    let dict_obj = event_to_dict(py, e);
                    let dict = dict_obj.downcast_bound::<PyDict>(py).unwrap();
                    dict.get_item("type")
                        .unwrap()
                        .unwrap()
                        .extract::<String>()
                        .unwrap()
                })
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
        });
    }

    #[test]
    fn job_info_repr() {
        let info = JobInfo {
            job_id: "abc".into(),
            status: "running".into(),
            workflow: "test.nika.yaml".into(),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
        };
        assert!(info.__repr__().contains("abc"));
        assert!(info.__repr__().contains("running"));
    }

    #[test]
    fn job_result_repr() {
        let result = JobResult {
            job_id: "abc".into(),
            output: Some("done".into()),
        };
        assert!(result.__repr__().contains("abc"));
    }

    #[test]
    fn job_info_eq() {
        let a = JobInfo {
            job_id: "j1".into(),
            status: "running".into(),
            workflow: "test.nika.yaml".into(),
            created_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn job_result_eq() {
        let a = JobResult {
            job_id: "j1".into(),
            output: Some("done".into()),
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(
            a,
            JobResult {
                job_id: "j2".into(),
                output: None,
            }
        );
    }

    #[test]
    fn artifact_info_eq() {
        let a = ArtifactInfo {
            name: "report.md".into(),
            size: 4096,
            format: Some("markdown".into()),
            content_type: "text/markdown".into(),
            checksum: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn artifact_info_repr() {
        let info = ArtifactInfo {
            name: "report.md".into(),
            size: 4096,
            format: Some("markdown".into()),
            content_type: "text/markdown".into(),
            checksum: None,
        };
        assert!(info.__repr__().contains("report.md"));
        assert!(info.__repr__().contains("4096"));
    }
}
