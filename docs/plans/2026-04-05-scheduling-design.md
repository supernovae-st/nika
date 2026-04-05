# Scheduling Design — `nika every` + `schedule:` (v0.72)

> **Status**: Design complete, ready for implementation
> **Research**: 13 agents (10 research + 3 devil's advocate), 8,000+ lines of findings
> **Estimate**: ~850 LOC, 6 phases, TDD
> **Depends on**: v0.71 (on_error) shipped
> **Crate decisions**: hron 1.0, chrono-tz 0.10, croner 3.0.1 (keep)

---

## Executive Summary

Two commands. One YAML field. Zero cron syntax required.

```bash
# Create — reads like English
nika every 6h report.nika.yaml

# Manage — industry standard lifecycle
nika schedule list
nika schedule pause daily-report
```

```yaml
# Declare — in the workflow file (source of truth)
schedule: "every day at 9am"
```

**Philosophy**: The CLI is imperative ("do this"). The YAML is declarative ("I want this").
Different cognitive modes deserve different words. This is the universal pattern
(kubectl apply ≠ kind: Deployment, docker compose up ≠ services:).

---

## Architecture

```
                    ┌─────────────────────────────────┐
                    │          User Intent             │
                    │  "Run this every 6 hours"        │
                    └──────────┬──────────────────────┘
                               │
              ┌────────────────┴────────────────┐
              │                                 │
     ┌────────▼────────┐             ┌──────────▼──────────┐
     │   CLI (create)  │             │   YAML (declare)    │
     │  nika every 6h  │             │  schedule: "@daily" │
     │  report.nika    │             │  report.nika.yaml   │
     └────────┬────────┘             └──────────┬──────────┘
              │                                 │
              │  DaemonRequest                  │  nika serve scanner
              │  ::ScheduleCreate               │  (60s poll)
              │                                 │
              └────────────────┬────────────────┘
                               │
                    ┌──────────▼──────────────────┐
                    │     schedules table          │
                    │     (SQLite, V5 schema)      │
                    │                              │
                    │  id, name, workflow, cron,    │
                    │  timezone, enabled, source    │
                    └──────────┬──────────────────┘
                               │
                    ┌──────────▼──────────────────┐
                    │   Cron Scheduler (60s tick)  │
                    │   fire_due_cron_jobs()       │
                    │   (already exists in daemon) │
                    └──────────┬──────────────────┘
                               │
                    ┌──────────▼──────────────────┐
                    │   Job execution              │
                    │   nika run workflow.nika.yaml │
                    └─────────────────────────────┘
```

**Source precedence**: YAML schedules are persistent (re-discovered on restart).
CLI schedules are ephemeral (persist in DB but not re-registered on restart).
`source: "yaml"` vs `source: "cli"` column distinguishes them.

---

<!-- Assembled from 4 parallel agents -->


## YAML Syntax

### String shorthand (the 80% case)

The `schedule:` field follows Nika's string-or-object pattern. A single string is enough for most use cases.

**Human-readable (hron)**:
```yaml
schedule: "every day at 9am"
schedule: "every 6 hours"
schedule: "every weekday at 9am"
schedule: "every monday at 8:30am"
schedule: "every weekday at 9am in Europe/Paris"
```

**Presets**:
```yaml
schedule: "@hourly"     # 0 * * * *
schedule: "@daily"      # 0 0 * * *
schedule: "@weekly"     # 0 0 * * 0
schedule: "@monthly"    # 0 0 1 * *
schedule: "@yearly"     # 0 0 1 1 *
```

**Raw cron** (5-field standard):
```yaml
schedule: "0 9 * * *"       # every day at 9:00
schedule: "*/15 * * * *"    # every 15 minutes
schedule: "0 9 * * 1-5"     # weekdays at 9:00
```

### Object form (production)

```yaml
schedule:
  cron: "0 9 * * 1-5"       # Required. Cron, hron, or @preset
  timezone: "Europe/Paris"   # IANA timezone. Default: UTC
  catchup: false             # Run missed ticks on startup? Default: false
  overlap: skip              # skip | queue | replace. Default: skip
  jitter: 30s                # Random delay 0..jitter. Default: 0s
  paused: false              # Register but don't fire. Default: false
```

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `cron` | string | *(required)* | Accepts hron, @preset, or raw cron |
| `timezone` | string | `"UTC"` | IANA tz database name |
| `catchup` | bool | `false` | Dangerous at scale — fires all missed ticks |
| `overlap` | enum | `skip` | `skip` safest; `replace` cancels in-flight |
| `jitter` | duration | `0s` | Spreads load across a window |
| `paused` | bool | `false` | Deploy without activating |

### Complete examples

**Simple daily report:**
```yaml
schema: "nika/workflow@0.12"
workflow: daily-summary
schedule: "@daily"

tasks:
  - id: summarize
    infer: "Summarize today's key events"
```

**Weekday monitoring with timezone:**
```yaml
schema: "nika/workflow@0.12"
workflow: competitor-monitor
schedule:
  cron: "every weekday at 9am"
  timezone: "Europe/Paris"
  overlap: skip
  jitter: 120s

tasks:
  - id: scrape
    for_each: "$inputs.competitors"
    fetch: { url: "{{with.item}}", extract: text }
  - id: analyze
    depends_on: [scrape]
    infer: "Flag pricing changes: {{$scrape | to_json}}"
```

**High-frequency health check:**
```yaml
schema: "nika/workflow@0.12"
workflow: uptime-check
schedule:
  cron: "*/15 * * * *"
  overlap: skip

tasks:
  - id: check
    fetch: { url: "https://api.example.com/health", timeout: 10 }
```

### Parsing priority

1. **Try hron** — `"every day at 9am"` → cron + optional timezone extraction
2. **Try @preset** — `@daily` → `0 0 * * *`
3. **Try raw cron** — validate 5-field with croner
4. **Fail** — NIKA-310 validation error

### Interaction with existing features

- **schedule: + on_error:** — Error handler fires within the run. Schedule waits for next tick.
- **schedule: + retry:** — Retries within a single run. Schedule controls when next run starts.
- **schedule: + for_each:** — Full loop runs per scheduled tick.
- **schedule: + when:** — If false, skip this tick (counts as completed for overlap tracking).

### Discovery

- `nika serve` scans `*.nika.yaml` headers at startup + every 60s
- Header-only parsing (fast, no task body)
- YAML is authoritative; CLI schedules are ephemeral

---


## CLI Commands

### `nika every` — Create (imperative one-liner)

```bash
# Interactive wizard
nika every

# Duration shorthand
nika every 6h report.nika.yaml
nika every 30m health.nika.yaml

# Named intervals
nika every day at 9am report.nika.yaml
nika every weekday at 9am report.nika.yaml
nika every monday at 9am report.nika.yaml

# Raw cron escape hatch
nika every --cron "0 */6 * * *" report.nika.yaml

# With options
nika every 6h report.nika.yaml --tz Europe/Paris
nika every 6h report.nika.yaml --name my-report
nika every 6h report.nika.yaml --overlap skip
nika every 6h report.nika.yaml --dry-run
```

### `nika schedule` — Lifecycle management

```bash
nika schedule list                     # Dashboard
nika schedule list --json              # JSON for scripting
nika schedule show <name>              # Detail card + history
nika schedule pause <name>             # Pause (--reason optional)
nika schedule resume <name>            # Resume
nika schedule trigger <name>           # Run NOW
nika schedule remove <name>            # Delete (confirms)
nika schedule next                     # What fires next?
```

### Aliases

| Input | Resolves to |
|-------|-------------|
| `nika schedules` | `nika schedule list` |
| `nika schedule ls` | `nika schedule list` |
| `nika schedule rm` | `nika schedule remove` |

### Flags

| Flag | Applies to | Default | Description |
|------|-----------|---------|-------------|
| `--cron` | `every` | — | Raw 5-field cron |
| `--tz` | `every` | System tz | IANA timezone |
| `--name` | `every` | Filename stem | Schedule name |
| `--overlap` | `every` | `skip` | skip/queue/replace |
| `--json` | `schedule list` | — | JSON output |
| `--reason` | `schedule pause` | — | Why pausing |
| `--dry-run` | `every`, `trigger` | — | Preview only |

### Error codes

| Code | Trigger | Message |
|------|---------|---------|
| NIKA-310 | Invalid cron | "Expected 5 fields" + Did you mean? |
| NIKA-311 | Workflow not found | Fuzzy match suggestions |
| NIKA-312 | Schedule exists | Show existing + options |
| NIKA-313 | Daemon not running | "Start with: nika daemon start" |

---


## UX Mockups

### 1. Interactive Wizard (`nika every` bare)

```
  nika every

  ◇  Workflow
  │  report.nika.yaml
  │
  ◇  How often?
  │  Daily at a specific time
  │
  ◇  Time
  │  09:00
  │
  ◇  Timezone
  │  Europe/Paris (UTC+2)
  │
  ◆  Preview
  │
  │  ╭──────────────────────────────────────────────────╮
  │  │  report.nika.yaml                                │
  │  │  Every day at 09:00 Europe/Paris                 │
  │  │  cron: 0 9 * * *                                 │
  │  ├──────────────────────────────────────────────────┤
  │  │  Next 5 runs                                     │
  │  │   1.  Mon 07 Apr  09:00   in 14h                 │
  │  │   2.  Tue 08 Apr  09:00   in 1d 14h              │
  │  │   3.  Wed 09 Apr  09:00   in 2d 14h              │
  │  │   4.  Thu 10 Apr  09:00   in 3d 14h              │
  │  │   5.  Fri 11 Apr  09:00   in 4d 14h              │
  │  ├──────────────────────────────────────────────────┤
  │  │  Est. cost  ~$0.03/run  ·  ~$0.90/month          │
  │  ╰──────────────────────────────────────────────────╯
  │
  ◆  Create this schedule? (Y/n)
```

### 2. One-liner Confirmation Card

```
$ nika every 6h report.nika.yaml

  ╭─────────────────────────────────────────────────────╮
  │                                                     │
  │   ✓  Schedule created                               │
  │                                                     │
  │   Name        report-6h                             │
  │   Workflow    report.nika.yaml                       │
  │   Interval    Every 6 hours                          │
  │   Cron        0 */6 * * *                            │
  │   Timezone    UTC                                    │
  │                                                     │
  ├─────────────────────────────────────────────────────┤
  │   Next 5 runs                                       │
  │    1.  Sat 05 Apr  21:00   in 2h                    │
  │    2.  Sun 06 Apr  03:00   in 8h                    │
  │    3.  Sun 06 Apr  09:00   in 14h                   │
  │    4.  Sun 06 Apr  15:00   in 20h                   │
  │    5.  Sun 06 Apr  21:00   tomorrow                 │
  ├─────────────────────────────────────────────────────┤
  │   Cost  ~$0.03/run · ~$0.12/day · ~$3.60/month     │
  ╰─────────────────────────────────────────────────────╯

  ◆  Run immediately? (Y/n)
```

### 3. Dashboard (`nika schedule list`)

```
  ╭─ Schedules ─────────────────────────────────────────────────────────────╮
  │                                                                         │
  │  HOURLY                                                                 │
  │   ● data-sync         every 2h     ✓✓✓✓✓✓✓✓✓✓  10/10     next: 12m    │
  │     ▐████████████████░░░░░░░░▌ 67%                                      │
  │   ● metrics            every 1h     ✓✓✓✓✓✓✓✓✓✓  10/10     next: 42m    │
  │     ▐███████████░░░░░░░░░░░░▌ 30%                                       │
  │                                                                         │
  │  DAILY                                                                  │
  │   ● daily-report       09:00 Paris  ✓✓✓✓✗✓✓✓✓✓   9/10     next: 14h   │
  │     ▐██████████████████████░▌ 92%                                        │
  │                                                                         │
  │  WEEKLY                                                                 │
  │   ⏸ weekly-digest      Mon 08:00    ✓✓✓✓✓✓✓✓──   8/8      paused      │
  │                                                                         │
  │  ON-DEMAND                                                              │
  │   ✗ deploy-staging     webhook      ✓✓✓✗✗✗────   3/6      failing     │
  │     last: 2h ago · NIKA-045 fetch timeout                               │
  │                                                                         │
  ├─────────────────────────────────────────────────────────────────────────┤
  │  5 schedules │ 1 paused │ 1 failing │ next: data-sync in 12m           │
  ╰─────────────────────────────────────────────────────────────────────────╯
```

### 4. Detail Card (`nika schedule show daily-report`)

```
  ╭─ daily-report ──────────────────────────────────────────────────────────╮
  │                                                                         │
  │   Workflow     report.nika.yaml                                         │
  │   Cron         0 9 * * *                                                │
  │   Human        Every day at 09:00                                       │
  │   Timezone     Europe/Paris (UTC+2)                                     │
  │   Status       ● Active                                                 │
  │                                                                         │
  ├─ Cycle ─────────────────────────────────────────────────────────────────┤
  │                                                                         │
  │   ▐██████████████████████████████████████░░░░░░░░░░░░░░░░░░▌            │
  │   09:00 ◆ completed        ├── 9h 37m ──┤── 14h 23m ──┤ ○ next         │
  │                                                                         │
  ├─ History ───────────────────────────────────────────────────────────────┤
  │                                                                         │
  │   Next 5 runs                     Last 5 runs                           │
  │   ─────────────────────           ──────────────────────────────────    │
  │   Sun 06 Apr  09:00  in 14h      Today     09:00  ✓  3.2s    $0.022   │
  │   Mon 07 Apr  09:00  in 1d       Yesterday 09:00  ✓  3.5s    $0.024   │
  │   Tue 08 Apr  09:00  in 2d       Thu 03    09:00  ✗  1.1s    $0.008   │
  │   Wed 09 Apr  09:00  in 3d       Wed 02    09:00  ✓  3.1s    $0.021   │
  │   Thu 10 Apr  09:00  in 4d       Tue 01    09:00  ✓  2.9s    $0.020   │
  │                                                                         │
  ├─ Stats (30d) ───────────────────────────────────────────────────────────┤
  │                                                                         │
  │   Success     ████████████████████████████░░  93% (28/30)               │
  │   Duration    avg 3.1s · p50 3.0s · p95 4.8s                           │
  │   Cost        avg $0.022/run · total $0.66/month                        │
  │                                                                         │
  ╰─────────────────────────────────────────────────────────────────────────╯
```

### 5. Color Semantics

```
  Bold Green      ✓  success, active, confirm
  Bold Red        ✗  failure, error
  Bold Yellow     ⏸  paused, warning, cost
  Cyan            ●  selected, commands, progress bar fill
  Dim White       labels, descriptions, secondary text
  Bright White    values, schedule names
  Dim Gray        borders, connectors, ░ empty bar
```

---


## Implementation Plan

### Overview

6 TDD phases, 25 tests, ~850 LOC. Each phase self-contained and independently shippable.

### Phase 1: Storage (~150 LOC, 6 tests)

V5 migration: `schedules` table. `CronSchedule` struct. 6 CRUD methods.

```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
    workflow TEXT NOT NULL, cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    paused INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'cli',
    inputs_json TEXT, last_run_at TEXT, next_run_at TEXT,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
```

Tests: insert+get, get_by_name, list_ordered, update, delete, name_unique.

### Phase 2: AST — `schedule:` in YAML (~80 LOC, 5 tests)

`ScheduleConfig` struct. String-or-object parsing. Validate cron (croner) + tz (chrono-tz).

Tests: cron_string, object_form, hron_string, invalid_cron (NIKA-280), preset.

### Phase 3: Protocol + Daemon (~120 LOC, 4 tests)

6 `DaemonRequest` + 3 `DaemonResponse` variants. Refactor `fire_due_cron_jobs` → schedules table.

Tests: create_via_protocol, list_via_protocol, pause_resume, fire_reads_table.

### Phase 4: CLI — `nika every` + `nika schedule` (~250 LOC, 5 tests)

`every.rs`: hron parsing, daemon call, cliclack wizard.
`schedule.rs`: list/show/pause/resume/trigger/remove.

Tests: parse_hron, parse_cron, auto_name, name_dedup, list_empty.

### Phase 5: Display (~150 LOC, 3 tests)

Schedule card (box drawing). Dashboard list (history dots). Cost estimation. Next-run previewer.

Tests: card_fields, list_ordering, cost_format.

### Phase 6: Serve Integration (~100 LOC, 2 tests)

Startup scanner. 60s re-scan. Reconciliation (yaml vs cli source). Banner update.

Tests: discovers_yaml, reconcile_orphan.

### Dependency Graph

```
Phase 1 (Storage)
    ├──▸ Phase 2 (AST) ──▸ Phase 6 (Serve)
    └──▸ Phase 3 (Protocol)
              └──▸ Phase 4 (CLI) ──▸ Phase 5 (Display)
```

### Crate Additions

| Crate | Version | Why |
|-------|---------|-----|
| hron | 1.0 | Human-readable cron (825 tests, bidirectional) |
| chrono-tz | 0.10 | IANA timezone validation |

### New Files (6)

| File | Phase |
|------|-------|
| `nika-storage/src/schedule.rs` | 1 |
| `nika-cli/src/every.rs` | 4 |
| `nika-cli/src/schedule.rs` | 4 |
| `nika-cli/src/display/schedule_card.rs` | 5 |
| `nika-cli/src/display/schedule_list.rs` | 5 |

### Modified Files (12)

| File | Phase | Change |
|------|-------|--------|
| `nika-storage/src/lib.rs` | 1 | V5 migration + mod schedule |
| `nika-core/src/ast/raw/workflow.rs` | 2 | ScheduleConfig + schedule field |
| `nika-core/src/ast/raw/parser.rs` | 2 | "schedule" in known keys + parse |
| `nika-core/src/ast/analyzer/analyze.rs` | 2 | Validate cron + tz |
| `nika-daemon/src/protocol.rs` | 3 | 6 request + 3 response variants |
| `nika-daemon/src/server.rs` | 3 | Dispatch + fire refactor |
| `nika-daemon/src/services/jobs.rs` | 3 | fire_due_cron_jobs reads schedules |
| `nika-cli/src/lib.rs` | 4 | mod every + mod schedule |
| `nika/src/main.rs` | 4 | Every + Schedule commands |
| `nika-serve/src/lib.rs` | 6 | Scanner + reconcile + banner |

### Error Codes

| Code | Meaning |
|------|---------|
| NIKA-280 | Invalid schedule expression |
| NIKA-282 | Invalid timezone |
| NIKA-283 | Schedule not found |
| NIKA-284 | Schedule name conflict |

### Verification (before each commit)

```bash
cd tools/
cargo test --workspace --lib --exclude nika-py
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

