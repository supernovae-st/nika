# Session C: Silent Failure Sweep + Events (~3-4h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
Master plan: `docs/plans/2026-03-28-v051-master-quality-plan.md` — READ PARTS 1+6 FIRST.

## Mission: Fix 15 silent failures + implement TaskEventGuard

---

### Part 1: TaskEventGuard Pattern (~45min)

Create a guard that GUARANTEES event emission. If dropped without `.complete()` or `.fail()`, it emits TaskFailed automatically.

**Create**: `nika-engine/src/runtime/event_guard.rs`

```rust
pub struct TaskEventGuard {
    task_id: Arc<str>,
    event_log: EventLog,
    completed: bool,
}

impl TaskEventGuard {
    pub fn start(event_log: EventLog, task_id: Arc<str>) -> Self {
        // Emit TaskStarted
        Self { task_id, event_log, completed: false }
    }
    pub fn complete(mut self, result: &Value, duration: Duration) { self.completed = true; /* emit TaskCompleted */ }
    pub fn fail(mut self, error: &str, duration: Duration) { self.completed = true; /* emit TaskFailed */ }
}

impl Drop for TaskEventGuard {
    fn drop(&mut self) {
        if !self.completed {
            tracing::error!(task_id = %self.task_id, "TaskEventGuard dropped without completion");
            // emit TaskFailed with "internal: guard dropped without completion"
        }
    }
}
```

**Test**: Create guard, drop without completing → verify TaskFailed emitted.
**Test**: Create guard, call .complete() → verify TaskCompleted emitted, no TaskFailed.

### Part 2: Fix Missing Events (~1h)

| # | Bug | File | Fix |
|---|-----|------|-----|
| 1 | SF2: Missing ProviderResponded on Layer 0a no-spec path | `executor/infer.rs:523-538` | Add event before `return Ok(...)` |
| 2 | SF3: for_each binding failure = no TaskFailed | `runner.rs:1800-1809` | Add `event_log.emit(TaskFailed { ... })` before `continue` |
| 3 | SF4: for_each "items not resolved" = no TaskFailed | `runner.rs:2246-2261` | Same fix |
| 4 | EV2: Chat path never emits ProviderResponded | `rig_agent_loop/chat.rs` | Add ProviderResponded after each turn with tokens + cost |
| 5 | EV5: MCP disconnect/reconnect = no events | `nika-mcp/src/client.rs:758,798` | Add EventKind variants or at minimum tracing::info! |

### Part 3: Fix Wrong Event Data (~45min)

| # | Bug | File | Fix |
|---|-----|------|-----|
| 1 | EV1: ContextAssembled hardcoded zeros | `executor/infer.rs:142-149` | Either populate `budget_used_pct`, `truncated`, `excluded` or remove the fields |
| 2 | EV6+7: Structured output retry uses estimated tokens | `structured_output.rs:526,690` | Extract actual tokens from provider response |
| 3 | EV8: Non-streaming fallback estimated tokens | `infer.rs:638,707` | Use provider response tokens if available |

### Part 4: Fix Silent Error Swallowing (~1h)

| # | Bug | File | Fix |
|---|-----|------|-----|
| 1 | SF6: EventLog drops trace writes with `let _ =` | `nika-event/src/log.rs:1042` | `if let Err(e) = writer.append_event(&event) { tracing::warn!(...) }` |
| 2 | SF7: Daemon job state dropped | `nika-daemon/src/services/jobs.rs:215-241` | Same pattern |
| 3 | SF8: debug! for errors that should be warn! | Multiple files (see audit) | Upgrade in policy.rs, run_context.rs, decompose.rs |
| 4 | CR1: SchemaGuardrail only checks required | `nika-core/src/ast/guardrails.rs:332` | Use `jsonschema::validator_for()` for FULL validation |

### Part 5: Fix Remaining v0.51 Bugs (~45min)

| # | Bug | File | Fix |
|---|-----|------|-----|
| 1 | M-orig7: extract: llm_txt raw HTML fallback | `executor/fetch.rs:554-607` | Return ExtractError instead of HTML |
| 2 | M-orig2: routing: dead code | Parser in nika-core | Emit warning or remove field |
| 3 | M-orig4: fetch: short form schema | Schema validator | Add string variant |
| 4 | M-orig5: format: markdown schema | Schema validator | Add to enum |

---

## After All Fixes
1. `cargo test --workspace --lib` — expect 8650+
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. `git push`
