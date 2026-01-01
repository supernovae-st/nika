# Event Sourcing Implementation Plan

> **Goal**: Full audit trail with replay capability for Nika workflows
> **User Choice**: Option C (Full Event Sourcing) + Fine-grained events + Hybrid struct/enum

---

## Research Insights

| Source | Key Pattern |
|--------|-------------|
| **eventually-rs** (590★) | Aggregate trait + apply(), Event Store, CQRS |
| **eventlogs** crate | LogManager, idempotency, optimistic concurrency, reduction caching |
| **Perplexity** | Arc for concurrent sharing, envelope pattern (type + correlation_id + payload + timestamp) |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Event (struct) - Envelope pattern                                   │
│  ├── id: u64           (monotonic sequence, AtomicU64)              │
│  ├── timestamp: Duration (relative to workflow start)              │
│  └── kind: EventKind   (typed event data)                           │
├─────────────────────────────────────────────────────────────────────┤
│  EventKind (enum) - 12 variants, 3 levels                           │
│                                                                     │
│  WORKFLOW LEVEL:                                                    │
│  ├── WorkflowStarted { task_count }                                 │
│  ├── WorkflowCompleted { final_output, duration }                   │
│  └── WorkflowFailed { error, failed_task }                          │
│                                                                     │
│  TASK LEVEL:                                                        │
│  ├── TaskScheduled { task_id, dependencies }                        │
│  ├── InputsResolved { task_id, inputs: Value }  ← ORIGINAL REQUEST  │
│  ├── TaskStarted { task_id }                                        │
│  ├── TaskCompleted { task_id, output, duration }                    │
│  └── TaskFailed { task_id, error, duration }                        │
│                                                                     │
│  FINE-GRAINED:                                                      │
│  ├── TemplateResolved { task_id, template, result }                 │
│  ├── ProviderCalled { task_id, provider, model, prompt_len }        │
│  ├── ProviderResponded { task_id, output_len, tokens }              │
│  └── SchemaValidated { task_id, schema_path, valid }                │
├─────────────────────────────────────────────────────────────────────┤
│  EventLog (struct) - Thread-safe, append-only                       │
│  ├── events: Arc<RwLock<Vec<Event>>>                                │
│  ├── start_time: Instant                                            │
│  └── next_id: Arc<AtomicU64>                                        │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Code Design

### Event Types (src/event.rs)

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use serde_json::Value;

/// Single event in the workflow execution log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic sequence ID (for ordering)
    pub id: u64,
    /// Time since workflow start
    pub timestamp: Duration,
    /// Event type and data
    pub kind: EventKind,
}

/// All possible event types (3 levels as user chose)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    // ═══════════════════════════════════════════
    // WORKFLOW LEVEL
    // ═══════════════════════════════════════════
    WorkflowStarted {
        task_count: usize,
    },
    WorkflowCompleted {
        final_output: Value,
        total_duration_ms: u64,
    },
    WorkflowFailed {
        error: String,
        failed_task: Option<Arc<str>>,
    },

    // ═══════════════════════════════════════════
    // TASK LEVEL
    // ═══════════════════════════════════════════
    TaskScheduled {
        task_id: Arc<str>,
        dependencies: Vec<Arc<str>>,
    },
    /// CORE: What each task RECEIVES (the original request!)
    InputsResolved {
        task_id: Arc<str>,
        inputs: Value,
    },
    TaskStarted {
        task_id: Arc<str>,
    },
    TaskCompleted {
        task_id: Arc<str>,
        output: Value,
        duration_ms: u64,
    },
    TaskFailed {
        task_id: Arc<str>,
        error: String,
        duration_ms: u64,
    },

    // ═══════════════════════════════════════════
    // FINE-GRAINED (template/provider/schema)
    // ═══════════════════════════════════════════
    TemplateResolved {
        task_id: Arc<str>,
        template: String,
        result: String,
    },
    ProviderCalled {
        task_id: Arc<str>,
        provider: Arc<str>,
        model: Arc<str>,
        prompt_len: usize,
    },
    ProviderResponded {
        task_id: Arc<str>,
        output_len: usize,
        tokens_used: Option<u32>,
    },
    SchemaValidated {
        task_id: Arc<str>,
        schema_path: String,
        valid: bool,
    },
}
```

### EventLog (src/event.rs)

```rust
/// Thread-safe, append-only event log
#[derive(Clone)]
pub struct EventLog {
    events: Arc<RwLock<Vec<Event>>>,
    start_time: Instant,
    next_id: Arc<AtomicU64>,
}

impl EventLog {
    /// Create a new event log (call at workflow start)
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Emit an event (thread-safe, returns event ID)
    pub fn emit(&self, kind: EventKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let event = Event {
            id,
            timestamp: self.start_time.elapsed(),
            kind,
        };

        self.events.write().unwrap().push(event);
        id
    }

    /// Get all events (cloned)
    pub fn events(&self) -> Vec<Event> {
        self.events.read().unwrap().clone()
    }

    /// Filter events by task ID
    pub fn filter_task(&self, task_id: &str) -> Vec<Event> {
        self.events()
            .into_iter()
            .filter(|e| e.kind.task_id().map(|id| id.as_ref() == task_id).unwrap_or(false))
            .collect()
    }

    /// Serialize to JSON for persistence/debugging
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.events()).unwrap_or(Value::Null)
    }

    /// Number of events
    pub fn len(&self) -> usize {
        self.events.read().unwrap().len()
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl EventKind {
    /// Extract task_id if event is task-related
    pub fn task_id(&self) -> Option<&Arc<str>> {
        match self {
            Self::TaskScheduled { task_id, .. }
            | Self::InputsResolved { task_id, .. }
            | Self::TaskStarted { task_id }
            | Self::TaskCompleted { task_id, .. }
            | Self::TaskFailed { task_id, .. }
            | Self::TemplateResolved { task_id, .. }
            | Self::ProviderCalled { task_id, .. }
            | Self::ProviderResponded { task_id, .. }
            | Self::SchemaValidated { task_id, .. } => Some(task_id),
            _ => None,
        }
    }
}
```

---

## Integration Points

### Runner (src/runner.rs)

```rust
pub struct Runner {
    workflow: Workflow,
    dag: DagAnalyzer,
    datastore: DataStore,
    executor: TaskExecutor,
    event_log: EventLog,  // NEW
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        let dag = DagAnalyzer::from_workflow(&workflow);
        let datastore = DataStore::new();
        let event_log = EventLog::new();  // NEW
        let executor = TaskExecutor::new(
            &workflow.provider,
            workflow.model.as_deref(),
            event_log.clone(),  // PASS TO EXECUTOR
        );
        // ...
    }

    pub async fn run(&self) -> Result<String, NikaError> {
        // EMIT: WorkflowStarted
        self.event_log.emit(EventKind::WorkflowStarted {
            task_count: self.workflow.tasks.len(),
        });

        // ... existing loop ...

        // Inside spawn, after context built:
        // EMIT: InputsResolved
        event_log.emit(EventKind::InputsResolved {
            task_id: task_id.clone().into(),
            inputs: context.to_value(),
        });
        // EMIT: TaskStarted
        event_log.emit(EventKind::TaskStarted {
            task_id: task_id.clone().into(),
        });

        // After execution:
        // EMIT: TaskCompleted or TaskFailed

        // At end:
        // EMIT: WorkflowCompleted
    }

    /// Get the event log for inspection/export
    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }
}
```

### Executor (src/executor.rs)

```rust
pub struct TaskExecutor {
    http_client: reqwest::Client,
    provider_cache: Arc<DashMap<String, Arc<dyn Provider>>>,
    default_provider: Arc<str>,
    default_model: Option<Arc<str>>,
    event_log: EventLog,  // NEW
}

impl TaskExecutor {
    pub fn new(provider: &str, model: Option<&str>, event_log: EventLog) -> Self {
        // ...
    }

    pub async fn execute(
        &self,
        task_id: &str,  // NEW PARAMETER
        action: &TaskAction,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        // ... existing logic with event emissions ...
    }

    async fn execute_infer(
        &self,
        task_id: &str,
        infer: &InferDef,
        context: &TaskContext,
    ) -> Result<String, NikaError> {
        let prompt = template::resolve(&infer.prompt, context)?;

        // EMIT: TemplateResolved
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: task_id.into(),
            template: infer.prompt.clone(),
            result: prompt.to_string(),
        });

        // ... get provider ...

        // EMIT: ProviderCalled
        self.event_log.emit(EventKind::ProviderCalled {
            task_id: task_id.into(),
            provider: provider_name.into(),
            model: model.into(),
            prompt_len: prompt.len(),
        });

        let result = provider.infer(&prompt, model).await?;

        // EMIT: ProviderResponded
        self.event_log.emit(EventKind::ProviderResponded {
            task_id: task_id.into(),
            output_len: result.len(),
            tokens_used: None,  // TODO: if provider returns token count
        });

        Ok(result)
    }
}
```

### TaskContext (src/context.rs)

```rust
impl TaskContext {
    /// Serialize context to JSON Value for event logging
    pub fn to_value(&self) -> Value {
        // If wrapping Value internally:
        self.data.clone()
        // If HashMap-based:
        // serde_json::to_value(&self.data).unwrap_or(Value::Null)
    }
}
```

---

## Implementation Phases

### Phase 1: Event Types (TDD)
```
[ ] Test: event_creation_with_monotonic_id
[ ] Test: eventkind_serialization_tagged
[ ] Impl: Event struct
[ ] Impl: EventKind enum with 12 variants
```

### Phase 2: EventLog (TDD)
```
[ ] Test: eventlog_new_starts_empty
[ ] Test: eventlog_emit_increments_id
[ ] Test: eventlog_thread_safe_append
[ ] Test: eventlog_filter_by_task
[ ] Test: eventlog_to_json
[ ] Impl: EventLog struct
[ ] Impl: emit(), events(), filter_task(), to_json(), len()
```

### Phase 3: Runner Integration
```
[ ] Test: runner_emits_workflow_started
[ ] Test: runner_emits_task_events
[ ] Test: runner_emits_workflow_completed
[ ] Mod: Add EventLog to Runner
[ ] Mod: Pass EventLog to TaskExecutor
[ ] Mod: Emit events at correct points
```

### Phase 4: Executor Integration
```
[ ] Test: executor_emits_template_resolved
[ ] Test: executor_emits_provider_events
[ ] Mod: Add EventLog to TaskExecutor
[ ] Mod: Add task_id parameter to execute()
[ ] Mod: Emit fine-grained events
```

### Phase 5: Context Integration
```
[ ] Test: context_to_value
[ ] Mod: Add to_value() to TaskContext
[ ] Mod: Use in InputsResolved event
```

---

## Key Invariants

| Invariant | Guarantee |
|-----------|-----------|
| **APPEND-ONLY** | Events never mutated after emission |
| **MONOTONIC IDs** | Ordering always deterministic |
| **RELATIVE TIME** | No clock sync issues (Duration from start) |
| **THREAD-SAFE** | Clone + Arc + RwLock for parallel tasks |
| **SERIALIZABLE** | Full JSON export for persistence |

---

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn event_ids_are_monotonic() { ... }

    #[test]
    fn eventlog_thread_safe_concurrent_emits() { ... }

    #[test]
    fn filter_task_returns_only_matching() { ... }

    #[test]
    fn eventkind_serializes_with_type_tag() { ... }

    #[tokio::test]
    async fn runner_full_workflow_audit_trail() { ... }

    #[tokio::test]
    async fn executor_fine_grained_events() { ... }

    #[test]
    fn inputs_resolved_captures_full_context() { ... }
}
```

---

## Replay Capability (Future)

```rust
// Future API for replay
impl EventLog {
    /// Replay events to reconstruct state at any point
    pub fn replay_until(&self, event_id: u64) -> DataStore {
        let mut store = DataStore::new();
        for event in self.events().iter().take_while(|e| e.id <= event_id) {
            match &event.kind {
                EventKind::TaskCompleted { task_id, output, duration_ms } => {
                    store.insert(task_id, TaskResult::success(
                        output.clone(),
                        Duration::from_millis(*duration_ms),
                    ));
                }
                _ => {}
            }
        }
        store
    }
}
```

---

## Summary

This implementation provides:

1. **Original request**: `InputsResolved` event captures what each task receives
2. **Full audit**: 12 event types across workflow/task/fine-grained levels
3. **Replay**: Deterministic ordering with monotonic IDs
4. **Debug**: Filter by task_id, export to JSON
5. **Thread-safe**: Works with parallel task execution

**No new dependencies** - uses std::sync + serde (already present).
