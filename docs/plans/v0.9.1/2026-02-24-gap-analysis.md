# Nika v0.9.1 — Gap Analysis

**Date:** 2026-02-24
**Status:** Review Required
**Authors:** Claude (Audit)

---

## Executive Summary

Audit of all v0.9.1 plan documents identified:
- **5 documents needing review** (incomplete/draft sections)
- **6 missing technical specifications** (referenced but not detailed)
- **5 new plans recommended** (for implementation precision)
- **3 consistency issues** (across documents)

**Priority:** Address gaps BEFORE B1.1 implementation to avoid rework.

---

## Part 1: Documents Needing Review

### 1.1 chat-as-workflow-dag.md — HIGH PRIORITY

| Section | Issue | Action Required |
|---------|-------|-----------------|
| Open Questions | 4 questions not resolved | Need decisions documented |
| Session Recovery | "Option: Persist to .nika/sessions/" | Need concrete JSON schema |
| Max DAG Size | "Option: Collapse old nodes" | Need memory limits spec |
| Error Recovery | "Option: Allow retry from DAG panel" | Need retry UX spec |
| Thread-safety | Not mentioned | INDEX mentions it's CRITICAL |
| Performance | Not specified | INDEX says "60fps with 100+ nodes" |

**Recommendation:** Add sections for Thread-safety, Performance Targets, and resolve Open Questions.

### 1.2 chat-dag-implementation-plan.md — HIGH PRIORITY

| Section | Issue | Action Required |
|---------|-------|-----------------|
| Phase 3 | "Builtin Tools" only referenced, not detailed | Need code snippets like other phases |
| NodeType | Only mentioned, no struct definitions | Need full enum with all fields |
| StableGraph | References B1.1 but assumes it's done | Need to clarify dependency |
| Thread-safety tests | Missing | INDEX says "Arc<Mutex<ChatWorkflow>>" pattern |

**Recommendation:** Add Phase 3 code snippets, NodeType struct definitions, thread-safety test section.

### 1.3 v091-consolidated-design.md — MEDIUM PRIORITY

| Section | Issue | Action Required |
|---------|-------|-----------------|
| Status | "Draft (Final Consolidation)" | Need to finalize |
| Part 2 Agent Mode 3 | File truncated, Mode 3 missing | Need to complete Mode 3 (Inherit) |
| Boot Sequence | Mentioned but not detailed | Need 6-phase breakdown |
| SOUL Pattern | Only referenced | Need complete SOUL structure |

**Recommendation:** Complete Part 2, add Boot Sequence section, add SOUL Pattern section.

### 1.4 memory-and-agents-design.md — MEDIUM PRIORITY

| Section | Issue | Action Required |
|---------|-------|-----------------|
| Status | Draft | Need review |
| memory.yaml validation | Not specified | Need schema + error messages |
| Episodic memory | Future v1.0 but mentioned | Clarify v0.9 vs v1.0 scope |
| MemoryConfig AST | Referenced but not defined | Need struct definition |

**Recommendation:** Add validation rules, clarify version scope, add AST structs.

### 1.5 nika-project-structure.md — LOW PRIORITY

| Section | Issue | Action Required |
|---------|-------|-----------------|
| Status | Draft | Need review |
| `--with-examples` | Files mentioned but no templates | Need example file contents |
| .nika/ validation | Not specified | Need validation on structure |
| Migration v0.5 | Only shows before/after | Need migration script/commands |

**Recommendation:** Add example templates, validation rules, migration guide.

---

## Part 2: Missing Technical Specifications

### 2.1 StableGraph Migration Spec — BLOCKING

**Referenced in:** INDEX.md B1.1, chat-dag-implementation-plan.md
**Needed for:** All Chat-as-DAG work, @mention stability

**Missing details:**
- petgraph dependency version (0.6.x?)
- Exact FlowGraph struct changes
- NodeIndex↔TaskId mapping strategy
- Node removal impact on references
- Performance benchmarks (add/remove operations)

**Recommended file:** `2026-02-24-stablegraph-migration-spec.md`

### 2.2 Builtin Tools Spec — BLOCKING

**Referenced in:** INDEX.md (TIER 1: 6 tools), chat-as-workflow-dag.md Phase 3
**Needed for:** nika:prompt, nika:run, nika:sleep, nika:log, nika:assert, nika:emit

**Missing details:**
- `BuiltinTool` trait definition (vs MCP ToolDyn)
- Per-tool parameters and return values
- invoke: routing logic (nika:* prefix detection)
- TUI integration for nika:prompt (pause/resume)
- Error codes per tool

**Recommended file:** `2026-02-24-builtin-tools-spec.md`

### 2.3 Thread-Safety Architecture — CRITICAL

**Referenced in:** INDEX.md Quality Gates (🔴 CRITICAL x5)
**Needed for:** ChatWorkflow, DAG mutations, EventLog

**Missing details:**
- `Arc<Mutex<ChatWorkflow>>` pattern code
- `AtomicU32` for task ID generation
- Locks across `.await` audit
- Bounded event queue (1000 events)
- `parking_lot::Mutex` vs `std::sync::Mutex`

**Recommended file:** `2026-02-24-thread-safety-architecture.md`

### 2.4 Unified Session Format — NEEDED

**Referenced in:** v091-implementation-plan.md, session.rs
**Needed for:** Chat/Workflow persistence, DAG state restoration

**Missing details:**
- Session JSON schema (TypeScript/JSON Schema)
- Chat session fields vs Workflow session fields
- DAG state serialization (nodes, edges, results)
- Backward compatibility (v0.8 sessions)

**Recommended file:** `2026-02-24-unified-session-schema.md`

### 2.5 HITL (Human-in-the-Loop) Flow — NEEDED

**Referenced in:** INDEX.md (nika:prompt in Chat AND Workflow)
**Needed for:** Workflow pause/resume, TUI prompt widget

**Missing details:**
- nika:prompt parameter types (confirm/text/select/multiselect)
- TUI widget design for prompt input
- Workflow pause mechanism (how Executor waits)
- Timeout handling (what happens on timeout?)
- Exported YAML representation

**Recommended file:** `2026-02-24-hitl-flow-spec.md`

### 2.6 NodeType Complete Spec — NEEDED

**Referenced in:** chat-as-workflow-dag.md, INDEX.md
**Needed for:** Chat message differentiation, binding resolution

**Missing details:**
```rust
// Need full definition with all fields
pub enum NodeType {
    Task {
        verb: TaskVerb,
        action: TaskAction,  // Missing in current docs
        status: TaskStatus,
        output: Option<Value>,
        duration: Option<Duration>,
        tokens: Option<TokenUsage>,  // Missing
    },
    UserInput {
        content: String,
        parsed_mentions: Vec<Mention>,  // Missing
        parsed_verb: Option<ParsedVerb>,  // Missing (for /infer prefix)
    },
    SystemMessage {
        level: MessageLevel,  // info/warn/error
        content: String,
    },
}
```

**Recommendation:** Add to chat-as-workflow-dag.md or create separate spec.

---

## Part 3: Recommended New Plans

### 3.1 2026-02-24-stablegraph-migration-spec.md — P0

**Purpose:** Detailed spec for FlowGraph → StableGraph refactor
**Contents:**
- petgraph version and features
- Struct definition before/after
- Migration steps
- Test cases for index stability
- Performance targets

### 3.2 2026-02-24-builtin-tools-spec.md — P0

**Purpose:** Complete specification for TIER 1 builtin tools
**Contents:**
- BuiltinTool trait design
- All 6 tools: params, returns, errors
- invoke: routing flowchart
- TUI integration diagrams

### 3.3 2026-02-24-thread-safety-architecture.md — P0

**Purpose:** Async-safe patterns for Chat-as-DAG
**Contents:**
- All CRITICAL issues from INDEX
- Code patterns (parking_lot, AtomicU32)
- Anti-patterns (locks across .await)
- Test checklist

### 3.4 2026-02-24-unified-session-schema.md — P1

**Purpose:** Session file format for Chat/Workflow/Heartbeat
**Contents:**
- JSON Schema definition
- Version compatibility
- Migration from v0.8
- Example files

### 3.5 2026-02-24-boot-sequence-implementation.md — P1

**Purpose:** Detailed 6-phase boot sequence implementation
**Contents:**
- Phase order and dependencies
- Error handling per phase
- Performance targets (<500ms)
- Fallback behavior

---

## Part 4: Consistency Issues

### 4.1 Schema Version

| Document | Mentions |
|----------|----------|
| CLAUDE.md | nika/workflow@0.5 |
| INDEX.md | nika/workflow@0.5 (MVP 8), @0.6 (v0.9) |
| v091-consolidated-design.md | nika/workflow@0.6 |
| chat-dag-implementation-plan.md | nika/workflow@0.5 |

**Resolution needed:** Clarify which features go in @0.5 vs @0.6.

### 4.2 Test Count Target

| Document | Target |
|----------|--------|
| INDEX.md | 2,400+ |
| v091-implementation-plan.md | 2,200+ |

**Resolution needed:** Pick one target, update both docs.

### 4.3 LOC Estimates

| Document | New LOC |
|----------|---------|
| INDEX.md | ~5,500 |
| v091-implementation-plan.md | ~4,500 to ~5,450 |

**Resolution needed:** Reconcile after completing missing specs.

---

## Part 5: Action Items

### Immediate (Before B1.1)

| ID | Action | Owner | Priority |
|----|--------|-------|----------|
| A1 | Create stablegraph-migration-spec.md | Dev | P0 |
| A2 | Create builtin-tools-spec.md | Dev | P0 |
| A3 | Create thread-safety-architecture.md | Dev | P0 |
| A4 | Resolve Open Questions in chat-as-workflow-dag.md | Thibaut | P0 |
| A5 | Add Phase 3 details to chat-dag-implementation-plan.md | Dev | P0 |

### Before B4.1 (Chat-as-DAG)

| ID | Action | Owner | Priority |
|----|--------|-------|----------|
| B1 | Complete NodeType enum spec | Dev | P1 |
| B2 | Create unified-session-schema.md | Dev | P1 |
| B3 | Create hitl-flow-spec.md | Dev | P1 |
| B4 | Add performance targets to chat-as-workflow-dag.md | Dev | P1 |

### Before v0.9.5

| ID | Action | Owner | Priority |
|----|--------|-------|----------|
| C1 | Create boot-sequence-implementation.md | Dev | P1 |
| C2 | Resolve schema version consistency | Thibaut | P2 |
| C3 | Reconcile test/LOC targets | Dev | P2 |
| C4 | Complete nika-project-structure.md examples | Dev | P2 |

---

## Appendix: Document Status Matrix

| Document | Status | Completeness | Blocking? |
|----------|--------|--------------|-----------|
| INDEX.md | Active | 95% | No |
| chat-as-workflow-dag.md | Draft | 75% | Yes (Open Questions) |
| chat-dag-implementation-plan.md | Draft | 80% | Yes (Phase 3, NodeType) |
| chat-workflow-conversion.md | New | 90% | No |
| v091-consolidated-design.md | Draft | 70% | Yes (Mode 3, Boot) |
| v091-implementation-plan.md | Approved | 85% | No |
| memory-and-agents-design.md | Draft | 70% | No |
| nika-project-structure.md | Draft | 65% | No |
| nika-meta-execution-plan.md | Draft | 60% | No |

---

## References

- [INDEX.md](./INDEX.md) — Master v0.9.1 plan
- [v0.10+ INDEX.md](../v0.10+/INDEX.md) — Future plans
- [CLAUDE.md](../../tools/nika/CLAUDE.md) — Current CLI documentation
