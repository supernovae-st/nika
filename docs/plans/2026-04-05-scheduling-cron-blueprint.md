# Scheduling / Cron — Enriched Implementation Blueprint (v0.72)

> **Status**: Ready for implementation
> **Depends on**: v0.71 (on_error) must ship first (uses V5 schema slot)
> **Estimate**: ~720 LOC, 5 phases, 3-4 hours
> **Base plan**: `docs/plans/2026-04-05-v071-post-launch-mega-handoff.md` FEATURE 2

---

## EXECUTIVE SUMMARY

The cron scheduler **already works** (`fire_due_cron_jobs` at `services/jobs.rs:486-554`).
But schedules are embedded as columns on `Job` rows, making them invisible, unmanageable,
and timezone-unaware. This blueprint promotes schedules to first-class entities with their
own table, CLI, protocol messages, and timezone support — while the existing scheduler
loop (`run_cron_scheduler`) keeps running with zero downtime.

---

## PHASE 1: Storage Layer (`nika-storage/src/lib.rs`)

### 1.1 CronSchedule Struct

Insert after `Checkpoint` struct (line ~120):

```rust
/// A persistent cron schedule — fires jobs on a recurring basis.
///
/// Schedules are independent of jobs: one schedule produces many jobs over time.
/// The `enabled` flag supports pause/resume without deleting the schedule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CronSchedule {
    /// Unique schedule ID (UUID v4).
    pub id: String,
    /// Human-readable name (optional, unique if set).
    pub name: Option<String>,
    /// Path to .nika.yaml workflow file (relative to project root).
    pub workflow: String,
    /// JSON-serialized workflow input args (optional).
    pub args: Option<String>,
    /// Cron expression (standard 5-field or @shortcut).
    /// Parsed by croner 3 at insertion time; invalid expressions are rejected.
    pub cron_expr: String,
    /// IANA timezone name (e.g. "Europe/Paris", "America/New_York").
    /// Defaults to "UTC". Validated against chrono-tz at insertion time.
    pub timezone: String,
    /// Whether this schedule is active. False = paused (no jobs fired).
    pub enabled: bool,
    /// Max retries for jobs spawned by this schedule.
    pub max_retries: u32,
    /// RFC 3339 timestamp of schedule creation.
    pub created_at: String,
    /// RFC 3339 timestamp of last successful job firing (None if never fired).
    pub last_run_at: Option<String>,
    /// RFC 3339 timestamp of the next computed firing time.
    /// Recomputed after each fire from `now` to avoid drift.
    pub next_run_at: Option<String>,
    /// Total number of jobs fired by this schedule.
    pub run_count: u64,
    /// ID of the most recently spawned job (for quick status lookup).
    pub last_job_id: Option<String>,
    /// JSON-serialized key-value tags (e.g. `{"env":"staging"}`).
    pub tags: Option<String>,
}
```

**Modification point**: `tools/nika-storage/src/lib.rs:120` (after `Checkpoint` struct)

### 1.2 V5 Migration SQL

Update `SCHEMA_VERSION` constant from 4 to 5 (line 21):

```rust
const SCHEMA_VERSION: u32 = 5;
```

Add V5 migration block inside `init_schema()` after the V4 block (line ~727, after `if version < 4 { ... }`):

```rust
    // V5: schedules table (first-class cron entities)
    if version < 5 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schedules (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE,
                workflow TEXT NOT NULL,
                args TEXT,
                cron_expr TEXT NOT NULL,
                timezone TEXT NOT NULL DEFAULT 'UTC',
                enabled INTEGER NOT NULL DEFAULT 1,
                max_retries INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                last_run_at TEXT,
                next_run_at TEXT,
                run_count INTEGER NOT NULL DEFAULT 0,
                last_job_id TEXT,
                tags TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_schedules_enabled ON schedules(enabled);
            CREATE INDEX IF NOT EXISTS idx_schedules_next_run ON schedules(next_run_at);
            CREATE INDEX IF NOT EXISTS idx_schedules_workflow ON schedules(workflow);",
        )
        .map_err(|e| StorageError::Other(format!("create schedules table: {e}")))?;
    }
```

**Note**: Column is `cron_expr` (not `cron`) to avoid confusion with the existing `jobs.cron`
column. The existing `jobs.cron` column stays — it is still written by `fire_due_cron_jobs`
when spawning jobs so `nika job list` shows which jobs were cron-triggered.

**V4 tags compatibility**: V5 adds a new table, does NOT touch the V4 `jobs.tags` column.
Existing databases upgrade cleanly: V1->V2->V3->V4->V5 in a single `init_schema()` pass.

**Modification points**:
- `tools/nika-storage/src/lib.rs:21` — `SCHEMA_VERSION: u32 = 4` -> `5`
- `tools/nika-storage/src/lib.rs:727` — insert V5 block after V4 block

### 1.3 Six DbCommand Variants

Add to the `DbCommand` enum (after `DeleteCheckpoints`, line ~219):

```rust
    // ── Schedules ──────────────────────────────────────────────────────
    InsertSchedule {
        schedule: CronSchedule,
        reply: oneshot::Sender<StorageResult<()>>,
    },
    GetSchedule {
        id: String,
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
```

**Modification point**: `tools/nika-storage/src/lib.rs:219` (after `DeleteCheckpoints` variant)

### 1.4 Six Storage Methods

Add to `impl Storage` (after `delete_checkpoints`, line ~503):

```rust
    // ═══════════════════════════════════════════════════════════════════
    // SCHEDULES
    // ═══════════════════════════════════════════════════════════════════

    /// Insert a new cron schedule.
    pub async fn insert_schedule(&self, schedule: CronSchedule) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::InsertSchedule { schedule, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Get a schedule by ID.
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

    /// List all schedules, optionally only enabled ones.
    pub async fn list_schedules(&self, enabled_only: bool) -> StorageResult<Vec<CronSchedule>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(DbCommand::ListSchedules { enabled_only, reply })
            .await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    /// Enable or disable a schedule (pause/resume).
    pub async fn update_schedule_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> StorageResult<()> {
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

    /// Delete a schedule by ID.
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

    /// Update schedule metadata after a successful job firing.
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
```

### 1.5 DbCommand Dispatch in `run_db_loop`

Add to the `match cmd { ... }` block in `run_db_loop` (after `DeleteCheckpoints` arm, line ~635):

```rust
            // ── Schedules ──────────────────────────────────────────────────
            DbCommand::InsertSchedule { schedule, reply } => {
                let _ = reply.send(do_insert_schedule(&conn, &schedule));
            }
            DbCommand::GetSchedule { id, reply } => {
                let _ = reply.send(do_get_schedule(&conn, &id));
            }
            DbCommand::ListSchedules { enabled_only, reply } => {
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
```

**Modification point**: `tools/nika-storage/src/lib.rs:635` (after `DeleteCheckpoints` arm)

### 1.6 Query Implementations

Add after `do_delete_old_jobs` (line ~1081):

```rust
// ═══════════════════════════════════════════════════════════════════════════
// SCHEDULE QUERIES
// ═══════════════════════════════════════════════════════════════════════════

const SCHEDULE_COLUMNS: &str = "id, name, workflow, args, cron_expr, timezone, enabled, \
    max_retries, created_at, last_run_at, next_run_at, run_count, last_job_id, tags";

fn row_to_schedule(row: &rusqlite::Row) -> rusqlite::Result<CronSchedule> {
    Ok(CronSchedule {
        id: row.get(0)?,
        name: row.get(1)?,
        workflow: row.get(2)?,
        args: row.get(3)?,
        cron_expr: row.get(4)?,
        timezone: row.get(5)?,
        enabled: row.get::<_, i32>(6)? != 0,
        max_retries: row.get(7)?,
        created_at: row.get(8)?,
        last_run_at: row.get(9)?,
        next_run_at: row.get(10)?,
        run_count: row.get::<_, i64>(11)? as u64,
        last_job_id: row.get(12)?,
        tags: row.get(13)?,
    })
}

fn do_insert_schedule(conn: &Connection, sched: &CronSchedule) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO schedules (id, name, workflow, args, cron_expr, timezone, enabled, \
         max_retries, created_at, last_run_at, next_run_at, run_count, last_job_id, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            sched.id,
            sched.name,
            sched.workflow,
            sched.args,
            sched.cron_expr,
            sched.timezone,
            sched.enabled as i32,
            sched.max_retries,
            sched.created_at,
            sched.last_run_at,
            sched.next_run_at,
            sched.run_count as i64,
            sched.last_job_id,
            sched.tags,
        ],
    )?;
    Ok(())
}

fn do_get_schedule(conn: &Connection, id: &str) -> StorageResult<Option<CronSchedule>> {
    let sql = format!("SELECT {} FROM schedules WHERE id = ?1", SCHEDULE_COLUMNS);
    conn.query_row(&sql, params![id], row_to_schedule)
        .optional()
        .map_err(StorageError::from)
}

fn do_list_schedules(conn: &Connection, enabled_only: bool) -> StorageResult<Vec<CronSchedule>> {
    let sql = if enabled_only {
        format!(
            "SELECT {} FROM schedules WHERE enabled = 1 ORDER BY created_at DESC LIMIT 1000",
            SCHEDULE_COLUMNS
        )
    } else {
        format!(
            "SELECT {} FROM schedules ORDER BY created_at DESC LIMIT 1000",
            SCHEDULE_COLUMNS
        )
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_schedule)?;
    let mut schedules = Vec::new();
    for row in rows {
        schedules.push(row?);
    }
    Ok(schedules)
}

fn do_update_schedule_enabled(
    conn: &Connection,
    id: &str,
    enabled: bool,
) -> StorageResult<()> {
    let affected = conn.execute(
        "UPDATE schedules SET enabled = ?1 WHERE id = ?2",
        params![enabled as i32, id],
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
    conn.execute(
        "UPDATE schedules SET last_run_at = ?1, next_run_at = ?2, last_job_id = ?3, \
         run_count = run_count + 1 WHERE id = ?4",
        params![last_run_at, next_run_at, last_job_id, id],
    )?;
    Ok(())
}
```

**Modification point**: `tools/nika-storage/src/lib.rs:1081` (after `do_delete_old_jobs`)

---

## PHASE 2: Daemon Protocol (`nika-daemon/src/protocol.rs`)

### 2.1 DaemonRequest Variants

Add to `DaemonRequest` enum after `JobHistory` (line ~93):

```rust
    // ── Schedules ────────────────────────────────────────────────────────
    /// Create a new cron schedule.
    ScheduleAdd {
        workflow: String,
        name: Option<String>,
        args: Option<String>,
        cron_expr: String,
        timezone: Option<String>,
        max_retries: Option<u32>,
        tags: Option<String>,
    },

    /// List all schedules.
    ScheduleList {
        /// If true, only return enabled schedules.
        enabled_only: bool,
    },

    /// Get a single schedule by ID.
    ScheduleGet { id: String },

    /// Remove (delete) a schedule.
    ScheduleRemove { id: String },

    /// Pause a schedule (set enabled=false).
    SchedulePause { id: String },

    /// Resume a schedule (set enabled=true).
    ScheduleResume { id: String },
```

**Modification point**: `tools/nika-daemon/src/protocol.rs:93` (after `JobHistory` variant)

### 2.2 DaemonRequest Debug Impl

Add to the `Debug` impl `match` block (after `JobHistory` arm, line ~192):

```rust
            Self::ScheduleAdd { workflow, .. } => write!(
                f,
                "DaemonRequest::ScheduleAdd {{ workflow: {workflow:?}, .. }}"
            ),
            Self::ScheduleList { .. } => write!(f, "DaemonRequest::ScheduleList"),
            Self::ScheduleGet { id } => {
                write!(f, "DaemonRequest::ScheduleGet {{ id: {id:?} }}")
            }
            Self::ScheduleRemove { id } => {
                write!(f, "DaemonRequest::ScheduleRemove {{ id: {id:?} }}")
            }
            Self::SchedulePause { id } => {
                write!(f, "DaemonRequest::SchedulePause {{ id: {id:?} }}")
            }
            Self::ScheduleResume { id } => {
                write!(f, "DaemonRequest::ScheduleResume {{ id: {id:?} }}")
            }
```

**Modification point**: `tools/nika-daemon/src/protocol.rs:192` (after `JobHistory` Debug arm)

### 2.3 DaemonResponse Variants

Add to `DaemonResponse` enum after `JobHistoryList` (line ~295):

```rust
    // ── Schedules ────────────────────────────────────────────────────────
    /// Schedule created successfully.
    ScheduleCreated { id: String },

    /// List of schedules.
    ScheduleList { schedules: Vec<serde_json::Value> },

    /// Single schedule details.
    ScheduleDetail { schedule: serde_json::Value },
```

**Modification point**: `tools/nika-daemon/src/protocol.rs:295` (after `JobHistoryList` variant)

### 2.4 DaemonResponse Debug Impl

Add to the `Debug` impl `match` block (after `JobHistoryList` tag, line ~366):

```rust
            Self::ScheduleCreated { .. } => "ScheduleCreated",
            Self::ScheduleList { .. } => "ScheduleList",
            Self::ScheduleDetail { .. } => "ScheduleDetail",
```

**Modification point**: `tools/nika-daemon/src/protocol.rs:366` (after `JobHistoryList` tag)

---

## PHASE 3: Server Dispatch (`nika-daemon/src/server.rs`)

### 3.1 Route Request

Add to `route_request()` after the `JobHistory` dispatch arm (line ~632):

```rust
        // ── Schedules ──────────────────────────────────────────────────
        DaemonRequest::ScheduleAdd {
            workflow,
            name,
            args,
            cron_expr,
            timezone,
            max_retries,
            tags,
        } => {
            // Validate cron expression at submission time (fail fast)
            if let Err(e) = cron_expr.parse::<croner::Cron>() {
                return DaemonResponse::Error {
                    code: "SCHED-001".into(),
                    message: format!("invalid cron expression '{}': {}", cron_expr, e),
                };
            }

            // Validate timezone via chrono-tz (if provided)
            let tz_str = timezone.as_deref().unwrap_or("UTC");
            let tz: chrono_tz::Tz = match tz_str.parse() {
                Ok(t) => t,
                Err(_) => {
                    return DaemonResponse::Error {
                        code: "SCHED-002".into(),
                        message: format!(
                            "invalid timezone '{}' — use IANA name (e.g. Europe/Paris)",
                            tz_str
                        ),
                    };
                }
            };

            // Compute initial next_run_at from now in the specified timezone
            let now_utc = chrono::Utc::now();
            let now_tz = now_utc.with_timezone(&tz);
            let cron: croner::Cron = cron_expr.parse().unwrap(); // already validated
            let next_run = cron
                .find_next_occurrence(&now_tz, false)
                .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339());

            let id = uuid::Uuid::new_v4().to_string();
            let schedule = nika_storage::CronSchedule {
                id: id.clone(),
                name,
                workflow,
                args,
                cron_expr,
                timezone: tz_str.to_string(),
                enabled: true,
                max_retries: max_retries.unwrap_or(0),
                created_at: now_utc.to_rfc3339(),
                last_run_at: None,
                next_run_at: next_run,
                run_count: 0,
                last_job_id: None,
                tags,
            };

            match state.job_service.storage().insert_schedule(schedule).await {
                Ok(()) => DaemonResponse::ScheduleCreated { id },
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-003".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ScheduleList { enabled_only } => {
            match state.job_service.storage().list_schedules(enabled_only).await {
                Ok(schedules) => {
                    let json: Vec<serde_json::Value> = schedules
                        .iter()
                        .filter_map(|s| serde_json::to_value(s).ok())
                        .collect();
                    DaemonResponse::ScheduleList { schedules: json }
                }
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-004".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ScheduleGet { id } => {
            match state.job_service.storage().get_schedule(&id).await {
                Ok(Some(sched)) => match serde_json::to_value(&sched) {
                    Ok(v) => DaemonResponse::ScheduleDetail { schedule: v },
                    Err(e) => DaemonResponse::Error {
                        code: "SCHED-004".into(),
                        message: format!("serialize: {e}"),
                    },
                },
                Ok(None) => DaemonResponse::Error {
                    code: "SCHED-005".into(),
                    message: format!("schedule not found: {id}"),
                },
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-004".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ScheduleRemove { id } => {
            match state.job_service.storage().delete_schedule(&id).await {
                Ok(()) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-006".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::SchedulePause { id } => {
            match state
                .job_service
                .storage()
                .update_schedule_enabled(&id, false)
                .await
            {
                Ok(()) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-007".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::ScheduleResume { id } => {
            match state
                .job_service
                .storage()
                .update_schedule_enabled(&id, true)
                .await
            {
                Ok(()) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Error {
                    code: "SCHED-007".into(),
                    message: e.to_string(),
                },
            }
        }
```

**Modification point**: `tools/nika-daemon/src/server.rs:632` (after `JobHistory` dispatch arm)

### 3.2 JobService Needs `storage()` Accessor

The server dispatch accesses `state.job_service.storage()`. Add a public accessor to
`JobService` (line ~376 in `services/jobs.rs`, after `get_history`):

```rust
    /// Access the underlying storage handle (for schedule operations).
    pub fn storage(&self) -> &Storage {
        &self.storage
    }
```

**Modification point**: `tools/nika-daemon/src/services/jobs.rs:376` (after `get_history`)

### 3.3 Dependency: `chrono-tz`

Add to workspace Cargo.toml:

```toml
# In tools/Cargo.toml [workspace.dependencies]
chrono-tz = "0.10"
```

Add to `nika-daemon/Cargo.toml`:

```toml
chrono-tz = { workspace = true }
```

**Modification points**:
- `tools/Cargo.toml:222` (near `croner = "3"`)
- `tools/nika-daemon/Cargo.toml:44` (near `croner`)

---

## PHASE 4: Refactored Cron Scheduler (`nika-daemon/src/services/jobs.rs`)

### 4.1 Transition Strategy

The existing `fire_due_cron_jobs` (lines 486-554) scans the `jobs` table for rows with
a non-null `cron` column. The new version reads the `schedules` table instead.

**Backward compatibility during transition**: the function first reads schedules. If the
schedules table is empty, it falls back to the old job-scanning behavior. This means:
- Existing daemon restarts after V5 migration keep working even if no schedules were
  created yet (cron jobs submitted via `nika job submit --cron` still fire).
- Once `nika schedule add` is used, the schedules table takes over.

### 4.2 Refactored `fire_due_cron_jobs`

Replace lines 486-554 entirely:

```rust
async fn fire_due_cron_jobs(service: &JobService) -> DaemonResult<()> {
    // ── New path: read from schedules table ──────────────────────────────
    let schedules = service.storage.list_schedules(true).await?;

    if !schedules.is_empty() {
        return fire_from_schedules_table(service, &schedules).await;
    }

    // ── Legacy fallback: scan jobs table for cron column ────────────────
    // Kept for backward compat until all users migrate to `nika schedule add`.
    fire_from_jobs_table_legacy(service).await
}

/// Fire due jobs based on first-class schedules (V5+).
async fn fire_from_schedules_table(
    service: &JobService,
    schedules: &[nika_storage::CronSchedule],
) -> DaemonResult<()> {
    let now_utc = chrono::Utc::now();

    for sched in schedules {
        // Parse timezone (validated at insertion, but defensive here)
        let tz: chrono_tz::Tz = sched.timezone.parse().unwrap_or(chrono_tz::UTC);
        let now_tz = now_utc.with_timezone(&tz);

        // Check if next_run_at has passed
        let is_due = match &sched.next_run_at {
            Some(next_str) => {
                // Parse the stored UTC timestamp
                match chrono::DateTime::parse_from_rfc3339(next_str) {
                    Ok(next) => next <= now_utc,
                    Err(_) => {
                        warn!(
                            schedule_id = %sched.id,
                            next_run_at = %next_str,
                            "invalid next_run_at timestamp — recomputing"
                        );
                        true // Force recompute
                    }
                }
            }
            None => {
                // next_run_at not yet computed — treat as due so we recompute
                true
            }
        };

        if !is_due {
            continue;
        }

        // Overlap protection: skip if a job for this workflow is pending/running
        let active_jobs = service.storage.list_jobs_for_workflow(&sched.workflow).await?;
        let already_active = active_jobs.iter().any(|j| {
            j.state == nika_storage::JobState::Pending
                || j.state == nika_storage::JobState::Running
        });
        if already_active {
            debug!(
                schedule_id = %sched.id,
                workflow = %sched.workflow,
                "skipping — active job exists"
            );
            continue;
        }

        // Fire: submit a new job
        match service
            .submit(
                &sched.workflow,
                sched.name.as_deref(),
                sched.args.as_deref(),
                Some(&sched.cron_expr),
                sched.max_retries,
            )
            .await
        {
            Ok(job_id) => {
                info!(
                    schedule_id = %sched.id,
                    job_id = %job_id,
                    workflow = %sched.workflow,
                    cron = %sched.cron_expr,
                    tz = %sched.timezone,
                    "cron schedule fired"
                );

                // Recompute next_run_at from NOW (not from the previous next_run_at)
                // This prevents drift and handles missed runs correctly.
                let cron: croner::Cron = match sched.cron_expr.parse() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let next_run = cron
                    .find_next_occurrence(&now_tz, false)
                    .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339());

                // Update schedule metadata
                if let Err(e) = service
                    .storage
                    .update_schedule_after_fire(
                        &sched.id,
                        &now_utc.to_rfc3339(),
                        next_run.as_deref(),
                        &job_id,
                    )
                    .await
                {
                    warn!(
                        schedule_id = %sched.id,
                        error = %e,
                        "failed to update schedule after fire"
                    );
                }
            }
            Err(e) => {
                warn!(
                    schedule_id = %sched.id,
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
/// This is the original fire_due_cron_jobs behavior before V5 schedules.
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
                && (j.state == nika_storage::JobState::Pending
                    || j.state == nika_storage::JobState::Running)
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
```

**Modification point**: `tools/nika-daemon/src/services/jobs.rs:486-554` (replace entire
`fire_due_cron_jobs` function)

### 4.3 chrono-tz Integration Detail

The key insight is that `croner::Cron::find_next_occurrence` accepts any
`chrono::DateTime<Tz>`. By computing `now.with_timezone(&tz)` we get timezone-aware
scheduling for free:

```rust
// croner 3 signature:
// fn find_next_occurrence<Tz: TimeZone>(&self, from: &DateTime<Tz>, inclusive: bool)
//     -> Option<DateTime<Tz>>

let tz: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
let now_paris = chrono::Utc::now().with_timezone(&tz);
let next = cron.find_next_occurrence(&now_paris, false);
// next is DateTime<chrono_tz::Tz> — convert back to UTC for storage:
let next_utc = next.map(|dt| dt.with_timezone(&chrono::Utc));
```

All timestamps stored in SQLite remain UTC (RFC 3339). The timezone is only used to
compute when the next fire should happen relative to the user's local time.

**DST handling**: `chrono-tz` handles DST transitions. A "0 3 * * *" Europe/Paris schedule
will fire at 03:00 Paris time whether that's UTC+1 (winter) or UTC+2 (summer).

---

## PHASE 5: CLI (`nika-cli/src/schedule.rs` + wiring)

### 5.1 ScheduleAction Enum (CREATE `tools/nika-cli/src/schedule.rs`)

```rust
//! `nika schedule` subcommand handler.
//!
//! Manages cron schedules via the daemon:
//! - `nika schedule add <workflow> --cron <expr>` — create a schedule
//! - `nika schedule list` — list all schedules
//! - `nika schedule get <id>` — show schedule details
//! - `nika schedule remove <id>` — delete a schedule
//! - `nika schedule pause <id>` — pause a schedule
//! - `nika schedule resume <id>` — resume a schedule

use clap::Subcommand;
use colored::Colorize;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

/// Schedule management actions.
#[derive(Subcommand)]
pub enum ScheduleAction {
    /// Create a new cron schedule for a workflow
    Add {
        /// Path to .nika.yaml workflow file
        workflow: String,

        /// Cron expression (e.g., "0 */6 * * *", "@daily", "@hourly")
        #[arg(long)]
        cron: String,

        /// Schedule name (optional, for display)
        #[arg(long)]
        name: Option<String>,

        /// IANA timezone (e.g., "Europe/Paris"). Defaults to UTC.
        #[arg(long, default_value = "UTC")]
        tz: String,

        /// Maximum retries for spawned jobs
        #[arg(long, default_value = "0")]
        max_retries: u32,

        /// JSON string of input args for the workflow
        #[arg(long)]
        args: Option<String>,
    },

    /// List all cron schedules
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show only enabled schedules
        #[arg(long)]
        enabled: bool,
    },

    /// Show schedule details
    Get {
        /// Schedule ID (or unique prefix)
        id: String,
    },

    /// Remove a schedule (does NOT cancel running jobs)
    Remove {
        /// Schedule ID
        id: String,
    },

    /// Pause a schedule (disable without deleting)
    Pause {
        /// Schedule ID
        id: String,
    },

    /// Resume a paused schedule
    Resume {
        /// Schedule ID
        id: String,
    },
}

pub async fn handle_schedule_command(
    action: ScheduleAction,
    quiet: bool,
) -> Result<(), NikaError> {
    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(10));

    if !client.socket_exists() {
        return Err(NikaError::Execution(
            "Daemon not running. Start with: nika daemon start".into(),
        ));
    }

    match action {
        ScheduleAction::Add {
            workflow,
            cron,
            name,
            tz,
            max_retries,
            args,
        } => {
            let resp = client
                .send(DaemonRequest::ScheduleAdd {
                    workflow: workflow.clone(),
                    name: name.clone(),
                    args,
                    cron_expr: cron.clone(),
                    timezone: Some(tz.clone()),
                    max_retries: Some(max_retries),
                    tags: None,
                })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleCreated { id } => {
                    if !quiet {
                        println!("{} schedule created", "✓".green().bold());
                        println!("  id:       {}", id);
                        println!("  workflow: {}", workflow);
                        println!("  cron:     {}", cron);
                        println!("  timezone: {}", tz);
                        if let Some(name) = name {
                            println!("  name:     {}", name);
                        }
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::List { json, enabled } => {
            let resp = client
                .send(DaemonRequest::ScheduleList {
                    enabled_only: enabled,
                })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleList { schedules } => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&schedules)
                                .unwrap_or_else(|_| "[]".into())
                        );
                        return Ok(());
                    }

                    if schedules.is_empty() {
                        println!("No schedules found");
                        return Ok(());
                    }

                    println!(
                        "{:<10} {:<8} {:<18} {:<16} {}",
                        "ID".bold(),
                        "ENABLED".bold(),
                        "CRON".bold(),
                        "TIMEZONE".bold(),
                        "WORKFLOW".bold(),
                    );

                    for sched in &schedules {
                        let id = sched["id"].as_str().unwrap_or("-");
                        let enabled = sched["enabled"].as_bool().unwrap_or(false);
                        let cron = sched["cron_expr"].as_str().unwrap_or("-");
                        let tz = sched["timezone"].as_str().unwrap_or("UTC");
                        let workflow = sched["workflow"].as_str().unwrap_or("-");
                        let run_count = sched["run_count"].as_u64().unwrap_or(0);

                        let short_id = if id.len() > 8 { &id[..8] } else { id };
                        let enabled_str = if enabled {
                            "active".green().to_string()
                        } else {
                            "paused".yellow().to_string()
                        };

                        println!(
                            "{:<10} {:<8} {:<18} {:<16} {} ({}x)",
                            short_id.dimmed(),
                            enabled_str,
                            cron,
                            tz.dimmed(),
                            workflow,
                            run_count,
                        );
                    }

                    println!("\n{} schedule(s)", schedules.len());
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Get { id } => {
            let resp = client
                .send(DaemonRequest::ScheduleGet { id })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleDetail { schedule } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&schedule)
                            .unwrap_or_else(|e| format!("(serialization error: {e})"))
                    );
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Remove { id } => {
            let resp = client
                .send(DaemonRequest::ScheduleRemove { id: id.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} removed", "✓".green().bold(), id);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Pause { id } => {
            let resp = client
                .send(DaemonRequest::SchedulePause { id: id.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} paused", "✓".green().bold(), id);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Resume { id } => {
            let resp = client
                .send(DaemonRequest::ScheduleResume { id: id.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} resumed", "✓".green().bold(), id);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }
    }

    Ok(())
}

fn sched_err(e: nika_daemon::DaemonError) -> NikaError {
    NikaError::Execution(format!("schedule: {e}"))
}
```

### 5.2 Wiring: `nika-cli/src/lib.rs`

Add after `pub mod jobs;` (line 31):

```rust
#[cfg(unix)]
pub mod schedule;
```

**Modification point**: `tools/nika-cli/src/lib.rs:31` (after `jobs`)

### 5.3 Wiring: `tools/nika/src/cli/mod.rs`

Add after `pub use nika_cli::jobs;` (line 18):

```rust
#[cfg(unix)]
pub use nika_cli::schedule;
```

**Modification point**: `tools/nika/src/cli/mod.rs:18` (after `jobs`)

### 5.4 Wiring: `tools/nika/src/main.rs`

Add `Schedule` variant to `Commands` enum (after `Job`, line ~761):

```rust
    /// Manage cron schedules via daemon
    #[cfg(unix)]
    #[command(next_help_heading = "SYSTEM")]
    Schedule {
        #[command(subcommand)]
        action: cli::schedule::ScheduleAction,
    },
```

Add dispatch arm (after `Job` dispatch, line ~1821):

```rust
        #[cfg(unix)]
        Some(Commands::Schedule { action }) => {
            cli::schedule::handle_schedule_command(action, quiet).await
        }
```

**Modification points**:
- `tools/nika/src/main.rs:761` (after `Job` variant in `Commands` enum)
- `tools/nika/src/main.rs:1821` (after `Job` dispatch in `match`)

---

## ERROR CODES

| Code | Meaning |
|------|---------|
| `SCHED-001` | Invalid cron expression (rejected by croner 3) |
| `SCHED-002` | Invalid timezone (not a valid IANA name) |
| `SCHED-003` | Schedule insert failed (DB error or duplicate name) |
| `SCHED-004` | Schedule query failed (DB error) |
| `SCHED-005` | Schedule not found |
| `SCHED-006` | Schedule delete failed |
| `SCHED-007` | Schedule enable/disable failed |

---

## TEST PLAN

### 5 Test Function Signatures

#### Test 1: `nika-storage` — schedule CRUD

```rust
// File: tools/nika-storage/src/lib.rs (inside #[cfg(test)] mod tests)
#[tokio::test]
async fn schedule_crud_lifecycle() {
    // 1. Insert a schedule → verify get_schedule returns it
    // 2. list_schedules(false) returns it, list_schedules(true) returns it (enabled=true)
    // 3. update_schedule_enabled(id, false) → list_schedules(true) excludes it
    // 4. update_schedule_after_fire(id, ...) → verify run_count incremented, last_job_id set
    // 5. delete_schedule(id) → get_schedule returns None
    // 6. delete non-existent → returns NotFound error
}
```

#### Test 2: `nika-storage` — V5 migration from V4

```rust
// File: tools/nika-storage/src/lib.rs (inside #[cfg(test)] mod tests)
#[tokio::test]
async fn v5_migration_creates_schedules_table() {
    // 1. Open in-memory DB (runs init_schema with V5)
    // 2. Insert a schedule → succeeds (table exists)
    // 3. Insert a job with tags → succeeds (V4 column still works)
    // 4. Verify both tables coexist without conflicts
}
```

#### Test 3: `nika-daemon` — schedule-based cron fires due job

```rust
// File: tools/nika-daemon/src/services/jobs.rs (inside #[cfg(test)] mod tests)
#[tokio::test]
async fn schedule_based_cron_fires_due_job() {
    // 1. Create a CronSchedule with "* * * * *" (every minute), next_run_at in the past
    // 2. Insert schedule into storage
    // 3. Call fire_due_cron_jobs → new job should be created
    // 4. Verify schedule.run_count == 1, schedule.last_job_id is set
    // 5. Verify schedule.next_run_at is in the future
}
```

#### Test 4: `nika-daemon` — paused schedule does not fire

```rust
// File: tools/nika-daemon/src/services/jobs.rs (inside #[cfg(test)] mod tests)
#[tokio::test]
async fn paused_schedule_does_not_fire() {
    // 1. Create a CronSchedule with enabled=true, next_run_at in the past
    // 2. Pause it via storage.update_schedule_enabled(id, false)
    // 3. Call fire_due_cron_jobs → no new jobs should be created
    // 4. Resume it via storage.update_schedule_enabled(id, true)
    // 5. Call fire_due_cron_jobs → now a job should be created
}
```

#### Test 5: `nika-daemon` — timezone-aware scheduling

```rust
// File: tools/nika-daemon/src/services/jobs.rs (inside #[cfg(test)] mod tests)
#[tokio::test]
async fn timezone_aware_next_run_computation() {
    // 1. Create schedule with timezone="Europe/Paris", cron_expr="0 3 * * *"
    // 2. Compute next_run_at → should be 03:00 Paris time (UTC+1 or UTC+2 depending on DST)
    // 3. Verify stored next_run_at is UTC (RFC 3339)
    // 4. Verify the UTC hour differs between winter (02:00 UTC) and summer (01:00 UTC)
    //    by using a fixed "now" timestamp in each season
}
```

---

## COMPLETE MODIFICATION INDEX

| # | File | Line | Change |
|---|------|------|--------|
| 1 | `tools/nika-storage/src/lib.rs` | 21 | `SCHEMA_VERSION: u32 = 4` -> `5` |
| 2 | `tools/nika-storage/src/lib.rs` | ~120 | Add `CronSchedule` struct |
| 3 | `tools/nika-storage/src/lib.rs` | ~219 | Add 6 `DbCommand` variants |
| 4 | `tools/nika-storage/src/lib.rs` | ~503 | Add 6 `Storage` async methods |
| 5 | `tools/nika-storage/src/lib.rs` | ~635 | Add 6 dispatch arms in `run_db_loop` |
| 6 | `tools/nika-storage/src/lib.rs` | ~727 | Add V5 migration block in `init_schema` |
| 7 | `tools/nika-storage/src/lib.rs` | ~1081 | Add 6 `do_*` query functions + `SCHEDULE_COLUMNS` + `row_to_schedule` |
| 8 | `tools/nika-daemon/src/protocol.rs` | ~93 | Add 6 `DaemonRequest` variants |
| 9 | `tools/nika-daemon/src/protocol.rs` | ~192 | Add 6 `Debug` arms for requests |
| 10 | `tools/nika-daemon/src/protocol.rs` | ~295 | Add 3 `DaemonResponse` variants |
| 11 | `tools/nika-daemon/src/protocol.rs` | ~366 | Add 3 `Debug` tags for responses |
| 12 | `tools/nika-daemon/src/server.rs` | ~632 | Add 6 dispatch arms in `route_request` |
| 13 | `tools/nika-daemon/src/services/jobs.rs` | ~376 | Add `storage()` accessor to `JobService` |
| 14 | `tools/nika-daemon/src/services/jobs.rs` | 486-554 | Replace `fire_due_cron_jobs` with schedule-based + legacy fallback |
| 15 | `tools/nika-daemon/Cargo.toml` | ~44 | Add `chrono-tz = { workspace = true }` |
| 16 | `tools/Cargo.toml` | ~222 | Add `chrono-tz = "0.10"` to workspace deps |
| 17 | `tools/nika-cli/src/schedule.rs` | **CREATE** | `ScheduleAction` enum + handler (~200 LOC) |
| 18 | `tools/nika-cli/src/lib.rs` | ~31 | Add `#[cfg(unix)] pub mod schedule;` |
| 19 | `tools/nika/src/cli/mod.rs` | ~18 | Add `#[cfg(unix)] pub use nika_cli::schedule;` |
| 20 | `tools/nika/src/main.rs` | ~761 | Add `Schedule` variant to `Commands` enum |
| 21 | `tools/nika/src/main.rs` | ~1821 | Add `Schedule` dispatch arm |

---

## IMPLEMENTATION ORDER (TDD)

```
Phase 1 (Storage):     ~250 LOC
  RED:   test_schedule_crud_lifecycle
  GREEN: CronSchedule struct, V5 migration, 6 DbCommand, 6 methods, 6 queries
  RED:   test_v5_migration_creates_schedules_table
  GREEN: Verify V4+V5 coexistence

Phase 2 (Protocol):    ~60 LOC
  Add 6 request + 3 response variants + Debug impls
  (No tests needed — these are pure data types, tested via Phase 3 integration)

Phase 3 (Server):      ~150 LOC
  Add 6 dispatch arms + storage() accessor + chrono-tz dep
  (Tested via Phase 4 integration)

Phase 4 (Scheduler):   ~160 LOC
  RED:   test_schedule_based_cron_fires_due_job
  GREEN: Replace fire_due_cron_jobs with schedule-based + legacy fallback
  RED:   test_paused_schedule_does_not_fire
  GREEN: Verify enabled flag is respected
  RED:   test_timezone_aware_next_run_computation
  GREEN: Verify chrono-tz integration

Phase 5 (CLI):         ~200 LOC
  CREATE schedule.rs + wire into lib.rs, mod.rs, main.rs
  (Manual testing: nika schedule add/list/get/pause/resume/remove)
```

**Total**: ~820 LOC (slightly over the 720 estimate due to legacy fallback code and
comprehensive chrono-tz integration — the fallback can be removed in v0.73+).

---

## WHAT THIS DOES NOT CHANGE

- `run_cron_scheduler()` loop (line 472-484) stays exactly as-is — still ticks every 60s
- `Job` struct stays as-is — spawned jobs still get the `cron` column populated
- `nika job submit --cron` still works — creates a job with cron column (legacy path)
- `nika serve` is not affected — schedules are daemon-only
- Existing tests (`cron_fire_due_jobs_fires_due_job`, `cron_fire_skips_when_already_active`)
  continue passing because the legacy fallback preserves old behavior
