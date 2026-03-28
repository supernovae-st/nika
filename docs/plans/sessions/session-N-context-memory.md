# Session N: P-CONTEXT + P-INTROSPECT + P-MEMORY-LOCAL (~8-10h, split across 3-4 sittings)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-28-phase1-context-memory.md` -- READ IT FIRST.
Master plan: `docs/plans/2026-03-28-v1-master-plan.md` for Phase 1.4-1.5 scope.
Dev reference: `tools/nika/CLAUDE.md` for conventions.

Depends on: Session M (P-RECORD) for Record struct and RunContext.records.

Key files:
- `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC) -- KNOWN_TASK_KEYS at line ~1683
- `tools/nika-engine/src/binding/resolve.rs` (3046 LOC) -- binding resolution
- `tools/nika-engine/src/runtime/runner.rs` (6524 LOC) -- task execution, post-workflow hook at ~2332
- `tools/nika-engine/src/runtime/executor/verbs.rs` (607 LOC) -- `estimate_tokens()` at line 12
- `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC) -- tool registration
- `tools/nika-event/src/log.rs` (3961 LOC) -- event definitions (43+ variants)
- `tools/nika-event/src/trace.rs` -- TraceWriter for NDJSON traces
- `tools/nika-daemon/src/storage.rs` (900 LOC) -- SQLite + WAL pattern
- `tools/nika-cli/src/trace.rs` (216 LOC) -- trace CLI

## Mission: Add context budgets, 4 introspection tools, NDJSON persistence, FTS5 search

This is the largest session -- covering 3 sub-phases that can be parallelized:
- **P-CONTEXT** (v0.54): `context_budget:` field + token counting + budget enforcement
- **P-INTROSPECT** (v0.54): 4 new builtin tools (nika:dag_info, nika:task_status, nika:threads, nika:orchestrate)
- **P-MEMORY-LOCAL** (v0.55): NDJSON record persistence + SQLite FTS5 index + `nika trace search`

### Methodology
For EVERY change: read code -> write failing test -> fix -> verify -> commit.
`cargo test --workspace --lib` (always --lib). 1 fix = 1 commit.

---

## PART 1: P-CONTEXT -- context_budget: AST Field

### Task 1: Add context_budget to AST pipeline

**File**: `tools/nika-core/src/ast/raw/task.rs` (209 LOC)
Add `pub context_budget: Option<Spanned<u32>>` to RawTask.

**File**: `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC)
- Add `"context_budget"` to KNOWN_TASK_KEYS (line ~1683)
- Parse: `get_u32_field(file_id, map, "context_budget")?`
- Assign to RawTask construction

**File**: `tools/nika-core/src/ast/analyzed/task.rs` (599 LOC)
Add `pub context_budget: Option<u32>` to AnalyzedTask.

**File**: `tools/nika-core/src/ast/analyzer/` (analyzer module)
Validate: `context_budget > 0 && context_budget <= 200_000`. Error code: NIKA-152.

**Tests** (6):
- Parse `context_budget: 4000` from YAML
- Parse missing `context_budget` (None)
- Reject `context_budget: 0` (NIKA-152)
- Reject `context_budget: 250000` (exceeds limit)
- Analyzer passes valid budget
- Unknown key detection still works with new field

**Estimated LOC**: ~90
**Commit**: `feat(ast): add context_budget field to task AST`

---

## PART 2: Token Counting Utilities

### Task 2: Create token_budget.rs module

**File**: `tools/nika-engine/src/binding/token_budget.rs` (NEW, ~220 LOC)

Extends the existing `estimate_tokens(char_len)` in `verbs.rs:12` with:
- `estimate_tokens_str(text)` -- CJK-aware (CJK ~2 chars/token, Latin ~4 chars/token)
- `estimate_tokens_value(value)` -- recursive JSON estimation
- `estimate_bindings_tokens(bindings)` -- total across all resolved bindings

Decision: NO `tiktoken-rs` dependency (3MB BPE data). Char-based heuristic is sufficient
for budget enforcement (truncating, not billing). Single upgrade point if precision needed later.

**Tests** (5):
- English text: "hello world" -> ~3 tokens
- CJK text: proportionally higher
- JSON value estimation
- Empty string -> 1 token (minimum)
- Mixed CJK/Latin

**Estimated LOC**: ~220 (module + tests)
**Commit**: `feat(binding): add token counting utilities for context budgets`

---

## PART 3: Budget Enforcement

### Task 3: Implement enforce_budget()

**File**: `tools/nika-engine/src/binding/token_budget.rs` (extend)

```rust
pub fn enforce_budget(
    bindings: &mut ResolvedBindings,
    budget: u32,
    task_id: &Arc<str>,
) -> Vec<EventKind> {
    let actual = estimate_bindings_tokens(bindings);
    if actual <= budget as u64 {
        return vec![EventKind::BudgetOk { task_id, budget, actual: actual as u32 }];
    }
    // Proportional truncation: sort by size descending, truncate largest
    // Word-boundary truncation for strings
    // Minimum 50 tokens per binding
    let truncated_fields = truncate_proportional(bindings, budget);
    vec![EventKind::BudgetExceeded { task_id, budget, actual: actual as u32, truncated_fields }]
}
```

**Integration**: `tools/nika-engine/src/runtime/runner.rs` (around line 2200-2300)
After `ResolvedBindings::from_with_spec_traced()`, call `enforce_budget()` if task has `context_budget`.

**Tests** (8):
- Under budget: no truncation, BudgetOk event
- Over budget: truncation applied, BudgetExceeded event
- Single large binding truncated
- Multiple bindings: proportional truncation
- Minimum 50 tokens preserved per binding
- JSON value truncation
- Word-boundary truncation for strings
- Budget enforcement disabled when no context_budget

**Estimated LOC**: ~160
**Commit**: `feat(binding): implement context budget enforcement with proportional truncation`

---

## PART 4: Budget Events

### Task 4: Add BudgetOk and BudgetExceeded events

**File**: `tools/nika-event/src/log.rs` (3961 LOC)

```rust
BudgetOk { task_id: Arc<str>, budget: u32, actual: u32 },
BudgetExceeded { task_id: Arc<str>, budget: u32, actual: u32, truncated_fields: Vec<String> },
```

**File**: `tools/nika-engine/src/display/format_event.rs` (739 LOC)
Formatting for both variants.

**Tests** (3): Serialization, task_id(), display.

**Estimated LOC**: ~50
**Commit**: `feat(event): add BudgetOk and BudgetExceeded events`

---

## PART 5: P-INTROSPECT -- 4 Builtin Tools

### Task 5: nika:dag_info tool

**File**: `tools/nika-engine/src/runtime/builtin/introspect_dag.rs` (NEW, ~80 LOC)

Returns: `{ task_count, edge_count, critical_path, parallel_groups, max_depth }`
Holds `Arc<Dag>`. Walks the DAG from `dag/flow.rs`.

**Tests** (3): single task, linear chain, diamond DAG.
**Commit**: `feat(builtin): add nika:dag_info introspection tool`

### Task 6: nika:task_status tool

**File**: `tools/nika-engine/src/runtime/builtin/introspect_task.rs` (NEW, ~100 LOC)

Params: `{ task_id: "research" }`
Returns: `{ task_id, status, duration_ms, tokens, cost_usd, has_record, output_preview }`
Holds `RunContext` + `EventLog`.

**Tests** (3): completed task, pending task, invalid task_id.
**Commit**: `feat(builtin): add nika:task_status introspection tool`

### Task 7: nika:threads tool

**File**: `tools/nika-engine/src/runtime/builtin/introspect_threads.rs` (NEW, ~90 LOC)

Params: `{ status?: "running" }`
Returns: array of `{ task_id, status, verb, started_at_ms, duration_ms }`
Scans EventLog for TaskStarted/Completed/Failed events.

**Tests** (3): no tasks, mixed status, filter by status.
**Commit**: `feat(builtin): add nika:threads introspection tool`

### Task 8: nika:orchestrate tool (stub)

**File**: `tools/nika-engine/src/runtime/builtin/introspect_orchestrate.rs` (NEW, ~70 LOC)

Returns basic workflow stats. Will be fully wired in P-ORCHESTRATE (v0.53).
For now: aggregates EventLog for round/budget/records info.

**Tests** (1): basic stats return.
**Commit**: `feat(builtin): add nika:orchestrate introspection tool (stub)`

### Task 9: Register all 4 introspection tools

**File**: `tools/nika-engine/src/runtime/builtin/mod.rs` (156 LOC)
Add module declarations + exports.

**File**: `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC)
Add `with_introspection(dag, datastore, event_log)` method.

**File**: `tools/nika-engine/src/runtime/runner.rs` (6524 LOC)
Call `with_introspection()` after constructing BuiltinToolRouter.

**Estimated LOC**: ~70
**Commit**: `feat(builtin): register 4 introspection tools in router`

---

## PART 6: P-MEMORY-LOCAL -- NDJSON Record Persistence

### Task 10: Create RecordWriter

**File**: `tools/nika-engine/src/store/record_writer.rs` (NEW, ~140 LOC)

After workflow completes (between WorkflowCompleted event at runner.rs ~2332 and write_trace
at ~2361), persist all task results as NDJSON records.

**Format**: One JSON line per record:
```json
{"timestamp":"2026-03-28T10:30:00Z","workflow":"landing-page","task_id":"research","summary":"...","confidence":0.9,"tokens_spent":1500}
```

**File naming**: `.nika/records/{workflow_name}_{ISO8601_compact}.ndjson`

**Integration**: Add `write_records()` call in `Runner::run()` after WorkflowCompleted.

**Tests** (5):
- Write records for simple workflow
- File naming convention
- NDJSON format validation (each line valid JSON)
- Empty workflow produces no file
- Sanitize workflow name for filesystem

**Estimated LOC**: ~140
**Commit**: `feat(store): add NDJSON record persistence after workflow completion`

### Task 11: SQLite FTS5 Index

**File**: `tools/nika-engine/src/store/record_index.rs` (NEW, ~230 LOC)

Follow daemon storage pattern: dedicated DB thread + mpsc channel + rusqlite with WAL.

**Schema**:
```sql
CREATE TABLE records (id INTEGER PRIMARY KEY, workflow_name TEXT, task_id TEXT, summary TEXT, ...);
CREATE VIRTUAL TABLE records_fts USING fts5(summary, key_findings, workflow_name, task_id, content='records');
```

**Dependencies**: `rusqlite = { version = "0.39", features = ["bundled"] }` (already in nika-daemon).

**Integration**: After writing NDJSON, also insert into FTS5 index.

**Tests** (7):
- Insert + search by keyword
- FTS5 BM25 ranking
- --since filter
- --workflow filter
- Empty results
- Cleanup removes old records
- Concurrent insert safety

**Estimated LOC**: ~230
**Commit**: `feat(store): add SQLite FTS5 index for cross-session record search`

### Task 12: `nika trace search` CLI command

**File**: `tools/nika-cli/src/trace.rs` (216 LOC)

Add `Search` variant to `TraceAction` enum:
```rust
Search { query: String, #[arg(short, long, default_value = "20")] limit: usize,
         #[arg(long)] since: Option<String>, #[arg(short, long)] workflow: Option<String> }
```

**Output**:
```
Records matching "QR code trends" (3 results)
  WORKFLOW              TASK         CONFIDENCE  DATE
  landing-page          research     0.90        2026-03-28
    QR code adoption grew 34% in 2025...
```

**Tests** (3): Results, no results, limit.
**Estimated LOC**: ~100
**Commit**: `feat(cli): add nika trace search for cross-session record search`

---

## PART 7: Supporting Features

### Task 13: Frozen snapshot documentation + guard

**File**: `tools/nika-engine/src/runtime/runner.rs` (~1171)
Context loading already frozen (loaded once, never re-read). Add:
- `frozen: bool` flag on LoadedContext to prevent double `set_context()`
- `ContextFrozen { file_count, total_tokens }` event

**Tests** (2): Second set_context returns error. Event emitted.
**Estimated LOC**: ~40
**Commit**: `feat(runtime): enforce frozen snapshot pattern on context loading`

### Task 14: File locking for concurrent writes

**File**: `tools/nika-engine/src/store/file_lock.rs` (NEW, ~75 LOC)
Use `fs2` crate for advisory locking. Wrap RecordWriter and RecordIndex inserts.

**Tests** (4): Acquire/release, second lock blocks, timeout, RAII cleanup.
**Estimated LOC**: ~75
**Commit**: `feat(store): add advisory file locking for concurrent record writes`

### Task 15: Security scanning on LLM outputs

**File**: `tools/nika-engine/src/runtime/output_scanner.rs` (NEW, ~200 LOC)

Scan patterns:
- Invisible Unicode (zero-width chars, directional overrides) -> Dangerous
- Exfiltration: curl/wget with variable interpolation -> Dangerous
- Role hijack: "ignore previous" patterns -> Warning
- Prompt injection: `<system>`, `` ```system `` -> Dangerous

Config: `security: { scan_outputs: true }` (default false in v0.54, true in v1.0).
Integration: runner.rs after task completes, before storing output in RunContext.

**New event**: `SecurityScanFinding { task_id, pattern, severity, sanitized }`

**Tests** (8): Each pattern type, clean text passes, sanitize removes dangerous, event emitted.
**Estimated LOC**: ~200
**Commit**: `feat(security): add output scanner for LLM injection detection`

---

## E2E Verification Workflows

### test-context-budget.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-context-budget
provider: mock

tasks:
  - id: long_output
    infer: "Generate a very long response about AI history"

  - id: constrained
    depends_on: [long_output]
    with: { data: $long_output }
    context_budget: 500
    infer: "Summarize: {{with.data}}"
    # Expected: data truncated to ~500 tokens before being passed to LLM
```
**Run**: `nika run test-context-budget.nika.yaml`

### test-introspection.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-introspection
provider: mock

tasks:
  - id: research
    infer: "Research topic"

  - id: agent_with_tools
    depends_on: [research]
    agent:
      prompt: "Check DAG status and task results"
      tools: [nika:dag_info, nika:task_status, nika:threads]
      max_turns: 3
      completion:
        mode: natural
```
**Run**: `nika run test-introspection.nika.yaml --provider mock`

### test-trace-search.sh (manual)
```bash
# Run a few workflows to populate records
nika run workflow-a.nika.yaml --provider mock
nika run workflow-b.nika.yaml --provider mock

# Search across sessions
nika trace search "QR code"
nika trace search "API" --workflow workflow-a --limit 5
nika trace search "trends" --since 2026-03-28
```

---

## After All Fixes

```bash
cd tools && cargo test --workspace --lib       # All pass, 67+ new tests
cd tools && cargo clippy --workspace -- -D warnings  # Zero warnings
nika check test-context-budget.nika.yaml       # Valid
nika trace search "test" 2>/dev/null           # Works or empty
```

---

## Commit Strategy (15 commits)

```
# P-CONTEXT (Parts 1-4)
feat(ast): add context_budget field to task AST
feat(binding): add token counting utilities for context budgets
feat(binding): implement context budget enforcement with proportional truncation
feat(event): add BudgetOk and BudgetExceeded events

# P-INTROSPECT (Parts 5-9)
feat(builtin): add nika:dag_info introspection tool
feat(builtin): add nika:task_status introspection tool
feat(builtin): add nika:threads introspection tool
feat(builtin): add nika:orchestrate introspection tool (stub)
feat(builtin): register 4 introspection tools in router

# P-MEMORY-LOCAL (Parts 10-12)
feat(store): add NDJSON record persistence after workflow completion
feat(store): add SQLite FTS5 index for cross-session record search
feat(cli): add nika trace search for cross-session record search

# Supporting (Parts 13-15)
feat(runtime): enforce frozen snapshot pattern on context loading
feat(store): add advisory file locking for concurrent record writes
feat(security): add output scanner for LLM injection detection
```
