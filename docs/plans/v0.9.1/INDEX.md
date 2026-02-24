# Nika v0.9.x Plan Index

> **For Claude:** Start with ROADMAP-v09x.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [ROADMAP-v09x.md](./ROADMAP-v09x.md) | Version overview, skill mapping | Starting v0.9.x work |
| [UX-UI-PRESERVE.md](./UX-UI-PRESERVE.md) | Components to keep | Before TUI changes |
| [WIRING-CHECKPOINTS.md](./WIRING-CHECKPOINTS.md) | Integration tests | After each version |
| **[6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md)** | **TUI 6-Views Architecture** | **Before ANY TUI work** |

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
| **v0.9.2** | [v0.9.2-MentionBindings.md](./v0.9.2-MentionBindings.md) | @mention parser | 10 | 35 |
| **v0.9.3** | [v0.9.3-BuiltinTools.md](./v0.9.3-BuiltinTools.md) | 6 nika:* tools | 10 | 45 |
| **v0.9.4** | [v0.9.4-DagPanel.md](./v0.9.4-DagPanel.md) | TUI DAG widget | 8 | 25 |
| **v0.9.5** | [v0.9.5-Polish.md](./v0.9.5-Polish.md) | Animations, export | 6 | 18 |

**Totals:** 46 tasks, 169 tests, 9 sessions (+ v0.8.9: 8 tasks, 56 tests)

---

## TaskBox Widget Family (v0.9.4 Sub-Specs)

The 5 semantic verbs have visual representations as **TaskBox** widgets. These appear in:
- Chat View (inline compact boxes)
- DAG Panel (node visualization)
- Execution Monitor (running workflows)
- History/Lists (historical traces)

| Spec | Verb | Icon | Color | Tasks | Tests |
|------|------|------|-------|-------|-------|
| [v0.9.4a-TaskBoxFoundation](./v0.9.4a-TaskBoxFoundation.md) | — | — | — | 15 | 60 |
| [v0.9.4b-InferBox](./v0.9.4b-InferBox.md) | `infer:` | ⚡ | Violet #8b5cf6 | 10 | 35 |
| [v0.9.4c-ExecBox](./v0.9.4c-ExecBox.md) | `exec:` | 📟 | Amber #f59e0b | 9 | 30 |
| [v0.9.4d-FetchBox](./v0.9.4d-FetchBox.md) | `fetch:` | 🛰️ | Cyan #06b6d4 | 8 | 28 |
| [v0.9.4e-InvokeBox](./v0.9.4e-InvokeBox.md) | `invoke:` | 🔌 | Emerald #10b981 | 8 | 25 |
| [v0.9.4f-AgentBox](./v0.9.4f-AgentBox.md) | `agent:` | 🐔/🐤 | Rose #f43f5e | 8 | 30 |
| **TaskBox Totals** | **5 verbs** | — | — | **58** | **208** |

### TaskBox Implementation Order

```
v0.9.4a Foundation (FIRST - shared effects, traits, types)
    │
    ├──► v0.9.4b InferBox ──┐
    ├──► v0.9.4c ExecBox ───┤ (can be parallel)
    ├──► v0.9.4d FetchBox ──┤
    └──► v0.9.4e InvokeBox ─┘
                            │
                            ▼
                  v0.9.4f AgentBox (LAST - depends on others for children)
```

### Key Components from Foundation

- **BoxState**: Queued → Running → Success/Failed/Skipped lifecycle
- **DecryptEffect**: Progressive text reveal `░▒▓█`
- **ProgressBar/Sparkline**: Visual metrics
- **TaskBoxBehavior**: Common trait for all widgets
- **RenderMode**: Compact (4-10 lines), Expanded (15-60), Full (unlimited)
- **Braille Spinner**: `⣾⣽⣻⢿⡿⣟⣯⣷` for running state

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
    ▼
v0.9.4 (DAG Panel)
    │
    ▼
v0.9.5 (Polish & Export)
    │
━━━ PHASE B: 6-Views Architecture (v0.10→v0.12) ━━━
    ▼
v0.10.0 (Explorer + Project Structure)
    │
    ▼
v0.10.1 (User Profile)
    │
    ▼
v0.10.2 (Long-term Memory)
    │
    ▼
v0.10.3 (Policies)
    │
    ▼
v0.10.4 (Heartbeat)
    │
    ▼
v0.10.5 (Enriched Agents)
    │
━━━ PHASE C: Context System + Settings ━━━━━━━━
    ▼
v0.11.x (Context Discovery + Settings view v0.11.1)
    │
━━━ PHASE D: Multi-Agent + Scheduler ━━━━━━━━━
    ▼
v0.12.x (Full Scheduler view + Multi-Agent)
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

## Files Created in This Planning Session

```
docs/plans/v0.9.1/
├── INDEX.md                        ← You are here
├── ROADMAP-v09x.md                 ← Version overview
├── UX-UI-PRESERVE.md               ← Component preservation
├── WIRING-CHECKPOINTS.md           ← Integration tests
│
│   Pre-Flight (EXECUTE FIRST)
├── v0.8.9-DX-Preparation.md        ← DX infrastructure readiness
│
│   Core Version Plans
├── v0.9.0-StableGraph.md           ← Version 0.9.0 plan
├── v0.9.1-ChatWorkflow.md          ← Version 0.9.1 plan
├── v0.9.2-MentionBindings.md       ← Version 0.9.2 plan
├── v0.9.3-BuiltinTools.md          ← Version 0.9.3 plan
├── v0.9.4-DagPanel.md              ← Version 0.9.4 plan
├── v0.9.5-Polish.md                ← Version 0.9.5 plan
├── 2026-02-24-*.md                 ← Design documents
│
│   TaskBox Widget Specs (v0.9.4 sub-specs)
├── v0.9.4a-TaskBoxFoundation.md    ← Effects, traits, shared types
├── v0.9.4b-InferBox.md             ← LLM generation widget
├── v0.9.4c-ExecBox.md              ← Shell command widget
├── v0.9.4d-FetchBox.md             ← HTTP request widget
├── v0.9.4e-InvokeBox.md            ← MCP tool call widget
└── v0.9.4f-AgentBox.md             ← Multi-turn agent widget
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
| New tests (core v0.9.x) | 169 |
| New tests (TaskBox widgets) | 208 |
| **Total new tests** | **433** |
| Existing tests | 1,902 unchanged |
| WIRING checkpoints | 6 passing |
| clippy warnings | 0 |
| Documentation | All files updated |
| Final verification | /nika-deep-verify passes |
| **Plans with Comprehensive Testing** | **13 / 13** |

---

## Grand Total Summary

| Component | Tasks | Tests | Effort |
|-----------|-------|-------|--------|
| **v0.8.9 (DX Prep)** | 8 | 56 | ~4 hours |
| v0.9.0-v0.9.5 (core) | 46 | 169 | 9 sessions |
| TaskBox widgets (v0.9.4*) | 58 | 208 | ~14 hours |
| **Total** | **112** | **433** | — |

**Enhanced:**
- Foundation spec: StreamChunk wiring, 5 performance patterns, 5 test patterns, memory bounds
- ALL 13 plans: Comprehensive Testing Protocol + Phase Completion Checkpoint
- NEW: v0.8.9 DX Preparation as mandatory pre-flight step
