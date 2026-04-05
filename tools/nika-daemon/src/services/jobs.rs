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

/// Maximum pending jobs in the queue (prevents unbounded DB growth / disk exhaustion).
const MAX_PENDING_JOBS: usize = 1000;

/// The job service manages job lifecycle.
#[derive(Clone)]
pub struct JobService {
    storage: Storage,
    /// Running job PIDs: job_id → child PID
    running: Arc<Mutex<HashMap<String, u32>>>,
    /// Notify channel to trigger pending job drain after a slot frees up.
    drain_tx: Arc<tokio::sync::Notify>,
}

impl JobService {
    /// Create a new job service.
    pub fn new(storage: Storage) -> Self {
        let drain_tx = Arc::new(tokio::sync::Notify::new());
        let svc = Self {
            storage,
            running: Arc::new(Mutex::new(HashMap::new())),
            drain_tx,
        };
        // Spawn background drain loop that starts pending jobs when slots free up
        let drain_svc = svc.clone();
        tokio::spawn(async move {
            loop {
                drain_svc.drain_tx.notified().await;
                drain_svc.drain_pending_once().await;
            }
        });
        svc
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
        // Reject submissions when the queue is full to prevent disk exhaustion.
        let pending = self.storage.list_jobs(Some(JobState::Pending)).await?;
        if pending.len() >= MAX_PENDING_JOBS {
            return Err(DaemonError::Protocol(format!(
                "job queue full: {MAX_PENDING_JOBS} pending jobs — wait for jobs to complete"
            )));
        }

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
            tags: None,
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
        // VPS-01: fallback to persisted exe path when current_exe() fails (e.g. after upgrade)
        let exe = std::env::current_exe()
            .or_else(|_| {
                let stored = std::fs::read_to_string(crate::daemon_dir().join("nika-exe-path"))
                    .map_err(|e| std::io::Error::other(format!("no stored exe path: {e}")))?;
                let path = std::path::PathBuf::from(stored.trim());
                if path.exists() {
                    Ok(path)
                } else {
                    Err(std::io::Error::other(format!(
                        "stored exe path does not exist: {}",
                        path.display()
                    )))
                }
            })
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
        let drain_tx = Arc::clone(&self.drain_tx);

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
                // Signal drain loop to try starting next pending job
                drain_tx.notify_one();
                return; // Already cancelled — don't overwrite with Failed
            }

            match result {
                Ok((exit_code, output)) => {
                    let state = if exit_code == 0 {
                        JobState::Completed
                    } else {
                        JobState::Failed
                    };

                    if let Err(e) = storage
                        .update_state(
                            &job_id,
                            state.clone(),
                            Some(exit_code),
                            Some(output.clone()),
                        )
                        .await
                    {
                        warn!(job_id, error = %e, "failed to update job state");
                    }

                    if let Err(e) = storage
                        .add_history(JobHistoryEvent {
                            job_id: job_id.clone(),
                            event: state.as_str().to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            details: Some(format!("exit_code: {exit_code}")),
                        })
                        .await
                    {
                        warn!(job_id, error = %e, "failed to add job history");
                    }

                    if exit_code == 0 {
                        info!(job_id, "job completed successfully");
                    } else {
                        warn!(job_id, exit_code, "job failed");
                    }
                }
                Err(e) => {
                    error!(job_id, error = %e, "job execution error");
                    if let Err(e2) = storage
                        .update_state(
                            &job_id,
                            JobState::Failed,
                            None,
                            Some(format!("execution error: {e}")),
                        )
                        .await
                    {
                        warn!(job_id, error = %e2, "failed to persist job failure state");
                    }
                }
            }

            // Signal drain loop to try starting next pending job
            drain_tx.notify_one();
        });

        Ok(())
    }

    /// Cancel a running job.
    pub async fn cancel(&self, job_id: &str) -> DaemonResult<()> {
        // Kill the process and release the slot, then drop the lock before storage I/O.
        {
            let mut running = self.running.lock().await;
            if running.contains_key(job_id) {
                #[cfg(unix)]
                if let Some(pid) = running.get(job_id).copied() {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid as i32),
                        nix::sys::signal::Signal::SIGTERM,
                    );
                }
                running.remove(job_id);
            }
        } // lock released — storage calls below are outside the critical section

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
        Ok(self.storage.get_job(id).await?)
    }

    /// List jobs, optionally filtered by state.
    pub async fn list_jobs(&self, state: Option<JobState>) -> DaemonResult<Vec<Job>> {
        Ok(self.storage.list_jobs(state).await?)
    }

    /// Get job history.
    pub async fn get_history(&self, job_id: &str) -> DaemonResult<Vec<JobHistoryEvent>> {
        Ok(self.storage.get_history(job_id).await?)
    }

    /// Get count of running jobs.
    pub async fn running_count(&self) -> usize {
        self.running.lock().await.len()
    }

    /// List recent jobs for a specific workflow file.
    pub async fn list_jobs_for_workflow(&self, workflow: &str) -> DaemonResult<Vec<Job>> {
        Ok(self.storage.list_jobs_for_workflow(workflow).await?)
    }

    /// Access the underlying storage handle (for schedule operations).
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Try to start the next pending job from the queue.
    ///
    /// Called by the drain loop when a running slot frees up.
    async fn drain_pending_once(&self) {
        match self.storage.list_jobs(Some(JobState::Pending)).await {
            Ok(pending) => {
                if let Some(next) = pending.first() {
                    debug!(job_id = %next.id, "auto-starting next pending job");
                    if let Err(e) = self.start_job(&next.id).await {
                        warn!(job_id = %next.id, error = %e, "failed to auto-start pending job");
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to query pending jobs for drain");
            }
        }
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
// CRON SCHEDULER
// ═══════════════════════════════════════════════════════════════════════════

/// Background task: fires cron-scheduled jobs every minute.
///
/// Checks all stored cron expressions each minute and submits a new job
/// execution if the expression matches the current minute and no job for
/// that workflow is already pending or running.
pub async fn run_cron_scheduler(service: JobService) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    // Skip the first immediate tick so we don't double-fire jobs that were
    // just submitted at startup.
    interval.tick().await;

    loop {
        interval.tick().await;
        if let Err(e) = fire_due_cron_jobs(&service).await {
            warn!(error = %e, "cron scheduler tick failed");
        }
    }
}

async fn fire_due_cron_jobs(service: &JobService) -> DaemonResult<()> {
    // ── New path: fire from first-class schedules table (V5+) ────────
    let schedules = service.storage.list_schedules(true).await?;

    if !schedules.is_empty() {
        return fire_from_schedules_table(service, &schedules).await;
    }

    // ── Legacy fallback: scan jobs table for cron column ─────────────
    // Kept until all users migrate from `nika job submit --cron` to `nika every`.
    fire_from_jobs_table_legacy(service).await
}

/// Fire due jobs from first-class schedules (V5+).
async fn fire_from_schedules_table(
    service: &JobService,
    schedules: &[nika_storage::CronSchedule],
) -> DaemonResult<()> {
    let now = chrono::Utc::now();

    for sched in schedules {
        // Check if next_run_at has passed
        let is_due = match &sched.next_run_at {
            Some(next_str) => match chrono::DateTime::parse_from_rfc3339(next_str) {
                Ok(next) => next <= now,
                Err(_) => {
                    warn!(
                        schedule = %sched.name,
                        next_run_at = %next_str,
                        "invalid next_run_at — recomputing"
                    );
                    true
                }
            },
            None => true, // Not yet computed — treat as due to bootstrap
        };

        if !is_due {
            continue;
        }

        // Overlap protection: skip if a job for this workflow is pending/running
        if sched.overlap == "skip" {
            let active_jobs = service
                .storage
                .list_jobs_for_workflow(&sched.workflow)
                .await?;
            let already_active = active_jobs.iter().any(|j| {
                j.state == nika_storage::JobState::Pending
                    || j.state == nika_storage::JobState::Running
            });
            if already_active {
                debug!(
                    schedule = %sched.name,
                    workflow = %sched.workflow,
                    "skipping — active job exists (overlap: skip)"
                );
                continue;
            }
        }

        // Fire: submit a new job
        match service
            .submit(
                &sched.workflow,
                Some(&sched.name),
                None,
                Some(&sched.cron_expr),
                0,
            )
            .await
        {
            Ok(job_id) => {
                info!(
                    schedule = %sched.name,
                    job_id = %job_id,
                    workflow = %sched.workflow,
                    cron = %sched.cron_expr,
                    "schedule fired"
                );

                // Recompute next_run_at from NOW (prevents drift)
                let cron: croner::Cron = match sched.cron_expr.parse() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let next_run = cron
                    .find_next_occurrence(&now, false)
                    .ok()
                    .map(|dt| dt.to_rfc3339());

                if let Err(e) = service
                    .storage
                    .update_schedule_after_fire(
                        &sched.id,
                        &now.to_rfc3339(),
                        next_run.as_deref(),
                        &job_id,
                    )
                    .await
                {
                    warn!(
                        schedule = %sched.name,
                        error = %e,
                        "failed to update schedule after fire"
                    );
                }
            }
            Err(e) => {
                warn!(
                    schedule = %sched.name,
                    workflow = %sched.workflow,
                    error = %e,
                    "failed to fire scheduled job"
                );
            }
        }
    }

    Ok(())
}

/// Legacy fallback: scan jobs table for cron column.
async fn fire_from_jobs_table_legacy(service: &JobService) -> DaemonResult<()> {
    let all_jobs = service.storage.list_jobs(None).await?;

    let mut seen = std::collections::HashSet::new();
    let templates: Vec<_> = all_jobs
        .iter()
        .filter(|j| j.cron.is_some())
        .filter(|j| seen.insert((j.workflow.clone(), j.cron.clone().unwrap_or_default())))
        .collect();

    if templates.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now();
    let window_start = now - chrono::Duration::seconds(60);

    for job in templates {
        let cron_expr = job.cron.as_deref().unwrap_or_default();

        let already_active = all_jobs.iter().any(|j| {
            j.workflow == job.workflow
                && (j.state == JobState::Pending || j.state == JobState::Running)
        });
        if already_active {
            continue;
        }

        let cron: croner::Cron = match cron_expr.parse() {
            Ok(c) => c,
            Err(e) => {
                warn!(expr = %cron_expr, error = %e, "invalid cron expression — skipping");
                continue;
            }
        };

        let due = cron
            .find_next_occurrence(&window_start, false)
            .map(|next| next <= now)
            .unwrap_or(false);

        if due {
            match service
                .submit(
                    &job.workflow,
                    job.name.as_deref(),
                    job.args.as_deref(),
                    Some(cron_expr),
                    job.max_retries,
                )
                .await
            {
                Ok(id) => {
                    info!(job_id = %id, workflow = %job.workflow, cron = %cron_expr, "legacy cron job fired")
                }
                Err(e) => {
                    warn!(workflow = %job.workflow, cron = %cron_expr, error = %e, "failed to fire legacy cron job")
                }
            }
        }
    }

    Ok(())
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

    #[tokio::test]
    async fn cron_fire_due_jobs_fires_due_job() {
        let svc = setup().await;

        // Submit a cron job with "every minute" expression.
        svc.submit("cron.nika.yaml", Some("ticker"), None, Some("* * * * *"), 0)
            .await
            .unwrap();

        // Mark the initial job as completed so it is no longer pending/running.
        let jobs = svc.list_jobs(None).await.unwrap();
        let initial_id = &jobs[0].id;
        svc.storage
            .update_state(initial_id, JobState::Completed, Some(0), None)
            .await
            .unwrap();

        let before = svc.list_jobs(None).await.unwrap().len();
        fire_due_cron_jobs(&svc).await.unwrap();
        let after = svc.list_jobs(None).await.unwrap().len();

        // A new job should have been submitted (cron * * * * * fires every minute).
        assert_eq!(
            after,
            before + 1,
            "cron scheduler should have submitted a new job"
        );
    }

    #[tokio::test]
    async fn cron_fire_skips_when_already_active() {
        let svc = setup().await;

        // Submit a cron job that stays in Running state.
        let id = svc
            .submit("cron.nika.yaml", None, None, Some("* * * * *"), 0)
            .await
            .unwrap();

        // Force it to Running state (simulates an actively running job).
        svc.storage
            .update_state(&id, JobState::Running, None, None)
            .await
            .unwrap();
        svc.running.lock().await.insert(id.clone(), 0);

        let before = svc.list_jobs(None).await.unwrap().len();
        fire_due_cron_jobs(&svc).await.unwrap();
        let after = svc.list_jobs(None).await.unwrap().len();

        // Should NOT fire a new job — the existing one is still running.
        assert_eq!(after, before, "should not double-fire while job is running");
    }
}
