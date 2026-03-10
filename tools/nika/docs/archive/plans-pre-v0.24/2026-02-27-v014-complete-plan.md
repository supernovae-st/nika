# Nika v0.14 Complete Implementation Plan

**Version:** v0.14.0 (Schema v0.7)
**Date:** 2026-02-27
**Status:** Approved

---

## Executive Summary

v0.14 is a major release introducing 5 feature areas:

| Feature | Impact | Effort |
|---------|--------|--------|
| Schema Migration (`memory:` → `context:`) | Breaking (with compat) | ~6h |
| Workflow Composition (`include:` + `invoke_workflow:`) | New capability | ~8h |
| Jobs Daemon (background scheduler) | New capability | ~16h |
| CLI DX (flags, completion, config) | Enhancement | ~4h |
| Enhanced Doctor | Enhancement | ~4h |
| **Total** | | **~38h** |

> **⚠️ PRE-REQUISITE:** See [Section 14](#14-critical-pre-v014-dependency-updates) for critical dependency updates (serde_yaml deprecation, security advisories) that MUST be resolved before v0.14 development begins.

---

## 1. Schema Migration: `memory:` → `context:`

### Rationale

- Align with MCP (Model Context Protocol) terminology
- Consistency with `.nika/context/` directory
- Reserve `memory:` for future persistent memory (vector stores)

### Schema Change

```yaml
# BEFORE (v0.6)
schema: nika/workflow@0.6
memory:
  files:
    brand: ./context/brand.md
  session: .nika/sessions/prev.json

# AFTER (v0.7)
schema: nika/workflow@0.7
context:
  files:
    brand: ./context/brand.md
  session: .nika/sessions/prev.json
```

### Backward Compatibility

Both syntaxes work in v0.7:
- `context:` — New preferred syntax
- `memory:` — Deprecated alias (emits warning)

Template bindings:
- `{{context.files.X}}` — New preferred syntax
- `{{memory.files.X}}` — Deprecated alias (emits warning)

### Files Impacted

See `2026-02-27-memory-to-context-migration.md` for detailed file inventory (28 files across 8 tiers).

### Implementation

```rust
// In workflow.rs - Support both syntaxes
#[derive(Debug, Deserialize)]
struct WorkflowRaw {
    #[serde(default)]
    pub context: Option<ContextConfig>,

    #[serde(default)]
    pub memory: Option<ContextConfig>,  // Deprecated alias
}

impl Workflow {
    pub fn from_raw(raw: WorkflowRaw) -> Self {
        let context = raw.context.or_else(|| {
            if raw.memory.is_some() {
                tracing::warn!("'memory:' is deprecated, use 'context:' instead");
            }
            raw.memory
        });
        // ...
    }
}
```

---

## 2. Workflow Composition

### Two Patterns

| Pattern | Use Case | Execution |
|---------|----------|-----------|
| `include:` | Reusable task libraries | DAG fusion, shared DataStore |
| `invoke_workflow:` | Isolated sub-workflows | Separate execution, params/result |

### 2.1 Include (DAG Fusion)

```yaml
schema: nika/workflow@0.7
workflow: main-workflow

include:
  - path: ./lib/seo-tasks.nika.yaml
    prefix: seo_           # Optional: prefix all task IDs
  - path: ./lib/common.nika.yaml

tasks:
  - id: generate
    infer: "Generate content"
    use.ctx: result

  - id: optimize
    infer: "Optimize for SEO using {{use.seo_keywords}}"
    depends_on: [generate, seo_analyze]  # Reference included task
```

**Behavior:**
- Included tasks merged into main DAG at parse time
- Shared DataStore (all bindings accessible)
- Circular includes detected and rejected
- Prefix prevents ID collisions

### 2.2 Invoke Workflow (Isolation)

```yaml
tasks:
  - id: generate_page
    invoke_workflow:
      path: ./workflows/page-generator.nika.yaml
      params:
        entity: "{{use.entity}}"
        locale: "fr-FR"
      timeout: 300s
    use.page: result
```

**Behavior:**
- Child workflow runs in isolation
- Own DataStore (params injected, result extracted)
- Parent waits for completion
- Timeout protection

### Implementation

```rust
// New AST types
pub struct IncludeSpec {
    pub path: String,
    pub prefix: Option<String>,
}

pub struct InvokeWorkflowSpec {
    pub path: String,
    pub params: FxHashMap<String, Value>,
    pub timeout: Option<Duration>,
}

// In TaskAction enum
pub enum TaskAction {
    Infer(InferSpec),
    Exec(ExecSpec),
    Fetch(FetchSpec),
    Invoke(InvokeSpec),
    Agent(AgentSpec),
    InvokeWorkflow(InvokeWorkflowSpec),  // NEW
}
```

### Crate Dependencies

None required — uses existing YAML parsing and runtime.

---

## 3. Jobs Daemon

### Overview

Background service for scheduled workflow execution with 4 trigger types.

### CLI Commands

```bash
# Daemon lifecycle
nika jobs start [--foreground]    # Start daemon
nika jobs stop                    # Stop daemon
nika jobs restart                 # Restart daemon
nika jobs status                  # Show daemon status

# Job management
nika jobs list                    # List all jobs
nika jobs run <name>              # Run job immediately
nika jobs enable <name>           # Enable disabled job
nika jobs disable <name>          # Disable job
nika jobs history [name]          # Show execution history

# Observability
nika jobs logs [name] [--follow]  # View job logs
nika jobs metrics                 # Show Prometheus metrics

# System integration
nika jobs install                 # Install as system service
nika jobs uninstall               # Remove system service
```

### Configuration

```toml
# .nika/config.toml

[jobs]
enabled = true
pid_file = "~/.nika/jobs.pid"
state_db = "~/.nika/jobs.db"
log_dir = "~/.nika/logs/jobs"
metrics_port = 9090

[jobs.webhook]
enabled = true
port = 8080
bind = "127.0.0.1"
auth_token = "${NIKA_WEBHOOK_TOKEN}"

[jobs.notify]
on_failure = ["slack"]
slack_webhook = "${SLACK_WEBHOOK_URL}"
discord_webhook = "${DISCORD_WEBHOOK_URL}"
email_smtp = "smtp.gmail.com:587"
email_from = "nika@example.com"

[[jobs.definitions]]
name = "daily-sync"
workflow = "./workflows/sync.nika.yaml"
enabled = true
trigger = { cron = "0 9 * * *", timezone = "Europe/Paris" }
retry = { max_attempts = 3, backoff = "exponential", initial_delay = "1s", max_delay = "5m" }
timeout = "30m"
on_failure = ["notify:slack", "notify:email"]
on_success = []

[[jobs.definitions]]
name = "watch-uploads"
workflow = "./workflows/process-upload.nika.yaml"
trigger = { watch = "./uploads/*.json", debounce = "5s" }
params = { file = "{{trigger.path}}" }

[[jobs.definitions]]
name = "webhook-deploy"
workflow = "./workflows/deploy.nika.yaml"
trigger = { webhook = "/hooks/deploy", method = "POST" }
params = { payload = "{{trigger.body}}" }

[[jobs.definitions]]
name = "health-check"
workflow = "./workflows/health.nika.yaml"
trigger = { interval = "5m" }
```

### Trigger Types

| Type | Description | Config |
|------|-------------|--------|
| `cron` | Cron expression | `{ cron = "0 9 * * *", timezone = "..." }` |
| `webhook` | HTTP endpoint | `{ webhook = "/path", method = "POST" }` |
| `watch` | File system events | `{ watch = "glob", debounce = "5s" }` |
| `interval` | Fixed interval | `{ interval = "5m" }` |

### Retry Strategy (BackON crate)

```rust
use backon::{ExponentialBuilder, Retryable};

let result = workflow_runner
    .run(workflow)
    .retry(
        ExponentialBuilder::default()
            .with_min_delay(Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(300))
            .with_max_times(3)
            .with_jitter()
    )
    .await;
```

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  Jobs Daemon Architecture                                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ Cron Trigger │    │Webhook Trigger│   │ Watch Trigger│    │Interval Trigger│ │
│  │ (tokio-cron) │    │   (axum)     │    │  (notify)    │    │  (tokio)     │  │
│  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘    └──────┬───────┘  │
│         │                   │                   │                   │          │
│         └───────────────────┴───────────────────┴───────────────────┘          │
│                                    │                                            │
│                                    ▼                                            │
│                          ┌─────────────────┐                                    │
│                          │  Job Scheduler  │                                    │
│                          │  (orchestrator) │                                    │
│                          └────────┬────────┘                                    │
│                                   │                                             │
│         ┌─────────────────────────┼─────────────────────────┐                   │
│         │                         │                         │                   │
│         ▼                         ▼                         ▼                   │
│  ┌─────────────┐          ┌─────────────┐          ┌─────────────┐             │
│  │   Runner    │          │   Runner    │          │   Runner    │             │
│  │ (workflow)  │          │ (workflow)  │          │ (workflow)  │             │
│  └──────┬──────┘          └──────┬──────┘          └──────┬──────┘             │
│         │                        │                        │                     │
│         └────────────────────────┼────────────────────────┘                     │
│                                  │                                              │
│                                  ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                         State DB (rusqlite)                              │   │
│  │  • Job definitions  • Execution history  • Retry state  • Metrics       │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                  │                                              │
│                                  ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                         Notifications                                    │   │
│  │  • Slack  • Discord  • Email (lettre)  • Webhook  • ntfy               │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Crate Dependencies

```toml
[dependencies]
# Cron scheduling (709 stars, production-ready)
tokio-cron-scheduler = { version = "0.15", features = ["english"] }

# File watching
notify = "6.1"

# Retry with exponential backoff + jitter
backon = "1.6"

# Webhook HTTP server
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["util", "map-request-body"] }

# HMAC signature verification
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
secrecy = "0.8"  # Protect secrets in memory

# State persistence
rusqlite = { version = "0.31", features = ["bundled"] }

# Metrics
metrics = "0.23"
metrics-exporter-prometheus = "0.15"

# Notifications
lettre = "0.11"           # Email
reqwest = "0.12"          # Webhook/Slack/Discord

# System
fd-lock = "4.0"           # PID file locking
chrono-tz = "0.10"        # Timezone support
```

### Webhook HMAC Verification Pattern

Based on GitHub/Lemon Squeezy webhook patterns:

```rust
use axum::{
    body::{self, BoxBody, Full},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Middleware to verify webhook HMAC signature
pub async fn verify_webhook_signature(
    secret: SecretString,
    req: Request<BoxBody>,
    next: Next<BoxBody>,
) -> Result<Response, Response> {
    let (parts, body_parts) = req.into_parts();

    // Extract signature header (configurable per provider)
    let signature_header = parts.headers
        .get("X-Hub-Signature-256")  // GitHub
        .or_else(|| parts.headers.get("X-Signature"))  // Lemon Squeezy
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Missing signature").into_response())?;

    // Read body bytes
    let bytes_body = hyper::body::to_bytes(body_parts)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?;

    // Compute HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?;
    mac.update(&bytes_body);

    // Extract and verify signature
    let sig_bytes = signature_header.as_bytes();
    let sig_hex = sig_bytes
        .strip_prefix(b"sha256=")
        .unwrap_or(sig_bytes);

    let decoded = hex::decode(sig_hex)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid signature format").into_response())?;

    mac.verify_slice(&decoded)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid signature").into_response())?;

    // Rebuild request and continue
    let new_req = Request::from_parts(parts, body::boxed(Full::from(bytes_body)));
    Ok(next.run(new_req).await)
}
```

### Webhook Handler Best Practices

```rust
/// Webhook handler - returns 200 FAST, processes async
pub async fn handle_webhook(
    State(state): State<AppState>,
    Json(payload): Json<WebhookPayload>,
) -> StatusCode {
    // 1. Dedupe by event_id (idempotency)
    if state.seen_events.contains(&payload.event_id) {
        return StatusCode::OK;
    }
    state.seen_events.insert(payload.event_id.clone());

    // 2. Store raw payload for debugging/replay
    state.event_store.store(&payload).await;

    // 3. Spawn async processing (don't block webhook sender)
    let workflow_path = state.config.webhook_workflow.clone();
    tokio::spawn(async move {
        if let Err(e) = run_workflow(&workflow_path, payload.into()).await {
            tracing::error!("Webhook workflow failed: {}", e);
        }
    });

    // 4. Return 200 immediately
    StatusCode::OK
}
```

### Cron Scheduler Pattern (tokio-cron-scheduler)

```rust
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};
use chrono_tz::Tz;

pub struct NikaJobScheduler {
    scheduler: JobScheduler,
    jobs: HashMap<String, Uuid>,  // name -> job_id
}

impl NikaJobScheduler {
    pub async fn new() -> Result<Self, JobSchedulerError> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            jobs: HashMap::new(),
        })
    }

    /// Add a cron job with timezone support
    pub async fn add_cron_job(
        &mut self,
        name: &str,
        cron_expr: &str,
        timezone: Option<Tz>,
        workflow_path: PathBuf,
    ) -> Result<Uuid, NikaError> {
        let path = workflow_path.clone();

        let job = if let Some(tz) = timezone {
            // Timezone-aware job
            JobBuilder::new()
                .with_timezone(tz)
                .with_cron_job_type()
                .with_schedule(cron_expr)?
                .with_run_async(Box::new(move |uuid, _lock| {
                    let p = path.clone();
                    Box::pin(async move {
                        tracing::info!("Running scheduled job {} for {}", uuid, p.display());
                        if let Err(e) = run_workflow_file(&p).await {
                            tracing::error!("Job {} failed: {}", uuid, e);
                        }
                    })
                }))
                .build()?
        } else {
            // UTC job (default)
            Job::new_async(cron_expr, move |uuid, _lock| {
                let p = path.clone();
                Box::pin(async move {
                    if let Err(e) = run_workflow_file(&p).await {
                        tracing::error!("Job {} failed: {}", uuid, e);
                    }
                })
            })?
        };

        // Add job lifecycle notifications
        let mut job = job;
        job.on_start_notification_add(
            &self.scheduler,
            Box::new(|job_id, _, _| {
                Box::pin(async move {
                    tracing::info!("Job {} started", job_id);
                })
            }),
        ).await?;

        job.on_stop_notification_add(
            &self.scheduler,
            Box::new(|job_id, _, _| {
                Box::pin(async move {
                    tracing::info!("Job {} completed", job_id);
                })
            }),
        ).await?;

        let job_id = self.scheduler.add(job).await?;
        self.jobs.insert(name.to_string(), job_id);

        Ok(job_id)
    }

    /// Add English-syntax job ("every 5 minutes")
    #[cfg(feature = "english")]
    pub async fn add_english_job(
        &mut self,
        name: &str,
        english_expr: &str,  // "every 5 minutes", "every day at 9am"
        workflow_path: PathBuf,
    ) -> Result<Uuid, NikaError> {
        // english-to-cron converts automatically
        self.add_cron_job(name, english_expr, None, workflow_path).await
    }

    pub async fn start(&self) -> Result<(), JobSchedulerError> {
        self.scheduler.start().await
    }

    pub async fn shutdown(&self) -> Result<(), JobSchedulerError> {
        self.scheduler.shutdown().await
    }
}
```

### File Watcher Pattern (notify v6)

```rust
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc;
use std::time::Duration;

pub struct FileWatchTrigger {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
    debounce_ms: u64,
}

impl FileWatchTrigger {
    pub fn new(
        glob_pattern: &str,
        debounce_ms: u64,
    ) -> Result<Self, NikaError> {
        let (tx, rx) = mpsc::channel();

        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default()
                .with_poll_interval(Duration::from_millis(500)),
        )?;

        Ok(Self {
            watcher,
            rx,
            debounce_ms,
        })
    }

    pub fn watch(&mut self, path: &Path) -> Result<(), NikaError> {
        self.watcher.watch(path, RecursiveMode::NonRecursive)?;
        Ok(())
    }

    /// Run event loop with debouncing
    pub async fn run<F, Fut>(
        &self,
        glob: &globset::GlobMatcher,
        handler: F,
    ) where
        F: Fn(PathBuf) -> Fut,
        Fut: Future<Output = ()>,
    {
        let mut last_event: Option<(PathBuf, Instant)> = None;

        loop {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    for path in event.paths {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if glob.is_match(name) {
                                // Debounce: only process if enough time passed
                                let now = Instant::now();
                                let should_process = match &last_event {
                                    Some((p, t)) if p == &path => {
                                        now.duration_since(*t).as_millis() as u64 > self.debounce_ms
                                    }
                                    _ => true,
                                };

                                if should_process {
                                    last_event = Some((path.clone(), now));
                                    handler(path).await;
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("Watch error: {}", e);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}
```

### New Files

```
src/
├── jobs/
│   ├── mod.rs              # Module exports
│   ├── daemon.rs           # Daemon lifecycle (start/stop/status)
│   ├── scheduler.rs        # Job orchestration
│   ├── trigger/
│   │   ├── mod.rs
│   │   ├── cron.rs         # Cron trigger (tokio-cron-scheduler)
│   │   ├── webhook.rs      # Webhook trigger (axum)
│   │   ├── watch.rs        # File watch trigger (notify)
│   │   └── interval.rs     # Interval trigger
│   ├── retry.rs            # Retry logic (backon)
│   ├── state.rs            # SQLite state management
│   ├── notify.rs           # Notification channels
│   └── metrics.rs          # Prometheus metrics
```

---

## 4. CLI DX Enhancements

### 4.1 Global Flags

```rust
#[derive(Parser)]
#[command(name = "nika", version, about)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Control color output
    #[arg(long, default_value = "auto", global = true, value_enum)]
    color: ColorChoice,

    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}
```

**Verbosity Levels:**
| Flag | Level | Output |
|------|-------|--------|
| (none) | 0 | Normal output |
| `-v` | 1 | Info messages |
| `-vv` | 2 | Debug messages |
| `-vvv` | 3 | Trace messages |

### 4.2 Shell Completion

```rust
use clap_complete::{generate, Shell};

#[derive(Subcommand)]
enum Commands {
    /// Generate shell completion scripts
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
    // ...
}

// Handler
Commands::Completion { shell } => {
    generate(shell, &mut Cli::command(), "nika", &mut std::io::stdout());
}
```

**Usage:**
```bash
# Bash
nika completion bash > ~/.local/share/bash-completion/completions/nika

# Zsh
nika completion zsh > ~/.zfunc/_nika
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit

# Fish
nika completion fish > ~/.config/fish/completions/nika.fish

# PowerShell
nika completion powershell >> $PROFILE
```

### 4.3 Config Command

```rust
#[derive(Subcommand)]
enum ConfigAction {
    /// List all configuration values
    List {
        #[arg(long)]
        json: bool,
    },
    /// Get a specific config value
    Get { key: String },
    /// Set a config value
    Set { key: String, value: String },
    /// Open config in $EDITOR
    Edit,
    /// Show config file path
    Path,
    /// Initialize default config
    Init {
        #[arg(long)]
        force: bool,
    },
}
```

**Usage:**
```bash
nika config list                    # Show all settings
nika config list --json             # JSON output
nika config get editor.theme        # Get specific value
nika config set editor.theme dark   # Set value
nika config edit                    # Open in $EDITOR
nika config path                    # Print config path
nika config init                    # Create default config
```

### Crate Dependencies

```toml
[dependencies]
clap_complete = "4.5"
config = "0.14"
directories = "5.0"
```

---

## 5. Enhanced Doctor Command

### CLI Interface

```bash
nika doctor [OPTIONS]

Options:
    --json              Output as JSON
    --fix               Auto-fix repairable issues
    --category <CAT>    Run specific category only
                        (config, providers, mcp, runtime, jobs, network)
```

### Check Categories

| Category | Checks |
|----------|--------|
| `config` | .nika/ exists, config.toml valid, schema version |
| `providers` | API keys present (masked), model availability |
| `mcp` | Servers defined, reachable, tools listing |
| `runtime` | Rust version, disk space, file permissions |
| `jobs` | Daemon status, cron validity, state DB integrity |
| `network` | API endpoints reachable (with timeout) |

### Output Format

```
Nika Doctor
═══════════════════════════════════════════════════════════════════
✓ Config: .nika/config.toml valid (v0.7)
✓ Providers: Claude API key configured (sk-ant-****)
⚠ Providers: OpenAI API key missing (optional)
✓ MCP: 2 servers defined (novanet, perplexity)
✗ MCP: novanet server not responding
  → Check: cargo run --manifest-path ../novanet-mcp/Cargo.toml
✓ Runtime: Rust 1.83.0, 12.4GB free disk
✓ Jobs: Daemon running (PID 12345), 3 jobs scheduled
───────────────────────────────────────────────────────────────────
Summary: 5 passed, 1 warning, 1 error

Run `nika doctor --fix` to auto-repair fixable issues
```

### Auto-Fix Capabilities

| Issue | Fix |
|-------|-----|
| Missing .nika/ directory | Create with defaults |
| Missing config.toml | Generate default |
| Stale PID file | Remove |
| Corrupted jobs.db | Recreate schema |
| Old session files | Clean up |

### Implementation

```rust
pub struct DoctorResult {
    pub category: String,
    pub status: CheckStatus,
    pub message: String,
    pub suggestion: Option<String>,
    pub fixable: bool,
}

pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

pub struct DoctorReport {
    pub results: Vec<DoctorResult>,
    pub passed: usize,
    pub warnings: usize,
    pub errors: usize,
}
```

---

## 6. New Dependencies Summary

```toml
[dependencies]
# Jobs daemon
tokio-cron-scheduler = "0.13"
notify = "6.1"
backon = "1.6"
axum = "0.7"
rusqlite = { version = "0.31", features = ["bundled"] }
metrics = "0.23"
metrics-exporter-prometheus = "0.15"
lettre = "0.11"
fd-lock = "4.0"
chrono-tz = "0.10"

# CLI DX
clap_complete = "4.5"
config = "0.14"
directories = "5.0"
```

---

## 7. Implementation Order

### Phase 1: Foundation (Week 1)

1. **CLI DX** (~4h)
   - Global flags (`--verbose`, `--quiet`, `--color`)
   - Shell completion command
   - Config command

2. **Schema Migration** (~6h)
   - Rename AST types
   - Add backward compatibility
   - Update tests

### Phase 2: Composition (Week 2)

3. **Workflow Composition** (~8h)
   - `include:` DAG fusion
   - `invoke_workflow:` verb
   - Circular detection
   - Integration tests

### Phase 3: Jobs (Week 3-4)

4. **Jobs Daemon** (~16h)
   - Daemon lifecycle (start/stop)
   - Cron trigger
   - Webhook trigger
   - Watch trigger
   - Retry logic
   - State persistence
   - Notifications

### Phase 4: Polish (Week 4)

5. **Enhanced Doctor** (~4h)
   - 6 check categories
   - Auto-fix logic
   - JSON output

6. **Documentation & Testing**
   - Update CLAUDE.md
   - Update CHANGELOG.md
   - Integration tests
   - Example workflows

---

## 8. Testing Strategy

### Unit Tests

Each new module requires comprehensive unit tests:

| Module | Min Tests |
|--------|-----------|
| `ast/context.rs` | 10 |
| `ast/include.rs` | 8 |
| `ast/invoke_workflow.rs` | 8 |
| `jobs/scheduler.rs` | 15 |
| `jobs/trigger/*.rs` | 20 |
| `jobs/retry.rs` | 10 |
| `jobs/state.rs` | 12 |
| `commands/config.rs` | 10 |
| `commands/doctor.rs` | 15 |

**Target:** +100 tests (from 2,997 to ~3,100)

### Integration Tests

```
tests/
├── workflow_composition_test.rs   # include: + invoke_workflow:
├── jobs_daemon_test.rs            # Full daemon lifecycle
├── cli_global_flags_test.rs       # Verbosity, color
├── cli_completion_test.rs         # Shell completion
├── cli_config_test.rs             # Config command
├── doctor_test.rs                 # Doctor checks
└── backward_compat_test.rs        # memory: still works
```

### Real API Tests

```yaml
# examples/test-jobs-cron.nika.yaml
# examples/test-workflow-include.nika.yaml
# examples/test-invoke-workflow.nika.yaml
```

---

## 9. Migration Guide

### For Existing Workflows

```yaml
# 1. Update schema version
schema: nika/workflow@0.7  # was @0.6

# 2. Rename memory: to context: (optional but recommended)
context:  # was memory:
  files:
    brand: ./context/brand.md

# 3. Update template bindings (optional but recommended)
infer: "Using {{context.files.brand}}"  # was {{memory.files.brand}}
```

### Migration Script

```bash
#!/bin/bash
# migrate-v014.sh

# Update schema version
find . -name "*.nika.yaml" -exec sed -i '' 's/workflow@0\.6/workflow@0.7/g' {} \;

# Rename memory: to context:
find . -name "*.nika.yaml" -exec sed -i '' 's/^memory:/context:/g' {} \;

# Update bindings
find . -name "*.nika.yaml" -exec sed -i '' 's/{{memory\./{{context./g' {} \;

echo "Migration complete. Review changes with: git diff"
```

---

## 10. Risk Assessment

| Risk | Mitigation |
|------|------------|
| Breaking existing workflows | Backward compat aliases |
| Jobs daemon complexity | Incremental development, cron-first |
| New dependencies | All community-validated, >1M downloads |
| State DB corruption | SQLite WAL mode, automatic backups |
| Webhook security | Token auth, localhost-only by default |

---

## 11. Success Criteria

- [ ] All 3,100+ tests pass
- [ ] Zero clippy warnings
- [ ] `memory:` workflows still parse (with deprecation warning)
- [ ] `{{memory.*}}` bindings still resolve
- [ ] Jobs daemon starts/stops cleanly
- [ ] Cron jobs execute on schedule
- [ ] Shell completion works for bash/zsh/fish
- [ ] Doctor identifies and fixes common issues
- [ ] CHANGELOG.md updated
- [ ] CLAUDE.md updated

---

## 12. Advanced Architecture & Patterns

This section documents production-grade patterns discovered through research for v0.14 implementation.

### 12.1 Dependencies for v0.14

#### Already Present in Cargo.toml (No Changes Needed)

These crates are already used and should NOT be duplicated:

| Crate | Version | Purpose |
|-------|---------|---------|
| `petgraph` | 0.6 + serde-1 | DAG with StableGraph |
| `tokio` | 1.49 | Async runtime |
| `tokio-util` | 0.7 | CancellationToken |
| `futures` | 0.3.32 | FutureExt::shared() |
| `dashmap` | 6.1 | Concurrent hashmap |
| `parking_lot` | 0.12 | Fast mutexes |
| `smallvec` | 1.13 | Stack arrays |
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 + env-filter | Log formatting |
| `miette` | 7.6 + fancy | Rich diagnostics |
| `thiserror` | 1.0 | Error derive |
| `notify` | 8 | File watching |
| `criterion` | 0.5 + async_tokio | Benchmarks |
| `clap` | 4.5 | CLI |
| `clap_complete` | 4.5 | Shell completion |
| `reqwest` | 0.12 | HTTP client |
| `camino` | 1.1 | UTF-8 paths |

#### NEW Dependencies for v0.14

```toml
[dependencies]
# ═══════════════════════════════════════════════════════════════════════════════
# JOBS DAEMON (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
tokio-cron-scheduler = { version = "0.15", features = ["english"] }  # Cron scheduling
backon = "1.6"                      # Retry with exponential backoff + jitter
fd-lock = "4.0"                     # PID file locking for daemon
chrono-tz = "0.10"                  # Timezone support for cron expressions

# ═══════════════════════════════════════════════════════════════════════════════
# WEBHOOK SERVER (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
axum = "0.7"                        # Web framework for webhook endpoints
tower = "0.4"                       # Middleware framework
tower-http = { version = "0.5", features = ["trace", "cors", "limit"] }
tower-governor = "0.4"              # Rate limiting middleware
hyper = { version = "1.5", features = ["server"] }

# ═══════════════════════════════════════════════════════════════════════════════
# WEBHOOK SECURITY (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
hmac = "0.12"                       # HMAC-SHA256 signature verification
sha2 = "0.10"                       # SHA-256 for webhook signatures
hex = "0.4"                         # Hex encoding for signatures
secrecy = "0.8"                     # Protect secrets in memory (zeroize on drop)

# ═══════════════════════════════════════════════════════════════════════════════
# STATE PERSISTENCE (NEW - for Jobs history)
# ═══════════════════════════════════════════════════════════════════════════════
rusqlite = { version = "0.31", features = ["bundled", "backup"] }

# ═══════════════════════════════════════════════════════════════════════════════
# LAYERED CONFIGURATION (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
figment = { version = "0.10", features = ["toml", "env", "yaml"] }

# ═══════════════════════════════════════════════════════════════════════════════
# OBSERVABILITY - OpenTelemetry (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
tracing-opentelemetry = "0.24"      # Bridge tracing → OpenTelemetry
opentelemetry = "0.23"              # OpenTelemetry API
opentelemetry-otlp = "0.16"         # OTLP exporter

# ═══════════════════════════════════════════════════════════════════════════════
# OBSERVABILITY - Prometheus (NEW)
# ═══════════════════════════════════════════════════════════════════════════════
metrics = "0.23"                    # Metrics API
metrics-exporter-prometheus = "0.15" # Prometheus exporter

# ═══════════════════════════════════════════════════════════════════════════════
# EMAIL NOTIFICATIONS (OPTIONAL)
# ═══════════════════════════════════════════════════════════════════════════════
lettre = { version = "0.11", features = ["tokio1-native-tls"], optional = true }

# ═══════════════════════════════════════════════════════════════════════════════
# MEMORY OPTIMIZATION (OPTIONAL)
# ═══════════════════════════════════════════════════════════════════════════════
arrayvec = "0.7"                    # Fixed-capacity stack arrays (complement smallvec)

[dev-dependencies]
# Already have: criterion, proptest, insta, tempfile, wiremock
tokio-test = "0.4"                  # Async test utilities (NEW)
```

#### Summary: v0.14 New Crates

| Category | Crates | Count |
|----------|--------|-------|
| Jobs Daemon | tokio-cron-scheduler, backon, fd-lock, chrono-tz | 4 |
| Webhook Server | axum, tower, tower-http, tower-governor, hyper | 5 |
| Webhook Security | hmac, sha2, hex, secrecy | 4 |
| State Persistence | rusqlite | 1 |
| Configuration | figment | 1 |
| OpenTelemetry | tracing-opentelemetry, opentelemetry, opentelemetry-otlp | 3 |
| Prometheus | metrics, metrics-exporter-prometheus | 2 |
| **Total NEW** | | **20** |

---

### 12.2 Async DAG Task Execution Pattern

Based on [w-graj.net](https://w-graj.net/posts/rust-async-task-graph/) research. Uses `FutureExt::shared()` for DAGs where multiple tasks depend on the same parent.

```rust
//! DAG Task Execution with Shared Futures
//!
//! Pattern: tokio::spawn + FutureExt::shared() for true parallel DAG execution

use futures::{Future, FutureExt, TryFutureExt};
use std::sync::Arc;
use tokio::task::JoinError;

/// Task trait: cloneable, fallible future
pub trait Task<R, E>: Future<Output = Result<R, E>> + Clone + Send + 'static {}
impl<T, R, E> Task<R, E> for T where T: Future<Output = Result<R, E>> + Clone + Send + 'static {}

/// Spawn a task that can be awaited by multiple dependents
pub fn spawn_shared<F, R, E>(future: F) -> impl Task<R, E>
where
    F: Future<Output = Result<R, E>> + Send + 'static,
    R: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + From<JoinError> + 'static,
{
    tokio::spawn(future).map(|r| r?).shared()
}

/// Spawn blocking work that can be awaited by multiple dependents
pub fn spawn_blocking_shared<F, R, E>(f: F) -> impl Task<R, E>
where
    F: FnOnce() -> Result<R, E> + Send + 'static,
    R: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + From<JoinError> + 'static,
{
    tokio::task::spawn_blocking(f).map(|r| r?).shared()
}

/// Example: Execute DAG with shared dependencies
///
/// ```text
///       ┌─── B ───┐
///   A ──┤         ├── D
///       └─── C ───┘
///            │
///            └────── E
/// ```
pub async fn execute_dag() -> Result<(), NikaError> {
    // A has no dependencies - spawn immediately
    let a = spawn_shared(task_a());

    // B and C depend on A - clone the shared future
    let b = {
        let a = a.clone();
        spawn_shared(async move {
            let a_result = a.await?;
            task_b(&a_result).await
        })
    };

    let c = {
        let a = a.clone();
        spawn_shared(async move {
            let a_result = a.await?;
            task_c(&a_result).await
        })
    };

    // D depends on both B and C
    let d = {
        let b = b.clone();
        let c = c.clone();
        spawn_shared(async move {
            let (b_result, c_result) = tokio::try_join!(b, c)?;
            task_d(&b_result, &c_result).await
        })
    };

    // E depends on C only
    let e = spawn_shared(async move {
        let c_result = c.await?;
        task_e(&c_result).await
    });

    // Wait for all leaf nodes
    tokio::try_join!(d, e)?;

    Ok(())
}

/// Non-Clone results: Wrap in Arc
pub async fn execute_with_arc<T: Send + Sync + 'static>(
    producer: impl Future<Output = Result<T, NikaError>> + Send + 'static,
) -> impl Task<Arc<T>, NikaError> {
    spawn_shared(producer.map_ok(Arc::new))
}
```

---

### 12.3 Graceful Shutdown Coordinator

Production-ready shutdown coordination based on [OneUptime patterns](https://oneuptime.com/blog/post/2026-01-07-rust-graceful-shutdown/view).

```rust
//! Graceful Shutdown Coordinator
//!
//! Features:
//! - SIGTERM + Ctrl+C handling
//! - Connection draining with timeout
//! - Background task coordination
//! - Kubernetes-compatible health probes

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{broadcast, Notify};

/// Shutdown coordinator for graceful termination
pub struct ShutdownCoordinator {
    /// Broadcast channel for shutdown signal
    notify_shutdown: broadcast::Sender<()>,
    /// Atomic flag for quick shutdown checks
    is_shutting_down: AtomicBool,
    /// Active connection counter
    active_connections: AtomicUsize,
    /// Notifier when all connections drained
    all_drained: Notify,
}

impl ShutdownCoordinator {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(1);
        Arc::new(Self {
            notify_shutdown: tx,
            is_shutting_down: AtomicBool::new(false),
            active_connections: AtomicUsize::new(0),
            all_drained: Notify::new(),
        })
    }

    /// Subscribe to shutdown notifications
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.notify_shutdown.subscribe()
    }

    /// Check if shutdown in progress (fast path)
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::SeqCst)
    }

    /// Trigger shutdown sequence
    pub fn trigger(&self) {
        self.is_shutting_down.store(true, Ordering::SeqCst);
        let _ = self.notify_shutdown.send(());
    }

    /// Increment active connections
    pub fn connection_started(&self) {
        self.active_connections.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement active connections
    pub fn connection_ended(&self) {
        let prev = self.active_connections.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.all_drained.notify_waiters();
        }
    }

    /// Get active connection count
    pub fn active_count(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }

    /// Wait for all connections to drain (with timeout)
    pub async fn wait_for_drain(&self, timeout: Duration) -> bool {
        if self.active_count() == 0 {
            return true;
        }

        tokio::select! {
            _ = self.all_drained.notified() => true,
            _ = tokio::time::sleep(timeout) => {
                tracing::warn!(
                    active = self.active_count(),
                    "Timeout waiting for connections to drain"
                );
                false
            }
        }
    }
}

/// RAII guard for connection tracking
pub struct ConnectionGuard {
    coordinator: Arc<ShutdownCoordinator>,
}

impl ConnectionGuard {
    pub fn new(coordinator: Arc<ShutdownCoordinator>) -> Self {
        coordinator.connection_started();
        Self { coordinator }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.coordinator.connection_ended();
    }
}

/// Wait for shutdown signal (SIGTERM or Ctrl+C)
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C"),
        _ = terminate => tracing::info!("Received SIGTERM"),
    }
}

/// Background task manager with graceful shutdown
pub struct BackgroundTaskManager {
    shutdown_tx: broadcast::Sender<()>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl BackgroundTaskManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self { shutdown_tx: tx, tasks: Vec::new() }
    }

    /// Spawn a task that respects shutdown
    pub fn spawn<F, Fut>(&mut self, name: &'static str, f: F)
    where
        F: FnOnce(broadcast::Receiver<()>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let rx = self.shutdown_tx.subscribe();
        let handle = tokio::spawn(async move {
            tracing::debug!(task = name, "Background task started");
            f(rx).await;
            tracing::debug!(task = name, "Background task stopped");
        });
        self.tasks.push(handle);
    }

    /// Shutdown all tasks with timeout
    pub async fn shutdown(self, timeout: Duration) {
        let _ = self.shutdown_tx.send(());

        let shutdown_future = async {
            for handle in self.tasks {
                let _ = handle.await;
            }
        };

        if tokio::time::timeout(timeout, shutdown_future).await.is_err() {
            tracing::warn!("Background tasks did not complete within timeout");
        }
    }
}
```

---

### 12.4 Quality Gates & CI Pipeline

```yaml
# .github/workflows/quality-gates.yml
name: Quality Gates

on: [push, pull_request]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-Dwarnings"

jobs:
  # ═══════════════════════════════════════════════════════════════════════════
  # TIER 1: Fast checks (< 2 min)
  # ═══════════════════════════════════════════════════════════════════════════
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Format check
        run: cargo fmt --all -- --check

      - name: Clippy (strict)
        run: cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic

      - name: Doc warnings
        run: RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps

  # ═══════════════════════════════════════════════════════════════════════════
  # TIER 2: Security (< 5 min)
  # ═══════════════════════════════════════════════════════════════════════════
  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-audit
        run: cargo install cargo-audit

      - name: Install cargo-deny
        run: cargo install cargo-deny

      - name: Audit dependencies
        run: cargo audit

      - name: Deny check (licenses, bans, advisories)
        run: cargo deny check

  # ═══════════════════════════════════════════════════════════════════════════
  # TIER 3: Tests (< 10 min)
  # ═══════════════════════════════════════════════════════════════════════════
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Install nextest
        run: cargo install cargo-nextest --locked

      - name: Run tests (parallel)
        run: cargo nextest run --all-features --profile ci

      - name: Doc tests
        run: cargo test --doc

  # ═══════════════════════════════════════════════════════════════════════════
  # TIER 4: Coverage (< 15 min)
  # ═══════════════════════════════════════════════════════════════════════════
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview

      - name: Install llvm-cov
        run: cargo install cargo-llvm-cov --locked

      - name: Generate coverage
        run: cargo llvm-cov nextest --all-features --lcov --output-path lcov.info

      - name: Upload to Codecov
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: true

  # ═══════════════════════════════════════════════════════════════════════════
  # TIER 5: Mutation Testing (weekly, optional)
  # ═══════════════════════════════════════════════════════════════════════════
  mutation:
    runs-on: ubuntu-latest
    if: github.event_name == 'schedule'
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-mutants
        run: cargo install cargo-mutants --locked

      - name: Run mutation testing (src/jobs/ only)
        run: cargo mutants --package nika -- --lib -j 4
        continue-on-error: true
```

```toml
# deny.toml - cargo-deny configuration
[advisories]
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "ISC", "Zlib", "MPL-2.0"]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = [
    # Known problematic crates
    { name = "openssl", wrappers = ["native-tls"] },
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

---

### 12.5 Observability Stack

```rust
//! Production Observability Setup
//!
//! Stack: tracing + OpenTelemetry + Prometheus

use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize full observability stack
pub fn init_observability(service_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. OpenTelemetry tracer (OTLP export)
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    // 2. Tracing subscriber with layers
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,nika=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    Ok(())
}

/// Prometheus metrics endpoint
pub async fn metrics_handler() -> impl axum::response::IntoResponse {
    use metrics_exporter_prometheus::PrometheusHandle;

    // Get global handle (set during init)
    let handle: &PrometheusHandle = todo!("Get from state");
    handle.render()
}

/// Key metrics to track
pub fn register_metrics() {
    use metrics::{describe_counter, describe_histogram, describe_gauge};

    // Workflow metrics
    describe_counter!("nika_workflows_total", "Total workflows executed");
    describe_counter!("nika_workflows_failed", "Total failed workflows");
    describe_histogram!("nika_workflow_duration_seconds", "Workflow execution duration");

    // Task metrics
    describe_counter!("nika_tasks_total", "Total tasks executed");
    describe_histogram!("nika_task_duration_seconds", "Task execution duration");

    // Jobs daemon metrics
    describe_gauge!("nika_jobs_active", "Currently running jobs");
    describe_counter!("nika_jobs_scheduled_total", "Total scheduled job executions");
    describe_counter!("nika_jobs_retry_total", "Total job retries");

    // Provider metrics
    describe_counter!("nika_provider_calls_total", "LLM provider API calls");
    describe_histogram!("nika_provider_latency_seconds", "LLM provider latency");
    describe_counter!("nika_provider_tokens_total", "Total tokens consumed");
}
```

---

### 12.6 Configuration with Figment

```rust
//! Layered Configuration with Figment
//!
//! Priority (highest to lowest):
//! 1. Environment variables (NIKA_*)
//! 2. Local config (.nika/config.toml)
//! 3. User config (~/.config/nika/config.toml)
//! 4. Defaults

use figment::{providers::{Env, Format, Toml, Serialized}, Figment};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NikaConfig {
    pub editor: EditorConfig,
    pub jobs: JobsConfig,
    pub providers: ProvidersConfig,
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JobsConfig {
    pub enabled: bool,
    pub pid_file: PathBuf,
    pub state_db: PathBuf,
    pub log_dir: PathBuf,
    pub webhook_port: u16,
    pub webhook_bind: String,
    pub metrics_port: u16,
    pub drain_timeout_secs: u64,
}

impl Default for JobsConfig {
    fn default() -> Self {
        let home = directories::BaseDirs::new()
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        Self {
            enabled: true,
            pid_file: home.join("nika/jobs.pid"),
            state_db: home.join("nika/jobs.db"),
            log_dir: home.join("nika/logs"),
            webhook_port: 8080,
            webhook_bind: "127.0.0.1".to_string(),
            metrics_port: 9090,
            drain_timeout_secs: 30,
        }
    }
}

/// Load configuration with Figment
pub fn load_config() -> Result<NikaConfig, figment::Error> {
    let home_config = directories::BaseDirs::new()
        .map(|d| d.config_dir().join("nika/config.toml"))
        .unwrap_or_default();

    Figment::new()
        // 1. Defaults
        .merge(Serialized::defaults(NikaConfig::default()))
        // 2. User config (~/.config/nika/config.toml)
        .merge(Toml::file(&home_config).nested())
        // 3. Local config (.nika/config.toml)
        .merge(Toml::file(".nika/config.toml").nested())
        // 4. Environment (NIKA_JOBS_ENABLED, NIKA_WEBHOOK_PORT, etc.)
        .merge(Env::prefixed("NIKA_").split("_"))
        .extract()
}
```

---

### 12.7 Performance Targets & Benchmarks

| Metric | Target | Measured | Notes |
|--------|--------|----------|-------|
| YAML parse (1 task) | < 10µs | ~4.6µs | serde_yaml |
| YAML parse (100 tasks) | < 500µs | ~340µs | |
| DAG toposort (10 nodes) | < 1µs | ~800ns | petgraph |
| DAG toposort (100 nodes) | < 50µs | TBD | |
| Binding resolution | < 1µs | ~450ns | |
| DataStore get | < 10ns | ~6ns | DashMap |
| Cron parse | < 5µs | TBD | tokio-cron-scheduler |
| Webhook HMAC verify | < 100µs | TBD | hmac-sha256 |
| Graceful shutdown | < 30s | configurable | drain timeout |

**Memory targets:**
- Idle daemon: < 50 MB RSS
- Per-workflow overhead: < 1 MB
- Per-task overhead: < 10 KB

---

### 12.8 State Machine Pattern (Optional)

For complex job states, consider `statig` crate:

```rust
use statig::prelude::*;

#[derive(Default)]
pub struct JobStateMachine;

pub enum Event {
    Start,
    Complete,
    Fail { reason: String },
    Retry,
    Cancel,
}

#[state_machine(
    initial = "State::pending()",
    state(derive(Debug, Clone)),
    on_transition = "Self::on_transition",
)]
impl JobStateMachine {
    #[state]
    fn pending(&self, event: &Event) -> Response<State> {
        match event {
            Event::Start => Transition(State::running()),
            Event::Cancel => Transition(State::cancelled()),
            _ => Super,
        }
    }

    #[state]
    fn running(&self, event: &Event) -> Response<State> {
        match event {
            Event::Complete => Transition(State::completed()),
            Event::Fail { reason } => Transition(State::failed(reason.clone())),
            Event::Cancel => Transition(State::cancelled()),
            _ => Super,
        }
    }

    #[state]
    fn failed(&self, reason: String, event: &Event) -> Response<State> {
        match event {
            Event::Retry => Transition(State::pending()),
            _ => Super,
        }
    }

    #[state]
    fn completed(&self) -> Response<State> { Super }

    #[state]
    fn cancelled(&self) -> Response<State> { Super }

    fn on_transition(&mut self, from: &State, to: &State) {
        tracing::info!(?from, ?to, "Job state transition");
    }
}
```

---

## 13. Advanced Example Workflows

### 13.1 Multi-Stage CI/CD Pipeline

```yaml
# examples/advanced/ci-cd-pipeline.nika.yaml
schema: nika/workflow@0.7
workflow: ci-cd-pipeline
description: Multi-stage CI/CD with parallel jobs and rollback

context:
  files:
    deploy_config: ./.nika/context/deploy.yaml

mcp:
  servers:
    github:
      command: npx
      args: ["-y", "@anthropic/mcp-server-github"]
    slack:
      command: npx
      args: ["-y", "@anthropic/mcp-server-slack"]

tasks:
  # ═══════════════════════════════════════════════════════════════════════════
  # STAGE 1: Build & Test (parallel)
  # ═══════════════════════════════════════════════════════════════════════════
  - id: lint
    exec: "cargo clippy --all-targets -- -D warnings"
    use.lint_result: result

  - id: test
    exec: "cargo nextest run --all-features"
    use.test_result: result

  - id: build
    exec: "cargo build --release"
    depends_on: [lint, test]
    use.build_artifact: result

  # ═══════════════════════════════════════════════════════════════════════════
  # STAGE 2: Deploy to environments (sequential)
  # ═══════════════════════════════════════════════════════════════════════════
  - id: deploy_staging
    depends_on: [build]
    exec: "kubectl apply -f k8s/staging/"
    use.staging_result: result

  - id: smoke_test
    depends_on: [deploy_staging]
    agent:
      prompt: |
        Run smoke tests against staging environment.
        Check health endpoint, run critical path tests.
        Report pass/fail with details.
      mcp: [github]
      max_turns: 5
    use.smoke_result: result

  - id: deploy_production
    depends_on: [smoke_test]
    exec: "kubectl apply -f k8s/production/"
    use.prod_result: result

  # ═══════════════════════════════════════════════════════════════════════════
  # STAGE 3: Notification
  # ═══════════════════════════════════════════════════════════════════════════
  - id: notify_success
    depends_on: [deploy_production]
    invoke:
      server: slack
      tool: send_message
      params:
        channel: "#deployments"
        text: "✅ Deployment successful: {{context.files.deploy_config.version}}"

  - id: notify_failure
    on_failure: true  # Only runs if pipeline fails
    invoke:
      server: slack
      tool: send_message
      params:
        channel: "#deployments"
        text: "❌ Deployment failed. Check logs."
```

### 13.2 Parallel Entity Generation

```yaml
# examples/advanced/parallel-entity-gen.nika.yaml
schema: nika/workflow@0.7
workflow: parallel-entity-generation
description: Generate content for multiple entities across locales

context:
  files:
    brand: ./.nika/context/brand.md
    style_guide: ./.nika/context/style-guide.md

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "../novanet-mcp/Cargo.toml"]

tasks:
  # Fetch entities from NovaNet
  - id: get_entities
    invoke:
      server: novanet
      tool: novanet_traverse
      params:
        start: "project:qrcode-ai"
        arc: "HAS_ENTITY"
        limit: 50
    use.entities: result.nodes

  # Parallel generation across entities x locales
  - id: generate_all
    depends_on: [get_entities]
    for_each: "{{use.entities}}"
    as: entity
    concurrency: 10
    fail_fast: false

    # Nested for_each for locales
    tasks:
      - id: generate_locales
        for_each: ["fr-FR", "en-US", "de-DE", "es-ES", "ja-JP"]
        as: locale
        concurrency: 5

        agent:
          prompt: |
            Generate native content for entity "{{use.entity.key}}" in locale {{use.locale}}.

            Brand guidelines: {{context.files.brand}}
            Style guide: {{context.files.style_guide}}

            Output JSON with: title, description, meta_title, meta_description
          mcp: [novanet]
          max_turns: 3
        use.content: result

  # Aggregate results
  - id: save_results
    depends_on: [generate_all]
    exec: |
      echo '{{use.content | json}}' > output/generated-content.json
```

### 13.3 Scheduled Jobs Configuration

```toml
# .nika/config.toml - Jobs section

[jobs]
enabled = true
pid_file = "~/.nika/jobs.pid"
state_db = "~/.nika/jobs.db"

[jobs.webhook]
enabled = true
port = 8080
bind = "127.0.0.1"
secret = "${NIKA_WEBHOOK_SECRET}"

[jobs.notify]
on_failure = ["slack", "email"]
slack_webhook = "${SLACK_WEBHOOK_URL}"
email_to = "alerts@example.com"

# ═══════════════════════════════════════════════════════════════════════════
# JOB DEFINITIONS
# ═══════════════════════════════════════════════════════════════════════════

[[jobs.definitions]]
name = "daily-content-sync"
workflow = "./workflows/content-sync.nika.yaml"
trigger = { cron = "0 9 * * *", timezone = "Europe/Paris" }
retry = { max_attempts = 3, backoff = "exponential" }
timeout = "30m"
on_success = []
on_failure = ["notify:slack"]

[[jobs.definitions]]
name = "hourly-health-check"
workflow = "./workflows/health-check.nika.yaml"
trigger = { interval = "1h" }
timeout = "5m"

[[jobs.definitions]]
name = "webhook-deploy"
workflow = "./workflows/deploy.nika.yaml"
trigger = { webhook = "/hooks/deploy", method = "POST" }
params = { payload = "{{trigger.body}}" }

[[jobs.definitions]]
name = "watch-uploads"
workflow = "./workflows/process-upload.nika.yaml"
trigger = { watch = "./uploads/*.json", debounce = "5s" }
params = { file = "{{trigger.path}}" }

[[jobs.definitions]]
name = "weekly-report"
workflow = "./workflows/weekly-report.nika.yaml"
trigger = { cron = "every monday at 9am", timezone = "Europe/Paris" }
enabled = true
```

---

## 14. CRITICAL: Pre-v0.14 Dependency Updates

> **⚠️ MUST FIX BEFORE v0.14 DEVELOPMENT BEGINS**

### 14.1 serde_yaml DEPRECATED → serde-saphyr

**Finding:** `serde_yaml v0.9.34+deprecated` is marked as deprecated.

**Root cause:** The original maintainer archived the repository. No security updates.

**Solution:** Migrate to `serde-saphyr` — a drop-in replacement with:
- Safe Rust (no unsafe code)
- Better error messages
- Same API

```toml
# Cargo.toml - BEFORE
[dependencies]
serde_yaml = "0.9"

# Cargo.toml - AFTER
[dependencies]
serde-saphyr = "0.0.20"  # Drop-in replacement
```

```rust
// Code changes - BEFORE
use serde_yaml;
let config: Config = serde_yaml::from_str(yaml_str)?;
let yaml_out = serde_yaml::to_string(&config)?;

// Code changes - AFTER
use serde_saphyr as serde_yaml;  // Alias for minimal changes
let config: Config = serde_yaml::from_str(yaml_str)?;
let yaml_out = serde_yaml::to_string(&config)?;
```

**Impact:** ~15 files use serde_yaml. Find with: `grep -r "serde_yaml" src/`

---

### 14.2 Security Advisories (cargo audit)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  RUSTSEC-2024-0436  │  paste 1.0.15 - UNMAINTAINED                            ║
║  RUSTSEC-2026-0002  │  lru 0.12.5 - UNSOUND (Stacked Borrows violation)       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║  Root cause: tui-textarea 0.7 depends on ratatui 0.29.0                       ║
║  We use: ratatui 0.30.0 (version mismatch causes vulnerable transitives)      ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Solution:** Replace `tui-textarea` with maintained fork `tui-textarea-2`:

```toml
# Cargo.toml - BEFORE
[dependencies]
tui-textarea = { version = "0.7", optional = true }
tui-input = { version = "0.11", features = ["crossterm"], optional = true }

# Cargo.toml - AFTER
tui-textarea = { package = "tui-textarea-2", version = "0.10", optional = true }
tui-input = { version = "0.15", features = ["crossterm"], optional = true }
```

**Benefits of tui-textarea-2 v0.10:**
- ratatui 0.30+ support (matches our version)
- Removes `paste` and `lru` vulnerable dependencies
- Actively maintained fork

---

### 14.3 Summary: Immediate Cargo.toml Changes

```toml
# ═══════════════════════════════════════════════════════════════════════════════
# FIXES REQUIRED BEFORE v0.14 DEVELOPMENT
# ═══════════════════════════════════════════════════════════════════════════════

[dependencies]
# YAML parsing - REPLACE serde_yaml (deprecated)
serde-saphyr = "0.0.20"

# TUI - UPDATE to fix security advisories
tui-textarea = { package = "tui-textarea-2", version = "0.10", optional = true }
tui-input = { version = "0.15", features = ["crossterm"], optional = true }
```

---

### 14.4 Validation: Current Choices Are Optimal

Research confirmed these crates remain the best choices for 2026:

| Crate | Status | Research Notes |
|-------|--------|----------------|
| **tokio** | ✅ Optimal | Dominant async runtime; async-std has faded |
| **petgraph** | ✅ Optimal | No better alternative; StableGraph perfect for DAGs |
| **miette** | ✅ Optimal | 2026 community favorite for CLI error handling |
| **axum** | ✅ Optimal | Tower ecosystem integration better than actix-web |
| **figment** | ✅ Optimal | Best layered config; supports TOML/YAML/ENV |
| **tokio-cron-scheduler** | ✅ Optimal | Best async cron; supports timezone + English syntax |
| **dashmap** | ✅ Optimal | Fastest concurrent hashmap |
| **parking_lot** | ✅ Optimal | Faster than std::sync::Mutex |

---

### 14.5 Migration Checklist

```
□ Replace serde_yaml with serde-saphyr in Cargo.toml
□ Add `use serde_saphyr as serde_yaml;` alias in affected files
□ Update tui-textarea to tui-textarea-2 v0.10
□ Update tui-input from 0.11 to 0.15
□ Run cargo audit - verify zero advisories
□ Run cargo test - verify 2,997 tests still pass
□ Run cargo clippy - verify zero warnings
□ Commit with: "fix(deps): migrate deprecated/vulnerable crates"
```

---

## References

### Crate Documentation
- [serde-saphyr](https://docs.rs/serde-saphyr/) - serde_yaml drop-in replacement (safe Rust)
- [tui-textarea-2](https://crates.io/crates/tui-textarea-2) - Maintained fork with ratatui 0.30+
- [BackON Crate](https://docs.rs/backon/) - Retry with backoff
- [tokio-cron-scheduler](https://docs.rs/tokio-cron-scheduler/) - Cron scheduling
- [notify Crate](https://docs.rs/notify/) - File watching
- [petgraph](https://docs.rs/petgraph/) - Graph algorithms
- [Figment](https://docs.rs/figment/) - Layered configuration
- [DashMap](https://docs.rs/dashmap/) - Concurrent HashMap
- [miette](https://docs.rs/miette/) - Rich error diagnostics
- [statig](https://docs.rs/statig/) - Async state machines
- [rust-logic-graph](https://lib.rs/crates/rust-logic-graph) - Workflow engine patterns
- [w-graj.net: Async Task Graphs](https://w-graj.net/posts/rust-async-task-graph/) - DAG execution patterns
- [OneUptime: Graceful Shutdown](https://oneuptime.com/blog/post/2026-01-07-rust-graceful-shutdown/view) - Shutdown patterns
- [cargo-nextest](https://nexte.st/) - Fast parallel test runner
- [cargo-llvm-cov](https://docs.rs/cargo-llvm-cov/) - Code coverage
- [cargo-mutants](https://docs.rs/cargo-mutants/) - Mutation testing
- [Claude Code CLI](https://docs.anthropic.com/claude-code) - Doctor patterns
- [Temporal.io](https://temporal.io/) - Workflow engine patterns
- [Prefect](https://www.prefect.io/) - Job scheduling patterns
