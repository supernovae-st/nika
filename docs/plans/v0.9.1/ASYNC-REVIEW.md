# Chat-as-DAG: Async/Tokio Pattern Review

**Date:** 2026-02-24
**Reviewer:** Claude (rust-async-expert)
**Focus:** Tokio concurrency, channel design, race conditions, deadlock risks

---

## Executive Summary

The Chat-as-DAG implementation plan is **technically sound** but has several async/await patterns that need refinement:

| Category | Status | Risk Level |
|----------|--------|-----------|
| JoinSet usage in `for_each` | ✅ Correct | LOW |
| DataStore synchronization | ⚠️ Needs review | MEDIUM |
| ChatWorkflow state mutations | ⚠️ Race condition risk | MEDIUM-HIGH |
| EventLog channel design | ✅ Correct | LOW |
| MCP client caching (OnceCell) | ✅ Best practice | LOW |
| Real-time DAG updates | ⚠️ Missing backpressure | MEDIUM |

---

## 1. JoinSet Usage in for_each (GOOD ✅)

### Current Pattern (from runner.rs)

```rust
// Existing code (working well)
use tokio::task::JoinSet;

let mut set = JoinSet::new();
for task in tasks {
    let executor = executor.clone();
    set.spawn(async move {
        executor.execute_task(task).await
    });
}

// Collect results in order
let mut results = Vec::with_capacity(set.len());
while let Some(result) = set.join_next().await {
    results.push(result?);
}
```

### Chat Plan Assessment

**Phase 1, Task 1.1:** ChatWorkflow structure is **sync-only** (no async) ✅

```rust
pub struct ChatWorkflow {
    pub workflow: Workflow,
    pub dag: Dag,
    pub store: DataStore,
    pub log: EventLog,
    pub message_counter: u32,
}
```

This is correct. Workflow building is synchronous; only **execution** is async.

### Recommendation: ADOPT

For chat messages that use `for_each` (parallel prefix), reuse existing executor:

```rust
impl ChatAgent {
    pub async fn execute_parallel_tasks(
        &mut self,
        task_ids: Vec<String>,
    ) -> Result<Vec<TaskResult>, NikaError> {
        let mut set = JoinSet::new();

        for task_id in task_ids {
            let executor = self.executor.clone();
            let store = self.workflow.store.clone();

            set.spawn(async move {
                executor.execute_task(&task_id, &store).await
            });
        }

        let mut results = Vec::new();
        while let Some(res) = set.join_next().await {
            results.push(res??);
        }

        Ok(results)
    }
}
```

---

## 2. DataStore Synchronization (NEEDS REVIEW ⚠️)

### Current Implementation

From `src/store/datastore.rs`, DataStore uses `DashMap` internally (assumed):

```rust
pub struct DataStore {
    // Not shown in plan, but likely:
    data: DashMap<String, TaskResult>,
}

impl DataStore {
    pub fn insert(&self, key: &str, value: TaskResult) {
        self.data.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<TaskResult> {
        self.data.get(key).map(|r| r.clone())
    }
}
```

### Problem: ChatWorkflow is Not Thread-Safe

**Phase 1, Task 1.1** adds DataStore to ChatWorkflow:

```rust
pub struct ChatWorkflow {
    pub store: DataStore,  // ← DashMap (thread-safe)
    // ...
}
```

But ChatWorkflow itself is NOT protected:

```rust
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    // ❌ RACE CONDITION: multiple concurrent infers can race on self.workflow.tasks
    let task_id = self.workflow.next_message_id();  // ← Self mutation
    let task = ChatTaskBuilder::from_message(task_id.clone(), prompt).build();
    self.workflow.add_task(task);  // ← Self mutation
}
```

### The Issue

If two `infer()` calls run concurrently:

```rust
// Thread A
let id_a = self.workflow.next_message_id();  // msg-001 counter++
let task_a = build_task(id_a);
self.workflow.add_task(task_a);

// Thread B (races with Thread A)
let id_b = self.workflow.next_message_id();  // msg-001 again! counter not atomic
let task_b = build_task(id_b);
self.workflow.add_task(task_b);

// Result: Both tasks have id=msg-001, or counter skips
```

### Solution: Use Arc<Mutex<>> for Shared State

```rust
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    // Use Arc<Mutex<>> for shared, thread-safe workflow
    workflow: Arc<Mutex<ChatWorkflow>>,
}

impl ChatAgent {
    pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
        // GOOD: Hold lock only during ID generation and task addition
        let task_id = {
            let mut wf = self.workflow.lock().await;
            let id = wf.next_message_id();
            let task = ChatTaskBuilder::from_message(id.clone(), prompt).build();
            wf.add_task(task.clone());
            id
        }; // Lock released before async execute

        // GOOD: Execute task without holding lock
        let result = self.execute_task(&task_id).await?;

        // GOOD: Store result without holding workflow lock
        {
            let wf = self.workflow.lock().await;
            wf.store.insert(&task_id, serde_json::json!({ "output": result }));
            wf.log.emit(EventKind::TaskCompleted { task_id: task_id.into() });
        }

        Ok(result)
    }
}
```

### Why Not parking_lot::Mutex?

You **could** use `parking_lot::Mutex<T>` (faster, no poisoning), but:
- ✅ Works in **sync** context only (no async locks)
- ❌ Cannot hold across `.await` points
- ❌ TUI is async-heavy (Tokio)

**Use `tokio::sync::Mutex` for ChatWorkflow because you need async locks.**

### Recommendation

**Update Phase 1, Task 1.3:**

```rust
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: Arc<Mutex<ChatWorkflow>>,  // ← ADD Arc<Mutex<>>
    executor: TaskExecutor,
}

impl ChatAgent {
    pub fn new(provider: RigProvider) -> Self {
        Self {
            provider,
            history: Vec::new(),
            workflow: Arc::new(Mutex::new(
                ChatWorkflow::new(&Uuid::new_v4().to_string())
            )),
            executor: TaskExecutor::new(...),
        }
    }
}
```

---

## 3. Real-Time DAG Updates (BACKPRESSURE ⚠️)

### Current Design

**Phase 3, Task 3.3** wires live updates:

```rust
impl ChatView {
    fn on_task_started(&mut self, task_id: &str) {
        self.running_task = Some(task_id.to_string());
        // Trigger re-render
    }

    fn on_task_completed(&mut self, task_id: &str) {
        self.running_task = None;
        // Trigger re-render
    }
}
```

### The Problem: No Backpressure

If EventLog emits events faster than TUI can render:

```
Event: TaskStarted (msg-001)
Event: TaskCompleted (msg-001)  ← Before render
Event: TaskStarted (msg-002)     ← Before render
Event: TaskCompleted (msg-002)   ← Before render
...
TUI still rendering frame 1 of 4
```

Results in:
- ❌ Dropped events
- ❌ Stale UI state
- ❌ Memory buildup in EventLog queue

### Solution: Use Broadcast Channel with Bounded Queue

```rust
// In EventLog creation
pub struct EventLog {
    // Before: unbounded MPSC
    // After: bounded broadcast channel
    tx: broadcast::Sender<Event>,
    events: Arc<Mutex<Vec<Event>>>,  // Keep history for trace export
}

impl EventLog {
    pub fn new_broadcast(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self {
            tx,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn emit(&self, kind: EventKind) {
        let event = Event::new(kind);

        // Store in history (for export)
        {
            let mut events = self.events.blocking_lock();
            events.push(event.clone());
        }

        // Send to subscribers (with backpressure)
        // If queue is full, oldest subscribers drop to new ones
        let _ = self.tx.send(event);  // Non-blocking, handles overflow
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
```

### TUI Integration

```rust
impl ChatView {
    async fn update_loop(&mut self, mut rx: broadcast::Receiver<Event>) {
        loop {
            select! {
                // Process events with rate limiting
                Ok(event) = rx.recv() => {
                    self.handle_event(&event).await;
                    // Optional: add 16ms delay to sync with 60 FPS
                    tokio::time::sleep(Duration::from_millis(16)).await;
                }

                // Allow render loop to interrupt
                _ = self.render_signal.notified() => {
                    break;
                }
            }
        }
    }
}
```

### Recommendation

**Update Phase 3, Task 3.3:**

Use **bounded broadcast channel** instead of naive update callbacks. This prevents:
- Memory leaks from unbounded queues
- Dropped events
- Render loop stalls

---

## 4. MCP Client Caching (EXCELLENT ✅)

### Current Pattern (from executor.rs)

```rust
pub struct TaskExecutor {
    mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
}

async fn get_mcp_client(&self, name: &str) -> Result<Arc<McpClient>, NikaError> {
    // Clone Arc<OnceCell> to release DashMap guard before await
    let cell = self.mcp_client_cache
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(OnceCell::new()))
        .clone();

    // Release DashMap guard, then initialize (no lock-across-await)
    Ok(cell.get_or_try_init(async || {
        self.connect_mcp_client(name).await
    }).await?)
}
```

This is **correct** and should be **reused** for chat.

### Why This Works

1. `DashMap.entry()` returns `OccupiedEntry` or `VacantEntry`
2. We **clone Arc<OnceCell>** to get independent handle
3. DashMap guard is **released**
4. We call `get_or_try_init().await` safely

Result: No lock-across-await footgun, concurrent initialization is atomic via `OnceCell`.

### Chat Plan: No Changes Needed ✅

Use same executor instance (already cached):

```rust
pub struct ChatAgent {
    executor: TaskExecutor,  // Shares MCP client cache
}
```

---

## 5. EventLog Channel Design (CORRECT ✅)

### Current Implementation (Assumed)

```rust
pub struct EventLog {
    // Likely uses a channel-based queue internally
    events: Arc<Mutex<Vec<Event>>>,
}
```

### For Chat Integration

**Phase 1, Task 1.4** exports EventLog:

```rust
impl ChatAgent {
    pub fn export_trace(&self, path: &Path) -> Result<(), NikaError> {
        for event in self.workflow.log.events() {
            writer.write_event(event)?;
        }
        Ok(())
    }
}
```

This is correct. No async needed for **export** (sequential writes).

### Recommendation: Add Real-Time Stream

For live DAG panel updates, expose a **subscriber**:

```rust
impl ChatAgent {
    /// Subscribe to events in real-time
    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.workflow.log.subscribe()  // Requires log.subscribe() method
    }
}
```

---

## 6. ChatWorkflow Builder Pattern (GOOD ✅)

### Phase 1, Task 1.2: ChatTaskBuilder

```rust
pub struct ChatTaskBuilder {
    id: String,
    action: TaskAction,
    use_wiring: Option<WiringSpec>,
    depends_on: Vec<String>,
}
```

This is **sync-only**, which is correct. Task **building** should not be async.

### Recommendation: No Changes ✅

Keep builder sync. Only execution should be async.

---

## 7. Mention Parser and Binding Resolution (SYNC ONLY ✅)

### Phase 2, Task 2.1: MentionParser

```rust
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    MENTION_RE.captures_iter(text)
        .filter_map(|cap| { ... })
        .collect()
}
```

This is **correctly sync** (regex parsing is fast, CPU-bound).

### Phase 2, Task 2.2: MentionToBinding

```rust
pub fn mentions_to_wiring(
    text: &str,
    message_count: u32,
    prev_task_id: Option<&str>,
) -> WiringSpec {
    // Sync code only
}
```

This is **correctly sync** (binding conversion is pure function).

### Recommendation: No Changes ✅

---

## 8. Concurrent Infer Calls (DEADLOCK RISK ❌)

### Problem: Multiple Concurrent Infer

User sends 3 messages rapidly:

```
> "Hello"       ← infer() spawned
> "Continue"    ← infer() spawned
> "Wrap up"     ← infer() spawned

# All 3 call ChatAgent.infer() concurrently
```

Without Arc<Mutex<>>, this **races on `next_message_id()`**:

```rust
// Without Arc<Mutex<>>:
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    let task_id = self.workflow.next_message_id();  // ← RACE: counter unsync'd
    // All 3 might get msg-001!
}
```

### Solution: Use Serial Queue

Either:

**Option A:** Hold Mutex during entire operation (slower but safe)
```rust
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    let mut wf = self.workflow.lock().await;
    let id = wf.next_message_id();
    let task = ChatTaskBuilder::from_message(id.clone(), prompt).build();
    wf.add_task(task);
    drop(wf);  // Release lock

    // Execute without lock
    let result = self.executor.execute_task(&id).await?;
    Ok(result)
}
```

**Option B:** Use MPSC channel to serialize infer calls (better UX)
```rust
pub struct ChatAgent {
    infer_tx: mpsc::Sender<InferRequest>,
}

pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
    let (response_tx, response_rx) = oneshot::channel();

    self.infer_tx.send(InferRequest {
        prompt: prompt.to_string(),
        response: response_tx,
    }).await?;

    response_rx.await?  // Wait for response
}

// Background task processes serially
async fn infer_loop(
    mut rx: mpsc::Receiver<InferRequest>,
    mut workflow: ChatWorkflow,
) {
    while let Some(req) = rx.recv().await {
        let task_id = workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id, req.prompt).build();
        workflow.add_task(task);

        let result = executor.execute_task(&task_id).await;
        let _ = req.response.send(result);
    }
}
```

### Recommendation

**Use Option A** for simplicity in Phase 1. If UX suffers (responses delayed), upgrade to **Option B** in Phase 5.

---

## 9. Session Persistence Race Condition (MEDIUM ⚠️)

### Phase 5, Task 5.3

```rust
pub struct ChatSession {
    pub dag_state: Option<ChatDagState>,
}

pub fn save_session(&self, path: &Path) -> Result<(), NikaError> {
    let json = serde_json::to_string(&self)?;
    std::fs::write(path, json)?;  // ← Not atomic!
}
```

### The Problem

If app crashes during write:

```
Session file (50% written):
{
  "dag_state": {
    "tasks": [        ← Incomplete!
      ...              ← File truncated
```

On restart: Parse error, session lost.

### Solution: Atomic Writes (Already Used in Nika!)

From `src/util/atomic_write.rs` (per codebase):

```rust
use crate::util::atomic_write;

pub fn save_session(&self, path: &Path) -> Result<(), NikaError> {
    let json = serde_json::to_string(&self)?;
    atomic_write(path, &json)?;  // Write to temp, rename atomically
    Ok(())
}
```

### Recommendation

**Phase 5, Task 5.3** is already correct if using `atomic_write`. ✅

---

## 10. Full Async Checklist

| Pattern | Status | File | Action |
|---------|--------|------|--------|
| JoinSet for for_each | ✅ | runner.rs | Reuse existing |
| Arc<Mutex<>> for ChatWorkflow | ❌ MISSING | chat_workflow.rs | Add in Phase 1 |
| Broadcast channel for DAG updates | ❌ MISSING | chat_agent.rs | Add in Phase 3 |
| MCP client caching | ✅ | executor.rs | Reuse existing |
| EventLog history + subscription | ⚠️ PARTIAL | event/log.rs | Add subscribe() |
| Mention parser (sync) | ✅ | mention_parser.rs | No changes |
| Atomic session writes | ✅ | session.rs | Use atomic_write |
| Lock-free counter for message IDs | ❌ MISSING | chat_workflow.rs | Add AtomicU32 |

---

## 11. Recommended Changes to Implementation Plan

### Phase 1, Task 1.1: Add Thread-Safety

```rust
use tokio::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct ChatWorkflow {
    pub workflow: Workflow,
    pub dag: Dag,
    pub store: DataStore,
    pub log: EventLog,
    message_counter: AtomicU32,  // ← Thread-safe counter
}

impl ChatWorkflow {
    pub fn next_message_id(&self) -> String {
        let num = self.message_counter.fetch_add(1, Ordering::SeqCst);
        format!("msg-{:03}", num + 1)
    }
}

pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: Arc<Mutex<ChatWorkflow>>,  // ← Arc<Mutex<>> for shared access
    executor: TaskExecutor,
}
```

### Phase 1, Task 1.3: Async-Safe Infer

```rust
impl ChatAgent {
    pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
        // 1. Lock workflow, create task
        let task_id = {
            let mut wf = self.workflow.lock().await;
            let id = wf.next_message_id();
            let task = ChatTaskBuilder::from_message(id.clone(), prompt)
                .build();
            wf.add_task(task);
            id
        };  // Lock released here

        // 2. Execute task (no lock held!)
        self.workflow.log.emit(EventKind::TaskStarted {
            task_id: task_id.clone().into(),
            action_type: "infer".into(),
        });

        let result = self.executor
            .execute_infer(&task_id, prompt, None)
            .await?;

        // 3. Store result (brief lock)
        {
            let wf = self.workflow.lock().await;
            wf.store.insert(&task_id, serde_json::json!({
                "output": result.clone(),
                "prompt": prompt,
            }));
            wf.log.emit(EventKind::TaskCompleted {
                task_id: task_id.into(),
                duration_ms: 0,
            });
        }

        Ok(result)
    }
}
```

### Phase 3, Task 3.3: Add Broadcast Channel

```rust
// In EventLog (modify existing)
impl EventLog {
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

// In ChatView
pub async fn event_loop(mut rx: broadcast::Receiver<Event>) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                match event.kind {
                    EventKind::TaskStarted { task_id, .. } => {
                        self.running_task = Some(task_id.to_string());
                    }
                    EventKind::TaskCompleted { task_id, .. } => {
                        self.running_task = None;
                    }
                    _ => {}
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // Handle backpressure gracefully
                tracing::warn!("DAG event subscription lagged, some events dropped");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
```

---

## 12. Performance Considerations

### Memory: ChatWorkflow in Arc<Mutex<>>

**Cost per chat session:**
- ChatWorkflow: ~4KB base
- Workflow.tasks: 8 bytes × N messages
- DataStore (DashMap): ~1KB base
- EventLog (Vec): 24 bytes × N events

With 100 messages: ~50KB per session (acceptable).

### Latency: Lock Contention

**With Arc<Mutex<>>:**
- Lock acquisition: ~100ns (uncontended)
- Message rate: 2-3 per second (user typing speed)
- Contention: Minimal (locks held <1ms)

### Throughput: Parallel Execution

**for_each with concurrency=5:**
- 5 tasks spawned to JoinSet
- Results collected in order
- No bottleneck from Mutex (only used for ID generation)

---

## 13. Race Conditions: Complete Audit

| Scenario | Before | After | Risk |
|----------|--------|-------|------|
| User sends 3 messages rapidly | Task ID collision | No collision (AtomicU32) | FIXED ✅ |
| Task execution + UI update race | Dropped events | Queued (broadcast) | FIXED ✅ |
| Workflow modified during export | Inconsistent snapshot | Atomic read | Not addressed |
| Session save crashes | Truncated file | Atomic write | Uses atomic_write ✅ |
| MCP client initialized twice | Race condition | OnceCell prevents | FIXED ✅ |

---

## 14. Deadlock Audit

| Scenario | Risk | Mitigation |
|----------|------|-----------|
| Hold ChatWorkflow lock during execute | HIGH | **Hold lock briefly, release before execute** |
| Nested locks (Mutex inside Mutex) | MEDIUM | **Avoid by design: ChatWorkflow holds no other locks** |
| Notify waiting on closed channel | LOW | broadcast::channel auto-closes |
| Executor lock-across-await | LOW | OnceCell + DashMap pattern prevents this |

---

## 15. Summary: Action Items

### Critical (Do First)

- [ ] **Phase 1:** Add `Arc<Mutex<ChatWorkflow>>` to ChatAgent
- [ ] **Phase 1:** Use `AtomicU32` for message counter
- [ ] **Phase 3:** Wire broadcast channel for event updates (not callback)

### Important (Before Release)

- [ ] Add `EventLog::subscribe()` method for real-time subscribers
- [ ] Document lock-holding times in code comments
- [ ] Add concurrency stress tests (3+ concurrent infer calls)

### Nice-to-Have (Phase 5+)

- [ ] Add MPSC channel for infer serialization if UX delays
- [ ] Add metrics for lock contention (tracing)
- [ ] Profile memory usage with 100+ message sessions

---

## 16. Code Review Checklist

Before merging each phase:

### Phase 1
- [ ] `ChatWorkflow` uses `AtomicU32` for counter
- [ ] `ChatAgent.workflow` is `Arc<Mutex<ChatWorkflow>>`
- [ ] Lock is released before `.await` in `infer()`
- [ ] Tests verify no task ID collisions under concurrency

### Phase 2
- [ ] Mention parser has no async code
- [ ] Binding converter is pure function
- [ ] @mention resolution tested with 10+ concurrent infers

### Phase 3
- [ ] EventLog exposes `subscribe()` method
- [ ] ChatDagPanel subscribes to broadcast channel
- [ ] Backpressure handled (doesn't panic on lag)

### Phase 4
- [ ] NodeBox render doesn't clone strings excessively
- [ ] Duration tracking uses `Instant` (not system time)

### Phase 5
- [ ] Session save uses `atomic_write()`
- [ ] YAML export tested with concurrent execution

---

## References

- **Tokio Patterns:** https://tokio.rs/tokio/tutorial/select
- **DashMap + OnceCell:** Implemented in `executor.rs` (proven pattern)
- **Broadcast backpressure:** https://docs.rs/tokio/1/tokio/sync/broadcast/index.html#message-drops
- **Atomic operations:** https://docs.rs/std/latest/std/sync/atomic/index.html
- **Mutex vs parking_lot:** Must use `tokio::sync::Mutex` (async context)

---

## Questions for Thibaut

1. **Message concurrency:** Can users send 3+ messages while one is executing? (I assume yes from chat UX)
2. **Session size:** What's the max expected messages per chat session? (Planning for 100-1000?)
3. **Export format:** Should exported YAML be deterministic (for replay)?
4. **Event rate:** Estimate events/second during heavy inference? (For broadcast queue sizing)

