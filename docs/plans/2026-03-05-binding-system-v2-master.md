# Binding System v2 — Master Orchestrator Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enhance Nika's binding system with implicit output references, agent namespaces, and persistent datastores across 3 minor releases.

**Architecture:** Incremental enhancement of `binding/` and `store/` modules, maintaining backward compatibility while adding syntactic sugar and new capabilities.

**Tech Stack:** Rust (rustc 1.86+), serde, dashmap, parking_lot, tokio

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  BINDING SYSTEM V2 — 3 FEATURES ACROSS 3 RELEASES                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.21.0 — Feature A: Implicit Output Reference                               ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Reference task output without `.output` suffix                             ║
║  • `$task` → `task.output` (syntactic sugar)                                  ║
║  • LSP/analyzer validation updates                                            ║
║  • Documentation and examples                                                 ║
║  Impact: binding/entry.rs, binding/resolve.rs, docs                           ║
║                                                                               ║
║  v0.22.0 — Feature C: Agent Output Namespaces                                 ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Agents emit multiple named outputs (not just single result)                ║
║  • `agent_task.artifacts`, `agent_task.summary`, etc.                         ║
║  • Runtime `emit_output()` from agent tools                                   ║
║  • TaskResult → NamespacedResult                                              ║
║  Impact: store/datastore.rs, runtime/rig_agent_loop.rs, AST                   ║
║                                                                               ║
║  v0.23.0 — Feature B: Persistent Datastore                                    ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Store results between workflow executions                                  ║
║  • File-backed storage with atomic writes                                     ║
║  • Workflow-scoped persistence via `persist:` field                           ║
║  • Import/export capabilities                                                 ║
║  Impact: store/persistent.rs (new), runtime/runner.rs, schema                 ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Dependency Graph

```
                    ┌─────────────────────────┐
                    │  v0.21.0 Feature A      │
                    │  Implicit Output        │
                    │  (1-2 days)             │
                    └───────────┬─────────────┘
                                │
                    ┌───────────▼─────────────┐
                    │  v0.22.0 Feature C      │
                    │  Agent Namespaces       │
                    │  (3-5 days)             │
                    └───────────┬─────────────┘
                                │
                    ┌───────────▼─────────────┐
                    │  v0.23.0 Feature B      │
                    │  Persistent Datastore   │
                    │  (5-7 days)             │
                    └─────────────────────────┘
```

**Rationale for order:**
1. **Feature A first** — Minimal impact, establishes patterns
2. **Feature C second** — Required for namespaced persistence
3. **Feature B last** — Builds on namespaced results

---

## Checkpoints & Verification

### Checkpoint 1: After Feature A (v0.21.0)

```bash
# Verification commands
cargo test binding::              # All binding tests pass
cargo test implicit_output        # New tests pass
cargo clippy -- -D warnings       # Zero warnings
cargo run -- check examples/v21-implicit-output.nika.yaml  # Example validates

# Manual verification
# 1. Write workflow using $task syntax
# 2. Verify LSP shows no errors
# 3. Run workflow successfully
```

**Exit criteria:**
- [ ] `$task` syntax works in `use:` blocks
- [ ] Backward compatible with `task.output` syntax
- [ ] 10+ new tests passing
- [ ] Example workflow validates and runs

### Checkpoint 2: After Feature C (v0.22.0)

```bash
# Verification commands
cargo test namespace              # Namespace tests pass
cargo test agent.*namespace       # Agent namespace tests pass
cargo test store::                # Store tests pass
cargo run -- check examples/v22-agent-namespaces.nika.yaml

# Manual verification
# 1. Agent emits multiple outputs
# 2. Access via task.namespace.field syntax
# 3. Event log shows NamespacedOutput events
```

**Exit criteria:**
- [ ] Agents can emit named outputs
- [ ] `task.artifacts[0]` syntax works
- [ ] `task.summary` syntax works
- [ ] 20+ new tests passing
- [ ] Example workflow validates and runs

### Checkpoint 3: After Feature B (v0.23.0)

```bash
# Verification commands
cargo test persistent             # Persistence tests pass
cargo test store::                # All store tests pass
cargo run -- check examples/v23-persistent-store.nika.yaml

# Manual verification
# 1. Run workflow with persist: true
# 2. Stop and restart workflow
# 3. Previous results are available
# 4. Export/import works
```

**Exit criteria:**
- [ ] `persist: true` saves results to disk
- [ ] Workflow restart loads previous results
- [ ] Atomic writes prevent corruption
- [ ] 25+ new tests passing
- [ ] Example workflow demonstrates persistence

---

## File Changes Summary

### Feature A: Implicit Output (v0.21.0)

| File | Change Type | Description |
|------|-------------|-------------|
| `binding/entry.rs` | Modify | Add `normalize_path()` for `$task` → `task` |
| `binding/resolve.rs` | Modify | Update `split_path()` docs, add tests |
| `ast/workflow.rs` | Modify | Add `$` prefix validation |
| `examples/v21-implicit-output.nika.yaml` | Create | Example workflow |
| `tests/implicit_output_test.rs` | Create | 10+ unit tests |

### Feature C: Agent Namespaces (v0.22.0)

| File | Change Type | Description |
|------|-------------|-------------|
| `store/datastore.rs` | Modify | Add `NamespacedResult`, `emit_namespace()` |
| `runtime/rig_agent_loop.rs` | Modify | Add `emit_output()` tool |
| `runtime/executor.rs` | Modify | Handle namespaced results |
| `event/log.rs` | Modify | Add `NamespacedOutput` event |
| `ast/output.rs` | Modify | Add namespace path parsing |
| `examples/v22-agent-namespaces.nika.yaml` | Create | Example workflow |
| `tests/namespace_test.rs` | Create | 20+ unit tests |

### Feature B: Persistent Datastore (v0.23.0)

| File | Change Type | Description |
|------|-------------|-------------|
| `store/persistent.rs` | Create | `PersistentStore` implementation |
| `store/mod.rs` | Modify | Export `PersistentStore` |
| `runtime/runner.rs` | Modify | Integrate persistence |
| `ast/workflow.rs` | Modify | Add `persist:` field |
| `schemas/nika-workflow.schema.json` | Modify | Add persistence schema |
| `examples/v23-persistent-store.nika.yaml` | Create | Example workflow |
| `tests/persistent_store_test.rs` | Create | 25+ unit tests |

---

## Commit Strategy

### Feature A Commits (5 commits)

```
feat(binding): add normalize_path for implicit output syntax
test(binding): add implicit output resolution tests
feat(ast): validate $ prefix in use block paths
docs(examples): add v21-implicit-output example
chore(release): bump version to 0.21.0
```

### Feature C Commits (8 commits)

```
feat(store): add NamespacedResult type
feat(store): implement namespace storage in DataStore
feat(runtime): add emit_output tool to RigAgentLoop
feat(event): add NamespacedOutput event variant
feat(binding): support namespace path resolution
test(namespace): add comprehensive namespace tests
docs(examples): add v22-agent-namespaces example
chore(release): bump version to 0.22.0
```

### Feature B Commits (10 commits)

```
feat(store): create PersistentStore module
feat(store): implement atomic file writes
feat(store): add load/save methods
feat(store): implement auto-cleanup
feat(runner): integrate PersistentStore
feat(ast): add persist field to workflow
feat(schema): update JSON schema for persist
test(persistent): add persistence tests
docs(examples): add v23-persistent-store example
chore(release): bump version to 0.23.0
```

---

## Skills Usage

| Phase | Skill | Purpose |
|-------|-------|---------|
| All | `superpowers:test-driven-development` | Write tests first |
| All | `superpowers:systematic-debugging` | Debug failing tests |
| All | `superpowers:verification-before-completion` | Verify before commit |
| Code Review | `superpowers:requesting-code-review` | Review after each feature |
| Planning | `superpowers:writing-plans` | Create detailed plans |
| Execution | `superpowers:executing-plans` | Execute task-by-task |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking backward compatibility | Keep old syntax working, add new as sugar |
| Performance regression | Benchmark DataStore operations before/after |
| Data loss in persistence | Atomic writes with temp file + rename |
| Complex merge conflicts | Small, focused commits with clear scope |
| LSP integration issues | Test in VS Code before release |

---

## Sub-Plans

Each feature has a detailed implementation plan:

1. **[Plan A: Implicit Output Reference](./2026-03-05-binding-v21-implicit-output.md)** — v0.21.0
2. **[Plan C: Agent Output Namespaces](./2026-03-05-binding-v22-namespaces.md)** — v0.22.0
3. **[Plan B: Persistent Datastore](./2026-03-05-binding-v23-persistent-store.md)** — v0.23.0

Execute plans in order with checkpoints between each.

---

## Success Criteria

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  BINDING SYSTEM V2 — COMPLETE WHEN:                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ✅ v0.21.0 released with implicit output syntax                              ║
║  ✅ v0.22.0 released with agent namespaces                                    ║
║  ✅ v0.23.0 released with persistent datastore                                ║
║  ✅ All 3 example workflows validate and run                                  ║
║  ✅ 55+ new tests across all features                                         ║
║  ✅ Zero clippy warnings                                                      ║
║  ✅ CHANGELOG updated for each release                                        ║
║  ✅ CLAUDE.md updated with new features                                       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```
