# MEGA HANDOFF: v0.71+ Post-Launch Features

> **Codebase**: nika v0.70.0 | ~395K LOC | 15 crates | 4694 tests | 62 tools | 63 transforms | 10 lint rules
> **Launch**: May 5, 2026 — FEATURE FREEZE until then (bug fixes only)
> **Post-launch**: 5 features in priority order, ~6-8 weeks total
> **Philosophy**: v0 = zero dead code, zero backward compat, TDD, 1 fix = 1 commit

---

## PRIORITY ORDER

```
                  EFFORT    IMPACT    SHIP BY
 1. on_error:      ~700 LOC  HIGH     v0.71 (week 1)
 2. Scheduling     ~720 LOC  HIGH     v0.72 (week 2)
 3. Multi-tenant   ~800 LOC  HIGH     v0.73 (week 3) — L1 only, L2/L3 later
 4. Observability  ~2000 LOC MEDIUM   v0.74 (weeks 4-6)
 5. PostgreSQL     ~1500 LOC LOW      v0.75 (weeks 7-8)
```

---

## FEATURE 1: `on_error:` Fallback Routing (v0.71)

### What
When a task fails, optionally route to a fallback instead of cascading NIKA-026 to dependents.

### YAML Syntax
```yaml
# Ignore failure, continue with null output
- id: optional_enrichment
  infer: "Enrich: {{with.data}}"
  on_error:
    ignore: true

# Failover to different provider
- id: generate
  provider: anthropic
  infer: "Write tagline for {{inputs.product}}"
  on_error:
    retry_with_provider: openai

# Use another task's action as fallback template
- id: fallback_gen
  provider: openai
  model: gpt-4o-mini
  infer: "Write tagline for {{inputs.product}}"

- id: generate
  provider: anthropic
  infer: "Write tagline for {{inputs.product}}"
  on_error:
    fallback: fallback_gen

# Works with retry: — retry fires first, on_error fires if ALL retries fail
- id: fragile_api
  retry: { max_attempts: 3, delay_ms: 2000 }
  fetch: { url: "https://api.example.com/data" }
  on_error:
    fallback: cached_fallback

# Works with for_each — per-item fallback
- id: translate
  for_each: "$articles"
  as: article
  fail_fast: false
  infer: "Translate: {{with.article.text}}"
  on_error:
    ignore: true   # failed items → null in array
```

### Architecture
- **NOT a DAG change** — fallback is runtime, not structural. Mirrors `retry:` pattern.
- Interception in `execute_task_iteration()` after all retries exhausted.
- `ignore:` → store `TaskResult::success(Value::Null)`, emit `TaskFallbackTriggered`.
- `retry_with_provider:` → rebuild action with new provider, execute once.
- `fallback:` → look up fallback task's `AnalyzedTaskAction`, execute once (depth limit 1).
- Result stored under ORIGINAL task_id → downstream sees success → no cascade.

### Files to Modify

| File | Change |
|------|--------|
| `nika-core/src/ast/raw/task.rs` | Add `on_error: Option<Spanned<Value>>` to `RawTask` |
| `nika-core/src/ast/raw/parser.rs` | Recognize `"on_error"` in `KNOWN_TASK_KEYS` + parse field |
| `nika-core/src/ast/analyzed/task.rs` | Add `AnalyzedOnError` + `OnErrorAction` enum + field on `AnalyzedTask` |
| `nika-core/src/ast/analyzed/mod.rs` | Re-export `AnalyzedOnError`, `OnErrorAction` |
| `nika-core/src/ast/analyzer/analyze.rs` | Parse + resolve fallback task_id via `task_table` |
| `nika-core/src/ast/analyzer/errors.rs` | Add `UnknownOnErrorFallback` variant = NIKA-290 |
| `nika-event/src/log.rs` | Add `TaskFallbackTriggered` variant + wire into `task_id()` match |
| `nika-engine/src/runtime/runner.rs` | on_error dispatch block in failure path (line ~1468-1479) |

---

### EXACT CODE: AnalyzedOnError + OnErrorAction (nika-core/src/ast/analyzed/task.rs)

Insert **after** `AnalyzedRetry` (after line 506, before `#[cfg(test)]` on line 508):

```rust
/// On-error fallback configuration (analyzed).
///
/// Evaluated at runtime AFTER all retries are exhausted. The fallback action
/// runs once — if it also fails, the task fails definitively.
///
/// Depth limit: when a task runs as a fallback, its own `on_error` is IGNORED.
/// This prevents infinite fallback chains (A -> B -> C -> ...).
#[derive(Debug, Clone)]
pub struct AnalyzedOnError {
    /// The recovery action to take.
    pub action: OnErrorAction,
    /// Span of the on_error: block (for diagnostics).
    pub span: Span,
}

/// What to do when a task fails after retries.
#[derive(Debug, Clone)]
pub enum OnErrorAction {
    /// Swallow the failure: store `Value::Null` as output, mark task succeeded.
    /// Downstream tasks see null (guard with `| default("fallback")`).
    Ignore,

    /// Re-execute the same action with a different provider (single attempt).
    /// The original task's prompt, model override, and bindings are preserved;
    /// only the provider is swapped.
    RetryWithProvider {
        provider: crate::ProviderName,
    },

    /// Execute a different task's action as the recovery path.
    /// The fallback task's `AnalyzedTaskAction` is cloned and executed once
    /// with the ORIGINAL task's resolved bindings. The fallback task must
    /// exist in the DAG (validated at analysis time via NIKA-290).
    Fallback {
        task_id: TaskId,
    },
}
```

Add the field to `AnalyzedTask` struct (after `retry` field, line 73):

```rust
    /// On-error fallback configuration (fires after all retries exhausted)
    pub on_error: Option<AnalyzedOnError>,
```

This means the `AnalyzedTask` struct gains one field. Every constructor site must add
`on_error: None` — there are 3 sites:
1. `analyze_task()` in `analyze.rs` (line ~733)
2. `test_module_exports()` in `analyzed/mod.rs` (line ~86)
3. Every test in `runner.rs` that constructs `AnalyzedTask` directly (~15 sites, grep for `AnalyzedTask {`)

---

### EXACT CODE: RawTask field (nika-core/src/ast/raw/task.rs)

Add after `when` field (line 93), before `span`:

```rust
    /// On-error fallback: `on_error: { ignore: true }`, `on_error: { fallback: task_id }`,
    /// or `on_error: { retry_with_provider: openai }`.
    /// Stored as raw JSON Value; resolved in Phase 2 analysis.
    pub on_error: Option<Spanned<serde_json::Value>>,
```

Update `RawTask::new()` (line 139) — add `on_error: None` to the Default spread.

---

### EXACT CODE: Parser addition (nika-core/src/ast/raw/parser.rs)

**Step 1**: Add `"on_error"` to `KNOWN_TASK_KEYS` array (line 1809).

Insert after `"when"` (line 1832), before `"skills"` (line 1833):

```rust
        "on_error",
```

**Step 2**: Parse the `on_error:` field in `parse_task()` (line 1893).

Insert after the `when` parse (line 2036), before the `standalone_concurrency` parse (line 2039):

```rust
    // Parse on_error: config (task-level fallback routing)
    let on_error = match map.get_node("on_error") {
        Some(node) => {
            let span = node_to_span(file_id, node);
            let value = node_to_json(node);
            Some(Spanned::new(value, span))
        }
        None => None,
    };
```

**Step 3**: Add `on_error` to the `RawTask` constructor (line 2050).

Insert after `when,` (line 2073), before closing brace:

```rust
        on_error,
```

---

### EXACT CODE: Analyzer resolution (nika-core/src/ast/analyzer/analyze.rs)

**In `analyze_task()` (starts line 725):**

Add `on_error: None` to the initial `AnalyzedTask` struct literal (around line 733-850).
Insert after `retry:` field assignment (line 753):

```rust
        on_error: None,
```

Then, after the `depends_on` resolution block (after line 941, before the `for_each`/`decompose`
warning on line 944), add the on_error resolution:

```rust
    // Resolve on_error: fallback configuration
    if let Some(ref on_error_raw) = raw.on_error {
        let value = &on_error_raw.value;
        let span = on_error_raw.span;

        if let Some(serde_json::Value::Bool(true)) = value.get("ignore") {
            // on_error: { ignore: true }
            task.on_error = Some(AnalyzedOnError {
                action: OnErrorAction::Ignore,
                span,
            });
        } else if let Some(serde_json::Value::String(provider_str)) =
            value.get("retry_with_provider")
        {
            // on_error: { retry_with_provider: "openai" }
            let provider = crate::ProviderName::parse(provider_str);
            task.on_error = Some(AnalyzedOnError {
                action: OnErrorAction::RetryWithProvider { provider },
                span,
            });
        } else if let Some(serde_json::Value::String(fallback_name)) = value.get("fallback") {
            // on_error: { fallback: "other_task_id" }
            if let Some(fallback_id) = task_table.get_id(fallback_name) {
                task.on_error = Some(AnalyzedOnError {
                    action: OnErrorAction::Fallback {
                        task_id: fallback_id,
                    },
                    span,
                });
            } else {
                // Unknown fallback task — NIKA-290
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion =
                    find_similar(fallback_name, &all_names, 0.6);
                ctx.add_error(AnalyzeError {
                    kind: AnalyzeErrorKind::UnknownOnErrorFallback,
                    span,
                    message: format!(
                        "on_error fallback references unknown task '{}'",
                        fallback_name
                    ),
                    suggestion: suggestion.map(|s| format!("did you mean '{}'?", s)),
                    note: Some(
                        "The fallback task must be defined in the same workflow".to_string(),
                    ),
                });
            }
        } else {
            ctx.add_error(AnalyzeError {
                kind: AnalyzeErrorKind::InvalidValue,
                span,
                message: "on_error: must contain exactly one of: ignore, retry_with_provider, \
                          or fallback"
                    .to_string(),
                suggestion: Some(
                    "Example: on_error: { ignore: true } or on_error: { fallback: task_id }"
                        .to_string(),
                ),
                note: None,
            });
        }
    }
```

**In `validate()` (starts line 100):**

Add on_error validation in the task reference validation loop (after line 166,
inside the `for raw_task in raw.tasks.value.iter()` loop that calls `validate_task_refs`).
Insert after `validate_task_refs(...)` call:

```rust
        // Validate on_error: fallback references
        if let Some(ref on_error_raw) = raw_task.value.on_error {
            if let Some(serde_json::Value::String(fallback_name)) =
                on_error_raw.value.get("fallback")
            {
                if !task_table.contains(fallback_name)
                    && !ctx.is_included_task(fallback_name)
                {
                    let all_names: Vec<&str> = task_names.iter().map(|s| s.as_str()).collect();
                    let suggestion = find_similar(fallback_name, &all_names, 0.6);
                    ctx.add_error(AnalyzeError {
                        kind: AnalyzeErrorKind::UnknownOnErrorFallback,
                        span: on_error_raw.span,
                        message: format!(
                            "on_error fallback references unknown task '{}'",
                            fallback_name
                        ),
                        suggestion: suggestion.map(|s| format!("did you mean '{}'?", s)),
                        note: None,
                    });
                }
            }
        }
```

---

### EXACT CODE: NIKA-290 Error (nika-core/src/ast/analyzer/errors.rs)

Add variant to `AnalyzeErrorKind` enum (after `MissingModel` on line 181):

```rust
    /// NIKA-290: on_error: fallback references a task that does not exist
    UnknownOnErrorFallback,
```

Add code mapping in `AnalyzeErrorKind::code()` (after `Self::MissingModel` on line 207):

```rust
            Self::UnknownOnErrorFallback => "NIKA-290",
```

Add convenience constructor on `AnalyzeError` (after `invalid_binding()` on line 147):

```rust
    /// Create an "unknown on_error fallback" error.
    pub fn unknown_on_error_fallback(
        span: Span,
        name: &str,
        suggestion: Option<&str>,
    ) -> Self {
        let mut err = Self::new(
            AnalyzeErrorKind::UnknownOnErrorFallback,
            span,
            format!("on_error fallback references unknown task '{}'", name),
        );
        if let Some(s) = suggestion {
            err = err.with_suggestion(format!("did you mean '{}'?", s));
        }
        err.with_note("The fallback task must be defined in the same workflow")
    }
```

---

### EXACT CODE: EventKind variant (nika-event/src/log.rs)

Add after `FallbackChainExhausted` (line 1064), before the closing `}` of `EventKind`:

```rust
    // ═══════════════════════════════════════════
    // ON_ERROR FALLBACK
    // ═══════════════════════════════════════════
    /// Task-level on_error: fallback was triggered after primary execution + retries failed.
    ///
    /// Event ordering: TaskStarted -> TaskFailed -> TaskFallbackTriggered -> TaskCompleted (or TaskFailed).
    /// The TaskFailed event is for the PRIMARY action. If fallback succeeds,
    /// TaskCompleted follows with the fallback output under the original task_id.
    TaskFallbackTriggered {
        task_id: Arc<str>,
        /// Which on_error action fired: "ignore", "retry_with_provider", or "fallback"
        action: String,
        /// For retry_with_provider: the provider name. For fallback: the fallback task id.
        /// For ignore: empty string.
        target: String,
        /// Whether the fallback itself succeeded.
        success: bool,
    },
```

Wire into `task_id()` match arm (add after `FallbackChainExhausted` on line 1130):

```rust
            | Self::TaskFallbackTriggered { task_id, .. }
```

Update the module docstring event count: `58 variants` -> `59 variants`.

---

### EXACT CODE: Runner interception (nika-engine/src/runtime/runner.rs)

The interception point is **inside `execute_task_iteration()`**, at lines 1468-1479.
This is the `Err(e)` branch of the `match result { Ok(output) => { ... }, Err(e) => { ... } }` block
that handles the case where all retries are exhausted.

**Current code (lines 1468-1479):**
```rust
                Err(e) => {
                    // Drain any orphaned media refs (defense-in-depth)
                    let _ = datastore.take_media(&task_id);
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some(e.code().to_string()),
                    });
                    TaskResult::failed(e.to_string(), duration)
                }
```

**Replace with:**
```rust
                Err(e) => {
                    // Drain any orphaned media refs (defense-in-depth)
                    let _ = datastore.take_media(&task_id);

                    // Emit TaskFailed for the PRIMARY action (always, even if fallback fires)
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some(e.code().to_string()),
                    });

                    // ── on_error: fallback routing ──
                    // Only fires when:
                    // 1. Task has on_error: config
                    // 2. We are NOT already running as a fallback (depth limit 1)
                    // 3. All retries have been exhausted
                    //
                    // The `is_fallback_execution` flag is passed into this function
                    // to prevent A.on_error -> B.on_error -> C infinite chains.
                    if !is_fallback_execution {
                        if let Some(ref on_error) = task.on_error {
                            match &on_error.action {
                                OnErrorAction::Ignore => {
                                    event_log.emit(EventKind::TaskFallbackTriggered {
                                        task_id: Arc::clone(&task_id),
                                        action: "ignore".into(),
                                        target: String::new(),
                                        success: true,
                                    });
                                    // Return success with null — downstream sees null
                                    let tr = TaskResult::success(Value::Null, duration);
                                    event_log.emit(EventKind::TaskCompleted {
                                        task_id: Arc::clone(&task_id),
                                        output: Arc::clone(&tr.output),
                                        duration_ms: duration.as_millis() as u64,
                                    });
                                    tr
                                }

                                OnErrorAction::RetryWithProvider { provider } => {
                                    // Rebuild the lowered action with the fallback provider
                                    let fallback_chain = None; // No further routing
                                    let fallback_action = lower_action(
                                        &task.action,
                                        &Some(provider.clone()),
                                        &task.model.clone().or(effective_model.clone()),
                                        &None, // No retry on fallback execution
                                        &resolved_base_url,
                                        &fallback_chain,
                                    );

                                    let fb_result = executor
                                        .execute(
                                            &task_id,
                                            &fallback_action,
                                            &bindings,
                                            &datastore,
                                            effective_output.as_ref(),
                                        )
                                        .await;

                                    let fb_success = fb_result.is_ok();
                                    event_log.emit(EventKind::TaskFallbackTriggered {
                                        task_id: Arc::clone(&task_id),
                                        action: "retry_with_provider".into(),
                                        target: provider.as_str().to_string(),
                                        success: fb_success,
                                    });

                                    match fb_result {
                                        Ok(output) => {
                                            let tr = make_task_result(
                                                output,
                                                effective_output.as_ref(),
                                                duration,
                                            )
                                            .await;
                                            event_log.emit(EventKind::TaskCompleted {
                                                task_id: Arc::clone(&task_id),
                                                output: Arc::clone(&tr.output),
                                                duration_ms: duration.as_millis() as u64,
                                            });
                                            tr
                                        }
                                        Err(fb_err) => {
                                            event_log.emit(EventKind::TaskFailed {
                                                task_id: Arc::clone(&task_id),
                                                error: format!(
                                                    "on_error retry_with_provider({}) also failed: {}",
                                                    provider.as_str(),
                                                    fb_err
                                                ),
                                                duration_ms: duration.as_millis() as u64,
                                                error_code: Some(fb_err.code().to_string()),
                                            });
                                            TaskResult::failed(
                                                format!(
                                                    "Primary: {}; Fallback({}): {}",
                                                    e, provider.as_str(), fb_err
                                                ),
                                                duration,
                                            )
                                        }
                                    }
                                }

                                OnErrorAction::Fallback { task_id: fb_task_id } => {
                                    // Look up the fallback task from the workflow
                                    let fb_task_opt = executor.get_task(*fb_task_id);

                                    if let Some(fb_task) = fb_task_opt {
                                        // Lower the fallback task's action and execute once
                                        let fb_provider_chain = fb_task
                                            .routing
                                            .as_ref()
                                            .filter(|r| r.fallback.len() > 1)
                                            .map(|r| {
                                                r.fallback
                                                    .iter()
                                                    .map(|s| nika_core::ProviderName::parse(s))
                                                    .collect()
                                            });
                                        let fb_action = lower_action(
                                            &fb_task.action,
                                            &fb_task.provider,
                                            &fb_task.model,
                                            &None, // No retry on fallback execution
                                            &fb_task.base_url,
                                            &fb_provider_chain,
                                        );

                                        let fb_result = executor
                                            .execute(
                                                &task_id,
                                                &fb_action,
                                                &bindings,
                                                &datastore,
                                                effective_output.as_ref(),
                                            )
                                            .await;

                                        let fb_success = fb_result.is_ok();
                                        event_log.emit(EventKind::TaskFallbackTriggered {
                                            task_id: Arc::clone(&task_id),
                                            action: "fallback".into(),
                                            target: fb_task.name.clone(),
                                            success: fb_success,
                                        });

                                        match fb_result {
                                            Ok(output) => {
                                                let tr = make_task_result(
                                                    output,
                                                    effective_output.as_ref(),
                                                    duration,
                                                )
                                                .await;
                                                event_log.emit(EventKind::TaskCompleted {
                                                    task_id: Arc::clone(&task_id),
                                                    output: Arc::clone(&tr.output),
                                                    duration_ms: duration.as_millis() as u64,
                                                });
                                                tr
                                            }
                                            Err(fb_err) => {
                                                event_log.emit(EventKind::TaskFailed {
                                                    task_id: Arc::clone(&task_id),
                                                    error: format!(
                                                        "on_error fallback('{}') also failed: {}",
                                                        fb_task.name, fb_err
                                                    ),
                                                    duration_ms: duration.as_millis() as u64,
                                                    error_code: Some(fb_err.code().to_string()),
                                                });
                                                TaskResult::failed(
                                                    format!(
                                                        "Primary: {}; Fallback({}): {}",
                                                        e, fb_task.name, fb_err
                                                    ),
                                                    duration,
                                                )
                                            }
                                        }
                                    } else {
                                        // Fallback task not found at runtime (should never happen
                                        // if analyzer did its job, but defensive)
                                        TaskResult::failed(
                                            format!(
                                                "on_error fallback task not found (internal error): {:?}",
                                                fb_task_id
                                            ),
                                            duration,
                                        )
                                    }
                                }
                            }
                        } else {
                            // No on_error config — original failure behavior
                            TaskResult::failed(e.to_string(), duration)
                        }
                    } else {
                        // Running as fallback — do NOT chain on_error further
                        TaskResult::failed(e.to_string(), duration)
                    }
                }
```

**Required signature change for `execute_task_iteration()`:**

Current signature (line 1078):
```rust
    async fn execute_task_iteration(
        task: Arc<AnalyzedTask>,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: RunContext,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>,
        workflow_artifacts: Option<ArtifactsConfig>,
        base_path: PathBuf,
        workflow_base_url: Option<String>,
    ) -> IterationResult {
```

New signature — add one parameter at the end:
```rust
    async fn execute_task_iteration(
        task: Arc<AnalyzedTask>,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: RunContext,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>,
        workflow_artifacts: Option<ArtifactsConfig>,
        base_path: PathBuf,
        workflow_base_url: Option<String>,
        is_fallback_execution: bool,
    ) -> IterationResult {
```

Update all 2 call sites:
- Line 2368 (for_each path): add `false` as last argument
- Line 2461 (regular task path): add `false` as last argument

**Required variables in scope**: The on_error dispatch block references `effective_model`,
`resolved_base_url`, `bindings`, `effective_output`, and `executor.execute()`. All of these
are already in scope at the interception point (they were computed earlier in the function
between lines 1211-1275). The `lower_action` and `make_task_result` functions are already
imported at the top of runner.rs. The `OnErrorAction` type needs a new import:

```rust
use crate::ast::analyzed::OnErrorAction; // Add to imports at line 22-24
```

**Required addition to `TaskExecutor`:**

The `executor.get_task(TaskId)` method must be added so the runner can look up the
fallback task's `AnalyzedTaskAction` at runtime. Check the actual field name via:
`grep "workflow\|tasks" tools/nika-engine/src/runtime/executor/mod.rs | head -20`

```rust
impl TaskExecutor {
    /// Look up an analyzed task by ID (for on_error: fallback execution).
    pub fn get_task(&self, id: TaskId) -> Option<&AnalyzedTask> {
        self.workflow.tasks.iter().find(|t| t.id == id)
    }
}
```

If `TaskExecutor` does not hold the workflow directly, the alternative is to pass
the workflow's task list (or a lookup closure) into `execute_task_iteration()`.

---

### EXACT CODE: Re-exports (nika-core/src/ast/analyzed/mod.rs)

Update the `pub use task::` line (line 64-68):

```rust
pub use task::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach,
    AnalyzedInferAction, AnalyzedInvokeAction, AnalyzedOnError, AnalyzedOutput, AnalyzedRetry,
    AnalyzedTask, AnalyzedTaskAction, HttpMethod, OnErrorAction, OutputFormat,
};
```

Update the `test_module_exports` test to include `on_error: None` in the `AnalyzedTask`
literal (after `retry: None,` on line 99).

---

### Interaction with existing `routing:` config

The `routing:` config (defined in `nika-core/src/ast/routing.rs`) handles **provider-level
fallback chains** — a list of providers to try in order for the SAME action. This operates
INSIDE the task execution via `executor.execute_with_routing()`.

The `on_error:` config operates at a HIGHER level — it fires AFTER the entire execution
(including all routing fallbacks and task-level retries) has failed.

**Layered recovery model:**

```
                   execute_task_iteration()
                   |
                   |  task-level retry loop
                   |  max_attempts x delay_ms x backoff
                   |  |
                   |  |  execute_with_routing()
                   |  |  routing: { fallback: [A, B, C] }
                   |  |  |
                   |  |  |  provider auto-retry (4x)
                   |  |  |  (500, 502, 503, 429)
                   |  |  |
                   |  |  (all providers in chain exhausted)
                   |  |
                   |  (all task-level retry attempts exhausted)
                   |
                   v ALL of the above failed
                   |
                   |  on_error: dispatch (ONCE, no further retry)
                   |  ignore     -> null output, mark succeeded
                   |  retry_with -> one provider call, no routing chain
                   |  fallback   -> execute different task action once
```

**Worst-case call count** with all 3 layers:
- `retry: { max_attempts: 3 }` + `routing: { fallback: [A, B, C] }` + `on_error: { retry_with_provider: D }`
- = 3 attempts x 3 providers x 4 auto-retries = 36 provider calls... then on_error fires for 1 more = 37 total
- This is documented intentionally. The RETRY COMPOUNDING comment at line 1316 already warns about this.

**Combined `routing:` + `on_error: { retry_with_provider: }` example:**
```yaml
- id: generate
  provider: [anthropic, openai]  # routing fallback: try anthropic, then openai
  retry: { max_attempts: 2 }     # retry the routing chain twice
  infer: "Write: {{inputs.text}}"
  on_error:
    retry_with_provider: groq    # last resort after routing + retry both exhaust
```

**Order of operations:**
1. Execute with `anthropic` (provider auto-retry 4x on 500/429)
2. Routing fallback to `openai` (provider auto-retry 4x)
3. Task retry: repeat steps 1-2
4. All retries exhausted -> `on_error: { retry_with_provider: groq }` fires ONCE
5. If groq also fails -> task definitively fails -> NIKA-026 cascades to dependents

The `on_error: { retry_with_provider: }` deliberately bypasses the routing chain
and the retry loop. It calls `executor.execute()` directly (NOT `execute_with_routing()`),
giving the fallback provider exactly one shot.

---

### Critical Details (expanded from base plan)

- **Depth limit 1**: The `is_fallback_execution: bool` parameter on `execute_task_iteration()`
  is `false` for normal execution and `true` when re-entering for `on_error: { fallback: }`.
  When `true`, the on_error block is skipped entirely (line: `if !is_fallback_execution {`).
  This prevents A -> B -> C -> ... infinite chains.

- **for_each**: The `on_error` config lives on the `AnalyzedTask` which is `Arc`-shared
  across all iterations. Each iteration independently evaluates the on_error path.
  Mixed results array: some items have primary output, some have fallback/null output.
  The `ForEachItemFailed` event fires for the primary failure, then `TaskFallbackTriggered`
  fires, then `ForEachItemCompleted` fires if fallback succeeded.

- **Event ordering (ignore case)**:
  `TaskStarted -> TaskFailed -> TaskFallbackTriggered(success=true) -> TaskCompleted`

- **Event ordering (fallback also fails)**:
  `TaskStarted -> TaskFailed -> TaskFallbackTriggered(success=false) -> TaskFailed`
  Two `TaskFailed` events: one for primary, one for fallback. TUI handlers
  should use the LAST event for status display. The `TaskFallbackTriggered`
  event provides the `success: bool` flag for disambiguation.

- **`ignore: true` + downstream**: `$ignored_task` resolves to `Value::Null`.
  Guard with `{{ with.result | default("fallback") }}`. This is consistent with
  how `when:` skipped tasks produce null output.

- **Error code NIKA-290**: Fires at ANALYSIS time (Phase 2), not runtime.
  The fallback task must exist in the workflow. Self-referencing (task fallbacks
  to itself) is allowed and handled by the depth-1 limit. Circular fallback
  chains (A.fallback=B, B.fallback=A) are safe because the `is_fallback_execution`
  flag prevents re-entering on_error when executing as a fallback.

- **`on_error: { fallback: }` + `depends_on:`**: The fallback task reference
  does NOT create a DAG dependency. The fallback task can run independently
  or even run before the task that references it. The fallback action is
  cloned and re-executed at runtime, not awaited as a dependency.

---

### TDD Sequence (9 tests)

All tests in `tools/nika-engine/src/runtime/runner.rs` (append to existing `#[cfg(test)] mod tests`),
except test 9 which belongs in `tools/nika-core/src/ast/analyzer/analyze.rs`.

```rust
/// 1. on_error: { ignore: true } produces null output
#[tokio::test]
async fn test_on_error_ignore_returns_null_output() {
    // Setup: workflow with one task that has a deliberately failing action
    // (e.g., exec: "false") and on_error: { ignore: true }.
    // Assert: task result is_success() == true, output == Value::Null.
}

/// 2. on_error: { ignore: true } does NOT cascade NIKA-026 to downstream
#[tokio::test]
async fn test_on_error_ignore_does_not_cascade() {
    // Setup: task_a (fails + ignore) -> task_b depends on task_a.
    // Assert: task_b runs successfully (receives null from task_a).
    // Assert: NO TaskSkipped event for task_b.
}

/// 3. on_error: { retry_with_provider: mock } succeeds when primary fails
#[tokio::test]
async fn test_on_error_retry_with_provider_succeeds() {
    // Setup: task with provider=mock_failing, on_error: { retry_with_provider: mock }.
    // Primary fails, fallback to mock provider succeeds.
    // Assert: task result is_success() == true.
    // Assert: output is the mock provider's response.
}

/// 4. on_error: { retry_with_provider: } also fails -> definitive failure
#[tokio::test]
async fn test_on_error_retry_with_provider_also_fails() {
    // Setup: both primary and fallback providers produce errors.
    // Assert: task result is_success() == false.
    // Assert: error message contains both primary and fallback errors.
}

/// 5. on_error: { fallback: other_task } uses the other task's action
#[tokio::test]
async fn test_on_error_fallback_uses_fallback_action() {
    // Setup: task_a (fails, on_error: { fallback: task_b }).
    // task_b has a different infer prompt that succeeds.
    // Assert: task_a result contains task_b's action output.
    // Assert: stored under task_a's ID (not task_b's).
}

/// 6. on_error: { ignore: true } with for_each produces mixed array
#[tokio::test]
async fn test_on_error_fallback_with_for_each() {
    // Setup: for_each over 3 items, item[1] fails, on_error: { ignore: true }.
    // Assert: result array has 3 elements.
    // Assert: result[0] and result[2] are successful outputs.
    // Assert: result[1] is Value::Null (the ignored failure).
}

/// 7. retry: + on_error: — retry fires first, on_error fires after exhaustion
#[tokio::test]
async fn test_on_error_combined_with_retry() {
    // Setup: task with retry: { max_attempts: 2 } and on_error: { ignore: true }.
    // Task action always fails.
    // Assert: TaskRetry event emitted (attempt 2).
    // Assert: TaskFallbackTriggered event emitted AFTER retries.
    // Assert: final result is_success() == true (via ignore).
}

/// 8. on_error: emits TaskFallbackTriggered event with correct fields
#[tokio::test]
async fn test_on_error_emits_fallback_event() {
    // Setup: task with on_error: { ignore: true }, action fails.
    // Assert: EventLog contains TaskFallbackTriggered event.
    // Assert: event.action == "ignore", event.success == true.
    // Assert: event ordering: TaskFailed appears BEFORE TaskFallbackTriggered.
}

/// 9. on_error: { fallback: "nonexistent" } rejected at analysis time (NIKA-290)
#[test]
fn test_on_error_unknown_fallback_rejected() {
    // Location: tools/nika-core/src/ast/analyzer/analyze.rs tests
    // Setup: parse + analyze a workflow with on_error: { fallback: "ghost_task" }.
    // Assert: AnalyzeResult.is_err() == true.
    // Assert: error kind == AnalyzeErrorKind::UnknownOnErrorFallback.
    // Assert: error code == "NIKA-290".
}
```

---

### Estimate: ~700 LOC, 2-3 hours implementation + 1 hour tests

---

## FEATURE 2: Scheduling / Cron (v0.72)

### What
`nika schedule add workflow.nika.yaml --cron "0 */6 * * *"` — first-class cron schedules.

### Key Discovery
**The cron scheduler ALREADY EXISTS** in `nika-daemon/src/services/jobs.rs:467-554` — `run_cron_scheduler()` + `fire_due_cron_jobs()` with overlap protection. It's spawned in `server.rs:162`. The `Job.cron` column exists since V1. What's MISSING: schedules are not first-class entities (no `schedules` table, no `nika schedule` CLI, no timezone, no pause/resume).

### CLI Commands
```bash
nika schedule add report.nika.yaml --cron "0 */6 * * *" --name "6h-report"
nika schedule add daily.nika.yaml --cron "@daily" --tz "Europe/Paris"
nika schedule list [--json]
nika schedule get <ID>
nika schedule remove <ID>
nika schedule pause <ID>
nika schedule resume <ID>
```

### Schema V5: `schedules` Table
```sql
CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY,
    name TEXT,
    workflow TEXT NOT NULL,
    args TEXT,
    cron TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    enabled INTEGER NOT NULL DEFAULT 1,
    max_retries INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    last_run_at TEXT,
    next_run_at TEXT,
    run_count INTEGER NOT NULL DEFAULT 0,
    last_job_id TEXT,
    tags TEXT
);
CREATE INDEX IF NOT EXISTS idx_schedules_enabled ON schedules(enabled);
CREATE INDEX IF NOT EXISTS idx_schedules_next_run ON schedules(next_run_at);
```

### Files to Modify/Create

| File | Change |
|------|--------|
| `nika-storage/src/lib.rs` | `CronSchedule` struct, V5 migration, 6 DbCommand variants, 6 Storage methods |
| `nika-daemon/src/services/jobs.rs` | Refactor `fire_due_cron_jobs` to read `schedules` table + timezone |
| `nika-daemon/src/protocol.rs` | 6 new `DaemonRequest` + 3 `DaemonResponse` variants |
| `nika-daemon/src/server.rs` | 6 dispatch branches |
| `nika-cli/src/schedule.rs` | **CREATE** — `ScheduleAction` enum + handler (~200 LOC) |
| `nika-cli/src/lib.rs` | Add `#[cfg(unix)] pub mod schedule;` |
| `nika/src/cli/mod.rs` | Re-export schedule |
| `nika/src/main.rs` | Add `Schedule` command + dispatch |
| `tools/Cargo.toml` | Add `chrono-tz = "0.10"` workspace dep |

### Critical Details
- **Overlap protection**: already exists — skip if previous run still pending/running.
- **`@` shortcuts**: `croner 3` supports `@daily`, `@hourly`, `@weekly`, `@monthly`.
- **Timezone**: `chrono-tz` crate, IANA names. Default UTC. Invalid tz → fallback UTC + warn.
- **`next_run_at`**: recomputed from `now` after each fire (avoids drift).
- **Daemon restart**: schedules persist in SQLite. Missed runs fire on next tick.
- **`nika schedule remove`**: does NOT cancel running jobs, only prevents future firings.

### Estimate: ~720 LOC, 3-4 hours across 5 phases

---

## FEATURE 3: Multi-Tenant Auth (v0.73)

### What
Multiple API keys with names, expiry, scopes. Replace single `NIKA_SERVE_TOKEN`.

### Three Levels (ship L1 first)
- **L1 Multi-key** (2-3 days): named API keys + optional expiry. Jungo gets its own key.
- **L2 Scoped** (1-2 days): keys restricted to specific workflow patterns.
- **L3 Full RBAC** (3-5 days, post-launch): users, roles (admin/operator/viewer), audit log.

### Schema V5 (or V6 if scheduling ships first): `serve_tokens` Table
```sql
CREATE TABLE serve_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash BLOB NOT NULL UNIQUE,   -- 32 bytes BLAKE3(raw_token)
    role TEXT NOT NULL DEFAULT 'operator',
    scope TEXT NOT NULL DEFAULT '*',    -- '*' or 'wf1.nika.yaml,wf2-*.nika.yaml'
    created_at TEXT NOT NULL,
    expires_at TEXT,
    last_used_at TEXT,
    revoked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_serve_tokens_hash ON serve_tokens(token_hash);
```

### Token Flow
```
Token created → BLAKE3 hash stored in DB → raw token shown ONCE to user
Request arrives → SHA-256 the bearer token → lookup in DashMap cache (60s TTL)
Cache miss → SELECT by token_hash → check expiry + revoked → build Principal
Principal attached to request extensions → handlers read scope/role
```

### Legacy Migration (ZERO downtime)
```
serve_tokens rows = 0 AND NIKA_SERVE_TOKEN set → legacy single-token mode (existing behavior)
serve_tokens rows > 0 → multi-key mode (NIKA_SERVE_TOKEN ignored)
```

### CLI Commands
```bash
nika serve token add --name "jungo-prod" [--expires 2026-12-31] [--scope "jungo-*.nika.yaml"]
nika serve token list
nika serve token revoke <id-or-name>
```

### Files to Modify/Create

| File | Change |
|------|--------|
| `nika-storage/src/tokens.rs` | **CREATE** — `TokenEntry`, CRUD, audit log (L3) |
| `nika-storage/src/lib.rs` | Schema migration, `mod tokens` |
| `nika-serve/src/auth.rs` | Rewrite: `TokenStore` + `Principal` + legacy fallback |
| `nika-serve/src/state.rs` | Add `token_store: TokenStore` to `AppState` |
| `nika-serve/src/config.rs` | `auth_token: Option<String>`, startup validation |
| `nika-serve/src/error.rs` | Add `ServeError::Forbidden` (403) |
| `nika-serve/src/routes/tokens.rs` | **CREATE** — POST/GET/DELETE/PATCH endpoints |
| `nika-cli/src/serve_token.rs` | **CREATE** — CLI commands |

### Critical Details
- **BLAKE3 hash** (not raw token) stored in DB. DB dump != credential dump.
- **DashMap cache** (60s TTL) avoids DB roundtrip per request. Invalidated on revoke.
- **Rate limiter key** changes from raw token string to `principal.token_id` (UUID).
- **403 Forbidden** (scoped key can't run this workflow) vs 401 Unauthorized (bad token).

### Estimate: L1 = 2-3 days, L2 = 1-2 days, L3 = 3-5 days

---

## FEATURE 4: Observability UI (v0.74)

### What
Web-based trace viewer embedded in `nika serve`, accessible at `http://localhost:3000/ui`.
Parses the existing NDJSON traces (58 EventKind variants across 15 categories),
aggregates cost/tokens/latency per task, and serves an SPA with four views --
zero external dependencies, zero npm, zero Grafana.

### Architecture
**Embedded SPA in the binary** via `rust-embed`. No separate process, no Grafana, no npm.

```
nika serve                   nika-obs crate
   |                            |
   |  /ui              -->  rust-embed serves index.html + JS
   |  /v1/traces       -->  list_traces() reads .nika/traces/*.ndjson
   |  /v1/traces/{id}  -->  parse_trace() deserializes NDJSON -> Vec<Event>
   |  /v1/traces/{id}/summary -->  aggregate() single-pass -> TraceSummary
   |  /v1/jobs/{id}/trace     -->  storage lookup -> generation_id -> redirect
   |
   |  Worker post-exec  -->  parse trace -> feed Prometheus counters
```

---

### New Crate: `nika-obs`

#### Directory Layout
```
tools/nika-obs/
+-- Cargo.toml
+-- src/
|   +-- lib.rs            # Public API: re-exports, TraceSummary, TaskSummary
|   +-- parser.rs         # parse_trace() NDJSON -> Vec<Event>
|   +-- aggregator.rs     # aggregate() single-pass Vec<Event> -> TraceSummary
|   +-- routes.rs         # Axum handlers + rust-embed asset serving
+-- assets/
    +-- index.html        # SPA shell (4 views, vanilla JS, no bundler)
    +-- app.js            # Router + view controller
    +-- dag.js            # SVG DAG renderer (Sugiyama layout)
    +-- waterfall.js      # Canvas task timeline / Gantt chart
    +-- cost.js           # Cost dashboard with stacked bar charts
    +-- live.js           # EventSource -> real-time task status cards
    +-- style.css         # Solarized CSS variables mapped from TUI palette
```

#### Cargo.toml
```toml
[package]
name = "nika-obs"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Observability UI and trace analysis for Nika workflow engine"
publish = true

[dependencies]
nika-event = { workspace = true }

axum = { workspace = true }
rust-embed = { version = "8", features = ["compression"] }
mime_guess = "2"
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "test-util"] }
pretty_assertions = { workspace = true }
tempfile = { workspace = true }
```

#### Workspace Changes (tools/Cargo.toml)
```toml
# Add to [workspace] members:
"nika-obs",

# Add to [workspace.dependencies]:
nika-obs = { path = "nika-obs", version = "0.70.0" }
rust-embed = { version = "8", features = ["compression"] }
mime_guess = "2"
```

---

### lib.rs -- Public API

```rust
//! Observability UI and trace analysis for Nika workflow engine.
//!
//! Provides:
//! - NDJSON trace parsing (`parser::parse_trace`)
//! - Single-pass aggregation into `TraceSummary` (`aggregator::aggregate`)
//! - Axum route handlers for `/ui` and `/v1/traces` endpoints
//! - Embedded SPA assets via `rust-embed`

pub mod aggregator;
pub mod parser;
pub mod routes;

// Re-export primary types for consumers
pub use aggregator::{TaskSummary, TraceSummary};
pub use parser::parse_trace;

/// Information about a discovered trace file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEntry {
    /// Generation ID (filename stem of the .ndjson file)
    pub generation_id: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// ISO-8601 creation timestamp (from filesystem metadata)
    pub created_at: Option<String>,
    /// Number of events in the trace (populated only when full parse is requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_count: Option<usize>,
}

/// Aggregated summary of a single workflow execution.
///
/// Built by `aggregator::aggregate()` in a single pass over `Vec<Event>`.
/// Every field is derived from the 58 EventKind variants -- no external data needed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceSummary {
    /// Generation ID for this trace
    pub generation_id: String,
    /// Workflow name (from WorkflowStarted event, if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    /// Nika version that produced this trace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nika_version: Option<String>,
    /// Workflow file hash (xxh3, for cache invalidation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_hash: Option<String>,
    /// Total task count declared at workflow start
    pub task_count: usize,

    // -- Timing --
    /// Wall-clock duration from WorkflowStarted to WorkflowCompleted/Failed (ms)
    pub duration_ms: u64,
    /// Earliest event timestamp_ms
    pub start_ms: u64,
    /// Latest event timestamp_ms
    pub end_ms: u64,

    // -- Cost --
    /// Total estimated cost across all ProviderResponded events (USD)
    pub total_cost_usd: f64,

    // -- Tokens --
    /// Sum of input_tokens from all ProviderResponded events
    pub total_input_tokens: u64,
    /// Sum of output_tokens from all ProviderResponded events
    pub total_output_tokens: u64,
    /// Sum of cache_read_tokens from all ProviderResponded events
    pub total_cache_read_tokens: u64,

    // -- Outcome --
    /// "completed", "failed", or "aborted"
    pub outcome: String,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Failed task ID (from WorkflowFailed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_task: Option<String>,

    // -- Per-task breakdown --
    /// One entry per task that emitted TaskStarted
    pub tasks: Vec<TaskSummary>,

    // -- Provider breakdown (for cost dashboard) --
    /// Tokens and cost grouped by provider+model
    pub providers: Vec<ProviderSummary>,

    // -- Structured output stats --
    /// Number of structured output attempts across all tasks
    pub structured_attempts: u32,
    /// Number of structured output successes
    pub structured_successes: u32,
    /// Number of structured output repair invocations (Layer 4)
    pub structured_repairs: u32,

    // -- MCP stats --
    /// Total MCP tool invocations
    pub mcp_calls: u32,
    /// Total MCP call duration (ms)
    pub mcp_duration_ms: u64,

    // -- Agent stats --
    /// Total agent turns across all agent tasks
    pub agent_turns: u32,

    // -- Artifact stats --
    /// Number of artifacts written
    pub artifacts_written: u32,
    /// Total artifact bytes
    pub artifacts_bytes: u64,

    // -- Event totals --
    /// Total number of events in the trace
    pub event_count: usize,
}

/// Per-task summary extracted from the event stream.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskSummary {
    pub task_id: String,
    /// Verb: "infer", "exec", "fetch", "invoke", "agent"
    pub verb: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub duration_ms: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    /// "completed", "failed", "skipped", "cancelled"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_each: Option<ForEachSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_layer: Option<u8>,
    pub retry_count: u32,
    pub mcp_calls: u32,
    pub agent_turns: u32,
}

/// for_each iteration summary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForEachSummary {
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub skipped: u32,
    pub concurrency: usize,
}

/// Per-provider aggregation for cost dashboard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSummary {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    pub call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_ttft_ms: Option<u64>,
}
```

---

### parser.rs -- parse_trace()

```rust
//! Parse NDJSON trace files into Vec<Event>.
//!
//! Each line is a JSON-serialized `nika_event::log::Event`.
//! Malformed lines are skipped with a warning (resilient parsing).

use std::io::{BufRead, BufReader};
use std::path::Path;
use nika_event::log::Event;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("trace file not found: {0}")]
    NotFound(String),
    #[error("I/O error reading trace: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse a trace file from disk. Resilient: skips malformed lines.
/// Returns events sorted by (id, timestamp_ms).
pub fn parse_trace(path: &Path) -> Result<Vec<Event>, ParseError> {
    if !path.exists() {
        return Err(ParseError::NotFound(path.display().to_string()));
    }
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::with_capacity(256);
    let mut skipped = 0u32;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(line = line_num + 1, error = %e, "failed to read trace line");
                skipped += 1;
                continue;
            }
        };
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<Event>(&line) {
            Ok(event) => events.push(event),
            Err(e) => {
                tracing::warn!(line = line_num + 1, error = %e, "skipping malformed event");
                skipped += 1;
            }
        }
    }
    if skipped > 0 {
        tracing::warn!(skipped, total = events.len() + skipped as usize, "trace parsed with skips");
    }
    events.sort_by_key(|e| (e.id, e.timestamp_ms));
    Ok(events)
}

/// Parse from raw NDJSON string (for testing and in-memory traces).
pub fn parse_trace_str(ndjson: &str) -> Vec<Event> {
    let mut events = Vec::new();
    for line in ndjson.lines() {
        if line.trim().is_empty() { continue; }
        if let Ok(event) = serde_json::from_str::<Event>(line) {
            events.push(event);
        }
    }
    events.sort_by_key(|e| (e.id, e.timestamp_ms));
    events
}
```

---

### aggregator.rs -- aggregate()

**Event -> Field Mapping (all 58 EventKind variants, grouped by category):**

| Category | EventKind | Fields Updated |
|----------|-----------|----------------|
| **Workflow (6)** | `WorkflowStarted` | task_count, nika_version, workflow_hash, start_ms |
| | `WorkflowCompleted` | duration_ms, outcome="completed" |
| | `WorkflowFailed` | outcome="failed", error, failed_task |
| | `WorkflowAborted` | outcome="aborted" |
| | `WorkflowPaused` | (no-op) |
| | `WorkflowResumed` | (no-op) |
| **Task (5)** | `TaskScheduled` | task.dependencies |
| | `TaskStarted` | task.verb, task.start_ms |
| | `TaskCompleted` | task.duration_ms, task.status="completed" |
| | `TaskFailed` | task.status="failed", task.error, task.error_code |
| | `TaskSkipped` | task.status="skipped" |
| **Fine-Grained (4)** | `TemplateResolved` | (no-op) |
| | `ProviderCalled` | task.provider, task.model |
| | `ProviderResponded` | task+global tokens/cost/ttft; provider rollup |
| | `ProviderFallback` | (no-op) |
| **Context (1)** | `ContextAssembled` | (no-op) |
| **MCP (5)** | `McpInvoke` | task.mcp_calls++, global mcp_calls++ |
| | `McpResponse` | global mcp_duration_ms |
| | `McpConnected` | (no-op) |
| | `McpError` | (no-op) |
| | `McpRetry` | (no-op) |
| **Agent (4)** | `PresetApplied` | (no-op) |
| | `AgentStart` | (via task verb) |
| | `AgentTurn` | task.agent_turns++, global agent_turns++ |
| | `AgentComplete` | (via TaskCompleted) |
| **Nested Agent (1)** | `AgentSpawned` | (no-op) |
| **Record (2)** | `RecordCreated` | (no-op) |
| | `RecordSkipped` | (no-op) |
| **Guardrail (3)** | `GuardrailPassed/Failed/Escalation` | (no-op) |
| **Builtin (2)** | `Log`, `Custom` | (no-op) |
| **Artifact (2)** | `ArtifactWritten` | artifacts_written++, artifacts_bytes += size |
| | `ArtifactFailed` | (no-op) |
| **Media (5)** | all 5 | (no-op) |
| **Structured (3)** | `StructuredOutputAttempt` | structured_attempts++; layer==4 -> repairs++ |
| | `StructuredOutputSuccess` | structured_successes++, task.structured_layer |
| | `StructuredOutputTimeout` | (via TaskFailed) |
| **Vision (1)** | `VisionContentResolved` | (no-op) |
| **HTTP (2)** | `HttpRequest/Response` | (no-op) |
| **Cleanup (1)** | `MediaCleanup` | (no-op) |
| **Exec (1)** | `ExecCompleted` | (via TaskCompleted) |
| **Fetch (2)** | `FetchRetry/Exhausted` | (no-op) |
| **Retry (1)** | `TaskRetry` | task.retry_count++ |
| **Provider Lifecycle (3)** | `ProviderAutoRetried/Initialized/BuiltinToolInvoked` | (no-op) |
| **Streaming (1)** | `StreamingDelta` | (no-op) |
| **Orchestrator (5)** | all 5 | (no-op) |
| **Extract (1)** | `ExtractApplied` | (no-op) |
| **Security (1)** | `SecurityScanFinding` | (no-op) |
| **for_each (5)** | `ForEachStarted` | task.for_each init |
| | `ForEachCompleted` | task.for_each succeeded/failed/skipped |
| | `ForEachItem*` (3) | (no-op) |
| **Cancellation (1)** | `TaskCancelled` | task.status="cancelled" |
| **Fallback (2)** | `FallbackTriggered/ChainExhausted` | (no-op -- in TaskFailed) |
| **Boot (2)** | `BootPhaseCompleted/NativeModelLoaded` | (no-op) |
| **Budget (2)** | `BudgetOk/Exceeded` | (no-op) |
| **Binding (4)** | all 4 | (no-op) |
| **Decompose (2)** | `DecomposeStarted/Completed` | (no-op) |
| **Policy (1)** | `PolicyBlocked` | (no-op) |

```rust
//! Single-pass aggregation. O(n) time, O(tasks + providers) memory.

use std::collections::HashMap;
use nika_event::log::{Event, EventKind};
use crate::{ForEachSummary, ProviderSummary, TaskSummary, TraceSummary};

/// Aggregate a parsed trace into a TraceSummary in a single pass.
pub fn aggregate(generation_id: &str, events: &[Event]) -> TraceSummary {
    let mut tasks: HashMap<String, TaskAcc> = HashMap::new();
    let mut providers: HashMap<ProviderKey, ProviderAcc> = HashMap::new();
    // ... 20 workflow-level accumulators ...

    for event in events {
        start_ms = start_ms.min(event.timestamp_ms);
        end_ms = end_ms.max(event.timestamp_ms);

        match &event.kind {
            EventKind::WorkflowStarted { task_count: tc, workflow_hash: wh, nika_version: nv, .. } => {
                task_count = *tc; nika_version = Some(nv.clone()); workflow_hash = Some(wh.clone());
            }
            EventKind::WorkflowCompleted { total_duration_ms, .. } => {
                duration_ms = *total_duration_ms; outcome = "completed".into();
            }
            EventKind::WorkflowFailed { error: e, failed_task: ft } => {
                outcome = "failed".into(); error = Some(e.clone());
                failed_task = ft.as_ref().map(|s| s.to_string());
            }
            EventKind::WorkflowAborted { .. } => { outcome = "aborted".into(); }
            EventKind::TaskScheduled { task_id, dependencies: deps } => { /* set deps */ }
            EventKind::TaskStarted { task_id, verb, .. } => { /* create/update TaskAcc */ }
            EventKind::TaskCompleted { task_id, duration_ms: d, .. } => { /* finalize task */ }
            EventKind::TaskFailed { task_id, error: e, duration_ms: d, error_code: ec } => { /* ... */ }
            EventKind::TaskSkipped { task_id, .. } => { /* status = "skipped" */ }
            EventKind::TaskCancelled { task_id, .. } => { /* status = "cancelled" */ }
            EventKind::ProviderCalled { task_id, provider, model, .. } => { /* set provider/model */ }
            EventKind::ProviderResponded { task_id, input_tokens, output_tokens,
                cache_read_tokens, ttft_ms, cost_usd, .. } => {
                // Update task-level + global totals + provider rollup
            }
            EventKind::McpInvoke { task_id, .. } => { mcp_calls += 1; /* task.mcp_calls++ */ }
            EventKind::McpResponse { duration_ms: d, .. } => { mcp_duration_ms += d; }
            EventKind::AgentTurn { task_id, .. } => { agent_turns += 1; /* task.agent_turns++ */ }
            EventKind::StructuredOutputAttempt { layer, .. } => {
                structured_attempts += 1; if *layer == 4 { structured_repairs += 1; }
            }
            EventKind::StructuredOutputSuccess { task_id, layer, .. } => {
                structured_successes += 1; /* task.structured_layer = layer */
            }
            EventKind::ArtifactWritten { size, .. } => { artifacts_written += 1; artifacts_bytes += size; }
            EventKind::ForEachStarted { task_id, item_count, concurrency, .. } => { /* init for_each */ }
            EventKind::ForEachCompleted { task_id, succeeded, failed, skipped, .. } => { /* fill for_each */ }
            EventKind::TaskRetry { task_id, .. } => { /* task.retry_count++ */ }
            _ => {} // 30+ other variants: no-op for summary (detail in full trace)
        }
    }

    // Finalize: default start_ms, compute duration from bounds, sort tasks by start_ms,
    // sort providers by cost desc, build and return TraceSummary
}
```

---

### routes.rs -- Axum Handlers + rust-embed

```rust
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::Router;
use axum::routing::get;
use rust_embed::Embed;

/// Embedded SPA assets. Debug: reads from disk. Release: brotli-compressed in binary.
#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

async fn serve_index() -> Response { /* Assets::get("index.html") */ }
async fn serve_asset(Path(path): Path<String>) -> Response { /* Assets::get or SPA fallback */ }

#[derive(serde::Deserialize)]
pub struct ListTracesQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,   // default: 50
    #[serde(default)]
    pub offset: usize,
}

/// GET /v1/traces -- list trace files, newest first
pub async fn list_traces(Query(params): Query<ListTracesQuery>)
    -> Result<Json<Vec<crate::TraceEntry>>, StatusCode> {
    // nika_event::trace::list_traces() reads .nika/traces/
}

/// GET /v1/traces/{id} -- full parsed trace as JSON array of Event
pub async fn get_trace(Path(id): Path<String>)
    -> Result<Json<Vec<nika_event::log::Event>>, StatusCode> {
    // Validate id, then parser::parse_trace()
}

/// GET /v1/traces/{id}/summary -- aggregated TraceSummary
pub async fn get_trace_summary(Path(id): Path<String>)
    -> Result<Json<crate::TraceSummary>, StatusCode> {
    // parse_trace() then aggregator::aggregate()
}

/// GET /v1/jobs/{id}/trace -- link job_id to trace, redirect to summary
pub async fn get_job_trace(Path(job_id): Path<String>, State(state): State</*AppState*/>)
    -> Result<Response, StatusCode> {
    // Scan first line of each trace for job_id match -> redirect
    // Phase 2: O(1) via Job.tags["generation_id"]
}

fn is_valid_trace_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && !id.contains('/')
        && !id.contains('\\')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == 'T')
}

pub fn build_obs_router() -> Router {
    Router::new()
        .route("/ui", get(serve_index))
        .route("/ui/{*path}", get(serve_asset))
        .route("/v1/traces", get(list_traces))
        .route("/v1/traces/{id}", get(get_trace))
        .route("/v1/traces/{id}/summary", get(get_trace_summary))
}
```

---

### Enhanced Prometheus Metrics

New functions in `nika-serve/src/metrics.rs`:

```rust
/// Record provider token usage (called from worker post-exec trace parse)
pub fn record_provider_tokens(provider: &str, model: &str, input: u64, output: u64, cache_read: u64) {
    metrics::counter!("nika_provider_tokens_total",
        "provider" => provider.to_string(), "model" => model.to_string(), "direction" => "input",
    ).increment(input);
    metrics::counter!("nika_provider_tokens_total",
        "provider" => provider.to_string(), "model" => model.to_string(), "direction" => "output",
    ).increment(output);
    if cache_read > 0 {
        metrics::counter!("nika_provider_tokens_total",
            "provider" => provider.to_string(), "model" => model.to_string(), "direction" => "cache_read",
        ).increment(cache_read);
    }
}

/// Record provider cost (histogram -- metrics 0.24 counter only supports u64)
pub fn record_provider_cost(provider: &str, model: &str, cost_usd: f64) {
    metrics::histogram!("nika_provider_cost_usd",
        "provider" => provider.to_string(), "model" => model.to_string(),
    ).record(cost_usd);
}

/// Record task duration by verb
pub fn record_task_duration(verb: &str, duration_secs: f64) {
    metrics::histogram!("nika_task_duration_seconds", "verb" => verb.to_string()).record(duration_secs);
}

/// Record structured output metrics
pub fn record_structured_output(attempts: u32, successes: u32, repairs: u32) {
    metrics::counter!("nika_structured_attempts_total").increment(attempts as u64);
    metrics::counter!("nika_structured_successes_total").increment(successes as u64);
    metrics::counter!("nika_structured_repairs_total").increment(repairs as u64);
}
```

**Full Prometheus surface after v0.74 (4 existing + 6 new = 10 metrics):**
```
# Existing
nika_jobs_total{status}                                  counter
nika_jobs_active                                         gauge
nika_job_duration_seconds                                histogram
nika_http_requests_total{method,path,status}             counter

# New
nika_provider_tokens_total{provider,model,direction}     counter   (input/output/cache_read)
nika_provider_cost_usd{provider,model}                   histogram (per-call USD)
nika_task_duration_seconds{verb}                         histogram (infer/exec/fetch/invoke/agent)
nika_structured_attempts_total                           counter
nika_structured_successes_total                          counter
nika_structured_repairs_total                            counter
```

---

### Worker Post-Execution Trace Parsing

**Modification point:** `nika-serve/src/worker.rs`, after `record_job_completed()` (~line 267)

```rust
// NEW: Parse trace -> feed Prometheus metrics (best-effort, non-blocking)
if let Some(gen_id) = extract_generation_id_from_output(&exec_result.output) {
    let trace_path = config.workflows_dir.join(".nika/traces").join(format!("{gen_id}.ndjson"));
    match nika_obs::parser::parse_trace(&trace_path) {
        Ok(events) => {
            let summary = nika_obs::aggregator::aggregate(&gen_id, &events);
            for p in &summary.providers {
                crate::metrics::record_provider_tokens(&p.provider, &p.model,
                    p.input_tokens, p.output_tokens, p.cache_read_tokens);
                crate::metrics::record_provider_cost(&p.provider, &p.model, p.cost_usd);
            }
            for t in &summary.tasks {
                crate::metrics::record_task_duration(&t.verb, t.duration_ms as f64 / 1000.0);
            }
            crate::metrics::record_structured_output(
                summary.structured_attempts, summary.structured_successes, summary.structured_repairs);
        }
        Err(e) => { tracing::debug!(error = %e, "trace parse for metrics failed (non-fatal)"); }
    }
}
```

Phase 2: Store `generation_id` in `Job.tags` at spawn time via `NIKA_GENERATION_ID` env var.

---

### Exact Modification Points in nika-serve

| # | File | Line/Location | Change |
|---|------|--------------|--------|
| 1 | `tools/Cargo.toml` | `members = [` (line 3) | Add `"nika-obs"` to workspace members |
| 2 | `tools/Cargo.toml` | `[workspace.dependencies]` (line 33) | Add `nika-obs`, `rust-embed`, `mime_guess` |
| 3 | `tools/nika-serve/Cargo.toml` | `[dependencies]` (line 12) | Add `nika-obs = { workspace = true }` |
| 4 | `tools/nika-serve/src/routes/mod.rs` | After `.finish_api_with()` (line 67) | `.merge(nika_obs::routes::build_obs_router())` |
| 5 | `tools/nika-serve/src/metrics.rs` | After `record_http_request()` (line 72) | Add 4 new `record_*` functions |
| 6 | `tools/nika-serve/src/worker.rs` | After `record_job_completed()` (~line 267) | Add trace parse + metrics feed block |

---

### HTML/JS Architecture -- 4 Views

| View | Hash Route | Data Source | Visualization |
|------|------------|-------------|---------------|
| **Trace List** | `#/traces` | `GET /v1/traces` | Sortable table (gen_id, date, duration, cost, outcome) |
| **Trace Detail** | `#/traces/{id}` | `GET /v1/traces/{id}/summary` + `/v1/traces/{id}` | DAG (SVG Sugiyama) + Waterfall (Canvas Gantt) + Events + Cost sidebar |
| **Cost Dashboard** | `#/cost` | `GET /v1/traces` (summaries) | Stacked bar (daily cost/provider) + Model table + Top workflows |
| **Live Monitor** | `#/live` | `EventSource /v1/events/{id}` | Task cards (verb color) + Event scroll |

**JS modules (ES modules, no bundler, no npm):**

| File | ~LOC | Purpose |
|------|------|---------|
| `app.js` | 150 | Hash router, view switching, fetch with auth, theme toggle |
| `dag.js` | 250 | dependencies -> adjacency list -> Sugiyama layering -> SVG rects + bezier edges |
| `waterfall.js` | 200 | Canvas Gantt: tasks as bars, x=time, color=verb, tooltip=tokens/cost/ttft |
| `cost.js` | 200 | Stacked bar (daily cost by provider), model breakdown, workflow ranking |
| `live.js` | 150 | EventSource, DOM card per task, status transitions |
| `style.css` | 200 | Solarized CSS vars (1:1 from palette.rs), responsive grid, dark/light |

Auth: Bearer token from `localStorage` (set via `/ui?token=...` on first visit).

---

### Color Mapping: TUI Palette --> CSS

1:1 from `tools/nika-tui/src/theme/palette.rs`:

```css
:root {
    --base03:  #002b36;  --base02:  #073642;  --base01:  #586e75;  --base00:  #657b83;
    --base0:   #839496;  --base1:   #93a1a1;  --base2:   #eee8d5;  --base3:   #fdf6e3;
    --yellow:  #b58900;  --orange:  #cb4b16;  --red:     #dc322f;  --magenta: #d33682;
    --violet:  #6c71c4;  --blue:    #268bd2;  --cyan:    #2aa198;  --green:   #859900;
    --verb-infer: var(--blue);    --verb-exec: var(--cyan);     --verb-fetch: var(--orange);
    --verb-invoke: var(--violet); --verb-agent: var(--magenta);
    --status-completed: var(--green);  --status-failed: var(--red);
    --status-running: var(--blue);     --status-skipped: var(--base01);
    --status-cancelled: var(--yellow);
}
```

---

### rust-embed Integration

```rust
#[derive(Embed)]
#[folder = "assets/"]
struct Assets;
// Debug: reads from disk (hot reload). Release: brotli-compressed in binary.
// SPA fallback: unknown paths -> index.html (client-side hash routing).
```

---

### Test Function Signatures (8 tests)

```rust
// tools/nika-obs/src/parser.rs #[cfg(test)]
#[test] fn test_parse_trace_valid_ndjson() { /* 3 events, sorted, correct kinds */ }
#[test] fn test_parse_trace_skips_malformed_lines() { /* 5 valid + 1 bad = 5 events */ }
#[test] fn test_parse_trace_file_not_found() { /* ParseError::NotFound */ }

// tools/nika-obs/src/aggregator.rs #[cfg(test)]
#[test] fn test_aggregate_simple_infer_workflow() { /* duration, cost, tokens, outcome, ttft, provider */ }
#[test] fn test_aggregate_multi_task_with_providers() { /* 2 providers, tasks sorted by start_ms */ }
#[test] fn test_aggregate_structured_output_stats() { /* attempts, successes, repairs, layer */ }
#[test] fn test_aggregate_for_each_summary() { /* total=5, succeeded=3, failed=1, skipped=1 */ }
#[test] fn test_aggregate_failed_workflow() { /* outcome="failed", error, failed_task */ }

// tools/nika-obs/src/routes.rs #[cfg(test)]
#[test] fn test_trace_id_validation() { /* accepts valid, rejects traversal/empty */ }
```

---

### Implementation Phases (5 phases, 3 weeks)

| Phase | What | LOC | Duration |
|-------|------|-----|----------|
| **P1** | Crate scaffold + parser.rs + 3 parser tests | ~200 | Day 1 |
| **P2** | aggregator.rs + TraceSummary + 5 aggregator tests | ~500 | Days 2-3 |
| **P3** | routes.rs + rust-embed + wiring into nika-serve | ~300 | Days 4-5 |
| **P4** | Enhanced Prometheus metrics + worker trace parse | ~200 | Day 6 |
| **P5** | SPA assets (HTML/JS/CSS, 4 views) | ~800 | Days 7-14 |

**Total: ~2000 LOC Rust + ~950 lines JS/HTML/CSS**

### Competitive Advantage vs LangSmith
- 58 event types across 15 categories (vs LangSmith's input/output strings)
- Structured output 5-layer defense visibility (attempts, repairs, which layer)
- MCP call traces with params/response, duration, cache hits
- Agent thinking blocks (AgentTurnMetadata)
- for_each item-level breakdown (succeeded/failed/skipped per item)
- Provider fallback chains and auto-retry history
- Self-hosted, zero-config, embedded in binary (single `nika serve`)
- Free forever (LangSmith: $1/1k traces)

---

## FEATURE 5: PostgreSQL Backend (v0.75)

### What
Optional PostgreSQL backend for `nika-storage`. Enables multi-instance `nika serve`.

### Architecture
**`StorageBackend` async trait + enum dispatch wrapper.**
- `Storage` remains a concrete `Clone` type (zero API change for callers).
- Internally: `Arc<dyn StorageBackend>` dispatches to `SqliteBackend` or `PostgresBackend`.
- Feature-gated: `cargo build --features postgres`.
- Selection: `NIKA_STORAGE_BACKEND=postgres NIKA_STORAGE_URL=postgres://...`

---

### StorageBackend Trait (all 16 methods, exact signatures)

```rust
// nika-storage/src/backend.rs

use async_trait::async_trait;
use crate::{
    Checkpoint, Job, JobArtifact, JobFilter, JobHistoryEvent, JobState,
    StorageResult,
};

/// Backend-agnostic storage interface.
///
/// Implemented by `SqliteBackend` (dedicated OS thread + mpsc channel)
/// and `PostgresBackend` (sqlx PgPool, natively async).
#[async_trait]
pub(crate) trait StorageBackend: Send + Sync + 'static {
    // ── Jobs ────────────────────────────────────────────────────────
    async fn insert_job(&self, job: Job) -> StorageResult<()>;
    async fn get_job(&self, id: &str) -> StorageResult<Option<Job>>;
    async fn list_jobs(&self, state: Option<JobState>) -> StorageResult<Vec<Job>>;
    async fn list_jobs_filtered(&self, filter: JobFilter) -> StorageResult<Vec<Job>>;
    async fn list_jobs_for_workflow(&self, workflow: &str) -> StorageResult<Vec<Job>>;
    async fn update_state(
        &self,
        id: &str,
        state: JobState,
        exit_code: Option<i32>,
        output: Option<String>,
    ) -> StorageResult<()>;
    async fn increment_retry(&self, id: &str) -> StorageResult<u32>;
    async fn reset_stale_running(&self, reason: &str) -> StorageResult<u64>;
    async fn delete_old_jobs(&self, max_age_secs: u64) -> StorageResult<u64>;

    // ── History ─────────────────────────────────────────────────────
    async fn add_history(&self, event: JobHistoryEvent) -> StorageResult<()>;
    async fn get_history(&self, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>>;

    // ── Artifacts ───────────────────────────────────────────────────
    async fn add_artifacts(
        &self,
        job_id: &str,
        artifacts: Vec<JobArtifact>,
    ) -> StorageResult<()>;
    async fn list_artifacts(&self, job_id: &str) -> StorageResult<Vec<JobArtifact>>;

    // ── Checkpoints ─────────────────────────────────────────────────
    async fn save_checkpoint(
        &self,
        job_id: &str,
        task_id: &str,
        output: &str,
    ) -> StorageResult<()>;
    async fn load_checkpoints(&self, job_id: &str) -> StorageResult<Vec<Checkpoint>>;
    async fn delete_checkpoints(&self, job_id: &str) -> StorageResult<()>;
}
```

---

### SqliteBackend Struct + Impl (extracted from current lib.rs)

```rust
// nika-storage/src/sqlite.rs

use rusqlite::Connection;
use tokio::sync::{mpsc, oneshot};
use crate::backend::StorageBackend;
use crate::{
    Checkpoint, DbCommand, Job, JobArtifact, JobFilter, JobHistoryEvent,
    JobState, StorageError, StorageResult,
};

/// SQLite backend using a dedicated OS thread with mpsc command channel.
///
/// rusqlite `Connection` is `Send` but NOT `Sync` -- the channel pattern
/// avoids any `Arc<Mutex<Connection>>` + `spawn_blocking` anti-pattern.
pub(crate) struct SqliteBackend {
    tx: mpsc::Sender<DbCommand>,
}

impl SqliteBackend {
    /// Open (or create) a SQLite database at the given path.
    /// Spawns a dedicated OS thread for all database operations.
    pub fn open(db_path: &std::path::Path) -> StorageResult<Self> {
        let db_path = db_path.to_path_buf();
        let (tx, rx) = mpsc::channel(256);
        std::thread::Builder::new()
            .name("nika-db".into())
            .spawn(move || {
                if let Err(e) = db_thread(db_path, rx) {
                    tracing::error!(error = %e, "database thread exited with error");
                }
            })
            .map_err(|e| StorageError::Other(format!("failed to spawn DB thread: {e}")))?;
        Ok(Self { tx })
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> StorageResult<Self> {
        let (tx, rx) = mpsc::channel(256);
        std::thread::Builder::new()
            .name("nika-db-test".into())
            .spawn(move || {
                let conn = Connection::open_in_memory().expect("open in-memory db");
                init_schema(&conn).expect("init schema");
                run_db_loop(conn, rx);
            })
            .map_err(|e| StorageError::Other(format!("failed to spawn DB thread: {e}")))?;
        Ok(Self { tx })
    }
}

#[async_trait::async_trait]
impl StorageBackend for SqliteBackend {
    async fn insert_job(&self, job: Job) -> StorageResult<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(DbCommand::InsertJob { job, reply }).await
            .map_err(|_| StorageError::ChannelClosed)?;
        rx.await.map_err(|_| StorageError::ChannelClosed)?
    }

    // ... all 16 methods delegate to DbCommand enum exactly as today ...
    // The do_* functions (do_insert_job, do_get_job, ...) move here unchanged.
    // init_schema(), run_db_loop(), db_thread() move here unchanged.
    // DbCommand enum stays in this file (private to sqlite module).
}
```

**What moves from lib.rs to sqlite.rs (exact code blocks):**

| Code block (lib.rs lines) | Destination |
|---|---|
| `enum DbCommand { ... }` (L149-L220) | `sqlite.rs` — private to module |
| `fn db_thread(...)` (L539-L558) | `sqlite.rs` |
| `fn run_db_loop(...)` (L560-L639) | `sqlite.rs` |
| `fn init_schema(...)` (L645-L733) | `sqlite.rs` |
| All `do_*` functions (L739-L1081) | `sqlite.rs` |
| `const JOB_COLUMNS` + `fn row_to_job` (L1084-L1103) | `sqlite.rs` |
| `const SCHEMA_VERSION: u32 = 4` (L21) | `sqlite.rs` |
| `const MAX_JOB_LIST: i64 = 1000` (L767) | `sqlite.rs` |

**What stays in lib.rs:**
- All public types: `Job`, `JobState`, `JobFilter`, `JobHistoryEvent`, `Checkpoint`, `JobArtifact`
- `StorageError`, `StorageResult`
- `Storage` struct (rewritten to `Arc<dyn StorageBackend>`)
- Convenience methods: `create_job`, `create_job_with_tags`, `complete_job`, `fail_job`

---

### Storage Wrapper with Arc<dyn StorageBackend> Dispatch

```rust
// nika-storage/src/lib.rs (rewritten)

mod backend;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "postgres")]
mod postgres;

// Re-export all public types (unchanged)
pub use backend::StorageBackend; // pub(crate) — not re-exported
// ... Job, JobState, etc. stay pub ...

use std::sync::Arc;

/// Async handle to the storage layer.
///
/// All callers see the same API regardless of backend. Internally dispatches
/// to `SqliteBackend` (dedicated OS thread) or `PostgresBackend` (PgPool).
#[derive(Clone)]
pub struct Storage {
    inner: Arc<dyn backend::StorageBackend>,
}

impl Storage {
    /// Open SQLite backend at the given path (default).
    #[cfg(feature = "sqlite")]
    pub fn open(db_path: &std::path::Path) -> StorageResult<Self> {
        let backend = sqlite::SqliteBackend::open(db_path)?;
        Ok(Self { inner: Arc::new(backend) })
    }

    /// Open an in-memory SQLite database (for testing).
    #[cfg(feature = "sqlite")]
    pub fn open_memory() -> StorageResult<Self> {
        let backend = sqlite::SqliteBackend::open_memory()?;
        Ok(Self { inner: Arc::new(backend) })
    }

    /// Open PostgreSQL backend with connection pool.
    #[cfg(feature = "postgres")]
    pub async fn open_postgres(url: &str) -> StorageResult<Self> {
        let backend = postgres::PostgresBackend::connect(url).await?;
        Ok(Self { inner: Arc::new(backend) })
    }

    /// Create from any backend implementation (for testing / DI).
    pub fn from_backend(backend: Arc<dyn backend::StorageBackend>) -> Self {
        Self { inner: backend }
    }

    // ── Delegated methods (all 16, unchanged public signatures) ────
    pub async fn insert_job(&self, job: Job) -> StorageResult<()> {
        self.inner.insert_job(job).await
    }
    pub async fn get_job(&self, id: &str) -> StorageResult<Option<Job>> {
        self.inner.get_job(id).await
    }
    pub async fn list_jobs(&self, state: Option<JobState>) -> StorageResult<Vec<Job>> {
        self.inner.list_jobs(state).await
    }
    pub async fn list_jobs_filtered(&self, filter: JobFilter) -> StorageResult<Vec<Job>> {
        self.inner.list_jobs_filtered(filter).await
    }
    pub async fn list_jobs_for_workflow(&self, workflow: &str) -> StorageResult<Vec<Job>> {
        self.inner.list_jobs_for_workflow(workflow).await
    }
    pub async fn update_state(
        &self, id: &str, state: JobState, exit_code: Option<i32>, output: Option<String>,
    ) -> StorageResult<()> {
        self.inner.update_state(id, state, exit_code, output).await
    }
    pub async fn increment_retry(&self, id: &str) -> StorageResult<u32> {
        self.inner.increment_retry(id).await
    }
    pub async fn reset_stale_running(&self, reason: &str) -> StorageResult<u64> {
        self.inner.reset_stale_running(reason).await
    }
    pub async fn delete_old_jobs(&self, max_age_secs: u64) -> StorageResult<u64> {
        self.inner.delete_old_jobs(max_age_secs).await
    }
    pub async fn add_history(&self, event: JobHistoryEvent) -> StorageResult<()> {
        self.inner.add_history(event).await
    }
    pub async fn get_history(&self, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>> {
        self.inner.get_history(job_id).await
    }
    pub async fn add_artifacts(&self, job_id: &str, artifacts: Vec<JobArtifact>) -> StorageResult<()> {
        self.inner.add_artifacts(job_id, artifacts).await
    }
    pub async fn list_artifacts(&self, job_id: &str) -> StorageResult<Vec<JobArtifact>> {
        self.inner.list_artifacts(job_id).await
    }
    pub async fn save_checkpoint(&self, job_id: &str, task_id: &str, output: &str) -> StorageResult<()> {
        self.inner.save_checkpoint(job_id, task_id, output).await
    }
    pub async fn load_checkpoints(&self, job_id: &str) -> StorageResult<Vec<Checkpoint>> {
        self.inner.load_checkpoints(job_id).await
    }
    pub async fn delete_checkpoints(&self, job_id: &str) -> StorageResult<()> {
        self.inner.delete_checkpoints(job_id).await
    }

    // ── Convenience methods (unchanged, delegate to above) ─────────
    pub async fn create_job(&self, id: &str, workflow: &str) -> StorageResult<()> {
        self.create_job_with_tags(id, workflow, None).await
    }
    pub async fn create_job_with_tags(&self, id: &str, workflow: &str, tags: Option<String>) -> StorageResult<()> {
        self.insert_job(Job {
            id: id.to_string(), name: None, workflow: workflow.to_string(),
            args: None, cron: None, state: JobState::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None, completed_at: None, exit_code: None, output: None,
            retry_count: 0, max_retries: 0, tags,
        }).await
    }
    pub async fn complete_job(&self, id: &str, output: &str) -> StorageResult<()> {
        self.update_state(id, JobState::Completed, Some(0), Some(output.to_string())).await
    }
    pub async fn fail_job(&self, id: &str, error: &str) -> StorageResult<()> {
        self.update_state(id, JobState::Failed, Some(1), Some(error.to_string())).await
    }
}
```

---

### PostgresBackend Struct + Impl

```rust
// nika-storage/src/postgres.rs

use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::Row;
use crate::backend::StorageBackend;
use crate::{
    Checkpoint, Job, JobArtifact, JobFilter, JobHistoryEvent, JobState,
    StorageError, StorageResult,
};

/// PostgreSQL backend using sqlx connection pool.
///
/// No dedicated OS thread needed -- sqlx is natively async.
/// Supports multi-instance `nika serve` via `instance_id` column.
pub(crate) struct PostgresBackend {
    pool: PgPool,
    /// Unique identifier for this nika-serve instance.
    /// Used by `reset_stale_running` to only reset jobs owned by this instance.
    instance_id: String,
}

impl PostgresBackend {
    /// Connect to PostgreSQL and run migrations.
    pub async fn connect(url: &str) -> StorageResult<Self> {
        Self::connect_with_instance(url, &generate_instance_id()).await
    }

    /// Connect with explicit instance_id (for testing).
    pub async fn connect_with_instance(url: &str, instance_id: &str) -> StorageResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)          // Sensible default for single serve instance
            .min_connections(2)           // Keep 2 warm connections
            .acquire_timeout(std::time::Duration::from_secs(5))
            .idle_timeout(std::time::Duration::from_secs(600))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .connect(url)
            .await
            .map_err(|e| StorageError::Other(format!("PG connect: {e}")))?;

        let backend = Self {
            pool,
            instance_id: instance_id.to_string(),
        };
        backend.run_migrations().await?;
        Ok(backend)
    }

    /// Run schema migrations via `schema_migrations` table.
    async fn run_migrations(&self) -> StorageResult<()> {
        // Create migrations tracking table (idempotent)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("create schema_migrations: {e}")))?;

        let current: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("read schema version: {e}")))?;

        if current < 1 {
            sqlx::query(include_str!("../migrations/postgres/001_jobs.sql"))
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("migration 001: {e}")))?;
            sqlx::query("INSERT INTO schema_migrations (version) VALUES (1)")
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("record migration 001: {e}")))?;
        }
        if current < 2 {
            sqlx::query(include_str!("../migrations/postgres/002_artifacts.sql"))
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("migration 002: {e}")))?;
            sqlx::query("INSERT INTO schema_migrations (version) VALUES (2)")
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("record migration 002: {e}")))?;
        }
        if current < 3 {
            sqlx::query(include_str!("../migrations/postgres/003_checkpoints.sql"))
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("migration 003: {e}")))?;
            sqlx::query("INSERT INTO schema_migrations (version) VALUES (3)")
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("record migration 003: {e}")))?;
        }
        if current < 4 {
            sqlx::query(include_str!("../migrations/postgres/004_instance_id.sql"))
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("migration 004: {e}")))?;
            sqlx::query("INSERT INTO schema_migrations (version) VALUES (4)")
                .execute(&self.pool).await
                .map_err(|e| StorageError::Other(format!("record migration 004: {e}")))?;
        }

        tracing::debug!(version = 4, "PG schema migrations applied");
        Ok(())
    }
}

/// Generate a unique instance_id from hostname + PID + random suffix.
fn generate_instance_id() -> String {
    let host = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let pid = std::process::id();
    let rand: u32 = rand::random();
    format!("{host}-{pid}-{rand:08x}")
}

/// Convert PG TIMESTAMPTZ to RFC3339 String for Job struct.
fn ts_to_rfc3339(ts: chrono::DateTime<chrono::Utc>) -> String {
    ts.to_rfc3339()
}

/// Parse RFC3339 String to PG TIMESTAMPTZ for bind parameters.
fn rfc3339_to_ts(s: &str) -> StorageResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| StorageError::Other(format!("invalid timestamp '{s}': {e}")))
}

fn pg_row_to_job(row: &PgRow) -> Result<Job, sqlx::Error> {
    Ok(Job {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        workflow: row.try_get("workflow")?,
        args: row.try_get("args")?,
        cron: row.try_get("cron")?,
        state: JobState::parse(row.try_get::<String, _>("state")?.as_str()),
        created_at: ts_to_rfc3339(row.try_get("created_at")?),
        started_at: row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at")?
            .map(ts_to_rfc3339),
        completed_at: row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at")?
            .map(ts_to_rfc3339),
        exit_code: row.try_get("exit_code")?,
        output: row.try_get("output")?,
        retry_count: row.try_get::<i32, _>("retry_count")? as u32,
        max_retries: row.try_get::<i32, _>("max_retries")? as u32,
        tags: row.try_get::<Option<serde_json::Value>, _>("tags")?
            .map(|v| v.to_string()),
    })
}

#[async_trait::async_trait]
impl StorageBackend for PostgresBackend {
    async fn insert_job(&self, job: Job) -> StorageResult<()> {
        let created_at = rfc3339_to_ts(&job.created_at)?;
        let tags: Option<serde_json::Value> = job.tags.as_deref()
            .map(|t| serde_json::from_str(t))
            .transpose()
            .map_err(|e| StorageError::Other(format!("invalid tags JSON: {e}")))?;

        sqlx::query(
            "INSERT INTO jobs (id, name, workflow, args, cron, state, created_at,
                              retry_count, max_retries, tags, instance_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
        )
        .bind(&job.id).bind(&job.name).bind(&job.workflow)
        .bind(&job.args).bind(&job.cron).bind(job.state.as_str())
        .bind(created_at)
        .bind(job.retry_count as i32).bind(job.max_retries as i32)
        .bind(&tags).bind(&self.instance_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("insert_job: {e}")))?;
        Ok(())
    }

    async fn get_job(&self, id: &str) -> StorageResult<Option<Job>> {
        let row = sqlx::query("SELECT * FROM jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Other(format!("get_job: {e}")))?;
        match row {
            Some(r) => Ok(Some(pg_row_to_job(&r)
                .map_err(|e| StorageError::Other(format!("row decode: {e}")))?)),
            None => Ok(None),
        }
    }

    async fn list_jobs(&self, state: Option<JobState>) -> StorageResult<Vec<Job>> {
        let rows = match state {
            Some(s) => {
                sqlx::query("SELECT * FROM jobs WHERE state = $1 ORDER BY created_at DESC LIMIT 1000")
                    .bind(s.as_str())
                    .fetch_all(&self.pool).await
            }
            None => {
                sqlx::query("SELECT * FROM jobs ORDER BY created_at DESC LIMIT 1000")
                    .fetch_all(&self.pool).await
            }
        }.map_err(|e| StorageError::Other(format!("list_jobs: {e}")))?;

        rows.iter().map(|r| pg_row_to_job(r)
            .map_err(|e| StorageError::Other(format!("row decode: {e}"))))
            .collect()
    }

    async fn list_jobs_filtered(&self, filter: JobFilter) -> StorageResult<Vec<Job>> {
        // Dynamic query builder — PG uses $N positional params
        let mut conditions = Vec::new();
        let mut idx = 1u32;

        // We build the SQL string and bind dynamically with sqlx::query_as
        // For simplicity, use raw query with explicit binds
        let mut sql = String::from("SELECT * FROM jobs");
        let mut binds: Vec<Box<dyn std::any::Any + Send>> = Vec::new();

        if filter.state.is_some() {
            conditions.push(format!("state = ${idx}"));
            idx += 1;
        }
        if filter.workflow.is_some() {
            conditions.push(format!("workflow = ${idx}"));
            idx += 1;
        }
        if filter.tag.is_some() {
            // JSONB operator: tags->>'key' = $N
            let (ref key, _) = filter.tag.as_ref().unwrap();
            conditions.push(format!("tags->>'{key}' = ${idx}"));
            idx += 1;
        }

        if !conditions.is_empty() {
            sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }

        let limit = filter.limit.unwrap_or(100).min(1000);
        let offset = filter.offset.unwrap_or(0);
        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT {limit} OFFSET {offset}"));

        // Build query with dynamic binds
        let mut query = sqlx::query(&sql);
        if let Some(ref state) = filter.state {
            query = query.bind(state.as_str().to_string());
        }
        if let Some(ref workflow) = filter.workflow {
            query = query.bind(workflow.clone());
        }
        if let Some((_, ref val)) = filter.tag {
            query = query.bind(val.clone());
        }

        let rows = query.fetch_all(&self.pool).await
            .map_err(|e| StorageError::Other(format!("list_jobs_filtered: {e}")))?;
        rows.iter().map(|r| pg_row_to_job(r)
            .map_err(|e| StorageError::Other(format!("row decode: {e}"))))
            .collect()
    }

    async fn list_jobs_for_workflow(&self, workflow: &str) -> StorageResult<Vec<Job>> {
        let rows = sqlx::query("SELECT * FROM jobs WHERE workflow = $1 ORDER BY created_at DESC LIMIT 10")
            .bind(workflow)
            .fetch_all(&self.pool).await
            .map_err(|e| StorageError::Other(format!("list_jobs_for_workflow: {e}")))?;
        rows.iter().map(|r| pg_row_to_job(r)
            .map_err(|e| StorageError::Other(format!("row decode: {e}"))))
            .collect()
    }

    async fn update_state(
        &self, id: &str, state: JobState, exit_code: Option<i32>, output: Option<String>,
    ) -> StorageResult<()> {
        let now = chrono::Utc::now();
        match state {
            JobState::Running => {
                sqlx::query("UPDATE jobs SET state = $1, started_at = $2 WHERE id = $3")
                    .bind(state.as_str()).bind(now).bind(id)
                    .execute(&self.pool).await
            }
            JobState::Completed | JobState::Failed | JobState::Cancelled => {
                sqlx::query(
                    "UPDATE jobs SET state = $1, completed_at = $2, exit_code = $3, output = $4
                     WHERE id = $5 AND state IN ('pending', 'running')"
                )
                .bind(state.as_str()).bind(now).bind(exit_code).bind(&output).bind(id)
                .execute(&self.pool).await
            }
            _ => {
                sqlx::query("UPDATE jobs SET state = $1 WHERE id = $2")
                    .bind(state.as_str()).bind(id)
                    .execute(&self.pool).await
            }
        }.map_err(|e| StorageError::Other(format!("update_state: {e}")))?;
        Ok(())
    }

    async fn increment_retry(&self, id: &str) -> StorageResult<u32> {
        // PG supports RETURNING — single roundtrip instead of UPDATE + SELECT
        let count: i32 = sqlx::query_scalar(
            "UPDATE jobs SET retry_count = retry_count + 1 WHERE id = $1 RETURNING retry_count"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("increment_retry: {e}")))?;
        Ok(count as u32)
    }

    async fn reset_stale_running(&self, reason: &str) -> StorageResult<u64> {
        // Multi-instance safe: only reset jobs owned by THIS instance
        let now = chrono::Utc::now();
        let result = sqlx::query(
            "UPDATE jobs SET state = 'failed', output = $1, completed_at = $2
             WHERE state = 'running' AND instance_id = $3"
        )
        .bind(reason).bind(now).bind(&self.instance_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Other(format!("reset_stale_running: {e}")))?;
        Ok(result.rows_affected())
    }

    async fn delete_old_jobs(&self, max_age_secs: u64) -> StorageResult<u64> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);

        // Single transaction: delete children then parents
        let mut tx = self.pool.begin().await
            .map_err(|e| StorageError::Other(format!("begin tx: {e}")))?;

        sqlx::query(
            "DELETE FROM checkpoints WHERE job_id IN (
                SELECT id FROM jobs WHERE state IN ('completed','failed','cancelled') AND created_at < $1
            )"
        ).bind(cutoff).execute(&mut *tx).await
            .map_err(|e| StorageError::Other(format!("gc checkpoints: {e}")))?;

        sqlx::query(
            "DELETE FROM job_artifacts WHERE job_id IN (
                SELECT id FROM jobs WHERE state IN ('completed','failed','cancelled') AND created_at < $1
            )"
        ).bind(cutoff).execute(&mut *tx).await
            .map_err(|e| StorageError::Other(format!("gc artifacts: {e}")))?;

        sqlx::query(
            "DELETE FROM job_history WHERE job_id IN (
                SELECT id FROM jobs WHERE state IN ('completed','failed','cancelled') AND created_at < $1
            )"
        ).bind(cutoff).execute(&mut *tx).await
            .map_err(|e| StorageError::Other(format!("gc history: {e}")))?;

        let result = sqlx::query(
            "DELETE FROM jobs WHERE state IN ('completed','failed','cancelled') AND created_at < $1"
        ).bind(cutoff).execute(&mut *tx).await
            .map_err(|e| StorageError::Other(format!("gc jobs: {e}")))?;

        tx.commit().await
            .map_err(|e| StorageError::Other(format!("commit gc tx: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn add_history(&self, event: JobHistoryEvent) -> StorageResult<()> {
        let ts = rfc3339_to_ts(&event.timestamp)?;
        sqlx::query(
            "INSERT INTO job_history (job_id, event, timestamp, details)
             VALUES ($1, $2, $3, $4)"
        )
        .bind(&event.job_id).bind(&event.event).bind(ts).bind(&event.details)
        .execute(&self.pool).await
        .map_err(|e| StorageError::Other(format!("add_history: {e}")))?;
        Ok(())
    }

    async fn get_history(&self, job_id: &str) -> StorageResult<Vec<JobHistoryEvent>> {
        let rows = sqlx::query(
            "SELECT job_id, event, timestamp, details FROM job_history
             WHERE job_id = $1 ORDER BY id ASC LIMIT 10000"
        )
        .bind(job_id)
        .fetch_all(&self.pool).await
        .map_err(|e| StorageError::Other(format!("get_history: {e}")))?;

        rows.iter().map(|r| {
            Ok(JobHistoryEvent {
                job_id: r.try_get("job_id").map_err(|e| StorageError::Other(e.to_string()))?,
                event: r.try_get("event").map_err(|e| StorageError::Other(e.to_string()))?,
                timestamp: ts_to_rfc3339(
                    r.try_get("timestamp").map_err(|e| StorageError::Other(e.to_string()))?
                ),
                details: r.try_get("details").map_err(|e| StorageError::Other(e.to_string()))?,
            })
        }).collect()
    }

    async fn add_artifacts(&self, job_id: &str, artifacts: Vec<JobArtifact>) -> StorageResult<()> {
        for a in &artifacts {
            sqlx::query(
                "INSERT INTO job_artifacts (job_id, name, path, size, format, checksum, content_type)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (job_id, name) DO UPDATE SET
                    path = EXCLUDED.path, size = EXCLUDED.size, format = EXCLUDED.format,
                    checksum = EXCLUDED.checksum, content_type = EXCLUDED.content_type"
            )
            .bind(job_id).bind(&a.name).bind(&a.path)
            .bind(a.size as i64).bind(&a.format).bind(&a.checksum).bind(&a.content_type)
            .execute(&self.pool).await
            .map_err(|e| StorageError::Other(format!("add_artifact: {e}")))?;
        }
        Ok(())
    }

    async fn list_artifacts(&self, job_id: &str) -> StorageResult<Vec<JobArtifact>> {
        let rows = sqlx::query(
            "SELECT job_id, name, path, size, format, checksum, content_type
             FROM job_artifacts WHERE job_id = $1 ORDER BY name"
        )
        .bind(job_id)
        .fetch_all(&self.pool).await
        .map_err(|e| StorageError::Other(format!("list_artifacts: {e}")))?;

        rows.iter().map(|r| {
            Ok(JobArtifact {
                job_id: r.try_get("job_id").map_err(|e| StorageError::Other(e.to_string()))?,
                name: r.try_get("name").map_err(|e| StorageError::Other(e.to_string()))?,
                path: r.try_get("path").map_err(|e| StorageError::Other(e.to_string()))?,
                size: r.try_get::<i64, _>("size").map_err(|e| StorageError::Other(e.to_string()))? as u64,
                format: r.try_get("format").map_err(|e| StorageError::Other(e.to_string()))?,
                checksum: r.try_get("checksum").map_err(|e| StorageError::Other(e.to_string()))?,
                content_type: r.try_get("content_type").map_err(|e| StorageError::Other(e.to_string()))?,
            })
        }).collect()
    }

    async fn save_checkpoint(&self, job_id: &str, task_id: &str, output: &str) -> StorageResult<()> {
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO checkpoints (job_id, task_id, output, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (job_id, task_id) DO UPDATE SET
                output = EXCLUDED.output, created_at = EXCLUDED.created_at"
        )
        .bind(job_id).bind(task_id).bind(output).bind(now)
        .execute(&self.pool).await
        .map_err(|e| StorageError::Other(format!("save_checkpoint: {e}")))?;
        Ok(())
    }

    async fn load_checkpoints(&self, job_id: &str) -> StorageResult<Vec<Checkpoint>> {
        let rows = sqlx::query(
            "SELECT job_id, task_id, output, created_at FROM checkpoints
             WHERE job_id = $1 ORDER BY created_at"
        )
        .bind(job_id)
        .fetch_all(&self.pool).await
        .map_err(|e| StorageError::Other(format!("load_checkpoints: {e}")))?;

        rows.iter().map(|r| {
            Ok(Checkpoint {
                job_id: r.try_get("job_id").map_err(|e| StorageError::Other(e.to_string()))?,
                task_id: r.try_get("task_id").map_err(|e| StorageError::Other(e.to_string()))?,
                output: r.try_get("output").map_err(|e| StorageError::Other(e.to_string()))?,
                created_at: ts_to_rfc3339(
                    r.try_get("created_at").map_err(|e| StorageError::Other(e.to_string()))?
                ),
            })
        }).collect()
    }

    async fn delete_checkpoints(&self, job_id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM checkpoints WHERE job_id = $1")
            .bind(job_id)
            .execute(&self.pool).await
            .map_err(|e| StorageError::Other(format!("delete_checkpoints: {e}")))?;
        Ok(())
    }
}
```

---

### Complete PostgreSQL DDL (4 migration files)

#### 001_jobs.sql
```sql
-- Migration 001: jobs + job_history tables

CREATE TABLE IF NOT EXISTS jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT,
    workflow      TEXT NOT NULL,
    args          TEXT,
    cron          TEXT,
    state         TEXT NOT NULL DEFAULT 'pending',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ,
    exit_code     INTEGER,
    output        TEXT,
    retry_count   INTEGER NOT NULL DEFAULT 0,
    max_retries   INTEGER NOT NULL DEFAULT 0,
    tags          JSONB,
    instance_id   TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow ON jobs(workflow);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_instance_id ON jobs(instance_id);
CREATE INDEX IF NOT EXISTS idx_jobs_tags ON jobs USING GIN (tags);

CREATE TABLE IF NOT EXISTS job_history (
    id          BIGSERIAL PRIMARY KEY,
    job_id      TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    event       TEXT NOT NULL,
    timestamp   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    details     TEXT
);

CREATE INDEX IF NOT EXISTS idx_job_history_job_id ON job_history(job_id);
```

#### 002_artifacts.sql
```sql
-- Migration 002: job_artifacts table

CREATE TABLE IF NOT EXISTS job_artifacts (
    job_id        TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    size          BIGINT NOT NULL DEFAULT 0,
    format        TEXT NOT NULL DEFAULT 'text',
    checksum      TEXT,
    content_type  TEXT NOT NULL DEFAULT 'application/octet-stream',
    PRIMARY KEY (job_id, name)
);

CREATE INDEX IF NOT EXISTS idx_job_artifacts_job_id ON job_artifacts(job_id);
```

#### 003_checkpoints.sql
```sql
-- Migration 003: checkpoints table

CREATE TABLE IF NOT EXISTS checkpoints (
    job_id      TEXT NOT NULL,
    task_id     TEXT NOT NULL,
    output      TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (job_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_job_id ON checkpoints(job_id);
```

#### 004_instance_id.sql
```sql
-- Migration 004: multi-instance safety
-- instance_id tracks which nika-serve instance owns each running job.
-- reset_stale_running uses this to only reset jobs from THIS instance,
-- preventing multi-instance serve from incorrectly resetting each other's jobs.

-- Column already included in 001 for fresh installs. This migration
-- ensures upgrades from earlier schemas also get the column + index.
-- PG IF NOT EXISTS for columns requires DO block:

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'jobs' AND column_name = 'instance_id'
    ) THEN
        ALTER TABLE jobs ADD COLUMN instance_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_jobs_instance_id ON jobs(instance_id);
    END IF;
END
$$;
```

---

### Key Query Translations (detailed)

| Operation | SQLite (current) | PostgreSQL |
|-----------|-----------------|-----------|
| JSON tag filter | `json_extract(tags, '$.key') = ?1` | `tags->>'key' = $1` (JSONB operator) |
| Upsert artifact | `INSERT OR REPLACE INTO job_artifacts ...` | `INSERT INTO job_artifacts ... ON CONFLICT (job_id, name) DO UPDATE SET ...` |
| Upsert checkpoint | `INSERT OR REPLACE INTO checkpoints ...` | `INSERT INTO checkpoints ... ON CONFLICT (job_id, task_id) DO UPDATE SET ...` |
| Schema version | `PRAGMA user_version` / `PRAGMA user_version = N` | `schema_migrations` table with `MAX(version)` |
| Increment+read | `UPDATE ... SET retry_count = retry_count + 1` then `SELECT retry_count` | `UPDATE ... SET retry_count = retry_count + 1 RETURNING retry_count` |
| WAL mode | `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;` | N/A (PG handles WAL natively) |
| Autoincrement | `INTEGER PRIMARY KEY AUTOINCREMENT` | `BIGSERIAL PRIMARY KEY` |
| Timestamp storage | `TEXT` (RFC3339 strings) | `TIMESTAMPTZ` (native, with conversion on read/write) |
| Timestamp comparison | `created_at < ?1` (string comparison works for RFC3339) | `created_at < $1` (native TIMESTAMPTZ comparison) |
| Reset stale running | `WHERE state = 'running'` (resets ALL) | `WHERE state = 'running' AND instance_id = $1` (resets only THIS instance) |
| GC transaction | Implicit (SQLite autocommit per statement) | Explicit `BEGIN` / `COMMIT` transaction |
| Parameter syntax | `?1, ?2, ?3` | `$1, $2, $3` |
| Dynamic params | `Vec<Box<dyn rusqlite::types::ToSql>>` | sqlx `.bind()` chain |

---

### Cargo.toml Feature Flag Configuration

```toml
# nika-storage/Cargo.toml

[package]
name = "nika-storage"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Storage layer for Nika job persistence (SQLite + optional PostgreSQL)"
publish = true

[features]
default = ["sqlite"]
sqlite = ["dep:rusqlite"]
postgres = ["dep:sqlx", "dep:hostname", "dep:rand"]

[dependencies]
# Always required
tokio = { workspace = true, features = ["sync"] }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
async-trait = { workspace = true }

# SQLite backend (default)
rusqlite = { workspace = true, optional = true }

# PostgreSQL backend (opt-in)
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "json"], optional = true }
hostname = { version = "0.4", optional = true }
rand = { version = "0.8", optional = true }

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "test-util"] }
tempfile = { workspace = true }
serde_json = { workspace = true }
# PG integration tests require: cargo test --features postgres
# and NIKA_TEST_PG_URL=postgres://localhost/nika_test env var
```

```toml
# tools/Cargo.toml — add to [workspace.dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "json"] }
hostname = "0.4"
```

```toml
# nika-serve/Cargo.toml — feature forwarding
[features]
default = []
postgres = ["nika-storage/postgres"]

[dependencies]
nika-storage = { workspace = true }  # sqlite feature active by default
```

---

### Backend Selection Logic in nika-serve Startup

```rust
// nika-serve/src/lib.rs — replace current Storage::open() call

pub async fn run_server(config: ServeConfig) -> Result<(), ServeError> {
    // FIX-14: Prevent two nika serve instances sharing the same DB (SQLite only)
    let _db_lock;

    // Backend selection: NIKA_STORAGE_BACKEND env var (default: sqlite)
    let storage = match std::env::var("NIKA_STORAGE_BACKEND").as_deref() {
        #[cfg(feature = "postgres")]
        Ok("postgres" | "pg" | "postgresql") => {
            let url = std::env::var("NIKA_STORAGE_URL")
                .map_err(|_| ServeError::Config(
                    "NIKA_STORAGE_BACKEND=postgres requires NIKA_STORAGE_URL".into()
                ))?;
            _db_lock = None::<DbLock>; // No flock needed for PG
            info!(url = %mask_pg_password(&url), "connecting to PostgreSQL");
            nika_storage::Storage::open_postgres(&url).await
                .map_err(|e| ServeError::Config(format!("PostgreSQL: {e}")))?
        }
        #[cfg(not(feature = "postgres"))]
        Ok("postgres" | "pg" | "postgresql") => {
            return Err(ServeError::Config(
                "PostgreSQL backend requested but nika was compiled without --features postgres.\n\
                 Recompile with: cargo build --features postgres".into()
            ));
        }
        _ => {
            // Default: SQLite
            _db_lock = Some(acquire_db_lock(&config.db_path)?);
            nika_storage::Storage::open(&config.db_path)
                .map_err(|e| ServeError::Config(format!("SQLite: {e}")))?
        }
    };

    // Reset stale running jobs (works for both backends)
    let reset_count = storage.reset_stale_running("Server restarted").await?;
    if reset_count > 0 {
        info!(count = reset_count, "reset stale running jobs from previous session");
    }

    // ... rest of run_server unchanged ...
}

/// Mask password in PG connection string for logging.
fn mask_pg_password(url: &str) -> String {
    // postgres://user:password@host/db → postgres://user:***@host/db
    if let Some(at) = url.find('@') {
        if let Some(colon) = url[..at].rfind(':') {
            let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
            if colon > scheme_end {
                return format!("{}***{}", &url[..colon + 1], &url[at..]);
            }
        }
    }
    url.to_string()
}
```

---

### Timestamp Handling: String (RFC3339) <-> TIMESTAMPTZ

The Job/JobHistoryEvent/Checkpoint structs use `String` for timestamps (RFC3339 format).
SQLite stores them as TEXT. PostgreSQL stores them as `TIMESTAMPTZ`.

**Conversion strategy (zero struct changes):**

```
Write path: String → chrono::DateTime::parse_from_rfc3339() → bind as TIMESTAMPTZ
Read path:  PG TIMESTAMPTZ → chrono::DateTime<Utc> → .to_rfc3339() → String
```

Two helper functions handle all conversions:
- `rfc3339_to_ts(s: &str) -> StorageResult<DateTime<Utc>>` — for write path
- `ts_to_rfc3339(ts: DateTime<Utc>) -> String` — for read path

**Edge case**: `created_at` is set by callers as `chrono::Utc::now().to_rfc3339()` before
calling `insert_job`. The PG backend parses this back. Alternative: use `DEFAULT NOW()` in
PG and ignore the caller's timestamp, but this would break test determinism. Keep parsing.

---

### Connection Pool Configuration (PgPoolOptions)

```rust
PgPoolOptions::new()
    .max_connections(10)          // 10 connections per nika-serve instance
    .min_connections(2)           // Keep 2 warm to reduce cold-start latency
    .acquire_timeout(Duration::from_secs(5))   // Fail fast if pool exhausted
    .idle_timeout(Duration::from_secs(600))     // 10 min idle before reclaim
    .max_lifetime(Duration::from_secs(1800))    // 30 min max lifetime (PG best practice)
    .connect(url)
```

**Environment overrides** (future, not Phase 1):
- `NIKA_PG_MAX_CONNECTIONS` — override max_connections
- `NIKA_PG_MIN_CONNECTIONS` — override min_connections

**Why these defaults:**
- `max_connections=10` — `nika serve` default `max_concurrent=4` jobs + GC + reset + headroom
- `min_connections=2` — avoid cold connection on first request after idle period
- `acquire_timeout=5s` — surface pool exhaustion quickly instead of hanging
- `max_lifetime=1800s` — PG docs recommend rotating connections to prevent memory leaks

---

### 10 Test Function Signatures

```rust
// ── SQLite backend tests (in sqlite.rs, run always) ────────────────────

#[cfg(test)]
mod tests {
    // All 26 existing tests in lib.rs move here UNCHANGED.
    // They use SqliteBackend::open_memory() instead of Storage::open_memory().
}

// ── Wrapper tests (in lib.rs, run always with default features) ────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify Storage wrapper delegates correctly through Arc<dyn>.
    #[tokio::test]
    async fn wrapper_delegates_to_sqlite_backend() { ... }

    /// Verify from_backend() constructor works for custom backends.
    #[tokio::test]
    async fn from_backend_accepts_custom_impl() { ... }
}

// ── PostgreSQL integration tests (in postgres.rs, gated) ───────────────

#[cfg(all(test, feature = "postgres"))]
mod tests {
    use super::*;

    /// Skip PG tests if NIKA_TEST_PG_URL is not set.
    fn pg_url() -> Option<String> {
        std::env::var("NIKA_TEST_PG_URL").ok()
    }

    /// PG: insert and retrieve a job (timestamp roundtrip verified).
    #[tokio::test]
    async fn pg_insert_and_get_job() { ... }

    /// PG: list_jobs filtered by state.
    #[tokio::test]
    async fn pg_list_jobs_by_state() { ... }

    /// PG: JSONB tag filtering with tags->>'key' operator.
    #[tokio::test]
    async fn pg_list_jobs_filtered_by_tag() { ... }

    /// PG: increment_retry uses RETURNING (single roundtrip).
    #[tokio::test]
    async fn pg_increment_retry_returning() { ... }

    /// PG: reset_stale_running only resets jobs with matching instance_id.
    #[tokio::test]
    async fn pg_reset_stale_running_instance_scoped() { ... }

    /// PG: reset_stale_running does NOT touch jobs from other instances.
    #[tokio::test]
    async fn pg_reset_stale_running_ignores_other_instances() { ... }

    /// PG: delete_old_jobs cascades to history, artifacts, checkpoints in one transaction.
    #[tokio::test]
    async fn pg_delete_old_jobs_cascades() { ... }

    /// PG: ON CONFLICT upsert for checkpoints and artifacts.
    #[tokio::test]
    async fn pg_upsert_checkpoint_and_artifact() { ... }

    /// PG: schema_migrations table tracks applied versions.
    #[tokio::test]
    async fn pg_schema_migrations_idempotent() { ... }

    /// PG: concurrent insert_job from two connections (pool safety).
    #[tokio::test]
    async fn pg_concurrent_inserts() { ... }
}
```

**Running PG tests:**
```bash
# Start PG (e.g., via Docker)
docker run -d --name nika-pg -e POSTGRES_DB=nika_test -e POSTGRES_PASSWORD=test -p 5432:5432 postgres:17

# Run PG integration tests
NIKA_TEST_PG_URL="postgres://postgres:test@localhost/nika_test" \
    cargo test --lib -p nika-storage --features postgres

# Run only SQLite tests (default, no PG needed)
cargo test --lib -p nika-storage
```

---

### Phase 1 Extraction Plan: Exact Code Blocks to Move

Phase 1 is pure refactor. Zero behavior change. All 26 existing tests pass.

**Step 1: Create `backend.rs`**
- Copy the `StorageBackend` trait definition (16 async methods).
- Add `use` imports for all public types.
- `pub(crate)` visibility.

**Step 2: Create `sqlite.rs`**
Move from `lib.rs`:

| What | lib.rs lines | Action |
|------|-------------|--------|
| `const SCHEMA_VERSION: u32 = 4;` | L21 | Move |
| `enum DbCommand { ... }` | L149-L220 | Move (make `pub(super)` or keep private) |
| `struct SqliteBackend { tx }` | New (replaces inner part of `Storage`) | Create |
| `impl SqliteBackend { open(), open_memory() }` | Extracted from `Storage::open/open_memory` (L236-L270) | Create |
| `impl StorageBackend for SqliteBackend` | New (wraps 16 methods from Storage impl, L272-L533) | Create |
| `fn db_thread(...)` | L539-L558 | Move |
| `fn run_db_loop(...)` | L560-L639 | Move |
| `fn init_schema(...)` | L645-L733 | Move |
| All `do_*` query functions | L739-L1081 | Move (16 functions) |
| `const JOB_COLUMNS` | L1084 | Move |
| `fn row_to_job(...)` | L1086-L1103 | Move |
| `const MAX_JOB_LIST` | L767 | Move |
| All `#[cfg(test)] mod tests { ... }` | L1109-L1704 | Move |

**Step 3: Rewrite `lib.rs`**
- Keep: all public types (`Job`, `JobState`, `JobFilter`, etc.)
- Keep: `StorageError`, `StorageResult`
- Add: `mod backend; mod sqlite;`
- Rewrite `Storage` struct: `inner: Arc<dyn backend::StorageBackend>`
- Delegate all 16 methods through `self.inner`
- Keep convenience methods (`create_job`, `complete_job`, `fail_job`)
- `Storage::open()` creates `SqliteBackend`, wraps in Arc
- `Storage::open_memory()` same

**Step 4: Verify**
```bash
cargo test --lib -p nika-storage        # All 26 tests pass
cargo test --lib -p nika-serve          # All serve tests pass
cargo test --lib -p nika-daemon         # All daemon tests pass
cargo check --workspace                  # Zero warnings
```

**Diff summary for Phase 1:**
- `lib.rs`: ~900 lines removed, ~120 lines rewritten (Storage wrapper)
- `backend.rs`: ~40 lines (trait definition)
- `sqlite.rs`: ~950 lines (moved from lib.rs, nearly unchanged)
- Net: ~+60 lines (trait + delegation boilerplate)

---

### Multi-Instance Safety: instance_id for reset_stale_running

**Problem**: Current `reset_stale_running` runs `UPDATE jobs SET state = 'failed' WHERE state = 'running'`.
With multiple `nika serve` instances sharing one PG database, Instance A restarting would reset
Instance B's legitimately running jobs.

**Solution**: `instance_id` column on `jobs` table.

```
Instance A starts → instance_id = "host-a-12345-abc"
Instance A inserts job → job.instance_id = "host-a-12345-abc"
Instance A crashes → restarts → reset_stale_running WHERE instance_id = "host-a-12345-abc"
Instance B's running jobs → untouched (different instance_id)
```

**instance_id format**: `{hostname}-{pid}-{random_hex}` — unique per process, survives hostname collisions.

**SQLite compatibility**: The `instance_id` column exists but is always `NULL` in SQLite mode.
`reset_stale_running` for SQLite continues to reset ALL running jobs (single-instance assumption
is correct for SQLite with flock).

**Garbage collection**: `instance_id` is informational after job completion. Old instance_ids are
cleaned up naturally when `delete_old_jobs` removes terminal jobs.

**Edge case**: If a PG instance crashes without restarting, its running jobs become orphans.
Future enhancement: heartbeat column (`last_heartbeat_at`) + reaper that fails jobs with stale
heartbeats from any instance. Not in Phase 1.

### Estimate: ~1500 LOC, 14-17 hours across 4 phases

**Phase 1** (4-5h): Extract trait + SqliteBackend. Zero behavior change. All tests pass.
**Phase 2** (4-5h): PostgresBackend impl. 4 migration files. 10 PG integration tests.
**Phase 3** (2-3h): Backend selection in nika-serve. Feature flags. Cargo.toml changes.
**Phase 4** (3-4h): instance_id column. Multi-instance reset_stale_running. Docker Compose test.

---

## DEPENDENCY GRAPH

```
          on_error (v0.71)     ← standalone, no deps
               │
          scheduling (v0.72)   ← storage V5 (schedules table)
               │
          multi-tenant (v0.73) ← storage V6 (serve_tokens table)
               │
          observability (v0.74) ← new crate, serve routes
               │
          postgresql (v0.75)   ← storage refactor, feature-gated
```

Features 1-3 are independent and could ship in any order.
Feature 4 depends on Feature 3 for auth on trace endpoints.
Feature 5 should come last (refactors storage, all other features must be stable first).

---

## WHAT NOT TO DO (reinforced)

- No new verbs (5 verbs are sacred)
- No Egghead/memory system (separate sprint, design bible exists)
- No TUI redesign (88K LOC, mature)
- No WebSocket (SSE works fine)
- No `nika diff` (git diff suffices)
- No `nika upgrade` / self-update (Homebrew handles this)
- No full JSON Schema validator in eval (use `structured:` in workflows)
- No multi-node PG without instance_id (Phase 4 of PG feature)

---

## SKILLS & WORKFLOW

```
Question → Research → Skills → Test → Code → Verify → Commit
```

| Skill | When |
|-------|------|
| `test-driven-development` | All code changes |
| `verification-before-completion` | Before every commit |
| `systematic-debugging` | When tests break |
| `rust` | All Rust code |

| Agent | When |
|-------|------|
| `rust-pro` | Code review after each feature |
| `rust-security` | Review auth changes (Feature 3) |
| `rust-async` | Review PG async pool (Feature 5) |

---

## COMMIT STRATEGY

```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

1 fix = 1 commit. Tests verts. Clippy zero. Push HTTPS.
