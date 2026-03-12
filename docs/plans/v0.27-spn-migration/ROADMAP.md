# Nika v0.27 Roadmap — spn→nika Feature Fusion

> **For Claude:** Each phase is a standalone milestone. Use TDD, subagent-driven-development, and Context7 for tokio patterns.

**Goal:** Consolidate all spn daemon features into nika, making nika the unified CLI.

**Date:** 2026-03-11
**Author:** Brainstorming session (Claude + Thibaut)

---

## Version Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.27 "spn→nika FUSION" RELEASE TRAIN                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ━━━ Phase 1 "Core Types" (~500 lines) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  Zero-dependency types that form the foundation                               ║
║                                                                               ║
║  1.1  🏗️  AutonomyLevel, RiskLevel, ApprovalLevel, Decision                   ║
║  1.2  🤖 AgentRole, AgentState, AgentStatus, AgentId, DelegatedTask          ║
║  1.3  🧠 MemoryNamespace, MemoryKey                                           ║
║  1.4  📊 TraceStepKind, TraceId, TraceMetadata                                ║
║                                                                               ║
║  ━━━ Phase 2 "Daemon Features" (~3,500 lines) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  The heavy lifting - concurrent data structures with tokio                    ║
║                                                                               ║
║  2.1  💾 MemoryStore       │ FxHashMap + JSON persistence                     ║
║  2.2  📝 TraceStore        │ NDJSON persistence for reasoning traces          ║
║  2.3  ⏰ JobScheduler      │ JoinSet + Semaphore for background jobs          ║
║  2.4  🤖 AgentManager      │ Depth limiting + concurrency control             ║
║  2.5  🎛️  AutonomyOrchestrator │ HITL approval workflows                      ║
║  2.6  💡 SuggestionAnalyzer │ Context triggers for proactive suggestions      ║
║                                                                               ║
║  ━━━ Phase 3 "Package Manager" ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  ✅ ALREADY DONE in v0.27.0 development cycle                                ║
║                                                                               ║
║  3.1  ✅ KNOWN_PROVIDERS (18)  │ src/core/providers.rs                        ║
║  3.2  ✅ KNOWN_MODELS (16+)    │ src/core/models.rs                           ║
║  3.3  ✅ MCP_ALIASES (48)      │ src/core/mcp_aliases.rs                      ║
║  3.4  ✅ McpConfig             │ src/core/mcp_config.rs                       ║
║  3.5  ✅ KeychainResolver      │ src/secrets/                                 ║
║                                                                               ║
║  ━━━ Phase 4 "Config Scope" (~400 lines) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  Three-level config hierarchy: Local → Team → Global                         ║
║                                                                               ║
║  4.1  📁 ConfigScope enum     │ Local, Team, Global                           ║
║  4.2  🔧 ConfigLoader         │ Merge with scope tracking                     ║
║  4.3  ⌨️  nika config command  │ show, where, set, files                       ║
║                                                                               ║
║  ━━━ Phase 5 "Interop & Retirement" (~200 lines) ━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  Deprecate spn and finalize nika as unified tool                             ║
║                                                                               ║
║  5.1  ⚠️  Deprecation warnings │ spn shows warning, suggests nika             ║
║  5.2  📖 Migration guide      │ docs/guides/spn-to-nika.md                    ║
║  5.3  🔌 Socket compatibility  │ ~/.spn/ → ~/.nika/ transition                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Dependency Graph

```
     v0.26.0 (current)
        │
        ▼
     Phase 1 "Core Types" (no dependencies, can start immediately)
        │
        ▼
     Phase 2 "Daemon Features" (depends on Phase 1 types)
        │
        ├───────────────────────┐
        ▼                       ▼
     Phase 4              Phase 5
   "Config Scope"     "Interop & Retirement"
        │                       │
        └──────────┬────────────┘
                   ▼
              v0.27.0 RELEASE
```

### Dependency Table

| Phase | Depends On | Blocking Feature | Can Parallelize? |
|-------|------------|------------------|------------------|
| Phase 1 | v0.26.0 | None - pure types | ❌ Sequential (foundation) |
| Phase 2 | Phase 1 | Core types needed | ❌ After Phase 1 |
| Phase 3 | - | ✅ Already done | ✅ N/A |
| Phase 4 | Phase 2 (soft) | ConfigScope useful for daemon | ✅ Parallel late Phase 2 |
| Phase 5 | Phase 2, 4 | All features working | ❌ After Phase 2+4 |

---

## Phase 1: Core Types (~500 lines, ~5 hours)

**Focus:** Zero-dependency types for daemon features
**Plan:** [01-PHASE-1-CORE-TYPES.md](./01-PHASE-1-CORE-TYPES.md)

### Tasks

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 1.1 | Create `src/core/autonomy.rs` | 4 | 1h |
| 1.2 | Create `src/core/agent_types.rs` | 3 | 1h |
| 1.3 | Create `src/core/memory_types.rs` | 1 | 0.5h |
| 1.4 | Create `src/core/trace_types.rs` | 2 | 0.5h |
| 1.5 | Update `src/core/mod.rs` exports | 0 | 0.25h |
| 1.6 | Serde roundtrip tests | 2 | 0.5h |
| 1.7 | Documentation | 0 | 0.5h |

**Total:** 7 tasks | 12 tests | ~5 hours

### WIRING Checkpoint

```
☐ cargo test core::autonomy
☐ cargo test core::agent_types
☐ cargo test core::memory_types
☐ cargo test core::trace_types
☐ cargo clippy -- -D warnings
☐ All types serialize with snake_case
```

---

## Phase 2: Daemon Features (~3,500 lines, ~45 hours)

**Focus:** Concurrent data structures with tokio
**Plan:** [02-PHASE-2-DAEMON-FEATURES.md](./02-PHASE-2-DAEMON-FEATURES.md)

### 2.1 MemoryStore (~440 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.1.1 | Create `src/store/memory.rs` | 0 | 0.5h |
| 2.1.2 | Implement `MemoryStore::new()`, `load()` | 2 | 1h |
| 2.1.3 | Implement `get()`, `set()`, `delete()` | 3 | 1h |
| 2.1.4 | Implement TTL expiration | 2 | 1h |
| 2.1.5 | Implement namespace operations | 2 | 0.5h |
| 2.1.6 | Persistence (JSON) | 2 | 1h |

**Subtotal:** 6 tasks | 11 tests | ~5 hours

### 2.2 TraceStore (~479 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.2.1 | Create `src/store/trace_store.rs` | 0 | 0.5h |
| 2.2.2 | Implement `start_trace()`, `end_trace()` | 2 | 1h |
| 2.2.3 | Implement `add_step()` | 2 | 1h |
| 2.2.4 | NDJSON persistence | 2 | 1h |
| 2.2.5 | List and cleanup | 2 | 1h |

**Subtotal:** 5 tasks | 8 tests | ~4.5 hours

### 2.3 JobScheduler (~500 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.3.1 | Create `src/jobs/mod.rs`, `scheduler.rs`, `store.rs` | 0 | 0.5h |
| 2.3.2 | Implement `JobStore` (SQLite) | 3 | 2h |
| 2.3.3 | Implement `submit()`, `cancel()` | 2 | 1h |
| 2.3.4 | JoinSet + Semaphore concurrency | 2 | 2h |
| 2.3.5 | Graceful shutdown | 2 | 1h |

**Subtotal:** 5 tasks | 9 tests | ~6.5 hours

### 2.4 AgentManager (~648 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.4.1 | Create `src/runtime/agent_manager.rs` | 0 | 0.5h |
| 2.4.2 | Implement `spawn()`, `cancel()` | 3 | 2h |
| 2.4.3 | Depth limiting logic | 3 | 1.5h |
| 2.4.4 | Concurrency control | 2 | 1h |
| 2.4.5 | Status reporting | 1 | 0.5h |

**Subtotal:** 5 tasks | 9 tests | ~5.5 hours

### 2.5 AutonomyOrchestrator (~1017 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.5.1 | Create `src/runtime/autonomy_orchestrator.rs` | 0 | 0.5h |
| 2.5.2 | Implement `request_approval()` | 3 | 2h |
| 2.5.3 | Implement approval flow | 3 | 2h |
| 2.5.4 | Policy enforcement | 3 | 1.5h |
| 2.5.5 | Decision logging | 2 | 1h |
| 2.5.6 | Stats tracking | 1 | 0.5h |

**Subtotal:** 6 tasks | 12 tests | ~7.5 hours

### 2.6 SuggestionAnalyzer (~907 lines)

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.6.1 | Create `src/runtime/suggestion_analyzer.rs` | 0 | 0.5h |
| 2.6.2 | Implement trigger registration | 2 | 1h |
| 2.6.3 | Implement context analysis | 3 | 2h |
| 2.6.4 | Suggestion generation | 3 | 2h |
| 2.6.5 | Priority ranking | 2 | 1h |

**Subtotal:** 5 tasks | 10 tests | ~6.5 hours

### 2.7 Daemon Integration

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 2.7.1 | Create `src/daemon/mod.rs`, `server.rs` | 0 | 0.5h |
| 2.7.2 | Unix socket server | 3 | 2h |
| 2.7.3 | IPC message handling | 3 | 2h |
| 2.7.4 | Wire all components | 2 | 2h |

**Subtotal:** 4 tasks | 8 tests | ~6.5 hours

**Phase 2 Total:** 36 tasks | 67 tests | ~42 hours

### WIRING Checkpoint

```
☐ cargo test store::memory
☐ cargo test store::trace_store
☐ cargo test jobs
☐ cargo test runtime::agent_manager
☐ cargo test runtime::autonomy_orchestrator
☐ cargo test runtime::suggestion_analyzer
☐ cargo test daemon
☐ nika daemon start works
☐ nika daemon status shows components
☐ cargo clippy -- -D warnings
```

---

## Phase 3: Package Manager (✅ DONE)

**Status:** Completed during v0.27.0 development
**Plan:** [03-PHASE-3-PACKAGE-MANAGER.md](./03-PHASE-3-PACKAGE-MANAGER.md)

No additional work required.

---

## Phase 4: Config Scope (~400 lines, ~7 hours)

**Focus:** Three-level config hierarchy
**Plan:** [04-PHASE-4-CONFIG-SCOPE.md](./04-PHASE-4-CONFIG-SCOPE.md)

### Tasks

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 4.1 | Create `src/core/config_scope.rs` | 2 | 0.5h |
| 4.2 | Create `src/core/config_loader.rs` | 3 | 2h |
| 4.3 | Implement merge logic | 2 | 1h |
| 4.4 | Create `nika config` command | 2 | 1.5h |
| 4.5 | Integration with provider detection | 1 | 1h |
| 4.6 | Documentation | 0 | 0.5h |

**Total:** 6 tasks | 10 tests | ~6.5 hours

### WIRING Checkpoint

```
☐ cargo test core::config_scope
☐ cargo test core::config_loader
☐ nika config show works
☐ nika config where provider shows scope
☐ nika config files lists all paths
☐ cargo clippy -- -D warnings
```

---

## Phase 5: Interop & Retirement (~200 lines, ~4.5 hours)

**Focus:** Deprecate spn and finalize nika
**Plan:** [05-PHASE-5-INTEROP.md](./05-PHASE-5-INTEROP.md)

### Tasks

| ID | Task | Tests | Effort |
|----|------|-------|--------|
| 5.1 | Add deprecation warning to spn | 1 | 0.5h |
| 5.2 | Implement command mapping | 1 | 0.5h |
| 5.3 | Write migration guide | 0 | 1h |
| 5.4 | Update README files | 0 | 0.5h |
| 5.5 | Socket path compatibility | 1 | 0.5h |
| 5.6 | Update CHANGELOG | 0 | 0.5h |
| 5.7 | End-to-end testing | 2 | 1h |

**Total:** 7 tasks | 5 tests | ~4.5 hours

### WIRING Checkpoint

```
☐ spn shows deprecation warning
☐ spn commands still work
☐ nika has all spn functionality
☐ Migration guide is accurate
☐ cargo test deprecation
```

---

## Summary

| Phase | Tasks | Tests | Lines | Hours |
|-------|-------|-------|-------|-------|
| Phase 1: Core Types | 7 | 12 | 500 | 5 |
| Phase 2: Daemon Features | 36 | 67 | 3,500 | 42 |
| Phase 3: Package Manager | - | - | - | ✅ Done |
| Phase 4: Config Scope | 6 | 10 | 400 | 7 |
| Phase 5: Interop | 7 | 5 | 200 | 4.5 |
| **Total** | **56** | **94** | **~4,600** | **~58.5** |

---

## Timeline

| Week | Focus | Deliverables |
|------|-------|--------------|
| 1 | Phase 1 + Phase 2.1-2.2 | Core types, MemoryStore, TraceStore |
| 2 | Phase 2.3-2.4 | JobScheduler, AgentManager |
| 3 | Phase 2.5-2.6 | AutonomyOrchestrator, SuggestionAnalyzer |
| 4 | Phase 2.7 + Phase 4-5 | Daemon integration, Config, Interop |

---

## Success Criteria

1. **All spn daemon features work in nika daemon**
2. **`spn <cmd>` shows deprecation warning pointing to `nika`**
3. **No functionality regression from spn**
4. **Test coverage ≥80% for all migrated code**
5. **Zero clippy warnings**
6. **Documentation updated**
7. **Migration guide accurate and tested**

---

## Related Documents

- [Master Plan](./00-MASTER-PLAN.md)
- [Phase 1: Core Types](./01-PHASE-1-CORE-TYPES.md)
- [Phase 2: Daemon Features](./02-PHASE-2-DAEMON-FEATURES.md)
- [Phase 3: Package Manager](./03-PHASE-3-PACKAGE-MANAGER.md)
- [Phase 4: Config Scope](./04-PHASE-4-CONFIG-SCOPE.md)
- [Phase 5: Interop](./05-PHASE-5-INTEROP.md)
