//! SQLite state persistence for Jobs Daemon.
//!
//! Stores job execution history and statistics in a local SQLite database.

// SQLite Connection is not Sync, but we wrap it in RwLock for safe access
#![allow(clippy::arc_with_non_send_sync)]

use crate::jobs::error::JobsError;
use crate::jobs::JobsResult;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Execution status for a job run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobExecutionStatus {
    /// Job is queued for execution.
    Queued,
    /// Job is currently running.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed with an error.
    Failed,
    /// Job was cancelled.
    Cancelled,
}

impl JobExecutionStatus {
    /// Returns true if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobExecutionStatus::Completed
                | JobExecutionStatus::Failed
                | JobExecutionStatus::Cancelled
        )
    }

    /// Convert to string for SQLite storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            JobExecutionStatus::Queued => "queued",
            JobExecutionStatus::Running => "running",
            JobExecutionStatus::Completed => "completed",
            JobExecutionStatus::Failed => "failed",
            JobExecutionStatus::Cancelled => "cancelled",
        }
    }

    /// Parse from string representation.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(JobExecutionStatus::Queued),
            "running" => Some(JobExecutionStatus::Running),
            "completed" => Some(JobExecutionStatus::Completed),
            "failed" => Some(JobExecutionStatus::Failed),
            "cancelled" => Some(JobExecutionStatus::Cancelled),
            _ => None,
        }
    }
}

/// Record of a single job execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    /// Unique execution ID.
    pub id: String,
    /// Job name.
    pub job_name: String,
    /// Execution status.
    pub status: JobExecutionStatus,
    /// Trigger that started this execution.
    pub trigger: String,
    /// Start timestamp.
    pub started_at: DateTime<Utc>,
    /// End timestamp (if completed).
    pub ended_at: Option<DateTime<Utc>>,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Retry attempt number (0 = first attempt).
    pub attempt: u32,
    /// Output summary.
    pub output: Option<String>,
}

impl ExecutionRecord {
    /// Create a new execution record.
    pub fn new(id: String, job_name: String, trigger: String) -> Self {
        Self {
            id,
            job_name,
            trigger,
            status: JobExecutionStatus::Queued,
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: None,
            error: None,
            attempt: 0,
            output: None,
        }
    }

    /// Mark as running.
    pub fn mark_running(&mut self) {
        self.status = JobExecutionStatus::Running;
        self.started_at = Utc::now();
    }

    /// Mark as completed.
    pub fn mark_completed(&mut self, output: Option<String>) {
        self.status = JobExecutionStatus::Completed;
        self.ended_at = Some(Utc::now());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds().max(0) as u64);
        self.output = output;
    }

    /// Mark as failed.
    pub fn mark_failed(&mut self, error: String) {
        self.status = JobExecutionStatus::Failed;
        self.ended_at = Some(Utc::now());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds().max(0) as u64);
        self.error = Some(error);
    }

    /// Mark as cancelled.
    pub fn mark_cancelled(&mut self) {
        self.status = JobExecutionStatus::Cancelled;
        self.ended_at = Some(Utc::now());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds().max(0) as u64);
    }
}

/// Statistics for a job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobStats {
    /// Total number of executions.
    pub total_executions: u64,
    /// Number of successful executions.
    pub successful: u64,
    /// Number of failed executions.
    pub failed: u64,
    /// Number of cancelled executions.
    pub cancelled: u64,
    /// Average duration in milliseconds.
    pub avg_duration_ms: u64,
    /// Last execution timestamp.
    pub last_run: Option<DateTime<Utc>>,
    /// Last successful execution timestamp.
    pub last_success: Option<DateTime<Utc>>,
    /// Last failure timestamp.
    pub last_failure: Option<DateTime<Utc>>,
}

/// SQLite-backed state store.
pub struct StateStore {
    conn: Arc<RwLock<Connection>>,
}

impl StateStore {
    /// Create a new state store.
    pub fn new(path: &Path) -> JobsResult<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).map_err(|_e| JobsError::DatabaseOpenFailed {
            path: path.to_path_buf(),
        })?;

        let store = Self {
            conn: Arc::new(RwLock::new(conn)),
        };

        store.migrate()?;
        Ok(store)
    }

    /// Create an in-memory store (for testing).
    pub fn in_memory() -> JobsResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(RwLock::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Run database migrations.
    fn migrate(&self) -> JobsResult<()> {
        let conn = self.conn.write();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS executions (
                id TEXT PRIMARY KEY,
                job_name TEXT NOT NULL,
                status TEXT NOT NULL,
                trigger TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_ms INTEGER,
                error TEXT,
                attempt INTEGER NOT NULL DEFAULT 0,
                output TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_executions_job_name ON executions(job_name);
            CREATE INDEX IF NOT EXISTS idx_executions_status ON executions(status);
            CREATE INDEX IF NOT EXISTS idx_executions_started_at ON executions(started_at DESC);

            CREATE TABLE IF NOT EXISTS job_stats (
                job_name TEXT PRIMARY KEY,
                total_executions INTEGER NOT NULL DEFAULT 0,
                successful INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                cancelled INTEGER NOT NULL DEFAULT 0,
                avg_duration_ms INTEGER NOT NULL DEFAULT 0,
                last_run TEXT,
                last_success TEXT,
                last_failure TEXT
            );
            "#,
        )
        .map_err(|e| JobsError::MigrationError {
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Insert a new execution record.
    pub fn insert_execution(&self, record: &ExecutionRecord) -> JobsResult<()> {
        let conn = self.conn.write();
        conn.execute(
            r#"
            INSERT INTO executions (id, job_name, status, trigger, started_at, ended_at, duration_ms, error, attempt, output)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                record.id,
                record.job_name,
                record.status.as_str(),
                record.trigger,
                record.started_at.to_rfc3339(),
                record.ended_at.map(|t| t.to_rfc3339()),
                record.duration_ms,
                record.error,
                record.attempt,
                record.output,
            ],
        )?;

        Ok(())
    }

    /// Update an execution record.
    pub fn update_execution(&self, record: &ExecutionRecord) -> JobsResult<()> {
        let conn = self.conn.write();
        conn.execute(
            r#"
            UPDATE executions
            SET status = ?1, ended_at = ?2, duration_ms = ?3, error = ?4, output = ?5
            WHERE id = ?6
            "#,
            params![
                record.status.as_str(),
                record.ended_at.map(|t| t.to_rfc3339()),
                record.duration_ms,
                record.error,
                record.output,
                record.id,
            ],
        )?;

        // Update stats - pass the connection to avoid deadlock (RwLock is not reentrant)
        self.update_stats_with_conn(&conn, &record.job_name, record)?;

        Ok(())
    }

    /// Get an execution by ID.
    pub fn get_execution(&self, id: &str) -> JobsResult<Option<ExecutionRecord>> {
        let conn = self.conn.read();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, job_name, status, trigger, started_at, ended_at, duration_ms, error, attempt, output
            FROM executions
            WHERE id = ?1
            "#,
        )?;

        let record = stmt
            .query_row(params![id], |row| Ok(self.row_to_execution(row)))
            .optional()?
            .flatten();

        Ok(record)
    }

    /// List recent executions, optionally filtered by job name.
    pub fn list_executions(
        &self,
        job_name: Option<&str>,
        limit: usize,
    ) -> JobsResult<Vec<ExecutionRecord>> {
        let conn = self.conn.read();

        if let Some(name) = job_name {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, job_name, status, trigger, started_at, ended_at, duration_ms, error, attempt, output
                FROM executions
                WHERE job_name = ?1
                ORDER BY started_at DESC
                LIMIT ?2
                "#,
            )?;
            let records: Vec<ExecutionRecord> = stmt
                .query_map(params![name, limit as i64], |row| {
                    Ok(self.row_to_execution(row))
                })?
                .filter_map(|r| r.ok().flatten())
                .collect();
            Ok(records)
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, job_name, status, trigger, started_at, ended_at, duration_ms, error, attempt, output
                FROM executions
                ORDER BY started_at DESC
                LIMIT ?1
                "#,
            )?;
            let records: Vec<ExecutionRecord> = stmt
                .query_map(params![limit as i64], |row| Ok(self.row_to_execution(row)))?
                .filter_map(|r| r.ok().flatten())
                .collect();
            Ok(records)
        }
    }

    /// Get all running executions.
    pub fn get_running_executions(&self) -> JobsResult<Vec<ExecutionRecord>> {
        let conn = self.conn.read();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, job_name, status, trigger, started_at, ended_at, duration_ms, error, attempt, output
            FROM executions
            WHERE status = 'running'
            "#,
        )?;

        let records = stmt
            .query_map([], |row| Ok(self.row_to_execution(row)))?
            .filter_map(|r| r.ok().flatten())
            .collect();

        Ok(records)
    }

    /// Get stats for a job.
    pub fn get_stats(&self, job_name: &str) -> JobsResult<JobStats> {
        let conn = self.conn.read();
        let mut stmt = conn.prepare(
            r#"
            SELECT total_executions, successful, failed, cancelled, avg_duration_ms, last_run, last_success, last_failure
            FROM job_stats
            WHERE job_name = ?1
            "#,
        )?;

        let stats = stmt
            .query_row(params![job_name], |row| {
                Ok(JobStats {
                    total_executions: row.get::<_, i64>(0)? as u64,
                    successful: row.get::<_, i64>(1)? as u64,
                    failed: row.get::<_, i64>(2)? as u64,
                    cancelled: row.get::<_, i64>(3)? as u64,
                    avg_duration_ms: row.get::<_, i64>(4)? as u64,
                    last_run: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    last_success: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    last_failure: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                })
            })
            .optional()?
            .unwrap_or_default();

        Ok(stats)
    }

    /// Update stats after an execution (uses existing connection to avoid deadlock).
    fn update_stats_with_conn(
        &self,
        conn: &Connection,
        job_name: &str,
        record: &ExecutionRecord,
    ) -> JobsResult<()> {
        // Upsert stats row
        conn.execute(
            r#"
            INSERT INTO job_stats (job_name, total_executions, successful, failed, cancelled, avg_duration_ms, last_run, last_success, last_failure)
            VALUES (?1, 0, 0, 0, 0, 0, NULL, NULL, NULL)
            ON CONFLICT(job_name) DO NOTHING
            "#,
            params![job_name],
        )?;

        // Update based on status
        let now = Utc::now().to_rfc3339();
        match record.status {
            JobExecutionStatus::Completed => {
                conn.execute(
                    r#"
                    UPDATE job_stats
                    SET total_executions = total_executions + 1,
                        successful = successful + 1,
                        last_run = ?1,
                        last_success = ?1,
                        avg_duration_ms = (avg_duration_ms * successful + ?2) / (successful + 1)
                    WHERE job_name = ?3
                    "#,
                    params![now, record.duration_ms.unwrap_or(0) as i64, job_name],
                )?;
            }
            JobExecutionStatus::Failed => {
                conn.execute(
                    r#"
                    UPDATE job_stats
                    SET total_executions = total_executions + 1,
                        failed = failed + 1,
                        last_run = ?1,
                        last_failure = ?1
                    WHERE job_name = ?2
                    "#,
                    params![now, job_name],
                )?;
            }
            JobExecutionStatus::Cancelled => {
                conn.execute(
                    r#"
                    UPDATE job_stats
                    SET total_executions = total_executions + 1,
                        cancelled = cancelled + 1,
                        last_run = ?1
                    WHERE job_name = ?2
                    "#,
                    params![now, job_name],
                )?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Clean up old execution records.
    pub fn cleanup(&self, max_age: Duration) -> JobsResult<u64> {
        let conn = self.conn.write();
        let cutoff =
            (Utc::now() - chrono::Duration::from_std(max_age).unwrap_or_default()).to_rfc3339();

        let deleted = conn.execute(
            r#"
            DELETE FROM executions
            WHERE started_at < ?1 AND status IN ('completed', 'failed', 'cancelled')
            "#,
            params![cutoff],
        )? as u64;

        Ok(deleted)
    }

    /// Convert a SQLite row to ExecutionRecord.
    fn row_to_execution(&self, row: &rusqlite::Row) -> Option<ExecutionRecord> {
        Some(ExecutionRecord {
            id: row.get(0).ok()?,
            job_name: row.get(1).ok()?,
            status: JobExecutionStatus::parse(&row.get::<_, String>(2).ok()?)?,
            trigger: row.get(3).ok()?,
            started_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4).ok()?)
                .ok()?
                .with_timezone(&Utc),
            ended_at: row
                .get::<_, Option<String>>(5)
                .ok()?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            duration_ms: row.get::<_, Option<i64>>(6).ok()?.map(|v| v as u64),
            error: row.get(7).ok()?,
            attempt: row.get::<_, i64>(8).ok()? as u32,
            output: row.get(9).ok()?,
        })
    }
}

// Helper trait for optional query results
trait OptionalResult<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalResult<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_status_is_terminal() {
        assert!(!JobExecutionStatus::Queued.is_terminal());
        assert!(!JobExecutionStatus::Running.is_terminal());
        assert!(JobExecutionStatus::Completed.is_terminal());
        assert!(JobExecutionStatus::Failed.is_terminal());
        assert!(JobExecutionStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_execution_record_lifecycle() {
        let mut record = ExecutionRecord::new(
            "exec-1".to_string(),
            "test-job".to_string(),
            "manual".to_string(),
        );

        assert_eq!(record.status, JobExecutionStatus::Queued);
        assert!(record.ended_at.is_none());

        record.mark_running();
        assert_eq!(record.status, JobExecutionStatus::Running);

        record.mark_completed(Some("success".to_string()));
        assert_eq!(record.status, JobExecutionStatus::Completed);
        assert!(record.ended_at.is_some());
        assert!(record.duration_ms.is_some());
        assert_eq!(record.output, Some("success".to_string()));
    }

    #[test]
    fn test_execution_record_failure() {
        let mut record = ExecutionRecord::new(
            "exec-2".to_string(),
            "test-job".to_string(),
            "cron".to_string(),
        );

        record.mark_running();
        record.mark_failed("error occurred".to_string());

        assert_eq!(record.status, JobExecutionStatus::Failed);
        assert_eq!(record.error, Some("error occurred".to_string()));
    }

    #[test]
    fn test_state_store_in_memory() {
        let store = StateStore::in_memory().unwrap();

        let mut record = ExecutionRecord::new(
            "exec-1".to_string(),
            "test-job".to_string(),
            "manual".to_string(),
        );

        // Insert
        store.insert_execution(&record).unwrap();

        // Get
        let retrieved = store.get_execution("exec-1").unwrap().unwrap();
        assert_eq!(retrieved.id, "exec-1");
        assert_eq!(retrieved.job_name, "test-job");

        // Update
        record.mark_running();
        record.mark_completed(Some("done".to_string()));
        store.update_execution(&record).unwrap();

        let updated = store.get_execution("exec-1").unwrap().unwrap();
        assert_eq!(updated.status, JobExecutionStatus::Completed);
        assert_eq!(updated.output, Some("done".to_string()));
    }

    #[test]
    fn test_list_executions() {
        let store = StateStore::in_memory().unwrap();

        // Insert multiple executions
        for i in 0..5 {
            let mut record = ExecutionRecord::new(
                format!("exec-{}", i),
                "test-job".to_string(),
                "cron".to_string(),
            );
            record.mark_running();
            record.mark_completed(None);
            store.insert_execution(&record).unwrap();
        }

        let executions = store.list_executions(Some("test-job"), 3).unwrap();
        assert_eq!(executions.len(), 3);
    }

    #[test]
    fn test_job_stats() {
        let store = StateStore::in_memory().unwrap();

        // Create some executions
        let mut record1 = ExecutionRecord::new(
            "exec-1".to_string(),
            "stats-job".to_string(),
            "cron".to_string(),
        );
        record1.mark_running();
        record1.mark_completed(None);
        store.insert_execution(&record1).unwrap();
        store.update_execution(&record1).unwrap();

        let mut record2 = ExecutionRecord::new(
            "exec-2".to_string(),
            "stats-job".to_string(),
            "cron".to_string(),
        );
        record2.mark_running();
        record2.mark_failed("error".to_string());
        store.insert_execution(&record2).unwrap();
        store.update_execution(&record2).unwrap();

        let stats = store.get_stats("stats-job").unwrap();
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.failed, 1);
    }
}
