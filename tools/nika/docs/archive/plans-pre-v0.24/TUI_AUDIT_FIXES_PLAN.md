# TUI Audit Fixes - Implementation Plan

**Date**: 2026-02-21
**Version**: v0.7.1
**Status**: Ready for Implementation

## Overview

This plan addresses 3 HIGH priority issues from the 10-agent TUI audit:

| Priority | Issue | Estimated Changes |
|----------|-------|-------------------|
| HIGH | Add timeouts to spawned async ops | 8 spawn sites + 2 method changes |
| HIGH | Wire spawn_background() | 8 call sites in app.rs |
| HIGH | Cache build_timeline_entries() | TuiState + ProgressPanel |

---

## Task 1: Add Timeouts to Spawned Async Operations

### Problem

TUI spawns async tasks without timeout protection. If LLM API or filesystem hangs, the TUI freezes.

### Affected Operations

| Operation | File:Line | Risk | Timeout Constant |
|-----------|-----------|------|------------------|
| LLM inference | `app.rs:1134` | CRITICAL | `INFER_TIMEOUT` (120s) |
| LLM streaming | `app.rs:1863` | CRITICAL | `INFER_TIMEOUT` (120s) |
| OpenAI fallback | `app.rs:1634` | HIGH | `INFER_TIMEOUT` (120s) |
| Shell exec | `app.rs:1913` | MEDIUM | `EXEC_TIMEOUT` (60s) |
| HTTP fetch | `app.rs:1944` | MEDIUM | `FETCH_TIMEOUT` (30s) |
| MCP invoke | `app.rs:2027` | LOW | Already protected |
| Agent with MCP | `app.rs:2208` | MEDIUM | `INFER_TIMEOUT` (120s) |
| Workflow run | `app.rs:2537` | HIGH | Custom workflow timeout |

### Implementation Steps

#### Step 1.1: Import timeout utilities

**File**: `src/tui/app.rs` (top of file)

```rust
// Add to imports
use tokio::time::timeout;
use crate::util::constants::{INFER_TIMEOUT, EXEC_TIMEOUT, FETCH_TIMEOUT};
```

#### Step 1.2: Wrap LLM inference (line ~1134)

**Before**:
```rust
tokio::spawn(async move {
    match ChatAgent::new() {
        Ok(mut agent) => match agent.infer(&prompt_with_context).await {
            // ...
        }
    }
});
```

**After**:
```rust
tokio::spawn(async move {
    match ChatAgent::new() {
        Ok(mut agent) => {
            match timeout(INFER_TIMEOUT, agent.infer(&prompt_with_context)).await {
                Ok(Ok(response)) => {
                    let _ = tx.send(response).await;
                }
                Ok(Err(e)) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
                Err(_) => {
                    let _ = tx.send(format!("Error: LLM inference timed out after {}s", INFER_TIMEOUT.as_secs())).await;
                }
            }
        }
        Err(e) => {
            let _ = tx.send(format!("Error: {}", e)).await;
        }
    }
});
```

#### Step 1.3: Wrap streaming inference (line ~1863)

Same pattern as Step 1.2 but for streaming variant.

#### Step 1.4: Wrap shell execution (line ~1913)

```rust
match timeout(EXEC_TIMEOUT, agent.exec_command(&command)).await {
    Ok(Ok(output)) => { /* ... */ }
    Ok(Err(e)) => { /* ... */ }
    Err(_) => {
        let _ = tx.send(format!("Command timed out after {}s", EXEC_TIMEOUT.as_secs())).await;
    }
}
```

#### Step 1.5: Wrap HTTP fetch (line ~1944)

```rust
match timeout(FETCH_TIMEOUT, agent.fetch(&url, &method)).await {
    Ok(Ok(response)) => { /* ... */ }
    Ok(Err(e)) => { /* ... */ }
    Err(_) => {
        let _ = tx.send(format!("HTTP request timed out after {}s", FETCH_TIMEOUT.as_secs())).await;
    }
}
```

#### Step 1.6: Wrap workflow execution (line ~2537)

```rust
// Add workflow timeout constant (5 minutes default)
const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300);

match timeout(WORKFLOW_TIMEOUT, runner.run(workflow)).await {
    Ok(Ok(results)) => { /* ... */ }
    Ok(Err(e)) => { /* ... */ }
    Err(_) => {
        event_log.emit(EventKind::WorkflowFailed {
            error: format!("Workflow timed out after {}s", WORKFLOW_TIMEOUT.as_secs()),
            failed_task: None,
        });
    }
}
```

### Tests to Add

```rust
#[tokio::test]
async fn test_infer_timeout_handling() {
    // Mock provider that hangs
    // Verify timeout error is returned
}

#[tokio::test]
async fn test_exec_timeout_handling() {
    // Use "sleep 120" command with 1s timeout override
    // Verify timeout error
}
```

---

## Task 2: Wire spawn_background() for Task Tracking

### Problem

8 `tokio::spawn()` calls in `app.rs` bypass the `spawn_background()` infrastructure, preventing proper task lifecycle management.

### Current State

```rust
// Infrastructure EXISTS but is UNUSED (marked #[allow(dead_code)])
fn spawn_background<F>(&mut self, future: F) -> bool
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    self.background_tasks.spawn(future);
    true
}
```

### Call Sites to Update

| Line | Context | Pattern |
|------|---------|---------|
| 1134 | Chat infer | spawn → spawn_background |
| 1634 | OpenAI fallback | spawn → spawn_background |
| 1863 | Streaming infer | spawn → spawn_background |
| 1913 | Shell exec | spawn → spawn_background |
| 1944 | HTTP fetch | spawn → spawn_background |
| 2027 | MCP invoke | spawn → spawn_background |
| 2208 | Agent with MCP | spawn → spawn_background |
| 2537 | Workflow run | spawn → spawn_background |

### Implementation Steps

#### Step 2.1: Remove #[allow(dead_code)] from spawn_background

**File**: `src/tui/app.rs` (line ~2606)

```rust
// Remove this annotation
// #[allow(dead_code)]
fn spawn_background<F>(&mut self, future: F) -> bool
```

#### Step 2.2: Change ownership pattern

The issue is `spawn_background` requires `&mut self`, but it's often called where we only have `&self` or we've moved `tx` channels.

**Solution**: Make spawn_background take `Arc<JoinSet>`:

```rust
// Change App field
struct App {
    // Before: background_tasks: JoinSet<()>
    background_tasks: Arc<Mutex<JoinSet<()>>>,
}

// Update spawn_background
fn spawn_background<F>(&self, future: F) -> bool
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(mut tasks) = self.background_tasks.lock() {
        tasks.spawn(future);
        true
    } else {
        false
    }
}
```

**Alternative (simpler)**: Use a channel-based approach:

```rust
// Add to App
spawn_tx: mpsc::UnboundedSender<BoxFuture<'static, ()>>,

// In event loop, drain the channel and spawn tasks
while let Ok(task) = self.spawn_rx.try_recv() {
    self.background_tasks.spawn(task);
}
```

#### Step 2.3: Update each call site

Pattern for each site:

**Before**:
```rust
tokio::spawn(async move {
    // task body
});
```

**After**:
```rust
self.spawn_background(async move {
    // task body
});
```

### Tests to Add

```rust
#[tokio::test]
async fn test_background_tasks_tracked() {
    let mut app = App::new_for_test();
    app.spawn_background(async { /* ... */ });
    assert!(!app.background_tasks.is_empty());
}

#[tokio::test]
async fn test_cancel_background_tasks_cleanup() {
    let mut app = App::new_for_test();
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    app.spawn_background(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        flag_clone.store(true, Ordering::SeqCst);
    });
    app.cancel_background_tasks().await;
    // Task should be aborted, not completed
    assert!(!flag.load(Ordering::SeqCst));
}
```

---

## Task 3: Cache build_timeline_entries()

### Problem

`build_timeline_entries()` allocates ~121 objects per frame at 60 FPS = 7,260+ allocations/second.

### Current Implementation

**File**: `src/tui/panels/progress.rs` (line ~99)

```rust
fn build_timeline_entries(&self) -> Vec<TimelineEntry> {
    self.state
        .task_order
        .iter()
        .filter_map(|id| { /* allocates per task */ })
        .collect()  // NEW Vec every frame
}
```

### Implementation Steps

#### Step 3.1: Add cache fields to TuiState

**File**: `src/tui/state.rs` (in TuiState struct)

```rust
pub struct TuiState {
    // ... existing fields ...

    // === CACHE: Timeline entries ===
    /// Cached timeline entries (rebuilt on state changes)
    cached_timeline_entries: Vec<TimelineEntry>,
    /// Cache version (incremented when timeline state changes)
    timeline_version: u32,
    /// Last version used to build cache
    timeline_cache_version: u32,
}
```

#### Step 3.2: Add cache initialization

```rust
impl TuiState {
    pub fn new(workflow_name: &str) -> Self {
        Self {
            // ... existing fields ...
            cached_timeline_entries: Vec::new(),
            timeline_version: 0,
            timeline_cache_version: 0,
        }
    }
}
```

#### Step 3.3: Add invalidation method

```rust
impl TuiState {
    /// Invalidate timeline cache (called when task state changes)
    #[inline]
    pub fn invalidate_timeline_cache(&mut self) {
        self.timeline_version = self.timeline_version.wrapping_add(1);
    }
}
```

#### Step 3.4: Add cache rebuild method

```rust
impl TuiState {
    /// Get cached timeline entries, rebuilding if necessary
    pub fn get_timeline_entries(&mut self) -> &[TimelineEntry] {
        if self.timeline_cache_version != self.timeline_version {
            self.rebuild_timeline_cache();
        }
        &self.cached_timeline_entries
    }

    fn rebuild_timeline_cache(&mut self) {
        self.cached_timeline_entries.clear();
        for id in &self.task_order {
            if let Some(task) = self.tasks.get(id) {
                let mut entry = TimelineEntry::new(&task.id, task.status);
                if let Some(ms) = task.duration_ms {
                    entry = entry.with_duration(ms);
                }
                if self.current_task.as_ref() == Some(&task.id) {
                    entry = entry.current();
                }
                entry = entry.with_breakpoint(self.has_breakpoint(&task.id));
                self.cached_timeline_entries.push(entry);
            }
        }
        self.timeline_cache_version = self.timeline_version;
    }
}
```

#### Step 3.5: Add invalidation calls to state mutation methods

**File**: `src/tui/state.rs`

```rust
// In update_task_status()
pub fn update_task_status(&mut self, task_id: &str, status: TaskStatus) {
    if let Some(task) = self.tasks.get_mut(task_id) {
        task.status = status;
    }
    self.invalidate_timeline_cache(); // ADD THIS
}

// In set_current_task()
pub fn set_current_task(&mut self, task_id: Option<String>) {
    self.current_task = task_id;
    self.invalidate_timeline_cache(); // ADD THIS
}

// In update_task_duration()
pub fn update_task_duration(&mut self, task_id: &str, duration_ms: u64) {
    if let Some(task) = self.tasks.get_mut(task_id) {
        task.duration_ms = Some(duration_ms);
    }
    self.invalidate_timeline_cache(); // ADD THIS
}

// In add_breakpoint() / remove_breakpoint()
pub fn toggle_breakpoint(&mut self, bp: Breakpoint) {
    if self.breakpoints.contains(&bp) {
        self.breakpoints.remove(&bp);
    } else {
        self.breakpoints.insert(bp);
    }
    self.invalidate_timeline_cache(); // ADD THIS
}
```

#### Step 3.6: Update ProgressPanel to use cache

**File**: `src/tui/panels/progress.rs`

**Before**:
```rust
fn render_timeline(&self, area: Rect, buf: &mut Buffer) {
    let entries = self.build_timeline_entries();
    // ...
}
```

**After**:
```rust
fn render_timeline(&mut self, area: Rect, buf: &mut Buffer) {
    // Note: render now needs &mut self for cache access
    let entries = self.state.get_timeline_entries();
    // ...
}
```

**Issue**: ratatui's `Widget::render()` takes `self` by value. We need to:
1. Pre-compute cache before rendering, OR
2. Use interior mutability (RefCell)

**Recommended approach**: Pre-compute in `App::draw()`:

```rust
// In App::draw() before calling render
self.state.get_timeline_entries(); // Forces cache rebuild if needed

// Then in ProgressPanel, just access the cached entries
fn render_timeline(&self, area: Rect, buf: &mut Buffer) {
    let entries = &self.state.cached_timeline_entries;
    // ...
}
```

### Performance Impact

| Metric | Before | After |
|--------|--------|-------|
| Allocations/frame | ~121 | 0 (cache hit) |
| Allocations/second (60 FPS) | 7,260+ | ~10 (only on state changes) |
| Memory | New Vec each frame | Reused Vec |

### Tests to Add

```rust
#[test]
fn test_timeline_cache_invalidation() {
    let mut state = TuiState::new("test");
    state.add_task("task1", TaskStatus::Pending);

    let v1 = state.timeline_version;
    let _ = state.get_timeline_entries();

    state.update_task_status("task1", TaskStatus::Running);
    let v2 = state.timeline_version;

    assert_ne!(v1, v2, "Version should change after status update");
}

#[test]
fn test_timeline_cache_reuse() {
    let mut state = TuiState::new("test");
    state.add_task("task1", TaskStatus::Pending);

    let entries1 = state.get_timeline_entries();
    let len1 = entries1.len();

    // No changes, cache should be reused
    let entries2 = state.get_timeline_entries();
    let len2 = entries2.len();

    assert_eq!(len1, len2);
    // Same cache version used
    assert_eq!(state.timeline_cache_version, state.timeline_version);
}
```

---

## Execution Order

1. **Task 3 (Cache)** - Lowest risk, isolated changes
2. **Task 1 (Timeouts)** - Add timeout wrappers incrementally
3. **Task 2 (spawn_background)** - Requires ownership changes, test thoroughly

## Commit Strategy

```
fix(tui): cache timeline entries to reduce per-frame allocations
fix(tui): add timeout protection to spawned async operations
refactor(tui): wire spawn_background() for proper task lifecycle
```

---

## Verification Checklist

- [ ] All 8 spawn sites use spawn_background()
- [ ] All async operations have timeout protection
- [ ] Timeline cache is invalidated on all state mutations
- [ ] Tests pass: `cargo nextest run --features tui`
- [ ] No performance regression: frame time < 16ms
- [ ] No memory leaks: run TUI for 5 minutes, check RSS

---

## Risk Assessment

| Task | Risk | Mitigation |
|------|------|------------|
| Timeouts | Low | Well-defined constants, simple wrapping |
| spawn_background | Medium | Ownership changes, test cancellation |
| Cache | Low | Isolated to TuiState/ProgressPanel |
