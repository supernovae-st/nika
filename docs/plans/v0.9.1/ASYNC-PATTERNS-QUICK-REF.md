# Async Patterns Quick Reference

**For:** Chat-as-DAG Implementation
**Keep:** Open while coding Phase 1-5

---

## 1. Lock Ordering: GOOD vs BAD

### ❌ BAD: Hold lock during async operation

```rust
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    let mut wf = self.workflow.lock().await;  // Lock acquired
    let id = wf.next_message_id();
    let task = ChatTaskBuilder::from_message(id, prompt).build();
    wf.add_task(task);

    // ❌ DEADLOCK RISK: Lock held across .await
    let result = self.executor.execute_task(&id, prompt).await?;  // Blocked!

    wf.store.insert(&id, result);
    Ok(result)
}
```

**Problem:**
- Lock held for entire LLM execution (~100ms+)
- Other tasks cannot acquire lock
- If second task tries to get ID, deadlock

### ✅ GOOD: Release lock before async

```rust
pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
    let id = {
        let mut wf = self.workflow.lock().await;  // Lock
        let id = wf.next_message_id();
        let task = ChatTaskBuilder::from_message(id.clone(), prompt).build();
        wf.add_task(task);
        id
    };  // ← UNLOCK HERE

    // ✅ No lock held, executor runs freely
    let result = self.executor.execute_task(&id, prompt).await?;

    // ✅ Brief lock for storage
    {
        let wf = self.workflow.lock().await;
        wf.store.insert(&id, result.clone());
    }

    Ok(result)
}
```

**Benefits:**
- Lock held <1ms
- Other tasks proceed immediately
- No deadlock risk

---

## 2. Atomic Operations: When to Use

### ✅ For Message Counter (ALWAYS)

```rust
// Use AtomicU32, not Mutex<u32>
pub struct ChatWorkflow {
    message_counter: Arc<AtomicU32>,
}

impl ChatWorkflow {
    pub fn next_message_id(&self) -> String {
        let num = self.message_counter.fetch_add(1, Ordering::SeqCst);
        format!("msg-{:03}", num + 1)
    }
}
```

**Why:**
- Lock-free (fastest)
- No mutex overhead
- Safe for concurrent access
- Can call from sync context

### ❌ Wrong: Mutex for Counter

```rust
pub struct ChatWorkflow {
    message_counter: Mutex<u32>,  // ❌ Slower, can deadlock
}

pub async fn next_message_id(&mut self) -> String {
    let mut c = self.message_counter.lock().await;
    *c += 1;
    format!("msg-{:03}", *c)
}
```

---

## 3. Broadcast Channel: For Event Streams

### ✅ GOOD: Subscribe to events

```rust
// Create bounded broadcast
let (tx, _rx) = broadcast::channel(1000);

impl EventLog {
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

// Use in TUI
pub async fn event_loop(mut rx: broadcast::Receiver<Event>) {
    loop {
        match rx.recv().await {
            Ok(event) => process_event(&event),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // Handle backpressure
                tracing::warn!("Events dropped");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
```

**Why broadcast (not mpsc):**
- Multiple subscribers (chat + DAG panel)
- Auto-cleanup of slow subscribers
- Bounded memory (prevents OOM)

### ❌ Wrong: Unbounded channel

```rust
let (tx, rx) = mpsc::unbounded_channel();  // ❌ Can OOM
```

---

## 4. DashMap + OnceCell: For Shared Caches

### ✅ GOOD: MCP client caching (already in executor)

```rust
pub struct TaskExecutor {
    mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
}

async fn get_mcp_client(&self, name: &str) -> Result<Arc<McpClient>, NikaError> {
    // Clone Arc<OnceCell> to release DashMap guard
    let cell = self.mcp_client_cache
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(OnceCell::new()))
        .clone();

    // Now we can call .await without holding DashMap lock
    Ok(cell.get_or_try_init(async || {
        self.connect_mcp_client(name).await
    }).await?)
}
```

**Why triple-Arc:**
1. Outer Arc<DashMap> — for Clone
2. Middle Arc<OnceCell> — release guard before await
3. Inner Arc<McpClient> — share across tasks

### ❌ Wrong: Hold DashMap lock across await

```rust
let mut entry = cache.entry(name).or_insert(...);
entry.get_or_init(async { ... }).await  // ❌ Lock held!
```

---

## 5. JoinSet: For Parallel Loops

### ✅ GOOD: for_each with concurrency

```rust
let mut set = JoinSet::new();

for (i, item) in items.iter().enumerate() {
    let executor = executor.clone();
    let item = item.clone();

    set.spawn(async move {
        executor.execute_task(&item).await
    });
}

// Collect in original order
let mut results = Vec::with_capacity(set.len());
while let Some(res) = set.join_next().await {
    results.push(res??);
}
```

**Why JoinSet (not spawn bulk):**
- Efficient task collection
- Ordered results possible
- Fine-grained error handling
- Works with for_each

### ❌ Wrong: spawn + Vec + collect

```rust
let handles: Vec<_> = items.iter()
    .map(|item| tokio::spawn(async move { ... }))
    .collect();

for handle in handles {
    results.push(handle.await??);
}  // ❌ Less ergonomic
```

---

## 6. CancellationToken: For Graceful Shutdown

### ✅ GOOD: Agent shutdown

```rust
use tokio_util::sync::CancellationToken;

pub struct ChatAgent {
    cancel_token: CancellationToken,
}

pub async fn infer_with_cancellation(&self, prompt: &str) -> Result<String, NikaError> {
    select! {
        result = self.executor.execute_task(&id, prompt) => {
            result
        }
        _ = self.cancel_token.cancelled() => {
            Err(NikaError::Cancelled)
        }
    }
}

// Caller cancels:
agent.cancel_token().cancel();
```

### ❌ Wrong: Thread sleep or busy loop

```rust
std::thread::sleep(Duration::from_secs(1));  // ❌ Blocks thread pool!
```

---

## 7. Timeouts: For Network Operations

### ✅ GOOD: Timeout on executor

```rust
use tokio::time::timeout;

let result = timeout(
    Duration::from_secs(30),
    self.executor.execute_task(&id, prompt)
).await;

match result {
    Ok(Ok(res)) => Ok(res),
    Ok(Err(e)) => Err(e),
    Err(_) => Err(NikaError::Timeout { ... }),
}
```

### ❌ Wrong: No timeout

```rust
// Can hang forever if network fails
self.executor.execute_task(&id, prompt).await?
```

---

## 8. Arc vs Box: For Task Sharing

### ✅ For multiple owners (Arc)

```rust
let workflow = Arc::new(ChatWorkflow::new(...));

// Clone for each task
let wf1 = Arc::clone(&workflow);
let wf2 = Arc::clone(&workflow);

tokio::spawn(async move {
    wf1.next_message_id()
});
```

### ✅ For single owner (Box)

```rust
let task = Box::new(infer_task);
queue.push(task);
```

---

## 9. Mutex Ordering: Prevent Deadlocks

### ✅ GOOD: Consistent lock order

```rust
pub async fn multi_step(&self) -> Result<(), NikaError> {
    // Always lock in same order:
    // 1. workflow
    // 2. store
    // 3. log

    let mut wf = self.workflow.lock().await;
    let mut store = self.store.lock().await;
    let log = self.log.subscribe();

    // Process...

    drop(store);
    drop(wf);
}
```

### ❌ Wrong: Variable lock order

```rust
// Task A: lock workflow, then store
let wf = workflow.lock().await;
let store = store.lock().await;

// Task B: lock store, then workflow
let store = store.lock().await;  // ← Waits for Task A
let wf = workflow.lock().await;  // ← Task A waits for store! DEADLOCK
```

**Rule:** Always acquire locks in the same order.

---

## 10. Error Propagation: With ?

### ✅ GOOD: Early exit

```rust
pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
    let mut wf = self.workflow.lock().await;
    let id = wf.next_message_id();

    // ✅ Propagate error immediately
    let result = self.executor
        .execute_task(&id, prompt)
        .await?;  // Early exit on error

    Ok(result)
}
```

### ❌ Wrong: Swallow error

```rust
let result = self.executor
    .execute_task(&id, prompt)
    .await
    .unwrap_or_default();  // ❌ May hide real error
```

---

## 11. Testing: Concurrency

### ✅ Stress test

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_infers() {
    let agent = Arc::new(ChatAgent::new(...));

    let mut tasks = Vec::new();
    for i in 0..50 {
        let a = Arc::clone(&agent);
        tasks.push(tokio::spawn(async move {
            a.infer(&format!("Message {}", i)).await
        }));
    }

    // All should succeed
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    // Verify state consistency
    let wf = agent.workflow.lock().await;
    assert_eq!(wf.workflow.tasks.len(), 50);
}
```

---

## 12. Debugging: Add Tracing

### ✅ Instrument important functions

```rust
use tracing::instrument;

#[instrument(skip(self, prompt))]
pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
    // tracing automatically logs function entry/exit
    // with task_id and other context
}

// Output:
// infer: entering
// infer: lock acquired
// infer: task created msg-001
// infer: exiting with Ok(...)
```

---

## Quick Decision Tree

```
Need to update state?
├─ Counter/flag? → AtomicU32/AtomicBool
├─ Complex state? → Arc<Mutex<T>>
└─ Temporary? → Local variable

Need to subscribe to updates?
├─ Multiple subscribers? → broadcast::channel
├─ Single consumer? → mpsc::channel
└─ Polling? → Arc<Mutex<Vec<T>>>

Need to run tasks in parallel?
├─ Unknown count? → JoinSet
├─ Known count? → join!
└─ Single? → Just .await

Need to cancel workflow?
├─ External cancel? → CancellationToken
├─ Timeout? → tokio::time::timeout
└─ Conditional? → select!
```

---

## Copy-Paste Boilerplate

### ChatAgent struct

```rust
use tokio::sync::Mutex;
use std::sync::Arc;

pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: Arc<Mutex<ChatWorkflow>>,
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
            executor: TaskExecutor::new("claude", None, None, EventLog::new()),
        }
    }
}
```

### EventLog with broadcast

```rust
use tokio::sync::broadcast;

pub struct EventLog {
    tx: broadcast::Sender<Event>,
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventLog {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            tx,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn emit(&self, kind: EventKind) {
        let event = Event::new(kind);
        let mut e = self.events.blocking_lock();
        e.push(event.clone());
        drop(e);
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
```

### Concurrent task loop

```rust
use tokio::task::JoinSet;

let mut set = JoinSet::new();

for item in items {
    let executor = executor.clone();
    set.spawn(async move {
        executor.process(&item).await
    });
}

while let Some(res) = set.join_next().await {
    match res {
        Ok(Ok(result)) => { /* process */ }
        Ok(Err(e)) => { /* handle task error */ }
        Err(e) => { /* handle join error */ }
    }
}
```

---

## No More Tokio Mistakes! 🎯

