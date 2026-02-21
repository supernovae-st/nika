# spawn_background Wiring Implementation Plan

**Status:** Ready for implementation
**Date:** 2026-02-21
**Risk Level:** Medium → Low (with Arc<Mutex> pattern)

## Problem Statement

The TUI has 8 `tokio::spawn()` call sites that run background tasks (LLM inference, shell execution, HTTP fetch, MCP invocations, agent loops, workflow execution). These tasks are **orphaned** - if the TUI exits, they continue running without proper cleanup.

### Current State

```rust
// Infrastructure exists but is unused (#[allow(dead_code)])
background_tasks: JoinSet<()>,

fn spawn_background<F>(&mut self, future: F) -> bool  // Requires &mut self
```

### The Ownership Problem

`spawn_background` requires `&mut self`, but at most spawn sites we've already borrowed parts of `self`:

```rust
let tx = self.llm_response_tx.clone();  // ← borrow of self
tokio::spawn(async move { ... });       // ← can't call spawn_background(&mut self) here
```

## Solution: Arc<Mutex<JoinSet>> Pattern

### Design

```rust
use tokio::sync::Mutex as TokioMutex;

// In TuiApp struct:
background_tasks: Arc<TokioMutex<JoinSet<()>>>,

// New helper - takes Arc clone, no &mut self needed
fn spawn_tracked(&self, future: impl Future<Output = ()> + Send + 'static) {
    let tasks = Arc::clone(&self.background_tasks);
    tokio::spawn(async move {
        let mut guard = tasks.lock().await;
        guard.spawn(future);
    });
}
```

**Why this works:**
- Arc allows shared ownership across spawn sites
- TokioMutex provides async-aware interior mutability
- No need for `&mut self` - can call from anywhere

### Alternative Considered: Channel-based spawner

```rust
// Rejected - adds indirection and a spawner task
let (spawn_tx, mut spawn_rx) = mpsc::channel::<BoxFuture<'static, ()>>(100);
tokio::spawn(async move {
    while let Some(fut) = spawn_rx.recv().await {
        join_set.spawn(fut);
    }
});
```

**Rejected because:** More complex, adds latency, harder to test.

## Implementation Steps

### Step 1: Update struct definition

**File:** `src/tui/app.rs` (line ~243)

```rust
// Before
background_tasks: JoinSet<()>,

// After
background_tasks: Arc<TokioMutex<JoinSet<()>>>,
```

### Step 2: Update constructors

**File:** `src/tui/app.rs` (lines ~300 and ~349)

```rust
// Before
background_tasks: JoinSet::new(),

// After
background_tasks: Arc::new(TokioMutex::new(JoinSet::new())),
```

### Step 3: Rewrite spawn_background

**File:** `src/tui/app.rs` (line ~2682)

```rust
// Before (dead code)
#[allow(dead_code)]
fn spawn_background<F>(&mut self, future: F) -> bool

// After (active, no mut)
fn spawn_tracked<F>(&self, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let tasks = Arc::clone(&self.background_tasks);
    // Spawn immediately - the future runs inside JoinSet tracking
    let tracked_future = async move {
        future.await;
    };

    // We need to spawn inside the JoinSet, not just tokio::spawn
    // Solution: spawn a task that adds to JoinSet
    let tasks_clone = tasks;
    tokio::spawn(async move {
        let mut guard = tasks_clone.lock().await;
        guard.spawn(tracked_future);
    });
}
```

**Wait - this has a race condition.** The inner future runs in a separate tokio::spawn, not inside the JoinSet. Let me redesign:

```rust
fn spawn_tracked<F>(&self, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let tasks = Arc::clone(&self.background_tasks);
    tokio::spawn(async move {
        // Lock, spawn into JoinSet, release lock immediately
        tasks.lock().await.spawn(future);
    });
}
```

**Still problematic** - the spawn happens inside a tokio::spawn, so it's not tracked properly.

### Revised Design: Direct spawn with handle

The issue is that `JoinSet::spawn()` returns an `AbortHandle` and the future runs inside the JoinSet's task management. We need to spawn INSIDE the JoinSet, not wrap it.

**Better approach - synchronous spawn with blocking lock:**

```rust
fn spawn_tracked<F>(&self, future: F) -> AbortHandle
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    // Use try_lock to avoid blocking, fall back to raw tokio::spawn
    match self.background_tasks.try_lock() {
        Ok(mut guard) => guard.spawn(future),
        Err(_) => {
            // Fallback: spawn untracked if lock contention
            // This is rare - only happens with concurrent spawns
            tracing::warn!("JoinSet lock contention, spawning untracked");
            let handle = tokio::spawn(future);
            handle.abort_handle()
        }
    }
}
```

**Hmm, try_lock is for std::sync::Mutex, not tokio::sync::Mutex.**

### Final Design: std::sync::Mutex (not tokio::sync::Mutex)

Since `JoinSet::spawn()` is synchronous and fast, we can use a regular `std::sync::Mutex`:

```rust
use std::sync::Mutex;

// In struct:
background_tasks: Arc<Mutex<JoinSet<()>>>,

// Helper method:
fn spawn_tracked<F>(&self, future: F) -> AbortHandle
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    self.background_tasks
        .lock()
        .expect("background_tasks mutex poisoned")
        .spawn(future)
}
```

**Why std::sync::Mutex works:**
- `JoinSet::spawn()` is non-blocking (just registers the future)
- Lock hold time is microseconds
- No async operations inside the critical section

### Step 4: Update cancel_background_tasks

```rust
async fn cancel_background_tasks(&mut self) {
    // Lock and abort all
    {
        let mut guard = self.background_tasks.lock().expect("mutex poisoned");
        guard.abort_all();
    }

    // Wait for all tasks to complete (they'll be aborted)
    loop {
        let maybe_result = {
            let mut guard = self.background_tasks.lock().expect("mutex poisoned");
            // join_next is async but returns immediately if nothing to join
            // We need to poll it - this is tricky with std::sync::Mutex
        };
        // ...
    }
}
```

**Problem:** `JoinSet::join_next()` is async, can't hold std::sync::Mutex across await.

### FINAL Design: Simpler approach with deferred collection

Since the main goal is cleanup on exit, we can:
1. Keep track of `AbortHandle`s separately
2. Abort all handles on cleanup
3. Don't wait for completion (they're cancelled anyway)

```rust
// In struct:
background_handles: Arc<Mutex<Vec<AbortHandle>>>,

fn spawn_tracked<F>(&self, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let handle = tokio::spawn(future);
    self.background_handles
        .lock()
        .expect("mutex poisoned")
        .push(handle.abort_handle());
}

fn cancel_background_tasks(&self) {
    let handles = self.background_handles
        .lock()
        .expect("mutex poisoned");
    for handle in handles.iter() {
        handle.abort();
    }
    tracing::debug!("Cancelled {} background tasks", handles.len());
}
```

**This is much simpler and achieves the goal: tracked cleanup.**

## Final Implementation Plan

### Files to Modify

1. **`src/tui/app.rs`**
   - Add import: `use tokio::task::AbortHandle;`
   - Change field: `background_tasks: JoinSet<()>` → `background_handles: Arc<Mutex<Vec<AbortHandle>>>`
   - Update constructors (2 places)
   - Rewrite `spawn_tracked()` (was `spawn_background`)
   - Simplify `cancel_background_tasks()`
   - Wire 8 spawn sites

### Spawn Sites to Wire

| Line | Context | Current | After |
|------|---------|---------|-------|
| 1154 | chat infer | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 1666 | overlay infer | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 1903 | streaming infer | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 1963 | exec_command | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 2002 | fetch | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 2095 | invoke (MCP) | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 2276 | agent loop | `tokio::spawn(...)` | `self.spawn_tracked(...)` |
| 2605 | workflow run | `tokio::spawn(...)` | `self.spawn_tracked(...)` |

### Tests to Add

```rust
#[cfg(test)]
mod background_task_tests {
    #[test]
    fn test_spawn_tracked_adds_handle();

    #[test]
    fn test_cancel_aborts_all_handles();

    #[tokio::test]
    async fn test_spawned_task_actually_runs();

    #[tokio::test]
    async fn test_abort_stops_running_task();
}
```

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Mutex poisoning | Use `.expect()` with clear error - panic is appropriate |
| Lock contention | Lock hold time is <1μs, contention highly unlikely |
| Memory growth | Clear handles on cleanup, but could grow in long sessions |
| Abort semantics | Tasks may not see abort if they don't await - acceptable |

## Rollback Plan

If issues arise:
1. Revert to `tokio::spawn()` without tracking
2. Accept orphaned tasks as trade-off
3. Document as known limitation

---

## Execution Checklist

- [ ] Update imports in app.rs
- [ ] Change struct field type
- [ ] Update constructors (2 places)
- [ ] Rewrite spawn_tracked method
- [ ] Simplify cancel_background_tasks
- [ ] Wire spawn site 1154 (chat infer)
- [ ] Wire spawn site 1666 (overlay infer)
- [ ] Wire spawn site 1903 (streaming infer)
- [ ] Wire spawn site 1963 (exec_command)
- [ ] Wire spawn site 2002 (fetch)
- [ ] Wire spawn site 2095 (invoke MCP)
- [ ] Wire spawn site 2276 (agent loop)
- [ ] Wire spawn site 2605 (workflow run)
- [ ] Add unit tests
- [ ] Run full test suite
- [ ] Commit with conventional message
