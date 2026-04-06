# SESSION PROMPT — Scheduling Polish (v0.72 → v0.73)

> **Copy-paste this into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.

---

## WHO YOU ARE

Rust engineer senior sur **Nika** — workflow engine YAML pour l'IA. Communication franglais, code/commits EN. Tu travailles avec Thibaut (créateur, Paris).

---

## CONTEXT — Ce qui a déjà été fait

Le scheduling feature est implémenté et fonctionnel (v0.72). Voici ce qui existe :

### Architecture
```
nika every 6h report.nika.yaml    → DaemonRequest::ScheduleCreate
                                   → schedules table (SQLite V5)
                                   → fire_due_cron_jobs() reads table every 60s
                                   → submits nika run <workflow>

nika schedule list/show/pause/resume/remove  → daemon CRUD
schedule: "@daily" (YAML field)              → AST parsing + analyzer validation
nika serve                                   → startup scanner (header-only)
```

### Crates touchés
| Crate | File | What |
|-------|------|------|
| nika-storage | `src/lib.rs` | CronSchedule struct, V5 migration, 7 CRUD methods, 8 tests |
| nika-core | `src/ast/schedule.rs` | ScheduleConfig, parse_schedule_value(), duration_to_cron() (pub), 8 tests |
| nika-core | `src/ast/analyzer/analyze.rs` | Wiring schedule validation into analyzer |
| nika-daemon | `src/protocol.rs` | 6 DaemonRequest + 3 DaemonResponse variants |
| nika-daemon | `src/server.rs` | 6 dispatch arms with cron/tz/overlap validation |
| nika-daemon | `src/services/jobs.rs` | fire_from_schedules_table() + legacy fallback |
| nika-cli | `src/every.rs` | nika every command + hron/cron/duration parsing + celebration |
| nika-cli | `src/schedule.rs` | nika schedule lifecycle + render_schedule_card() + next-5-runs |
| nika-serve | `src/lib.rs` | scan_scheduled_workflows() + banner |

### Audit résultats (6 agents, 2026-04-06)
- 2 CRITICAL fixes applied (fire_due_cron_jobs + next_run_at)
- 7 HIGH fixes applied (overlap validation, unwrap, DRY, trigger stub, etc.)
- 3 UX improvements applied (celebration, next-5-runs, schedules alias)
- **Remaining: 6 deferred items below**

### Baseline
- **10,208 tests**, 0 failures, clippy clean, fmt clean
- v0.72, Rust 1.94, AGPL-3.0-or-later

---

## ÉTAPE 1 — Analyser l'état actuel (AVANT tout)

```bash
git log --oneline -10                                            # Recent commits
git status && git diff --stat                                    # Dirty state
cd tools && cargo test --workspace --lib --exclude nika-py 2>&1 | tail -5
cd tools && cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cd tools && cargo fmt --all --check 2>&1 | head -3
```

---

## 6 DEFERRED ITEMS — Implémente-les dans l'ordre

### ITEM 1 — Interactive wizard (`nika every` bare) — ~100 LOC

**What**: Quand l'utilisateur tape `nika every` sans arguments, lancer un wizard interactif cliclack.

**Current**: every.rs retourne une erreur usage.

**Design** (from UX Bible at `docs/plans/2026-04-05-scheduling-ux-bible.md`):
```
$ nika every

  ┌  nika every · Schedule a recurring workflow
  │
  ◆  Which workflow?
  │  (fuzzy search list of *.nika.yaml files)
  │
  ◆  How often?
  │  ● Every few hours / Every day / Every weekday / Every week / Type it yourself
  │
  ◆  Time (for daily/weekly)
  │  09:00 (24h format, live cron preview)
  │
  ◆  Timezone
  │  ● Europe/Paris (detected) / UTC / Other...
  │
  ◆  Preview card (next 5 runs + cost if available)
  │
  ◆  Create? (Y/n)
```

**Pattern to follow**: `tools/nika-cli/src/keys.rs` uses cliclack extensively (lines 990-1196). Copy the cliclack::intro/select/password/confirm/outro patterns.

**Exact insertion point**: `tools/nika-cli/src/every.rs`, replace the early return error block (currently lines ~46-56) with the wizard.

**Steps**:
1. Discover workflows: `glob::glob("**/*.nika.yaml")` or shell `find . -name "*.nika.yaml"`
2. `cliclack::select()` for workflow
3. `cliclack::select()` for frequency (hourly/daily/weekday/weekly/custom)
4. If daily/weekly: `cliclack::input()` for time (validate HH:MM format)
5. `cliclack::select()` for timezone (auto-detect system tz, offer UTC, Other with input)
6. Show preview (reuse existing card rendering)
7. `cliclack::confirm()` to create
8. Call the existing DaemonRequest::ScheduleCreate flow

**Timezone auto-detect**: use `iana_time_zone::get_timezone()` (add `iana-time-zone = "0.1"` to workspace + nika-cli deps). Fallback to UTC if detection fails.

**Tests**: 2 unit tests (workflow discovery, timezone detection).

---

### ITEM 2 — Cost estimation — ~80 LOC

**What**: Show estimated cost per run/day/month on schedule creation and in schedule show.

**Current**: No cost data anywhere in scheduling.

**Design** (UX Bible):
```
  │  Cost estimate (based on last run: $0.031)
  │  Per run      $0.031    ~2,400 tokens in · ~800 out
  │  Per day      $0.37     12 runs
  │  Per month    $11.16    ~360 runs
  │
  │  ⚠ $11/month — want to reduce? Try "every 6h" ($3.60/mo)
```

**Data source**: The daemon already tracks job cost in the jobs table. Need to:
1. Add a `GetLastRunCost { workflow: String }` request/response to protocol
2. Server dispatch: query jobs table for most recent completed job for this workflow, extract cost
3. CLI: after ScheduleCreated, fetch last run cost, compute daily/monthly estimates
4. Cost warning: if monthly > $10, suggest cheaper interval

**Alternatively** (simpler, v1): Just compute runs/day from cron and show that. Cost per run is unknown until first execution — show "first run will calibrate" placeholder. This is the pragmatic v1.

**Exact files**: 
- `nika-cli/src/every.rs` — add cost section after next-5-runs in celebration
- `nika-cli/src/schedule.rs` — add cost section in render_schedule_card()

---

### ITEM 3 — History dots + sparkline — ~60 LOC

**What**: Show run history as ✓✓✓✗✓ dots in `nika schedule list` and `nika schedule show`.

**Current**: Only shows `run_count` number.

**Design** (UX Bible):
```
  ● daily-report       09:00 Paris  ✓✓✓✓✗✓✓✓✓✓   9/10     next: 14h
```

**Data needed**: The daemon already stores job history (job_history table). Need:
1. Add `ScheduleHistory { name: String, limit: u32 }` request/response to protocol
2. Server dispatch: query jobs table for recent jobs matching schedule workflow, return status list
3. CLI: render as colored dots (green ✓, red ✗, dim ─ for not-yet-run)

**Exact files**:
- `nika-daemon/src/protocol.rs` — add request/response variant
- `nika-daemon/src/server.rs` — add dispatch arm
- `nika-cli/src/schedule.rs` — render dots in list + show card

**Tests**: 1 test for dot rendering (given statuses, verify string output).

---

### ITEM 4 — Timeline view (`nika schedule list --timeline`) — ~120 LOC

**What**: 24h ASCII timeline showing when schedules fire, with overlap warnings.

**Current**: Not implemented, no CLI flag.

**Design** (UX Bible):
```
  ╭─ 24h Overview ──────────────────────────────────────────────────╮
  │        0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 │
  │        ┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──┼──  │
  │  data  ◆     ◆     ◆     ◆     ◆     ◆     ◆     ◆     ◆      │
  │  check ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆  ◆     │
  │  report                        ◆                                 │
  │  ⚠ 08:00–09:00: 3 workflows overlap — consider staggering       │
  ╰──────────────────────────────────────────────────────────────────╯
```

**Implementation**:
1. Add `--timeline` flag to ScheduleAction::List
2. For each schedule: compute next 24 occurrences using croner
3. Map occurrences to hour slots (0-23)
4. Render ASCII grid with ◆ marks
5. Detect overlaps (multiple schedules in same hour slot) → warning

**Exact files**:
- `nika-cli/src/schedule.rs` — add `timeline: bool` flag, new render function

**Tests**: 1 test for timeline rendering (given schedule data, verify grid output).

---

### ITEM 5 — Typed enums for overlap/source — ~50 LOC

**What**: Replace `String` with proper Rust enums for `overlap` and `source` fields.

**Current**: Both are `pub overlap: String` / `pub source: String` in CronSchedule and ScheduleConfig.

**Audit finding** (Rust Pro agent): "The single most impactful finding — stringly typed enums."

**Implementation**:
1. Define enums in `nika-core/src/ast/schedule.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum OverlapPolicy { Skip, Queue, Replace }
   
   #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum ScheduleSource { Cli, Yaml }
   ```
2. Update `ScheduleConfig` (nika-core) — `overlap: OverlapPolicy`
3. Update `CronSchedule` (nika-storage) — `overlap: String` stays for SQLite compat but add `impl From<OverlapPolicy>` helpers
4. Update server.rs dispatch — remove string validation (enum does it)
5. Update every.rs — use `OverlapPolicy::Skip` instead of `"skip"`
6. Update schedule.rs display — `policy.to_string()` or Display impl

**Files**: nika-core/ast/schedule.rs, nika-storage/lib.rs, nika-daemon/server.rs, nika-cli/every.rs, nika-cli/schedule.rs

**Tests**: 2 tests (serde roundtrip, default value).

---

### ITEM 6 — 6-field cron rejection — ~15 LOC

**What**: Decide whether to accept or reject 6-field cron (with seconds).

**Audit finding** (Edge Case agent): "croner accepts 6-field cron but our docs say 5-field. Downstream code may break."

**Decision needed**: Reject 6-field cron with a clear error message.

**Implementation**:
1. In `validate_cron()` (nika-core/src/ast/schedule.rs), after croner validation:
   ```rust
   fn validate_cron(expr: &str) -> Result<(), String> {
       if !expr.starts_with('@') {
           let field_count = expr.split_whitespace().count();
           if field_count != 5 {
               return Err(format!(
                   "expected 5-field cron expression, got {field_count} fields. \
                    Nika uses standard cron (min hour day month weekday)."
               ));
           }
       }
       expr.parse::<croner::Cron>()
           .map(|_| ())
           .map_err(|e| format!("invalid cron expression '{}': {}", expr, e))
   }
   ```
2. Add test: `"0 0 9 * * *"` (6 fields) → rejected with clear message
3. Add test: `"0 9 * * *"` (5 fields) → accepted
4. Add test: `"@daily"` (preset) → accepted (no field count check)

**Exact file**: `nika-core/src/ast/schedule.rs`, function `validate_cron()` at ~line 152.

---

## EXECUTION ORDER

```
1. Item 6 — 6-field cron rejection (15 LOC, quick, foundational)
2. Item 5 — Typed enums (50 LOC, quality, touches many files)
3. Item 1 — Interactive wizard (100 LOC, biggest UX win)
4. Item 2 — Cost estimation v1 (80 LOC, runs/day only)
5. Item 3 — History dots (60 LOC, needs protocol addition)
6. Item 4 — Timeline view (120 LOC, luxury feature)
```

Total: ~425 LOC, 8+ tests, 6 commits.

---

## V0 PHILOSOPHY (absolute)

- **Zero dead code** — if unused, nuke it
- **Zero backward compat** — v0.x = rename/restructure freely
- **AGPL-3.0-or-later** — all crates
- **No Keychain popups** — always `cargo test --workspace --lib`
- **1 fix = 1 commit** — `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
- **Don't ask cleanup questions** — just do what's best architecturally

## Verification (before EVERY commit)
```bash
cd tools/
cargo test --workspace --lib --exclude nika-py   # 0 failures
cargo clippy --workspace -- -D warnings          # 0 warnings
cargo fmt --all --check                           # clean
```

## Skills
```
test-driven-development        RED → GREEN → REFACTOR
verification-before-completion cargo test + clippy + fmt BEFORE commit
systematic-debugging           Root cause BEFORE fix
rust                           Idiomatic Rust, proper error types, type safety
```

---

## DESIGN DOCS (reference)

```bash
cat docs/plans/2026-04-05-scheduling-design.md       # 535 lines — architecture
cat docs/plans/2026-04-05-scheduling-ux-bible.md      # 626 lines — every UX detail
cat docs/plans/2026-04-05-scheduling-mega-prompt.md   # 475 lines — file:line locations
```

---

## START

1. Check état actuel (git, tests, clippy, fmt)
2. Read the 3 design docs pour UX requirements
3. Item 6 → commit → Item 5 → commit → ... → Item 4 → commit
4. Push quand tout est vert
5. Total attendu: ~6 commits, ~425 LOC, 8+ new tests
