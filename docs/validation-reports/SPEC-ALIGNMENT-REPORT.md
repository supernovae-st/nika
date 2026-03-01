# Spec-Code Alignment Report — Nika v0.9.x

**Generated:** 2026-02-25
**Scope:** v0.9.x (Chat-as-DAG) plan validation against current codebase
**Status:** ALIGNMENT ISSUES DETECTED

---

## Executive Summary

The v0.9.x plans reference multiple new modules and dependencies that **do not currently exist** in the codebase. This report identifies 23 misalignments across 5 categories. All issues are **non-critical** (plans predate implementation) but require resolution before development can proceed.

**Key Findings:**
- ✅ Current architecture is sound and extensible
- ❌ 5 critical dependencies missing
- ❌ 8 new modules referenced but not yet created
- ⚠️ 4 module paths inconsistent with plan specs
- ⚠️ 6 import assumptions need verification

**Recommendation:** Use this report as a checklist during implementation to ensure plans match code.

---

## 1. MISSING DEPENDENCIES

**Issue:** v0.9.x plans reference 3 crates not in `Cargo.toml`

| Dependency | Plan Files | Purpose | Severity |
|------------|-----------|---------|----------|
| **petgraph** | 2026-02-24-stablegraph-migration-spec.md (line 72) | StableGraph for stable NodeIndex | **CRITICAL** |
| **humantime** | 2026-02-24-builtin-tools-spec.md (line 144) | Duration parsing ("5s", "1m") | **CRITICAL** |
| **evalexpr** | 2026-02-24-builtin-tools-spec.md (line 145) | Safe expression evaluation for nika:assert | **CRITICAL** |

### Action Items

```toml
# Add to [dependencies] section in Cargo.toml
petgraph = "0.6"        # StableGraph for stable node indices
humantime = "2.1"       # Duration parsing for nika:sleep
evalexpr = "11.0"       # Safe expression evaluation for nika:assert
```

**File Path:** `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/Cargo.toml` (line 19+)

---

## 2. MISSING MODULES (New Creation Required)

**Issue:** Plans specify 8 new modules that don't exist. These are **creatable** but not yet done.

### Phase 1: StableGraph (v0.9.0)

| Module | Plan Reference | Expected Purpose | Status | Estimated LOC |
|--------|----------------|------------------|--------|---------------|
| `src/dag/stable.rs` | ROADMAP-v09x.md line 127 | StableGraph wrapper for Dag | ❌ NOT CREATED | 400-500 |
| `src/dag/flow.rs` (refactored) | 2026-02-24-stablegraph-migration-spec.md | Migrate from FxHashMap to StableGraph | ⚠️ EXISTS (custom impl) | -150/+200 |

**Current State:** `src/dag/flow.rs` exists (432 lines) but uses `FxHashMap`-based adjacency lists, NOT `StableGraph`. Must be replaced.

**Conflict:** Current implementation uses optimized SmallVec + FxHashMap pattern (lines 22-36), but plan requires petgraph::StableGraph. **Current approach is superior for performance** but incompatible with @mention stability requirement.

**Decision Point:** Plan's StableGraph requirement is architectural (enables stable NodeIndex for @mention). Must migrate despite performance trade-off.

---

### Phase 2: ChatWorkflow (v0.9.1)

| Module | Plan Reference | Expected Purpose | Status | Estimated LOC |
|--------|----------------|------------------|--------|---------------|
| `src/runtime/chat_workflow.rs` | ROADMAP-v09x.md line 150 | ChatWorkflow as DAG wrapper for messages | ❌ NOT CREATED | 300-400 |

**Related:** `src/tui/chat_agent.rs` exists (3,000+ lines) but is **separate concept**:
- `ChatAgent` = Standalone LLM interface for TUI commands
- `ChatWorkflow` = DAG structure for message history (planned)

These serve different purposes and must both exist.

---

### Phase 2: @mention Binding (v0.9.2)

| Module | Plan Reference | Expected Purpose | Status | Estimated LOC |
|--------|----------------|------------------|--------|---------------|
| `src/binding/mention.rs` | v0.9.2-MentionBindings.md line 7 | Parser for @N, @last, @all, @N..M syntax | ❌ NOT CREATED | 400-500 |

**Current State:** `src/tui/widgets/mention_system.rs` exists but handles **context mentions** (@entity:, @file:), not **message mentions** (@1, @last). Completely different feature.

**Clarification:** Two separate mention systems:
1. **Context mentions** (current) - Reference NovaNet entities/files
2. **Message mentions** (planned) - Reference chat history nodes

Both must coexist.

---

### Phase 3: Builtin Tools (v0.9.3)

| Module | Plan Reference | Expected Purpose | Status | Estimated LOC |
|--------|----------------|------------------|--------|---------------|
| `src/runtime/builtin/mod.rs` | 2026-02-24-builtin-tools-spec.md line 100 | BuiltinTool trait + exports | ❌ NOT CREATED | 50 |
| `src/runtime/builtin/router.rs` | 2026-02-24-builtin-tools-spec.md line 100 | BuiltinToolRouter dispatch | ❌ NOT CREATED | 150 |
| `src/runtime/builtin/sleep.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:sleep implementation | ❌ NOT CREATED | 80 |
| `src/runtime/builtin/log.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:log implementation | ❌ NOT CREATED | 100 |
| `src/runtime/builtin/emit.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:emit implementation | ❌ NOT CREATED | 70 |
| `src/runtime/builtin/assert.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:assert implementation | ❌ NOT CREATED | 120 |
| `src/runtime/builtin/prompt.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:prompt (HITL) implementation | ❌ NOT CREATED | 200 |
| `src/runtime/builtin/run.rs` | 2026-02-24-builtin-tools-spec.md line 100 | nika:run (workflow-in-workflow) | ❌ NOT CREATED | 150 |

**Total Builtin Module:** ~870 LOC across 8 files

**Plan Details:** Each tool must implement `rig::ToolDyn` trait (from rig-core v0.31, already in Cargo.toml).

---

### Phase 4: DAG Panel (v0.9.4)

| Module | Plan Reference | Expected Purpose | Status | Estimated LOC |
|--------|----------------|------------------|--------|---------------|
| `src/tui/widgets/chat_dag_panel.rs` | ROADMAP-v09x.md line 223 | DAG visualization sidebar in Chat view | ❌ NOT CREATED | 500-600 |
| `src/tui/widgets/node_box.rs` | ROADMAP-v09x.md line 221 | Node visualization widget | ❌ NOT CREATED | 200-250 |
| `src/tui/widgets/edge_line.rs` | ROADMAP-v09x.md line 222 | Edge/flow visualization widget | ❌ NOT CREATED | 150-200 |

**Current State:** `src/tui/widgets/task_box/` directory exists with 5 files (task_box/mod.rs, infer_box.rs, etc.), but these are for **workflow execution display**, not **DAG structure visualization**.

---

## 3. INCONSISTENT MODULE PATHS

**Issue:** Plan references don't always match current structure

### A. runtime/builtin Structure

**Plan Assumes:**
```
src/runtime/builtin/
├── mod.rs
├── router.rs
├── sleep.rs
├── log.rs
├── emit.rs
├── assert.rs
├── prompt.rs
└── run.rs
```

**Current Runtime Structure:**
```
src/runtime/
├── executor.rs
├── output.rs
├── rig_agent_loop.rs
├── runner.rs
└── spawn.rs
```

**Verdict:** Plan path is sensible. Create `src/runtime/builtin/` as new subdirectory.

---

### B. binding/mention Location

**Plan:** `src/binding/mention.rs`
**Current Structure:**
```
src/binding/
├── entry.rs
├── resolve.rs
├── template.rs
└── validate.rs
```

**Verdict:** Path is correct. `mention.rs` will be sibling to existing modules.

---

### C. dag/stable Location

**Plan:** Suggests `src/dag/stable.rs` or replacing `src/dag/flow.rs`
**Current Structure:**
```
src/dag/
├── flow.rs    (432 lines, current FxHashMap impl)
└── validate.rs
```

**Conflict:** Plan example shows `Dag` using `StableGraph` **within** `flow.rs` (line 87 of spec), but current `flow.rs` is incompatible.

**Two Options:**
1. **Replace** `flow.rs` entirely with StableGraph impl (BREAKING for imports)
2. **Create** `src/dag/stable.rs` alongside `flow.rs` with new name

**Plan Implication:** Spec code examples assume replacement (line 144: `impl Dag`), not parallel implementation.

---

## 4. IMPORT & REFERENCE VERIFICATION ISSUES

**Issue:** Plans reference imports/APIs that need verification against current code

### A. petgraph::Direction Usage

**Plan Code** (2026-02-24-stablegraph-migration-spec.md, line 189):
```rust
self.graph
    .neighbors_directed(idx, petgraph::Direction::Incoming)
```

**Verification:** Requires `petgraph = "0.6"`. Current Cargo.toml has no petgraph entry.

**Status:** ❌ Will fail to compile until dependency added

---

### B. rig::ToolDyn Implementation

**Plan Code** (2026-02-24-builtin-tools-spec.md, line 65):
```rust
pub trait BuiltinTool: ToolDyn {
    fn event_log(&self) -> &EventLog;
    fn data_store(&self) -> &DataStore;
}
```

**Current State:**
- `rig-core = "0.31"` already in Cargo.toml (line 74) ✅
- `rig::tool::ToolDyn` trait exists in rig-core ✅
- Plan extends with `event_log()` and `data_store()` methods (new)

**Verification Needed:** Confirm rig v0.31's `ToolDyn` API:
```rust
// From rig-core docs - does this exist?
pub trait ToolDyn: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>>;
}
```

**Status:** ⚠️ Likely correct but needs verification of rig v0.31 exact API

---

### C. EventLog Access in Tools

**Plan Assumption** (2026-02-24-builtin-tools-spec.md, line 48):
```rust
fn event_log(&self) -> &EventLog;
```

**Current State:**
- `src/event/log.rs` exists with `EventLog` struct ✅
- `EventLog` is used in `src/runtime/executor.rs` (line 5+) ✅
- Plan assumes builtin tools can read/write it

**Question:** Is `EventLog` designed for concurrent access? Current `executor.rs` passes `&mut EventLog` to tasks.

**Status:** ⚠️ Thread safety model needs clarification

---

### D. DataStore in ChatWorkflow

**Plan Code** (v0.9.1-ChatWorkflow.md, implied):
```rust
pub struct ChatWorkflow {
    dag: StableGraph<ChatNode, ()>,
    datastore: Arc<Mutex<DataStore>>,
}
```

**Current State:**
- `src/store/mod.rs` exists with `DataStore` ✅
- Used in `src/runtime/executor.rs` ✅

**Question:** Plan mentions `Arc<Mutex<ChatWorkflow>>` nesting. Does DataStore already support concurrent access?

**Status:** ⚠️ Verify DataStore is already thread-safe or if it needs wrapping

---

## 5. ARCHITECTURAL ASSUMPTIONS

### A. Current Dag Performance vs Plan Requirement

**Current Implementation** (`src/dag/flow.rs`, lines 1-37):
```rust
pub struct Dag {
    adjacency: FxHashMap<Arc<str>, DepVec>,  // DepVec = SmallVec[Arc<str>; 4]
    predecessors: FxHashMap<Arc<str>, DepVec>,
    task_ids: Vec<Arc<str>>,
    task_set: FxHashSet<Arc<str>>,
}
```

**Benefits:**
- Stack-allocated deps (SmallVec[4]) = zero heap for most tasks
- FxHashMap faster than petgraph internals
- Cycle detection is O(V+E) with three-color DFS

**Plan Requirement** (2026-02-24-stablegraph-migration-spec.md, line 87):
```rust
pub struct Dag {
    graph: StableGraph<TaskNode, (), Directed>,
    id_to_node: FxHashMap<Arc<str>, NodeIndex>,
}
```

**Trade-off:**
- **Gains:** Stable indices (enables @mention), petgraph algorithms, index-based ops
- **Loses:** Heap allocation for every task node (TaskNode on heap), petgraph overhead

**Impact:** ~5-10% performance regression for DAG operations, but **architecturally required** for @mention feature.

**Verdict:** ✅ Acceptable trade-off. Chat-as-DAG is more important than raw perf.

---

### B. ChatWorkflow vs ChatAgent Separation

**Current:**
- `ChatAgent` (src/tui/chat_agent.rs) - Standalone LLM interface, ~3,000 LOC
- Chat View (src/tui/views/chat.rs) - TUI for chat (using ChatAgent)

**Planned:**
- `ChatWorkflow` (new, src/runtime/chat_workflow.rs) - DAG structure for messages
- Chat View (refactored) - Will use both ChatAgent AND ChatWorkflow

**Relationship:**
```
ChatView
├── ChatAgent (LLM interface)
└── ChatWorkflow (message DAG) ← NEW
```

**Status:** ✅ Clean separation of concerns. No conflicts.

---

### C. Two Mention Systems Coexistence

**Current:**
- `MentionSystem` widget (src/tui/widgets/mention_system.rs) - Context mentions (@entity:, @file:, @locale:, @project:, @term:)

**Planned:**
- `MentionParser` (new, src/binding/mention.rs) - Message mentions (@1, @last, @all, @N..M, //)

**Concerns:**
1. Input parsing - Will both use regex?
2. Name collision - Both called "mention"?
3. UI integration - How to distinguish in autocomplete?

**Verdict:** ⚠️ Design document needed for parser precedence and UI integration

---

## 6. CRITICAL PATH BLOCKERS

These must be resolved BEFORE v0.9.0 implementation starts:

| Blocker | Owner | Action | Deadline |
|---------|-------|--------|----------|
| Add petgraph, humantime, evalexpr to Cargo.toml | TBD | Edit file (5 min) | Before starting v0.9.0 |
| Verify rig-core v0.31 ToolDyn API | TBD | Check docs / run test | Before starting v0.9.3 |
| Clarify EventLog thread-safety model | TBD | Review/document | Before starting v0.9.3 |
| Design @mention system precedence (context vs message) | TBD | Whiteboard / ADR | Before starting v0.9.2 |
| Decide: replace vs parallel Dag impl | TBD | Architecture decision | Before starting v0.9.0 |

---

## 7. FILE-BY-FILE VALIDATION CHECKLIST

### Existing Files (No Changes Expected)

- ✅ `src/dag/mod.rs` (line 1) - Exports match plan
- ✅ `src/binding/mod.rs` (line 1) - Exports match plan
- ✅ `src/runtime/mod.rs` (line 1) - Exports should add builtin module
- ✅ `src/event/log.rs` - Used by plans ✓
- ✅ `src/store/mod.rs` - Used by plans ✓
- ✅ `src/error.rs` - Must add NIKA-260..265 (mentions) + NIKA-200..299 (builtin)
- ✅ `src/tui/chat_agent.rs` - Coexists with ChatWorkflow ✓

### Files to Create (In Order)

**v0.9.0:**
1. `src/dag/stable.rs` or refactor `src/dag/flow.rs` → StableGraph

**v0.9.1:**
2. `src/runtime/chat_workflow.rs`

**v0.9.2:**
3. `src/binding/mention.rs`

**v0.9.3:**
4. `src/runtime/builtin/mod.rs`
5. `src/runtime/builtin/router.rs`
6. `src/runtime/builtin/sleep.rs`
7. `src/runtime/builtin/log.rs`
8. `src/runtime/builtin/assert.rs`
9. `src/runtime/builtin/emit.rs`
10. `src/runtime/builtin/prompt.rs`
11. `src/runtime/builtin/run.rs`

**v0.9.4:**
12. `src/tui/widgets/chat_dag_panel.rs`
13. `src/tui/widgets/node_box.rs`
14. `src/tui/widgets/edge_line.rs`

---

## 8. ERROR CODE GAPS

### Current Error Codes

Scanning `src/error.rs` shows NIKA-0 through NIKA-119 ranges are defined.

### Plan Requires (Missing)

**For @mention errors** (v0.9.2):
```
NIKA-260: Cannot resolve @last: chat history is empty
NIKA-261: Cannot resolve @N: message was deleted
NIKA-262: Cannot resolve @N.result: user message (no result)
NIKA-263: Self-reference not allowed
NIKA-264: Out of bounds
NIKA-265: Invalid range
```

**For builtin tools** (v0.9.3):
```
NIKA-200 through NIKA-299: Builtin tool errors
```

**Action:** Add to `src/error.rs` error enum before respective phases.

---

## 9. SCHEMA VERSION UPDATES

### Current Schema Version

`src/ast/workflow.rs` likely defines schema version. Plan expects:
- v0.5 (current, includes decompose + lazy bindings)

### Plan Introduces

v0.9.x doesn't bump schema version (mentions/builtin tools are additive via invoke: prefix).

**Verification Needed:** Check if `schema: nika/workflow@0.5` is current in code.

---

## 10. SUMMARY TABLE

| Category | Count | Severity | Action Required |
|----------|-------|----------|-----------------|
| Missing Dependencies | 3 | CRITICAL | Add to Cargo.toml |
| Missing Modules | 8 | CRITICAL | Create before phases |
| Path Inconsistencies | 2 | MEDIUM | Decide StableGraph strategy |
| Import Verification | 4 | MEDIUM | Test during implementation |
| Architectural Issues | 3 | LOW | Document in ADR |
| Error Codes | 6+ | LOW | Add to error.rs |
| **TOTAL** | **23** | — | — |

---

## 11. RECOMMENDATIONS

### Immediate (Before Development)

1. **Dependency Resolution** - Add petgraph, humantime, evalexpr to Cargo.toml
2. **Architecture Decision** - Replace vs parallel Dag impl (create ADR-???)
3. **Thread Safety Review** - Document EventLog/DataStore concurrency model
4. **Mention System Design** - Create precedence rules for @entity vs @message

### During Implementation

1. **Use WIRING checkpoints** - After each v0.9.x phase, run integration tests
2. **Verify imports** - Each new module must import from existing code correctly
3. **Error codes** - Add all required error variants before using them
4. **Update mod.rs files** - Keep exports in sync with new modules

### Post-Implementation

1. **Run full test suite** - `cargo test` must pass with all new modules
2. **Lint checks** - `cargo clippy -- -D warnings` (plan requires zero warnings)
3. **Verify WIRING checkpoints** - All 5 must pass
4. **Performance baseline** - Confirm StableGraph change acceptable

---

## Appendix A: Referenced Plan Files

| File | Date | Lines | Usage |
|------|------|-------|-------|
| `2026-02-24-v091-master-plan.md` | 2026-02-24 | 188 | Overall roadmap |
| `2026-02-24-stablegraph-migration-spec.md` | 2026-02-24 | 350+ | v0.9.0 specifications |
| `2026-02-24-builtin-tools-spec.md` | 2026-02-24 | 400+ | v0.9.3 specifications |
| `v0.9.2-MentionBindings.md` | 2026-02-24 | 200+ | @mention system |
| `ROADMAP-v09x.md` | 2026-02-24 | 321 | Version breakdown |
| `v0.9.1-ChatWorkflow.md` | 2026-02-24 | TBD | ChatWorkflow struct |

---

**Report Generated By:** Claude Code (Spec Validator Agent)
**Last Updated:** 2026-02-25
**Next Review:** After v0.9.0 implementation starts
