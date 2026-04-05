# MEGA PROMPT — Scheduling Feature Implementation (v0.72)

> **Copy-paste this into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.

---

## CONTEXT

Tu travailles sur **Nika**, un moteur de workflows YAML pour l'IA. Rust, 17 crates, ~395K LOC.
Feature 1 (on_error) est shipped. On implémente Feature 2 : Scheduling.

**IMPORTANT**: Lis ces fichiers AVANT de coder :

```bash
# Design complet (535 lines — architecture, YAML syntax, CLI, UX, implementation plan)
cat docs/plans/2026-04-05-scheduling-design.md

# UX Bible (626 lines — every interaction detail, animations, errors, helpers)
cat docs/plans/2026-04-05-scheduling-ux-bible.md

# Original blueprint (1,458 lines — storage SQL, protocol, daemon refactor)
cat docs/plans/2026-04-05-scheduling-cron-blueprint.md
```

**Workspace**: `cd tools/` pour tout cargo command. `Cargo.toml` workspace est dans `tools/`.

---

## DESIGN DECISIONS (LOCKED — ne pas changer)

### Dual Naming
- `nika every` = CLI create command (imperative, one-liner)
- `nika schedule` = CLI lifecycle (list/show/pause/resume/trigger/remove)
- `schedule:` = YAML field (declarative, source of truth)
- Le CLI ≠ le YAML field. C'est le pattern universel (kubectl apply ≠ kind: Deployment).

### YAML Syntax (string-or-object, comme `infer:`)
```yaml
# String shorthand
schedule: "every day at 9am"          # hron
schedule: "@daily"                     # preset
schedule: "0 9 * * *"                  # raw cron

# Object form
schedule:
  cron: "0 9 * * 1-5"
  timezone: "Europe/Paris"
  catchup: false
  overlap: skip            # skip | queue | replace
  jitter: 30s
  paused: false
```

### Crate Decisions (LOCKED)
| Crate | Version | Why |
|-------|---------|-----|
| hron | 1.0 | Human-readable cron (825 tests, bidirectional). MSRV 1.93 — bump if needed. |
| chrono-tz | 0.10 | IANA timezone validation |
| croner | 3.0.1 (keep) | Cron evaluation, .describe(), @presets already supported |
| cliclack | (already in deps) | Interactive wizard |

### Parsing Priority
1. Try hron → if valid, extract cron + optional timezone
2. Try @preset (@daily, @hourly, @weekly, @monthly, @yearly)
3. Try raw 5-field cron → validate with croner
4. Fail → NIKA-280

### Source Precedence
- YAML schedules = persistent (re-discovered on restart, `source: "yaml"`)
- CLI schedules = ephemeral (persist in DB, not re-registered, `source: "cli"`)

---

## EXACT CODEBASE LOCATIONS

### RawWorkflow struct
**File**: `tools/nika-core/src/ast/raw/workflow.rs:14-89`
Add `schedule:` field after `routing:` (line 79), before `max_duration_secs:` (line 82):
```rust
    /// Schedule configuration: cron expression, hron, or @preset.
    /// String form: `schedule: "@daily"`. Object form: `schedule: { cron: "...", timezone: "..." }`.
    pub schedule: Option<Spanned<serde_json::Value>>,
```

### Known workflow keys
**File**: `tools/nika-core/src/ast/raw/parser.rs:1456-1477`
Add `"schedule"` after `"routing"` (line 1474), before `"max_duration_secs"` (line 1475):
```rust
        "routing",
        "schedule",        // ← ADD THIS
        "max_duration_secs",
```

### Parse schedule field
**File**: `tools/nika-core/src/ast/raw/parser.rs` — in `parse_workflow()` function.
After `routing` parse (~line 1437), before `max_duration_secs`:
```rust
    workflow.schedule = match map.get_node("schedule") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        None => None,
    };
```

### Storage — current schema V4
**File**: `tools/nika-storage/src/lib.rs:21`
```rust
const SCHEMA_VERSION: u32 = 4;  // → bump to 5
```

### Daemon protocol
**File**: `tools/nika-daemon/src/protocol.rs`
Wire format: 4-byte length prefix (u32 big-endian) + JSON payload. Max 16 MB.

Existing variants (for pattern reference):
```rust
DaemonRequest::JobSubmit { workflow, name, args, cron, max_retries }
DaemonRequest::JobList { state }
DaemonRequest::JobStatus { id }
```

Add 6 new variants:
```rust
DaemonRequest::ScheduleCreate { name, workflow, cron_expr, timezone, inputs }
DaemonRequest::ScheduleList
DaemonRequest::ScheduleGet { name }
DaemonRequest::SchedulePause { name, reason }
DaemonRequest::ScheduleResume { name }
DaemonRequest::ScheduleDelete { name }
```

### Daemon cron scheduler
**File**: `tools/nika-daemon/src/services/jobs.rs:472-554`
- `run_cron_scheduler()` — 60s tick loop (line 472)
- `fire_due_cron_jobs()` — reads jobs with cron column (line 486)
- Refactor to read `schedules` table instead

### Serve config
**File**: `tools/nika-serve/src/config.rs:113` — `ServeConfig` struct
**File**: `tools/nika-serve/src/lib.rs:356` — workflow scanning at startup

### Main.rs command dispatch
**File**: `tools/nika/src/main.rs`
- Commands are a clap enum. Add `Every` and `Schedule` variants.
- Pattern: grep for `Command::` to see all existing commands.

### Existing `nika job` command
**File**: `tools/nika-cli/src/jobs.rs`
- `JobAction` enum with Submit, List, Status, Cancel, Retry, History
- Already talks to daemon via `DaemonClient`
- `nika every` follows the same pattern

---

## RULES

### Skills obligatoires
- **test-driven-development**: RED → GREEN → REFACTOR
- **verification-before-completion**: `cargo test --workspace --lib --exclude nika-py` + `cargo clippy --workspace -- -D warnings` + `cargo fmt --all --check` AVANT chaque commit
- **systematic-debugging**: Si un test casse, diagnose root cause avant de fix
- **rust**: Toujours `--lib` pour éviter les popups Keychain macOS

### Commits
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
1 phase = 1 commit. Push quand tout est vert.

### Conventions
- Errors: `NikaError` avec codes NIKA-XXX
- AST: Raw → Analyzed → Lower
- Tests: `cargo test --lib` toujours
- Zero dead code, zero backward compat
- License: AGPL-3.0-or-later

---

## PHASE 1: Storage (~150 LOC, 6 tests)

### What
`CronSchedule` struct + V5 migration + 6 CRUD methods.

### V5 SQL
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
CREATE INDEX idx_schedules_workflow ON schedules(workflow);
```

### Files
- `tools/nika-storage/src/lib.rs` — bump SCHEMA_VERSION 4→5, add V5 migration block, add `pub mod schedule;`
- `tools/nika-storage/src/schedule.rs` — CREATE: CronSchedule struct + 6 methods + 6 tests

### TDD Tests
1. `test_schedule_insert_and_get`
2. `test_schedule_get_by_name`
3. `test_schedule_list_ordered` (by next_run_at)
4. `test_schedule_update`
5. `test_schedule_delete`
6. `test_schedule_name_unique` (constraint error)

---

## PHASE 2: AST — `schedule:` in YAML (~80 LOC, 5 tests)

### What
Parse `schedule:` from workflow YAML. Validate cron/hron at analysis time.

### ScheduleConfig struct
```rust
/// Parsed schedule configuration.
#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    /// Canonical cron expression (always 5-field after parsing).
    pub cron: String,
    /// IANA timezone. None = UTC.
    pub timezone: Option<String>,
    /// Original human-readable string (for display). None if raw cron.
    pub human: Option<String>,
    /// Overlap policy: skip (default), queue, replace.
    pub overlap: Option<String>,
    /// Start paused?
    pub paused: Option<bool>,
    /// Span for diagnostics.
    pub span: Span,
}
```

### Files
- `tools/nika-core/src/ast/raw/workflow.rs:79` — add `schedule: Option<Spanned<serde_json::Value>>`
- `tools/nika-core/src/ast/raw/parser.rs:1437` — parse schedule field
- `tools/nika-core/src/ast/raw/parser.rs:1474` — add "schedule" to known_workflow_keys
- `tools/nika-core/src/ast/analyzed/workflow.rs` — add `schedule: Option<ScheduleConfig>` to AnalyzedWorkflow
- `tools/nika-core/src/ast/analyzer/analyze.rs` — validate cron (croner) + tz (chrono-tz) + hron parsing
- `tools/nika-core/Cargo.toml` — add hron, chrono-tz

### Parsing Logic
```rust
fn parse_schedule_config(value: &serde_json::Value, span: Span) -> Result<ScheduleConfig> {
    match value {
        Value::String(s) => {
            // 1. Try hron
            if let Ok(schedule) = hron::Schedule::parse(s) {
                let cron = schedule.to_cron().unwrap_or_else(|_| s.clone());
                return Ok(ScheduleConfig {
                    cron, timezone: schedule.timezone().map(|tz| tz.to_string()),
                    human: Some(s.clone()), overlap: None, paused: None, span,
                });
            }
            // 2. Try @preset — croner handles these
            // 3. Try raw cron
            croner::Cron::new(s).map_err(|e| /* NIKA-280 */)?;
            Ok(ScheduleConfig { cron: s.clone(), timezone: None, human: None, overlap: None, paused: None, span })
        }
        Value::Object(map) => {
            let cron_str = map.get("cron").or(map.get("every"))
                .and_then(|v| v.as_str())
                .ok_or(/* NIKA-280: missing cron field */)?;
            // Parse cron_str same as string form
            // Extract timezone, overlap, paused from map
        }
    }
}
```

### TDD Tests
1. `test_schedule_cron_string` — `schedule: "0 9 * * *"`
2. `test_schedule_object_form` — `schedule: { cron: "...", timezone: "..." }`
3. `test_schedule_hron_string` — `schedule: "every weekday at 9am"`
4. `test_schedule_invalid` — `schedule: "not a cron"` → NIKA-280
5. `test_schedule_preset` — `schedule: "@daily"`

---

## PHASE 3: Protocol + Daemon (~120 LOC, 4 tests)

### What
6 DaemonRequest + 3 DaemonResponse variants. Refactor fire_due_cron_jobs.

### Files
- `tools/nika-daemon/src/protocol.rs` — add 9 variants
- `tools/nika-daemon/src/server.rs` — add 6 dispatch arms
- `tools/nika-daemon/src/services/jobs.rs` — refactor fire_due_cron_jobs → read schedules table
- `tools/nika-daemon/Cargo.toml` — add chrono-tz

### TDD Tests
1. `test_schedule_create_via_protocol`
2. `test_schedule_list_via_protocol`
3. `test_schedule_pause_resume`
4. `test_fire_due_reads_schedules_table`

---

## PHASE 4: CLI — `nika every` + `nika schedule` (~250 LOC, 5 tests)

### What
Two new CLI modules. Interactive wizard. Beautiful output.

### Files
- `tools/nika-cli/src/every.rs` — CREATE: hron parsing + daemon call + wizard
- `tools/nika-cli/src/schedule.rs` — CREATE: list/show/pause/resume/trigger/remove
- `tools/nika-cli/src/lib.rs` — add mod every, mod schedule
- `tools/nika/src/main.rs` — add Every + Schedule command variants + dispatch

### `nika every` arg parsing
```
nika every                                    → wizard (cliclack)
nika every 6h report.nika.yaml                → parse "6h" as interval
nika every day at 9am report.nika.yaml        → parse as hron
nika every weekday at 9am report.nika.yaml    → parse as hron
nika every --cron "0 */6 * * *" report.nika.yaml → raw cron
```

Rule: the LAST argument is always the workflow path. Everything before it is the schedule expression.

### TDD Tests
1. `test_parse_hron_to_cron`
2. `test_parse_raw_cron_passthrough`
3. `test_auto_name_from_workflow`
4. `test_auto_name_dedup`
5. `test_schedule_list_empty`

---

## PHASE 5: Display (~150 LOC, 3 tests)

### What
Schedule card, dashboard list, cost estimation, next-run previewer.

### Key Renderers
1. **Schedule card** — box drawing, colors, next 5 runs, cost
2. **Dashboard list** — tree by frequency, history dots ✓✓✓✗, progress bar
3. **Cost estimation** — avg from last N runs, project per day/month
4. **Next-run previewer** — croner .find_next_occurrence() × 5

### UX Requirements (from UX Bible)
- Cascading celebration: 3-step spinner → checkmark (200ms each)
- Braille spinners: ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ at 80ms
- Semantic colors: green=active, yellow=paused, red=failed, cyan=selected
- Progress bar: ▐████████░░░░░░░░▌ for cycle position
- History dots: ✓✓✓✓✗ (last 10 runs inline)
- Cost warning when > $10/month
- Overlap detection warning
- "is live! 🦋" celebration
- "Run it now to test?" proactive offer

### Error UX (from UX Bible)
- Did-you-mean for misspelled commands, names, cron fields
- Numbered fix suggestions for failing schedules
- Auto-pause countdown ("2 more failures before auto-pause")
- Cron cheat sheet via `nika help cron`
- Empty state: "Get started in 10 seconds" + examples

### TDD Tests
1. `test_schedule_card_contains_fields`
2. `test_dashboard_list_ordering`
3. `test_cost_estimation_formats`

---

## PHASE 6: Serve Integration (~100 LOC, 2 tests)

### What
`nika serve` auto-discovers `schedule:` from YAML files. Reconciles with DB.

### Discovery flow
1. Startup: scan all `*.nika.yaml` headers → extract `schedule:`
2. For each: upsert into schedules table with `source: "yaml"`
3. Every 60s: re-scan, add new, update changed, pause orphans
4. Startup banner: "4 scheduled workflows (3 active, 1 paused)"

### Reconciliation rules
| Scenario | Action |
|----------|--------|
| YAML has schedule, DB has no entry | Insert (source: yaml) |
| YAML has schedule, DB matches | No-op |
| YAML has schedule, DB cron differs | Update cron + recompute next_run_at |
| YAML removed schedule, DB has source:yaml | Pause + log warning |
| DB entry has source:cli | Never touched by reconciliation |

### Files
- `tools/nika-serve/src/lib.rs` — add scanner + reconcile + banner update

### TDD Tests
1. `test_serve_discovers_yaml_schedules`
2. `test_serve_reconcile_removes_orphan`

---

## ERROR CODES

| Code | Meaning | Phase |
|------|---------|-------|
| NIKA-280 | Invalid schedule expression (cron/hron parse failure) | 2 |
| NIKA-282 | Invalid timezone (chrono-tz parse failure) | 2 |
| NIKA-283 | Schedule not found by name | 3 |
| NIKA-284 | Schedule name conflict (duplicate) | 3 |

---

## VERIFICATION (before each commit)

```bash
cd tools/
cargo test --workspace --lib --exclude nika-py
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

---

## AUDIT FINDINGS (2 agents, verified)

### hron Compatibility — CONFIRMED OK
- Rust 1.94 > hron MSRV 1.93 ✓
- jiff (hron dep) coexists with chrono (Nika dep) — zero conflicts ✓
- MIT license compatible with AGPL ✓
- croner @presets already work (@daily, @hourly, @weekly, @monthly, @yearly) ✓
- croner .describe() gives human-readable output ✓
- **hron adds**: "every weekday at 9am", "every 30 min from 09:00 to 17:00", timezone modifier, exception dates

### Daemon cron scheduler — ALREADY WORKS
- `fire_due_cron_jobs()` at `services/jobs.rs:486-554` — production-ready
- 60s tick loop, overlap protection, croner parsing
- Just needs refactoring to read `schedules` table instead of `jobs.cron` column

### Gaps found (address in implementation)
1. **nika.toml `[scheduler]`** — hardcoded 60s polling. Add `scan_interval` config field in Phase 6.
2. **TUI `--view scheduler`** — referenced at main.rs:164 but unimplemented. Defer to Phase 7 (separate sprint).
3. **LSP completions for `schedule:`** — no inline schema. Add to LSP completions in a follow-up.
4. **AnalyzedWorkflow** — needs `schedule: Option<ScheduleConfig>` added alongside RawWorkflow.

---

## WHAT NOT TO DO

- No new verbs (5 verbs sacred)
- No changes to existing `nika job` commands (they coexist)
- No WebSocket (SSE/polling is fine)
- No TUI integration yet (Phase 7, separate sprint)
- Don't rename `fire_due_cron_jobs` — refactor its body to read schedules table
- Don't delete the Job.cron column — deprecate gradually
- hron MSRV 1.93 — CONFIRMED OK (Nika is Rust 1.94)

---

## START HERE

1. Lis le design: `cat docs/plans/2026-04-05-scheduling-design.md`
2. Lis la UX bible: `cat docs/plans/2026-04-05-scheduling-ux-bible.md`
3. Crée des TodoWrite tasks pour chaque phase
4. TDD: RED → GREEN → REFACTOR
5. Commit par phase, push quand tout est vert
