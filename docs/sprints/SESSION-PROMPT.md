# SESSION PROMPT — Nika v0.71+ Post-Launch Implementation

> **Copy-paste this entire prompt into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.

---

## CONTEXT

Tu travailles sur **Nika**, un moteur de workflows YAML pour l'IA. Rust, 15 crates, ~395K LOC.
v0.70.0 est taggé. Feature freeze levé. On implémente 5 features post-launch.

**IMPORTANT**: Lis ces fichiers AVANT de coder quoi que ce soit:

```
# Master plan (3,100 lines — architecture, YAML syntax, exact code blocks)
cat docs/plans/2026-04-05-v071-post-launch-mega-handoff.md

# Dedicated blueprints (détails par feature)
cat docs/plans/2026-04-05-scheduling-cron-blueprint.md      # 1,458 lines
cat docs/plans/2026-04-05-multi-tenant-auth-blueprint.md     # 1,402 lines

# Crate research (décisions verrouillées)
cat docs/research/2026-04-05-error-recovery-fallback-patterns.md
cat docs/research/2026-04-05-auth-rbac-crate-research.md
cat docs/research/2026-04-05-embedded-web-dashboard-research.md

# Sprint index + crate decisions
cat docs/sprints/README.md
```

**Workspace**: `cd tools/` pour tout cargo command. `Cargo.toml` workspace est dans `tools/`.

---

## RULES

### Skills obligatoires
- **test-driven-development**: RED → GREEN → REFACTOR. Pas de code prod sans test qui fail d'abord.
- **verification-before-completion**: `cargo test --workspace --lib --exclude nika-py` + `cargo clippy --workspace -- -D warnings` + `cargo fmt --all --check` AVANT chaque commit.
- **systematic-debugging**: Si un test casse, diagnose root cause avant de fix.
- **rust**: Toujours `--lib` pour éviter les popups Keychain macOS.

### Commits
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
1 fix = 1 commit. Granulaire. Push quand le sprint est complet + tests verts.

### Conventions Rust
- Errors: `NikaError` avec codes NIKA-XXX, jamais `anyhow`
- AST: Raw → Analyzed → Lower, jamais skip
- Tests: `cargo test --lib` toujours (pas de keychain)
- Zero dead code, zero backward compat
- License: AGPL-3.0-or-later

### Crate decisions (VERROUILLÉES — ne pas changer)
| Feature | Crate | Version |
|---------|-------|---------|
| Cron | croner (keep) | 3.0.1 |
| Timezone | chrono-tz (add) | 0.10 |
| Token hash | blake3 (already in deps) | 1.8 |
| Token cache | dashmap (already in deps) | 6.1 |
| RBAC | bitflags u32 (no crate) | — |
| PostgreSQL | sqlx (add, feature-gated) | 0.8.6 |
| Web UI embed | rust-embed (add) | 8.11 |
| Templates | maud (add) | 0.27 |
| Charts | uPlot (CDN) | 1.6 |
| UI framework | htmx (CDN) | 2.0 |
| DAG layout | petgraph (already in deps) | — |

---

## FEATURE 1: `on_error:` Fallback Routing (v0.71) — ~700 LOC, 2-3h

### What
3 variants quand un task fail: `ignore: true`, `retry_with_provider: openai`, `fallback: task_id`.

### Execution Order
1. **nika-core AST** (Phase 1-3): RawTask.on_error → AnalyzedOnError + OnErrorAction enum → parser + analyzer
2. **nika-event** (Phase 4): `TaskFallbackTriggered` event variant
3. **nika-engine runner** (Phase 5): Interception dans `execute_task_iteration()` après retry exhaustion
4. **Tests** (Phase 6): 9 tests E2E

### Key Files
```
tools/nika-core/src/ast/raw/task.rs           — add on_error field
tools/nika-core/src/ast/raw/parser.rs         — recognize "on_error" key
tools/nika-core/src/ast/analyzed/task.rs      — AnalyzedOnError struct + OnErrorAction enum
tools/nika-core/src/ast/analyzer/analyze.rs   — resolve fallback task_id
tools/nika-core/src/ast/analyzer/errors.rs    — NIKA-290 UnknownOnErrorFallback
tools/nika-event/src/log.rs                   — TaskFallbackTriggered variant
tools/nika-engine/src/runtime/runner.rs       — interception after retry loop (line ~1468)
```

### TDD Sequence
```
1. test_on_error_ignore_returns_null → RED → implement ignore branch → GREEN
2. test_on_error_ignore_no_cascade → RED → verify downstream runs → GREEN
3. test_on_error_retry_with_provider → RED → provider switch → GREEN
4. test_on_error_retry_both_fail → RED → double failure → GREEN
5. test_on_error_fallback_action → RED → fallback exec → GREEN
6. test_on_error_for_each_per_item → RED → per-item fallback → GREEN
7. test_on_error_after_retry → RED → retry exhausts then on_error → GREEN
8. test_on_error_event_emitted → RED → EventLog check → GREEN
9. test_on_error_unknown_rejected → RED → analyzer NIKA-290 → GREEN
```

### Critical Rules
- Depth limit 1: fallback task's own on_error is IGNORED
- for_each: per-item evaluation
- DAG: NO edges for fallback (runtime concern, not structural)
- `ignore: true` stores `Value::Null` — downstream must use `| default()`

---

## FEATURE 2: Scheduling / Cron (v0.72) — ~820 LOC, 3-4h

### What
`nika schedule add workflow.nika.yaml --cron "0 */6 * * *" --tz "Europe/Paris"`

### Key Discovery
Le scheduler EXISTE DÉJÀ dans `nika-daemon/src/services/jobs.rs:467-554`. On ajoute la table `schedules` comme entité first-class.

### Execution Order
1. **Storage** (Phase 1): `CronSchedule` struct, V5 migration, 6 DbCommand variants
2. **Timezone** (Phase 2): Refactor `fire_due_cron_jobs` pour lire `schedules` table + chrono-tz
3. **Protocol** (Phase 3): 6 DaemonRequest + 3 DaemonResponse variants
4. **Server** (Phase 4): 6 dispatch branches dans `server.rs`
5. **CLI** (Phase 5): `nika-cli/src/schedule.rs` (CREATE ~200 LOC)

### Key Files
```
tools/nika-storage/src/lib.rs                   — CronSchedule, V5 SQL, 6 methods
tools/nika-daemon/src/services/jobs.rs          — refactor fire_due_cron_jobs
tools/nika-daemon/src/protocol.rs               — new request/response variants
tools/nika-daemon/src/server.rs                 — dispatch branches
tools/nika-cli/src/schedule.rs                  — CREATE new file
tools/nika-cli/src/lib.rs                       — add mod schedule
tools/nika/src/cli/mod.rs                       — re-export
tools/nika/src/main.rs                          — Schedule command + dispatch
```

### Blueprint complet: `docs/plans/2026-04-05-scheduling-cron-blueprint.md`

---

## FEATURE 3: Multi-Tenant Auth L1 (v0.73) — ~850 LOC, 2-3 days

### What
Multiple API keys nommées avec expiry. BLAKE3 hash. DashMap cache 60s TTL.

### Execution Order
1. **Storage** (Phase 1): `TokenEntry`, V6 migration (`serve_tokens` table), 6 DbCommand variants
2. **TokenStore** (Phase 2): DashMap cache + TTL + invalidation
3. **Auth middleware** (Phase 3): Rewrite avec `AuthMode::Legacy` / `AuthMode::MultiKey`
4. **Routes** (Phase 4): POST/GET/DELETE `/v1/tokens`
5. **CLI** (Phase 5): `nika serve token add/list/revoke`
6. **Startup** (Phase 6): 4-case validation logic

### Key Files
```
tools/nika-storage/src/lib.rs                   — TokenEntry, V6 SQL
tools/nika-serve/src/auth.rs                    — REWRITE avec TokenStore
tools/nika-serve/src/state.rs                   — add token_store
tools/nika-serve/src/config.rs                  — auth_token: Option<String>
tools/nika-serve/src/error.rs                   — ServeError::Forbidden (403)
tools/nika-serve/src/routes/tokens.rs           — CREATE management endpoints
tools/nika-cli/src/serve_token.rs               — CREATE CLI commands
```

### Blueprint complet: `docs/plans/2026-04-05-multi-tenant-auth-blueprint.md`

### Critical Rules
- Token format: `nk_` prefix + 24 random bytes hex
- BLAKE3 hash stored, raw token shown ONCE
- Legacy mode: NIKA_SERVE_TOKEN continues working si 0 rows dans serve_tokens
- Rate limiter key: migrer de raw token → token_id (UUID)

---

## FEATURE 4: Observability UI (v0.74) — ~2000 LOC, 3 weeks

### What
Web dashboard embedded dans `nika serve` à `http://localhost:3000/ui`.

### New Crate: `nika-obs`
```
tools/nika-obs/
  Cargo.toml          — nika-event, axum, rust-embed, maud, chrono
  src/
    lib.rs            — pub mod parser, aggregator, routes
    parser.rs         — parse NDJSON traces → Vec<Event>
    aggregator.rs     — single-pass TraceSummary (cost, tokens, TTFT)
    routes.rs         — /ui, /v1/traces, /v1/traces/{id}/summary
  assets/
    index.html        — htmx SPA shell, 4 views
    app.js            — hash router, fetch, EventSource
    waterfall.js      — Canvas task timeline
    style.css         — Solarized dark theme
```

### 4 Views
1. **Trace List** — table, filter by workflow/date/status
2. **Trace Detail** — DAG (SVG) + waterfall (Canvas) + events + cost sidebar
3. **Cost Dashboard** — daily cost per provider, model breakdown
4. **Live Monitor** — SSE `/v1/events/{id}`, real-time task status

### Enhanced Prometheus Metrics (add to nika-serve)
```
nika_provider_tokens_total{provider,model,direction}
nika_provider_cost_usd_total{provider,model}
nika_task_duration_seconds{verb}
nika_structured_output_total{layer,outcome}
```

### Blueprint: Feature 4 section dans `docs/plans/2026-04-05-v071-post-launch-mega-handoff.md`
### Research: `docs/research/2026-04-05-embedded-web-dashboard-research.md`

---

## FEATURE 5: PostgreSQL Backend (v0.75) — ~1500 LOC, 2 weeks

### What
`StorageBackend` async trait. SQLite (default) + PostgreSQL (feature-gated).

### Execution Order
1. **Phase 1 — Extract trait** (zero risk): `backend.rs` trait, `sqlite.rs` extraction, lib.rs wrapper
2. **Phase 2 — PostgreSQL**: `postgres.rs` avec sqlx 0.8, 4 migration files
3. **Phase 3 — Serve integration**: `NIKA_STORAGE_BACKEND=postgres` env var
4. **Phase 4 — Multi-instance**: `instance_id` pour `reset_stale_running`

### Key Decision
```rust
#[derive(Clone)]
pub struct Storage {
    backend: Arc<dyn StorageBackend>,  // SQLite or PostgreSQL
}
```
Zero API change pour les callers. `AppState`, `JobService`, tous les handlers inchangés.

### Blueprint: Feature 5 section dans `docs/plans/2026-04-05-v071-post-launch-mega-handoff.md`

---

## WHAT NOT TO DO

- No new verbs (5 verbs are sacred)
- No Egghead/memory system (separate sprint)
- No TUI redesign (88K LOC, mature)
- No WebSocket (SSE works fine)
- No full JSON Schema in eval (use structured: in workflows)
- Ne change PAS les crate decisions ci-dessus

---

## VERIFICATION CHECKLIST (avant chaque tag)

```bash
cd tools/
cargo test --workspace --lib --exclude nika-py   # 0 failures
cargo clippy --workspace -- -D warnings          # 0 warnings
cargo fmt --all --check                           # clean
```

---

## START HERE

1. Lis le master handoff: `cat docs/plans/2026-04-05-v071-post-launch-mega-handoff.md`
2. Lis le blueprint du sprint que tu implémentes
3. Lis le research report correspondant
4. Crée des TodoWrite tasks pour chaque phase
5. TDD: RED → GREEN → REFACTOR
6. Commit granulaire, push quand sprint complet
