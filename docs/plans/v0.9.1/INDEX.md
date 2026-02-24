# Nika v0.9.x Plan Index

> **For Claude:** Start with ROADMAP-v09x.md for overview, then individual version files for implementation.

---

## Quick Navigation

| Document | Purpose | Read When |
|----------|---------|-----------|
| [ROADMAP-v09x.md](./ROADMAP-v09x.md) | Version overview, skill mapping | Starting v0.9.x work |
| [UX-UI-PRESERVE.md](./UX-UI-PRESERVE.md) | Components to keep | Before TUI changes |
| [WIRING-CHECKPOINTS.md](./WIRING-CHECKPOINTS.md) | Integration tests | After each version |

---

## Version Plans

| Version | File | Focus | Tasks | Tests |
|---------|------|-------|-------|-------|
| **v0.9.0** | [v0.9.0-StableGraph.md](./v0.9.0-StableGraph.md) | StableGraph migration | 6 | 25 |
| **v0.9.1** | [v0.9.1-ChatWorkflow.md](./v0.9.1-ChatWorkflow.md) | ChatWorkflow struct | 6 | 20 |
| **v0.9.2** | [v0.9.2-MentionBindings.md](./v0.9.2-MentionBindings.md) | @mention parser | 10 | 35 |
| **v0.9.3** | [v0.9.3-BuiltinTools.md](./v0.9.3-BuiltinTools.md) | 6 nika:* tools | 10 | 45 |
| **v0.9.4** | [v0.9.4-DagPanel.md](./v0.9.4-DagPanel.md) | TUI DAG widget | 8 | 25 |
| **v0.9.5** | [v0.9.5-Polish.md](./v0.9.5-Polish.md) | Animations, export | 6 | 18 |

**Totals:** 46 tasks, 168 tests, 9 sessions

---

## Implementation Order

```
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
    ▼
v1.0.0 (Chat-as-DAG Complete)
```

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
├── v0.9.0-StableGraph.md           ← Version 0.9.0 plan
├── v0.9.1-ChatWorkflow.md          ← Version 0.9.1 plan
├── v0.9.2-MentionBindings.md       ← Version 0.9.2 plan
├── v0.9.3-BuiltinTools.md          ← Version 0.9.3 plan
├── v0.9.4-DagPanel.md              ← Version 0.9.4 plan
├── v0.9.5-Polish.md                ← Version 0.9.5 plan
└── 2026-02-24-*.md                 ← Design documents
```

---

## Success Metrics

| Metric | Target |
|--------|--------|
| New tests | 168 |
| Existing tests | 1,902 unchanged |
| WIRING checkpoints | 6 passing |
| clippy warnings | 0 |
| Documentation | All files updated |
| Final verification | /nika-deep-verify passes |
