# Nika Native Daemon — Architecture Plan

> The daemon is Nika's background brain: secrets, jobs, cache, watch, events.

## Design Principles

1. **Single binary** — `nika daemon start` uses the same binary
2. **Optional superpower** — `nika run` works without daemon, daemon adds persistent features
3. **Unix socket IPC** — `~/.nika/daemon/nika.sock`, length-prefixed JSON
4. **tokio-native** — runs on the same async runtime as the rest of Nika
5. **Graceful lifecycle** — PID file, SIGTERM handling, auto-cleanup

## Architecture

```
                    nika daemon start
                         │
                    ┌─────▼─────┐
                    │  Daemon    │ ← ~/.nika/daemon/nika.sock
                    │  Process   │ ← ~/.nika/daemon/nika.pid
                    ├────────────┤
                    │            │
    ┌───────────────┤  Services  ├───────────────┐
    │               │            │               │
    ▼               ▼            ▼               ▼
┌────────┐   ┌──────────┐  ┌─────────┐   ┌──────────┐
│Secrets │   │  Jobs    │  │ Watch   │   │  Cache   │
│Manager │   │Scheduler │  │  Mode   │   │  (LLM)   │
├────────┤   ├──────────┤  ├─────────┤   ├──────────┤
│keyring │   │ sqlite   │  │ notify  │   │ DashMap  │
│env vars│   │ cron     │  │ debounce│   │ +sqlite  │
│rotation│   │ retry    │  │ re-run  │   │ TTL      │
└────────┘   └──────────┘  └─────────┘   └──────────┘
    │               │            │               │
    └───────────────┴────────────┴───────────────┘
                         │
                    ┌────▼────┐
                    │  Event  │ ← broadcast::channel
                    │   Bus   │ ← TUI subscribes
                    └─────────┘
```

## New Crate: `nika-daemon`

```
tools/nika-daemon/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API (DaemonClient, DaemonServer)
    ├── server.rs           # tokio UnixListener, request router
    ├── client.rs           # DaemonClient (connect, send, recv)
    ├── protocol.rs         # IPC message types (Request, Response)
    ├── services/
    │   ├── mod.rs
    │   ├── secrets.rs      # Keychain access (sole accessor)
    │   ├── jobs.rs         # Job scheduler (submit, list, cancel)
    │   ├── watch.rs        # File watcher (notify + debounce)
    │   ├── cache.rs        # LLM response cache (DashMap + sqlite)
    │   └── health.rs       # Provider health monitoring
    ├── storage.rs          # SQLite for jobs + cache persistence
    └── lifecycle.rs        # PID file, daemonize, signal handling
```

## IPC Protocol

Wire format: `[4-byte big-endian length][JSON payload]`

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum DaemonRequest {
    // Secrets
    GetSecret { provider: String },
    SetSecret { provider: String, value: String },
    HasSecret { provider: String },
    ListSecrets,

    // Jobs
    JobSubmit { workflow: String, args: Map<String, Value>, name: Option<String>, cron: Option<String> },
    JobList { state: Option<JobState> },
    JobStatus { id: String },
    JobCancel { id: String },
    JobRetry { id: String },

    // Watch
    WatchStart { dir: String, patterns: Vec<String> },
    WatchStop,
    WatchStatus,

    // Cache
    CacheGet { key: String },
    CacheSet { key: String, value: Value, ttl_secs: Option<u64> },
    CacheClear,
    CacheStats,

    // Health
    Ping,
    Status,
    ProviderHealth,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum DaemonResponse {
    Ok,
    Error { code: String, message: String },
    Secret { value: Option<String> },
    SecretExists { exists: bool },
    SecretList { providers: Vec<ProviderSecretInfo> },
    JobCreated { id: String },
    JobList { jobs: Vec<JobInfo> },
    JobDetail { job: JobInfo },
    WatchActive { dir: String, watching: usize },
    CacheHit { value: Value },
    CacheMiss,
    CacheStatsResult { entries: usize, hits: u64, misses: u64, size_bytes: u64 },
    Pong { version: String, uptime_secs: u64 },
    StatusInfo { pid: u32, uptime_secs: u64, services: Vec<ServiceStatus> },
    ProviderHealthResult { providers: Vec<ProviderHealthInfo> },
}
```

## CLI Commands

```bash
# Lifecycle
nika daemon start              # Start daemon (background, daemonize)
nika daemon start --foreground # Start daemon (foreground, for debugging)
nika daemon stop               # Graceful shutdown (SIGTERM)
nika daemon restart            # Stop + start
nika daemon status             # PID, uptime, services, socket path
nika daemon logs               # Tail daemon log file

# Jobs (via daemon IPC)
nika job submit workflow.nika.yaml           # One-shot job
nika job submit workflow.nika.yaml --cron "0 * * * *"  # Recurring
nika job submit workflow.nika.yaml --name "daily-report"
nika job list                                # All jobs
nika job list --running                      # Filter by state
nika job status <id>                         # Job detail + logs
nika job cancel <id>                         # Cancel running job
nika job retry <id>                          # Retry failed job
nika job history                             # Recent completions

# Watch (via daemon IPC)
nika watch .                                 # Watch current dir
nika watch ./workflows --pattern "*.nika.yaml"
nika watch stop                              # Stop watching

# Cache
nika cache stats                             # Hit/miss/size
nika cache clear                             # Flush LLM cache

# Doctor integration
nika doctor                                  # Now checks daemon health
```

## Job Scheduler Design

```rust
struct Job {
    id: String,          // uuid v4
    name: Option<String>,
    workflow: PathBuf,
    args: Map<String, Value>,
    cron: Option<String>,  // cron expression for recurring
    state: JobState,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    exit_code: Option<i32>,
    output: Option<String>,  // last N lines of output
    retry_count: u32,
    max_retries: u32,        // default 0 (no retry)
}

enum JobState {
    Pending,
    Running { pid: u32 },
    Completed,
    Failed { error: String },
    Cancelled,
}
```

Jobs execute by spawning `nika run <workflow> --json-output` as a child process. The daemon captures stdout/stderr and tracks lifecycle.

SQLite storage at `~/.nika/daemon/jobs.db`:
```sql
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    name TEXT,
    workflow TEXT NOT NULL,
    args TEXT,  -- JSON
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

CREATE TABLE job_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id),
    event TEXT NOT NULL,  -- 'started', 'completed', 'failed', 'cancelled', 'retried'
    timestamp TEXT NOT NULL,
    details TEXT
);
```

## LLM Cache Design

Cache key: `blake3(model + provider + prompt + system + temperature + max_tokens)`

```rust
struct CacheEntry {
    key: String,       // blake3 hash hex
    provider: String,
    model: String,
    response: String,  // full LLM response
    tokens_in: u64,
    tokens_out: u64,
    cost: f64,
    created_at: DateTime<Utc>,
    ttl_secs: u64,     // default 3600 (1 hour)
    hits: u64,
}
```

In-memory DashMap for hot cache + SQLite for persistence across daemon restarts.

## Watch Mode Design

Uses the `notify` crate for filesystem events:

```rust
struct WatchService {
    watcher: RecommendedWatcher,
    debounce: Duration,  // 500ms default
    patterns: Vec<GlobPattern>,
    // On file change → match patterns → submit Job for modified workflow
}
```

Workflow: file change → debounce → `nika check <file>` → if valid → `nika run <file>` as job.

## Implementation Phases

### Phase 1: Foundation (2-3 sessions)
1. Create `nika-daemon` crate with server/client/protocol
2. Implement DaemonServer (UnixListener, request router)
3. Implement DaemonClient (connect, send, recv)
4. `nika daemon start/stop/status` commands
5. Secrets service (migrate from current keyring code)
6. `nika doctor` daemon health check
7. PID file, signal handling, graceful shutdown

### Phase 2: Jobs (2 sessions)
1. SQLite storage for jobs
2. Job submission + execution (spawn child process)
3. Job list/status/cancel commands
4. Cron scheduling (use `cron` crate for parsing)
5. TUI Control view: job list panel
6. Job retry + max_retries

### Phase 3: Watch + Cache (1-2 sessions)
1. Watch service (notify crate + debounce)
2. `nika watch` command
3. LLM cache service (DashMap + SQLite)
4. Cache integration in nika-engine provider layer
5. `nika cache stats/clear` commands

### Phase 4: Events + TUI (1 session)
1. Event bus (broadcast channel, clients subscribe)
2. TUI subscribes to daemon events for live job updates
3. Dashboard view with job history, cache stats

## Dependencies

```toml
# nika-daemon/Cargo.toml
[dependencies]
nika-core = { workspace = true }
nika-engine = { workspace = true }

# Async
tokio = { workspace = true, features = ["net", "signal", "process"] }

# IPC
serde = { workspace = true }
serde_json = { workspace = true }

# Storage
rusqlite = { version = "0.32", features = ["bundled"] }

# File watching
notify = "7"
notify-debouncer-full = "0.4"

# Scheduling
cron = "0.13"
chrono = { workspace = true }

# Hashing (cache keys)
blake3 = { workspace = true }

# Keychain
keyring = { version = "3", features = ["apple-native", "windows-native"] }

# Lifecycle
nix = { version = "0.29", features = ["signal", "process"] }

# Logging
tracing = { workspace = true }
```

## File Layout

```
~/.nika/
├── config.toml          # Existing
├── daemon/
│   ├── nika.sock        # Unix socket
│   ├── nika.pid         # PID file
│   ├── nika.log         # Daemon log (rotated)
│   ├── jobs.db          # SQLite (jobs + history)
│   └── cache.db         # SQLite (LLM cache)
├── media/               # CAS store (existing)
└── traces/              # Trace files (existing)
```
