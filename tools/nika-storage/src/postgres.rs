//! PostgreSQL storage backend for multi-instance nika serve.
//!
//! Feature-gated behind `postgres`. Uses sqlx with a connection pool for
//! concurrent access across multiple nika-serve instances sharing one database.

use sqlx_core::pool::Pool;
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_postgres::PgPoolOptions;
use sqlx_postgres::Postgres;

use crate::*;

/// PostgreSQL schema — executed on connect to ensure tables exist.
const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS jobs (
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
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 0,
    tags TEXT
);
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow ON jobs(workflow);

CREATE TABLE IF NOT EXISTS job_history (
    id SERIAL PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    details TEXT
);
CREATE INDEX IF NOT EXISTS idx_history_job ON job_history(job_id);

CREATE TABLE IF NOT EXISTS job_artifacts (
    id SERIAL PRIMARY KEY,
    job_id TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    size BIGINT NOT NULL DEFAULT 0,
    format TEXT NOT NULL DEFAULT 'text',
    checksum TEXT,
    content_type TEXT NOT NULL DEFAULT 'text/plain'
);
CREATE INDEX IF NOT EXISTS idx_artifacts_job ON job_artifacts(job_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_artifacts_unique ON job_artifacts(job_id, name);

CREATE TABLE IF NOT EXISTS checkpoints (
    job_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    output TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (job_id, task_id)
);

CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    workflow TEXT NOT NULL,
    cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    paused BOOLEAN NOT NULL DEFAULT FALSE,
    source TEXT NOT NULL DEFAULT 'cli',
    overlap TEXT NOT NULL DEFAULT 'skip',
    inputs_json TEXT,
    last_run_at TEXT,
    next_run_at TEXT,
    run_count BIGINT NOT NULL DEFAULT 0,
    last_job_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS serve_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash BYTEA NOT NULL UNIQUE,
    role TEXT NOT NULL DEFAULT 'operator',
    scope TEXT NOT NULL DEFAULT '*',
    created_at TEXT NOT NULL,
    expires_at TEXT,
    last_used_at TEXT,
    revoked BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_tokens_hash ON serve_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_tokens_name ON serve_tokens(name);
"#;

/// PostgreSQL storage backend using sqlx connection pool.
#[derive(Clone)]
pub struct PostgresStorage {
    pool: Pool<Postgres>,
}

impl PostgresStorage {
    /// Connect to PostgreSQL and run schema migration.
    pub async fn connect(url: &str) -> StorageResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|e| StorageError::Other(format!("PostgreSQL connect: {e}")))?;

        // Run schema migration
        query(SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(|e| StorageError::Other(format!("PostgreSQL schema: {e}")))?;

        Ok(Self { pool })
    }

    // ═══════════════════════════════════════════════════════════════════
    // JOBS
    // ═══════════════════════════════════════════════════════════════════

    pub async fn insert_job(&self, job: Job) -> StorageResult<()> {
        query(
            "INSERT INTO jobs (id, name, workflow, args, cron, state, created_at, \
             started_at, completed_at, exit_code, output, retry_count, max_retries, tags) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(&job.id)
        .bind(&job.name)
        .bind(&job.workflow)
        .bind(&job.args)
        .bind(&job.cron)
        .bind(job.state.as_str())
        .bind(&job.created_at)
        .bind(&job.started_at)
        .bind(&job.completed_at)
        .bind(job.exit_code)
        .bind(&job.output)
        .bind(job.retry_count as i32)
        .bind(job.max_retries as i32)
        .bind(&job.tags)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("insert_job: {e}")))?;
        Ok(())
    }

    pub async fn get_job(&self, id: &str) -> StorageResult<Option<Job>> {
        let row = query(
            "SELECT id, name, workflow, args, cron, state, created_at, started_at, \
             completed_at, exit_code, output, retry_count, max_retries, tags \
             FROM jobs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_job: {e}")))?;

        Ok(row.as_ref().map(pg_row_to_job))
    }

    pub async fn list_jobs(&self, state: Option<&JobState>) -> StorageResult<Vec<Job>> {
        let rows = if let Some(state) = state {
            query(
                "SELECT id, name, workflow, args, cron, state, created_at, started_at, \
                 completed_at, exit_code, output, retry_count, max_retries, tags \
                 FROM jobs WHERE state = $1 ORDER BY created_at DESC LIMIT 1000",
            )
            .bind(state.as_str())
            .fetch_all(&self.pool)
            .await
        } else {
            query(
                "SELECT id, name, workflow, args, cron, state, created_at, started_at, \
                 completed_at, exit_code, output, retry_count, max_retries, tags \
                 FROM jobs ORDER BY created_at DESC LIMIT 1000",
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| StorageError::Other(format!("list_jobs: {e}")))?;

        Ok(rows.iter().map(pg_row_to_job).collect())
    }

    pub async fn update_state(
        &self,
        id: &str,
        state: &JobState,
        exit_code: Option<i32>,
        output: Option<&str>,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();

        match state {
            JobState::Running => {
                query("UPDATE jobs SET state = $1, started_at = $2 WHERE id = $3")
                    .bind(state.as_str())
                    .bind(&now)
                    .bind(id)
                    .execute(&self.pool)
                    .await
            }
            JobState::Completed | JobState::Failed | JobState::Cancelled => {
                query(
                    "UPDATE jobs SET state = $1, completed_at = $2, exit_code = $3, output = $4 \
                     WHERE id = $5 AND state IN ('pending', 'running')",
                )
                .bind(state.as_str())
                .bind(&now)
                .bind(exit_code)
                .bind(output)
                .bind(id)
                .execute(&self.pool)
                .await
            }
            _ => {
                query("UPDATE jobs SET state = $1 WHERE id = $2")
                    .bind(state.as_str())
                    .bind(id)
                    .execute(&self.pool)
                    .await
            }
        }
        .map_err(|e| StorageError::Other(format!("update_state: {e}")))?;

        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // HISTORY
    // ═══════════════════════════════════════════════════════════════════

    pub async fn add_history(&self, event: JobHistoryEvent) -> StorageResult<()> {
        query(
            "INSERT INTO job_history (job_id, event, timestamp, details) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&event.job_id)
        .bind(&event.event)
        .bind(&event.timestamp)
        .bind(&event.details)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("add_history: {e}")))?;
        Ok(())
    }

    pub async fn get_history(&self, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>> {
        let rows = query(
            "SELECT job_id, event, timestamp, details FROM job_history \
             WHERE job_id = $1 ORDER BY id ASC LIMIT 10000",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_history: {e}")))?;

        Ok(rows
            .iter()
            .map(|row| JobHistoryEvent {
                job_id: row.get("job_id"),
                event: row.get("event"),
                timestamp: row.get("timestamp"),
                details: row.get("details"),
            })
            .collect())
    }

    // ═══════════════════════════════════════════════════════════════════
    // RETRY
    // ═══════════════════════════════════════════════════════════════════

    pub async fn increment_retry(&self, id: &str) -> StorageResult<u32> {
        let row = query(
            "UPDATE jobs SET retry_count = retry_count + 1 WHERE id = $1 RETURNING retry_count",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("increment_retry: {e}")))?;

        let count: i32 = row.get("retry_count");
        Ok(count as u32)
    }

    // ═══════════════════════════════════════════════════════════════════
    // WORKFLOW LISTING
    // ═══════════════════════════════════════════════════════════════════

    pub async fn list_jobs_for_workflow(&self, workflow: &str) -> StorageResult<Vec<Job>> {
        let rows = query(
            "SELECT id, name, workflow, args, cron, state, created_at, started_at, \
             completed_at, exit_code, output, retry_count, max_retries, tags \
             FROM jobs WHERE workflow = $1 ORDER BY created_at DESC LIMIT 100",
        )
        .bind(workflow)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("list_jobs_for_workflow: {e}")))?;

        Ok(rows.iter().map(pg_row_to_job).collect())
    }

    pub async fn list_jobs_filtered(&self, filter: &JobFilter) -> StorageResult<Vec<Job>> {
        let mut sql = String::from(
            "SELECT id, name, workflow, args, cron, state, created_at, started_at, \
             completed_at, exit_code, output, retry_count, max_retries, tags \
             FROM jobs WHERE 1=1",
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(ref state) = filter.state {
            params.push(state.as_str().to_string());
            sql.push_str(&format!(" AND state = ${}", params.len()));
        }
        if let Some(ref workflow) = filter.workflow {
            params.push(workflow.clone());
            sql.push_str(&format!(" AND workflow = ${}", params.len()));
        }
        if let Some((ref key, ref val)) = filter.tag {
            // PostgreSQL JSON extraction: tags::jsonb->>key = val
            params.push(key.clone());
            params.push(val.clone());
            sql.push_str(&format!(
                " AND tags::jsonb->>${}::text = ${}",
                params.len() - 1,
                params.len()
            ));
        }

        let limit = filter.limit.unwrap_or(100).min(1000);
        let offset = filter.offset.unwrap_or(0);
        params.push(limit.to_string());
        let limit_idx = params.len();
        params.push(offset.to_string());
        let offset_idx = params.len();
        sql.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ${limit_idx}::bigint OFFSET ${offset_idx}::bigint"
        ));

        // Build the query dynamically — sqlx requires compile-time knowledge of
        // param count for the typed API, so we use raw query + manual binding.
        let mut query = query(&sql);
        for p in &params {
            query = query.bind(p);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("list_jobs_filtered: {e}")))?;

        Ok(rows.iter().map(pg_row_to_job).collect())
    }

    // ═══════════════════════════════════════════════════════════════════
    // MAINTENANCE
    // ═══════════════════════════════════════════════════════════════════

    pub async fn reset_stale_running(&self, reason: &str) -> StorageResult<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = query(
            "UPDATE jobs SET state = 'failed', output = $1, completed_at = $2 \
             WHERE state = 'running'",
        )
        .bind(reason)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("reset_stale_running: {e}")))?;

        Ok(result.rows_affected())
    }

    pub async fn delete_old_jobs(&self, max_age_secs: u64) -> StorageResult<u64> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        // Delete checkpoints for old terminal jobs first
        query(
            "DELETE FROM checkpoints WHERE job_id IN (\
             SELECT id FROM jobs \
             WHERE state IN ('completed', 'failed', 'cancelled') AND created_at < $1)",
        )
        .bind(&cutoff_str)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("delete_old_jobs checkpoints: {e}")))?;

        // Delete artifacts for old terminal jobs
        query(
            "DELETE FROM job_artifacts WHERE job_id IN (\
             SELECT id FROM jobs \
             WHERE state IN ('completed', 'failed', 'cancelled') AND created_at < $1)",
        )
        .bind(&cutoff_str)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("delete_old_jobs artifacts: {e}")))?;

        // Delete history for old terminal jobs (CASCADE handles this too but be explicit)
        query(
            "DELETE FROM job_history WHERE job_id IN (\
             SELECT id FROM jobs \
             WHERE state IN ('completed', 'failed', 'cancelled') AND created_at < $1)",
        )
        .bind(&cutoff_str)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("delete_old_jobs history: {e}")))?;

        // Delete the jobs themselves
        let result = query(
            "DELETE FROM jobs \
             WHERE state IN ('completed', 'failed', 'cancelled') AND created_at < $1",
        )
        .bind(&cutoff_str)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("delete_old_jobs: {e}")))?;

        Ok(result.rows_affected())
    }

    // ═══════════════════════════════════════════════════════════════════
    // ARTIFACTS
    // ═══════════════════════════════════════════════════════════════════

    pub async fn add_artifacts(
        &self,
        job_id: &str,
        artifacts: &[JobArtifact],
    ) -> StorageResult<()> {
        for a in artifacts {
            query(
                "INSERT INTO job_artifacts (job_id, name, path, size, format, checksum, content_type) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT (job_id, name) DO UPDATE SET \
                 path = EXCLUDED.path, size = EXCLUDED.size, format = EXCLUDED.format, \
                 checksum = EXCLUDED.checksum, content_type = EXCLUDED.content_type",
            )
            .bind(job_id)
            .bind(&a.name)
            .bind(&a.path)
            .bind(a.size as i64)
            .bind(&a.format)
            .bind(&a.checksum)
            .bind(&a.content_type)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("add_artifacts: {e}")))?;
        }
        Ok(())
    }

    pub async fn list_artifacts(&self, job_id: &str) -> StorageResult<Vec<JobArtifact>> {
        let rows = query(
            "SELECT job_id, name, path, size, format, checksum, content_type \
             FROM job_artifacts WHERE job_id = $1 ORDER BY name",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("list_artifacts: {e}")))?;

        Ok(rows
            .iter()
            .map(|row| JobArtifact {
                job_id: row.get("job_id"),
                name: row.get("name"),
                path: row.get("path"),
                size: row.get::<i64, _>("size") as u64,
                format: row.get("format"),
                checksum: row.get("checksum"),
                content_type: row.get("content_type"),
            })
            .collect())
    }

    // ═══════════════════════════════════════════════════════════════════
    // CHECKPOINTS
    // ═══════════════════════════════════════════════════════════════════

    pub async fn save_checkpoint(
        &self,
        job_id: &str,
        task_id: &str,
        output: &str,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        query(
            "INSERT INTO checkpoints (job_id, task_id, output, created_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (job_id, task_id) DO UPDATE SET \
             output = EXCLUDED.output, created_at = EXCLUDED.created_at",
        )
        .bind(job_id)
        .bind(task_id)
        .bind(output)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("save_checkpoint: {e}")))?;
        Ok(())
    }

    pub async fn load_checkpoints(&self, job_id: &str) -> StorageResult<Vec<Checkpoint>> {
        let rows = query(
            "SELECT job_id, task_id, output, created_at \
             FROM checkpoints WHERE job_id = $1 ORDER BY created_at",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("load_checkpoints: {e}")))?;

        Ok(rows
            .iter()
            .map(|row| Checkpoint {
                job_id: row.get("job_id"),
                task_id: row.get("task_id"),
                output: row.get("output"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn delete_checkpoints(&self, job_id: &str) -> StorageResult<()> {
        query("DELETE FROM checkpoints WHERE job_id = $1")
            .bind(job_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("delete_checkpoints: {e}")))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // SCHEDULES
    // ═══════════════════════════════════════════════════════════════════

    pub async fn insert_schedule(&self, schedule: &CronSchedule) -> StorageResult<()> {
        query(
            "INSERT INTO schedules (id, name, workflow, cron_expr, timezone, paused, source, \
             overlap, inputs_json, last_run_at, next_run_at, run_count, last_job_id, \
             created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(&schedule.id)
        .bind(&schedule.name)
        .bind(&schedule.workflow)
        .bind(&schedule.cron_expr)
        .bind(&schedule.timezone)
        .bind(schedule.paused)
        .bind(&schedule.source)
        .bind(&schedule.overlap)
        .bind(&schedule.inputs_json)
        .bind(&schedule.last_run_at)
        .bind(&schedule.next_run_at)
        .bind(schedule.run_count as i64)
        .bind(&schedule.last_job_id)
        .bind(&schedule.created_at)
        .bind(&schedule.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("insert_schedule: {e}")))?;
        Ok(())
    }

    pub async fn get_schedule(&self, id: &str) -> StorageResult<Option<CronSchedule>> {
        let row = query(
            "SELECT id, name, workflow, cron_expr, timezone, paused, source, overlap, \
             inputs_json, last_run_at, next_run_at, run_count, last_job_id, created_at, \
             updated_at FROM schedules WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_schedule: {e}")))?;

        Ok(row.as_ref().map(pg_row_to_schedule))
    }

    pub async fn get_schedule_by_name(&self, name: &str) -> StorageResult<Option<CronSchedule>> {
        let row = query(
            "SELECT id, name, workflow, cron_expr, timezone, paused, source, overlap, \
             inputs_json, last_run_at, next_run_at, run_count, last_job_id, created_at, \
             updated_at FROM schedules WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_schedule_by_name: {e}")))?;

        Ok(row.as_ref().map(pg_row_to_schedule))
    }

    pub async fn list_schedules(&self, enabled_only: bool) -> StorageResult<Vec<CronSchedule>> {
        let rows = if enabled_only {
            query(
                "SELECT id, name, workflow, cron_expr, timezone, paused, source, overlap, \
                 inputs_json, last_run_at, next_run_at, run_count, last_job_id, created_at, \
                 updated_at FROM schedules WHERE paused = FALSE ORDER BY next_run_at ASC LIMIT 1000",
            )
            .fetch_all(&self.pool)
            .await
        } else {
            query(
                "SELECT id, name, workflow, cron_expr, timezone, paused, source, overlap, \
                 inputs_json, last_run_at, next_run_at, run_count, last_job_id, created_at, \
                 updated_at FROM schedules ORDER BY created_at DESC LIMIT 1000",
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| StorageError::Other(format!("list_schedules: {e}")))?;

        Ok(rows.iter().map(pg_row_to_schedule).collect())
    }

    pub async fn update_schedule_enabled(&self, id: &str, enabled: bool) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = query("UPDATE schedules SET paused = $1, updated_at = $2 WHERE id = $3")
            .bind(!enabled)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("update_schedule_enabled: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn delete_schedule(&self, id: &str) -> StorageResult<()> {
        let result = query("DELETE FROM schedules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("delete_schedule: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn update_schedule_after_fire(
        &self,
        id: &str,
        last_run_at: &str,
        next_run_at: Option<&str>,
        last_job_id: &str,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = query(
            "UPDATE schedules SET last_run_at = $1, next_run_at = $2, last_job_id = $3, \
             run_count = run_count + 1, updated_at = $4 \
             WHERE id = $5 AND (last_run_at IS NULL OR last_run_at < $1)",
        )
        .bind(last_run_at)
        .bind(next_run_at)
        .bind(last_job_id)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("update_schedule_after_fire: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn update_schedule_cron(
        &self,
        id: &str,
        cron_expr: &str,
        next_run_at: Option<&str>,
        paused: bool,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = query(
            "UPDATE schedules SET cron_expr = $1, next_run_at = $2, paused = $3, \
             updated_at = $4 WHERE id = $5",
        )
        .bind(cron_expr)
        .bind(next_run_at)
        .bind(paused)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("update_schedule_cron: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // TOKENS
    // ═══════════════════════════════════════════════════════════════════

    pub async fn insert_token(&self, entry: &TokenEntry) -> StorageResult<()> {
        query(
            "INSERT INTO serve_tokens (id, name, token_hash, role, scope, created_at, \
             expires_at, revoked) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&entry.id)
        .bind(&entry.name)
        .bind(&entry.token_hash)
        .bind(entry.role.as_str())
        .bind(&entry.scope)
        .bind(&entry.created_at)
        .bind(&entry.expires_at)
        .bind(entry.revoked)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("insert_token: {e}")))?;
        Ok(())
    }

    pub async fn get_token_by_hash(&self, hash: &[u8]) -> StorageResult<Option<TokenEntry>> {
        let row = query(
            "SELECT id, name, token_hash, role, scope, created_at, expires_at, \
             last_used_at, revoked FROM serve_tokens WHERE token_hash = $1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_token_by_hash: {e}")))?;

        Ok(row.as_ref().map(pg_row_to_token))
    }

    pub async fn get_token_by_name(&self, name: &str) -> StorageResult<Option<TokenEntry>> {
        let row = query(
            "SELECT id, name, token_hash, role, scope, created_at, expires_at, \
             last_used_at, revoked FROM serve_tokens WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("get_token_by_name: {e}")))?;

        Ok(row.as_ref().map(pg_row_to_token))
    }

    pub async fn list_tokens(&self) -> StorageResult<Vec<TokenEntry>> {
        let rows = query(
            "SELECT id, name, token_hash, role, scope, created_at, expires_at, \
             last_used_at, revoked FROM serve_tokens \
             WHERE revoked = FALSE ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("list_tokens: {e}")))?;

        Ok(rows.iter().map(pg_row_to_token).collect())
    }

    pub async fn revoke_token(&self, id: &str) -> StorageResult<()> {
        let result =
            query("UPDATE serve_tokens SET revoked = TRUE WHERE id = $1 AND revoked = FALSE")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Other(format!("revoke_token: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn touch_token(&self, id: &str) -> StorageResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        query("UPDATE serve_tokens SET last_used_at = $1 WHERE id = $2")
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("touch_token: {e}")))?;
        Ok(())
    }

    pub async fn count_tokens(&self) -> StorageResult<u64> {
        let row = query("SELECT COUNT(*) as cnt FROM serve_tokens WHERE revoked = FALSE")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("count_tokens: {e}")))?;

        let count: i64 = row.get("cnt");
        Ok(count as u64)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ROW MAPPERS
// ═══════════════════════════════════════════════════════════════════════════

fn pg_row_to_job(row: &sqlx_postgres::PgRow) -> Job {
    Job {
        id: row.get("id"),
        name: row.get("name"),
        workflow: row.get("workflow"),
        args: row.get("args"),
        cron: row.get("cron"),
        state: JobState::parse(row.get::<&str, _>("state")),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        exit_code: row.get("exit_code"),
        output: row.get("output"),
        retry_count: row.get::<i32, _>("retry_count") as u32,
        max_retries: row.get::<i32, _>("max_retries") as u32,
        tags: row.get("tags"),
    }
}

fn pg_row_to_schedule(row: &sqlx_postgres::PgRow) -> CronSchedule {
    CronSchedule {
        id: row.get("id"),
        name: row.get("name"),
        workflow: row.get("workflow"),
        cron_expr: row.get("cron_expr"),
        timezone: row.get("timezone"),
        paused: row.get("paused"),
        source: row.get("source"),
        overlap: row.get("overlap"),
        inputs_json: row.get("inputs_json"),
        last_run_at: row.get("last_run_at"),
        next_run_at: row.get("next_run_at"),
        run_count: row.get::<i64, _>("run_count") as u64,
        last_job_id: row.get("last_job_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn pg_row_to_token(row: &sqlx_postgres::PgRow) -> TokenEntry {
    TokenEntry {
        id: row.get("id"),
        name: row.get("name"),
        token_hash: row.get("token_hash"),
        role: Role::parse(row.get::<&str, _>("role")),
        scope: row.get("scope"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        last_used_at: row.get("last_used_at"),
        revoked: row.get("revoked"),
    }
}
