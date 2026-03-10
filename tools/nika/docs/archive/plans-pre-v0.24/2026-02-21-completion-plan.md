# Plan: Nika v0.6.1 - Completion to 100%

## Critical Bugs Found

### BUG 1: spawn_agent Depth Calculation (CRITICAL)

**File:** `src/runtime/spawn.rs:216`

**Problem:**
```rust
let remaining_depth = self.max_depth.saturating_sub(child_depth);
```

With `depth_limit=3`:
- Root (depth 1) → spawns child with `remaining_depth = 3-2 = 1`
- Child RigAgentLoop sets `current_depth = 1`, `max_depth = 1`
- Condition `1 < 1` = false → Child CANNOT spawn grandchildren

**Expected:** 3 levels of nesting (root, child, grandchild)
**Actual:** Only 2 levels work

**Fix:** Pass same `max_depth` to children, track actual depth differently

### BUG 2: TraceWriter Only On Success (CRITICAL)

**File:** `src/runtime/runner.rs:741-747`

**Problem:** Trace writing is ONLY after `WorkflowCompleted`. If workflow fails:
- Line 414: `WorkflowFailed` → `return Err(...)` → NO TRACE
- Line 621: `WorkflowAborted` → `return Err(...)` → NO TRACE

**Fix:** Move trace writing to a `finally` block or write before returning errors

---

## Medium Priority

### FIX 3: Wire validate_refs to `nika check`

**Files:** `src/binding/template.rs:235-249`, `src/dag/validator.rs` or CLI

**Problem:** `validate_refs()` exists but never called - catches template typos
**Fix:** Add static validation during `nika check --strict`

### FIX 4: Delete Dead Code

**DELETE:**
- `src/tui/watcher.rs` (225 lines) - FileWatcher never used
- `src/tui/file_resolve.rs:223-233` - to_utf8_path utilities
- `src/tui/app.rs:187` - standalone_state field

**WIRE:**
- `src/tui/views/studio.rs:353-359` - current_line/col to status bar

---

## Implementation Order

| # | Fix | Risk | Effort |
|---|-----|------|--------|
| 1 | spawn_agent depth bug | HIGH | 30 min |
| 2 | TraceWriter on failure | HIGH | 20 min |
| 3 | Wire validate_refs | MEDIUM | 30 min |
| 4 | Delete dead code | LOW | 15 min |

**Total:** ~1h30
