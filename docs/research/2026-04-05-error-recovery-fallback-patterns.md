# Error Recovery & Fallback Patterns for DAG Workflow Engines

> Research report for Nika's `on_error:` feature design.
> Date: 2026-04-05 | Author: Claude Opus 4.6

## Summary

This report analyzes error recovery patterns across 6 domains: Temporal's activity fallback,
Airflow/Prefect callbacks, Tower middleware, backon retry, circuit breakers, and multi-LLM
router failover. The goal is to inform the design of three `on_error:` variants for Nika:
`ignore`, `retry_with_provider`, and `fallback_action`.

---

## 1. Temporal SDK Core (Rust) -- Activity Fallback

### Architecture

Temporal's Rust SDK (`temporal-sdk-core`) uses a **deterministic replay** model. Workflows
are state machines; activities are the units of side-effectful work.

**Activity retry policy** (equivalent to nika's `retry:`):
```
RetryPolicy {
    initial_interval: Duration,
    backoff_coefficient: f64,
    maximum_interval: Option<Duration>,
    maximum_attempts: u32,
    non_retryable_error_types: Vec<String>,
}
```

**Fallback pattern** -- Temporal does NOT have a built-in `fallback_activity`. Instead,
the idiomatic pattern is **workflow-level error handling**:

```rust
// Temporal pattern: try primary, catch, run fallback
let result = match execute_activity(primary_activity).await {
    Ok(v) => v,
    Err(e) if e.is_retriable() => {
        // Already retried N times per RetryPolicy. Now fallback:
        execute_activity(fallback_activity).await?
    }
    Err(e) => return Err(e),
};
```

### Key Insights for Nika

1. **Retry and fallback are separate concerns**: Temporal retries the SAME activity,
   then the WORKFLOW decides what to do on exhaustion. This matches nika's architecture
   where `retry:` is per-task and `on_error:` would be a new layer above retry.

2. **Error classification matters**: Temporal classifies errors as `ApplicationError`
   (retryable or not), `CancelledError`, `TimeoutError`. Nika already has `is_retryable()`
   but needs a richer taxonomy for fallback decisions.

3. **Saga compensation**: Temporal supports compensating transactions. If activity B fails
   after A succeeded, you can run `undo_A()`. This is relevant for nika's DAG: a failed
   task mid-chain could trigger cleanup of earlier tasks' side effects.

4. **No "ignore and continue"**: Temporal treats every activity failure as significant.
   The closest is `continueAsNew` which restarts the workflow from a checkpoint. Nika's
   `ignore` is a simpler, more pragmatic pattern for non-critical tasks.

---

## 2. Airflow / Prefect -- on_failure Callbacks

### Airflow's Pattern

Airflow uses **trigger rules** on downstream tasks:

```python
# Airflow trigger_rule options:
"all_success"       # Default -- all parents must succeed
"all_failed"        # All parents must fail (for cleanup tasks)
"one_success"       # At least one parent succeeds
"one_failed"        # At least one parent fails
"none_failed"       # No parent failed (but may be skipped)
"all_done"          # All parents finished (success, fail, or skip)
```

Plus **callbacks** on the task itself:

```python
task = PythonOperator(
    task_id='critical_etl',
    python_callable=run_etl,
    on_failure_callback=send_alert,      # Called on failure
    on_success_callback=log_success,     # Called on success
    on_retry_callback=log_retry,         # Called on each retry
    retries=3,
    retry_delay=timedelta(minutes=5),
)
```

### Prefect's Pattern

Prefect uses **state handlers** and **result types**:

```python
@task(
    retries=3,
    retry_delay_seconds=[10, 30, 60],  # Progressive backoff
    on_failure=[send_slack_alert],      # Hook list
    on_completion=[log_metrics],
)
def critical_task():
    ...
```

Prefect 3.x added **transactions with rollback**:

```python
@task
def critical_task():
    with transaction() as txn:
        txn.on_rollback(cleanup_partial_results)
        result = do_work()
        txn.on_commit(finalize)
        return result
```

### Key Insights for Nika

1. **Trigger rules on DOWNSTREAM tasks** (Airflow) vs **callbacks on THIS task** (Prefect).
   Nika's design should support BOTH: `on_error` on the failing task decides what value to
   produce, while downstream tasks can use `when:` to react to upstream status.

2. **Multiple failure callbacks**: Both systems allow a LIST of callbacks. For nika, a
   single `on_error:` strategy per task is simpler and avoids the "which callback won?" problem.

3. **Separation of alerting from recovery**: Airflow's `on_failure_callback` is for
   side-effects (alerts), not for changing the task's output. Prefect's state handlers
   CAN change the returned state. Nika's `on_error:` should be recovery-focused (producing
   a value), with alerting via events (EventKind::TaskFallbackTriggered).

4. **"all_done" trigger rule**: This is essentially "ignore upstream failures and run anyway".
   In nika terms, this is what `on_error: ignore` enables for downstream consumers.

---

## 3. Tower::retry -- Middleware Pattern

### Architecture

Tower's retry is a `Service<Request>` middleware wrapper:

```
Retry<P, S> where P: Policy<Req, Res, E>, S: Service<Req>
```

The `Policy` trait is the decision maker:

```rust
trait Policy<Req, Res, E> {
    type Future: Future<Output = ()>;

    /// Return Some(delay_future) to retry, None to stop.
    /// MAY mutate req (e.g., change provider header).
    /// MAY mutate result (convert failure to success).
    fn retry(&mut self, req: &mut Req, result: &mut Result<Res, E>) -> Option<Self::Future>;

    /// Clone the request for retry. None = cannot retry.
    fn clone_request(&mut self, req: &Req) -> Option<Req>;
}
```

### Critical Design Features

1. **Request mutation on retry**: The policy can CHANGE the request before retrying.
   This is exactly what `retry_with_provider` needs -- mutate the provider field.

2. **Result mutation**: The policy can convert `Err` to `Ok`. This is the `ignore`
   pattern -- convert a failure into a default value.

3. **Retry budget**: Tower provides `tower::retry::budget::Budget` to limit retries
   across ALL requests (not just per-request). This prevents retry storms.

4. **Composability**: `Retry<P, S>` is itself a `Service`, so you can stack:
   `Retry<FallbackPolicy, Retry<SameProviderPolicy, InnerService>>`.

### Adaptation for Nika

Tower's Policy pattern maps cleanly to nika's task execution:

```
Request  = (TaskAction, Bindings)
Response = String (task output)
Error    = NikaError
Policy   = on_error configuration
```

The key insight: **Tower separates the retry DECISION from the retry EXECUTION**.
The policy says "yes, retry" and optionally mutates the request; the middleware
handles the actual re-execution. Nika can adopt this separation.

However, Tower's retry is designed for **stateless** services. Nika tasks have
**side effects** (LLM calls cost money, exec commands mutate filesystems). The
retry budget concept becomes critical to prevent runaway costs.

---

## 4. backon -- Retry with Backoff (Already in Nika deps)

### Current Usage

Nika uses backon 1.3 (1.6 available in cache) in `nika-mcp/src/retry.rs` for MCP
call retrying with exponential backoff.

### API Surface (v1.6)

```rust
// Basic retry
operation.retry(ExponentialBuilder::default())
    .when(|e| is_retryable(e))       // Filter retryable errors
    .notify(|e, dur| log(e, dur))    // Observe retries
    .adjust(|e, dur| custom_dur(e))  // Dynamic backoff (e.g., Retry-After header)
    .await;

// Context-passing retry (for mutable state like provider index)
|mut ctx: RetryCtx| async {
    let result = try_with_provider(&ctx.providers[ctx.idx]).await;
    (ctx, result)
}.retry(builder).context(initial_ctx).await;
```

### Does backon Support Fallback to Different Strategy?

**No.** backon retries the SAME operation with the SAME backoff strategy. It has:

- `.when()` -- filter which errors trigger retry (but same operation)
- `.adjust()` -- modify delay, can return `None` to stop retrying
- `.notify()` -- observe, cannot change behavior
- `RetryableWithContext` -- pass mutable context, but still same operation

**Missing for nika's use case**: There is no built-in way to say "after N retries
with provider A, switch to provider B". The context variant COULD be abused for this
by mutating the context, but it's awkward and not the intended pattern.

### Recommendation

Keep using backon for what it's good at (MCP call retry, HTTP retry). For task-level
`on_error`, implement a custom retry loop in the runner (as nika already does for
structured output validation). The runner already has the right abstraction level.

---

## 5. Circuit Breaker Pattern in Rust

### failsafe-rs

```rust
// failsafe-rs API
let circuit_breaker = Config::new()
    .failure_policy(consecutive_failures(3)) // Open after 3 failures
    .success_policy(consecutive_successes(1)) // Close after 1 success
    .build();

let result = circuit_breaker.call(|| provider.infer(prompt)).await;
// Returns Err(Rejected) if circuit is open -- fast-fail without calling provider
```

**States**: Closed (normal) -> Open (failing, reject all) -> HalfOpen (test one request)

### circuit-breaker-rs

Simpler API, similar concept. Neither crate has seen significant updates recently.

### Relevance for Nika

Circuit breakers are **session-level** state machines -- they track failure rates
across multiple calls to the SAME service. They are relevant for:

1. **Provider health tracking in `nika serve`**: If anthropic has failed 5 times in
   the last minute, automatically route to openai. This is a RUNTIME concern, not
   a YAML config concern.

2. **for_each loops**: If the first 3 iterations all fail on the same provider, a
   circuit breaker could short-circuit the remaining iterations.

3. **NOT for single task execution**: A circuit breaker for a single task with 3
   retries adds no value -- it's just a retry with extra state.

### Recommendation

**Do NOT add circuit breakers to task-level `on_error:`**. They belong in the
provider layer (`nika-engine/src/runtime/executor/infer.rs`) for `nika serve`
scenarios. Consider adding after launch as a performance optimization for
long-running server workloads.

---

## 6. Multi-LLM Router Failover Patterns

### LiteLLM

```python
# LiteLLM router configuration
router = Router(
    model_list=[
        {"model_name": "gpt-4", "litellm_params": {"model": "gpt-4", "api_key": "..."}},
        {"model_name": "gpt-4", "litellm_params": {"model": "claude-3-opus", "api_key": "..."}},
    ],
    routing_strategy="simple-shuffle",  # or: least-busy, latency-based, cost-based
    fallbacks=[{"gpt-4": ["claude-3-opus"]}],  # Explicit fallback mapping
    context_window_fallbacks=[{"gpt-4": ["gpt-4-32k"]}],  # Fallback on context overflow
    num_retries=2,
    retry_after=5,  # seconds
    timeout=120,
    allowed_fails=3,  # Circuit breaker threshold
    cooldown_time=60,  # Circuit breaker cooldown
)
```

**LiteLLM patterns**:
1. **Explicit fallback map**: model X fails -> try model Y
2. **Context window fallback**: If prompt too long -> try model with bigger context
3. **Content policy fallback**: If content filtered -> try different provider
4. **Budget fallback**: If rate limited -> try cheaper model
5. **Error-type-specific routing**: 429 -> different provider, 500 -> retry same, 401 -> fail

### OpenRouter

OpenRouter handles fallback transparently at the API level:

1. Request goes to preferred model
2. If provider is down, automatically routes to another provider hosting same model
3. If model is unavailable, returns error (no cross-model fallback)
4. Supports `transforms: ["middle-out"]` for automatic context window fitting

**Key design**: OpenRouter treats the MODEL as the identity and providers as
interchangeable backends. This differs from nika where providers have distinct
identities and capabilities.

### Martian (multi-model router)

```
Request -> Classifier -> Router -> [Provider A, Provider B, Provider C] -> Response
```

Martian classifies requests by complexity and routes to the cheapest capable model.
Fallback is transparent and cost-optimized.

### Key Insights for Nika

1. **Error-type-specific fallback**: Different error types should trigger different
   fallback strategies:
   - **429 (rate limit)**: Switch provider immediately, no retry on same
   - **500 (server error)**: Retry same provider first, then fallback
   - **401 (auth)**: No retry, no fallback -- permanent failure
   - **Context overflow**: Fallback to model with larger context window
   - **Content filter**: Fallback to different provider (different filter rules)

2. **Nika already has provider fallback** via `routing: { fallback: [a, b, c] }`.
   The gap is: retry exhaustion on ALL providers in the chain should be able to
   trigger a DIFFERENT action (not just fail).

3. **Cost awareness**: LiteLLM tracks cost per provider and avoids burning budget
   on retries. Nika's `on_error:` should emit cost events so users can set limits.

---

## 7. Nika's Current State (Code Analysis)

### What Exists

| Feature | Implementation | Location |
|---------|---------------|----------|
| Task-level retry | Hand-rolled loop with backoff | `runner.rs:1294-1395` |
| Structured output retry | Separate loop with prompt repair | `runner.rs:802-977` |
| Provider fallback chain | `routing: { fallback: [a, b, c] }` | `executor/mod.rs:604-698` |
| MCP retry | backon ExponentialBuilder | `nika-mcp/src/retry.rs` |
| Dependency cascade | DependencyFailed blocks downstream | `runner.rs:577-596` |
| Error classification | `is_retryable()` on NikaError | `runner.rs:1013-1021` |
| Task outcomes | Success, PartialSuccess, Failed, DependencyFailed, Skipped | `store/run_context.rs` |

### What's Missing

1. **No "ignore failure"**: A failed task always blocks downstream via DependencyFailed
2. **No "fallback action"**: Cannot re-execute with different verb/params on failure
3. **No error-type-specific routing**: `is_retryable()` is binary; no way to say
   "on rate limit, switch provider; on auth error, fail immediately"
4. **No default value on failure**: Cannot produce `Value::String("N/A")` on failure

### Architecture Touch Points

The `on_error:` feature touches these files:

1. **AST**: `nika-core/src/ast/analyzed/task.rs` -- add `on_error: Option<OnErrorConfig>`
2. **Parser**: `nika-engine/src/ast/action.rs` -- parse YAML `on_error:` block
3. **Runner**: `runner.rs:1277-1395` -- after retry exhaustion, apply on_error policy
4. **Store**: `run_context.rs` -- new `TaskOutcome::IgnoredFailure` variant
5. **Events**: new `EventKind::OnErrorTriggered` variant
6. **Validator**: Schema validation for on_error config
7. **LSP**: Completion for `on_error:` field

---

## 8. Proposed Design: Three `on_error:` Variants

### Variant 1: `ignore` -- Produce Default Value on Failure

**Semantics**: Task fails after all retries. Instead of cascading DependencyFailed,
store a default value and mark the task as "succeeded with fallback".

**YAML syntax**:

```yaml
- id: optional_enrichment
  infer: "Enrich this data: {{with.raw}}"
  retry: { max_attempts: 2 }
  on_error: ignore                      # Short form: null output, continue DAG

- id: optional_with_default
  infer: "Translate: {{with.text}}"
  on_error:
    strategy: ignore
    default: "Translation unavailable"  # Custom default value
```

**Implementation sketch** (in `runner.rs`, after the retry loop):

```rust
// After retry loop produces a TaskResult with Failed status:
if let TaskOutcome::Failed(ref error) = task_result.status {
    if let Some(on_error) = &task.on_error {
        match on_error.strategy {
            OnErrorStrategy::Ignore => {
                let default_value = on_error.default.clone()
                    .unwrap_or(Value::Null);

                event_log.emit(EventKind::OnErrorTriggered {
                    task_id: Arc::clone(&task_id),
                    strategy: "ignore".into(),
                    original_error: error.clone(),
                });

                task_result = TaskResult {
                    output: Arc::new(default_value),
                    duration: task_result.duration,
                    status: TaskOutcome::IgnoredFailure {
                        original_error: error.clone(),
                    },
                    media: vec![],
                };
            }
            // ... other strategies
        }
    }
}
```

**TaskOutcome change**:

```rust
pub enum TaskOutcome {
    Success,
    PartialSuccess { error_summary: String, succeeded: u32, failed: u32 },
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
    // NEW:
    IgnoredFailure {
        original_error: String,
    },
}
```

`IgnoredFailure` must return `true` from `is_usable()` so downstream tasks can proceed.

**Dependency gating change** in `is_completed_successfully()`:

```rust
pub fn is_completed_successfully(&self, task_id: &str) -> Option<bool> {
    self.results.get(task_id).map(|r| r.value().is_usable())
    // is_usable() already returns true for Success and PartialSuccess.
    // Add IgnoredFailure to is_usable():
    // matches!(self.status, Success | PartialSuccess { .. } | IgnoredFailure { .. })
}
```

---

### Variant 2: `retry_with_provider` -- Switch Provider on Exhaustion

**Semantics**: After primary retry loop exhausts all attempts on the configured
provider (or routing chain), try a DIFFERENT provider as a last resort.

**Important distinction from `routing: { fallback: [...] }`**: The existing routing
fallback tries providers on FIRST failure (no retry per provider). `retry_with_provider`
retries the primary provider N times, THEN switches. This is useful when:
- Primary provider has intermittent issues (retry helps)
- Fallback provider is expensive (don't use it unless primary truly failed)

**YAML syntax**:

```yaml
- id: critical_inference
  provider: anthropic
  model: claude-sonnet-4-20250514
  infer: "Analyze: {{with.data}}"
  retry: { max_attempts: 3 }
  on_error:
    strategy: retry_with_provider
    provider: openai
    model: gpt-4o                       # Optional model override
    max_attempts: 2                     # Retries on fallback provider
```

**Implementation sketch** (in `runner.rs`):

```rust
OnErrorStrategy::RetryWithProvider { provider, model, max_attempts } => {
    event_log.emit(EventKind::OnErrorTriggered {
        task_id: Arc::clone(&task_id),
        strategy: format!("retry_with_provider({})", provider),
        original_error: error.clone(),
    });

    // Clone and mutate the action with new provider/model
    let mut fallback_action = lowered_action.clone();
    match &mut fallback_action {
        TaskAction::Infer { infer } => {
            infer.provider = Some(ProviderName::parse(&provider));
            infer.provider_chain = None;
            if let Some(ref m) = model {
                infer.model = Some(m.clone());
            }
        }
        TaskAction::Agent { agent } => {
            agent.provider = Some(ProviderName::parse(&provider));
            agent.provider_chain = None;
            if let Some(ref m) = model {
                agent.model = Some(m.clone());
            }
        }
        _ => {
            // Non-LLM verbs: on_error.retry_with_provider is a schema error
            // (caught at validation time, not here)
        }
    }

    // Execute fallback with its own retry loop
    let fallback_max = max_attempts.unwrap_or(1);
    let mut fallback_result = None;

    for attempt in 1..=fallback_max {
        match executor
            .execute(&task_id, &fallback_action, &bindings, &datastore, effective_output.as_ref())
            .await
        {
            Ok(output) => {
                fallback_result = Some(TaskResult::success_str(output, start.elapsed()));
                break;
            }
            Err(e) if attempt < fallback_max && Self::is_retryable(&e) => {
                // Retry on fallback provider
                continue;
            }
            Err(e) => {
                fallback_result = Some(TaskResult::failed(e.to_string(), start.elapsed()));
                break;
            }
        }
    }

    if let Some(result) = fallback_result {
        task_result = result;
    }
}
```

**Cost guard**: This pattern can double LLM costs. Emit `EventKind::OnErrorCostWarning`
with estimated cost of fallback execution. Respect `limits.max_cost_usd` if set.

---

### Variant 3: `fallback_action` -- Execute Different Action on Failure

**Semantics**: On failure, execute a COMPLETELY DIFFERENT action. The fallback action
can use a different verb, different prompt, different tool. The original task's bindings
are available, plus `$error` containing the failure details.

**YAML syntax**:

```yaml
- id: web_search
  fetch:
    url: "https://api.search.com/q={{with.query | shell}}"
    extract: article
  on_error:
    strategy: fallback
    action:
      infer: "I could not fetch search results. Based on your knowledge, answer: {{with.query}}"
    # Optional: default value if fallback ALSO fails
    default: "Search unavailable"

- id: primary_analysis
  provider: anthropic
  infer: "Deep analysis of {{with.data}}"
  on_error:
    strategy: fallback
    action:
      provider: groq
      model: llama-3.3-70b-versatile
      infer: "Quick analysis of {{with.data}}"
```

**Implementation approach**:

The fallback action is essentially a **synthetic task** -- parsed at AST time,
lowered at runtime, and executed in the same slot as the original task.

```rust
OnErrorStrategy::Fallback { action, default } => {
    event_log.emit(EventKind::OnErrorTriggered {
        task_id: Arc::clone(&task_id),
        strategy: "fallback".into(),
        original_error: error.clone(),
    });

    // The fallback action was parsed at AST time and lowered during planning.
    // Execute it with the same bindings, plus $error context.
    let mut fallback_bindings = bindings.clone();
    fallback_bindings.insert(
        "error".into(),
        Value::String(error.clone()),
    );

    match executor
        .execute(&task_id, &fallback_action_lowered, &fallback_bindings, &datastore, None)
        .await
    {
        Ok(output) => {
            task_result = TaskResult::success_str(output, start.elapsed());
        }
        Err(fallback_err) => {
            // Fallback also failed. Use default if provided.
            if let Some(default_val) = default {
                task_result = TaskResult {
                    output: Arc::new(default_val.clone()),
                    duration: start.elapsed(),
                    status: TaskOutcome::IgnoredFailure {
                        original_error: format!(
                            "primary: {}; fallback: {}",
                            error, fallback_err
                        ),
                    },
                    media: vec![],
                };
            } else {
                // Both primary and fallback failed, no default
                task_result = TaskResult::failed(
                    format!("primary failed: {}; fallback failed: {}", error, fallback_err),
                    start.elapsed(),
                );
            }
        }
    }
}
```

---

## 9. AST Design

### OnErrorConfig (in nika-core)

```rust
/// Error recovery configuration for a task.
///
/// Executed AFTER retry exhaustion. Three strategies:
/// - `ignore`: produce default value, continue DAG
/// - `retry_with_provider`: re-execute with different LLM provider
/// - `fallback`: execute a completely different action
#[derive(Debug, Clone, PartialEq)]
pub enum OnErrorConfig {
    /// Produce a default value and continue (task treated as succeeded).
    Ignore {
        /// Default value to produce. None = Value::Null.
        default: Option<serde_json::Value>,
    },

    /// Retry with a different provider after primary retry exhaustion.
    /// Only valid for infer: and agent: verbs.
    RetryWithProvider {
        /// Fallback provider name.
        provider: ProviderName,
        /// Optional model override for the fallback provider.
        model: Option<String>,
        /// Max attempts on the fallback provider (default: 1).
        max_attempts: u32,
    },

    /// Execute a completely different action on failure.
    Fallback {
        /// The fallback action to execute.
        action: Box<AnalyzedTaskAction>,
        /// Provider for the fallback action (optional, inherits from task).
        provider: Option<ProviderName>,
        /// Model for the fallback action (optional).
        model: Option<String>,
        /// Default value if fallback also fails (optional).
        default: Option<serde_json::Value>,
    },
}
```

### YAML Parsing

```yaml
# Short form
on_error: ignore

# Full form -- ignore with default
on_error:
  strategy: ignore
  default: "N/A"

# Full form -- retry_with_provider
on_error:
  strategy: retry_with_provider
  provider: openai
  model: gpt-4o
  max_attempts: 2

# Full form -- fallback action
on_error:
  strategy: fallback
  action:
    infer: "Fallback prompt: {{with.data}}"
  provider: groq
  model: llama-3.3-70b-versatile
  default: "Everything failed"
```

### Validation Rules

1. `retry_with_provider` is only valid on `infer:` and `agent:` verbs
2. `fallback.action` cannot itself have `on_error:` (no recursion)
3. `fallback.action` cannot have `for_each:`, `depends_on:`, `artifact:` (it's inline)
4. `default` value is a JSON literal -- string, number, object, array, null
5. `on_error:` is applied AFTER `retry:` exhaustion, never instead of it
6. `retry_with_provider.max_attempts` capped at 5 (prevent cost explosion)

---

## 10. Event Model

```rust
pub enum EventKind {
    // ... existing events ...

    /// on_error: strategy was triggered after retry exhaustion
    OnErrorTriggered {
        task_id: Arc<str>,
        strategy: String,        // "ignore", "retry_with_provider(openai)", "fallback"
        original_error: String,
    },

    /// on_error: fallback action also failed
    OnErrorFallbackFailed {
        task_id: Arc<str>,
        primary_error: String,
        fallback_error: String,
        used_default: bool,      // true if default value was used
    },
}
```

---

## 11. Execution Order (Full Picture)

```
Task execution flow with on_error:

1. Resolve bindings (with:)
2. Check when: condition --> skip if false
3. Execute action (infer/exec/fetch/invoke/agent)
   |
   |--> Success? --> Store result, continue DAG
   |
   |--> Failure?
         |
         |--> retry: configured?
         |     |
         |     |--> Retry loop (max_attempts, backoff)
         |     |     |
         |     |     |--> Success on retry? --> Store result, continue
         |     |     |--> All retries exhausted? --> Continue to on_error
         |     |
         |     |--> retry not configured? --> Continue to on_error
         |
         |--> on_error: configured?
         |     |
         |     |--> ignore: Store default, mark IgnoredFailure, continue DAG
         |     |
         |     |--> retry_with_provider: Execute on fallback provider
         |     |     |--> Success? --> Store result, continue DAG
         |     |     |--> Failure? --> Mark Failed (no further recovery)
         |     |
         |     |--> fallback: Execute fallback action
         |           |--> Success? --> Store result, continue DAG
         |           |--> Failure?
         |                 |--> default configured? --> Store default, continue
         |                 |--> no default? --> Mark Failed
         |
         |--> on_error not configured?
               |
               |--> Mark Failed
               |--> Cascade DependencyFailed to downstream
```

---

## 12. Crate Recommendations

| Need | Recommendation | Rationale |
|------|---------------|-----------|
| Retry with backoff | Keep `backon` for MCP/HTTP; custom loop for task-level | backon lacks fallback-to-different-operation |
| Circuit breaker | Defer post-launch | Only useful in `nika serve` for provider health |
| Tower patterns | Adopt `Policy` concept (not the crate) | Policy trait pattern maps to OnErrorConfig |
| Error classification | Extend `is_retryable()` to return enum | Need: Retryable, NotRetryable, SwitchProvider |

### No New Dependencies Needed

The `on_error:` feature requires zero new crates. All three strategies can be
implemented in the existing runner loop with the existing executor infrastructure.
The provider fallback mechanism already exists in `execute_with_routing()` and
can be reused for `retry_with_provider`.

---

## 13. Implementation Priority

### Phase 1: `ignore` (low risk, high value)

- ~150 LOC change
- Unblocks non-critical optional tasks in DAGs
- Touch points: TaskOutcome enum, runner.rs, AST parser, validator
- 15-20 new tests

### Phase 2: `retry_with_provider` (medium risk, high value)

- ~250 LOC change
- Reuses existing provider resolution infrastructure
- Touch points: runner.rs retry loop, AST parser, validator, events
- 20-25 new tests (including wiremock provider simulation)

### Phase 3: `fallback` (higher complexity, medium value)

- ~400 LOC change
- Requires lowering fallback action at AST time
- Touch points: AST analyzer, lower.rs, runner.rs, validator
- 25-30 new tests (including fallback-of-fallback prevention)

---

## 14. Interaction with Existing Features

### with `routing: { fallback: [...] }`

`routing.fallback` operates INSIDE a single execution attempt. `on_error.retry_with_provider`
operates AFTER all attempts (including routing fallback) fail. They compose:

```yaml
- id: critical
  routing:
    fallback: [anthropic, openai]          # Try openai if anthropic key missing
  retry: { max_attempts: 3 }              # Retry 3x on transient errors
  on_error:
    strategy: retry_with_provider
    provider: groq                         # Last resort: different provider class
    model: llama-3.3-70b-versatile
```

Execution: anthropic fails? -> openai. openai fails? -> retry (3x on routing chain).
All retries exhausted? -> groq (on_error).

### with `structured: { schema: ... }`

Structured output validation retries happen BEFORE task-level retry. If structured
validation exhausts `max_retries`, the task fails, triggering task-level retry, then
`on_error:`. The `on_error: ignore` with a default value is useful here to provide
a valid-but-generic JSON object matching the schema.

### with `for_each:`

`on_error:` applies to EACH ITERATION independently. If iteration 3 of 10 fails:
- `on_error: ignore` -> iteration 3 produces null/default, other 9 proceed
- Already compatible with `fail_fast: false`

### with `when:`

`when:` is evaluated BEFORE execution. `on_error:` is evaluated AFTER failure.
No interaction.

### with `--resume`

On `--resume`, tasks with `IgnoredFailure` outcome should be treated as completed
(not re-executed). They already "succeeded" from the DAG's perspective.

---

## Sources

1. Temporal SDK Core (Rust) -- github.com/temporalio/sdk-core (activity retry policy)
2. Tower 0.5.3 -- local source: `~/.cargo/registry/src/*/tower-0.5.3/src/retry/`
3. backon 1.6.0 -- local source: `~/.cargo/registry/src/*/backon-1.6.0/src/`
4. Airflow documentation -- trigger_rules, callbacks, retry mechanics
5. Prefect documentation -- state handlers, transactions, retries
6. LiteLLM router -- github.com/BerriAI/litellm (fallback, routing strategies)
7. OpenRouter -- openrouter.ai/docs (transparent provider failover)
8. Nika source code -- `runner.rs`, `executor/mod.rs`, `retry.rs`, `run_context.rs`

## Methodology

- Local source analysis: backon 1.6, tower 0.5.3, nika engine codebase
- Nika architecture review: runner loop, executor routing, TaskOutcome/TaskResult
- Pattern synthesis from 6 domains into 3 concrete on_error variants
- Code sketches tested against actual nika type signatures

## Confidence Level

**High** -- Based on direct source code analysis of nika's internals and the referenced
crates. The proposed design aligns with existing abstractions (TaskOutcome, routing chain,
retry loop) and requires no architectural changes to the DAG execution model.
