//! SQLite storage layer for jobs and cache.
//!
//! Extracted from `nika-daemon` as a standalone crate so that any Nika binary
//! (`nika-serve`, `nika-daemon`, tests) can embed job persistence without
//! pulling in the full daemon dependency tree.
//!
//! Uses a dedicated OS thread with a command channel (research finding:
//! rusqlite `Connection` is `Send` but NOT `Sync` -- never use
//! `Arc<Mutex<Connection>>` with `spawn_blocking`, use a dedicated thread
//! with mpsc instead).
//!
//! WAL mode for concurrent reads. `user_version` pragma for schema migrations.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

/// Current schema version.
const SCHEMA_VERSION: u32 = 5;

// ═══════════════════════════════════════════════════════════════════════════
// ERROR TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Errors from the storage layer.
#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("job not found: {0}")]
    NotFound(String),

    #[error("storage error: {0}")]
    Other(String),
}

/// Result type alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

// ═══════════════════════════════════════════════════════════════════════════
// JOB TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// A job in the scheduler.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Job {
    pub id: String,
    pub name: Option<String>,
    pub workflow: String,
    pub args: Option<String>,
    pub cron: Option<String>,
    pub state: JobState,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    /// JSON-serialized key-value tags for job metadata (e.g. `{"env":"staging"}`).
    pub tags: Option<String>,
}

/// Job state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// A job history event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobHistoryEvent {
    pub job_id: String,
    pub event: String,
    pub timestamp: String,
    pub details: Option<String>,
}

/// A checkpoint: a task's output saved during execution for resume.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub job_id: String,
    pub task_id: String,
    pub output: String,
    pub created_at: String,
}

/// A persistent cron schedule — fires jobs on a recurring basis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CronSchedule {
    pub id: String,
    pub name: String,
    pub workflow: String,
    pub cron_expr: String,
    pub timezone: String,
    pub paused: bool,
    /// "cli" or "yaml" — how this schedule was created.
    pub source: String,
    /// "skip", "queue", or "replace" — overlap policy.
    pub overlap: String,
    pub inputs_json: Option<String>,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: u64,
    pub last_job_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Filter criteria for listing jobs.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub state: Option<JobState>,
    pub workflow: Option<String>,
    /// Filter by tag key=value (matches JSON `tags` column via LIKE).
    pub tag: Option<(String, String)>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// An artifact produced by a job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobArtifact {
    pub job_id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub format: String,
    pub checksum: Option<String>,
    pub content_type: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// DATABASE COMMANDS (sent to the dedicated DB thread)
// ═══════════════════════════════════════════════════════════════════════════

enum DbCommand {
    InsertJob {
        job: Job,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    GetJob {
        id: String,
        reply: oneshot::Sender<StorageResult<Option<Job>>>,
    },
    ListJobs {
        state: Option<JobState>,
        reply: oneshot::Sender<StorageResult<Vec<Job>>>,
    },
    UpdateState {
        id: String,
        state: JobState,
        exit_code: Option<i32>,
        output: Option<String>,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    AddHistory {
        event: JobHistoryEvent,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    GetHistory {
        job_id: String,
        reply: oneshot::Sender<StorageResult<Vec<JobHistoryEvent>>>,
    },
    IncrementRetry {
        id: String,
        reply: oneshot::Sender<StorageResult<u32>>,
    },
    ListJobsForWorkflow {
        workflow: String,
        reply: oneshot::Sender<StorageResult<Vec<Job>>>,
    },
    ListJobsFiltered {
        filter: JobFilter,
        reply: oneshot::Sender<StorageResult<Vec<Job>>>,
    },
    ResetStaleRunning {
        reason: String,
        reply: oneshot::Sender<StorageResult<u64>>,
    },
    DeleteOldJobs {
        max_age_secs: u64,
        reply: oneshot::Sender<StorageResult<u64>>,
    },
    AddArtifacts {
        job_id: String,
        artifacts: Vec<JobArtifact>,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    ListArtifacts {
        job_id: String,
        reply: oneshot::Sender<StorageResult<Vec<JobArtifact>>>,
    },
    SaveCheckpoint {
        job_id: String,
        task_id: String,
        output: String,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    LoadCheckpoints {
        job_id: String,
        reply: oneshot::Sender<StorageResult<Vec<Checkpoint>>>,
    },
    DeleteCheckpoints {
        job_id: String,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    // ── Schedules ──────────────────────────────────────────────────────
    InsertSchedule {
        schedule: CronSchedule,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    GetSchedule {
        id: String,
        reply: oneshot::Sender<StorageResult<Option<CronSchedule>>>,
    },
    GetScheduleByName {
        name: String,
        reply: oneshot::Sender<StorageResult<Option<CronSchedule>>>,
    },
    ListSchedules {
        enabled_only: bool,
        reply: oneshot::Sender<StorageResult<Vec<CronSchedule>>>,
    },
    UpdateScheduleEnabled {
        id: String,
        enabled: bool,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    DeleteSchedule {
        id: String,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    UpdateScheduleAfterFire {
        id: String,
        last_run_at: String,
        next_run_at: Option<String>,
        last_job_id: String,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    UpdateScheduleCron {
        id: String,
        cron_expr: String,
        next_run_at: Option<String>,
        paused: bool,
        reply: oneshot::Sender<StorageResult<()>>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// STORAGE HANDLE (async-safe, send commands to DB thread)
// ═══════════════════════════════════════════════════════════════════════════

/// Async handle to the storage layer.
///
/// All operations send commands to a dedicated DB thread via an mpsc channel.
/// This avoids blocking the tokio runtime with synchronous SQLite calls.
#[derive(Clone)]
pub struct Storage {
    tx: mpsc::Sender<DbCommand>,
}

impl Storage {
    /// Open (or create) a SQLite database at the given path.
    ///
    /// Spawns a dedicated OS thread for all database operations.
    pub fn open(db_path: &Path) -> StorageResult<Self> {
        let db_path = db_path.to_path_buf();
        let (tx, rx) = mpsc::channel(256);

        // Spawn dedicated DB thread
        std::thread::Builder::new()
            .name("nika-db".into())
            .spawn(move || {
                if let Err(e) = db_thread(db_path, rx) {
                    error!(error = %e, "database thread exited with error");
                }
            })
            .map_err(|e| StorageError::Other(format!("failed to spawn DB thread: {e}")))?;

        Ok(Self { tx })
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> StorageResult<Self> {
        let (tx, rx) = mpsc::channel(256);

        std::thread::Builder::new()
            .name("nika-db-test".into())
            .spawn(move || {
                let conn = Connection::open_in_memory().expect("open in-memory db");
                init_schema(&conn).expect("init schema");
                run_db_loop(conn, rx);
            })
            .map_err(|e| StorageError::Other(format!("failed to spawn DB thread: {e}")))?;

        Ok(Self { tx })
    }

    pub async fn insert_job(&self, job: Job) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::InsertJob { job, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn get_job(&self, id: &str) -> StorageResult<Option<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetJob {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn list_jobs(&self, state: Option<JobState>) -> StorageResult<Vec<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListJobs { state, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn update_state(
        &self,
        id: &str,
        state: JobState,
        exit_code: Option<i32>,
        output: Option<String>,
    ) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::UpdateState {
                id: id.to_string(),
                state,
                exit_code,
                output,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn add_history(&self, event: JobHistoryEvent) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::AddHistory { event, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn get_history(&self, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetHistory {
                job_id: job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn increment_retry(&self, id: &str) -> StorageResult<u32> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::IncrementRetry {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// List jobs for a specific workflow file, ordered by most recent first.
    pub async fn list_jobs_for_workflow(&self, workflow: &str) -> StorageResult<Vec<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListJobsForWorkflow {
                workflow: workflow.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    // ═══════════════════════════════════════════════════════════════════════
    // CONVENIENCE METHODS
    // ═══════════════════════════════════════════════════════════════════════

    /// List jobs with filtering (state, workflow, tag, limit, offset).
    pub async fn list_jobs_filtered(&self, filter: JobFilter) -> StorageResult<Vec<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListJobsFiltered { filter, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Create a new pending job.
    pub async fn create_job(&self, id: &str, workflow: &str) -> StorageResult<()> {
        self.create_job_with_tags(id, workflow, None).await
    }

    /// Create a new pending job with optional tags.
    pub async fn create_job_with_tags(
        &self,
        id: &str,
        workflow: &str,
        tags: Option<String>,
    ) -> StorageResult<()> {
        self.insert_job(Job {
            id: id.to_string(),
            name: None,
            workflow: workflow.to_string(),
            args: None,
            cron: None,
            state: JobState::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
            retry_count: 0,
            max_retries: 0,
            tags,
        })
        .await
    }

    /// Mark a job as completed with output.
    pub async fn complete_job(&self, id: &str, output: &str) -> StorageResult<()> {
        self.update_state(id, JobState::Completed, Some(0), Some(output.to_string()))
            .await
    }

    /// Mark a job as failed with error message.
    pub async fn fail_job(&self, id: &str, error: &str) -> StorageResult<()> {
        self.update_state(id, JobState::Failed, Some(1), Some(error.to_string()))
            .await
    }

    /// Reset stale "running" jobs to "failed" on server restart.
    ///
    /// Returns the number of jobs that were reset.
    pub async fn reset_stale_running(&self, reason: &str) -> StorageResult<u64> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ResetStaleRunning {
                reason: reason.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Store artifacts produced by a job.
    pub async fn add_artifacts(
        &self,
        job_id: &str,
        artifacts: Vec<JobArtifact>,
    ) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::AddArtifacts {
                job_id: job_id.to_string(),
                artifacts,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Save a task checkpoint (upsert).
    pub async fn save_checkpoint(
        &self,
        job_id: &str,
        task_id: &str,
        output: &str,
    ) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::SaveCheckpoint {
                job_id: job_id.to_string(),
                task_id: task_id.to_string(),
                output: output.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Load all checkpoints for a job.
    pub async fn load_checkpoints(&self, job_id: &str) -> StorageResult<Vec<Checkpoint>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::LoadCheckpoints {
                job_id: job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Delete all checkpoints for a job.
    pub async fn delete_checkpoints(&self, job_id: &str) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::DeleteCheckpoints {
                job_id: job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// List artifacts for a job.
    pub async fn list_artifacts(&self, job_id: &str) -> StorageResult<Vec<JobArtifact>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListArtifacts {
                job_id: job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Delete completed/failed/cancelled jobs older than `max_age_secs`.
    ///
    /// Also deletes associated job_history entries.
    /// Returns the number of jobs deleted.
    pub async fn delete_old_jobs(&self, max_age_secs: u64) -> StorageResult<u64> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::DeleteOldJobs {
                max_age_secs,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    // ═══════════════════════════════════════════════════════════════════
    // SCHEDULES
    // ═══════════════════════════════════════════════════════════════════

    pub async fn insert_schedule(&self, schedule: CronSchedule) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::InsertSchedule { schedule, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn get_schedule(&self, id: &str) -> StorageResult<Option<CronSchedule>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetSchedule {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn get_schedule_by_name(&self, name: &str) -> StorageResult<Option<CronSchedule>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetScheduleByName {
                name: name.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn list_schedules(&self, enabled_only: bool) -> StorageResult<Vec<CronSchedule>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListSchedules {
                enabled_only,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn update_schedule_enabled(&self, id: &str, enabled: bool) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::UpdateScheduleEnabled {
                id: id.to_string(),
                enabled,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn delete_schedule(&self, id: &str) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::DeleteSchedule {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    pub async fn update_schedule_after_fire(
        &self,
        id: &str,
        last_run_at: &str,
        next_run_at: Option<&str>,
        last_job_id: &str,
    ) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::UpdateScheduleAfterFire {
                id: id.to_string(),
                last_run_at: last_run_at.to_string(),
                next_run_at: next_run_at.map(|s| s.to_string()),
                last_job_id: last_job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Update a schedule's cron expression, next_run_at, and paused state.
    /// Used by serve reconciliation when YAML changes.
    pub async fn update_schedule_cron(
        &self,
        id: &str,
        cron_expr: &str,
        next_run_at: Option<&str>,
        paused: bool,
    ) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::UpdateScheduleCron {
                id: id.to_string(),
                cron_expr: cron_expr.to_string(),
                next_run_at: next_run_at.map(|s| s.to_string()),
                paused,
                reply,
            })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DB THREAD (runs on dedicated OS thread)
// ═══════════════════════════════════════════════════════════════════════════

fn db_thread(db_path: std::path::PathBuf, rx: mpsc::Receiver<DbCommand>) -> StorageResult<()> {
    // Ensure parent dir exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| StorageError::Other(format!("create db directory: {e}")))?;
    }

    let conn = Connection::open(&db_path)
        .map_err(|e| StorageError::Other(format!("open database: {e}")))?;

    // WAL mode + synchronous NORMAL (daemon-appropriate durability)
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(|e| StorageError::Other(format!("set pragmas: {e}")))?;

    init_schema(&conn)?;
    info!(path = %db_path.display(), "database opened");

    run_db_loop(conn, rx);
    Ok(())
}

fn run_db_loop(conn: Connection, mut rx: mpsc::Receiver<DbCommand>) {
    // Block the OS thread waiting for commands
    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            DbCommand::InsertJob { job, reply } => {
                let _ = reply.send(do_insert_job(&conn, &job));
            }
            DbCommand::GetJob { id, reply } => {
                let _ = reply.send(do_get_job(&conn, &id));
            }
            DbCommand::ListJobs { state, reply } => {
                let _ = reply.send(do_list_jobs(&conn, state.as_ref()));
            }
            DbCommand::UpdateState {
                id,
                state,
                exit_code,
                output,
                reply,
            } => {
                let _ = reply.send(do_update_state(
                    &conn,
                    &id,
                    &state,
                    exit_code,
                    output.as_deref(),
                ));
            }
            DbCommand::AddHistory { event, reply } => {
                let _ = reply.send(do_add_history(&conn, &event));
            }
            DbCommand::GetHistory { job_id, reply } => {
                let _ = reply.send(do_get_history(&conn, &job_id));
            }
            DbCommand::IncrementRetry { id, reply } => {
                let _ = reply.send(do_increment_retry(&conn, &id));
            }
            DbCommand::ListJobsForWorkflow { workflow, reply } => {
                let _ = reply.send(do_list_jobs_for_workflow(&conn, &workflow));
            }
            DbCommand::ListJobsFiltered { filter, reply } => {
                let _ = reply.send(do_list_jobs_filtered(&conn, &filter));
            }
            DbCommand::ResetStaleRunning { reason, reply } => {
                let _ = reply.send(do_reset_stale_running(&conn, &reason));
            }
            DbCommand::DeleteOldJobs {
                max_age_secs,
                reply,
            } => {
                let _ = reply.send(do_delete_old_jobs(&conn, max_age_secs));
            }
            DbCommand::AddArtifacts {
                job_id,
                artifacts,
                reply,
            } => {
                let _ = reply.send(do_add_artifacts(&conn, &job_id, &artifacts));
            }
            DbCommand::ListArtifacts { job_id, reply } => {
                let _ = reply.send(do_list_artifacts(&conn, &job_id));
            }
            DbCommand::SaveCheckpoint {
                job_id,
                task_id,
                output,
                reply,
            } => {
                let _ = reply.send(do_save_checkpoint(&conn, &job_id, &task_id, &output));
            }
            DbCommand::LoadCheckpoints { job_id, reply } => {
                let _ = reply.send(do_load_checkpoints(&conn, &job_id));
            }
            DbCommand::DeleteCheckpoints { job_id, reply } => {
                let _ = reply.send(do_delete_checkpoints(&conn, &job_id));
            }
            // ── Schedules ──────────────────────────────────────────────────
            DbCommand::InsertSchedule { schedule, reply } => {
                let _ = reply.send(do_insert_schedule(&conn, &schedule));
            }
            DbCommand::GetSchedule { id, reply } => {
                let _ = reply.send(do_get_schedule(&conn, &id));
            }
            DbCommand::GetScheduleByName { name, reply } => {
                let _ = reply.send(do_get_schedule_by_name(&conn, &name));
            }
            DbCommand::ListSchedules {
                enabled_only,
                reply,
            } => {
                let _ = reply.send(do_list_schedules(&conn, enabled_only));
            }
            DbCommand::UpdateScheduleEnabled { id, enabled, reply } => {
                let _ = reply.send(do_update_schedule_enabled(&conn, &id, enabled));
            }
            DbCommand::DeleteSchedule { id, reply } => {
                let _ = reply.send(do_delete_schedule(&conn, &id));
            }
            DbCommand::UpdateScheduleAfterFire {
                id,
                last_run_at,
                next_run_at,
                last_job_id,
                reply,
            } => {
                let _ = reply.send(do_update_schedule_after_fire(
                    &conn,
                    &id,
                    &last_run_at,
                    next_run_at.as_deref(),
                    &last_job_id,
                ));
            }
            DbCommand::UpdateScheduleCron {
                id,
                cron_expr,
                next_run_at,
                paused,
                reply,
            } => {
                let _ = reply.send(do_update_schedule_cron(
                    &conn,
                    &id,
                    &cron_expr,
                    next_run_at.as_deref(),
                    paused,
                ));
            }
        }
    }
    debug!("database thread shutting down");
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA
// ═══════════════════════════════════════════════════════════════════════════

fn init_schema(conn: &Connection) -> StorageResult<()> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| StorageError::Other(format!("read user_version: {e}")))?;

    if version >= SCHEMA_VERSION {
        return Ok(());
    }

    // V1: jobs + job_history
    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
            id TEXT PRIMARY KEY,
            name TEXT,
            workflow TEXT NOT NULL,
            args TEXT,
            cron TEXT,
            state TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            exit_code INTEGER,
            output TEXT,
            retry_count INTEGER DEFAULT 0,
            max_retries INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS job_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id TEXT NOT NULL REFERENCES jobs(id),
            event TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            details TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
        CREATE INDEX IF NOT EXISTS idx_job_history_job_id ON job_history(job_id);",
        )
        .map_err(|e| StorageError::Other(format!("create tables: {e}")))?;
    }

    // V2: job_artifacts
    if version < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS job_artifacts (
            job_id TEXT NOT NULL REFERENCES jobs(id),
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            format TEXT NOT NULL DEFAULT 'text',
            checksum TEXT,
            content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
            PRIMARY KEY (job_id, name)
        );

        CREATE INDEX IF NOT EXISTS idx_job_artifacts_job_id ON job_artifacts(job_id);",
        )
        .map_err(|e| StorageError::Other(format!("create job_artifacts table: {e}")))?;
    }

    // V3: checkpoints
    if version < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoints (
            job_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            output TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (job_id, task_id)
        );

        CREATE INDEX IF NOT EXISTS idx_checkpoints_job_id ON checkpoints(job_id);",
        )
        .map_err(|e| StorageError::Other(format!("create checkpoints table: {e}")))?;
    }

    // V4: tags column on jobs
    if version < 4 {
        conn.execute_batch("ALTER TABLE jobs ADD COLUMN tags TEXT;")
            .map_err(|e| StorageError::Other(format!("add tags column: {e}")))?;
    }

    // V5: schedules table (first-class cron entities)
    if version < 5 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                workflow TEXT NOT NULL,
                cron_expr TEXT NOT NULL,
                timezone TEXT NOT NULL DEFAULT 'UTC',
                paused INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'cli',
                overlap TEXT NOT NULL DEFAULT 'skip',
                inputs_json TEXT,
                last_run_at TEXT,
                next_run_at TEXT,
                run_count INTEGER NOT NULL DEFAULT 0,
                last_job_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_schedules_next_run ON schedules(next_run_at) WHERE paused = 0;
            CREATE INDEX IF NOT EXISTS idx_schedules_workflow ON schedules(workflow);",
        )
        .map_err(|e| StorageError::Other(format!("create schedules table: {e}")))?;
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|e| StorageError::Other(format!("set user_version: {e}")))?;

    debug!(version = SCHEMA_VERSION, "schema initialized");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// QUERY IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════════════════

fn do_insert_job(conn: &Connection, job: &Job) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO jobs (id, name, workflow, args, cron, state, created_at, retry_count, max_retries, tags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            job.id,
            job.name,
            job.workflow,
            job.args,
            job.cron,
            job.state.as_str(),
            job.created_at,
            job.retry_count,
            job.max_retries,
            job.tags,
        ],
    )?;
    Ok(())
}

fn do_get_job(conn: &Connection, id: &str) -> StorageResult<Option<Job>> {
    let sql = format!("SELECT {} FROM jobs WHERE id = ?1", JOB_COLUMNS);
    conn.query_row(&sql, params![id], row_to_job)
        .optional()
        .map_err(StorageError::from)
}

/// Maximum rows returned by list_jobs (prevents unbounded allocation on large tables).
const MAX_JOB_LIST: i64 = 1000;

fn do_list_jobs(conn: &Connection, state: Option<&JobState>) -> StorageResult<Vec<Job>> {
    let mut jobs = Vec::new();

    if let Some(state) = state {
        let sql = format!(
            "SELECT {} FROM jobs WHERE state = ?1 ORDER BY created_at DESC LIMIT ?2",
            JOB_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![state.as_str(), MAX_JOB_LIST], row_to_job)?;
        for row in rows {
            jobs.push(row?);
        }
    } else {
        let sql = format!(
            "SELECT {} FROM jobs ORDER BY created_at DESC LIMIT ?1",
            JOB_COLUMNS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![MAX_JOB_LIST], row_to_job)?;
        for row in rows {
            jobs.push(row?);
        }
    }

    Ok(jobs)
}

fn do_update_state(
    conn: &Connection,
    id: &str,
    state: &JobState,
    exit_code: Option<i32>,
    output: Option<&str>,
) -> StorageResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    match state {
        JobState::Running => {
            conn.execute(
                "UPDATE jobs SET state = ?1, started_at = ?2 WHERE id = ?3",
                params![state.as_str(), now, id],
            )?;
        }
        JobState::Completed | JobState::Failed | JobState::Cancelled => {
            conn.execute(
                "UPDATE jobs SET state = ?1, completed_at = ?2, exit_code = ?3, output = ?4 \
                 WHERE id = ?5 AND state IN ('pending', 'running')",
                params![state.as_str(), now, exit_code, output, id],
            )?;
        }
        _ => {
            conn.execute(
                "UPDATE jobs SET state = ?1 WHERE id = ?2",
                params![state.as_str(), id],
            )?;
        }
    }
    Ok(())
}

fn do_add_history(conn: &Connection, event: &JobHistoryEvent) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO job_history (job_id, event, timestamp, details) VALUES (?1, ?2, ?3, ?4)",
        params![event.job_id, event.event, event.timestamp, event.details],
    )?;
    Ok(())
}

fn do_get_history(conn: &Connection, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>> {
    let mut stmt = conn.prepare_cached(
        "SELECT job_id, event, timestamp, details FROM job_history
         WHERE job_id = ?1 ORDER BY id ASC LIMIT 10000",
    )?;

    let rows = stmt.query_map(params![job_id], |row| {
        Ok(JobHistoryEvent {
            job_id: row.get(0)?,
            event: row.get(1)?,
            timestamp: row.get(2)?,
            details: row.get(3)?,
        })
    })?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

fn do_increment_retry(conn: &Connection, id: &str) -> StorageResult<u32> {
    conn.execute(
        "UPDATE jobs SET retry_count = retry_count + 1 WHERE id = ?1",
        params![id],
    )?;

    let count: u32 = conn.query_row(
        "SELECT retry_count FROM jobs WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;

    Ok(count)
}

fn do_list_jobs_for_workflow(conn: &Connection, workflow: &str) -> StorageResult<Vec<Job>> {
    // Raised from LIMIT 10 to LIMIT 100 to prevent overlap detection blind spots
    // when history depth exceeds the limit. Also used by GetWorkflowHistory for
    // history dots, so we return all states (not just pending/running).
    let sql = format!(
        "SELECT {} FROM jobs WHERE workflow = ?1 ORDER BY created_at DESC LIMIT 100",
        JOB_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![workflow], row_to_job)?;

    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row?);
    }
    Ok(jobs)
}

fn do_list_jobs_filtered(conn: &Connection, filter: &JobFilter) -> StorageResult<Vec<Job>> {
    let mut conditions = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref state) = filter.state {
        conditions.push(format!("state = ?{}", param_values.len() + 1));
        param_values.push(Box::new(state.as_str().to_string()));
    }
    if let Some(ref workflow) = filter.workflow {
        conditions.push(format!("workflow = ?{}", param_values.len() + 1));
        param_values.push(Box::new(workflow.clone()));
    }
    if let Some((ref key, ref val)) = filter.tag {
        // Match JSON tag: tags column contains '{"key":"val"...}'
        // Use json_extract for exact match
        conditions.push(format!(
            "json_extract(tags, ?{}) = ?{}",
            param_values.len() + 1,
            param_values.len() + 2
        ));
        param_values.push(Box::new(format!("$.{key}")));
        param_values.push(Box::new(val.clone()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    let limit = filter.limit.unwrap_or(100).min(MAX_JOB_LIST);
    let offset = filter.offset.unwrap_or(0);

    let sql = format!(
        "SELECT {} FROM jobs{} ORDER BY created_at DESC LIMIT {} OFFSET {}",
        JOB_COLUMNS, where_clause, limit, offset
    );

    let params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_job)?;

    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row?);
    }
    Ok(jobs)
}

fn do_reset_stale_running(conn: &Connection, reason: &str) -> StorageResult<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE jobs SET state = 'failed', output = ?1, completed_at = ?2 WHERE state = 'running'",
        params![reason, now],
    )?;
    Ok(affected as u64)
}

fn do_add_artifacts(
    conn: &Connection,
    job_id: &str,
    artifacts: &[JobArtifact],
) -> StorageResult<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT OR REPLACE INTO job_artifacts (job_id, name, path, size, format, checksum, content_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for a in artifacts {
        stmt.execute(params![
            job_id,
            a.name,
            a.path,
            a.size as i64,
            a.format,
            a.checksum,
            a.content_type,
        ])?;
    }
    Ok(())
}

fn do_list_artifacts(conn: &Connection, job_id: &str) -> StorageResult<Vec<JobArtifact>> {
    let mut stmt = conn.prepare_cached(
        "SELECT job_id, name, path, size, format, checksum, content_type
         FROM job_artifacts WHERE job_id = ?1 ORDER BY name",
    )?;
    let rows = stmt.query_map(params![job_id], |row| {
        Ok(JobArtifact {
            job_id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            size: row.get::<_, i64>(3)? as u64,
            format: row.get(4)?,
            checksum: row.get(5)?,
            content_type: row.get(6)?,
        })
    })?;
    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(row?);
    }
    Ok(artifacts)
}

fn do_save_checkpoint(
    conn: &Connection,
    job_id: &str,
    task_id: &str,
    output: &str,
) -> StorageResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO checkpoints (job_id, task_id, output, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![job_id, task_id, output, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn do_load_checkpoints(conn: &Connection, job_id: &str) -> StorageResult<Vec<Checkpoint>> {
    let mut stmt = conn.prepare_cached(
        "SELECT job_id, task_id, output, created_at
         FROM checkpoints WHERE job_id = ?1 ORDER BY created_at",
    )?;
    let rows = stmt.query_map(params![job_id], |row| {
        Ok(Checkpoint {
            job_id: row.get(0)?,
            task_id: row.get(1)?,
            output: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    let mut checkpoints = Vec::new();
    for row in rows {
        checkpoints.push(row?);
    }
    Ok(checkpoints)
}

fn do_delete_checkpoints(conn: &Connection, job_id: &str) -> StorageResult<()> {
    conn.execute("DELETE FROM checkpoints WHERE job_id = ?1", params![job_id])?;
    Ok(())
}

fn do_delete_old_jobs(conn: &Connection, max_age_secs: u64) -> StorageResult<u64> {
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
    let cutoff_str = cutoff.to_rfc3339();

    // Delete checkpoints for old terminal jobs first (FK-safe)
    conn.execute(
        "DELETE FROM checkpoints WHERE job_id IN (
            SELECT id FROM jobs
            WHERE state IN ('completed', 'failed', 'cancelled')
            AND created_at < ?1
        )",
        params![cutoff_str],
    )?;

    // Delete artifacts for old terminal jobs first (FK-safe)
    conn.execute(
        "DELETE FROM job_artifacts WHERE job_id IN (
            SELECT id FROM jobs
            WHERE state IN ('completed', 'failed', 'cancelled')
            AND created_at < ?1
        )",
        params![cutoff_str],
    )?;

    // Delete history entries for old terminal jobs (FK-safe)
    conn.execute(
        "DELETE FROM job_history WHERE job_id IN (
            SELECT id FROM jobs
            WHERE state IN ('completed', 'failed', 'cancelled')
            AND created_at < ?1
        )",
        params![cutoff_str],
    )?;

    // Delete the jobs themselves
    let deleted = conn.execute(
        "DELETE FROM jobs
         WHERE state IN ('completed', 'failed', 'cancelled')
         AND created_at < ?1",
        params![cutoff_str],
    )?;

    if deleted > 0 {
        info!(count = deleted, "garbage collected old jobs");
    }

    Ok(deleted as u64)
}

/// Column list for all SELECT queries — keep in sync with row_to_job.
const JOB_COLUMNS: &str = "id, name, workflow, args, cron, state, created_at, started_at, completed_at, exit_code, output, retry_count, max_retries, tags";

fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        name: row.get(1)?,
        workflow: row.get(2)?,
        args: row.get(3)?,
        cron: row.get(4)?,
        state: JobState::parse(&row.get::<_, String>(5)?),
        created_at: row.get(6)?,
        started_at: row.get(7)?,
        completed_at: row.get(8)?,
        exit_code: row.get(9)?,
        output: row.get(10)?,
        retry_count: row.get(11)?,
        max_retries: row.get(12)?,
        tags: row.get(13)?,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEDULE QUERIES
// ═══════════════════════════════════════════════════════════════════════════

const SCHEDULE_COLUMNS: &str = "id, name, workflow, cron_expr, timezone, paused, source, \
    overlap, inputs_json, last_run_at, next_run_at, run_count, last_job_id, created_at, \
    updated_at";

fn row_to_schedule(row: &rusqlite::Row) -> rusqlite::Result<CronSchedule> {
    Ok(CronSchedule {
        id: row.get(0)?,
        name: row.get(1)?,
        workflow: row.get(2)?,
        cron_expr: row.get(3)?,
        timezone: row.get(4)?,
        paused: row.get::<_, i32>(5)? != 0,
        source: row.get(6)?,
        overlap: row.get(7)?,
        inputs_json: row.get(8)?,
        last_run_at: row.get(9)?,
        next_run_at: row.get(10)?,
        run_count: row.get::<_, i64>(11)? as u64,
        last_job_id: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn do_insert_schedule(conn: &Connection, sched: &CronSchedule) -> StorageResult<()> {
    conn.execute(
        &format!(
            "INSERT INTO schedules ({}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            SCHEDULE_COLUMNS
        ),
        params![
            sched.id,
            sched.name,
            sched.workflow,
            sched.cron_expr,
            sched.timezone,
            sched.paused as i32,
            sched.source,
            sched.overlap,
            sched.inputs_json,
            sched.last_run_at,
            sched.next_run_at,
            sched.run_count as i64,
            sched.last_job_id,
            sched.created_at,
            sched.updated_at,
        ],
    )?;
    Ok(())
}

fn do_get_schedule(conn: &Connection, id: &str) -> StorageResult<Option<CronSchedule>> {
    let sql = format!("SELECT {SCHEDULE_COLUMNS} FROM schedules WHERE id = ?1");
    conn.query_row(&sql, params![id], row_to_schedule)
        .optional()
        .map_err(StorageError::from)
}

fn do_get_schedule_by_name(conn: &Connection, name: &str) -> StorageResult<Option<CronSchedule>> {
    let sql = format!("SELECT {SCHEDULE_COLUMNS} FROM schedules WHERE name = ?1");
    conn.query_row(&sql, params![name], row_to_schedule)
        .optional()
        .map_err(StorageError::from)
}

fn do_list_schedules(conn: &Connection, enabled_only: bool) -> StorageResult<Vec<CronSchedule>> {
    let sql = if enabled_only {
        format!(
            "SELECT {SCHEDULE_COLUMNS} FROM schedules WHERE paused = 0 ORDER BY next_run_at ASC LIMIT 1000"
        )
    } else {
        format!("SELECT {SCHEDULE_COLUMNS} FROM schedules ORDER BY created_at DESC LIMIT 1000")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_schedule)?;
    let mut schedules = Vec::new();
    for row in rows {
        schedules.push(row?);
    }
    Ok(schedules)
}

fn do_update_schedule_enabled(conn: &Connection, id: &str, enabled: bool) -> StorageResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE schedules SET paused = ?1, updated_at = ?2 WHERE id = ?3",
        params![(!enabled) as i32, now, id],
    )?;
    if affected == 0 {
        return Err(StorageError::NotFound(id.to_string()));
    }
    Ok(())
}

fn do_delete_schedule(conn: &Connection, id: &str) -> StorageResult<()> {
    let affected = conn.execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(StorageError::NotFound(id.to_string()));
    }
    Ok(())
}

fn do_update_schedule_after_fire(
    conn: &Connection,
    id: &str,
    last_run_at: &str,
    next_run_at: Option<&str>,
    last_job_id: &str,
) -> StorageResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    // TOCTOU guard: only update if no concurrent tick already fired (last_run_at < ours)
    let affected = conn.execute(
        "UPDATE schedules SET last_run_at = ?1, next_run_at = ?2, last_job_id = ?3, \
         run_count = run_count + 1, updated_at = ?4 \
         WHERE id = ?5 AND (last_run_at IS NULL OR last_run_at < ?1)",
        params![last_run_at, next_run_at, last_job_id, now, id],
    )?;
    if affected == 0 {
        return Err(StorageError::NotFound(id.to_string()));
    }
    Ok(())
}

fn do_update_schedule_cron(
    conn: &Connection,
    id: &str,
    cron_expr: &str,
    next_run_at: Option<&str>,
    paused: bool,
) -> StorageResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE schedules SET cron_expr = ?1, next_run_at = ?2, paused = ?3, \
         updated_at = ?4 WHERE id = ?5",
        params![cron_expr, next_run_at, paused as i32, now, id],
    )?;
    if affected == 0 {
        return Err(StorageError::NotFound(id.to_string()));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job(id: &str, workflow: &str) -> Job {
        Job {
            id: id.to_string(),
            name: None,
            workflow: workflow.to_string(),
            args: None,
            cron: None,
            state: JobState::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            output: None,
            retry_count: 0,
            max_retries: 0,
            tags: None,
        }
    }

    #[tokio::test]
    async fn create_tables_idempotent() {
        let storage = Storage::open_memory().unwrap();
        // Opening twice should not fail (IF NOT EXISTS)
        let storage2 = Storage::open_memory().unwrap();
        drop(storage);
        drop(storage2);
    }

    #[tokio::test]
    async fn insert_and_get_job() {
        let storage = Storage::open_memory().unwrap();
        let job = make_job("job-1", "test.nika.yaml");
        storage.insert_job(job.clone()).await.unwrap();

        let fetched = storage.get_job("job-1").await.unwrap().unwrap();
        assert_eq!(fetched.id, "job-1");
        assert_eq!(fetched.workflow, "test.nika.yaml");
        assert_eq!(fetched.state, JobState::Pending);
    }

    #[tokio::test]
    async fn get_nonexistent_job_returns_none() {
        let storage = Storage::open_memory().unwrap();
        let result = storage.get_job("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_jobs_all() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "a.nika.yaml"))
            .await
            .unwrap();
        storage
            .insert_job(make_job("j2", "b.nika.yaml"))
            .await
            .unwrap();
        storage
            .insert_job(make_job("j3", "c.nika.yaml"))
            .await
            .unwrap();

        let all = storage.list_jobs(None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn list_jobs_by_state() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "a.nika.yaml"))
            .await
            .unwrap();
        storage
            .insert_job(make_job("j2", "b.nika.yaml"))
            .await
            .unwrap();

        // Update j1 to running
        storage
            .update_state("j1", JobState::Running, None, None)
            .await
            .unwrap();

        let pending = storage.list_jobs(Some(JobState::Pending)).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "j2");

        let running = storage.list_jobs(Some(JobState::Running)).await.unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "j1");
    }

    #[tokio::test]
    async fn update_job_state_running() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        storage
            .update_state("j1", JobState::Running, None, None)
            .await
            .unwrap();

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Running);
        assert!(job.started_at.is_some());
    }

    #[tokio::test]
    async fn update_job_state_completed() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        storage
            .update_state("j1", JobState::Completed, Some(0), Some("done".to_string()))
            .await
            .unwrap();

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert_eq!(job.output, Some("done".to_string()));
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn update_job_state_failed() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        storage
            .update_state("j1", JobState::Failed, Some(1), Some("error".to_string()))
            .await
            .unwrap();

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Failed);
        assert_eq!(job.exit_code, Some(1));
    }

    #[tokio::test]
    async fn job_history_tracking() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        storage
            .add_history(JobHistoryEvent {
                job_id: "j1".into(),
                event: "started".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: None,
            })
            .await
            .unwrap();

        storage
            .add_history(JobHistoryEvent {
                job_id: "j1".into(),
                event: "completed".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                details: Some("exit code 0".into()),
            })
            .await
            .unwrap();

        let history = storage.get_history("j1").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].event, "started");
        assert_eq!(history[1].event, "completed");
        assert_eq!(history[1].details, Some("exit code 0".into()));
    }

    #[tokio::test]
    async fn increment_retry_count() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        let count = storage.increment_retry("j1").await.unwrap();
        assert_eq!(count, 1);

        let count = storage.increment_retry("j1").await.unwrap();
        assert_eq!(count, 2);

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.retry_count, 2);
    }

    #[tokio::test]
    async fn insert_job_with_name_and_cron() {
        let storage = Storage::open_memory().unwrap();
        let mut job = make_job("j1", "daily.nika.yaml");
        job.name = Some("daily-report".into());
        job.cron = Some("0 0 * * *".into());
        job.max_retries = 3;

        storage.insert_job(job).await.unwrap();

        let fetched = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(fetched.name, Some("daily-report".into()));
        assert_eq!(fetched.cron, Some("0 0 * * *".into()));
        assert_eq!(fetched.max_retries, 3);
    }

    #[tokio::test]
    async fn insert_duplicate_job_fails() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "a.nika.yaml"))
            .await
            .unwrap();
        let result = storage.insert_job(make_job("j1", "b.nika.yaml")).await;
        assert!(result.is_err());
    }

    #[test]
    fn job_state_roundtrip() {
        for state in [
            JobState::Pending,
            JobState::Running,
            JobState::Completed,
            JobState::Failed,
            JobState::Cancelled,
        ] {
            assert_eq!(JobState::parse(state.as_str()), state);
        }
    }

    #[tokio::test]
    async fn storage_open_file_based() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = Storage::open(&db_path).unwrap();
        storage
            .insert_job(make_job("j1", "test.nika.yaml"))
            .await
            .unwrap();

        // Verify file was created
        assert!(db_path.exists());

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.id, "j1");
    }

    #[tokio::test]
    async fn list_jobs_for_workflow_returns_matching() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_job(make_job("j1", "a.nika.yaml"))
            .await
            .unwrap();
        storage
            .insert_job(make_job("j2", "a.nika.yaml"))
            .await
            .unwrap();
        storage
            .insert_job(make_job("j3", "b.nika.yaml"))
            .await
            .unwrap();

        let a_jobs = storage.list_jobs_for_workflow("a.nika.yaml").await.unwrap();
        assert_eq!(a_jobs.len(), 2);
        assert!(a_jobs.iter().all(|j| j.workflow == "a.nika.yaml"));

        let b_jobs = storage.list_jobs_for_workflow("b.nika.yaml").await.unwrap();
        assert_eq!(b_jobs.len(), 1);
        assert_eq!(b_jobs[0].id, "j3");

        let c_jobs = storage.list_jobs_for_workflow("c.nika.yaml").await.unwrap();
        assert!(c_jobs.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // CONVENIENCE METHOD TESTS
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn create_job_convenience() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("c1", "flow.nika.yaml").await.unwrap();

        let job = storage.get_job("c1").await.unwrap().unwrap();
        assert_eq!(job.id, "c1");
        assert_eq!(job.workflow, "flow.nika.yaml");
        assert_eq!(job.state, JobState::Pending);
        assert!(job.name.is_none());
    }

    #[tokio::test]
    async fn complete_job_convenience() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("c2", "flow.nika.yaml").await.unwrap();
        storage.complete_job("c2", "all good").await.unwrap();

        let job = storage.get_job("c2").await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert_eq!(job.output, Some("all good".to_string()));
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn fail_job_convenience() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("c3", "flow.nika.yaml").await.unwrap();
        storage.fail_job("c3", "something broke").await.unwrap();

        let job = storage.get_job("c3").await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Failed);
        assert_eq!(job.exit_code, Some(1));
        assert_eq!(job.output, Some("something broke".to_string()));
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn reset_stale_running_jobs() {
        let storage = Storage::open_memory().unwrap();

        // Create 3 jobs: 1 pending, 2 running
        storage.create_job("s1", "a.nika.yaml").await.unwrap();
        storage.create_job("s2", "b.nika.yaml").await.unwrap();
        storage.create_job("s3", "c.nika.yaml").await.unwrap();

        storage
            .update_state("s1", JobState::Running, None, None)
            .await
            .unwrap();
        storage
            .update_state("s2", JobState::Running, None, None)
            .await
            .unwrap();
        // s3 stays pending

        let affected = storage
            .reset_stale_running("server restarted")
            .await
            .unwrap();
        assert_eq!(affected, 2);

        // Both running jobs should now be failed
        let s1 = storage.get_job("s1").await.unwrap().unwrap();
        assert_eq!(s1.state, JobState::Failed);
        assert_eq!(s1.output, Some("server restarted".to_string()));
        assert!(s1.completed_at.is_some());

        let s2 = storage.get_job("s2").await.unwrap().unwrap();
        assert_eq!(s2.state, JobState::Failed);

        // Pending job should be untouched
        let s3 = storage.get_job("s3").await.unwrap().unwrap();
        assert_eq!(s3.state, JobState::Pending);
    }

    #[tokio::test]
    async fn reset_stale_running_with_no_running_jobs() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("x1", "a.nika.yaml").await.unwrap();

        let affected = storage
            .reset_stale_running("server restarted")
            .await
            .unwrap();
        assert_eq!(affected, 0);
    }

    #[tokio::test]
    async fn delete_old_jobs_removes_terminal_jobs() {
        let storage = Storage::open_memory().unwrap();

        // Create and complete some jobs
        storage.create_job("gc1", "a.nika.yaml").await.unwrap();
        storage.complete_job("gc1", "done").await.unwrap();
        storage.create_job("gc2", "b.nika.yaml").await.unwrap();
        storage.fail_job("gc2", "boom").await.unwrap();
        storage.create_job("gc3", "c.nika.yaml").await.unwrap(); // stays pending

        // Delete jobs older than 0 seconds (everything is "old")
        let deleted = storage.delete_old_jobs(0).await.unwrap();
        assert_eq!(
            deleted, 2,
            "should delete completed + failed but not pending"
        );

        // Verify pending job still exists
        let gc3 = storage.get_job("gc3").await.unwrap();
        assert!(gc3.is_some(), "pending job should survive GC");

        // Verify completed/failed jobs are gone
        let gc1 = storage.get_job("gc1").await.unwrap();
        assert!(gc1.is_none(), "completed job should be deleted");
    }

    #[tokio::test]
    async fn delete_old_jobs_respects_age() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("age1", "a.nika.yaml").await.unwrap();
        storage.complete_job("age1", "done").await.unwrap();

        // Delete only jobs older than 1 hour — newly created job should survive
        let deleted = storage.delete_old_jobs(3600).await.unwrap();
        assert_eq!(deleted, 0, "recent job should not be deleted");
    }

    #[tokio::test]
    async fn complete_job_does_not_overwrite_cancelled() {
        let storage = Storage::open_memory().unwrap();
        let id = "race-1";
        storage.create_job(id, "test.nika.yaml").await.unwrap();
        storage
            .update_state(id, JobState::Running, None, None)
            .await
            .unwrap();
        storage
            .update_state(id, JobState::Cancelled, None, Some("cancelled".into()))
            .await
            .unwrap();
        // Try to complete after cancel — must be no-op
        storage.complete_job(id, "late result").await.unwrap();
        let job = storage.get_job(id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Cancelled);
    }

    #[test]
    fn storage_error_display() {
        let err = StorageError::ChannelClosed;
        assert_eq!(err.to_string(), "channel closed");

        let err = StorageError::NotFound("job-42".into());
        assert_eq!(err.to_string(), "job not found: job-42");

        let err = StorageError::Other("disk full".into());
        assert_eq!(err.to_string(), "storage error: disk full");
    }

    // ── V4: tags + filtered listing ─────────────────────────────

    #[tokio::test]
    async fn create_job_with_tags_stores_and_retrieves() {
        let storage = Storage::open_memory().unwrap();
        let tags = serde_json::json!({"env": "staging", "team": "platform"}).to_string();
        storage
            .create_job_with_tags("j1", "test.nika.yaml", Some(tags.clone()))
            .await
            .unwrap();

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.tags, Some(tags));
    }

    #[tokio::test]
    async fn create_job_without_tags_is_null() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("j1", "test.nika.yaml").await.unwrap();

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert!(job.tags.is_none());
    }

    #[tokio::test]
    async fn list_jobs_filtered_by_state() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("j1", "a.nika.yaml").await.unwrap();
        storage.create_job("j2", "b.nika.yaml").await.unwrap();
        storage
            .update_state("j1", JobState::Running, None, None)
            .await
            .unwrap();

        let running = storage
            .list_jobs_filtered(JobFilter {
                state: Some(JobState::Running),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "j1");
    }

    #[tokio::test]
    async fn list_jobs_filtered_by_workflow() {
        let storage = Storage::open_memory().unwrap();
        storage.create_job("j1", "a.nika.yaml").await.unwrap();
        storage.create_job("j2", "b.nika.yaml").await.unwrap();
        storage.create_job("j3", "a.nika.yaml").await.unwrap();

        let a_jobs = storage
            .list_jobs_filtered(JobFilter {
                workflow: Some("a.nika.yaml".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(a_jobs.len(), 2);
    }

    #[tokio::test]
    async fn list_jobs_filtered_by_tag() {
        let storage = Storage::open_memory().unwrap();
        let tags_staging = serde_json::json!({"env": "staging"}).to_string();
        let tags_prod = serde_json::json!({"env": "prod"}).to_string();

        storage
            .create_job_with_tags("j1", "a.nika.yaml", Some(tags_staging))
            .await
            .unwrap();
        storage
            .create_job_with_tags("j2", "b.nika.yaml", Some(tags_prod))
            .await
            .unwrap();
        storage.create_job("j3", "c.nika.yaml").await.unwrap(); // no tags

        let staging = storage
            .list_jobs_filtered(JobFilter {
                tag: Some(("env".into(), "staging".into())),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(staging.len(), 1);
        assert_eq!(staging[0].id, "j1");
    }

    #[tokio::test]
    async fn list_jobs_filtered_with_limit_offset() {
        let storage = Storage::open_memory().unwrap();
        for i in 0..10 {
            storage
                .create_job(&format!("j{i}"), "test.nika.yaml")
                .await
                .unwrap();
        }

        let page = storage
            .list_jobs_filtered(JobFilter {
                limit: Some(3),
                offset: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.len(), 3);
    }

    #[tokio::test]
    async fn list_jobs_filtered_combined() {
        let storage = Storage::open_memory().unwrap();
        let tags = serde_json::json!({"env": "staging"}).to_string();

        storage
            .create_job_with_tags("j1", "a.nika.yaml", Some(tags.clone()))
            .await
            .unwrap();
        storage
            .create_job_with_tags("j2", "b.nika.yaml", Some(tags.clone()))
            .await
            .unwrap();
        storage
            .create_job_with_tags("j3", "a.nika.yaml", None)
            .await
            .unwrap();

        // Filter: workflow=a AND tag env=staging
        let results = storage
            .list_jobs_filtered(JobFilter {
                workflow: Some("a.nika.yaml".into()),
                tag: Some(("env".into(), "staging".into())),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "j1");
    }

    // ── Schedule tests ─────────────────────────────────────────────────

    fn make_schedule(id: &str, name: &str) -> CronSchedule {
        let now = chrono::Utc::now().to_rfc3339();
        CronSchedule {
            id: id.to_string(),
            name: name.to_string(),
            workflow: "test.nika.yaml".to_string(),
            cron_expr: "0 9 * * *".to_string(),
            timezone: "UTC".to_string(),
            paused: false,
            source: "cli".to_string(),
            overlap: "skip".to_string(),
            inputs_json: None,
            last_run_at: None,
            next_run_at: Some("2026-04-06T09:00:00+00:00".to_string()),
            run_count: 0,
            last_job_id: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn schedule_insert_and_get() {
        let storage = Storage::open_memory().unwrap();
        let sched = make_schedule("s1", "daily-report");

        storage.insert_schedule(sched.clone()).await.unwrap();

        let got = storage.get_schedule("s1").await.unwrap().unwrap();
        assert_eq!(got.id, "s1");
        assert_eq!(got.name, "daily-report");
        assert_eq!(got.cron_expr, "0 9 * * *");
        assert_eq!(got.timezone, "UTC");
        assert!(!got.paused);
        assert_eq!(got.run_count, 0);
    }

    #[tokio::test]
    async fn schedule_get_by_name() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_schedule(make_schedule("s1", "daily-report"))
            .await
            .unwrap();

        let got = storage
            .get_schedule_by_name("daily-report")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.id, "s1");

        let missing = storage.get_schedule_by_name("nope").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn schedule_list_ordered() {
        let storage = Storage::open_memory().unwrap();
        let mut s1 = make_schedule("s1", "alpha");
        s1.next_run_at = Some("2026-04-06T10:00:00+00:00".to_string());
        let mut s2 = make_schedule("s2", "beta");
        s2.next_run_at = Some("2026-04-06T08:00:00+00:00".to_string());

        storage.insert_schedule(s1).await.unwrap();
        storage.insert_schedule(s2).await.unwrap();

        // enabled_only=true → ordered by next_run_at ASC
        let list = storage.list_schedules(true).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "beta"); // 08:00 first
        assert_eq!(list[1].name, "alpha"); // 10:00 second
    }

    #[tokio::test]
    async fn schedule_update_enabled() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();

        // Pause
        storage.update_schedule_enabled("s1", false).await.unwrap();
        let got = storage.get_schedule("s1").await.unwrap().unwrap();
        assert!(got.paused);

        // enabled_only=true should exclude paused
        let active = storage.list_schedules(true).await.unwrap();
        assert!(active.is_empty());

        // Resume
        storage.update_schedule_enabled("s1", true).await.unwrap();
        let active = storage.list_schedules(true).await.unwrap();
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn schedule_delete() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();

        storage.delete_schedule("s1").await.unwrap();
        let got = storage.get_schedule("s1").await.unwrap();
        assert!(got.is_none());

        // Delete non-existent → NotFound
        let err = storage.delete_schedule("s1").await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn schedule_name_unique() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();

        // Same name, different ID → unique constraint violation
        let err = storage
            .insert_schedule(make_schedule("s2", "daily"))
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Sqlite(_)));
    }

    #[tokio::test]
    async fn schedule_update_after_fire() {
        let storage = Storage::open_memory().unwrap();
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();

        storage
            .update_schedule_after_fire(
                "s1",
                "2026-04-06T09:00:00+00:00",
                Some("2026-04-07T09:00:00+00:00"),
                "job-42",
            )
            .await
            .unwrap();

        let got = storage.get_schedule("s1").await.unwrap().unwrap();
        assert_eq!(got.run_count, 1);
        assert_eq!(got.last_job_id.as_deref(), Some("job-42"));
        assert_eq!(
            got.last_run_at.as_deref(),
            Some("2026-04-06T09:00:00+00:00")
        );
        assert_eq!(
            got.next_run_at.as_deref(),
            Some("2026-04-07T09:00:00+00:00")
        );
    }

    #[tokio::test]
    async fn v5_migration_coexists_with_jobs() {
        let storage = Storage::open_memory().unwrap();

        // Jobs still work (V4 tables intact)
        storage.create_job("j1", "test.nika.yaml").await.unwrap();
        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.workflow, "test.nika.yaml");

        // Schedules work (V5 table exists)
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();
        let sched = storage.get_schedule("s1").await.unwrap().unwrap();
        assert_eq!(sched.name, "daily");
    }

    #[tokio::test]
    async fn schedule_update_cron() {
        let storage = Storage::open_memory().unwrap();

        // Insert a schedule with initial cron and fire it once to set run_count=1
        storage
            .insert_schedule(make_schedule("s1", "daily"))
            .await
            .unwrap();
        storage
            .update_schedule_after_fire(
                "s1",
                "2026-04-06T09:00:00+00:00",
                Some("2026-04-07T09:00:00+00:00"),
                "job-1",
            )
            .await
            .unwrap();

        // Verify initial state: run_count = 1, cron = "0 9 * * *", paused = false
        let before = storage.get_schedule("s1").await.unwrap().unwrap();
        assert_eq!(before.run_count, 1);
        assert_eq!(before.cron_expr, "0 9 * * *");
        assert!(!before.paused);

        // Update cron expression, next_run_at, and pause the schedule
        storage
            .update_schedule_cron(
                "s1",
                "0 12 * * 1-5",
                Some("2026-04-07T12:00:00+00:00"),
                true,
            )
            .await
            .unwrap();

        // Verify: cron changed, paused changed, run_count preserved
        let after = storage.get_schedule("s1").await.unwrap().unwrap();
        assert_eq!(after.cron_expr, "0 12 * * 1-5", "cron should be updated");
        assert_eq!(
            after.next_run_at.as_deref(),
            Some("2026-04-07T12:00:00+00:00"),
            "next_run_at should be updated"
        );
        assert!(after.paused, "schedule should now be paused");
        assert_eq!(
            after.run_count, 1,
            "run_count should be preserved (not reset)"
        );
        assert_eq!(
            after.last_job_id.as_deref(),
            Some("job-1"),
            "last_job_id should be preserved"
        );
    }
}
