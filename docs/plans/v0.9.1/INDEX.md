# Nika v0.9.x Plan Index

> **For Claude:** Start with ROADMAP.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [ROADMAP.md](./ROADMAP.md) | Version overview, skill mapping | Starting v0.9.x work |
| [UX-UI-PRESERVE.md](./UX-UI-PRESERVE.md) | Components to keep | Before TUI changes |
| [WIRING-CHECKPOINTS.md](./WIRING-CHECKPOINTS.md) | Integration tests | After each version |
| **[6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md)** | **TUI 6-Views Architecture** | **Before ANY TUI work** |
| [v0.10/](../v0.10/) | TaskBox widget plans | After v0.9.3 |
| [v0.11/](../v0.11/) | Six Views architecture | After v0.10 |
| [v0.12/](../v0.12/) | Providers wiring | Parallel with v0.10 |

---

## TUI Views Evolution

**v0.8.x → v0.10+:** Transition from 4 views to 6 views.

| # | View | Key | Version | Predecessor |
|---|------|-----|---------|-------------|
| 1 | **Explorer** | `1` / `e` | v0.10.0 | Home |
| 2 | **Chat** | `2` / `c` | v0.9.x | Chat (enhanced) |
| 3 | **Editor** | `3` / `d` | v0.10.0 | Studio |
| 4 | **Runner** | `4` / `r` | v0.10.0 | Monitor |
| 5 | **Scheduler** | `5` / `s` | v0.12.0 | NEW |
| 6 | **Settings** | `6` / `,` | v0.11.1 | NEW (standby) |

> **Note v0.10.0:** Settings view is in **standby** — opens existing Provider Modal until v0.11.1.

---

## Pre-Flight: DX Preparation

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.8.9** | [v0.8.9-DX-Preparation.md](./v0.8.9-DX-Preparation.md) | DX infrastructure readiness | 8 | 56 |

> **EXECUTE FIRST:** Complete v0.8.9 DX Preparation before starting v0.9.0

---

## Version Plans

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.9.0** | [v0.9.0-StableGraph.md](./v0.9.0-StableGraph.md) | StableGraph migration | 6 | 25 |
| **v0.9.1** | [v0.9.1-ChatWorkflow.md](./v0.9.1-ChatWorkflow.md) | ChatWorkflow struct | 6 | 21 |
| **v0.9.2** | [v0.9.2-MentionBindings.md](./v0.9.2-MentionBindings.md) | @mention parser | 10 | 40 |
| **v0.9.3** | [v0.9.3-BuiltinTools.md](./v0.9.3-BuiltinTools.md) | 6 nika:* tools | 10 | 45 |

**Totals:** 32 tasks, 131 tests, 6 sessions (+ v0.8.9: 8 tasks, 56 tests)

> **Note:** v0.9.4+ (TaskBox, DAG Panel, Polish) moved to [v0.10/](../v0.10/) as "TaskBox" release.

---

## TaskBox Widgets (Moved to v0.10)

TaskBox widget specs have been moved to [v0.10/archive/](../v0.10/archive/).

See [v0.10/INDEX.md](../v0.10/INDEX.md) for the complete TaskBox implementation plan.

---

## Implementation Order

```
━━━ PHASE 0: DX Preparation ━━━━━━━━━━━━━
v0.8.9 (DX Infrastructure)
    │
    ▼ ← MUST COMPLETE BEFORE v0.9.0
━━━ PHASE A: Chat-as-DAG Core ━━━━━━━━━━━━
v0.9.0 (StableGraph)
    │
    ▼
v0.9.1 (ChatWorkflow)
    │
    ▼
v0.9.2 (@mention Bindings)
    │
    ▼
v0.9.3 (Builtin Tools)
    │
━━━ PHASE B: TaskBox (v0.10.x) ━━━━━━━━━━━━
    ▼
v0.10.x (TaskBox widgets, DAG Panel, Polish)
    │    See docs/plans/v0.10/ for breakdown
    │
━━━ PHASE C: Six Views (v0.11.x) ━━━━━━━━
    ▼
v0.11.x (Explorer, Editor, Runner, Scheduler, Settings)
    │    See docs/plans/v0.11/ for breakdown
    │
━━━ PHASE D: Providers (v0.12.x) — PARALLEL with v0.10 ━━━
    ▼
v0.12.x (Keyring wiring, Ollama handlers)
    │    See docs/plans/v0.12/ for breakdown
```

**NO v1.0 — We stay in 0.XX versioning.**

---

## Design Documents (Background)

| Document | Purpose |
|----------|---------|
| [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md) | Original design |
| [2026-02-24-builtin-tools-spec.md](./2026-02-24-builtin-tools-spec.md) | 6 nika:* tools spec |
| [2026-02-24-thread-safety-architecture.md](./2026-02-24-thread-safety-architecture.md) | Concurrency patterns |
| [2026-02-24-stablegraph-migration-spec.md](./2026-02-24-stablegraph-migration-spec.md) | StableGraph rationale |

---

## Skill References

| Skill | When to Use |
|-------|-------------|
| `@rust-core` | All Rust implementation |
| `@rust-async` | Tokio, async patterns |
| `@test-driven-development` | Every task |
| `@frontend-design` | TUI widgets |
| `@verification-before-completion` | Before marking complete |
| `@superpowers:executing-plans` | Executing any plan |
| `@superpowers:subagent-driven-development` | Per-task subagents |

---

## Git Workflow

```bash
# Start each version on a branch
git checkout -b feature/v0.9.0-stablegraph

# Work through tasks with TDD
# Run WIRING checkpoint
cargo test wiring_checkpoint_0

# Merge when all tests pass
git checkout main
git merge feature/v0.9.0-stablegraph
git tag v0.9.0

# Start next version
git checkout -b feature/v0.9.1-chatworkflow
```

---

## Quick Commands

```bash
# Validate all DAG tests
cargo test dag:: --lib

# Run WIRING checkpoints
cargo test wiring_checkpoint --test 'wiring_checkpoint_*'

# Full verification
/nika-deep-verify

# Live test chat
cargo run -- chat
```

---

## Files in v0.9.1/

```
docs/plans/v0.9.1/
├── INDEX.md                        ← You are here
├── ROADMAP.md                      ← Version overview (consolidated)
├── UX-UI-PRESERVE.md               ← Component preservation
├── WIRING-CHECKPOINTS.md           ← Integration tests
│
│   Pre-Flight (EXECUTE FIRST)
├── v0.8.9-DX-Preparation.md        ← DX infrastructure readiness
│
│   Core Version Plans (v0.9.0-v0.9.3)
├── v0.9.0-StableGraph.md           ← Version 0.9.0 plan
├── v0.9.1-ChatWorkflow.md          ← Version 0.9.1 plan
├── v0.9.2-MentionBindings.md       ← Version 0.9.2 plan
├── v0.9.3-BuiltinTools.md          ← Version 0.9.3 plan
├── 2026-02-24-*.md                 ← Design documents
│
│   Archived (superseded)
└── archive-v0.8/                   ← Historical documents
    └── ROADMAP-v09x-superseded.md  ← Old roadmap (pre-restructure)

TaskBox specs (v0.9.4+) moved to:  ../v0.10/archive/
```

---

## Comprehensive Testing Protocol (ALL PLANS)

Every plan now includes standardized testing sections:

### 1. Unit Tests Table

```markdown
| Category | Test Names | Count |
|----------|-----------|-------|
| **Feature A** | `test_a_new`, `test_a_update` | 2 |
```

### 2. Property-Based Tests (proptest)

```rust
proptest! {
    #[test]
    fn feature_never_panics(input in "\\PC{0,1000}") {
        // Invariant checks
    }
}
```

### 3. Live Test Scenarios (4 per plan)

```bash
cargo run --example test_feature_scenario
# Expected: Visual/behavioral outcome
```

### 4. Benchmark Tests (criterion)

```rust
c.bench_function("operation_name", |b| {
    b.iter(|| black_box(operation()));
});
```

### 5. Performance Targets Table

```markdown
| Operation | Target | Measured |
|-----------|--------|----------|
| render | <5ms | ~2ms |
```

---

## Phase Completion Checkpoint (ALL PLANS)

Every plan ends with a 6-step completion workflow:

```
Step 1: Full Test Suite      → cargo test <module> --lib
Step 2: Commit Phase         → git add && git commit
Step 3: Push & Verify CI     → git push && gh run watch
Step 4: Tag Release          → git tag -a v0.9.Xx
Step 5: Verify Tag           → git tag -l "v0.9.*"
Step 6: Update CHANGELOG     → Add release notes
```

---

## Success Metrics

| Metric | Target |
|--------|--------|
| New tests (DX prep v0.8.9) | 56 |
| New tests (core v0.9.x) | 131 |
| **Total new tests (v0.9.x scope)** | **187** |
| Existing tests | 1,902 unchanged |
| WIRING checkpoints | 4 passing |
| clippy warnings | 0 |
| Documentation | All files updated |
| Final verification | /nika-deep-verify passes |

---

## Grand Total Summary

| Component | Tasks | Tests | Location |
|-----------|-------|-------|----------|
| **v0.8.9 (DX Prep)** | 8 | 56 | Here |
| v0.9.0-v0.9.3 (core) | 32 | 131 | Here |
| **v0.9.x Subtotal** | **40** | **187** | — |
| v0.10.x (TaskBox) | 22 | 75 | [v0.10/](../v0.10/) |
| v0.11.x (Six Views) | 30 | 90 | [v0.11/](../v0.11/) |
| v0.12.x (Providers) | 15 | 45 | [v0.12/](../v0.12/) |
| **Grand Total** | **107** | **397** | — |

> **Note:** v0.12.x can run PARALLEL with v0.10.x (no file conflicts).
