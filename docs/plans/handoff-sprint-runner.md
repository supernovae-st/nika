# Handoff: Runner Sprint (~6h)

> Copy this file as first message in a new Claude Code session.

## Context
Runner is solid (DAG scheduling, cancellation, for_each all work) but has performance and edge case issues.

## Codebase
```
cd /Users/thibaut/dev/supernovae/nika
cargo test --workspace --lib  # 9057 pass
```

## Items

### 1. Cancellation in binding resolution (~1h)
**File:** `nika-engine/src/runtime/runner.rs:1965-1993`
**Bug:** Path traversal during binding resolution doesn't check `cancel_token.is_cancelled()`. A cancelled workflow can spend time resolving deep JSON paths.
**Fix:** Add `if cancel.is_cancelled() { return Err(...) }` in the path traversal loop.

### 2. Binding from failed task warning (~30min)
**File:** `nika-engine/src/binding/resolve.rs`
**Bug:** When a task binds to `$failed_task.field`, the binding silently returns the error message string. No warning logged.
**Fix:** In `resolve_with_entry_traced`, check if source task's `TaskOutcome` is `Failed`. If so, `tracing::warn!("Binding $alias from failed task — value may be error message")`.

### 3. EventLog O(n) drain → ring buffer (~2h)
**File:** `nika-event/src/log.rs:1186`
**Bug:** `drain()` copies all events. For workflows with 10K+ events (heavy for_each), this is slow.
**Fix:** Replace `Vec<Event>` with a ring buffer (VecDeque) with capacity. Use `drain(..)` on the deque.

### 4. Template resolution unbounded allocations (~1h)
**File:** `nika-engine/src/binding/template.rs:370`
**Bug:** Each `{{...}}` resolution allocates a new String. For templates with many variables, this creates allocation pressure.
**Fix:** Pre-allocate `String::with_capacity(template.len() * 2)` for the result buffer.

### 5. Circular with: bindings indirect detection (~2h)
**File:** `nika-core/src/ast/analyzer/validate.rs:184`
**Bug:** Circular binding detection only checks direct self-reference (`$self.field`). Indirect cycles (`A→B→A`) are not caught.
**Fix:** Build a binding dependency graph and run cycle detection (DFS with coloring).

## Verification
```bash
cargo test --workspace --lib
./tools/target/debug/nika run tests/e2e-overnight/D02-diamond.nika.yaml --no-live
./tools/target/debug/nika run tests/e2e-overnight/S02-for-each-100.nika.yaml --no-live
```
