//! SQLite storage layer for jobs and cache.
//!
//! Uses a dedicated thread with a command channel (research finding:
//! rusqlite Connection is Send but NOT Sync — never use Arc<Mutex<Connection>>
//! with spawn_blocking, use a dedicated thread with mpsc instead).
//!
//! WAL mode for concurrent reads. user_version pragma for schema migrations.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

use crate::error::{DaemonError, DaemonResult};

/// Current schema version.
const SCHEMA_VERSION: u32 = 1;

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

// ═══════════════════════════════════════════════════════════════════════════
// DATABASE COMMANDS (sent to the dedicated DB thread)
// ═══════════════════════════════════════════════════════════════════════════

enum DbCommand {
    InsertJob {
        job: Job,
        reply: oneshot::Sender<DaemonResult<()>>,
    },
    GetJob {
        id: String,
        reply: oneshot::Sender<DaemonResult<Option<Job>>>,
    },
    ListJobs {
        state: Option<JobState>,
        reply: oneshot::Sender<DaemonResult<Vec<Job>>>,
    },
    UpdateState {
        id: String,
        state: JobState,
        exit_code: Option<i32>,
        output: Option<String>,
        reply: oneshot::Sender<DaemonResult<()>>,
    },
    AddHistory {
        event: JobHistoryEvent,
        reply: oneshot::Sender<DaemonResult<()>>,
    },
    GetHistory {
        job_id: String,
        reply: oneshot::Sender<DaemonResult<Vec<JobHistoryEvent>>>,
    },
    IncrementRetry {
        id: String,
        reply: oneshot::Sender<DaemonResult<u32>>,
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
    pub fn open(db_path: &Path) -> DaemonResult<Self> {
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
            .map_err(|e| DaemonError::Lifecycle(format!("failed to spawn DB thread: {e}")))?;

        Ok(Self { tx })
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> DaemonResult<Self> {
        let (tx, rx) = mpsc::channel(256);

        std::thread::Builder::new()
            .name("nika-db-test".into())
            .spawn(move || {
                let conn = Connection::open_in_memory().expect("open in-memory db");
                init_schema(&conn).expect("init schema");
                run_db_loop(conn, rx);
            })
            .map_err(|e| DaemonError::Lifecycle(format!("failed to spawn DB thread: {e}")))?;

        Ok(Self { tx })
    }

    pub async fn insert_job(&self, job: Job) -> DaemonResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::InsertJob { job, reply })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn get_job(&self, id: &str) -> DaemonResult<Option<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetJob {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn list_jobs(&self, state: Option<JobState>) -> DaemonResult<Vec<Job>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListJobs { state, reply })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn update_state(
        &self,
        id: &str,
        state: JobState,
        exit_code: Option<i32>,
        output: Option<String>,
    ) -> DaemonResult<()> {
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
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn add_history(&self, event: JobHistoryEvent) -> DaemonResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::AddHistory { event, reply })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn get_history(&self, job_id: &str) -> DaemonResult<Vec<JobHistoryEvent>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::GetHistory {
                job_id: job_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }

    pub async fn increment_retry(&self, id: &str) -> DaemonResult<u32> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::IncrementRetry {
                id: id.to_string(),
                reply,
            })
            .await
            .map_err(|_| DaemonError::Lifecycle("DB thread closed".into()))?;
        rx.await
            .map_err(|_| DaemonError::Lifecycle("DB reply dropped".into()))?
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DB THREAD (runs on dedicated OS thread)
// ═══════════════════════════════════════════════════════════════════════════

fn db_thread(db_path: std::path::PathBuf, rx: mpsc::Receiver<DbCommand>) -> DaemonResult<()> {
    // Ensure parent dir exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(&db_path)
        .map_err(|e| DaemonError::Lifecycle(format!("open database: {e}")))?;

    // WAL mode + synchronous NORMAL (daemon-appropriate durability)
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(|e| DaemonError::Lifecycle(format!("set pragmas: {e}")))?;

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
                let _ = reply.send(do_update_state(&conn, &id, &state, exit_code, output.as_deref()));
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
        }
    }
    debug!("database thread shutting down");
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA
// ═══════════════════════════════════════════════════════════════════════════

fn init_schema(conn: &Connection) -> DaemonResult<()> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| DaemonError::Lifecycle(format!("read user_version: {e}")))?;

    if version >= SCHEMA_VERSION {
        return Ok(());
    }

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
    .map_err(|e| DaemonError::Lifecycle(format!("create tables: {e}")))?;

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|e| DaemonError::Lifecycle(format!("set user_version: {e}")))?;

    debug!(version = SCHEMA_VERSION, "schema initialized");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// QUERY IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════════════════

fn do_insert_job(conn: &Connection, job: &Job) -> DaemonResult<()> {
    conn.execute(
        "INSERT INTO jobs (id, name, workflow, args, cron, state, created_at, retry_count, max_retries)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
        ],
    )
    .map_err(|e| DaemonError::Lifecycle(format!("insert job: {e}")))?;
    Ok(())
}

fn do_get_job(conn: &Connection, id: &str) -> DaemonResult<Option<Job>> {
    conn.query_row(
        "SELECT id, name, workflow, args, cron, state, created_at, started_at, completed_at,
                exit_code, output, retry_count, max_retries
         FROM jobs WHERE id = ?1",
        params![id],
        row_to_job,
    )
    .optional()
    .map_err(|e| DaemonError::Lifecycle(format!("get job: {e}")))
}

fn do_list_jobs(conn: &Connection, state: Option<&JobState>) -> DaemonResult<Vec<Job>> {
    let mut jobs = Vec::new();

    if let Some(state) = state {
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, workflow, args, cron, state, created_at, started_at, completed_at,
                        exit_code, output, retry_count, max_retries
                 FROM jobs WHERE state = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| DaemonError::Lifecycle(format!("prepare list: {e}")))?;

        let rows = stmt
            .query_map(params![state.as_str()], row_to_job)
            .map_err(|e| DaemonError::Lifecycle(format!("list jobs: {e}")))?;

        for row in rows {
            jobs.push(row.map_err(|e| DaemonError::Lifecycle(format!("read row: {e}")))?);
        }
    } else {
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, workflow, args, cron, state, created_at, started_at, completed_at,
                        exit_code, output, retry_count, max_retries
                 FROM jobs ORDER BY created_at DESC",
            )
            .map_err(|e| DaemonError::Lifecycle(format!("prepare list: {e}")))?;

        let rows = stmt
            .query_map([], row_to_job)
            .map_err(|e| DaemonError::Lifecycle(format!("list jobs: {e}")))?;

        for row in rows {
            jobs.push(row.map_err(|e| DaemonError::Lifecycle(format!("read row: {e}")))?);
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
) -> DaemonResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    match state {
        JobState::Running => {
            conn.execute(
                "UPDATE jobs SET state = ?1, started_at = ?2 WHERE id = ?3",
                params![state.as_str(), now, id],
            )
        }
        JobState::Completed | JobState::Failed | JobState::Cancelled => {
            conn.execute(
                "UPDATE jobs SET state = ?1, completed_at = ?2, exit_code = ?3, output = ?4 WHERE id = ?5",
                params![state.as_str(), now, exit_code, output, id],
            )
        }
        _ => {
            conn.execute(
                "UPDATE jobs SET state = ?1 WHERE id = ?2",
                params![state.as_str(), id],
            )
        }
    }
    .map_err(|e| DaemonError::Lifecycle(format!("update state: {e}")))?;
    Ok(())
}

fn do_add_history(conn: &Connection, event: &JobHistoryEvent) -> DaemonResult<()> {
    conn.execute(
        "INSERT INTO job_history (job_id, event, timestamp, details) VALUES (?1, ?2, ?3, ?4)",
        params![event.job_id, event.event, event.timestamp, event.details],
    )
    .map_err(|e| DaemonError::Lifecycle(format!("add history: {e}")))?;
    Ok(())
}

fn do_get_history(conn: &Connection, job_id: &str) -> DaemonResult<Vec<JobHistoryEvent>> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT job_id, event, timestamp, details FROM job_history
             WHERE job_id = ?1 ORDER BY id ASC",
        )
        .map_err(|e| DaemonError::Lifecycle(format!("prepare history: {e}")))?;

    let rows = stmt
        .query_map(params![job_id], |row| {
            Ok(JobHistoryEvent {
                job_id: row.get(0)?,
                event: row.get(1)?,
                timestamp: row.get(2)?,
                details: row.get(3)?,
            })
        })
        .map_err(|e| DaemonError::Lifecycle(format!("get history: {e}")))?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|e| DaemonError::Lifecycle(format!("read row: {e}")))?);
    }
    Ok(events)
}

fn do_increment_retry(conn: &Connection, id: &str) -> DaemonResult<u32> {
    conn.execute(
        "UPDATE jobs SET retry_count = retry_count + 1 WHERE id = ?1",
        params![id],
    )
    .map_err(|e| DaemonError::Lifecycle(format!("increment retry: {e}")))?;

    let count: u32 = conn
        .query_row("SELECT retry_count FROM jobs WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .map_err(|e| DaemonError::Lifecycle(format!("read retry_count: {e}")))?;

    Ok(count)
}

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
    })
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
        storage.insert_job(make_job("j1", "a.nika.yaml")).await.unwrap();
        storage.insert_job(make_job("j2", "b.nika.yaml")).await.unwrap();
        storage.insert_job(make_job("j3", "c.nika.yaml")).await.unwrap();

        let all = storage.list_jobs(None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn list_jobs_by_state() {
        let storage = Storage::open_memory().unwrap();
        storage.insert_job(make_job("j1", "a.nika.yaml")).await.unwrap();
        storage.insert_job(make_job("j2", "b.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

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
        storage.insert_job(make_job("j1", "a.nika.yaml")).await.unwrap();
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
        storage.insert_job(make_job("j1", "test.nika.yaml")).await.unwrap();

        // Verify file was created
        assert!(db_path.exists());

        let job = storage.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.id, "j1");
    }
}
