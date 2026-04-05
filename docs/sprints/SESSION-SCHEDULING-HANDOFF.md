# SESSION PROMPT — Cleanup + Scheduling Implementation

> **Copy-paste this into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.

---

## WHO YOU ARE

Rust engineer senior sur **Nika** — workflow engine YAML pour l'IA. Communication franglais, code/commits EN. Tu travailles avec Thibaut (créateur, Paris).

---

## ÉTAPE 1 — Analyser l'état actuel (AVANT tout)

Lance plusieurs agents en parallèle :
```bash
git log --oneline -20 && git log --oneline origin/main..HEAD   # Commits + unpushed
git status && git diff --stat                                    # Dirty state
cd tools && cargo test --workspace --lib --exclude nika-py 2>&1 | tail -5  # Tests
cd tools && cargo fmt --all --check 2>&1 | grep "^Diff in" | sort -u      # Format
cd tools && cargo clippy --workspace -- -D warnings 2>&1 | tail -3         # Clippy
```

**Baseline connu** (7 agents ont vérifié) : 10,151 tests, 0 fail, clippy clean, 8 files need fmt, 9 commits unpushed.

---

## PART A — CLEANUP (9 fixes, ~30 min)

### CRITICAL (2)

1. **doctor.rs:189** — `"nika provider vault-reset"` référence une commande supprimée.
   ```bash
   grep -n "vault-reset\|provider vault" tools/nika-cli/src/doctor.rs
   ```
   Fix: supprimer ou remplacer par guidance vers `nika keys`.

2. **infer-verb-guide.md:208** — `enable_extractor: true` documenté comme feature active, mais supprimé. Layer numbering saute Layer 2.
   ```bash
   grep -n "enable_extractor\|Layer 1\|Layer 2\|Layer 3\|Layer 4" docs/content-suite/03-user-guide/05-infer-verb-guide.md
   ```
   Fix: supprimer section enable_extractor, renumber layers (0→1→2→3).

### HIGH (2)

3. **exercises_advanced.rs:1026,1055** — `json_query` (supprimé) encore référencé.
   ```bash
   grep -n "json_query" tools/nika-init/src/course/exercises_advanced.rs
   ```
   Fix: remplacer par `jq`.

4. **showcase_llm.rs** — `provider: "{{PROVIDER}}"` dans 10+ workflows. Vérifier si c'est encore valide (probablement OK car c'est un template variable, pas une commande).

### LOW (3)

5. **levels.rs:29** — `"provider setup"` → `"keys setup"` ou `"API key management"`.
6. `cargo fmt --all` → commit style.
7. Push les commits unpushed + tes fixes.

### Ordre d'exécution Part A
```
1. cargo fmt --all → commit "style: cargo fmt"
2. Fix doctor.rs → commit "fix(cli): remove vault-reset reference"
3. Fix infer-verb-guide.md → commit "docs: remove enable_extractor, fix layers"
4. Fix exercises json_query → commit "fix(course): replace json_query with jq"
5. Fix levels.rs → commit "fix(course): update terminology keys"
6. Verify: cargo test + clippy + fmt → push all
```

---

## PART B — SCHEDULING IMPLEMENTATION (~850 LOC, 6 phases)

### Design docs à lire AVANT de coder
```bash
cat docs/plans/2026-04-05-scheduling-design.md        # 535 lines — architecture + YAML + CLI
cat docs/plans/2026-04-05-scheduling-ux-bible.md       # 626 lines — wizard, animations, errors
cat docs/plans/2026-04-05-scheduling-mega-prompt.md    # 475 lines — file:line, structs, SQL
cat docs/plans/2026-04-05-scheduling-cron-blueprint.md # 1458 lines — original blueprint
```

### Design Decisions (ALL LOCKED)

**Dual Naming**: `nika every` (CLI create) + `nika schedule` (lifecycle) + `schedule:` (YAML field).

**YAML** (string-or-object, comme `infer:`):
```yaml
schedule: "every day at 9:00"          # hron human-readable (24h format ONLY)
schedule: "@daily"                      # preset
schedule: "0 9 * * *"                   # raw cron
schedule:                               # full form
  cron: "0 9 * * 1-5"
  timezone: "Europe/Paris"
  catchup: false
  overlap: skip                         # skip | queue | replace
  jitter: 30s
  paused: false
```

**Crates** (testés et validés le 2026-04-05) :
| Crate | Version | Status |
|-------|---------|--------|
| hron | 1.0 | Human-readable cron. **Attention**: 24h format only (pas de am/pm), day-level minimum (pas "every 5m"). |
| chrono-tz | 0.10 | Timezone IANA |
| croner | 3.0.1 (keep) | **API**: `.parse::<Cron>()` via FromStr (PAS `Cron::new()`). @presets OK. |
| cliclack | (already) | Wizard interactif |

### IMPORTANT — hron grammar limits (audit 2026-04-05)
```
✓ "every day at 9:00"                → 0 9 * * *
✓ "every weekday at 14:30"           → 30 14 * * 1-5
✓ "every monday at 9:00"             → 0 9 * * 1
✓ "every weekday at 9:00 in Europe/Paris" → 0 9 * * 1-5 + tz
✗ "every day at 9am"                 → REJECTED (use 9:00)
✗ "every hour"                       → REJECTED (use @hourly or 0 * * * *)
✗ "every 5 minutes"                  → REJECTED (use */5 * * * *)
✗ "every 1st of month"               → REJECTED (use 0 0 1 * *)
```

**Parsing priority mis à jour** :
1. Try @preset (@daily, @hourly, @weekly, @monthly, @yearly) → croner handles these
2. Try hron (`"every day at 9:00"`, `"every weekday at 14:30"`) → convert to cron
3. Try raw cron (5-field) → validate with croner
4. Try duration shorthand (`"6h"`, `"30m"`, `"1d"`) → convert to cron interval
5. Fail → NIKA-280

### UPDATED Insertion Points (vérifié 2026-04-05, post-15 commits)

**Phase 1: Storage**
| What | File | Line |
|------|------|------|
| SCHEMA_VERSION 4→5 | `nika-storage/src/lib.rs` | **21** (exact) |
| V5 migration block | `nika-storage/src/lib.rs` | after V4 block (~**728**) |
| CronSchedule struct | `nika-storage/src/schedule.rs` | CREATE |

**Phase 2: AST**
| What | File | Line |
|------|------|------|
| RawWorkflow.schedule | `nika-core/src/ast/raw/workflow.rs` | after `routing:` (**79**, exact) |
| known_workflow_keys | `nika-core/src/ast/raw/parser.rs` | add `"schedule"` after `"routing"` (**1467**) |
| Parse schedule | `nika-core/src/ast/raw/parser.rs` | in parse_workflow() |
| AnalyzedWorkflow.schedule | `nika-core/src/ast/analyzed/workflow.rs` | after `routing` (**80**) |
| Validate | `nika-core/src/ast/analyzer/analyze.rs` | workflow-level validation |

**Phase 3: Protocol + Daemon**
| What | File | Line |
|------|------|------|
| 6 DaemonRequest | `nika-daemon/src/protocol.rs` | after JobRetry (~**86**) |
| 3 DaemonResponse | `nika-daemon/src/protocol.rs` | after JobHistoryList (~**257**) |
| fire_due_cron_jobs refactor | `nika-daemon/src/services/jobs.rs` | **486-554** (exact) |

**Phase 4: CLI** (follow keys.rs wiring pattern)
| What | File | Action |
|------|------|--------|
| every.rs | `nika-cli/src/every.rs` | CREATE — handler + wizard |
| schedule.rs | `nika-cli/src/schedule.rs` | CREATE — lifecycle |
| Module export | `nika-cli/src/lib.rs` | add `pub mod every; pub mod schedule;` after line 47 |
| Commands enum | `nika/src/main.rs` | add Every + Schedule variants (near Keys at ~1642) |
| Dep | `nika-cli/Cargo.toml` | add `croner = { workspace = true }` + `hron = "1"` + `chrono-tz = { workspace = true }` |

**Phase 5: Display**
| What | File |
|------|------|
| Schedule card | inline in schedule.rs or new display module |
| Dashboard | inline in schedule.rs |

**Phase 6: Serve**
| What | File | Line |
|------|------|------|
| Scanner | `nika-serve/src/lib.rs` | after workflow counting (~**356**) |

### UX Requirements (from UX Bible — NOT optional)

- **Wizard** (`nika every` bare): cliclack steps, cost preview, next 5 runs, "Run now?"
- **Cascading celebration**: ✓ validate → ✓ register → ✓ next run → "is live! 🦋"
- **Dashboard** (`nika schedule list`): grouped HOURLY/DAILY/WEEKLY, ●⏸✗ icons, ✓✓✓✗ dots, progress bar
- **Did-you-mean**: misspelled commands, names, cron fields
- **Cost warnings**: > $10/month → suggest cheaper
- **Overlap warnings**: stagger suggestion
- **Auto-pause**: 5 consecutive failures
- **Timeline**: `nika schedule list --timeline` 24h view
- **Micro-copy**: "See you tomorrow! 🦋", "Welcome to automation! 🦋"
- **`nika help cron`**: inline cheat sheet

### Error Codes
| Code | Meaning |
|------|---------|
| NIKA-280 | Invalid schedule expression |
| NIKA-282 | Invalid timezone |
| NIKA-283 | Schedule not found |
| NIKA-284 | Schedule name conflict |

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
```

## Worktree (si conflit avec autre session)
```bash
git worktree add /tmp/nika-sched HEAD -b feat/scheduling
cd /tmp/nika-sched/tools && # work here
```

---

## START

1. Lis les 4 design docs (paths ci-dessus)
2. Part A: cleanup 9 fixes → commit each → push
3. Part B: scheduling 6 phases TDD → commit each → push
4. Total attendu: ~15 commits, ~880 LOC, 25+ new tests
