# v0.48 Daemon — Remaining Issues Handoff

> Give this ENTIRE file to a new Claude Code session.

---

## SITUATION REPORT

### Previous Session (v0.48 — 80+ commits)

**Built from scratch:**
- `nika-daemon` crate: 4,800 LOC, 113 tests, 12th workspace crate
- 5 services: secrets, jobs, watch, cache, events
- Full CLI: `nika daemon start/stop/restart/status/logs/install/uninstall`
- Full CLI: `nika job submit/list/status/cancel/retry/history`
- Full CLI: `nika cache stats/clear`
- TUI: daemon job dashboard in Control view

**Discovery fixes completed: 22/29**
- All 4 CRITICAL, all 10 HIGH done
- 8 of 15 MEDIUM done
- reqwest 0.13, git2 removed, thiserror 2.0, EventLog cap, InvokeBox cached JSON

**Architecture:**
- ARCH-3 phases 1-3: domain error sub-enums (Provider, DAG, Binding migrated)
- LSP: 3 features migrated from nika-engine to nika-lsp-core (-1,884 lines)
- pub(crate) visibility: 72 items narrowed

**Security fixes from architect review:**
- C1: workflow path validation, C2: PID file exclusive creation
- H1: Semaphore max_connections, H4: UTF-8 safe truncation
- H5: watch HashMap pruning, H6: 1-hour job timeout, L5: cancel race

### Current State
- **HEAD**: `5acbd1c20` on `main`
- **Tests**: 4,744 passing (113 daemon + 3,581 engine + 133 event + 2,120 TUI + others)
- **Clippy**: 0 warnings (all-targets, all-features)
- **Version**: 0.47.1 published on crates.io + GitHub
- **Workspace**: 12 crates

---

## WHAT THIS SESSION MUST DO

### Phase 1: Fix 7 Review Items (MEDIUM/LOW)

#### M1. Silent serde_json error in server routing (30 min)

**File**: `nika-daemon/src/server.rs:260,279,329`

**Problem**: `serde_json::to_value(j).unwrap_or_default()` silently replaces serialization failures with `Value::Null`. Jobs that fail to serialize disappear from listings.

**Fix**:
```rust
// Before (server.rs line 260)
.map(|j| serde_json::to_value(j).unwrap_or_default())

// After
.filter_map(|j| match serde_json::to_value(j) {
    Ok(v) => Some(v),
    Err(e) => {
        tracing::warn!(error = %e, "failed to serialize job");
        None
    }
})
```

Apply to all 3 locations (JobList, JobDetail, JobHistoryList responses).

**Test**: Add a test that verifies JobList response serializes all jobs correctly.

---

#### M3. Cache key hash collision between None and empty string (30 min)

**File**: `nika-daemon/src/services/cache.rs:82-98`

**Problem**: `compute_key("p", "m", "hello", None, ...)` and `compute_key("p", "m", "hello", Some(""), ...)` produce the same blake3 hash because nothing is hashed for `None`.

**Fix**: Hash a discriminant byte before optional fields:
```rust
// Before
if let Some(sys) = system {
    hasher.update(sys.as_bytes());
}
hasher.update(b"|");

// After
match system {
    Some(sys) => {
        hasher.update(&[0x01]); // discriminant: Some
        hasher.update(sys.as_bytes());
    }
    None => {
        hasher.update(&[0x00]); // discriminant: None
    }
}
hasher.update(b"|");
```

Apply same pattern to `temperature` (hash f64 bytes vs `[0x00]`) and `max_tokens`.

**Test**:
```rust
#[test]
fn cache_key_none_vs_empty_string_differ() {
    let k1 = CacheService::compute_key("p", "m", "hi", None, None, None);
    let k2 = CacheService::compute_key("p", "m", "hi", Some(""), None, None);
    assert_ne!(k1, k2); // Currently fails!
}
```

---

#### M4. WatchStart/WatchStop return Ok but do nothing (1 hour)

**File**: `nika-daemon/src/server.rs:347-354` + `nika-daemon/src/services/watch.rs`

**Problem**: The Watch service exists in `services/watch.rs` but is never wired into the server. `WatchStart` returns `Ok` but does nothing — the client is lied to.

**Fix**:
1. Add `watch_service: Option<WatchService>` to `ServerState`
2. On `WatchStart { dir, patterns }`:
   - Create a `WatchConfig` from the request
   - Start a `WatchService` and store in state
   - Spawn a task that reads `next_event()` and publishes to `EventBus`
   - Return `WatchActive { dir, patterns }`
3. On `WatchStop`: drop the WatchService
4. On `WatchStatus`: return `WatchActive` or `WatchInactive` based on state

**Challenge**: `ServerState` is behind `Arc` (shared across handlers). The WatchService needs `&mut self` for `next_event()`. Solution: wrap in `tokio::sync::Mutex<Option<WatchService>>`.

**Test**: Integration test that starts watch via IPC, creates a .nika.yaml file, verifies WatchTriggered event.

---

#### M5. EventSubscribe returns Ok without streaming (2 hours)

**File**: `nika-daemon/src/server.rs:407-411` + `nika-daemon/src/protocol.rs`

**Problem**: The current protocol is request-response (one message per connection). EventSubscribe can't stream events because the connection closes after one response.

**Fix** — Two approaches:

**Option A (Simple)**: Long-poll pattern
- `EventSubscribe` holds the connection open
- Server reads from `EventBus::subscribe()` and writes `Event { event }` messages continuously
- Client reconnects on timeout/error
- Change `handle_connection` to detect EventSubscribe and enter a streaming loop

**Option B (Clean)**: Separate event socket
- Daemon listens on a second socket `~/.nika/daemon/events.sock`
- Clients connect to events socket for streaming
- Main socket remains request-response

Recommend **Option A** — simpler, works within existing protocol:

```rust
async fn handle_connection(stream: UnixStream, state: &ServerState) -> DaemonResult<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let request: DaemonRequest = decode_message(&mut reader).await?;

    match request {
        DaemonRequest::EventSubscribe => {
            // Enter streaming mode
            let mut rx = state.event_bus.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let json = serde_json::to_value(&event).unwrap_or_default();
                        write_message(&mut writer, &DaemonResponse::Event { event: json }).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("event subscriber lagged by {n}");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            Ok(())
        }
        other => {
            let response = route_request(other, state).await;
            write_message(&mut writer, &response).await
        }
    }
}
```

**Test**: Spawn server, subscribe to events, submit a job, verify JobSubmitted event received.

---

#### M6. Missing cfg(unix) on daemonize (5 min)

**File**: `nika-daemon/src/lifecycle.rs:137`

**Problem**: The self-exec `daemonize()` function at line 137 has no `#[cfg(unix)]` attribute. The `#[cfg(not(unix))]` fallback at line 164 would conflict on non-Unix. Currently works because we only target macOS/Linux.

**Fix**: Add `#[cfg(unix)]` to the first `daemonize` function. This is a 1-line fix:
```rust
#[cfg(unix)]  // ← add this
pub fn daemonize(log_path: &std::path::Path) -> DaemonResult<()> {
```

Wait — actually check if it already has `#[cfg(unix)]` or not. The function uses `std::process::Command` which is cross-platform. The issue might be a false positive from the reviewer. Read the actual code to confirm before changing.

---

#### M8. cron dependency declared but unused (1 hour if implementing, 5 min if removing)

**File**: `nika-daemon/Cargo.toml:53` + `nika-daemon/src/services/jobs.rs`

**Problem**: `cron = "0.13"` is in Cargo.toml but no cron scheduling is implemented. The `cron` field on jobs is stored in the DB but never used to trigger re-runs.

**Option A**: Remove `cron` dep and `cron` field from jobs (if we don't need it yet)

**Option B**: Implement cron scheduler loop:
```rust
// In JobService, spawn a background task:
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        // Query DB for cron jobs
        let cron_jobs = storage.list_jobs(Some(JobState::Completed)).await?;
        for job in cron_jobs.iter().filter(|j| j.cron.is_some()) {
            let schedule = cron::Schedule::from_str(&job.cron.unwrap()).ok()?;
            if schedule.upcoming(chrono::Utc).next().map(|t| t <= chrono::Utc::now()) {
                // Re-submit the job
            }
        }
    }
});
```

**Note**: cron crate is 0.13 but latest is 0.16. The `cron` crate uses 7-field format (sec min hour dom month dow [year]), not standard 5-field. If accepting user input in 5-field, prepend `"0 "`. Update to 0.16 when implementing.

---

#### L1-L4. Connection pooling + error types (defer)

These are optimization items with low impact:

- **L1**: DaemonClient creates new connection per request → implement persistent mode for TUI
- **L2**: Server handles one request per connection → support pipelining
- **L3**: `DaemonError::Connection` conflates all IO errors → add `StorageError`, `WatchError`
- **L4**: No `Shutdown` IPC request → add `DaemonRequest::Shutdown`

**Recommendation**: Defer to v0.49. Only implement if TUI perf becomes an issue.

---

### Phase 2: Bug Hunt (Discovery Remaining)

#### ARCH-3 Execution Error Migration (58 sites)

**Problem**: 58 call sites in 5 files still use `NikaError::ExecError`, `FetchError`, `ExtractError`, `Execution`, `WorkflowCancelled`, `TaskPanicked` directly instead of the domain `ExecutionError` enum.

**Files**:
- `runtime/executor/fetch.rs` — 20 sites
- `runtime/executor/extract.rs` — 19 sites
- `runtime/executor/exec.rs` — 13 sites
- `runtime/runner.rs` — 5 sites
- `ast/lower.rs` — 1 site

**Why it failed in regex approach**: These files use `.map_err(|e| NikaError::FetchError { reason: ... })` closures where the return type is inferred as `NikaError`. Changing to `ExecutionError::FetchFailed { ... }` changes the closure return type, but the enclosing `Result<_, NikaError>` can't use `?` without explicit `.into()` or `From` conversion. The `.map_err(|e| ...)` closures also assign to `Option<NikaError>` variables (e.g., `last_error`), creating type mismatches.

**Correct approach**: Do it manually, file by file, function by function:

1. **Start with `ast/lower.rs`** (1 site, simplest)
2. **Then `runtime/runner.rs`** (5 sites, Cancelled/Panicked patterns)
3. **Then `runtime/executor/exec.rs`** (13 sites)
4. **Then `runtime/executor/extract.rs`** (19 sites)
5. **Finally `runtime/executor/fetch.rs`** (20 sites, most complex — `last_error` pattern)

**For each file**:
- Read the entire function
- Identify which `NikaError::*` → `ExecutionError::*` mappings apply
- For `return Err(NikaError::ExecError { ... })` → `return Err(ExecutionError::ExecFailed { ... }.into())`
- For `.map_err(|e| NikaError::FetchError { ... })` → keep as `NikaError::FetchError` (map_err closure type inference)
- For `last_error = Some(NikaError::FetchError { ... })` → keep as `NikaError` (variable type is `Option<NikaError>`)

**Rule**: Only migrate `Err(NikaError::Variant { ... })` returns, not `.map_err()` closures or typed variable assignments.

**Verify**: `cargo test -p nika-engine --lib` after each file (3,581 tests).

---

#### Discovery M-items Still Open

| ID | Issue | Effort |
|----|-------|--------|
| M1 (original) | rxing 108s build | Can't fix (external dep) |
| M3 (original) | Feature-gate jsonschema | 2h, low ROI |
| M12 | lower_action clones entire struct | Needs 5 fn signature changes |

**M12 detail**: `lower_action()` calls `a.clone()` for each match arm (Infer, Exec, Fetch, Invoke, Agent). The `lower_*` functions take owned values. To fix properly:
1. Change `lower_infer(infer: AnalyzedInferAction, ...)` to `lower_infer(infer: &AnalyzedInferAction, ...)`
2. Clone individual fields inside (e.g., `prompt: infer.prompt.clone()`)
3. This may actually increase clones (N field clones vs 1 struct clone) — measure first
4. **Recommendation**: Profile before changing. The current approach may be optimal.

---

### Phase 3: E2E Testing

Run these verification commands:

```bash
# Full workspace tests (must: 4,744+ pass, 0 fail)
cargo test --workspace --lib 2>&1 | grep "test result:"

# Clippy clean (must: 0 errors)
cd tools/nika && cargo clippy --all-targets --all-features -- -D warnings

# Daemon-specific tests
cargo test -p nika-daemon --lib

# Engine secrets integration
cargo test -p nika-engine --lib -- secrets

# TUI control view
cargo test -p nika-tui --lib -- control

# Check for unwrap() in daemon production code
grep -rn "\.unwrap()" tools/nika-daemon/src/ | grep -v test | grep -v "#\[cfg(test)\]"

# Check for TODO/FIXME
grep -rn "TODO\|FIXME" tools/nika-daemon/src/

# Check unused deps
cargo machete 2>&1 || echo "install: cargo install cargo-machete"
```

---

### Phase 4: Deep Bug Hunt with Agents

Launch these agents in parallel:

**Agent 1 — Security Audit** (rust-security):
```
Audit /Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/ for:
- Command injection paths (job spawn, exec)
- Path traversal (workflow paths, CAS paths)
- Secret leakage (Debug impls, log output, error messages)
- Socket security (permissions, authentication)
- Resource exhaustion (unbounded allocations, missing timeouts)
- TOCTOU races (file operations, PID checks)
Report: CRITICAL/HIGH/MEDIUM with exact file:line references.
```

**Agent 2 — Async Correctness** (rust-async-expert):
```
Review /Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/ for:
- Blocking calls in async context (std::fs, std::net, etc.)
- DashMap guard held across .await (deadlock)
- Missing cancellation safety (select! branches)
- tokio::sync::Mutex held too long
- Broadcast channel lag handling
- Missing timeouts on IPC operations
Report with specific code fixes.
```

**Agent 3 — Test Coverage** (general-purpose):
```
Analyze test coverage gaps in /Users/thibaut/dev/supernovae/nika/tools/nika-daemon/src/:
1. List all public functions without test coverage
2. List all error paths not tested
3. List all edge cases: empty inputs, max values, concurrent access
4. Suggest 20 specific new tests with descriptions
Focus on: protocol edge cases, storage failures, job lifecycle races, cache eviction, watch debounce timing.
```

---

## WORKSPACE MAP

```
tools/
├── nika/              Binary CLI (2k LOC)
├── nika-engine/       Execution engine (140k LOC, 3,581 tests)
│   ├── src/secrets/   ← Daemon IPC integration for boot
│   └── src/error_domains.rs ← ARCH-3 domain enums
├── nika-daemon/       Background daemon (4.8k LOC, 113 tests) [THIS SESSION'S FOCUS]
│   ├── src/protocol.rs    32 request types, 18 response types
│   ├── src/client.rs      Unix socket client + timeout
│   ├── src/server.rs      Request router + Semaphore
│   ├── src/lifecycle.rs   Self-exec daemonize, PID, signals
│   ├── src/storage.rs     SQLite dedicated DB thread
│   ├── src/events.rs      Broadcast channel (12 event types)
│   ├── src/install.rs     launchd + systemd
│   └── src/services/
│       ├── secrets.rs     Env + keychain (spawn_blocking)
│       ├── jobs.rs        Submit, execute, cancel, retry, timeout
│       ├── watch.rs       notify + debounce + glob
│       └── cache.rs       DashMap + blake3 + lazy TTL
├── nika-init/         Project scaffolding (21k LOC)
├── nika-core/         AST + types (23k) — ZERO I/O
├── nika-event/        EventLog (4k, 133 tests)
├── nika-mcp/          MCP client (9k)
├── nika-media/        CAS store + media tools (25k)
├── nika-cli/          CLI subcommands (8k)
│   ├── src/daemon.rs  nika daemon start/stop/restart/status/logs/install/uninstall
│   ├── src/jobs.rs    nika job submit/list/status/cancel/retry/history
│   └── src/cache_cmd.rs  nika cache stats/clear
├── nika-tui/          Terminal UI (86k, 2,120 tests)
│   └── src/views/control.rs  Daemon dashboard (40/60 split)
├── nika-lsp-core/     LSP intelligence (9k)
└── nika-lsp/          LSP binary (2.5k)
```

---

## MANDATORY SKILLS

| When | Skill |
|------|-------|
| Before Rust code | `/spn-rust:rust` |
| Async patterns | `/spn-rust:rust-async` |
| Before implementing | `/spn-powers:brainstorming` |
| Before claiming done | `/spn-powers:verification-before-completion` |
| Debugging | `/spn-powers:systematic-debugging` |

---

## TESTING

```bash
cargo test --workspace --lib                    # All crates (4,744+, safe)
cargo test -p nika-daemon --lib                 # Daemon (113)
cargo clippy --all-targets --all-features       # Zero warnings
```

**WARNING:** `cargo test` without `--lib` triggers macOS Keychain popups. Always use `--lib`.

---

## WARNINGS

- Pre-commit hook: `cargo clippy --all-targets --all-features -- -D warnings`
- NIKA-160-164: Reserved for nika-core ParseErrorKind
- nika-core is ZERO I/O
- nika-daemon does NOT depend on nika-engine
- Co-author: `Nika 🦋 <nika@supernovae.studio>`
- 1 fix = 1 commit, push every 3-5 commits
- AGPL-3.0 license on all crates

---

## TERMINAL PROMPT

```
ultrathink   Lis ce fichier en entier avant de faire quoi que ce soit:

1. docs/plans/2026-03-26-v048-daemon-remaining-handoff.md (CE FICHIER — le plan détaillé)

Ensuite lis aussi:
2. tools/nika-daemon/CLAUDE.md ou tools/nika/CLAUDE.md (conventions)
3. CLAUDE.md (racine — architecture)

CONTEXTE:
- Nika = workflow engine YAML pour AI (schema nika/workflow@0.12)
- Session précédente: nika-daemon créé from scratch, 4.8k LOC, 113 tests
- 2 CRITICAL + 6 HIGH security fixes déjà appliqués
- 7 review items MEDIUM/LOW restent + ARCH-3 execution errors (58 sites)

INSTRUCTIONS:
1. Phase 1 — Fix les 7 review items (M1, M3, M4, M5, M6, M8, L1-L4)
2. Phase 2 — ARCH-3 execution error migration (58 sites, fichier par fichier)
3. Phase 3 — E2E testing (cargo test --workspace --lib, clippy, unwrap check)
4. Phase 4 — Lance 3 agents en parallèle: security audit, async correctness, test coverage

RÈGLES CRITIQUES:
- JAMAIS cargo test sans --lib
- TDD: tests RED first, puis GREEN
- 1 fix = 1 commit granulaire, push tous les 3-5 commits
- /spn-rust:rust AVANT tout code Rust

Commence par Phase 1, item M1 (le plus simple), puis enchaîne.
```
