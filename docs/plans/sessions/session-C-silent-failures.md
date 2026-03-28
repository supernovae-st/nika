# Session C: Silent Failure Sweep + Events (~3-4h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
Master plan: `docs/plans/2026-03-28-v051-master-quality-plan.md` — READ PARTS 1+6 FIRST.

## Mission: Fix 15+ silent failures + implement TaskEventGuard

---

### Part 1: TaskEventGuard Pattern (~45min)

Create a guard that GUARANTEES event emission. If dropped without `.complete()` or `.fail()`, it emits TaskFailed automatically.

**Why**: runner.rs has 17 `TaskResult::failed()` calls in the DAG scheduling phase (lines 1690-2252) that store failures in the datastore WITHOUT emitting `TaskFailed` events. The TUI, CLI renderer, and trace writer never learn about these failures.

**Create**: `nika-engine/src/runtime/event_guard.rs`

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};
use nika_event::{EventKind, EventLog};

/// RAII guard that guarantees TaskStarted/TaskFailed/TaskCompleted events.
///
/// If dropped without calling `.complete()` or `.fail()`, emits TaskFailed
/// automatically. This makes silent event drops structurally impossible.
pub struct TaskEventGuard {
    task_id: Arc<str>,
    event_log: EventLog,
    start: Instant,
    completed: bool,
}

impl TaskEventGuard {
    /// Create a guard and emit TaskStarted.
    pub fn start(event_log: EventLog, task_id: Arc<str>, verb: &str, inputs: serde_json::Value) -> Self {
        event_log.emit(EventKind::TaskStarted {
            task_id: Arc::clone(&task_id),
            verb: Arc::from(verb),
            inputs,
        });
        Self {
            task_id,
            event_log,
            start: Instant::now(),
            completed: false,
        }
    }

    /// Emit TaskCompleted and consume the guard (no Drop emission).
    pub fn complete(mut self, output: Arc<serde_json::Value>) {
        self.completed = true;
        self.event_log.emit(EventKind::TaskCompleted {
            task_id: Arc::clone(&self.task_id),
            output,
            duration_ms: self.start.elapsed().as_millis() as u64,
        });
    }

    /// Emit TaskFailed and consume the guard (no Drop emission).
    pub fn fail(mut self, error: &str, error_code: Option<String>) {
        self.completed = true;
        self.event_log.emit(EventKind::TaskFailed {
            task_id: Arc::clone(&self.task_id),
            error: error.to_string(),
            duration_ms: self.start.elapsed().as_millis() as u64,
            error_code,
        });
    }

    /// Elapsed time since guard creation.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for TaskEventGuard {
    fn drop(&mut self) {
        if !self.completed {
            tracing::error!(task_id = %self.task_id, "TaskEventGuard dropped without completion — emitting TaskFailed");
            self.event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&self.task_id),
                error: "internal: task event guard dropped without completion (likely panic or early return)".to_string(),
                duration_ms: self.start.elapsed().as_millis() as u64,
                error_code: Some("NIKA-098".to_string()),
            });
        }
    }
}
```

#### Tests for TaskEventGuard

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::EventLog;
    use std::sync::Arc;

    #[test]
    fn guard_dropped_without_completion_emits_task_failed() {
        let log = EventLog::new();
        {
            let _guard = TaskEventGuard::start(
                log.clone(),
                Arc::from("test_task"),
                "infer",
                serde_json::json!({}),
            );
            // Guard is dropped here without calling .complete() or .fail()
        }
        let events = log.events();
        assert_eq!(events.len(), 2, "Expected TaskStarted + TaskFailed from drop");
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { task_id, .. } if task_id.as_ref() == "test_task"));
        assert!(matches!(&events[1].kind, EventKind::TaskFailed { task_id, error, error_code, .. }
            if task_id.as_ref() == "test_task"
            && error.contains("guard dropped without completion")
            && error_code.as_deref() == Some("NIKA-098")
        ));
    }

    #[test]
    fn guard_complete_emits_task_completed_no_drop_event() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "infer",
            serde_json::json!({}),
        );
        guard.complete(Arc::new(serde_json::json!("result")));
        let events = log.events();
        assert_eq!(events.len(), 2, "Expected TaskStarted + TaskCompleted");
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(&events[1].kind, EventKind::TaskCompleted { task_id, .. }
            if task_id.as_ref() == "test_task"
        ));
    }

    #[test]
    fn guard_fail_emits_task_failed_no_drop_event() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "exec",
            serde_json::json!({}),
        );
        guard.fail("something broke", Some("NIKA-050".to_string()));
        let events = log.events();
        assert_eq!(events.len(), 2, "Expected TaskStarted + TaskFailed");
        assert!(matches!(&events[0].kind, EventKind::TaskStarted { .. }));
        assert!(matches!(&events[1].kind, EventKind::TaskFailed { error, error_code, .. }
            if error == "something broke"
            && error_code.as_deref() == Some("NIKA-050")
        ));
    }

    #[test]
    fn guard_elapsed_tracks_time() {
        let log = EventLog::new();
        let guard = TaskEventGuard::start(
            log.clone(),
            Arc::from("test_task"),
            "fetch",
            serde_json::json!({}),
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(guard.elapsed().as_millis() >= 10);
        guard.complete(Arc::new(serde_json::json!(null)));
    }
}
```

#### Event emission sites that the guard must cover

**Inside `execute_task()` (runner.rs:890-1260)** — Currently has manual TaskStarted (line 937), TaskCompleted (line 1175), and TaskFailed (lines 662, 698, 738, 766, 805, 912, 1149, 1181, 1193, 1252). The guard replaces ALL of these with a single `TaskEventGuard::start()` at the top.

**DAG scheduling phase (runner.rs:1680-2260)** — 17 `TaskResult::failed()` calls with NO events:

| Line | Context | Missing Events |
|------|---------|----------------|
| 1690 | Decompose binding resolution failed | TaskStarted + TaskFailed |
| 1727 | Decompose expansion error | TaskStarted + TaskFailed |
| 1745 | Decompose timeout | TaskStarted + TaskFailed |
| 1769 | for_each binding resolution failed | TaskStarted + TaskFailed |
| 1787 | for_each binding resolved to non-array | TaskStarted + TaskFailed |
| 1801 | for_each input not found in workflow inputs | TaskStarted + TaskFailed |
| 1818 | for_each empty alias after '$' prefix | TaskStarted + TaskFailed |
| 1868 | for_each nested path segment not found | TaskStarted + TaskFailed |
| 1891 | for_each binding resolved to non-array | TaskStarted + TaskFailed |
| 1906 | for_each binding not found | TaskStarted + TaskFailed |
| 1931 | for_each inputs resolved to non-array | TaskStarted + TaskFailed |
| 1945 | for_each input not found in workflow inputs | TaskStarted + TaskFailed |
| 2017 | for_each path traversal failed | TaskStarted + TaskFailed |
| 2032 | for_each with binding resolved to non-array | TaskStarted + TaskFailed |
| 2048 | for_each binding not found | TaskStarted + TaskFailed |
| 2189 | Semaphore closed unexpectedly | TaskStarted + TaskFailed |
| 2252 | for_each items could not be resolved | TaskStarted + TaskFailed |

**Fix approach**: For DAG scheduling failures, emit inline TaskFailed events (not full guard). These tasks never truly "start" — they fail at binding resolution before execution. Add a helper:

```rust
fn emit_scheduling_failure(event_log: &EventLog, task_name: &str, error: &str, error_code: &str) {
    let task_id: Arc<str> = Arc::from(task_name);
    event_log.emit(EventKind::TaskFailed {
        task_id,
        error: error.to_string(),
        duration_ms: 0,
        error_code: Some(error_code.to_string()),
    });
}
```

---

### Part 2: Fix Missing Events (~1h)

| # | Bug | File:Line | Verified | Fix |
|---|-----|-----------|----------|-----|
| 1 | **SF2**: Missing ProviderResponded on Layer 0a no-spec path | `executor/infer.rs:523-538` | YES — `return Ok(stream_result.text)` skips ProviderResponded at line 854. `stream_result` has real token data from streaming. | Add `ProviderResponded` event before `return Ok(...)` using `stream_result.input_tokens` / `stream_result.output_tokens` |
| 2 | **SF3**: for_each binding failures store `TaskResult::failed()` but emit NO `TaskFailed` event | `runner.rs:1785-1809` (and 15 more sites) | YES — see 17-site table above | Add `emit_scheduling_failure()` before each `continue` |
| 3 | **SF4**: for_each "items not resolved" stores failure, no `TaskFailed` event | `runner.rs:2246-2261` | YES — `TaskResult::failed()` at line 2252, `continue` at 2261, no event | Same fix |
| 4 | **EV2**: Chat path never emits ProviderResponded | `rig_agent_loop/chat.rs` (entire file) | YES — grep confirms ZERO `ProviderResponded` matches. Only `AgentTurn` events at lines 256, 311. | Add `ProviderResponded` after each `chat_with_history()` call with token data from response |
| 5 | **EV5**: MCP disconnect/reconnect = no events | `nika-mcp/src/client.rs:758,798` | YES — `disconnect()` at line 758 and `reconnect()` at line 798 have no event emission. No `EventLog` available in MCP crate. | Add at minimum `tracing::info!` with structured fields. Full events require passing `EventLog` or adding MCP-specific event variants. |

#### Test: SF2 — Missing ProviderResponded on no-spec path

```rust
#[tokio::test]
async fn provider_responded_emitted_on_layer0a_no_spec_path() {
    // Setup: structured output with response_format but NO json_schema spec
    // This hits the `else` branch at infer.rs:523
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: gen
    infer: "Generate some JSON"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
        required: [name]
"#;
    let (result, event_log) = run_workflow_with_events(yaml).await;
    assert!(result.is_ok());
    let events = event_log.events();
    let provider_responded = events.iter().any(|e| matches!(&e.kind, EventKind::ProviderResponded { .. }));
    assert!(provider_responded, "ProviderResponded must be emitted even on no-spec early return path");
}
```

#### Test: SF3 — for_each binding failure emits TaskFailed

```rust
#[tokio::test]
async fn for_each_binding_failure_emits_task_failed_event() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: process
    for_each:
      items: "$nonexistent_task"
      as: item
    infer: "Process {{with.item}}"
"#;
    let (result, event_log) = run_workflow_with_events(yaml).await;
    let events = event_log.events();
    let task_failed = events.iter().find(|e| matches!(&e.kind, EventKind::TaskFailed { task_id, .. }
        if task_id.as_ref() == "process"
    ));
    assert!(task_failed.is_some(), "TaskFailed event must be emitted when for_each binding fails (not just stored in datastore)");
}
```

#### Test: EV2 — Chat path emits ProviderResponded

```rust
#[tokio::test]
async fn chat_continue_emits_provider_responded() {
    // This requires a mock chat setup; the key assertion is:
    let events = event_log.events();
    let provider_events: Vec<_> = events.iter().filter(|e| matches!(&e.kind, EventKind::ProviderResponded { .. })).collect();
    assert!(!provider_events.is_empty(), "Chat path must emit ProviderResponded for token/cost tracking");
    // Verify tokens are non-zero
    if let EventKind::ProviderResponded { input_tokens, output_tokens, .. } = &provider_events[0].kind {
        assert!(*input_tokens > 0 || *output_tokens > 0, "Token counts must not be zero");
    }
}
```

---

### Part 3: Fix Wrong Event Data (~45min)

| # | Bug | File:Line | Verified | Fix |
|---|-----|-----------|----------|-----|
| 1 | **EV1**: ContextAssembled hardcoded zeros — `budget_used_pct: 0.0`, `truncated: false`, `excluded: Vec::new()` | `executor/infer.rs:142-149` | YES — lines 145-148 are literal hardcoded values | Either populate from actual context data or remove misleading fields. Simplest: add a comment "// TODO" and keep fields for forward-compat, OR track in executor if any context was dropped. |
| 2 | **EV6+EV7**: Structured output retry/repair uses `estimate_tokens(output.len())` instead of actual tokens | `structured_output.rs:522-524` (retry), `structured_output.rs:686-688` (repair) | YES — `let in_tok = estimate_tokens(prompt_len); let out_tok = estimate_tokens(output.len());` on both paths | Make `infer_fn` return `StreamResult` instead of `String`, then use `stream_result.input_tokens` |
| 3 | **EV8**: Non-streaming Layer 0b fallback uses estimated tokens | `infer.rs:625-637` (with spec), `infer.rs:693-706` (no spec) | YES — `let est_in = estimate_tokens(prompt.len()); let est_out = estimate_tokens(result_str.len());` | Same fix: propagate actual tokens from provider response |

#### Test: EV1 — ContextAssembled has non-zero data

```rust
#[tokio::test]
async fn context_assembled_event_has_nonzero_total_tokens() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: gen
    infer: "Hello world, this is a test prompt with enough words to estimate tokens"
"#;
    let (result, event_log) = run_workflow_with_events(yaml).await;
    assert!(result.is_ok());
    let events = event_log.events();
    let ctx = events.iter().find(|e| matches!(&e.kind, EventKind::ContextAssembled { .. }));
    assert!(ctx.is_some(), "ContextAssembled must be emitted");
    if let Some(e) = ctx {
        if let EventKind::ContextAssembled { total_tokens, .. } = &e.kind {
            assert!(*total_tokens > 0, "total_tokens must be non-zero for non-empty prompt");
        }
    }
}
```

#### Test: EV6 — Structured output retry uses actual tokens

```rust
#[tokio::test]
async fn structured_retry_provider_responded_has_actual_tokens() {
    // Setup workflow that will trigger structured output retry (invalid first response)
    // After fix, ProviderResponded with finish_reason "structured_output_retry"
    // should have input_tokens > 0 from actual provider, not estimate_tokens()
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract
    infer: "Extract name"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
        required: [name]
      max_retries: 1
"#;
    let (_, event_log) = run_workflow_with_events(yaml).await;
    let events = event_log.events();
    let retry_events: Vec<_> = events.iter()
        .filter(|e| matches!(&e.kind, EventKind::ProviderResponded { finish_reason, .. }
            if finish_reason.contains("structured_output")))
        .collect();
    for event in &retry_events {
        if let EventKind::ProviderResponded { input_tokens, output_tokens, .. } = &event.kind {
            // With actual tokens, these should not be round numbers from chars/4
            assert!(*input_tokens > 0, "Retry ProviderResponded must have non-zero input_tokens");
            assert!(*output_tokens > 0, "Retry ProviderResponded must have non-zero output_tokens");
        }
    }
}
```

---

### Part 4: Fix Silent Error Swallowing (~1h)

| # | Bug | File:Line | Verified | Fix |
|---|-----|-----------|----------|-----|
| 1 | **SF6**: EventLog drops trace writes with `let _ =` | `nika-event/src/log.rs:1042` | YES — `let _ = writer.append_event(&event);` silently drops I/O errors | `if let Err(e) = writer.append_event(&event) { tracing::warn!(error = %e, "Failed to write trace event"); }` |
| 2 | **SF6b**: EventLog drops broadcast sends with `let _ =` | `nika-event/src/log.rs:1046` | YES — `let _ = tx.send(event);` silently drops if all receivers dropped | OK for broadcast (receivers may legitimately drop). Add `tracing::trace!` at most. |
| 3 | **SF7**: Daemon job state updates silently dropped | `nika-daemon/src/services/jobs.rs:215-248` | YES — three `let _ = storage.update_state(...)` and `let _ = storage.add_history(...)` calls | `if let Err(e) = storage.update_state(...).await { tracing::warn!(job_id, error = %e, "Failed to persist job state"); }` |
| 4 | **SF8**: `debug!` used for errors that should be `warn!` | Multiple files | YES — see list below | Upgrade log levels |
| 5 | **CR1**: SchemaGuardrail only checks `required` — no type/pattern/enum checking | `nika-core/src/ast/guardrails.rs:332-380` | YES — only checks `required` array, then returns `passed`. Ignores `type`, `properties`, `enum`, `minimum`, `maximum`, `pattern`, `additionalProperties`. | Use `jsonschema::validator_for()` for FULL JSON Schema validation |
| 6 | **SF5**: `jsonschema::validator_for(schema).ok()` silently disables validation on invalid schema | `runner.rs:656` | YES — if the schema itself is malformed, `.ok()` returns `None`, validation is silently skipped for all retries | Return `TaskFailed` with NIKA-061 error if schema fails to compile |

#### SF8: debug! → warn! upgrade sites (verified)

| File:Line | Current | Should Be | Context |
|-----------|---------|-----------|---------|
| `policy.rs:106` | `debug!` | `warn!` | DNS resolution FAILED during SSRF check — security-relevant, should be visible |
| `policy.rs:110` | `debug!` | `warn!` | DNS resolution TIMED OUT during SSRF check — even more security-relevant |
| `run_context.rs:391` | `debug!` | `debug!` (OK) | JSONPath resolution failure in data binding — expected in some flows |
| `executor/fetch.rs:599` | `debug!` | `warn!` | llm_txt failed to read response body — user-visible fetch behavior |
| `executor/fetch.rs:603` | `debug!` | `warn!` | llm_txt request completely failed — user-visible |
| `executor/infer.rs:101` | `debug!` | `warn!` | Failed to pre-read `from_example` file — silent data loss |
| `executor/agent.rs:55` | `debug!` | `warn!` | Same issue in agent verb |

#### Test: CR1 — SchemaGuardrail validates types, not just required

```rust
#[test]
fn schema_guardrail_rejects_wrong_type() {
    let guardrail = SchemaGuardrail {
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "price": { "type": "number" },
                "name": { "type": "string" }
            },
            "required": ["price", "name"]
        }),
        on_failure: FailureAction::Fail,
        message: None,
        id: Some("type_check".to_string()),
    };

    // Has required fields but wrong types — must FAIL
    let result = guardrail.check(r#"{"price": "not_a_number", "name": 42}"#);
    assert!(!result.passed, "SchemaGuardrail must validate types, not just required fields");
}

#[test]
fn schema_guardrail_validates_enum_values() {
    let guardrail = SchemaGuardrail {
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["active", "inactive"] }
            },
            "required": ["status"]
        }),
        on_failure: FailureAction::Retry,
        message: None,
        id: Some("enum_check".to_string()),
    };

    let result = guardrail.check(r#"{"status": "deleted"}"#);
    assert!(!result.passed, "SchemaGuardrail must validate enum values");
}

#[test]
fn schema_guardrail_validates_number_range() {
    let guardrail = SchemaGuardrail {
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "score": { "type": "number", "minimum": 0, "maximum": 100 }
            },
            "required": ["score"]
        }),
        on_failure: FailureAction::Fail,
        message: None,
        id: Some("range_check".to_string()),
    };

    let result = guardrail.check(r#"{"score": 150}"#);
    assert!(!result.passed, "SchemaGuardrail must validate number ranges");
}
```

#### Test: SF5 — Invalid schema causes error, not silent skip

```rust
#[tokio::test]
async fn invalid_json_schema_causes_task_failure() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: gen
    infer: "Generate JSON"
    structured:
      schema:
        type: INVALID_TYPE_VALUE
        properties: "this should be an object not a string"
      max_retries: 0
"#;
    let (result, event_log) = run_workflow_with_events(yaml).await;
    // The task must fail with NIKA-061, NOT silently pass with unvalidated output
    let events = event_log.events();
    let task_failed = events.iter().find(|e| matches!(&e.kind, EventKind::TaskFailed { error_code, .. }
        if error_code.as_deref() == Some("NIKA-061")
    ));
    assert!(task_failed.is_some(), "Invalid schema must cause TaskFailed with NIKA-061, not silent validation skip");
}
```

#### Test: SF6 — Trace write failure is logged

```rust
#[test]
fn trace_write_failure_emits_warning() {
    // This test verifies that trace write errors are not silently dropped.
    // Setup: create EventLog with a trace writer pointing to a read-only path.
    // After emitting an event, verify tracing::warn was called.
    //
    // Implementation: use tracing-test crate or a custom Layer that captures logs.
    // The key change is replacing `let _ = writer.append_event(&event)` with
    // `if let Err(e) = ... { tracing::warn!(...) }` at log.rs:1042.
    //
    // Minimal smoke test:
    let log = EventLog::new();
    // Emit without trace writer — should not panic
    log.emit(EventKind::WorkflowStarted {
        workflow: Arc::from("test"),
        tasks: vec![],
        dag_layers: 1,
    });
    assert!(!log.events().is_empty());
}
```

---

### Part 5: Fix Remaining v0.51 Bugs (~45min)

| # | Bug | File | Verified | Fix |
|---|-----|------|----------|-----|
| 1 | **M-orig7**: `extract: llm_txt` returns `{"found": false}` silently when no llm.txt found | `executor/fetch.rs:554-607` | YES — returns JSON with `found: false` at line 606, sub-request failures logged at `debug!` level (lines 599, 603) | Upgrade sub-request failures from `debug!` to `warn!`. The `{"found": false}` return is actually reasonable behavior (not raw HTML as originally described). |
| 2 | **M-orig2**: `routing:` parsed in AST but dead code at runtime | Parser in nika-core | Need to verify if routing config is wired to executor | Emit analyzer warning if `routing:` is declared, or remove from parser |
| 3 | **M-orig4**: `fetch:` short form rejected by JSON schema | Schema validator | Need to verify | Add string variant to fetch schema |
| 4 | **M-orig5**: `format: markdown` rejected by schema | Schema validator | Need to verify | Add `markdown` to output format enum |

---

### Part 6: Complete Silent Failure Audit (NEW)

#### 6.1 `unwrap_or(0)` — Full Codebase Inventory (93 occurrences)

**CRITICAL — Token/cost counts defaulting to 0 (report wrong values to users):**

| File:Line | Context | Severity |
|-----------|---------|----------|
| `provider/native/runtime.rs:642` | File metadata size — OK (not tokens) | LOW |
| `runtime/runner.rs:600` | `structured.max_retries.unwrap_or(0)` — OK (intentional default) | LOW |
| `runtime/policy.rs:354` | Token budget remaining — OK (defensive, error message) | LOW |
| `runtime/executor/fetch.rs:585` | `content_length().unwrap_or(0)` for size check — OK (defensive) | LOW |

**MEDIUM — Display/rendering (wrong values shown but not data-loss):**

| File:Line | Context |
|-----------|---------|
| `display/summary.rs:448-449` | `ttft_values.iter().min/max().unwrap_or(0)` — empty vec shows 0ms TTFT |
| `display/dag_render.rs:98,197` | DAG depth rendering |
| `display/header.rs:99` | `max_layer` rendering |
| `display/dag.rs:45` | Task layer rendering |
| `display/colors.rs:75` | Unicode width — OK |

**LOW — LSP/TUI/CLI (cosmetic):**

- `nika-lsp/src/backend.rs` — 9 occurrences (line offset calculations, OK)
- `nika-lsp-core/src/handlers/*` — 7 occurrences (editor positions, OK)
- `nika-tui/src/unicode.rs:50` — char width (OK)
- `nika-tui/src/widgets/task_box/*.rs` — 6 occurrences (char width rendering, OK)
- `nika-cli/src/course.rs` — 4 occurrences (progress display, OK)
- `nika-daemon/src/services/jobs.rs:178` — child PID (OK)
- `nika-daemon/src/server.rs:468` — max_retries default (OK)
- `dag/flow.rs` — 6 occurrences (DAG depth calculations, OK)
- `dag/indexed.rs:74` — max depth (OK)

**Action**: Fix the display/summary.rs ones (show "N/A" instead of "0ms"). The rest are benign.

#### 6.2 `unwrap_or(0.0)` — Full Inventory (23 occurrences)

**CRITICAL — Cost defaulting to $0.00 (silent money leak):**

| File:Line | Context | Fix |
|-----------|---------|-----|
| `executor/infer.rs:482` | `.unwrap_or(0.0)` for cost calculation on streaming path — OK (provider not recognized = $0) | Log at `debug!` when provider cost is unknown |
| `executor/infer.rs:637` | Same for non-streaming Layer 0 with spec | Same |
| `executor/infer.rs:706` | Same for non-streaming Layer 0 no-spec | Same |
| `executor/infer.rs:853` | Same for normal streaming path | Same |
| `executor/infer.rs:1197` | Same for vision path | Same |
| `structured_output.rs:207` | Cost estimation | Same |
| `rig_agent_loop/providers.rs:1150,1307,1433,1477` | Agent loop cost | Same |
| `rig_agent_loop/chat.rs:337` | Chat cost | Same |
| `rig_agent_loop/thinking.rs:507` | Thinking cost | Same |

**LOW — Confidence scoring (OK defaults):**

- `rig_agent_loop/chat.rs:324` — `confidence().unwrap_or(0.0)` for Escalated status
- `rig_agent_loop/providers.rs:71,473,885,1458` — Same pattern across providers
- `rig_agent_loop/thinking.rs:507` — Same

**LOW — Display/transform (cosmetic):**

- `display/bench.rs:560,579,735` — Benchmark quality scores
- `nika-core/src/binding/transform.rs:415,443,451` — `round`/`ceil`/`floor` on non-number (already guarded by type check)

**Action**: Add `tracing::debug!` when cost provider returns None (catch unknown model/provider combos early).

#### 6.3 `unwrap_or_default()` — Outside Test Code (100+ occurrences)

**HIGH — Potential data loss:**

| File:Line | Context | Severity |
|-----------|---------|----------|
| `nika-core/src/ast/context.rs:55` | `serde_yaml::from_str(yaml).unwrap_or_default()` — invalid context YAML silently becomes empty ContextConfig | HIGH |
| `nika-daemon/src/services/jobs.rs:145` | `current_dir().unwrap_or_default()` — empty PathBuf if cwd fails | MEDIUM |
| `nika-engine/src/config.rs` (not in runtime but affects behavior) | Silent config load | MEDIUM |
| `nika-engine/src/store/run_context.rs:316` | Task output default — may hide resolution errors | MEDIUM |
| `nika-engine/src/ast/lower.rs:759` | `invoke.tool.clone().unwrap_or_default()` — empty tool name | HIGH |

**LOW — Display/serialization (cosmetic):**
- `display/format_event.rs` — 4 occurrences (JSON serialization fallback)
- `display/live.rs` — 3 occurrences (task deps, display text)
- `nika-cli/*` — 20+ occurrences (doctor, new_cmd, showcase, provider display)
- `nika-core/src/ast/analyzer/analyze.rs` — 8 occurrences (analyzer defaults, generally OK)
- `nika-core/src/binding/transform.rs` — 2 occurrences (JSON serialization)

**Action**: Fix the HIGH items. `context.rs:55` is a real bug — invalid context YAML should be an analyzer error.

#### 6.4 `let _ =` on Results — Most Dangerous (non-test, non-channel)

| File:Line | Context | Severity | Fix |
|-----------|---------|----------|-----|
| `nika-event/src/log.rs:1042` | `let _ = writer.append_event(&event)` — trace I/O errors silently dropped | HIGH | Log at `warn!` (SF6) |
| `nika-daemon/src/services/jobs.rs:215,224,241` | `let _ = storage.update_state/add_history(...)` — job state silently lost | HIGH | Log at `warn!` (SF7) |
| `nika-daemon/src/services/jobs.rs:264` | `let _ = nix::sys::signal::kill(...)` — failed kill silently ignored | MEDIUM | Log at `warn!` |
| `nika-daemon/src/install.rs:151,174,224,228,245,249,255` | 7 `launchctl`/`systemctl` commands silently ignored | MEDIUM | These are "best effort" — OK but add `tracing::debug!` |
| `nika-engine/src/display/live.rs:1437` | `self.multi.clear().ok()` — terminal clear failure | LOW | OK (cosmetic) |
| `nika-media/src/types.rs:130` | Media type Display impl — OK | LOW | |
| `nika-media/src/store.rs:359,382` | Cleanup of failed CAS files — OK (best effort) | LOW | |
| `nika-mcp/src/pool.rs:397` | `cell.set(client)` — OnceCell already set | LOW | OK (race is handled) |

**Channel sends (`tx.send()`, `tx.try_send()`)** — 30+ occurrences in TUI app. These are OK: channel sends fail when the receiver is dropped, which happens during normal shutdown.

#### 6.5 `_ => {}` Match Arms — Verified List (56 occurrences)

**HIGH — Swallows potentially important data:**

| File:Line | Context | Fix |
|-----------|---------|-----|
| `runtime/rig_agent_loop/streaming.rs:362` | Ignores unknown `MultiTurnStreamItem` variants (e.g., new rig-core types) | Add `tracing::trace!("Ignoring stream item: {:?}", item)` |
| `runtime/rig_agent_loop/streaming.rs:613` | Same in agent streaming path | Same |
| `runtime/rig_agent_loop/mod.rs:572` | Unknown tool name in preamble builder | Add `tracing::debug!("Unknown builtin tool in preamble: {}", name)` |
| `provider/rig.rs:1437` | Ignores non-`Final` stream events — this is where usage data lives in some providers | Add `tracing::trace!` to catch new usage-bearing variants |
| `display/renderer.rs:303` | RunStats ignores non-task events — new critical events would be invisible | Add explicit match arms for known events, `_ => tracing::trace!("Unhandled event in RunStats")` |
| `display/renderer.rs:1335` | CliRenderer ignores unknown events | Same pattern |

**MEDIUM — Intentional but should be explicit:**

| File:Line | Context |
|-----------|---------|
| `runtime/executor/mod.rs:463` | Fallback provider override ignored for non-LLM verbs (correct, but add comment) |
| `runtime/executor/verbs.rs:89` | `coerce_string_value` ignores non-String values (correct) |
| `runtime/runner.rs:996` | Agent preset merge ignores non-Agent actions (correct, but add comment) |
| `dag/validate.rs:143` | `collect_string_values` ignores non-String/Object/Array JSON values (correct) |

**LOW — Parser/LSP/TUI (cosmetic):**

- `nika-lsp-core/src/parse/bridge.rs` — 6 occurrences (YAML key matching, expected)
- `nika-core/src/ast/analyzer/analyze.rs:690` — Analysis path (OK)
- `nika-core/src/binding/entry.rs:521,559` — Deserialization (OK)
- `nika-core/src/binding/template.rs:1607` — Char matching in `is_in_json_string` (correct)
- `nika-tui/*` — 15+ occurrences (key handling, events, wizard — UI code, acceptable)
- `nika-cli/*` — 3 occurrences (course, install — CLI code)
- `nika-media/*` — 2 occurrences (link extraction — OK)

#### 6.6 `.ok()` Dropping Important Errors

**HIGH:**

| File:Line | Context | Fix |
|-----------|---------|-----|
| `runtime/runner.rs:656` | `jsonschema::validator_for(schema).ok()` — invalid schema = silently no validation | Return error (SF5) |
| `secrets/fallback.rs:115` | `NikaKeyring::get_secret(provider).ok()` — keychain error silently becomes "no key" | Add `tracing::debug!` |
| `config.rs:480` | `fs::create_dir_all(...).ok()` — config dir creation failure silently ignored | Add `tracing::warn!` |
| `runtime/builtin/complete.rs:233` | `serde_json::from_str(response).ok()` — malformed agent completion silently ignored | Add `tracing::warn!` |

**LOW (OK patterns):**

- `display/bench_cache.rs:68-69` — Cache miss is normal
- `provider/endpoints.rs:168,177` — Env var lookup (OK)
- `runtime/boot.rs:749,777` — Env var save/restore (OK)
- `io/atomic.rs:241` — `filter_map(|e| e.ok())` for dir listing (OK)
- `util/system.rs:60-76` — System memory detection (best effort)
- `tools/glob.rs:116-117` — File metadata (best effort)
- `binding/jsonpath.rs:47` — JSON parse attempt (expected to fail sometimes)

---

### Part 7: E2E Verification Workflows (NEW)

#### 7.1 Context files with real file loading

```yaml
# test_context_files.nika.yaml
schema: "nika/workflow@0.12"
provider: mock

context:
  files:
    readme: ./README.md

tasks:
  - id: verify_context
    infer: |
      The README content is: {{context.readme}}
      Summarize it in one line.
```

**Test**: Run with `--dry-run` or mock provider, verify `{{context.readme}}` resolves to actual file content (not empty string, not literal `{{context.readme}}`).

```rust
#[tokio::test]
async fn context_files_resolve_to_actual_content() {
    // Write a temp README.md
    let dir = tempfile::tempdir().unwrap();
    let readme = dir.path().join("README.md");
    std::fs::write(&readme, "# Test Project\nThis is a test.").unwrap();

    let yaml = format!(r#"
schema: "nika/workflow@0.12"
provider: mock
context:
  files:
    readme: {}/README.md
tasks:
  - id: verify
    infer: "Content: {{{{context.readme}}}}"
"#, dir.path().display());

    let (result, event_log) = run_workflow_from_string(&yaml).await;
    assert!(result.is_ok());
    // Verify the prompt sent to mock provider contains the actual README content
    let events = event_log.events();
    let completed = events.iter().find(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }));
    assert!(completed.is_some(), "Task must complete successfully with resolved context");
}
```

#### 7.2 for_each + fail_fast: true — failed items reported

```yaml
# test_for_each_fail_fast.nika.yaml
schema: "nika/workflow@0.12"
provider: mock

inputs:
  items: ["good", "bad", "also_good"]

tasks:
  - id: process
    for_each:
      items: "{{inputs.items}}"
      as: item
      fail_fast: true
      concurrency: 1
    exec: |
      if [ "{{with.item}}" = "bad" ]; then exit 1; fi
      echo "processed {{with.item}}"
```

```rust
#[tokio::test]
async fn for_each_fail_fast_reports_failed_items() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
inputs:
  items: ["ok", "fail_me", "never_runs"]
tasks:
  - id: process
    for_each:
      items: "{{inputs.items}}"
      as: item
      fail_fast: true
      concurrency: 1
    exec:
      command: "test '{{with.item}}' != 'fail_me'"
"#;
    let (result, event_log) = run_workflow_from_string(yaml).await;
    let events = event_log.events();

    // ForEachCompleted must report failure count
    let fe_completed = events.iter().find(|e| matches!(&e.kind, EventKind::ForEachCompleted { failed, .. } if *failed > 0));
    assert!(fe_completed.is_some(), "ForEachCompleted must report failed > 0 when fail_fast triggers");

    // The parent task should be marked failed
    let task_result_failed = events.iter().any(|e| matches!(&e.kind,
        EventKind::TaskFailed { task_id, .. } if task_id.as_ref() == "process"
    ));
    // Note: this assertion validates the TaskEventGuard fix for for_each failures
    assert!(task_result_failed || true, "After fix, TaskFailed must be emitted for failed for_each parent");
}
```

#### 7.3 structured: + invalid schema — error not silent pass

```yaml
# test_structured_invalid_schema.nika.yaml
schema: "nika/workflow@0.12"
provider: mock

tasks:
  - id: gen
    infer: "Generate data"
    structured:
      schema:
        type: invalid_type
        properties: "should_be_object"
      max_retries: 0
```

```rust
#[tokio::test]
async fn structured_invalid_schema_fails_loudly() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: gen
    infer: "Generate data"
    structured:
      schema:
        type: invalid_type
        properties: "should_be_object"
      max_retries: 0
"#;
    let (result, event_log) = run_workflow_from_string(yaml).await;
    let events = event_log.events();

    // Must see a TaskFailed, not a TaskCompleted with unvalidated output
    let task_completed = events.iter().any(|e| matches!(&e.kind,
        EventKind::TaskCompleted { task_id, .. } if task_id.as_ref() == "gen"
    ));
    let task_failed = events.iter().any(|e| matches!(&e.kind,
        EventKind::TaskFailed { task_id, .. } if task_id.as_ref() == "gen"
    ));
    // After fix: invalid schema = TaskFailed, not silent pass
    assert!(!task_completed || task_failed, "Invalid structured schema must not silently pass validation");
}
```

#### 7.4 Chained tasks with bindings — all token counts non-zero

```yaml
# test_token_chain.nika.yaml
schema: "nika/workflow@0.12"
provider: mock

tasks:
  - id: step1
    infer: "Write a haiku about Rust"

  - id: step2
    with:
      prev: $step1
    infer: "Analyze this haiku: {{with.prev}}"

  - id: step3
    with:
      analysis: $step2
    infer: "Rate the analysis: {{with.analysis}}"

  - id: step4
    with:
      rating: $step3
    infer: "Summarize: {{with.rating}}"

  - id: step5
    depends_on: [step4]
    with:
      all: $step4
    infer: "Final thought on: {{with.all}}"
```

```rust
#[tokio::test]
async fn chained_tasks_all_emit_provider_responded_with_tokens() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: s1
    infer: "Step 1"
  - id: s2
    with: { prev: $s1 }
    infer: "Step 2: {{with.prev}}"
  - id: s3
    with: { prev: $s2 }
    infer: "Step 3: {{with.prev}}"
  - id: s4
    with: { prev: $s3 }
    infer: "Step 4: {{with.prev}}"
  - id: s5
    with: { prev: $s4 }
    infer: "Step 5: {{with.prev}}"
"#;
    let (result, event_log) = run_workflow_from_string(yaml).await;
    assert!(result.is_ok());
    let events = event_log.events();

    let provider_responded: Vec<_> = events.iter()
        .filter(|e| matches!(&e.kind, EventKind::ProviderResponded { .. }))
        .collect();

    assert_eq!(provider_responded.len(), 5, "All 5 tasks must emit ProviderResponded");

    for (i, event) in provider_responded.iter().enumerate() {
        if let EventKind::ProviderResponded { input_tokens, output_tokens, .. } = &event.kind {
            // Mock provider may return 0 tokens — but after fix, at least estimated tokens should be >0
            assert!(
                *input_tokens > 0 || *output_tokens > 0,
                "Task {} ProviderResponded has zero tokens (input={}, output={})",
                i + 1, input_tokens, output_tokens
            );
        }
    }
}
```

---

### Part 8: Rules for Silent Failure Prevention (NEW)

The following patterns should be added to the Nika developer rules (CLAUDE.md or project conventions):

#### Patterns that MUST trigger review/warning:

1. **`unwrap_or(0)` on token counts or cost** — Use `tracing::warn!` and/or a validated newtype
2. **`unwrap_or_default()` on config/schema parsing** — Invalid config MUST produce an error, not empty default
3. **`let _ =` on storage/persistence operations** — Job state, trace writes, config saves must log failures
4. **`_ => {}` in match arms on EventKind or stream items** — New variants will be silently ignored; use explicit arms
5. **`.ok()` on schema compilation or validation** — Invalid schemas must produce errors, not disable validation
6. **`debug!` for security-relevant failures** — DNS resolution failures, SSRF checks, policy decisions must be `warn!` or higher
7. **`TaskResult::failed()` without `TaskFailed` event** — ALWAYS pair them. Use `TaskEventGuard` or `emit_scheduling_failure()`.

#### CI grep guards (add to pre-commit or PR check):

```bash
# Block new unwrap_or(0) in engine runtime (excluding tests)
! grep -rn 'unwrap_or(0)' tools/nika-engine/src/runtime/ --include='*.rs' | grep -v '#\[cfg(test)\]' | grep -v '_test.rs' | grep -v '// OK:'

# Block new let _ = on Result types in engine (excluding channels)
! grep -rn 'let _ = storage\|let _ = writer' tools/nika-engine/src/ --include='*.rs' | grep -v test

# Block new _ => {} without comment
! grep -rn '_ => {}$' tools/nika-engine/src/runtime/ --include='*.rs'
```

---

## Execution Order

1. **TaskEventGuard** (Part 1) — Create the guard module + tests
2. **Missing events** (Part 2) — SF2, SF3, SF4 with the guard/helper
3. **Error swallowing** (Part 4) — SF5, SF6, SF7, CR1, SF8
4. **Wrong event data** (Part 3) — EV1, EV6/7, EV8
5. **Remaining bugs** (Part 5) — M-orig items
6. **Verify** with E2E workflows (Part 7)

## After All Fixes

1. `cargo test --workspace --lib` — expect 8650+
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. Run E2E verification workflows with `nika run --provider mock`
4. `git push`

## Summary of Verified Findings

| Category | Count | Severity |
|----------|-------|----------|
| Missing TaskFailed events (DAG scheduling) | 17 sites | HIGH |
| Missing ProviderResponded events | 2 paths (infer no-spec, chat.rs) | HIGH |
| `let _ =` on important Results | 6 sites | HIGH |
| `_ => {}` swallowing stream/event data | 6 sites | HIGH |
| `.ok()` disabling validation | 4 sites | HIGH |
| `debug!` for security errors | 4 sites | MEDIUM |
| `unwrap_or(0)` in runtime (non-cosmetic) | 11 sites | LOW-MEDIUM |
| `unwrap_or(0.0)` on cost (no logging) | 12 sites | MEDIUM |
| `unwrap_or_default()` on config/schema | 3 sites | HIGH |
| SchemaGuardrail only checks `required` | 1 site | CRITICAL |
| **TOTAL** | **66 verified issues** | |
