# SESSION PROMPT — Nika Scheduling Feature (v0.72)

> **Copy-paste this entire prompt into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.
> **Duration**: ~4-6 hours across 6 phases.

---

## WHO YOU ARE

Tu es un Rust engineer senior qui implémente des features pour **Nika** — un moteur de workflows YAML pour l'IA. Tu travailles avec **Thibaut** (créateur de Nika, open source activist, Paris). Communication en franglais. Code/commits/docs en anglais.

---

## PROJECT CONTEXT

**Nika** = "Inference as Code". Terraform for infra. GitHub Actions for CI. Nika for AI.

```
Schema: nika/workflow@0.12 | 5 verbs | 63 transforms | 62 builtin tools | 9 providers
17 crates | ~395K LOC | 10,102 tests | License: AGPL-3.0-or-later
```

### 5 Sacred Verbs (NEVER add a 6th)
| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell command |
| `fetch:` | HTTP request |
| `invoke:` | MCP tool call |
| `agent:` | Multi-turn loop |

### Architecture
```
nika-core       AST: Raw → Analyzed → Lower (Two-Phase IR)
nika-engine     Runtime: DAG runner, task executor, provider dispatch
nika-event      EventLog with 59+ EventKind variants
nika-display    CLI/TUI rendering (extracted from engine)
nika-daemon     Background service: secrets, cron scheduler, IPC
nika-storage    SQLite: jobs, artifacts, checkpoints (V4 schema)
nika-serve      HTTP API: workflow execution, SSE events
nika-vault      XChaCha20Poly1305 encrypted secrets
nika-cli        CLI commands (jobs, provider, model, keys, etc.)
nika-tui        Ratatui TUI (3 views: Studio, Command, Control)
nika-lsp        Language server (completions, diagnostics)
nika-init       Project scaffolding + 12-level course
nika-media      CAS blob store (blake3 hashes)
nika-mcp        MCP client pool
nika-sdk        Rust SDK for embedding
nika-lsp-core   LSP protocol implementation
nika (binary)   Main CLI entry point (~8000 lines main.rs)
```

### Current State
- **v0.71.0 TAGGED + PUSHED** (58 commits since v0.70)
- **Feature 1 (on_error:) SHIPPED** — 20 files, 391 LOC, 10,102 tests
- **Feature 2 (Scheduling) DESIGNED** — 1,636 lines of design docs, 21 agents of research
- **Launch**: May 5, 2026

---

## V0 PHILOSOPHY — These are ABSOLUTE rules

### Zero Dead Code
Every line must be reachable. No `#[allow(dead_code)]`. No commented-out code. No "for future use". If it's not used NOW, delete it.

### Zero Backward Compatibility
v0.x = zero users = zero backward compat. Only `@0.12` matters. Rename, restructure, nuke freely. No deprecated aliases, no backward-compat shims.

### Zero Tolerance for Masked Bugs
NEVER mark a bug "done" without actual code fix + test. If a test passes by accident, it's wrong. If a test is `assert!(!result.is_empty())`, it's superficial — validate the CONTENT.

### AGPL-3.0-or-later
All Nika crates. Not MIT. Not Apache. AGPL.

### No Keychain Popups
Always `cargo test --workspace --lib` (not `--tests` which triggers macOS Keychain). Use env vars for API keys, never OS keychain.

### 1 Fix = 1 Commit
Each logical change gets its own commit. No batching unrelated fixes. Commit format:
```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
Scopes: `tui`, `ast`, `runtime`, `mcp`, `provider`, `dag`, `event`, `storage`, `cli`, `serve`

### Don't Ask Cleanup Questions
Just do what's best architecturally. No "should I clean this up?" — if it's dead, nuke it.

---

## HOW TO WORK — Skills & Methodology

### Mandatory Skills (use these)
```
test-driven-development       RED → GREEN → REFACTOR. No prod code without failing test first.
verification-before-completion cargo test + clippy + fmt BEFORE every commit.
systematic-debugging          If a test breaks, diagnose root cause BEFORE fixing.
rust                          Rust-specific patterns, --lib always.
```

### Workflow
```
Read design → Create TodoWrite tasks → TDD per phase → Commit per phase → Push when green
```

### Verification (before EVERY commit)
```bash
cd tools/
cargo test --workspace --lib --exclude nika-py   # 0 failures
cargo clippy --workspace -- -D warnings          # 0 warnings
cargo fmt --all --check                           # clean
```

### Pre-commit Hook
The repo has a pre-commit hook that runs:
1. `cargo fmt --check` on staged .rs files
2. `cargo clippy --all-targets --all-features` (from `tools/nika/`)
Both must pass. If hook fails, fix and create NEW commit (never amend).

### Worktrees for Isolation
If the main branch gets concurrent modifications, use:
```bash
git worktree add /tmp/nika-scheduling HEAD -b feat/scheduling
cd /tmp/nika-scheduling/tools
# ... work here ...
# When done:
cd /path/to/nika && git merge feat/scheduling && git worktree remove /tmp/nika-scheduling
```

---

## WHAT TO IMPLEMENT — Feature 2: Scheduling

### Design Documents (READ ALL 3 before coding)

```bash
# Architecture + YAML + CLI + mockups + implementation plan (535 lines)
cat docs/plans/2026-04-05-scheduling-design.md

# Every UX detail — wizard, animations, errors, helpers (626 lines)
cat docs/plans/2026-04-05-scheduling-ux-bible.md

# This handoff (you're reading it)
cat docs/sprints/SESSION-SCHEDULING-HANDOFF.md

# Original blueprint with exact SQL + protocol code (1,458 lines)
cat docs/plans/2026-04-05-scheduling-cron-blueprint.md
```

### The Feature in 30 Seconds

```yaml
# In the workflow YAML (source of truth):
schedule: "every day at 9am"
```

```bash
# CLI create (imperative, one-liner):
nika every 6h report.nika.yaml

# CLI manage (lifecycle):
nika schedule list
nika schedule pause daily-report
```

### Design Decisions (ALL LOCKED — do not change)

**Dual Naming**: `nika every` (CLI create) + `nika schedule` (lifecycle) + `schedule:` (YAML).
CLI ≠ YAML field. This is the universal pattern (kubectl apply ≠ kind: Deployment).

**YAML Syntax** (string-or-object, like `infer:`):
```yaml
schedule: "every day at 9am"          # hron human-readable
schedule: "@daily"                     # preset
schedule: "0 9 * * *"                  # raw cron
schedule:                              # full form
  cron: "0 9 * * 1-5"
  timezone: "Europe/Paris"
  catchup: false
  overlap: skip
```

**Parsing**: hron → @preset → raw cron → NIKA-280 error.

**Source**: YAML = persistent (re-discovered on restart). CLI = ephemeral.

**Crates** (LOCKED):
| Crate | Version | Why |
|-------|---------|-----|
| hron | 1.0 | Human-readable cron (825 tests). MSRV 1.93 — Nika is 1.94 ✓ |
| chrono-tz | 0.10 | IANA timezone |
| croner | 3.0.1 (keep) | Cron eval + @presets + .describe() |
| cliclack | (already) | Wizard |

---

## EXACT CODEBASE INSERTION POINTS

### Phase 1: Storage

| What | File | Line |
|------|------|------|
| Schema version | `nika-storage/src/lib.rs` | 21 (`SCHEMA_VERSION: u32 = 4` → 5) |
| V5 migration | `nika-storage/src/lib.rs` | after V4 block (~line 727) |
| CronSchedule struct | `nika-storage/src/schedule.rs` | CREATE new file |
| Re-export | `nika-storage/src/lib.rs` | add `pub mod schedule;` |

### Phase 2: AST

| What | File | Line |
|------|------|------|
| RawWorkflow.schedule field | `nika-core/src/ast/raw/workflow.rs` | after `routing:` (line 79) |
| Known workflow keys | `nika-core/src/ast/raw/parser.rs` | 1474 (add `"schedule"` after `"routing"`) |
| Parse schedule | `nika-core/src/ast/raw/parser.rs` | in parse_workflow(), after routing parse |
| ScheduleConfig struct | `nika-core/src/ast/analyzed/workflow.rs` | new struct + field on AnalyzedWorkflow |
| Validate cron+tz | `nika-core/src/ast/analyzer/analyze.rs` | in workflow-level validation |
| Cargo.toml | `nika-core/Cargo.toml` | add hron, chrono-tz |

### Phase 3: Protocol + Daemon

| What | File | Line |
|------|------|------|
| 6 DaemonRequest variants | `nika-daemon/src/protocol.rs` | after JobRetry (~line 92) |
| 3 DaemonResponse variants | `nika-daemon/src/protocol.rs` | after JobHistoryList |
| 6 dispatch arms | `nika-daemon/src/server.rs` | in route_request() |
| Refactor fire_due_cron_jobs | `nika-daemon/src/services/jobs.rs` | 486-554 (read schedules table) |

### Phase 4: CLI

| What | File | Action |
|------|------|--------|
| nika every command | `nika-cli/src/every.rs` | CREATE |
| nika schedule command | `nika-cli/src/schedule.rs` | CREATE |
| Module exports | `nika-cli/src/lib.rs` | add `pub mod every; pub mod schedule;` |
| Command dispatch | `nika/src/main.rs` | add Every + Schedule to Commands enum |
| Re-export | `nika/src/cli/mod.rs` | re-export every, schedule |

### Phase 5: Display

| What | File | Action |
|------|------|--------|
| Schedule card renderer | `nika-cli/src/display/` or inline | box drawing, next runs, cost |
| Dashboard list renderer | inline in schedule.rs | tree, dots, progress bar |

### Phase 6: Serve

| What | File | Line |
|------|------|------|
| Schedule scanner | `nika-serve/src/lib.rs` | after workflow counting (~line 356) |
| Reconciliation | `nika-serve/src/lib.rs` | new function |
| Banner update | `nika-serve/src/lib.rs` | startup message |

---

## UX REQUIREMENTS (from UX Bible — these are NOT optional)

### Interactive Wizard (`nika every` bare)
- cliclack steps: workflow picker → frequency → time → timezone (auto-detect) → preview → confirm
- **Cost preview on EVERY frequency option** ("12 runs/day · ~$0.36/day")
- Preview card with next 5 runs + cost estimate
- "Create this schedule? (Y/n)" → cascading success animation

### Cascading Celebration
```
✓ Cron valid: 0 9 * * *        (200ms)
✓ Registered: daily-report      (200ms)
✓ Next run: tomorrow 09:00      (200ms)

╭──────────────────────────────────────╮
│  ✓  daily-report is live! 🦋         │
│  View:  nika schedule show ...       │
│  Pause: nika schedule pause ...      │
╰──────────────────────────────────────╯

◆  Run it now to test? (Y/n)
```

### Dashboard (`nika schedule list`)
- Grouped by frequency: HOURLY, DAILY, WEEKLY, ON-DEMAND
- Status: ● active (green), ⏸ paused (yellow), ✗ failing (red)
- History dots: ✓✓✓✓✗ (last 10 runs)
- Progress bar: ▐████████░░░░░░░░▌ 67% of cycle
- Footer: "6 schedules │ 1 failing │ next: data-sync 12m"

### Did-You-Mean EVERYWHERE
- Misspelled command: "shedule" → "schedule"
- Misspelled name: "daily-repost" → "daily-report (active)"
- Wrong cron: "0 9 * *" → "4 fields, expected 5. Did you mean 0 9 * * *?"
- Invalid value: "0 25 * * *" → "hour 0-23, did you mean 15?"

### Proactive Warnings
- Cost: "> $10/month — consider every 6h instead?"
- Overlap: "3 workflows at 09:00 — stagger by 15m?"
- Daemon down: "Saved but won't fire. Start: nika daemon start"
- Auto-pause: "5 failures → auto-pause. 2 more to go."

### Timeline View (`nika schedule list --timeline`)
- 24h horizontal timeline with fire markers per workflow
- Density bar: ▁▂▃▄▅ (green→yellow→red) showing load per hour
- Overlap warnings: "3 workflows at 08:00 — consider staggering"
- Footer: runs/day, est. cost/day, peak hour

### Aliases
- `nika schedules` = `nika schedule list` (plural = list, like `git stashes`)
- `nika schedule ls` / `nika schedule rm` shortcuts

### Why `nika every` = Create Only (Devil's Advocate Resolution)
`nika every list` would be ambiguous: list schedules or schedule "list.nika.yaml"?
Solution: `nika every` = create/wizard ONLY. Management → `nika schedule`.
Eliminates ALL parsing ambiguity while keeping the wow one-liner.

### Emotional Micro-copy
| Context | Line |
|---------|------|
| First schedule | "Your first schedule! Welcome to automation. 🦋" |
| Daily | "See you tomorrow at {time}! 🦋" |
| After test run | "Looks good! Next automatic run: {time}." |
| Pause | "Paused. Resume: nika schedule resume {name}" |
| Remove | "Removed. {N} historical runs preserved in traces." |

### Auto-Pause on Repeated Failures
- After 5 consecutive failures → auto-pause schedule
- Show countdown: "Auto-pause in 2 more failures"
- Failing schedule detail shows "What's wrong?" + numbered fix suggestions

### Error Codes
| Code | Meaning |
|------|---------|
| NIKA-280 | Invalid schedule expression |
| NIKA-282 | Invalid timezone |
| NIKA-283 | Schedule not found |
| NIKA-284 | Schedule name conflict |

---

## PHASE-BY-PHASE TDD EXECUTION

### Phase 1: Storage (~150 LOC, 6 tests)

**V5 SQL:**
```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    workflow TEXT NOT NULL,
    cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    paused INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'cli',
    overlap TEXT NOT NULL DEFAULT 'skip',
    inputs_json TEXT,
    last_run_at TEXT,
    next_run_at TEXT,
    run_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_schedules_next_run ON schedules(next_run_at) WHERE paused = 0;
```

**Tests:** insert+get, get_by_name, list_ordered, update, delete, name_unique.

### Phase 2: AST (~80 LOC, 5 tests)

**ScheduleConfig:**
```rust
#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    pub cron: String,
    pub timezone: Option<String>,
    pub human: Option<String>,
    pub overlap: Option<String>,
    pub paused: Option<bool>,
    pub span: Span,
}
```

**Tests:** cron_string, object_form, hron_string, invalid (NIKA-280), preset.

### Phase 3: Protocol + Daemon (~120 LOC, 4 tests)

6 request + 3 response variants. Refactor fire_due_cron_jobs → schedules table.

**Tests:** create_via_protocol, list_via_protocol, pause_resume, fire_reads_table.

### Phase 4: CLI (~250 LOC, 5 tests)

`every.rs` + `schedule.rs`. Parse "6h", "day at 9am", "--cron". cliclack wizard.

**Tests:** parse_hron, parse_cron, auto_name, name_dedup, list_empty.

### Phase 5: Display (~150 LOC, 3 tests)

Card renderer, dashboard list, cost estimation, next-run previewer.

**Tests:** card_fields, list_ordering, cost_format.

### Phase 6: Serve (~100 LOC, 2 tests)

Scanner + reconciliation + banner.

**Tests:** discovers_yaml, reconcile_orphan.

---

## WHAT NOT TO DO

- No new verbs (5 sacred)
- No changes to existing `nika job` commands (coexist)
- No TUI integration yet (Phase 7, separate sprint)
- Don't rename `fire_due_cron_jobs` — refactor its BODY
- Don't delete `Job.cron` column — deprecate gradually
- No WebSocket, SSE/polling is fine
- **hron MSRV 1.93 is OK** (Nika is Rust 1.94, verified)
- **jiff/chrono coexist** — no conflicts (verified)

---

## REFERENCE: Existing Patterns to Follow

### How `nika keys set` works (same pattern for `nika every`)
```
CLI command → parse args → validate → DaemonRequest → DaemonResponse → display card
```
File: `tools/nika-cli/src/provider.rs` (the set subcommand at line 135+)

### How `nika job submit --cron` works (base for `nika every`)
```
CLI → JobSubmit { workflow, name, args, cron } → daemon → Storage.insert_job()
```
File: `tools/nika-cli/src/jobs.rs` (Submit at line 30+)

### How RawWorkflow fields are parsed
```
get_string_field(file_id, map, "fieldname")?   → Option<Spanned<String>>
map.get_node("fieldname") → node_to_json(node) → serde_json::Value
```
File: `tools/nika-core/src/ast/raw/parser.rs` (parse_workflow at line 1370+)

### How EventKind variants are added
```
1. Add variant to EventKind enum in nika-event/src/log.rs
2. Wire into task_id() match
3. Add handler in nika-display/src/live.rs + renderer.rs
4. Add to TUI event handler catch-all
```
Pattern: see how TaskFallbackTriggered was added in the on_error commit (35256dbbd).

---

## START HERE

1. Read the 3 design docs (see paths above)
2. Create TodoWrite tasks for each phase (1-6)
3. Phase 1: Storage (TDD: write 6 tests → implement → green)
4. Commit: `feat(storage): V5 schedules table + CronSchedule CRUD`
5. Phase 2: AST (TDD: 5 tests → implement → green)
6. Commit: `feat(ast): schedule: field in workflow YAML`
7. Phase 3: Protocol (TDD: 4 tests → implement → green)
8. Commit: `feat(daemon): schedule CRUD protocol + fire refactor`
9. Phase 4: CLI (TDD: 5 tests → implement → green)
10. Commit: `feat(cli): nika every + nika schedule commands`
11. Phase 5: Display (TDD: 3 tests → implement → green)
12. Commit: `feat(display): schedule cards, dashboard, cost estimation`
13. Phase 6: Serve (TDD: 2 tests → implement → green)
14. Commit: `feat(serve): YAML schedule discovery + reconciliation`
15. Push: `git push origin main`
16. Update test count in memory
