# Post-Launch Sprint Plans (v0.71+)

> 10-agent deep research. 8,000+ lines of blueprints. Exact code, line numbers, Rust crates validated.

## Sprint Documents

### Mega Handoff (master document)
- [`../plans/2026-04-05-v071-post-launch-mega-handoff.md`](../plans/2026-04-05-v071-post-launch-mega-handoff.md) — 3,100 lines
  - Feature 1: `on_error:` fallback routing (~720 lines, exact code blocks)
  - Feature 4: Observability UI (~740 lines, crate structure + HTML architecture)
  - Feature 5: PostgreSQL backend (~1,250 lines, trait + DDL + migration)

### Dedicated Blueprints
- [`../plans/2026-04-05-scheduling-cron-blueprint.md`](../plans/2026-04-05-scheduling-cron-blueprint.md) — 1,458 lines
  - CronSchedule struct, V5 SQL, protocol, CLI, 5 phases
- [`../plans/2026-04-05-multi-tenant-auth-blueprint.md`](../plans/2026-04-05-multi-tenant-auth-blueprint.md) — 1,402 lines
  - TokenStore, auth rewrite, V6 SQL, management API, 10 tests

### Crate Research Reports
- [`../research/2026-04-05-error-recovery-fallback-patterns.md`](../research/2026-04-05-error-recovery-fallback-patterns.md) — 930 lines
  - Temporal, Airflow, Tower, backon, circuit breakers, LLM routers
- [`../research/2026-04-05-auth-rbac-crate-research.md`](../research/2026-04-05-auth-rbac-crate-research.md) — 599 lines
  - BLAKE3 vs SHA-256, DashMap vs Redis, casbin vs oso vs bitflags
- [`../research/2026-04-05-embedded-web-dashboard-research.md`](../research/2026-04-05-embedded-web-dashboard-research.md) — 518 lines
  - rust-embed, htmx vs SPA, uPlot vs Chart.js, d3-dag vs petgraph

## Crate Decisions (from research)

| Feature | Crate | Status |
|---------|-------|--------|
| Cron parsing | **croner 3.0.1** (keep) | Already in deps, best parser |
| Timezone | **chrono-tz 0.10** (add) | croner-compatible, IANA tz |
| Token hashing | **blake3 1.8** (already in deps) | 10-14x faster than SHA-256 |
| Token cache | **dashmap 6.1** (already in deps) | 50-100ns lookups |
| RBAC | **bitflags** (no crate, u32) | 4 permissions, no policy engine |
| PostgreSQL | **sqlx 0.8.6** (add, feature-gated) | Compile-time checked, async |
| Web UI embedding | **rust-embed 8.11** (add) | Hot-reload dev, compressed release |
| HTML templates | **maud 0.27** (add) | Compile-time, type-safe |
| Charting | **uPlot 1.6** (CDN, 50KB) | Fastest Canvas, MIT |
| UI framework | **htmx 2.0** (CDN, 50KB) | Zero build step, SSE native |
| DAG layout | **petgraph** (already in deps) | Server-side Sugiyama |

## Priority & Timeline

```
v0.71  on_error: fallback       ~700 LOC   week 1    zero new deps
v0.72  scheduling/cron          ~820 LOC   week 2    +chrono-tz
v0.73  multi-tenant auth L1     ~850 LOC   week 3    zero new deps
v0.74  observability UI         ~2000 LOC  weeks 4-6 +rust-embed, +maud
v0.75  PostgreSQL backend       ~1500 LOC  weeks 7-8 +sqlx (feature-gated)
```
