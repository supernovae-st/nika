# Nika v0.1 Refactoring Plan (R1-R4)

> Major architectural refactoring for cleaner, more modular, more scalable code.

## Executive Summary

| Phase | Focus | Files | Impact | Status |
|-------|-------|-------|--------|--------|
| R1 | Split runner.rs monolith | 5 | Critical | ⏳ |
| R2 | Consolidate wiring module | 4 | Medium | ⏳ |
| R3 | Extract event module | 3 | Medium | ⏳ |
| R4 | Cleanup & polish | 10+ | Low | ⏳ |

## Current State Analysis

```
20 files | 4,247 lines | 103 tests | 38 public exports

HOTSPOTS (files > 200 lines):
├── runner.rs         630 lines  (15%) ← MONOLITHIC 🔴
├── event_log.rs      484 lines  (11%)
├── use_bindings.rs   473 lines  (11%)
├── template.rs       425 lines  (10%)
├── task_executor.rs  322 lines  (8%)
└── jsonpath.rs       232 lines  (5%)
```

## Problems Identified

1. **MONOLITHIC runner.rs** (630 lines)
   - DAG execution loop
   - Event emission (scattered)
   - Output formatting (println!)
   - Schema validation
   - Test helpers

2. **NAMING CONFUSION**
   - `UseWiring` vs `UseBindings` (too similar)
   - `DataStore` vs `EventLog` (inconsistent)
   - `TaskExecutor` vs `Runner` (overlapping)

3. **SCATTERED RESPONSIBILITIES**
   - Event emission in 3 files
   - Output handling mixed with execution
   - Validation scattered

4. **DEAD_CODE SMELL**
   - 11x `#[allow(dead_code)]` annotations

---

## R1: Split runner.rs Monolith

### Goal
Split 630-line runner.rs into focused modules (~150 lines each).

### Target Structure
```
src/runner/
├── mod.rs          (public API: Runner struct)
├── scheduler.rs    (DAG loop: get_ready_tasks, all_done)
├── execution.rs    (task spawning, permit handling)
└── output.rs       (console formatting, make_task_result)
```

### Tasks

#### R1.1: Create runner/mod.rs
```rust
//! DAG workflow runner (v0.1)
//!
//! Split into focused submodules:
//! - scheduler: DAG traversal and task ordering
//! - execution: Task spawning with semaphore
//! - output: Console formatting and result handling

mod execution;
mod output;
mod scheduler;

pub use self::execution::Runner;
```

#### R1.2: Extract scheduler.rs (~100 lines)
Move from runner.rs:
- `get_ready_tasks()` method
- `all_done()` method
- `get_final_output()` method
- DAG-related logic

#### R1.3: Extract execution.rs (~200 lines)
Move from runner.rs:
- `Runner` struct
- `Runner::new()`, `with_max_concurrent()`
- `run()` method (main loop)
- Semaphore handling

#### R1.4: Extract output.rs (~100 lines)
Move from runner.rs:
- `make_task_result()` function
- `validate_schema()` function
- Console output formatting (println! calls)

#### R1.5: Update imports
- Update `lib.rs` and `main.rs` to use new paths
- Verify all 103 tests pass

### Success Criteria
- [ ] runner.rs → 4 files total
- [ ] Each file < 250 lines
- [ ] All 103 tests pass
- [ ] No new warnings

---

## R2: Consolidate Wiring Module

### Goal
Clarify naming and group related code.

### Renames
| Current | New | Reason |
|---------|-----|--------|
| `UseWiring` | `WiringSpec` | YAML declaration (spec) |
| `UseBindings` | `ResolvedInputs` | Runtime values (resolved) |
| `UseEntry` | `WiringEntry` | Consistency |
| `UseAdvanced` | `AdvancedWiring` | Consistency |

### Target Structure
```
src/wiring/
├── mod.rs          (public API)
├── spec.rs         (WiringSpec, WiringEntry - YAML types)
├── resolver.rs     (ResolvedInputs - runtime resolution)
└── validator.rs    (DAG validation - from validator.rs)
```

### Tasks

#### R2.1: Create wiring/spec.rs
Move from `use_wiring.rs`:
- `UseWiring` → `WiringSpec`
- `UseEntry` → `WiringEntry`
- `UseAdvanced` → `AdvancedWiring`

#### R2.2: Create wiring/resolver.rs
Move from `use_bindings.rs`:
- `UseBindings` → `ResolvedInputs`
- `from_use_wiring()` → `from_spec()`

#### R2.3: Move validator.rs to wiring/
Rename: `validator.rs` → `wiring/validator.rs`

#### R2.4: Update all imports
- Update all files using `UseWiring`, `UseBindings`
- Update tests

### Success Criteria
- [ ] Clear naming distinction (Spec vs Resolved)
- [ ] All related code in one module
- [ ] All 103 tests pass

---

## R3: Extract Event Module

### Goal
Group event-related code and reduce scattered emission.

### Target Structure
```
src/event/
├── mod.rs          (public API)
├── types.rs        (Event, EventKind)
├── log.rs          (EventLog)
└── emitter.rs      (EventEmitter trait/helper)
```

### Tasks

#### R3.1: Create event/types.rs
Move from `event_log.rs`:
- `Event` struct
- `EventKind` enum

#### R3.2: Create event/log.rs
Move from `event_log.rs`:
- `EventLog` struct
- All methods

#### R3.3: Create event/emitter.rs
New helper trait:
```rust
pub trait EventEmitter {
    fn emit(&self, kind: EventKind) -> u64;
}

impl EventEmitter for EventLog { ... }
```

#### R3.4: Update event emission in runner/executor
- Use EventEmitter trait for cleaner emission
- Reduce code duplication

### Success Criteria
- [ ] Event code grouped in one module
- [ ] Cleaner emission API
- [ ] All 103 tests pass

---

## R4: Cleanup & Polish

### Goal
Remove dead code, fix warnings, final polish.

### Tasks

#### R4.1: Remove #[allow(dead_code)]
Audit all 11 occurrences:
- `flow_graph.rs:31` - task_set field
- `flow_graph.rs:99` - get_successors
- `flow_graph.rs:127` - contains
- `interner.rs:68` - len
- `interner.rs:75` - is_empty
- `interner.rs:94` - intern_arc
- `runner.rs:48` - with_max_concurrent
- `runner.rs:67` - event_log
- `template.rs:193` - extract_refs
- `template.rs:209` - validate_refs
- `use_bindings.rs:135` - is_empty

Decision for each:
- DELETE if truly unused
- KEEP without annotation if used in tests
- ADD test if should be tested

#### R4.2: Unify naming conventions
- All types: PascalCase
- All fields: snake_case
- All modules: snake_case
- Verify consistency

#### R4.3: Documentation cleanup
- Ensure all public items have doc comments
- Remove outdated comments
- Update module-level docs

#### R4.4: Final verification
- `cargo clippy -- -D warnings`
- `cargo test` (103 tests)
- `cargo doc --no-deps` (no warnings)

### Success Criteria
- [ ] 0 `#[allow(dead_code)]` annotations
- [ ] 0 clippy warnings
- [ ] 0 doc warnings
- [ ] All 103 tests pass

---

## Execution Order

```
R1 (runner.rs split) ──┐
                       ├──► R4 (cleanup)
R2 (wiring module) ────┤
                       │
R3 (event module) ─────┘
```

R1, R2, R3 can be done in parallel.
R4 must be last.

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking tests | Run tests after each step |
| Import errors | Use `cargo check` frequently |
| Missing re-exports | Verify lib.rs exports |
| Performance regression | Benchmark before/after |

---

## Rollback Plan

If issues arise:
1. `git stash` current changes
2. Return to last working commit
3. Fix issue in isolation
4. Reapply changes

All changes will be committed atomically per phase.
