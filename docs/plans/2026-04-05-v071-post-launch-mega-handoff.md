# MEGA HANDOFF: v0.71+ Post-Launch Features

> **Codebase**: nika v0.70.0 | ~395K LOC | 15 crates | 4694 tests | 62 tools | 63 transforms | 10 lint rules
> **Launch**: May 5, 2026 — FEATURE FREEZE until then (bug fixes only)
> **Post-launch**: 5 features in priority order, ~6-8 weeks total
> **Philosophy**: v0 = zero dead code, zero backward compat, TDD, 1 fix = 1 commit

---

## PRIORITY ORDER

```
                  EFFORT    IMPACT    SHIP BY
 1. on_error:      ~700 LOC  HIGH     v0.71 (week 1)
 2. Scheduling     ~720 LOC  HIGH     v0.72 (week 2)
 3. Multi-tenant   ~800 LOC  HIGH     v0.73 (week 3) — L1 only, L2/L3 later
 4. Observability  ~2000 LOC MEDIUM   v0.74 (weeks 4-6)
 5. PostgreSQL     ~1500 LOC LOW      v0.75 (weeks 7-8)
```

---

## FEATURE 1: `on_error:` Fallback Routing (v0.71)

### What
When a task fails, optionally route to a fallback instead of cascading NIKA-026 to dependents.

### YAML Syntax
```yaml
# Ignore failure, continue with null output
- id: optional_enrichment
  infer: "Enrich: {{with.data}}"
  on_error:
    ignore: true

# Failover to different provider
- id: generate
  provider: anthropic
  infer: "Write tagline for {{inputs.product}}"
  on_error:
    retry_with_provider: openai

# Use another task's action as fallback template
- id: fallback_gen
  provider: openai
  model: gpt-4o-mini
  infer: "Write tagline for {{inputs.product}}"

- id: generate
  provider: anthropic
  infer: "Write tagline for {{inputs.product}}"
  on_error:
    fallback: fallback_gen

# Works with retry: — retry fires first, on_error fires if ALL retries fail
- id: fragile_api
  retry: { max_attempts: 3, delay_ms: 2000 }
  fetch: { url: "https://api.example.com/data" }
  on_error:
    fallback: cached_fallback

# Works with for_each — per-item fallback
- id: translate
  for_each: "$articles"
  as: article
  fail_fast: false
  infer: "Translate: {{with.article.text}}"
  on_error:
    ignore: true   # failed items → null in array
```

### Architecture
- **NOT a DAG change** — fallback is runtime, not structural. Mirrors `retry:` pattern.
- Interception in `execute_task_iteration()` after all retries exhausted.
- `ignore:` → store `TaskResult::success(Value::Null)`, emit `TaskFallbackTriggered`.
- `retry_with_provider:` → rebuild action with new provider, execute once.
- `fallback:` → look up fallback task's `AnalyzedTaskAction`, execute once (depth limit 1).
- Result stored under ORIGINAL task_id → downstream sees success → no cascade.

### Files to Modify

| File | Change |
|------|--------|
| `nika-core/src/ast/raw/task.rs` | Add `on_error: Option<Spanned<Value>>` to `RawTask` |
| `nika-core/src/ast/raw/parser.rs` | Recognize `"on_error"` field |
| `nika-core/src/ast/analyzed/task.rs` | Add `AnalyzedOnError` + `OnErrorAction` enum + field on `AnalyzedTask` |
| `nika-core/src/ast/analyzed/mod.rs` | Re-export new types |
| `nika-core/src/ast/analyzer/analyze.rs` | Parse + resolve fallback task_id via task_table |
| `nika-core/src/ast/analyzer/errors.rs` | Add `NIKA-290 UnknownOnErrorFallback` |
| `nika-event/src/log.rs` | Add `TaskFallbackTriggered` variant (58→59 event types) |
| `nika-engine/src/runtime/runner.rs` | Add `fallback_task` param to `execute_task_iteration()`, dispatch block in failure path |

### OnErrorAction Enum
```rust
pub enum OnErrorAction {
    Fallback { task_id: TaskId },
    Ignore,
    RetryWithProvider { provider: ProviderName },
}
```

### Critical Details
- **Depth limit 1**: fallback task's own `on_error` is IGNORED when running as fallback. Prevents infinite chains.
- **for_each**: fallback evaluated per-item inside iteration. Mixed results array (some primary, some fallback).
- **Event ordering**: `TaskStarted → TaskFailed → TaskFallbackTriggered → TaskCompleted` (if fallback succeeded).
- **`ignore: true` + downstream**: `$ignored_task` resolves to `null`. Guard with `| default("fallback")`.
- **Error code**: NIKA-290 for unknown fallback task reference.

### TDD Sequence (9 tests)
```
1. test_on_error_ignore_returns_null_output → RED → implement ignore branch → GREEN
2. test_on_error_ignore_does_not_cascade → RED → verify downstream runs → GREEN
3. test_on_error_retry_with_provider_succeeds → RED → implement provider switch → GREEN
4. test_on_error_retry_with_provider_also_fails → RED → verify double failure → GREEN
5. test_on_error_fallback_uses_fallback_action → RED → implement fallback exec → GREEN
6. test_on_error_fallback_with_for_each → RED → per-item fallback → GREEN
7. test_on_error_combined_with_retry → RED → retry exhausts then on_error → GREEN
8. test_on_error_emits_fallback_event → RED → check EventLog → GREEN
9. test_on_error_unknown_fallback_rejected → RED → analyzer error → GREEN
```

### Estimate: ~700 LOC, 2-3 hours implementation + 1 hour tests

---

## FEATURE 2: Scheduling / Cron (v0.72)

### What
`nika schedule add workflow.nika.yaml --cron "0 */6 * * *"` — first-class cron schedules.

### Key Discovery
**The cron scheduler ALREADY EXISTS** in `nika-daemon/src/services/jobs.rs:467-554` — `run_cron_scheduler()` + `fire_due_cron_jobs()` with overlap protection. It's spawned in `server.rs:162`. The `Job.cron` column exists since V1. What's MISSING: schedules are not first-class entities (no `schedules` table, no `nika schedule` CLI, no timezone, no pause/resume).

### CLI Commands
```bash
nika schedule add report.nika.yaml --cron "0 */6 * * *" --name "6h-report"
nika schedule add daily.nika.yaml --cron "@daily" --tz "Europe/Paris"
nika schedule list [--json]
nika schedule get <ID>
nika schedule remove <ID>
nika schedule pause <ID>
nika schedule resume <ID>
```

### Schema V5: `schedules` Table
```sql
CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    name TEXT,
    workflow TEXT NOT NULL,
    args TEXT,
    cron TEXT NOT NULL,
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
```

### Files to Modify/Create

| File | Change |
|------|--------|
| `nika-storage/src/lib.rs` | `CronSchedule` struct, V5 migration, 6 DbCommand variants, 6 Storage methods |
| `nika-daemon/src/services/jobs.rs` | Refactor `fire_due_cron_jobs` to read `schedules` table + timezone |
| `nika-daemon/src/protocol.rs` | 6 new `DaemonRequest` + 3 `DaemonResponse` variants |
| `nika-daemon/src/server.rs` | 6 dispatch branches |
| `nika-cli/src/schedule.rs` | **CREATE** — `ScheduleAction` enum + handler (~200 LOC) |
| `nika-cli/src/lib.rs` | Add `#[cfg(unix)] pub mod schedule;` |
| `nika/src/cli/mod.rs` | Re-export schedule |
| `nika/src/main.rs` | Add `Schedule` command + dispatch |
| `tools/Cargo.toml` | Add `chrono-tz = "0.10"` workspace dep |

### Critical Details
- **Overlap protection**: already exists — skip if previous run still pending/running.
- **`@` shortcuts**: `croner 3` supports `@daily`, `@hourly`, `@weekly`, `@monthly`.
- **Timezone**: `chrono-tz` crate, IANA names. Default UTC. Invalid tz → fallback UTC + warn.
- **`next_run_at`**: recomputed from `now` after each fire (avoids drift).
- **Daemon restart**: schedules persist in SQLite. Missed runs fire on next tick.
- **`nika schedule remove`**: does NOT cancel running jobs, only prevents future firings.

### Estimate: ~720 LOC, 3-4 hours across 5 phases

---

## FEATURE 3: Multi-Tenant Auth (v0.73)

### What
Multiple API keys with names, expiry, scopes. Replace single `NIKA_SERVE_TOKEN`.

### Three Levels (ship L1 first)
- **L1 Multi-key** (2-3 days): named API keys + optional expiry. Jungo gets its own key.
- **L2 Scoped** (1-2 days): keys restricted to specific workflow patterns.
- **L3 Full RBAC** (3-5 days, post-launch): users, roles (admin/operator/viewer), audit log.

### Schema V5 (or V6 if scheduling ships first): `serve_tokens` Table
```sql
CREATE TABLE serve_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash BLOB NOT NULL UNIQUE,   -- 32 bytes BLAKE3(raw_token)
    role TEXT NOT NULL DEFAULT 'operator',
    scope TEXT NOT NULL DEFAULT '*',    -- '*' or 'wf1.nika.yaml,wf2-*.nika.yaml'
    created_at TEXT NOT NULL,
    expires_at TEXT,
    last_used_at TEXT,
    revoked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_serve_tokens_hash ON serve_tokens(token_hash);
```

### Token Flow
```
Token created → BLAKE3 hash stored in DB → raw token shown ONCE to user
Request arrives → SHA-256 the bearer token → lookup in DashMap cache (60s TTL)
Cache miss → SELECT by token_hash → check expiry + revoked → build Principal
Principal attached to request extensions → handlers read scope/role
```

### Legacy Migration (ZERO downtime)
```
serve_tokens rows = 0 AND NIKA_SERVE_TOKEN set → legacy single-token mode (existing behavior)
serve_tokens rows > 0 → multi-key mode (NIKA_SERVE_TOKEN ignored)
```

### CLI Commands
```bash
nika serve token add --name "jungo-prod" [--expires 2026-12-31] [--scope "jungo-*.nika.yaml"]
nika serve token list
nika serve token revoke <id-or-name>
```

### Files to Modify/Create

| File | Change |
|------|--------|
| `nika-storage/src/tokens.rs` | **CREATE** — `TokenEntry`, CRUD, audit log (L3) |
| `nika-storage/src/lib.rs` | Schema migration, `mod tokens` |
| `nika-serve/src/auth.rs` | Rewrite: `TokenStore` + `Principal` + legacy fallback |
| `nika-serve/src/state.rs` | Add `token_store: TokenStore` to `AppState` |
| `nika-serve/src/config.rs` | `auth_token: Option<String>`, startup validation |
| `nika-serve/src/error.rs` | Add `ServeError::Forbidden` (403) |
| `nika-serve/src/routes/tokens.rs` | **CREATE** — POST/GET/DELETE/PATCH endpoints |
| `nika-cli/src/serve_token.rs` | **CREATE** — CLI commands |

### Critical Details
- **BLAKE3 hash** (not raw token) stored in DB. DB dump != credential dump.
- **DashMap cache** (60s TTL) avoids DB roundtrip per request. Invalidated on revoke.
- **Rate limiter key** changes from raw token string to `principal.token_id` (UUID).
- **403 Forbidden** (scoped key can't run this workflow) vs 401 Unauthorized (bad token).

### Estimate: L1 = 2-3 days, L2 = 1-2 days, L3 = 3-5 days

---

## FEATURE 4: Observability UI (v0.74)

### What
Web-based trace viewer embedded in `nika serve`, accessible at `http://localhost:3000/ui`.

### Architecture
**Embedded SPA in the binary** via `rust-embed`. No separate process, no Grafana, no npm.

### New Crate: `nika-obs`
```
tools/nika-obs/
  src/
    lib.rs          Public API
    parser.rs       Parse NDJSON traces → Vec<Event>
    aggregator.rs   Single-pass cost/token/latency summary
    routes.rs       Axum handlers for /ui, /v1/traces
  assets/
    index.html      SPA (vanilla JS, no bundler)
    dag.js          SVG DAG via Sugiyama layout
    waterfall.js    Canvas task timeline
```

### API Endpoints
```
GET  /ui                               Serve SPA
GET  /v1/traces                        List traces (generation_id, date, size)
GET  /v1/traces/{id}                   Full parsed trace as JSON
GET  /v1/traces/{id}/summary           Aggregated cost/token/latency per task
GET  /v1/jobs/{id}/trace               Link job_id → trace
```

### TraceSummary Response
```json
{
  "duration_ms": 8421,
  "total_cost_usd": 0.0087,
  "total_input_tokens": 12400,
  "total_output_tokens": 3200,
  "tasks": [{
    "task_id": "research", "verb": "infer", "provider": "anthropic",
    "duration_ms": 3200, "input_tokens": 8400, "cost_usd": 0.0062,
    "ttft_ms": 340, "status": "completed"
  }]
}
```

### 4 UI Views
1. **Trace List** — table of recent traces, click to open detail
2. **Trace Detail** — DAG visualization + waterfall timeline + event stream + cost sidebar
3. **Cost Dashboard** — daily cost per provider, model breakdown, top expensive workflows
4. **Live Monitor** — EventSource to `/v1/events/{id}`, real-time task status

### Enhanced Prometheus Metrics
```
nika_provider_tokens_total{provider,model,direction}  counter
nika_provider_cost_usd_total{provider,model}           counter
nika_task_duration_seconds{verb}                        histogram
```

### Competitive Advantage vs LangSmith
- Nika captures 58 event types (vs LangSmith's input/output strings)
- Shows structured output repair layers, MCP call traces, agent thinking
- Self-hosted, zero-config, embedded in binary
- Free (LangSmith is paid)

### Files to Create/Modify

| File | Change |
|------|--------|
| `tools/nika-obs/` | **CREATE** entire crate (~2000 LOC) |
| `tools/Cargo.toml` | Add `nika-obs` to workspace |
| `nika-serve/Cargo.toml` | Add `nika-obs` dependency |
| `nika-serve/src/routes/mod.rs` | Wire /ui + /v1/traces routes |
| `nika-serve/src/metrics.rs` | Add provider/model/verb metrics |
| `nika-serve/src/worker.rs` | Post-execution trace parsing for metrics |

### Estimate: ~2000 LOC, 3 weeks across 5 phases

---

## FEATURE 5: PostgreSQL Backend (v0.75)

### What
Optional PostgreSQL backend for `nika-storage`. Enables multi-instance `nika serve`.

### Architecture
**`StorageBackend` async trait + enum dispatch wrapper.**
- `Storage` remains a concrete `Clone` type (zero API change for callers).
- Internally: `Arc<dyn StorageBackend>` dispatches to `SqliteBackend` or `PostgresBackend`.
- Feature-gated: `cargo build --features postgres`.
- Selection: `NIKA_STORAGE_BACKEND=postgres NIKA_STORAGE_URL=postgres://...`

### Trait Definition
```rust
#[async_trait]
pub(crate) trait StorageBackend: Send + Sync + 'static {
    async fn insert_job(&self, job: Job) -> StorageResult<()>;
    async fn get_job(&self, id: &str) -> StorageResult<Option<Job>>;
    // ... 16 methods total, mirrors current Storage API
}
```

### PostgreSQL DDL
- `TIMESTAMPTZ` instead of `TEXT` for timestamps
- `JSONB` instead of `TEXT` for tags (GIN index)
- `BIGSERIAL` instead of `INTEGER AUTOINCREMENT`
- `INSERT ... ON CONFLICT DO UPDATE` instead of `INSERT OR REPLACE`
- `tags->>'key' = $1` instead of `json_extract(tags, '$.key')`
- `RETURNING retry_count` instead of UPDATE + SELECT

### Key Query Translations
| SQLite | PostgreSQL |
|--------|-----------|
| `json_extract(tags, '$.key')` | `tags->>'key'` |
| `INSERT OR REPLACE` | `INSERT ... ON CONFLICT DO UPDATE` |
| `PRAGMA user_version` | `schema_migrations` table |
| UPDATE + SELECT | `UPDATE ... RETURNING` |

### Files to Create/Modify

| File | Change |
|------|--------|
| `nika-storage/src/backend.rs` | **CREATE** — `StorageBackend` trait (16 methods) |
| `nika-storage/src/sqlite.rs` | **CREATE** — Extract current impl from lib.rs |
| `nika-storage/src/postgres.rs` | **CREATE** — `sqlx::PgPool` implementation |
| `nika-storage/migrations/postgres/` | **CREATE** — 4 SQL migration files |
| `nika-storage/src/lib.rs` | Rewrite to `backend: Arc<dyn StorageBackend>` |
| `nika-storage/Cargo.toml` | Add features: `sqlite` (default), `postgres` |
| `nika-serve/src/lib.rs` | Backend selection at startup |
| `tools/Cargo.toml` | Add `sqlx` to workspace deps |

### Critical Details
- **Phase 1 is risk-free**: extract trait + move code, zero behavior change, all tests pass.
- **PG doesn't need dedicated OS thread** — sqlx pool is natively async.
- **`reset_stale_running` multi-instance**: needs `instance_id` column (Phase 4).
- **sqlx offline mode** for CI: `SQLX_OFFLINE=true` + `.sqlx/` query cache.
- **Timestamp impedance**: PG stores `TIMESTAMPTZ`, Job struct stores `String` (RFC3339). Convert on read/write.

### Estimate: ~1500 LOC, 14-17 hours across 4 phases

---

## DEPENDENCY GRAPH

```
          on_error (v0.71)     ← standalone, no deps
               │
          scheduling (v0.72)   ← storage V5 (schedules table)
               │
          multi-tenant (v0.73) ← storage V6 (serve_tokens table)
               │
          observability (v0.74) ← new crate, serve routes
               │
          postgresql (v0.75)   ← storage refactor, feature-gated
```

Features 1-3 are independent and could ship in any order.
Feature 4 depends on Feature 3 for auth on trace endpoints.
Feature 5 should come last (refactors storage, all other features must be stable first).

---

## WHAT NOT TO DO (reinforced)

- No new verbs (5 verbs are sacred)
- No Egghead/memory system (separate sprint, design bible exists)
- No TUI redesign (88K LOC, mature)
- No WebSocket (SSE works fine)
- No `nika diff` (git diff suffices)
- No `nika upgrade` / self-update (Homebrew handles this)
- No full JSON Schema validator in eval (use `structured:` in workflows)
- No multi-node PG without instance_id (Phase 4 of PG feature)

---

## SKILLS & WORKFLOW

```
Question → Research → Skills → Test → Code → Verify → Commit
```

| Skill | When |
|-------|------|
| `test-driven-development` | All code changes |
| `verification-before-completion` | Before every commit |
| `systematic-debugging` | When tests break |
| `rust` | All Rust code |

| Agent | When |
|-------|------|
| `rust-pro` | Code review after each feature |
| `rust-security` | Review auth changes (Feature 3) |
| `rust-async` | Review PG async pool (Feature 5) |

---

## COMMIT STRATEGY

```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

1 fix = 1 commit. Tests verts. Clippy zero. Push HTTPS.
