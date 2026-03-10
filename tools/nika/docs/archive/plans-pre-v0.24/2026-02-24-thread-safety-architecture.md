# Thread-Safety Architecture for Nika v0.9.1 Chat-as-DAG

**Date:** 2026-02-24
**Status:** Reference Document
**Context:** v0.9.1 Chat-as-DAG feature requires thread-safe shared state between TUI event loop and background async tasks

---

## Executive Summary

This document provides a comprehensive guide to thread-safety patterns in Nika, specifically for the Chat-as-DAG feature in v0.9.1. The feature converts chat conversations into DAG workflows that execute in background tasks while the TUI remains responsive.

**Key Challenges:**
1. Background workflow execution must update TUI state without blocking
2. Multiple concurrent workflows may execute simultaneously
3. Event streaming must handle backpressure from fast providers
4. ID generation for messages/tasks must be lock-free

---

## 1. Current Concurrency Patterns in Codebase

### 1.1 EventLog (src/event/log.rs) - EXEMPLARY PATTERN

The EventLog demonstrates the best practices currently used in Nika:

```rust
// EventLog: Thread-safe, append-only log
#[derive(Clone)]
pub struct EventLog {
    events: Arc<RwLock<Vec<Event>>>,           // parking_lot::RwLock
    start_time: Instant,
    next_id: Arc<AtomicU64>,                   // Lock-free ID generation
    broadcast_tx: Option<broadcast::Sender<Event>>,  // Async event streaming
}
```

**Why This Works:**
1. `parking_lot::RwLock` is 2-3x faster than std and doesn't poison
2. `AtomicU64` provides lock-free monotonic ID generation
3. `broadcast::Sender` enables multiple TUI subscribers without locks
4. `Clone` is cheap (Arc references only)

### 1.2 DataStore (src/store/datastore.rs) - LOCK-FREE PATTERN

```rust
// DataStore: Lock-free concurrent HashMap
#[derive(Clone, Default)]
pub struct DataStore {
    results: Arc<DashMap<Arc<str>, TaskResult>>,
}
```

**Why This Works:**
1. `DashMap` provides lock-free concurrent access for common operations
2. `Arc<str>` keys enable zero-cost cloning (string interning)
3. `Arc<Value>` outputs avoid deep cloning large JSON structures

### 1.3 App Background Tasks (src/tui/app.rs)

```rust
pub struct App {
    // Background task handles for cancellation
    background_handles: Arc<Mutex<Vec<AbortHandle>>>,  // parking_lot::Mutex

    // Cached MCP clients (lazy-initialized)
    mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,

    // Verification cache with TTL
    verification_cache: Arc<Mutex<VerificationCache>>,

    // Event channel for TUI updates
    broadcast_rx: Option<broadcast::Receiver<NikaEvent>>,
    stream_chunk_rx: mpsc::Receiver<StreamChunk>,
}
```

### 1.4 String Interner (src/util/interner.rs) - GLOBAL LOCK-FREE

```rust
// Global string interner (thread-safe, lock-free)
static INTERNER: LazyLock<Interner> = LazyLock::new(Interner::new);

pub struct Interner {
    strings: DashMap<Arc<str>, ()>,
}
```

---

## 2. Recommended Patterns for Chat-as-DAG

### 2.1 ChatWorkflow State

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use parking_lot::RwLock;
use tokio::sync::{broadcast, mpsc};
use dashmap::DashMap;

/// Unique workflow ID generator (lock-free)
static NEXT_WORKFLOW_ID: AtomicU32 = AtomicU32::new(1);

/// Chat-as-DAG workflow state
#[derive(Clone)]
pub struct ChatWorkflow {
    /// Unique workflow ID
    pub id: u32,

    /// Tasks by ID (lock-free concurrent access)
    tasks: Arc<DashMap<String, ChatTask>>,

    /// Execution state (single writer, multiple readers)
    state: Arc<RwLock<WorkflowState>>,

    /// Lock-free message ID counter
    next_message_id: Arc<AtomicU64>,

    /// Event broadcast for TUI updates
    event_tx: broadcast::Sender<WorkflowEvent>,

    /// Cancellation token
    cancel_token: CancellationToken,
}

impl ChatWorkflow {
    pub fn new() -> (Self, broadcast::Receiver<WorkflowEvent>) {
        // P1 Fix: Increased from 256 to 512 for fast providers
        let (event_tx, event_rx) = broadcast::channel(512);

        let workflow = Self {
            id: NEXT_WORKFLOW_ID.fetch_add(1, Ordering::Relaxed),
            tasks: Arc::new(DashMap::new()),
            state: Arc::new(RwLock::new(WorkflowState::Pending)),
            next_message_id: Arc::new(AtomicU64::new(1)),
            event_tx,
            cancel_token: CancellationToken::new(),
        };

        (workflow, event_rx)
    }

    /// Generate unique message ID (lock-free)
    pub fn next_message_id(&self) -> u64 {
        self.next_message_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Update state (write lock, keep short!)
    pub fn set_state(&self, new_state: WorkflowState) {
        *self.state.write() = new_state;
        let _ = self.event_tx.send(WorkflowEvent::StateChanged(new_state));
    }

    /// Read state (read lock, can have multiple concurrent readers)
    pub fn state(&self) -> WorkflowState {
        self.state.read().clone()
    }
}
```

### 2.2 Lock-Free ID Generation Pattern

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// AtomicU32 for IDs (4B max = enough for any session)
/// Uses Relaxed ordering - no synchronization needed for unique IDs
static NEXT_TASK_ID: AtomicU32 = AtomicU32::new(1);

pub fn generate_task_id() -> u32 {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

/// AtomicU64 for message IDs (supports 18 quintillion unique IDs)
/// Per-workflow counter for stable message tracking
pub struct MessageIdGenerator {
    counter: AtomicU64,
}

impl MessageIdGenerator {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }

    /// Thread-safe ID generation without locks
    #[inline]
    pub fn next(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }
}
```

### 2.3 Bounded Event Queue Pattern

```rust
use tokio::sync::{broadcast, mpsc};

/// Event queue configuration
pub struct EventQueueConfig {
    /// Capacity for bounded channels (prevents memory exhaustion)
    pub capacity: usize,
    /// Lag policy for broadcast (how to handle slow receivers)
    pub lag_policy: LagPolicy,
}

pub enum LagPolicy {
    /// Drop oldest events when queue is full (default for TUI)
    DropOldest,
    /// Return error when queue is full (for critical events)
    Error,
}

impl Default for EventQueueConfig {
    fn default() -> Self {
        Self {
            // P1 Fix: 512 handles fast providers like Groq (~200 tok/s)
            capacity: 512,
            lag_policy: LagPolicy::DropOldest,
        }
    }
}

/// Create bounded broadcast channel for workflow events
pub fn create_event_channel() -> (broadcast::Sender<WorkflowEvent>, broadcast::Receiver<WorkflowEvent>) {
    let config = EventQueueConfig::default();
    broadcast::channel(config.capacity)
}

/// Create bounded mpsc channel for streaming chunks
pub fn create_stream_channel() -> (mpsc::Sender<StreamChunk>, mpsc::Receiver<StreamChunk>) {
    let config = EventQueueConfig::default();
    mpsc::channel(config.capacity)
}
```

---

## 3. Anti-Patterns to Avoid

### 3.1 NEVER: Hold Locks Across .await

```rust
// BAD: Lock held across await point
async fn bad_pattern(state: &Arc<Mutex<State>>) {
    let guard = state.lock();
    some_async_call().await;  // DANGER: Other tasks blocked!
    drop(guard);
}

// GOOD: Drop lock before await
async fn good_pattern(state: &Arc<Mutex<State>>) {
    let data = {
        let guard = state.lock();
        guard.data.clone()  // Clone what you need
    };  // Lock dropped here

    some_async_call_with(data).await;
}
```

### 3.2 NEVER: Use std::sync::Mutex (Use parking_lot Instead)

```rust
// BAD: std::sync::Mutex can poison and is slower
use std::sync::Mutex;
let state = Arc::new(Mutex::new(data));
let guard = state.lock().unwrap();  // Can panic on poison!

// GOOD: parking_lot::Mutex is faster and never poisons
use parking_lot::Mutex;
let state = Arc::new(Mutex::new(data));
let guard = state.lock();  // No unwrap needed!
```

### 3.3 NEVER: Unbounded Channels for Fast Producers

```rust
// BAD: Unbounded can cause memory exhaustion
let (tx, rx) = mpsc::unbounded_channel();

// GOOD: Bounded with backpressure
let (tx, rx) = mpsc::channel(512);

// Handle send result
match tx.send(event).await {
    Ok(()) => {},
    Err(_) => {
        // Channel full or closed - decide how to handle
        tracing::warn!("Event queue full, dropping event");
    }
}
```

### 3.4 NEVER: Blocking in Async Context

```rust
// BAD: Blocks the tokio runtime
async fn bad_file_read() {
    let content = std::fs::read_to_string("file.txt").unwrap();  // BLOCKING!
}

// GOOD: Use tokio's async file I/O
async fn good_file_read() {
    let content = tokio::fs::read_to_string("file.txt").await.unwrap();
}

// GOOD: For CPU-intensive work, use spawn_blocking
async fn cpu_intensive_work(data: Vec<u8>) {
    let result = tokio::task::spawn_blocking(move || {
        expensive_computation(&data)
    }).await.unwrap();
}
```

### 3.5 NEVER: Clone Arc More Than Once Per Scope

```rust
// BAD: Multiple clones
fn bad_cloning(data: &Arc<Data>) {
    let clone1 = data.clone();
    let clone2 = data.clone();  // Redundant
    spawn(async move { use_data(clone1).await });
    spawn(async move { use_data(clone2).await });
}

// GOOD: Clone once, use Arc::clone for intent clarity
fn good_cloning(data: &Arc<Data>) {
    let data = Arc::clone(data);  // Clear intent: this is an Arc clone
    spawn({
        let d = Arc::clone(&data);
        async move { use_data(d).await }
    });
    spawn({
        let d = Arc::clone(&data);
        async move { use_data(d).await }
    });
}
```

---

## 4. Synchronization Primitive Selection Guide

| Use Case | Recommended | Why |
|----------|-------------|-----|
| ID generation | `AtomicU32/U64` | Lock-free, zero contention |
| Shared read-heavy state | `parking_lot::RwLock` | Multiple readers, fast |
| Short critical sections | `parking_lot::Mutex` | Fast, no poisoning |
| Concurrent HashMap | `DashMap` | Lock-free for common ops |
| Single init | `tokio::sync::OnceCell` | Async-safe lazy init |
| Event streaming | `broadcast::channel` | Multi-consumer |
| Task communication | `mpsc::channel` | Single consumer |
| Global singleton | `std::sync::LazyLock` | Thread-safe lazy static |
| Cancellation | `CancellationToken` | Graceful shutdown |

---

## 5. ChatWorkflow Integration with TUI

### 5.1 State Update Flow

```text
┌─────────────────┐     broadcast::channel     ┌─────────────────┐
│  Background     │──────────────────────────▶│  TUI Event      │
│  Workflow Task  │     WorkflowEvent          │  Loop           │
└─────────────────┘                            └─────────────────┘
        │                                              │
        │ DashMap::insert()                            │ try_recv()
        ▼                                              ▼
┌─────────────────┐                            ┌─────────────────┐
│  ChatWorkflow   │◀───────────────────────────│  ChatView       │
│  .tasks         │     read-only access       │  .render()      │
└─────────────────┘                            └─────────────────┘
```

### 5.2 Implementation Example

```rust
impl App {
    /// Start a chat workflow in background
    async fn start_chat_workflow(&mut self, prompt: String) {
        // Create workflow with event channel
        let (workflow, event_rx) = ChatWorkflow::new();
        let workflow = Arc::new(workflow);

        // Store workflow reference in ChatView
        self.chat_view.set_active_workflow(Arc::clone(&workflow));

        // Spawn background execution
        self.spawn_background({
            let workflow = Arc::clone(&workflow);
            let mcp_clients = Arc::clone(&self.mcp_client_cache);

            async move {
                workflow.set_state(WorkflowState::Running);

                match execute_chat_workflow(&workflow, &mcp_clients).await {
                    Ok(result) => {
                        workflow.set_state(WorkflowState::Completed(result));
                    }
                    Err(e) => {
                        workflow.set_state(WorkflowState::Failed(e.to_string()));
                    }
                }
            }
        });

        // Store receiver for polling
        self.workflow_event_rx = Some(event_rx);
    }

    /// Poll workflow events (called every frame)
    fn poll_workflow_events(&mut self) {
        if let Some(ref mut rx) = self.workflow_event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => self.handle_workflow_event(event),
                    Err(broadcast::error::TryRecvError::Empty) => break,
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("TUI lagged by {} workflow events", n);
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        self.workflow_event_rx = None;
                        break;
                    }
                }
            }
        }
    }
}
```

---

## 6. Testing Thread Safety

### 6.1 Concurrent Access Tests

```rust
#[tokio::test]
async fn test_concurrent_workflow_updates() {
    let (workflow, _rx) = ChatWorkflow::new();
    let workflow = Arc::new(workflow);

    let mut handles = vec![];

    // Spawn 100 concurrent task insertions
    for i in 0..100 {
        let wf = Arc::clone(&workflow);
        handles.push(tokio::spawn(async move {
            wf.tasks.insert(
                format!("task_{}", i),
                ChatTask::new(format!("Task {}", i)),
            );
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(workflow.tasks.len(), 100);
}

#[test]
fn test_lock_free_id_generation() {
    use std::thread;

    let ids: Vec<_> = (0..10)
        .map(|_| {
            thread::spawn(|| {
                (0..100).map(|_| generate_task_id()).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();

    // All 1000 IDs should be unique
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 1000);
}
```

### 6.2 Backpressure Tests

```rust
#[tokio::test]
async fn test_broadcast_backpressure() {
    let (tx, mut rx) = broadcast::channel::<u32>(10);

    // Send more than capacity
    for i in 0..20 {
        let _ = tx.send(i);  // Won't block
    }

    // First recv should report lag
    match rx.try_recv() {
        Err(broadcast::error::TryRecvError::Lagged(n)) => {
            assert_eq!(n, 10);  // Lagged by 10 messages
        }
        _ => panic!("Expected Lagged error"),
    }

    // Subsequent recv gets remaining messages
    let msg = rx.try_recv().unwrap();
    assert_eq!(msg, 10);  // First message after lag
}
```

---

## 7. Performance Considerations

### 7.1 Channel Buffer Sizing

| Channel Type | Use Case | Recommended Size |
|--------------|----------|------------------|
| `stream_chunk_rx` | Token streaming | 512 (fast providers) |
| `broadcast` events | Workflow events | 512 |
| `mpsc` LLM responses | Complete responses | 32 |
| `broadcast` TUI events | Runtime events | 512 |

### 7.2 Lock Contention Mitigation

1. **Keep critical sections short** - Clone data and release lock
2. **Use read locks when possible** - RwLock allows concurrent readers
3. **Prefer lock-free structures** - DashMap, atomics when applicable
4. **Batch updates** - Collect changes, apply in single lock

### 7.3 Memory Efficiency

1. **Use Arc<str> for task IDs** - String interning via global interner
2. **Use Arc<Value> for large JSON** - Avoid deep cloning
3. **Reuse buffers** - Pre-allocate event_buffer in App
4. **Bounded channels** - Prevent unbounded memory growth

---

## 8. Checklist for New Concurrent Code

- [ ] Use `parking_lot` instead of `std::sync` for Mutex/RwLock
- [ ] Use `AtomicU32/U64` for ID generation (not Mutex<u32>)
- [ ] Use bounded channels (`mpsc::channel(N)`, not unbounded)
- [ ] Never hold locks across `.await` points
- [ ] Use `DashMap` for concurrent HashMap access
- [ ] Use `CancellationToken` for graceful shutdown
- [ ] Handle `Lagged` error from broadcast channels
- [ ] Clone Arc with `Arc::clone(&x)` for clarity
- [ ] Test concurrent access with multiple spawned tasks
- [ ] Document Send + Sync bounds in trait definitions

---

## 9. References

- **EventLog**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/event/log.rs`
- **DataStore**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/store/datastore.rs`
- **App**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/tui/app.rs`
- **ChatAgent**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/tui/chat_agent.rs`
- **Runner**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/runtime/runner.rs`
- **Interner**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/util/interner.rs`
- **PERFORMANCE.md**: `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/.claude/rules/PERFORMANCE.md`

---

## Appendix A: Current Problematic Patterns Found

### A.1 ChatView Message ID Counter (Minor Issue)

**Location:** `src/tui/views/chat.rs:426`

```rust
/// v0.9: Counter for generating unique message IDs
message_id_counter: u64,
```

**Issue:** This is a non-atomic counter in a struct that could be accessed from multiple contexts.

**Recommendation:** If ChatView is only accessed from TUI thread, this is fine. If accessed from background tasks, use `AtomicU64`:

```rust
// RECOMMENDED for multi-threaded access
message_id_counter: Arc<AtomicU64>,
```

### A.2 ChatAgent Not Thread-Safe

**Location:** `src/tui/chat_agent.rs:199`

```rust
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    streaming_state: StreamingState,
    // ...
}
```

**Issue:** ChatAgent is designed for single-threaded use. For Chat-as-DAG, we need a thread-safe version.

**Recommendation:** Create `SharedChatAgent` wrapper:

```rust
pub struct SharedChatAgent {
    inner: Arc<RwLock<ChatAgent>>,
    streaming_tx: mpsc::Sender<StreamChunk>,
}

impl SharedChatAgent {
    pub async fn infer(&self, prompt: &str) -> Result<String, NikaError> {
        let tx = self.streaming_tx.clone();
        let provider = {
            self.inner.read().provider.clone()
        };
        // Now we can use provider without holding lock
        provider.infer_stream(prompt, tx, None).await
    }
}
```

### A.3 App MCP Client Cache Race Condition (Potential)

**Location:** `src/tui/app.rs:251`

```rust
mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
```

**Analysis:** This pattern is actually correct. `OnceCell` ensures single initialization. However, the initialization itself could benefit from timeout protection.

---

## Appendix B: Recommended New Types for v0.9.1

```rust
// src/tui/chat_workflow.rs

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Global workflow ID counter
static NEXT_WORKFLOW_ID: AtomicU32 = AtomicU32::new(1);

/// Chat-as-DAG workflow state
#[derive(Clone)]
pub struct ChatWorkflow {
    pub id: u32,
    tasks: Arc<DashMap<Arc<str>, ChatTask>>,
    state: Arc<RwLock<WorkflowState>>,
    next_task_id: Arc<AtomicU32>,
    event_tx: broadcast::Sender<ChatWorkflowEvent>,
    cancel_token: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowState {
    Pending,
    Running,
    Paused,
    Completed(String),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum ChatWorkflowEvent {
    StateChanged(WorkflowState),
    TaskStarted { task_id: Arc<str>, verb: String },
    TaskCompleted { task_id: Arc<str>, duration_ms: u64 },
    TaskFailed { task_id: Arc<str>, error: String },
    StreamChunk { task_id: Arc<str>, content: String },
}

impl ChatWorkflow {
    pub fn new() -> (Self, broadcast::Receiver<ChatWorkflowEvent>) {
        let (event_tx, event_rx) = broadcast::channel(512);

        let workflow = Self {
            id: NEXT_WORKFLOW_ID.fetch_add(1, Ordering::Relaxed),
            tasks: Arc::new(DashMap::new()),
            state: Arc::new(RwLock::new(WorkflowState::Pending)),
            next_task_id: Arc::new(AtomicU32::new(1)),
            event_tx,
            cancel_token: CancellationToken::new(),
        };

        (workflow, event_rx)
    }

    /// Generate unique task ID (lock-free)
    pub fn generate_task_id(&self) -> Arc<str> {
        let id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        Arc::from(format!("task_{}", id))
    }

    /// Add task (lock-free DashMap insert)
    pub fn add_task(&self, task: ChatTask) -> Arc<str> {
        let id = self.generate_task_id();
        self.tasks.insert(Arc::clone(&id), task);
        id
    }

    /// Get task (lock-free DashMap get)
    pub fn get_task(&self, id: &str) -> Option<ChatTask> {
        self.tasks.get(id).map(|r| r.value().clone())
    }

    /// Update workflow state (write lock, emits event)
    pub fn set_state(&self, new_state: WorkflowState) {
        {
            *self.state.write() = new_state.clone();
        }
        let _ = self.event_tx.send(ChatWorkflowEvent::StateChanged(new_state));
    }

    /// Get current state (read lock)
    pub fn state(&self) -> WorkflowState {
        self.state.read().clone()
    }

    /// Cancel workflow execution
    pub fn cancel(&self) {
        self.cancel_token.cancel();
        self.set_state(WorkflowState::Cancelled);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Emit task event
    pub fn emit(&self, event: ChatWorkflowEvent) {
        let _ = self.event_tx.send(event);
    }
}
```
