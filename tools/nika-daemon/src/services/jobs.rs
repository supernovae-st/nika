//! Job service — submit, execute, cancel, retry.
//!
//! Jobs execute by spawning `nika run <workflow> --json-output -y` as a child process.
//! The daemon captures stdout/stderr, tracks lifecycle, and stores results in SQLite.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::error::{DaemonError, DaemonResult};
use crate::storage::{Job, JobHistoryEvent, JobState, Storage};

/// Default job execution timeout: 1 hour.
const JOB_TIMEOUT: Duration = Duration::from_secs(3600);

/// Maximum concurrent jobs.
const MAX_CONCURRENT_JOBS: usize = 4;

/// The job service manages job lifecycle.
pub struct JobService {
    storage: Storage,
    /// Running job PIDs: job_id → child PID
    running: Arc<Mutex<HashMap<String, u32>>>,
}

impl JobService {
    /// Create a new job service.
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Submit a new job.
    pub async fn submit(
        &self,
        workflow: &str,
        name: Option<&str>,
        args: Option<&str>,
        cron: Option<&str>,
        max_retries: u32,
    ) -> DaemonResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let job = Job {
            id: id.clone(),
            name: name.map(|s| s.to_string()),
            workflow: workflow.to_string(),
            args: args.map(|s| s.to_string()),
            cron: cron.map(|s| s.to_string()),
            state: JobState::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
            retry_count: 0,
            max_retries,
        };

        self.storage.insert_job(job).await?;

        self.storage
            .add_history(JobHistoryEvent {
                job_id: id.clone(),
                event: "submitted".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: Some(format!("workflow: {workflow}")),
            })
            .await?;

        info!(job_id = %id, workflow, "job submitted");

        // Auto-start (capacity check is inside start_job, atomically)
        self.start_job(&id).await?;

        Ok(id)
    }

    /// Start executing a pending job.
    async fn start_job(&self, job_id: &str) -> DaemonResult<()> {
        // Atomically check capacity and reserve a slot (prevents concurrent over-spawning).
        // Uses placeholder PID=0 so capacity is claimed before any storage I/O.
        {
            let mut running = self.running.lock().await;
            if running.len() >= MAX_CONCURRENT_JOBS {
                return Ok(()); // At capacity — job stays pending
            }
            running.insert(job_id.to_string(), 0); // Reserve slot
        }

        let job = self
            .storage
            .get_job(job_id)
            .await?
            .ok_or_else(|| DaemonError::Protocol(format!("job not found: {job_id}")))?;

        if job.state != JobState::Pending {
            // Job was cancelled or already started — release the reserved slot
            self.running.lock().await.remove(job_id);
            return Ok(());
        }

        // Update state to Running
        self.storage
            .update_state(job_id, JobState::Running, None, None)
            .await?;

        self.storage
            .add_history(JobHistoryEvent {
                job_id: job_id.to_string(),
                event: "started".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: None,
            })
            .await?;

        // Validate workflow path (C1: prevent path traversal)
        let workflow_path = std::path::Path::new(&job.workflow);
        if workflow_path.extension().is_none_or(|e| e != "yaml") || !job.workflow.contains(".nika.")
        {
            return Err(DaemonError::Protocol(format!(
                "invalid workflow path: must end with .nika.yaml, got '{}'",
                job.workflow
            )));
        }
        // J1: Canonicalize and verify path is under current directory (prevent traversal)
        if let Ok(canonical) = workflow_path.canonicalize() {
            let cwd = std::env::current_dir().unwrap_or_default();
            if !canonical.starts_with(&cwd) {
                return Err(DaemonError::Protocol(format!(
                    "workflow path '{}' escapes working directory '{}'",
                    job.workflow,
                    cwd.display()
                )));
            }
        }

        // Spawn child process (H3: error on missing exe instead of fallback)
        let exe = std::env::current_exe()
            .map_err(|e| DaemonError::Lifecycle(format!("cannot find nika binary: {e}")))?;
        let mut cmd = Command::new(exe);
        cmd.args(["run", &job.workflow, "-y", "--no-interactive"]);

        // Add input args if present
        if job.args.is_some() {
            cmd.args(["--input-file", "-"]);
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        // J2: kill child on drop (prevents orphans on timeout or panic)
        cmd.kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| DaemonError::Lifecycle(format!("failed to spawn nika run: {e}")))?;

        let child_pid = child.id().unwrap_or(0);
        debug!(job_id, child_pid, "job process spawned");

        // Track the running process
        self.running
            .lock()
            .await
            .insert(job_id.to_string(), child_pid);

        // Spawn a task to wait for completion
        let storage = self.storage.clone();
        let running = Arc::clone(&self.running);
        let job_id = job_id.to_string();

        tokio::spawn(async move {
            let result = wait_for_child(child, &job_id).await;

            // Remove from running map
            running.lock().await.remove(&job_id);

            // L5 fix: check if job was cancelled before overwriting state
            let current = storage.get_job(&job_id).await.ok().flatten();
            if current
                .as_ref()
                .is_some_and(|j| j.state == JobState::Cancelled)
            {
                return; // Already cancelled — don't overwrite with Failed
            }

            match result {
                Ok((exit_code, output)) => {
                    let state = if exit_code == 0 {
                        JobState::Completed
                    } else {
                        JobState::Failed
                    };

                    let _ = storage
                        .update_state(
                            &job_id,
                            state.clone(),
                            Some(exit_code),
                            Some(output.clone()),
                        )
                        .await;

                    let _ = storage
                        .add_history(JobHistoryEvent {
                            job_id: job_id.clone(),
                            event: state.as_str().to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            details: Some(format!("exit_code: {exit_code}")),
                        })
                        .await;

                    if exit_code == 0 {
                        info!(job_id, "job completed successfully");
                    } else {
                        warn!(job_id, exit_code, "job failed");
                    }
                }
                Err(e) => {
                    error!(job_id, error = %e, "job execution error");
                    let _ = storage
                        .update_state(
                            &job_id,
                            JobState::Failed,
                            None,
                            Some(format!("execution error: {e}")),
                        )
                        .await;
                }
            }
        });

        Ok(())
    }

    /// Cancel a running job.
    pub async fn cancel(&self, job_id: &str) -> DaemonResult<()> {
        let mut running = self.running.lock().await;

        if let Some(&pid) = running.get(job_id) {
            // Kill the child process
            #[cfg(unix)]
            {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGTERM,
                );
            }

            running.remove(job_id);
        }

        self.storage
            .update_state(job_id, JobState::Cancelled, None, None)
            .await?;

        self.storage
            .add_history(JobHistoryEvent {
                job_id: job_id.to_string(),
                event: "cancelled".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: None,
            })
            .await?;

        info!(job_id, "job cancelled");
        Ok(())
    }

    /// Retry a failed job.
    pub async fn retry(&self, job_id: &str) -> DaemonResult<String> {
        let job = self
            .storage
            .get_job(job_id)
            .await?
            .ok_or_else(|| DaemonError::Protocol(format!("job not found: {job_id}")))?;

        if job.state != JobState::Failed && job.state != JobState::Cancelled {
            return Err(DaemonError::Protocol(format!(
                "can only retry failed/cancelled jobs, got {:?}",
                job.state
            )));
        }

        if job.max_retries > 0 && job.retry_count >= job.max_retries {
            return Err(DaemonError::Protocol(format!(
                "max retries ({}) exceeded",
                job.max_retries
            )));
        }

        // Increment retry counter and reset to pending
        self.storage.increment_retry(job_id).await?;
        self.storage
            .update_state(job_id, JobState::Pending, None, None)
            .await?;

        self.storage
            .add_history(JobHistoryEvent {
                job_id: job_id.to_string(),
                event: "retried".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: Some(format!("retry #{}", job.retry_count + 1)),
            })
            .await?;

        // Auto-start
        self.start_job(job_id).await?;

        Ok(job_id.to_string())
    }

    /// Get a job by ID (delegates to storage).
    pub async fn get_job(&self, id: &str) -> DaemonResult<Option<Job>> {
        self.storage.get_job(id).await
    }

    /// List jobs, optionally filtered by state.
    pub async fn list_jobs(&self, state: Option<JobState>) -> DaemonResult<Vec<Job>> {
        self.storage.list_jobs(state).await
    }

    /// Get job history.
    pub async fn get_history(&self, job_id: &str) -> DaemonResult<Vec<JobHistoryEvent>> {
        self.storage.get_history(job_id).await
    }

    /// Get count of running jobs.
    pub async fn running_count(&self) -> usize {
        self.running.lock().await.len()
    }
}

/// Wait for a child process to complete with timeout, capturing output.
/// H6 fix: 1-hour timeout prevents hung jobs from blocking the scheduler.
/// J2 fix: caller must set kill_on_drop(true) — child is killed when future drops on timeout.
async fn wait_for_child(child: tokio::process::Child, job_id: &str) -> DaemonResult<(i32, String)> {
    // H6: Wrap in timeout to prevent hung jobs
    // J2: child has kill_on_drop(true), so dropping the future kills the process
    let output = match tokio::time::timeout(JOB_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => {
            result.map_err(|e| DaemonError::Lifecycle(format!("wait for job {job_id}: {e}")))?
        }
        Err(_) => {
            return Err(DaemonError::Lifecycle(format!(
                "job {job_id} timed out after {}s (process killed)",
                JOB_TIMEOUT.as_secs()
            )));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);

    // Combine stdout + stderr
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.stderr.is_empty() {
        combined.push_str("\n--- stderr ---\n");
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
    }

    // H4 fix: Truncate to 10KB at a valid char boundary (not mid-UTF8)
    const MAX_OUTPUT: usize = 10 * 1024;
    if combined.len() > MAX_OUTPUT {
        // Find a valid char boundary near the target offset
        let start = combined.len() - MAX_OUTPUT;
        // Find next valid char boundary at or after `start`
        let safe_start = (start..combined.len())
            .find(|&i| combined.is_char_boundary(i))
            .unwrap_or(combined.len());
        combined = format!("...(truncated)...\n{}", &combined[safe_start..]);
    }

    Ok((exit_code, combined))
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> JobService {
        let storage = Storage::open_memory().unwrap();
        JobService::new(storage)
    }

    #[tokio::test]
    async fn submit_creates_pending_job() {
        let svc = setup().await;
        let id = svc
            .submit("test.nika.yaml", None, None, None, 0)
            .await
            .unwrap();

        let job = svc.get_job(&id).await.unwrap().unwrap();
        assert_eq!(job.workflow, "test.nika.yaml");
        // Job starts executing immediately (Running) since we're under capacity
        assert!(job.state == JobState::Pending || job.state == JobState::Running);
    }

    #[tokio::test]
    async fn submit_with_name_and_cron() {
        let svc = setup().await;
        let id = svc
            .submit(
                "daily.nika.yaml",
                Some("daily-report"),
                None,
                Some("0 0 * * *"),
                3,
            )
            .await
            .unwrap();

        let job = svc.get_job(&id).await.unwrap().unwrap();
        assert_eq!(job.name, Some("daily-report".into()));
        assert_eq!(job.cron, Some("0 0 * * *".into()));
        assert_eq!(job.max_retries, 3);
    }

    #[tokio::test]
    async fn list_jobs_returns_submitted() {
        let svc = setup().await;
        svc.submit("a.nika.yaml", None, None, None, 0)
            .await
            .unwrap();
        svc.submit("b.nika.yaml", None, None, None, 0)
            .await
            .unwrap();

        let jobs = svc.list_jobs(None).await.unwrap();
        assert_eq!(jobs.len(), 2);
    }

    #[tokio::test]
    async fn cancel_job() {
        let svc = setup().await;
        let id = svc
            .submit("test.nika.yaml", None, None, None, 0)
            .await
            .unwrap();

        // Cancel immediately
        svc.cancel(&id).await.unwrap();

        let job = svc.get_job(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Cancelled);

        let history = svc.get_history(&id).await.unwrap();
        assert!(history.iter().any(|h| h.event == "cancelled"));
    }

    #[tokio::test]
    async fn job_history_records_events() {
        let svc = setup().await;
        let id = svc
            .submit("test.nika.yaml", None, None, None, 0)
            .await
            .unwrap();

        let history = svc.get_history(&id).await.unwrap();
        assert!(history.iter().any(|h| h.event == "submitted"));
    }

    #[tokio::test]
    async fn running_count_tracks_jobs() {
        let svc = setup().await;
        assert_eq!(svc.running_count().await, 0);
    }
}
