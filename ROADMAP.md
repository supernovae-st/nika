# Nika Roadmap

> Last updated: 2026-03-31
> Nika stays 0.x.x forever. Schema `nika/workflow@0.12`.

## v0.55.0 — VPS Production Hardening (DONE)

17 fixes across 4 waves. NikaVault encrypted secrets, vLLM compat, daemon hardening.

- [x] systemd Restart=always + EnvironmentFile + Type=notify
- [x] NikaVault (orion XChaCha20Poly1305 + Argon2i) — replaces OS keychain
- [x] `nika provider set` works on headless Linux (no D-Bus, no popup)
- [x] timeout_secs propagation to custom endpoints
- [x] `<think>` tag extraction (Qwen3.5 reasoning models)
- [x] Provider HTTP retry with exponential backoff
- [x] SSRF auto-allow custom endpoint private IPs
- [x] Socket cleanup Drop guard
- [x] SQLite failure fatal + pending job drain + connection drain
- [x] 9,086 tests

## v0.56.0 — `nika serve` V1 + Engine Cleanup (IN PROGRESS)

HTTP API via `nika serve` subcommand. Same binary, same config. Nicolas can call Nika from Django/Jungo. Subprocess model, SQLite jobs, basic auth.

### v0.56 Fixup Sprint (10 security/robustness fixes from 3-agent audit)

- [ ] Full UUID for job IDs (was 12 chars with hyphen = 40 bits = collision risk)
- [ ] WorkerGuard drop guard (panic → job stuck Running forever)
- [ ] AtomicUsize queue depth (was 2 non-atomic DB queries = race condition)
- [ ] Bounded stdout/stderr read (was wait_with_output = unbounded RAM)
- [ ] select! shutdown signal in workers (was: ignore SIGTERM, block drain 30s)
- [ ] env_clear + allowlist (was: env_remove denylist = future secret leak)
- [ ] Configurable CORS origin (was: Any = CSRF risk)
- [ ] Abort handles after drain timeout (was: drop = orphan processes)
- [ ] Windows compile fix (child moved in cfg block)
- [ ] Construct ServeConfig direct (was: set_var deprecated)

### Mega Stability Audit (6 phases, real providers, TDD strict)

Full plan: `docs/plans/2026-04-01-mega-stability-audit.md`

- [ ] Phase 1: Structured Output x 7 providers (8 torture cases)
- [ ] Phase 2: E2E complex workflows (fan-out, multi-provider, for_each chains)
- [ ] Phase 3: Bindings & 38 transforms torture
- [ ] Phase 4: Security (SSRF, exec injection, template injection)
- [ ] Phase 5: Agent verb + 4 guardrails + completion modes
- [ ] Phase 6: Socratic loop (fix → test → retest → repeat until zero failures)

### Engine Cleanup

- [ ] Remove INTERNER global DashMap — replace with `Arc::from()` (OOM fix)
- [ ] Global task concurrency Semaphore(64)
- [ ] Lockfile flock(LOCK_EX|LOCK_NB)
- [ ] Package cache 5-minute TTL
- [ ] Remove `keyring` crate entirely (32 files, 216 references)
- [ ] Fix onboarding.rs NikaKeyring bug

### nika-storage (new crate)

- [ ] Extract SQLite actor from nika-daemon
- [ ] StorageError type
- [ ] Convenience methods: create_job(), complete_job(), fail_job()
- [ ] Stale job recovery on startup

### nika-serve (new crate, `nika serve` subcommand)

- [ ] Axum 0.8, feature flag `serve` (default-on)
- [ ] `POST /v1/run` + `GET /v1/status/{id}` + `POST /v1/cancel/{id}` + `GET /health`
- [ ] Bearer token auth (constant-time)
- [ ] Worker: `current_exe()` + `setsid()` + `kill_on_drop` + PGID kill + timeout
- [ ] Worker tracking: `HashMap<JobId, JoinHandle>`
- [ ] Graceful shutdown + CORS + body limit 1MB
- [ ] Path traversal validation + UTF-8 safe truncation

## v0.57.0 — `nika serve` V2 (Embedded Runner + Production Scale)

Upgrade serve from subprocess to embedded Runner. Add rate limiting, observability, and performance tuning. This is what makes `nika serve` production-grade for real multi-tenant traffic.

### Embedded Runner (no more subprocess)

- [ ] `nika serve` calls `Runner::new(workflow).run()` directly
- [ ] Zero fork overhead — share provider connection pools across workflows
- [ ] Per-workflow CancellationToken isolation
- [ ] SSE event streaming from EventLog (`GET /v1/events/{id}`)
- [ ] Real-time token-by-token streaming to clients via SSE
- [ ] WorkflowExecutor trait (V1 SubprocessExecutor → V2 EmbeddedExecutor)

### Rate Limiting & Queuing

- [ ] apalis integration (PostgreSQL backend) — persistent priority job queue
- [ ] Per-user rate limiting (tower-governor GCRA)
- [ ] Per-user concurrency (DashMap<UserId, Arc<Semaphore>>)
- [ ] Tiered limits: free (2 concurrent, 10 req/min) vs pro (10 concurrent, 100 req/min)
- [ ] Age-based anti-starvation (free-tier jobs rise in priority over time)
- [ ] CoDel queue health algorithm (sojourn time, not just depth)
- [ ] Admission control + load shedding (503 when overloaded)

### Observability

- [ ] Prometheus metrics endpoint (`/metrics`)
- [ ] `nika_jobs_total`, `nika_jobs_active`, `nika_job_duration_seconds`
- [ ] Per-user cost attribution (tokens consumed, compute time)
- [ ] OpenTelemetry tracing (X-Request-Id, distributed trace propagation)
- [ ] Structured JSON logging with job_id + user_id in every line
- [ ] Per-job trace correlation (NIKA_TRACE_ID env var to subprocess)

### Performance (God-Tier Rust Stack)

- [ ] mimalloc allocator (+5-10% across the board, 3 lines of code)
- [ ] sonic-rs for JSON (+200-300% parse/serialize, SIMD accelerated)
- [ ] zstd response compression (70-85% ratio at 400 MB/s)
- [ ] rustls for TLS (-20% handshake latency, -80% per-connection memory)
- [ ] fat LTO + codegen-units=1 + PGO (+25-40% runtime, release only)

### API Hardening

- [ ] Idempotency keys (`Idempotency-Key` header) — prevent double execution
- [ ] `X-Request-Id` header on all responses
- [ ] Job GC (cleanup completed/failed jobs older than N days)
- [ ] HMAC webhook signatures (for callback to Django)
- [ ] JWT auth option (multi-tenant with claims: tenant_id, tier, scopes)
- [ ] Storage trait abstraction (SQLite → PostgreSQL swap, zero code change)

## v0.58.0 — NikaVault Universal Identity

> "Same crypto as 1Password. Built for AI agents."

Evolve NikaVault from 7 API keys into a universal credential vault. Nika becomes the AI that logs in for you — research, write, publish, deploy, notify, all from a single YAML file.

### Vault Schema v2

```rust
enum VaultEntry {
    Key(String),                     // backward compat
    Credentials {
        fields: HashMap<String, String>,
        service_url: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        scopes: Vec<String>,
    },
}
```

### Credential Categories

- [ ] **SaaS APIs** — GitHub, Linear, Vercel, Stripe, Supabase, Notion, Airtable
- [ ] **Email / Comms** — Gmail (OAuth), Slack (bot + user tokens), Telegram (bot)
- [ ] **Cloud Infra** — Scaleway, AWS, GCP, Cloudflare, DigitalOcean
- [ ] **Databases** — PostgreSQL, Redis, Neo4j, MongoDB connection strings
- [ ] **Social / Publishing** — Twitter/X, YouTube (OAuth), WordPress, Ghost
- [ ] **Certificates / SSH** — deploy keys, TLS certs, code signing

### Workflow Integration

- [ ] New binding source: `$vault.SERVICE.FIELD`
- [ ] Template: `{{vault.stripe.webhook}}` in all contexts
- [ ] Null safety: `{{vault.stripe.webhook ?? ""}}`
- [ ] Secret redaction: vault values NEVER in logs, traces, errors

### CLI: `nika vault`

```bash
nika vault set github --field user=ThibautMelen --field token=ghp_xxx
nika vault set stripe --field secret=sk_live_xxx --field webhook=whsec_xxx
nika vault set gmail --oauth           # Browser OAuth2 PKCE flow
nika vault list                        # Services only, never values
nika vault check                       # Validate all credentials still work
nika vault export --format env         # For Docker
nika vault import --from 1password     # Import from op CLI
nika vault import --from bitwarden     # Import from bw CLI
nika vault import --from env           # Import *_API_KEY, *_TOKEN, *_SECRET
```

### Secrets Backend Integration (Doppler-first)

Nicolas uses Doppler for Jungo (Node.js). NikaVault integrates WITH Doppler, not against it.

- [ ] `nika vault import --from doppler` — pull from Doppler project into local vault
- [ ] `nika vault import --from 1password` — pull from `op` CLI
- [ ] `nika vault import --from bitwarden` — pull from `bw` CLI
- [ ] `nika vault import --from env` — pull from current env vars
- [ ] `nika vault sync --with doppler` — runtime read from Doppler (no local copy needed)
- [ ] `NIKA_VAULT_BACKEND=doppler|1password|local` — pluggable backend
- [ ] Encrypted export/import for backup/migration

### OAuth2 & Token Lifecycle

- [ ] Browser-based OAuth2 PKCE flow for Google, GitHub, Slack
- [ ] Store access_token + refresh_token + expiry in vault
- [ ] Auto-refresh expired tokens before workflow execution
- [ ] Scopes management per service
- [ ] `nika vault rotate SERVICE` — generate new, revoke old

### Security

- [ ] Audit log: which workflow accessed which credential, when
- [ ] Scoped access: workflow declares required vault entries, user confirms first run
- [ ] Vault lock/unlock with optional passphrase
- [ ] Field-level encryption (compromise one field ≠ compromise all)
- [ ] Expiry warnings in `nika doctor` output

## v0.59.0 — Nika Egghead (Memory Engine)

7 cognitive mechanisms, 4 memory types. SQLite + usearch + petgraph + fastembed.

- [ ] Working memory (task context during execution)
- [ ] Episodic memory (past workflow runs, outcomes)
- [ ] Semantic memory (domain knowledge, embeddings)
- [ ] Procedural memory (learned patterns, skill refinement)
- [ ] Attention mechanism (relevance filtering)
- [ ] Consolidation (compress + prune)
- [ ] Retrieval (vector search + graph traversal)

## Future — Infrastructure

### Scaleway Production (~€85/month → ~€5.6K/month at scale)

- [ ] Stage 1: Single VPS + H100 (~100 workflows/day) — CURRENT
- [ ] Stage 2: Bigger VPS, still SQLite (~10K workflows/day, ~€150/month)
- [ ] Stage 3: 2x App + LB + PostgreSQL (~100K workflows/day, ~€300/month)
- [ ] Stage 4: Multi-region Paris + Amsterdam (~1M workflows/day, ~€5.6K/month)
- [ ] Stage 5: Kubernetes + NATS + Neon (~100M workflows/day)

### vLLM Optimization

- [ ] `--enable-prefix-caching` (60-80% hit rate)
- [ ] `--scheduling-policy priority` (paid > free)
- [ ] `--reasoning-parser qwen3` (separate thinking server-side)
- [ ] Adaptive client-side concurrency limiter (AIMD)
- [ ] Prometheus metrics polling

### Legal Pre-Launch

- [ ] e-Soleau INPI (15€) — BEFORE public launch
- [ ] Trademark "Nika" — attorney review (Nike phonetic risk)
- [ ] Privacy Policy + Mentions Légales (LCEN)
- [ ] AGPL source code link in UI
- [ ] Plausible Analytics (no cookie banner)
