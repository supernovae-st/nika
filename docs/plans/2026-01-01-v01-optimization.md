# Nika v0.1 Optimization Plan

> **Date**: 2026-01-01 | **Status**: COMPLETED

## Summary

Simplified and optimized v0.1 implementation with cleaner architecture while maintaining scalability.

### Results

| Metric | Lines | Tests |
|--------|-------|-------|
| **Total** | 1053 | 23 pass |
| context.rs | 61 | NEW |
| datastore.rs | 207 | |
| template.rs | 225 | |
| runner.rs | 419 | |
| workflow.rs | 95 | |
| use_block.rs | 46 | |

**Key improvements:**
- Single syntax: `{{use.alias}}` (legacy removed)
- Lock-free DataStore with DashMap
- Arc<Task> for zero-cost clone in spawn blocks
- TaskContext for inline resolution (no intermediate storage)
- Clippy clean with no warnings

## Decisions Made

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Remove legacy `{{ task.output }}` | Single syntax, single-pass resolution |
| 2 | Unify DataStore → 1 HashMap | No duplication, 1 lock, coherent API |
| 3 | `Arc<Task>` in Workflow | Zero-cost clone in spawn |
| 4 | Remove `UseEntry::Advanced` | YAGNI - Form 1+2 cover 99% |
| 5 | Inline resolution via `TaskContext` | No intermediate storage |

## New Architecture

```
Workflow → Runner → execute_task(task, ctx) → ctx.resolve("{{use.x}}")
                  → DataStore.insert(TaskResult)
```

### Core Types (3 structs)

```rust
// 1. TaskResult (replaces TaskData)
pub struct TaskResult {
    pub output: Value,           // Always JSON
    pub duration: Duration,
    pub status: TaskStatus,
}

pub enum TaskStatus {
    Success,
    Failed(String),
}

// 2. TaskContext (new)
pub struct TaskContext<'a> {
    task: &'a Task,
    store: &'a DataStore,
}

impl TaskContext<'_> {
    pub fn resolve(&self, template: &str) -> Result<String, NikaError>;
    pub fn get(&self, alias: &str) -> Option<Value>;
}

// 3. DataStore (simplified)
pub struct DataStore {
    results: DashMap<Arc<str>, TaskResult>,
}
```

### Modern Patterns

- **DashMap**: Lock-free concurrent HashMap
- **Arc<str>**: Interned task IDs, zero-cost clone
- **Cow<str>**: No allocation if no substitution
- **Single-pass tokenizer**: No String::replace() loops

## Implementation Phases

### Phase 1: Clean UseEntry (10 min)
- [ ] Remove `UseEntry::Advanced` variant
- [ ] Remove `UseAdvanced` struct
- [ ] Update tests
- [ ] ~40 lines removed

### Phase 2: Simplify DataStore (20 min)
- [ ] Create `TaskResult` with `output: Value`
- [ ] Create `TaskStatus` enum
- [ ] Replace 3 HashMaps with single `DashMap<Arc<str>, TaskResult>`
- [ ] Remove `get_output()`, `get_json_output()`, `set_output()`
- [ ] Keep `resolve_path()` for use block resolution
- [ ] Update tests
- [ ] ~180 lines removed

### Phase 3: Create TaskContext (15 min)
- [ ] New file `src/context.rs`
- [ ] `TaskContext` struct with task ref + store ref
- [ ] `resolve()` method - single-pass template resolution
- [ ] `get()` method - direct alias access
- [ ] Inline use block resolution (no pre-storage)
- [ ] Tests

### Phase 4: Simplify Template (20 min)
- [ ] Remove `LEGACY_RE` regex
- [ ] Remove `resolve()` function (legacy)
- [ ] Keep only `resolve_use()` → rename to `resolve()`
- [ ] Single-pass tokenizer (no String::replace loops)
- [ ] Use `Cow<str>` for zero-alloc when no templates
- [ ] Update tests
- [ ] ~140 lines removed

### Phase 5: Update Runner (25 min)
- [ ] Change `Workflow.tasks` to `Vec<Arc<Task>>`
- [ ] Remove `resolve_use_block()` function
- [ ] Use `TaskContext` in `execute_task()`
- [ ] Simplify spawn block (Arc clone instead of field clones)
- [ ] Update `TaskData` → `TaskResult` usage
- [ ] ~140 lines removed

### Phase 6: Update Workflow parsing (10 min)
- [ ] Parse into `Arc<Task>` directly
- [ ] Remove `use_block` field from Task (resolved inline)
- [ ] Keep `output` policy for format detection

### Phase 7: Final cleanup (10 min)
- [ ] Remove dead code
- [ ] Update lib.rs exports
- [ ] Run clippy
- [ ] Run all tests
- [ ] Update SPEC-v0.1.md if needed

## Expected Results

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| datastore.rs | 260 | ~80 | -180 |
| template.rs | 292 | ~150 | -142 |
| runner.rs | 439 | ~300 | -139 |
| use_block.rs | 81 | ~40 | -41 |
| **Total** | 1072 | ~570 | **-47%** |

## Files to Modify

1. `src/use_block.rs` - Remove Advanced
2. `src/datastore.rs` - Rewrite with DashMap
3. `src/context.rs` - NEW
4. `src/template.rs` - Single-pass, remove legacy
5. `src/runner.rs` - TaskContext, Arc<Task>
6. `src/workflow.rs` - Arc<Task>
7. `src/lib.rs` - Update exports
8. `src/main.rs` - Update if needed

## Risks

- **DashMap API differences**: May need adjustment
- **Lifetime issues with TaskContext**: May need Arc instead of refs
- **Test coverage**: Ensure all 46 tests still pass or are updated
