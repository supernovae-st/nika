# spn→nika Migration Master Plan

## v0.27.0: spn→nika Feature Fusion

**Status**: Planning
**Target Release**: v0.27.0
**Total Scope**: ~3,991 lines | 59 types | 29 tests

---

## Executive Summary

This plan consolidates all `spn` daemon features into `nika`, making `nika` the **single CLI tool** for the SuperNovae ecosystem. After this migration, `spn` will be deprecated and users will interact exclusively with `nika`.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  BEFORE (v0.26)                    AFTER (v0.27)                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────┐     ┌─────────┐       ┌─────────────────────────────────────┐   ║
║  │  nika   │     │   spn   │       │              nika                  │   ║
║  │ ─────── │     │ ─────── │   →   │ ─────────────────────────────────── │   ║
║  │ Workflow│     │ Daemon  │       │ Workflows + Daemon + Package Mgmt  │   ║
║  │ Engine  │     │ Package │       │                                     │   ║
║  │         │     │ Manager │       │ provider, model, mcp, sync, setup   │   ║
║  └─────────┘     └─────────┘       │ daemon, jobs, backup, traces        │   ║
║       ↓               ↓             └─────────────────────────────────────┘   ║
║  ┌─────────────────────────┐                        ↓                         ║
║  │     spn daemon          │       ┌─────────────────────────────────────┐   ║
║  │ ─────────────────────── │   →   │           nika daemon               │   ║
║  │ Memory, Traces, Jobs    │       │ (same features, unified binary)     │   ║
║  │ Agents, Autonomy        │       └─────────────────────────────────────┘   ║
║  └─────────────────────────┘                                                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Migration Phases

| Phase | Name | Lines | Types | Tests | Status |
|-------|------|-------|-------|-------|--------|
| 1 | Core Types | ~500 | 15 | 8 | 📋 Planned |
| 2 | Daemon Features | ~3,500 | 44 | 21 | 📋 Planned |
| 3 | Package Manager | ~0 | - | - | ✅ Done (v0.27) |
| 4 | Config Scope | ~400 | 5 | 6 | 📋 Planned |
| 5 | Interop & Retirement | ~200 | 2 | 4 | 📋 Planned |

---

## What's Already Migrated (v0.27.0)

The package manager components were migrated in the v0.27.0 development cycle:

| Component | Location in nika | Status |
|-----------|------------------|--------|
| KNOWN_PROVIDERS (18) | `src/core/providers.rs` | ✅ Done |
| KNOWN_MODELS (16+) | `src/core/models.rs` | ✅ Done |
| MCP_ALIASES (48) | `src/core/mcp_aliases.rs` | ✅ Done |
| McpConfig | `src/core/mcp_config.rs` | ✅ Done |
| KeychainResolver | `src/secrets/mod.rs` | ✅ Done |
| DaemonClient IPC | `src/secrets/resolve.rs` | ✅ Done |

---

## What Remains to Migrate

### From `supernovae-cli/crates/spn/src/daemon/`

| Module | File | Lines | Key Types |
|--------|------|-------|-----------|
| **Memory** | `memory.rs` | 440 | MemoryStore, MemoryEntry, MemoryKey, MemoryNamespace |
| **Traces** | `traces.rs` | 479 | TraceStore, ReasoningTrace, TraceStep, TraceStepKind |
| **Jobs** | `jobs.rs` | 500 | JobScheduler, Job, JobId, JobState, JobStatus |
| **Agents** | `agents.rs` | 648 | AgentManager, Agent, AgentId, AgentRole, AgentState |
| **Autonomy** | `autonomy.rs` | 1017 | AutonomyOrchestrator, AutonomyLevel, ApprovalLevel, Decision |
| **Proactive** | `proactive.rs` | 907 | SuggestionAnalyzer, ProactiveSuggestion, ContextTrigger |

---

## Directory Structure (Target)

```
nika/tools/nika/src/
├── core/
│   ├── mod.rs                 # Re-exports
│   ├── providers.rs           # ✅ KNOWN_PROVIDERS (already done)
│   ├── models.rs              # ✅ KNOWN_MODELS (already done)
│   ├── mcp_aliases.rs         # ✅ MCP_ALIASES (already done)
│   ├── mcp_config.rs          # ✅ McpConfig (already done)
│   ├── autonomy.rs            # 🆕 AutonomyLevel, ApprovalLevel, Decision
│   └── agent_types.rs         # 🆕 AgentRole, AgentState, AgentStatus
├── store/
│   ├── mod.rs                 # Re-exports
│   ├── data.rs                # ✅ DataStore (already exists)
│   ├── memory.rs              # 🆕 MemoryStore
│   └── trace_store.rs         # 🆕 TraceStore
├── runtime/
│   ├── agent_manager.rs       # 🆕 AgentManager
│   ├── autonomy_orchestrator.rs # 🆕 AutonomyOrchestrator
│   └── suggestion_analyzer.rs # 🆕 SuggestionAnalyzer
├── jobs/
│   ├── mod.rs                 # 🆕 Job scheduling
│   ├── scheduler.rs           # 🆕 JobScheduler
│   └── store.rs               # 🆕 JobStore (SQLite)
└── daemon/
    ├── mod.rs                 # 🆕 Daemon entry point
    └── server.rs              # 🆕 Unix socket server
```

---

## Phase Summaries

### Phase 1: Core Types (~500 lines)
Zero-dependency types that form the foundation. No async, no I/O.
- `AutonomyLevel`, `ApprovalLevel`, `Decision`
- `AgentRole`, `AgentState`, `AgentStatus`, `DelegatedTask`
- `MemoryNamespace`, `MemoryKey`
- `TraceStepKind`

### Phase 2: Daemon Features (~3,500 lines)
The heavy lifting - concurrent data structures with tokio.
- `MemoryStore` with FxHashMap + JSON persistence
- `TraceStore` with NDJSON persistence
- `JobScheduler` with JoinSet + Semaphore
- `AgentManager` with depth limiting
- `AutonomyOrchestrator` with approval workflows
- `SuggestionAnalyzer` with context triggers

### Phase 3: Package Manager (✅ Done)
Already completed in v0.27 development.

### Phase 4: Config Scope
Three-level config hierarchy: Local → Team → Global.

### Phase 5: Interop & Retirement
- Deprecation warnings in spn
- Migration guide
- spn-client library maintenance

---

## Success Criteria

1. **All spn daemon features work in nika daemon**
2. **`spn <cmd>` shows deprecation warning pointing to `nika`**
3. **No functionality regression from spn**
4. **Test coverage ≥80% for all migrated code**
5. **Zero clippy warnings**
6. **Documentation updated**

---

## Plan Files

- `01-PHASE-1-CORE-TYPES.md` - Core type definitions
- `02-PHASE-2-DAEMON-FEATURES.md` - Daemon feature migration
- `03-PHASE-3-PACKAGE-MANAGER.md` - Package manager (already done)
- `04-PHASE-4-CONFIG-SCOPE.md` - Config hierarchy
- `05-PHASE-5-INTEROP.md` - Final integration and deprecation

---

## Timeline

| Week | Focus | Deliverables |
|------|-------|--------------|
| 1 | Phase 1 | Core types + tests |
| 2-3 | Phase 2 | Memory, Traces, Jobs |
| 3-4 | Phase 2 | Agents, Autonomy, Proactive |
| 4 | Phase 4-5 | Config, Interop, Deprecation |

---

## References

- **spn Daemon Source**: `supernovae-cli/crates/spn/src/daemon/`
- **ADR-008**: Inference Architecture (native inference in nika)
- **v0.27.0 CHANGELOG**: Package manager migration
- **Tokio Patterns**: JoinSet, Semaphore, CancellationToken
