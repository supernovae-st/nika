# TUI Improvements v4 — Performance & Coverage

> **For Claude:** Reference plan for future sessions. Findings from 3-agent deep scan after v3 cleanup.

**Baseline:** 2153 tests, 0 clippy warnings, v3 plan fully executed.

---

## FIXED in this session (Phase 2)

- **task.rs:43 phase clobbering** — `on_task_started` else clause clobbered Pause/Abort (CRITICAL, fixed)
- **on_context_assembled** — zero test coverage (now tested)
- **on_template_resolved** — zero test coverage (now tested, including cap-at-10)

---

## Remaining Findings

### HIGH — Performance: JSON clones in event handlers

| File:Line | Type cloned | Impact |
|---|---|---|
| `provider.rs:158` | `Option<serde_json::Value>` (MCP params) | Per MCP invocation |
| `provider.rs:206` | `Option<serde_json::Value>` (MCP response) | Per MCP response |
| `task.rs:34` | `serde_json::Value` (task inputs) | Per task start |

**Fix:** Wrap large values in `Arc<Value>` at the event source, pass `Arc` through to TUI state. This is an engine-level change affecting `EventKind`.

### HIGH — Performance: DAG deps cloned per render

| File:Line | Type cloned | Impact |
|---|---|---|
| `monitor/render_dag.rs:89` | `FxHashMap<String, Vec<String>>` | Per frame (~60fps) |
| `monitor/mod.rs:154,171` | `String` task_id, `Vec<String>` deps | Per frame cache rebuild |

**Fix:** Cache DAG layout, invalidate only on task status change (already partially implemented via dirty flags).

### MEDIUM — format!() in render paths

~20 instances of `format!()` in render methods (render_editor, render_dag, render_mission, monitor). Each allocates per frame.

**Fix:** Pre-build formatted strings in state, invalidate on value change. Low priority — terminal rendering is already fast.

### MEDIUM — String allocations in event handlers

~15 instances of `to_string()` in event handlers for task_id, verb, error_message etc. Could use `Arc<str>` for shared strings.

**Fix:** Pass event data as `Arc<str>` from engine. Requires engine-level refactor.

### LOW — Monitor view Block widget clones

`Block::default()` clones in render_output.rs, render_reasoning.rs. Minimal cost.

---

## Event Handler Coverage (post-fix)

| Status | Count |
|---|---|
| Tested | 26/26 (100%) |
| Total handlers | 26 |

All event handlers now have at least one dedicated test.

---

## Recommended Priority for Next Session

1. **Arc<Value> refactor** — highest perf impact, requires engine + TUI change
2. **DAG render cache** — reduce per-frame allocations in monitor view
3. **String interning** — `Arc<str>` for task_id, verb across event pipeline
