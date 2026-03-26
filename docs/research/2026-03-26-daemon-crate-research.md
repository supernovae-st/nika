# Daemon Crate Research: rusqlite + notify + cron + DashMap

**Date:** 2026-03-26
**Purpose:** Job scheduler + file watcher + cache for nika daemon

---

## Version Reality Check

The versions asked about are outdated. Current state as of 2026-03-26:

| Crate | Asked | Latest Stable | Latest Overall |
|-------|-------|---------------|----------------|
| rusqlite | 0.32 | **0.39.0** | 0.39.0 |
| notify | 7.x | **8.2.0** | 9.0.0-rc.2 |
| cron | 0.13 | **0.16.0** | 0.16.0 |
| dashmap | 6.x | **6.1.0** | 7.0.0-rc2 |

**Recommendation:** Use rusqlite 0.39, notify 8.2 (stable) or 9.0.0-rc.2, cron 0.16, dashmap 6.1.

---

## 1. rusqlite 0.39 (bundled)

### Cargo.toml

```toml
[dependencies]
rusqlite = { version = "0.39.0", features = ["bundled"] }
tokio = { version = "1", features = ["full"] }
```

The `bundled` feature compiles SQLite 3.51.3 from source, avoiding system library issues.
Other useful features: `serde_json` (JSON columns), `chrono` or `time` (datetime types),
`backup` (online backup API).

### Creating Tables, Inserting, Querying

```rust
use rusqlite::{Connection, Result, params};

#[derive(Debug)]
struct Job {
    id: i64,
    name: String,
    cron_expr: String,
    next_run: i64,     // Unix timestamp
    enabled: bool,
}

fn setup_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS jobs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL UNIQUE,
            cron_expr  TEXT NOT NULL,
            next_run   INTEGER NOT NULL,
            enabled    BOOLEAN NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_next_run
            ON jobs(next_run) WHERE enabled = 1;

        CREATE TABLE IF NOT EXISTS job_runs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id     INTEGER NOT NULL REFERENCES jobs(id),
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status     TEXT NOT NULL DEFAULT 'running',
            error      TEXT
        );"
    )?;
    Ok(())
}

fn insert_job(conn: &Connection, name: &str, cron_expr: &str, next_run: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO jobs (name, cron_expr, next_run) VALUES (?1, ?2, ?3)",
        params![name, cron_expr, next_run],
    )?;
    Ok(conn.last_insert_rowid())
}

fn get_due_jobs(conn: &Connection, now: i64) -> Result<Vec<Job>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, cron_expr, next_run, enabled
         FROM jobs
         WHERE enabled = 1 AND next_run <= ?1
         ORDER BY next_run ASC"
    )?;

    let jobs = stmt.query_map(params![now], |row| {
        Ok(Job {
            id: row.get(0)?,
            name: row.get(1)?,
            cron_expr: row.get(2)?,
            next_run: row.get(3)?,
            enabled: row.get(4)?,
        })
    })?.collect::<Result<Vec<_>>>()?;

    Ok(jobs)
}
```

**Key API note:** `prepare_cached()` returns a `CachedStatement` that is returned to the
internal LRU cache when dropped. Always prefer it for repeated queries.

### WAL Mode + Best Practices

```rust
fn open_daemon_db(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;

    // WAL mode: allows concurrent readers + one writer without blocking
    conn.pragma_update(None, "journal_mode", "WAL")?;

    // Sync mode: NORMAL is safe with WAL (data survives process crash,
    // not OS crash — acceptable for a daemon's job state)
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    // Busy timeout: wait up to 5s if another connection holds a lock
    // (default is 5000ms already, but explicit is better)
    conn.busy_timeout(std::time::Duration::from_secs(5))?;

    // Enable foreign keys (off by default in SQLite!)
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // Increase cache size (negative = KiB, default is -2000 = 2MB)
    conn.pragma_update(None, "cache_size", "-8000")?; // 8MB

    // Memory-map I/O for reads (256MB — massive speedup for read-heavy)
    conn.pragma_update(None, "mmap_size", "268435456")?;

    Ok(conn)
}
```

### Tokio Integration Pattern (spawn_blocking)

**rusqlite's `Connection` is `Send` but NOT `Sync`.**
It cannot be shared across threads via `&Connection`. The canonical pattern is:

```rust
use std::sync::Mutex;
use tokio::task;

/// Wraps a rusqlite Connection for async access.
/// One connection behind a Mutex, dispatched to spawn_blocking.
struct DbPool {
    conn: Mutex<Connection>,
}

impl DbPool {
    fn new(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = open_daemon_db(path)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Run a closure on the connection from a blocking thread.
    async fn call<F, T>(&self, f: F) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        // IMPORTANT: lock INSIDE spawn_blocking so we don't hold a
        // MutexGuard across an await point (which would panic on
        // std::sync::Mutex with tokio)
        //
        // Actually, we CAN'T move &self into spawn_blocking.
        // Pattern: lock first, extract what we need, move into blocking.
        //
        // Better pattern: move the Connection into the blocking task.
        // But we can't do that with a shared pool.
        //
        // Real solution: use a dedicated thread.
        todo!()
    }
}

/// RECOMMENDED PATTERN: Dedicated database thread with a command channel.
/// This is how r2d2, deadpool, and tokio-rusqlite all work internally.
use tokio::sync::{mpsc, oneshot};

type DbCommand = Box<dyn FnOnce(&Connection) + Send>;

struct DaemonDb {
    tx: mpsc::Sender<DbCommand>,
}

impl DaemonDb {
    fn new(path: std::path::PathBuf) -> rusqlite::Result<Self> {
        let conn = open_daemon_db(&path)?;
        let (tx, mut rx) = mpsc::channel::<DbCommand>(64);

        // Dedicated thread — not a tokio task!
        std::thread::Builder::new()
            .name("nika-db".into())
            .spawn(move || {
                while let Some(cmd) = rx.blocking_recv() {
                    cmd(&conn);
                }
            })
            .expect("failed to spawn db thread");

        Ok(Self { tx })
    }

    /// Execute a read/write operation and get the result back.
    async fn call<F, T>(&self, f: F) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx.send(Box::new(move |conn| {
            let result = f(conn);
            let _ = resp_tx.send(result);
        })).await.map_err(|_| "db channel closed")?;

        resp_rx.await?.map_err(Into::into)
    }
}

// Usage:
async fn example(db: &DaemonDb) {
    let jobs = db.call(|conn| get_due_jobs(conn, chrono::Utc::now().timestamp()))
        .await
        .unwrap();
}
```

**Why not `tokio::task::spawn_blocking` directly?** You can, but you'd need to
clone an `Arc<Mutex<Connection>>` for each call, and the Mutex contention defeats
the purpose. The dedicated-thread pattern:
- Zero Mutex contention (single owner)
- Predictable ordering (channel is FIFO)
- Clean shutdown (drop the sender)
- Used by `tokio-rusqlite` crate internally

**Alternative: `tokio-rusqlite` crate** wraps exactly this pattern:
```toml
tokio-rusqlite = "0.6"
```
```rust
use tokio_rusqlite::Connection;

let conn = Connection::open("daemon.db").await?;
let jobs = conn.call(|conn| {
    // This closure runs on a dedicated thread
    get_due_jobs(conn, chrono::Utc::now().timestamp())
}).await?;
```

### Migration Pattern (Schema Versioning)

SQLite has a built-in `user_version` pragma. No external crate needed:

```rust
const CURRENT_SCHEMA_VERSION: i32 = 3;

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                cron_expr TEXT NOT NULL,
                next_run INTEGER NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT 1
            );
            CREATE TABLE job_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL REFERENCES jobs(id),
                started_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running'
            );"
        )?;
    }

    if version < 2 {
        conn.execute_batch(
            "ALTER TABLE job_runs ADD COLUMN finished_at TEXT;
             ALTER TABLE job_runs ADD COLUMN error TEXT;"
        )?;
    }

    if version < 3 {
        conn.execute_batch(
            "CREATE INDEX idx_jobs_next_run ON jobs(next_run) WHERE enabled = 1;
             ALTER TABLE jobs ADD COLUMN created_at TEXT NOT NULL DEFAULT (datetime('now'));
             ALTER TABLE jobs ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));"
        )?;
    }

    // Stamp the version
    if version < CURRENT_SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    }

    Ok(())
}
```

For more complex needs, the `refinery` crate provides file-based migrations with
a SQL runner for rusqlite. But for a daemon with a handful of tables, `user_version`
is simpler and has zero dependencies.

### Gotchas

1. **Connection is `Send` but not `Sync`** -- you cannot share `&Connection` across threads.
   Use the dedicated-thread pattern or `tokio-rusqlite`.

2. **WAL mode requires `journal_mode` pragma BEFORE any reads/writes** -- set it immediately
   after opening.

3. **`execute()` vs `execute_batch()`** -- `execute()` handles one statement with parameters.
   `execute_batch()` handles multiple statements separated by `;` but NO parameters. Use
   `execute_batch()` for DDL, `execute()` for DML.

4. **`prepare_cached()` returns `CachedStatement`** which borrows `&self` on `Connection`.
   You cannot hold two `CachedStatement`s from the same connection simultaneously in some cases.
   If you need to, use `prepare()` instead (not cached).

5. **`bundled` feature increases compile time** (~30s first build). Worth it for reproducibility.

6. **Default busy timeout is 5000ms** since recent versions. Earlier versions had 0ms (immediate
   `SQLITE_BUSY` error). Always set it explicitly.

7. **SQLite `BOOLEAN` is stored as INTEGER** (0/1). rusqlite handles this transparently via
   `FromSql`/`ToSql` for `bool`.

---

## 2. notify 8.x / 9.0.0-rc

### Version Decision

- **notify 7.0** (Oct 2024): MSRV 1.72, removed internal crossbeam, mio 1.0
- **notify 8.0** (Jan 2025): MSRV 1.77, notify-types 2.0, symlink config
- **notify 8.2** (Aug 2025): stable, inotify max_user_watches warning
- **notify 9.0.0-rc.2** (Feb 2026): MSRV 1.85, native tokio/futures `EventHandler` impls,
  `EventKindMask` filtering, macOS FSEvents rewrite (objc2)

**For a new daemon project in 2026: use 9.0.0-rc.2** (or 9.0 stable when released).
The tokio feature alone is worth it.

### Cargo.toml

```toml
# Stable choice
notify = "8.2"
notify-debouncer-full = "0.7"

# Bleeding edge (recommended for new code)
notify = { version = "9.0.0-rc.2", features = ["tokio"] }
notify-debouncer-full = "0.7"
```

### Watching Directories Recursively

```rust
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;

// --- Sync version (notify 8.x / 9.x) ---
fn watch_sync(path: &Path) -> notify::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    // RecommendedWatcher auto-selects:
    //   macOS  -> FSEventsWatcher (default) or KqueueWatcher (feature macos_kqueue)
    //   Linux  -> INotifyWatcher
    //   Windows -> ReadDirectoryChangesWatcher
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    // RecursiveMode::Recursive watches all subdirectories
    watcher.watch(path, RecursiveMode::Recursive)?;

    for result in rx {
        match result {
            Ok(event) => handle_event(event),
            Err(e) => eprintln!("watch error: {e}"),
        }
    }
    Ok(())
}

fn handle_event(event: Event) {
    use notify::EventKind;
    match event.kind {
        EventKind::Create(_) => println!("created: {:?}", event.paths),
        EventKind::Modify(_) => println!("modified: {:?}", event.paths),
        EventKind::Remove(_) => println!("removed: {:?}", event.paths),
        _ => {}
    }
}
```

### Tokio Integration

**notify 9.x with `tokio` feature (best):**

```rust
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

async fn watch_tokio(path: &std::path::Path) -> notify::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // In notify 9.x with `tokio` feature, UnboundedSender<Result<Event>>
    // implements EventHandler directly!
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(path, RecursiveMode::Recursive)?;

    while let Some(result) = rx.recv().await {
        match result {
            Ok(event) => handle_event(event),
            Err(e) => eprintln!("watch error: {e}"),
        }
    }

    Ok(())
}
```

**notify 8.x (manual channel bridging):**

```rust
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

fn make_watcher() -> notify::Result<(
    RecommendedWatcher,
    mpsc::UnboundedReceiver<notify::Result<Event>>,
)> {
    let (tx, rx) = mpsc::unbounded_channel();

    let watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            // This callback runs on the OS watcher thread (FSEvents/inotify).
            // UnboundedSender::send is non-blocking, safe to call here.
            let _ = tx.send(res);
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}

async fn watch_with_tokio(path: &std::path::Path) -> notify::Result<()> {
    let (mut watcher, mut rx) = make_watcher()?;
    watcher.watch(path, RecursiveMode::Recursive)?;

    while let Some(result) = rx.recv().await {
        match result {
            Ok(event) => handle_event(event),
            Err(e) => eprintln!("watch error: {e}"),
        }
    }
    Ok(())
}
```

### FSEvents (macOS) vs inotify (Linux)

| Aspect | FSEvents (macOS) | inotify (Linux) |
|--------|-----------------|-----------------|
| Granularity | Directory-level batched events | Per-file events |
| Recursive | Native recursive support | Must add watches per subdirectory |
| Rename tracking | Provides event flags | Gives FROM/TO as separate events |
| Latency | Configurable (default ~500ms internal) | Near-instant |
| Limits | Unlimited watches | `/proc/sys/fs/inotify/max_user_watches` (default 65536) |
| Move detection | Clone-annotated in v9 | Cookie-based IN_MOVED_FROM/TO |

**Practical impact for daemon:** On macOS, FSEvents batches events with internal latency.
On Linux, you get events immediately but may hit watch limits on huge trees.
notify's `Config` allows tuning:

```rust
let config = Config::default()
    .with_poll_interval(std::time::Duration::from_secs(2)); // only for PollWatcher fallback
```

### Debouncing: notify-debouncer-full vs notify-debouncer-mini

| Feature | debouncer-mini | debouncer-full |
|---------|---------------|----------------|
| Event merging | Time-based only | Semantic (rename, create+modify) |
| Rename tracking | No (separate events) | Yes (FROM+TO merged into one Rename) |
| File ID tracking | No | Yes (optional, FSEvents/Windows) |
| Duplicate removal | Basic | Advanced (no Modify after Create) |
| Directory delete | Multiple events | Single Remove event |
| Memory overhead | Minimal | Moderate (file ID cache) |
| Dependency weight | Light | Heavier (file-id crate) |

**Recommendation for daemon: `notify-debouncer-full`**

A daemon cares about "what changed" not "every intermediate event". The full debouncer
gives you clean, semantic events:

```rust
use notify::{RecursiveMode};
use notify_debouncer_full::{new_debouncer, RecommendedCache, DebouncedEvent};
use std::time::Duration;
use tokio::sync::mpsc;

async fn watch_debounced(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Bridge: debouncer uses std::sync callback, we bridge to tokio channel
    let mut debouncer = new_debouncer(
        Duration::from_millis(500), // debounce timeout
        None,                        // tick rate (None = automatic)
        move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
            let _ = tx.send(result);
        },
    )?;

    debouncer.watch(path, RecursiveMode::Recursive)?;

    while let Some(result) = rx.recv().await {
        match result {
            Ok(events) => {
                for event in events {
                    println!("debounced: {:?} {:?}", event.kind, event.paths);
                }
            }
            Err(errors) => {
                for e in errors {
                    eprintln!("debouncer error: {e}");
                }
            }
        }
    }

    Ok(())
}
```

**With EventKindMask filtering (notify 9.x):**

```rust
use notify::{EventKindMask, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, RecommendedCache};
use std::time::Duration;

fn create_filtered_debouncer() -> Result</* ... */, Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel();

    // CORE mask: CREATE, REMOVE, MODIFY_DATA, MODIFY_META, MODIFY_NAME
    // Excludes noisy ACCESS events (OPEN, CLOSE, READ)
    let notify_config = notify::Config::default()
        .with_event_kinds(EventKindMask::CORE);

    let debouncer = new_debouncer_opt::<_, RecommendedWatcher, RecommendedCache>(
        Duration::from_millis(500),
        None,
        tx,
        RecommendedCache::new(),
        notify_config,
    )?;

    Ok((debouncer, rx))
}
```

### Gotchas

1. **Watcher must be kept alive.** If the `RecommendedWatcher` (or debouncer) is dropped,
   watching stops. Store it in your daemon struct.

2. **macOS FSEvents has inherent batching latency** (~300-500ms). Events arrive in bursts.
   The debouncer timeout should be >= 500ms on macOS to catch full batches.

3. **Linux inotify watch limit.** Default is 65536 watches. Each subdirectory is one watch.
   `notify 8.2` warns when this limit is hit. Increase with:
   `echo 524288 | sudo tee /proc/sys/fs/inotify/max_user_watches`

4. **The callback runs on the OS watcher thread.** Never block in it. Send to a channel.
   With tokio channels, `send()` is non-blocking (good). With `futures::channel::mpsc`,
   the `block_on` in the official example is a known wart -- use tokio channels instead.

5. **`RecursiveMode::Recursive` on Linux** adds a watch for every existing subdirectory AND
   automatically watches new subdirectories as they appear. This can be slow for huge trees
   at startup.

6. **notify-debouncer-full 0.7 emits `Vec<DebouncedEvent>`**, not single events. Each
   debounce cycle yields a batch.

---

## 3. cron 0.16

### Cargo.toml

```toml
[dependencies]
cron = "0.16"
chrono = "0.4"
```

The cron crate depends on chrono. It uses a 7-field expression format:
`sec min hour day-of-month month day-of-week [year]`.

### API: Parsing and Next Run Time

```rust
use cron::Schedule;
use chrono::{Utc, DateTime, TimeZone};
use std::str::FromStr;

fn next_run_time(cron_expr: &str) -> Option<DateTime<Utc>> {
    let schedule = Schedule::from_str(cron_expr).ok()?;
    schedule.upcoming(Utc).next()
}

fn next_n_runs(cron_expr: &str, n: usize) -> Vec<DateTime<Utc>> {
    let schedule = Schedule::from_str(cron_expr).unwrap();
    schedule.upcoming(Utc).take(n).collect()
}

// Get next run AFTER a specific time (useful for rescheduling)
fn next_run_after(cron_expr: &str, after: &DateTime<Utc>) -> Option<DateTime<Utc>> {
    let schedule = Schedule::from_str(cron_expr).unwrap();
    schedule.after(after).next()
}

// Check if a specific time matches the schedule
fn is_match(cron_expr: &str, dt: DateTime<Utc>) -> bool {
    let schedule = Schedule::from_str(cron_expr).unwrap();
    schedule.includes(dt)
}

// Get previous occurrence (useful for "last run" display)
fn previous_run(cron_expr: &str) -> Option<DateTime<Utc>> {
    let schedule = Schedule::from_str(cron_expr).unwrap();
    schedule.upcoming(Utc).next_back()
}

// Owned iterator (no lifetime dependency on Schedule)
fn make_owned_iter(cron_expr: &str) -> cron::OwnedScheduleIterator<Utc> {
    let schedule = Schedule::from_str(cron_expr).unwrap();
    schedule.upcoming_owned(Utc)
}
```

### Shortcut Support (@hourly, @daily, etc.)

**YES -- the cron crate supports all standard shortcuts.** Verified from test suite:

| Shorthand | Equivalent | Supported |
|-----------|-----------|-----------|
| `@yearly` | `0 0 0 1 1 * *` | YES |
| `@annually` | same as @yearly | YES |
| `@monthly` | `0 0 0 1 * * *` | YES |
| `@weekly` | `0 0 0 * * 1 *` (Sunday) | YES |
| `@daily` | `0 0 0 * * * *` | YES |
| `@midnight` | same as @daily | YES |
| `@hourly` | `0 0 * * * * *` | YES |

```rust
// All of these parse successfully:
let _ = Schedule::from_str("@yearly").unwrap();
let _ = Schedule::from_str("@monthly").unwrap();
let _ = Schedule::from_str("@weekly").unwrap();
let _ = Schedule::from_str("@daily").unwrap();
let _ = Schedule::from_str("@hourly").unwrap();
```

### 7-Field Format (Not Standard 5-Field!)

This is the biggest gotcha. The cron crate uses **7 fields** (with seconds and optional year),
NOT the standard 5-field Unix crontab format:

```
sec  min  hour  day-of-month  month  day-of-week  [year]
```

So `0 30 9 * * Mon-Fri` means "every weekday at 9:30:00",
NOT `30 9 * * Mon-Fri` (which would be a parse error -- only 5 fields).

```rust
// WRONG: standard 5-field crontab
// Schedule::from_str("30 9 * * Mon-Fri").unwrap(); // ERROR!

// CORRECT: 7-field (or 6-field without year)
Schedule::from_str("0 30 9 * * Mon-Fri").unwrap();     // 6 fields (no year)
Schedule::from_str("0 30 9 * * Mon-Fri *").unwrap();    // 7 fields (all years)
```

### Thread Safety

`Schedule` is `Send + Sync + Clone`. You can share it across threads freely:

```rust
use std::sync::Arc;

let schedule = Arc::new(Schedule::from_str("@hourly").unwrap());

// Clone the Arc for each thread/task
let s = schedule.clone();
tokio::spawn(async move {
    let next = s.upcoming(Utc).next();
    // ...
});
```

### Integration with Daemon Scheduler

```rust
use cron::Schedule;
use chrono::Utc;
use std::str::FromStr;
use tokio::time::{sleep, Duration};

async fn scheduler_loop(db: &DaemonDb) {
    loop {
        let now = Utc::now();

        // Get all due jobs from SQLite
        let jobs = db.call(move |conn| {
            get_due_jobs(conn, now.timestamp())
        }).await.unwrap();

        for job in jobs {
            // Spawn each job execution
            let cron_expr = job.cron_expr.clone();
            let job_id = job.id;
            tokio::spawn(async move {
                execute_job(job_id).await;
            });

            // Compute and store next run time
            let next_run = Schedule::from_str(&cron_expr)
                .ok()
                .and_then(|s| s.after(&now).next())
                .map(|dt| dt.timestamp())
                .unwrap_or(i64::MAX); // disable if unparseable

            db.call(move |conn| {
                conn.execute(
                    "UPDATE jobs SET next_run = ?1, updated_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![next_run, job_id],
                )
            }).await.unwrap();
        }

        // Sleep until next check (1 second granularity matches cron's seconds field)
        sleep(Duration::from_secs(1)).await;
    }
}
```

### Gotchas

1. **7-field format, not 5-field.** The leading `sec` field catches everyone. If you need
   to accept standard 5-field crontab from users, prepend `"0 " + user_input`.

2. **`upcoming()` borrows `&self`** -- use `upcoming_owned()` or `after_owned()` if you need
   a `'static` iterator (e.g., to return from a function or move into a task).

3. **Year field is bounded.** By default, the iterator will stop producing values after
   the year range is exhausted. For "run forever" crons, omit the year field or use `*`.

4. **chrono dependency.** The crate is tightly coupled to chrono. If your project uses `time`
   crate instead, you'll need conversion.

5. **No `@every 5m` syntax.** The crate only supports standard cron expressions and the
   `@shorthand` aliases. For "every N minutes", use `0 */5 * * * *`.

6. **`next()` returns `Option<DateTime>`** -- it can return `None` if no future match exists
   (e.g., a year-constrained expression in the past).

---

## 4. DashMap 6.x

### Cargo.toml

```toml
[dependencies]
dashmap = "6.1"
```

Features: `serde`, `rayon`, `raw-api`, `inline-more`, `arbitrary`.

### Basic Cache-Like Usage

```rust
use dashmap::DashMap;
use std::sync::Arc;

// DashMap is internally sharded -- no external Mutex/RwLock needed.
// Default shard count = available_parallelism * 4, rounded to power of 2.
let cache: Arc<DashMap<String, CachedResult>> = Arc::new(DashMap::new());

// Insert (takes &self, not &mut self!)
cache.insert("key".into(), CachedResult { data: vec![], fetched_at: Instant::now() });

// Get returns a Ref guard (like RwLockReadGuard)
if let Some(entry) = cache.get("key") {
    println!("hit: {:?}", entry.value());
    // entry is dropped here, releasing the shard lock
}

// Get or insert
let entry = cache.entry("key".into()).or_insert_with(|| {
    CachedResult { data: vec![], fetched_at: Instant::now() }
});

// Modify in place
cache.alter("key", |_k, mut v| {
    v.fetched_at = Instant::now();
    v
});

// Remove and get the value
if let Some((_key, value)) = cache.remove("key") {
    println!("removed: {:?}", value);
}

// Conditional remove
cache.remove_if("key", |_k, v| v.is_expired());

// Iterate (locks shards one at a time, not all at once)
for entry in cache.iter() {
    println!("{}: {:?}", entry.key(), entry.value());
}

// Retain (bulk conditional removal)
cache.retain(|_k, v| !v.is_expired());

// Length and capacity
println!("entries: {}, capacity: {}", cache.len(), cache.capacity());
```

### TTL / Expiration Pattern

DashMap has no built-in TTL. Here are the three standard patterns:

**Pattern 1: Lazy Expiration (check on read)**

```rust
use std::time::{Duration, Instant};

#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
}

struct TtlCache<K: Eq + Hash, V> {
    map: DashMap<K, CacheEntry<V>>,
    default_ttl: Duration,
}

impl<K: Eq + Hash, V: Clone> TtlCache<K, V> {
    fn new(default_ttl: Duration) -> Self {
        Self {
            map: DashMap::new(),
            default_ttl,
        }
    }

    fn insert(&self, key: K, value: V) {
        self.map.insert(key, CacheEntry {
            value,
            expires_at: Instant::now() + self.default_ttl,
        });
    }

    fn get(&self, key: &K) -> Option<V> {
        let entry = self.map.get(key)?;
        if entry.expires_at > Instant::now() {
            Some(entry.value.clone())
        } else {
            drop(entry); // IMPORTANT: release read lock before removing
            self.map.remove(key);
            None
        }
    }

    fn get_or_insert_with<F: FnOnce() -> V>(&self, key: K, f: F) -> V
    where
        K: Clone,
    {
        // Check existing
        if let Some(v) = self.get(&key) {
            return v;
        }
        // Insert new
        let value = f();
        self.insert(key, value.clone());
        value
    }
}
```

**Pattern 2: Background Reaper (periodic sweep)**

```rust
impl<K: Eq + Hash + Clone + Send + Sync + 'static, V: Send + Sync + 'static> TtlCache<K, V> {
    fn spawn_reaper(&self, interval: Duration) -> tokio::task::JoinHandle<()>
    where
        K: Clone,
    {
        let map = self.map.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let now = Instant::now();
                map.retain(|_k, v| v.expires_at > now);
            }
        })
    }
}

// Usage:
let cache = Arc::new(TtlCache::new(Duration::from_secs(300)));
let _reaper = cache.spawn_reaper(Duration::from_secs(60)); // sweep every minute
```

**Pattern 3: Hybrid (lazy + reaper) -- RECOMMENDED**

Use lazy expiration on reads (instant accuracy) plus a background reaper to prevent
memory growth from entries that are written but never read again.

### Memory Management and Eviction Strategies

DashMap grows without bound. For a daemon cache, you need eviction:

**LRU-ish eviction with DashMap:**

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct BoundedCache<K: Eq + Hash, V> {
    map: DashMap<K, CacheEntry<V>>,
    max_entries: usize,
    default_ttl: Duration,
    access_counter: AtomicU64,
}

#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
    last_access: u64, // monotonic counter, not timestamp
}

impl<K: Eq + Hash + Clone + Send + Sync + 'static, V: Clone + Send + Sync + 'static>
    BoundedCache<K, V>
{
    fn new(max_entries: usize, default_ttl: Duration) -> Self {
        Self {
            map: DashMap::with_capacity(max_entries),
            max_entries,
            default_ttl,
            access_counter: AtomicU64::new(0),
        }
    }

    fn get(&self, key: &K) -> Option<V> {
        let mut entry = self.map.get_mut(key)?;
        if entry.expires_at <= Instant::now() {
            drop(entry);
            self.map.remove(key);
            return None;
        }
        entry.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        Some(entry.value.clone())
    }

    fn insert(&self, key: K, value: V) {
        // Evict if over capacity
        if self.map.len() >= self.max_entries {
            self.evict_oldest();
        }

        let counter = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.map.insert(key, CacheEntry {
            value,
            expires_at: Instant::now() + self.default_ttl,
            last_access: counter,
        });
    }

    fn evict_oldest(&self) {
        // Find entry with lowest access counter
        // NOTE: This iterates all shards. For hot paths, consider
        // a probabilistic approach (sample N entries, evict oldest).
        let mut oldest_key = None;
        let mut oldest_access = u64::MAX;

        for entry in self.map.iter() {
            if entry.last_access < oldest_access {
                oldest_access = entry.last_access;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.map.remove(&key);
        }
    }
}
```

**For production LRU:** Consider `moka` (concurrent cache with TTL, LRU, and size-based
eviction built in). DashMap is great as a building block but `moka` is purpose-built:

```toml
moka = { version = "0.12", features = ["future"] }
```

```rust
use moka::future::Cache;

let cache: Cache<String, Vec<u8>> = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(300))
    .time_to_idle(Duration::from_secs(60))
    .build();

// Async get-or-insert
let value = cache.get_with("key".into(), async {
    fetch_expensive_data().await
}).await;
```

### Gotchas

1. **Ref guards hold shard locks.** Never hold a `Ref` or `RefMut` across an `.await` point
   or while doing other DashMap operations on the same shard -- instant deadlock:

   ```rust
   // DEADLOCK: holding ref while trying to remove
   let entry = cache.get("key");
   cache.remove("key"); // blocks forever if same shard!

   // CORRECT: drop the ref first
   let value = cache.get("key").map(|r| r.value().clone());
   drop(value); // or just let it go out of scope
   cache.remove("key");
   ```

2. **`entry()` API takes ownership of the key.** Use `entry(key.clone())` if you need
   the key afterwards.

3. **Iteration is not atomic.** `iter()` locks shards one at a time. Concurrent modifications
   to other shards are visible. For a consistent snapshot, use `into_read_only()`.

4. **No ordering guarantees.** Unlike `BTreeMap`, iteration order is arbitrary and may change
   between runs.

5. **`retain()` locks each shard exclusively.** It's efficient for bulk cleanup but will
   briefly block writers on each shard.

6. **Memory: DashMap never shrinks.** Even after removing entries, the allocated hash table
   memory is not returned to the allocator. For a long-running daemon, this means peak
   memory usage determines steady-state memory. Use `shrink_to_fit()` periodically if this
   matters.

7. **`remove_if()` is atomic.** The check and remove happen under the same shard lock --
   no TOCTOU race. Prefer it over `get()` + `remove()`.

---

## Recommended Cargo.toml for Daemon

```toml
[dependencies]
# Database
rusqlite = { version = "0.39", features = ["bundled"] }
# Or use the tokio wrapper:
# tokio-rusqlite = "0.6"

# File watching
notify = { version = "9.0.0-rc.2", features = ["tokio"] }
notify-debouncer-full = "0.7"

# Cron scheduling
cron = "0.16"
chrono = "0.4"

# In-memory cache
dashmap = "6.1"
# Or for production LRU+TTL:
# moka = { version = "0.12", features = ["future"] }

# Async runtime
tokio = { version = "1", features = ["full"] }
```

## Architecture Sketch

```
                    +-------------------+
                    |   Daemon Main     |
                    | (tokio runtime)   |
                    +--------+----------+
                             |
              +--------------+--------------+
              |              |              |
     +--------v---+  +------v------+  +----v--------+
     | Scheduler  |  | FileWatcher |  | Cache Layer |
     | (cron)     |  | (notify)    |  | (DashMap)   |
     +--------+---+  +------+------+  +----+--------+
              |              |              |
              +--------------+--------------+
                             |
                    +--------v----------+
                    |    DaemonDb       |
                    | (rusqlite, WAL)   |
                    | dedicated thread  |
                    +-------------------+
```

- **Scheduler** polls SQLite every 1s for due jobs, computes next_run via cron
- **FileWatcher** feeds debounced events into a tokio channel for the daemon to react
- **Cache** (DashMap or moka) stores hot data (parsed schedules, recent results)
- **DaemonDb** runs on a dedicated thread, accessed via async command channel

## Sources

1. [rusqlite README](https://github.com/rusqlite/rusqlite) -- v0.39.0, bundled SQLite 3.51.3
2. [rusqlite docs.rs](https://docs.rs/rusqlite/0.39.0) -- Connection API, CachedStatement
3. [notify README](https://github.com/notify-rs/notify) -- Platform backends, MSRV
4. [notify CHANGELOG](https://github.com/notify-rs/notify/blob/main/notify/CHANGELOG.md) -- v7/v8/v9 changes
5. [notify examples](https://github.com/notify-rs/notify/tree/main/examples) -- async_monitor, debouncer_full
6. [notify-debouncer-full docs](https://docs.rs/notify-debouncer-full/0.7.0) -- Semantic event merging
7. [cron crate](https://github.com/zslayton/cron) -- v0.16.0, 7-field format
8. [cron test suite](https://github.com/zslayton/cron/blob/master/tests/lib.rs) -- @shorthand tests
9. [dashmap README](https://github.com/xacrimon/dashmap) -- v6.1/v7-rc, sharded concurrent map
10. [dashmap source](https://github.com/xacrimon/dashmap/blob/master/src/lib.rs) -- Internal architecture

## Confidence Level

**High** -- All information verified against source code, official READMEs, test suites,
and docs.rs API documentation. Version numbers confirmed via `cargo search`. The cron
shorthand support was verified from actual test cases in the crate's test suite.
