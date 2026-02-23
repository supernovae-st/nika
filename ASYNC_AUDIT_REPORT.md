# Nika TUI Async/Tokio Audit Report

**Date:** 2026-02-23
**Scope:** src/tui/app.rs, src/tui/chat_agent.rs, src/tui/state.rs, src/tui/command.rs
**Status:** READ-ONLY AUDIT (No modifications made)

---

## Executive Summary

The Nika TUI demonstrates **strong async practices** with proper timeout protection, bounded channels, graceful shutdown, and no blocking operations in critical paths. No **CRITICAL** issues found. Two **MEDIUM** severity concerns identified for future optimization.

**Overall Risk Level:** LOW

---

## Findings by Severity

### CRITICAL Issues
None found.

### HIGH Issues
None found.

### MEDIUM Issues

#### Issue 1: Unbounded Event Collection in poll_runtime_events

**Location:** src/tui/app.rs:607-630
**Severity:** MEDIUM
**Category:** Channel Issues / Memory Management

**Code:**
```rust
// Line 607-630
let mut events: Vec<crate::event::Event> = Vec::new();

// Check broadcast receiver (v0.4.1 preferred)
if let Some(ref mut rx) = self.broadcast_rx {
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                tracing::warn!("TUI lagged behind by {} events", n);
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                self.workflow_done = true;
                break;
            }
        }
    }
}
```

**Issue Description:**

The `events` Vec is unbounded and can accumulate a large number of events in a single frame if the event producer is significantly faster than the consumer. In high-volume scenarios (e.g., fast streaming responses or large MCP payloads), this can cause:
- Memory pressure and potential OOM on long-running sessions
- GC-like pauses when the Vec reallocates
- Delayed rendering of other frames

**Current Safeguards:**
- `try_recv()` prevents blocking (non-blocking check only)
- Broadcast receiver drops old events after lag (prevents unbounded buffer)
- Frame rate is 60 FPS (16ms target), limiting accumulation window

**When This Matters:**
- Workflows with 1000+ events per second throughput
- Long-running TUI sessions (>1 hour)
- Memory-constrained environments

**Suggested Fix:**

```rust
let mut events: Vec<crate::event::Event> = Vec::with_capacity(128);

// Hard limit to prevent pathological cases
const MAX_EVENTS_PER_FRAME: usize = 256;

if let Some(ref mut rx) = self.broadcast_rx {
    loop {
        if events.len() >= MAX_EVENTS_PER_FRAME {
            tracing::warn!("Event batch limit reached, {} events buffered", events.len());
            break;
        }
        match rx.try_recv() {
            Ok(event) => events.push(event),
            // ...
        }
    }
}
```

---

#### Issue 2: Fire-and-Forget Task Spawning Without AbortHandle Cleanup in Some Paths

**Location:** src/tui/app.rs:2737-2744, 2750-2758
**Severity:** MEDIUM
**Category:** Task Management / Resource Cleanup

**Code:**
```rust
// Lines 2737-2744: spawn_tracked() correctly tracks tasks
fn spawn_tracked<F>(&self, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let handle = tokio::spawn(future);
    self.background_handles.lock().push(handle.abort_handle());
}

// Lines 2750-2758: cancel_background_tasks() aborts all
fn cancel_background_tasks(&self) {
    let handles = self.background_handles.lock();
    let count = handles.len();
    for handle in handles.iter() {
        handle.abort();
    }
}
```

**Issue Description:**

While `spawn_tracked()` is **correctly implemented** for most spawns (infer, exec, fetch, agent, workflow handlers), there's **potential for developers to accidentally use raw `tokio::spawn()` instead** in future code. The pattern is not enforced by the type system.

Specific concerns:
1. **No compile-time enforcement** - Developers must remember to use `spawn_tracked()`
2. **Background handles stored in parking_lot::Mutex** - Good for lock-free safety, but the collection is accessed both in async contexts (spawn_tracked via self) and sync contexts (cleanup)
3. **Drop impl cleanup (line 2789-2800)** is best-effort only - may not complete in time if async runtime is already shutting down

**Current State (GOOD):**
- All major async spawns (1135, 1673, 1915, 1970, 2014) use `spawn_tracked()`
- MCP and agent tasks properly tracked
- Workflow execution wrapped with timeout (2716)
- All spawned tasks have timeout protection

**Risk:**
- Low in practice due to disciplined codebase
- Higher if code grows without centralized spawn wrapper

**Suggested Enhancement:**

```rust
// Consider a newtype wrapper to enforce spawn tracking
pub struct TrackedSpawner {
    handles: Arc<Mutex<Vec<AbortHandle>>>,
}

impl TrackedSpawner {
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = tokio::spawn(future);
        self.handles.lock().push(handle.abort_handle());
        handle
    }
}

// Move to app.rs as field: spawner: TrackedSpawner
// This prevents accidental raw tokio::spawn() at compile time
```

---

### LOW Issues / Best Practices

#### 1. Bounded Channels - EXCELLENT

**Location:** src/tui/app.rs:279-281, 329-331
**Status:** ✅ GOOD

**Code:**
```rust
let (llm_response_tx, llm_response_rx) = mpsc::channel(32);
let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(512);  // P1 Fix: buffer 256→512
```

**Analysis:**
- `llm_response_rx`: 32-message buffer for complete LLM responses (good)
- `stream_chunk_rx`: 512-token buffer for streaming (allows fast providers like Groq ~200 tok/s)
- Buffers are sized to prevent backpressure while avoiding unbounded growth
- Comment explains the Groq optimization

**Note:** The increase from 256→512 in v0.7.2 is correct for handling high-speed token streams from Groq/DeepSeek.

---

#### 2. Timeout Protection - EXCELLENT

**Location:** src/tui/app.rs:34, 1138, 1678, 1916, 1974, 2014, 2716
**Status:** ✅ GOOD

**Constants (util/constants.rs):**
```rust
const INFER_TIMEOUT: Duration = Duration::from_secs(300);    // 5 minutes
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);     // 2 minutes
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);     // 1 minute
const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour
```

**Coverage:**
- ✅ LLM inference: `timeout(INFER_TIMEOUT, agent.infer())` (lines 1138, 1916)
- ✅ Shell exec: `timeout(EXEC_TIMEOUT, agent.exec_command())` (line 1974)
- ✅ HTTP fetch: `timeout(FETCH_TIMEOUT, agent.fetch())` (line 2014)
- ✅ Workflow run: `timeout(WORKFLOW_TIMEOUT, runner.run())` (line 2716)

**Timeout Handling:**
```rust
match timeout(INFER_TIMEOUT, agent.infer(&prompt)).await {
    Ok(Ok(response)) => { /* success */ }
    Ok(Err(e)) => { /* API error */ }
    Err(_) => {
        // Timeout: graceful error message
        let _ = tx.send(format!(
            "Error: LLM inference timed out after {}s",
            INFER_TIMEOUT.as_secs()
        )).await;
    }
}
```

**Assessment:** This is production-grade. Timeouts prevent the TUI from freezing indefinitely.

---

#### 3. Event Loop Polling - EXCELLENT

**Location:** src/tui/app.rs:554, 604-775
**Status:** ✅ GOOD

**Pattern:**
```rust
// Main event loop (line 554)
self.poll_runtime_events();

// Non-blocking poll implementation (line 612-623)
loop {
    match rx.try_recv() {  // Try-receive only, never blocks
        Ok(event) => events.push(event),
        Err(broadcast::error::TryRecvError::Empty) => break,
        Err(broadcast::error::TryRecvError::Lagged(n)) => {
            tracing::warn!("TUI lagged behind by {} events", n);
        }
        Err(broadcast::error::TryRecvError::Closed) => {
            self.workflow_done = true;
            break;
        }
    }
}
```

**Strengths:**
- ✅ Non-blocking `try_recv()` prevents runtime starvation
- ✅ Dual-receiver support (broadcast preferred, mpsc fallback)
- ✅ Lag detection with warning
- ✅ Graceful closed-channel handling
- ✅ Collected events processed after poll completes (no borrow checker issues)

**Broadcast over MPSC:**
The v0.4.1 migration to broadcast is correct because:
- Broadcast auto-drops old events (prevents unbounded buffer growth)
- Multiple subscribers possible (future extensibility)
- MPSC still available as fallback for legacy systems

---

#### 4. No Blocking Operations in Async Context

**Location:** src/tui/app.rs, src/tui/chat_agent.rs
**Status:** ✅ GOOD

**Verified:**
- ✅ File I/O uses async where needed: `std::fs::read_to_string()` (line 497) - **sync**, called outside async spawns
- ✅ Shell execution: `tokio::process::Command` (chat_agent.rs:554) - **async**
- ✅ HTTP requests: `reqwest::Client::send()` (chat_agent.rs:602) - **async**
- ✅ No `.lock()` calls holding across `.await` points
- ✅ `parking_lot::Mutex` used (never poisons) for short critical sections

**File I/O Note:**
```rust
// Line 497-501: Safe because called in init, not in event loop
let yaml_content = match std::fs::read_to_string(&self.workflow_path) {
    Ok(content) => content,
    Err(e) => {
        tracing::warn!("Failed to read workflow for MCP init: {}", e);
        return;
    }
};
```

This is called during initialization (`init_mcp_clients()`), not in the 60 FPS frame loop, so sync I/O is acceptable.

---

#### 5. Graceful Shutdown & Cleanup - EXCELLENT

**Location:** src/tui/app.rs:2746-2801
**Status:** ✅ GOOD

**Cleanup Hierarchy:**
```rust
// Line 580-587: Unified exit path
if self.should_quit {
    self.cancel_background_tasks();  // Abort all spawned tasks
    break;  // Exit event loop
}
```

**Graceful termination:**
1. User presses Ctrl+C or 'q' → `should_quit = true`
2. Event loop detects and calls `cancel_background_tasks()`
3. All `AbortHandle`s called → tasks receive cancellation
4. `cleanup()` called → terminal state restored
5. Drop impl as fallback

**Abort mechanism:**
```rust
// Line 2754-2757: Simple and effective
for handle in handles.iter() {
    handle.abort();
}
```

**Drop impl safety:**
```rust
impl Drop for App {
    fn drop(&mut self) {
        // Best-effort cleanup on panic or unexpected drop
        if let Some(ref mut terminal) = self.terminal {
            let _ = disable_raw_mode();
            let _ = execute!(...);
        }
    }
}
```

---

## Race Conditions Analysis

### Shared State Synchronization

**Protected Fields:**
1. `background_handles: Arc<Mutex<Vec<AbortHandle>>>` - parking_lot, no poisoning ✅
2. `mcp_client_cache: Arc<DashMap<...>>` - lock-free DashMap ✅
3. `state: TuiState` - Single-threaded, only accessed from main TUI thread ✅

**Potential Race (Low Risk):**

The `background_handles` is accessed from:
- **Spawning path:** `spawn_tracked()` → lock and push (line 2743)
- **Cleanup path:** `cancel_background_tasks()` → lock and iterate (line 2752)
- **Drop path:** Best-effort without locking (line 2791)

If the TUI is in cleanup() and a spawned task tries to... (not possible - spawned tasks are pure futures, no self reference).

**Verdict:** No race conditions. The `parking_lot::Mutex` correctly serializes access.

---

## Channel Analysis

### mpsc Channels

| Channel | Buffer | Sender | Receiver | Risk |
|---------|--------|--------|----------|------|
| `llm_response_tx/rx` | 32 | spawn_tracked tasks | poll_runtime_events | Low |
| `stream_chunk_tx/rx` | 512 | spawn_tracked tasks | poll_runtime_events | Low |
| `event_rx` (fallback) | ? | EventLog broadcast | poll_runtime_events | Low |
| `broadcast_rx` | (auto-drop) | EventLog broadcast | poll_runtime_events | Low |

**Issues:**
- None. Buffers are bounded, non-blocking polls used.

**Receiver Drops:**
If a receiver is dropped while sender holds clones:
- `llm_response_rx`: If dropped, subsequent `tx.send()` will fail with `RecvError` - properly handled with `let _ = tx.send()` (lines 1142, 1145)
- `stream_chunk_rx`: Same pattern, safe

---

## Streaming Responses (chat_agent.rs)

**Location:** src/tui/chat_agent.rs:468-530
**Status:** ✅ GOOD

**Pattern:**
```rust
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    // ...
    if let Some(tx) = self.stream_chunk_tx.clone() {
        let result = self.provider.infer_stream(prompt, tx, self.model_override.as_deref()).await?;
        self.total_input_tokens += result.input_tokens;
        self.total_output_tokens += result.output_tokens;
        result.text
    } else {
        self.provider.infer(prompt, None).await?
    }
}
```

**Strengths:**
- ✅ Channel cloned before use (line 483, 485)
- ✅ Tokens tracked after stream completes
- ✅ Fallback to blocking mode if channel unavailable
- ✅ Metrics sent via separate clone (line 500)

**Metrics Send:**
```rust
let _ = metrics_tx.send(StreamChunk::Metrics {
    input_tokens: result.input_tokens,
    output_tokens: result.output_tokens,
}).await;
```

Ignoring `Err` is correct (receiver may have disconnected). Good use of `let _`.

---

## ChatAgent Construction

**Location:** src/tui/chat_agent.rs:221-248
**Status:** ✅ GOOD

**Streaming Channel Setup:**
```rust
pub fn with_stream_chunks(mut self, tx: mpsc::Sender<StreamChunk>) -> Self {
    self.stream_chunk_tx = Some(tx);
    self
}
```

Builder pattern correctly allows optional streaming. If channel is dropped externally, `infer()` will handle the send error gracefully.

---

## Specific Code Review

### Async Spawn Pattern (GOOD)

**Example 1: LLM Infer (Line 1135-1161)**
```rust
self.spawn_tracked(async move {
    match crate::tui::ChatAgent::new() {
        Ok(mut agent) => {
            match timeout(INFER_TIMEOUT, agent.infer(&prompt_with_context)).await {
                Ok(Ok(response)) => {
                    let _ = tx.send(response).await;
                }
                Ok(Err(e)) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
                Err(_) => {
                    let _ = tx.send(format!("Error: ... timeout ...")).await;
                }
            }
        }
        Err(e) => {
            let _ = tx.send(format!("Error: {}", e)).await;
        }
    }
});
```

**Assessment:**
- ✅ Tracked via `spawn_tracked()` for cleanup
- ✅ Timeout wraps the async operation
- ✅ All errors handled with graceful send
- ✅ tx cloned before move (thread-safe)
- ✅ No .await outside timeout - tight scope

**Example 2: Workflow Execution (Line 2727-2727)**
```rust
self.spawn_tracked(async move {
    // ... setup ...
    match timeout(WORKFLOW_TIMEOUT, runner.run()).await {
        Ok(Ok(output)) => { tracing::info!("Workflow completed: {} chars", output.len()); }
        Ok(Err(e)) => { tracing::error!("Workflow execution failed: {}", e); }
        Err(_) => { tracing::error!("Workflow timed out after {}s", WORKFLOW_TIMEOUT.as_secs()); }
    }
});
```

**Assessment:**
- ✅ Tracked
- ✅ Timeout protection
- ✅ Error telemetry logged
- ✅ Clean error messages

---

## Testing Recommendations

### 1. High-Volume Event Stress Test
```rust
#[tokio::test]
async fn test_poll_runtime_events_unbounded_accumulation() {
    // Send 10k events per frame for 100 frames
    // Verify: Vec doesn't cause OOM, memory stays bounded
}
```

### 2. Broadcast Lagging
```rust
#[tokio::test]
async fn test_broadcast_lag_warning() {
    // Create broadcast with size=10
    // Send 100 events rapidly
    // Verify: lag warning logged, TUI recovers
}
```

### 3. Spawn Tracking Cleanup
```rust
#[tokio::test]
async fn test_cancel_background_tasks_aborts_all() {
    // Spawn 10 tasks with long sleeps
    // Call cancel_background_tasks()
    // Verify: all tasks aborted within timeout
}
```

### 4. Channel Dropout
```rust
#[tokio::test]
async fn test_stream_chunk_rx_dropout() {
    // Drop stream_chunk_rx
    // Spawn task that tries to send chunks
    // Verify: graceful send failure handling
}
```

---

## Summary Table

| Category | Status | Notes |
|----------|--------|-------|
| **Event Loop** | ✅ GOOD | Non-blocking polls, no deadlocks |
| **Timeouts** | ✅ EXCELLENT | All async ops wrapped |
| **Channels** | ✅ GOOD | Bounded, non-blocking |
| **Task Tracking** | ✅ GOOD | spawn_tracked() covers major paths |
| **Graceful Shutdown** | ✅ GOOD | AbortHandle cleanup implemented |
| **Locking** | ✅ GOOD | parking_lot prevents poisoning |
| **Race Conditions** | ✅ NONE FOUND | Shared state protected |
| **Blocking in Async** | ✅ NONE | File I/O outside event loop |
| **Unbounded Growth** | ⚠️ MEDIUM | Event Vec could spike (low risk) |
| **Spawn Enforcement** | ⚠️ MEDIUM | No compile-time guarantee |

---

## Recommendations (Priority Order)

### P0 (Do Now - Production Safety)
None. The codebase is production-grade.

### P1 (Soon - Robustness)
1. Add `MAX_EVENTS_PER_FRAME` limit in `poll_runtime_events()` to cap unbounded growth
2. Monitor long-running sessions (>1 hour) for memory creep

### P2 (Nice to Have - Enforcement)
1. Consider `TrackedSpawner` newtype to prevent accidental `tokio::spawn()` in future code
2. Add stress tests for high-volume event scenarios

### P3 (Documentation)
1. Document the MCP client lazy-init pattern in CLAUDE.md
2. Add timeout rationale comments explaining 5min/2min/1min/1hr values

---

## Conclusion

The Nika TUI demonstrates **professional async/Tokio practices**:

✅ **No CRITICAL or HIGH severity async issues**
✅ **Timeout protection on all blocking operations**
✅ **Bounded channels with proper error handling**
✅ **Graceful task cancellation and cleanup**
✅ **Non-blocking event loop at 60 FPS**
✅ **No blocking operations in async context**
✅ **Proper shared state synchronization**

The two MEDIUM items are **preventive enhancements** for edge cases, not bugs. The code is safe to deploy and extend.

---

**Report End**
