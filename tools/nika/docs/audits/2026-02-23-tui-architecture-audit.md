# Nika TUI Architecture Audit

**Date:** 2026-02-23
**Version:** v0.7.2
**Auditor:** Claude Opus 4.5 (rust-architect)
**Scope:** `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/tui/`

---

## Executive Summary

The Nika TUI is a **well-architected** terminal UI with solid foundations:
- Panic recovery hook for terminal state restoration
- `spawn_tracked()` pattern for background task lifecycle management
- `parking_lot::Mutex` for lock-free task tracking
- Drop implementation for cleanup
- Dirty flags pattern for render optimization

However, several **design patterns pose crash risks** that should be addressed:

| Category | Severity | Issues Found |
|----------|----------|--------------|
| Error Propagation | HIGH | 40+ `let _ =` silent drops |
| State Management | MEDIUM | Dual focus state requires sync |
| Resource Cleanup | LOW | Best-effort cleanup without retry |
| Panic Points | LOW | 1 production `unwrap()` |

---

## 1. State Management Issues

### 1.1 Dual Focus State (MEDIUM)

**Location:** `app.rs` + `state.rs`

The application maintains two focus states:
- `App.focus_state: FocusState` (in app.rs)
- `TuiState.focus: FocusedPanel` (in state.rs)

**Risk:** These can become desynchronized if updated independently.

**Current Mitigation:** `apply_action()` synchronizes them via `Action::FocusPanel`.

**Recommendation:**
```rust
// BEFORE: Dual state
struct App {
    focus_state: FocusState,
    state: TuiState, // contains state.focus
}

// AFTER: Single source of truth
struct App {
    state: TuiState, // owns focus
}
impl App {
    fn focus(&self) -> &FocusState { &self.state.focus }
}
```

### 1.2 View State Lifecycle (LOW)

**Location:** `views/` modules

Each view (`ChatView`, `HomeView`, `StudioView`) maintains its own state independently. View state is not persisted across view switches in some cases.

**Risk:** User may lose context when switching views rapidly.

**Recommendation:** Consider a unified `ViewContext` that persists across switches.

---

## 2. Event Loop Problems

### 2.1 Non-Blocking Channel Drain (GOOD)

**Location:** `app.rs` - `poll_runtime_events()`

The event loop correctly uses `try_recv()` for non-blocking channel reads:

```rust
loop {
    match self.event_rx.try_recv() {
        Ok(event) => self.handle_event(event),
        Err(TryRecvError::Empty) => break,
        Err(TryRecvError::Lagged(n)) => {
            tracing::warn!("Missed {} events", n);
            break;
        }
    }
}
```

**Assessment:** Correct pattern. The `Lagged` handling logs but continues.

### 2.2 Tick Rate and Responsiveness (GOOD)

**Location:** `app.rs` - `run_unified()`

```rust
let tick_rate = Duration::from_millis(16); // ~60 FPS
if event::poll(tick_rate)? {
    // Handle input
}
```

**Assessment:** 60 FPS target is appropriate for terminal UIs.

### 2.3 Async Response Handling (GOOD)

**Location:** `app.rs` - `poll_response_channels()`

Response channels are polled after event handling:
```rust
self.poll_runtime_events();
self.state.tick();
self.poll_response_channels();
```

**Assessment:** Correct ordering - responses are processed each frame.

---

## 3. Rendering Bugs

### 3.1 Dirty Flags Pattern (GOOD)

**Location:** `state.rs` - `DirtyFlags`

```rust
pub struct DirtyFlags {
    pub dag_changed: bool,
    pub mcp_changed: bool,
    pub logs_changed: bool,
    pub tasks_changed: bool,
}
```

**Assessment:** Proper optimization pattern to avoid unnecessary redraws.

### 3.2 JSON Format Caching (GOOD)

**Location:** `state.rs` - `JsonFormatCache`

```rust
pub struct JsonFormatCache {
    pub input: Option<String>,
    pub output: Option<String>,
    pub context: Option<String>,
}
```

**Assessment:** Caches expensive JSON formatting operations.

### 3.3 Potential Stale Render (LOW)

**Location:** `views/studio.rs`

```rust
*self.cached_workflow.borrow_mut() = workflow.ok();
```

**Risk:** `workflow.ok()` silently discards parse errors. The cached workflow may be `None` when it should show an error.

**Recommendation:**
```rust
// Store Result, not Option
cached_workflow: RefCell<Result<Workflow, ParseError>>
```

---

## 4. Resource Leaks

### 4.1 Background Task Tracking (GOOD)

**Location:** `app.rs` - `spawn_tracked()`

```rust
pub fn spawn_tracked<F>(&self, name: &str, future: F)
where F: Future<Output = ()> + Send + 'static
{
    let handle = tokio::spawn(future);
    self.background_tasks.lock().push((name.to_string(), handle.abort_handle()));
}
```

**Assessment:** Excellent pattern. All spawned tasks are tracked with `AbortHandle`.

### 4.2 Task Cancellation (GOOD)

**Location:** `app.rs` - `cancel_background_tasks()`

```rust
fn cancel_background_tasks(&self) {
    let tasks = self.background_tasks.lock();
    for (name, handle) in tasks.iter() {
        handle.abort();
        tracing::debug!("Aborted background task: {}", name);
    }
}
```

**Assessment:** All tracked tasks are aborted on exit.

### 4.3 MCP Client Lifecycle (MEDIUM)

**Location:** `app.rs` - `mcp_clients: DashMap<String, OnceCell<Arc<McpClient>>>`

**Risk:** MCP clients are created lazily but never explicitly closed. The `Drop` for `McpClient` should handle cleanup, but this was not verified.

**Recommendation:** Add explicit `close()` method to `McpClient` and call in `cleanup()`.

### 4.4 Channel Cleanup (GOOD)

**Location:** `app.rs` - Drop impl

Channels are dropped when App is dropped, which signals senders to stop.

---

## 5. Error Propagation Gaps

### 5.1 Silent Error Drops (HIGH)

**Found:** 40+ instances of `let _ =` pattern in production code.

**Critical Locations:**

| File | Line | Pattern | Risk |
|------|------|---------|------|
| `app.rs` | 276 | `let _ = studio_view.load_file(...)` | File load failure ignored |
| `app.rs` | 426 | `let _ = self.studio_view.load_file(path)` | Silent failure |
| `app.rs` | 1142-1158 | `let _ = tx.send(...)` | Channel send failures ignored |
| `app.rs` | 2790-2796 | `let _ = disable_raw_mode()` | Terminal cleanup may fail |
| `chat_agent.rs` | 477-526 | Multiple `let _ = tx.send(...)` | Streaming failures ignored |

**Impact:** Users may not see errors when:
- Files fail to load
- MCP connections fail
- Streaming responses fail
- Terminal cleanup fails

**Recommendation:**
```rust
// BEFORE: Silent drop
let _ = tx.send(response).await;

// AFTER: Log on failure
if tx.send(response).await.is_err() {
    tracing::warn!("Channel closed, response discarded");
}
```

### 5.2 `.ok()` Error Suppression (MEDIUM)

**Found:** 9 instances converting errors to `None`.

| File | Line | Pattern |
|------|------|---------|
| `app.rs` | 284 | `ChatAgent::new().ok()` |
| `app.rs` | 334 | `ChatAgent::new().ok()` |
| `app.rs` | 459 | `ChatAgent::new().ok()` |
| `views/studio.rs` | 692 | `workflow.ok()` |
| `views/chat.rs` | 325 | `Clipboard::new().ok()` |

**Impact:** Initialization failures are silently ignored. Users get no feedback.

**Recommendation:**
```rust
// BEFORE
let chat_agent = ChatAgent::new().ok();

// AFTER
let chat_agent = match ChatAgent::new() {
    Ok(agent) => Some(agent),
    Err(e) => {
        tracing::warn!("ChatAgent init failed: {}", e);
        None
    }
};
```

### 5.3 Production Unwrap (LOW)

**Found:** 1 production `unwrap()` in non-test code.

**Location:** `app.rs:2090`
```rust
available_servers.into_iter().next().unwrap()
```

**Risk:** Panic if `available_servers` is empty.

**Recommendation:**
```rust
// BEFORE
available_servers.into_iter().next().unwrap()

// AFTER
available_servers.into_iter().next()
    .ok_or(NikaError::NoMcpServersAvailable)?
```

---

## 6. Architectural Improvements

### 6.1 Unified State Machine

**Current:** State scattered across `App`, `TuiState`, `FocusState`, `ViewState`.

**Proposed:**
```
┌─────────────────────────────────────────────────────────────────────────┐
│  AppState (single source of truth)                                      │
├─────────────────────────────────────────────────────────────────────────┤
│  ├── TuiMode (Normal, Streaming, Search, Help, Settings)               │
│  ├── FocusState (current panel, history stack)                         │
│  ├── ViewState (Chat, Home, Studio, Monitor)                           │
│  │   └── Each view owns its internal state                             │
│  ├── WorkflowState (tasks, flows, progress)                            │
│  └── RuntimeState (MCP connections, background tasks)                  │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Error Result Channel

**Current:** Errors sent as strings via `tx.send(format!("Error: {}", e))`.

**Proposed:**
```rust
enum TuiMessage {
    Response(String),
    Error { code: String, message: String, recoverable: bool },
    Progress { task_id: String, percent: u8 },
}
```

This enables:
- Structured error display
- Retry logic for recoverable errors
- Progress tracking for long operations

### 6.3 Graceful Degradation

**Current:** Components fail silently (clipboard, chat agent, MCP).

**Proposed:**
```rust
struct FeatureStatus {
    clipboard: FeatureState,
    chat_agent: FeatureState,
    mcp_servers: HashMap<String, FeatureState>,
}

enum FeatureState {
    Available,
    Degraded { reason: String },
    Unavailable { error: String },
}
```

Display degraded features in status bar:
```
[Chat] [MCP:novanet] [Clipboard: unavailable]
```

### 6.4 Terminal Recovery Enhancement

**Current:** Panic hook restores terminal, but cleanup is best-effort.

**Proposed:**
```rust
fn cleanup(&mut self) -> Result<(), CleanupError> {
    let mut errors = Vec::new();

    // Cancel tasks with timeout
    self.cancel_background_tasks_with_timeout(Duration::from_secs(5))?;

    // Close MCP connections
    for (name, client) in self.mcp_clients.iter() {
        if let Err(e) = client.close().await {
            errors.push(format!("MCP {}: {}", name, e));
        }
    }

    // Restore terminal (retry on failure)
    for attempt in 1..=3 {
        match self.restore_terminal() {
            Ok(()) => break,
            Err(e) if attempt < 3 => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => errors.push(format!("Terminal: {}", e)),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CleanupError { errors })
    }
}
```

---

## 7. Test Coverage Analysis

**Current:** Most `unwrap()`/`expect()` calls are in `#[cfg(test)]` modules (240+ instances).

**Assessment:** Test code using `unwrap()` is acceptable.

**Gap:** Integration tests for:
- Panic recovery (does terminal restore?)
- Channel capacity overflow
- MCP connection timeout
- View switch state persistence

---

## 8. Summary of Recommendations

### High Priority

1. **Add logging to `let _ =` patterns** - 40+ silent drops mask errors
2. **Replace production `unwrap()`** - `app.rs:2090` can panic
3. **Log `.ok()` failures** - Initialization errors should be visible

### Medium Priority

4. **Unify focus state** - Single source of truth for focus
5. **Add MCP client cleanup** - Explicit close on exit
6. **Structured error channel** - `TuiMessage::Error` enum

### Low Priority

7. **Feature status display** - Show degraded features
8. **Retry cleanup** - Robust terminal restoration
9. **View context persistence** - Maintain state across switches

---

## 9. Crash Risk Assessment

| Scenario | Current Behavior | Risk Level |
|----------|------------------|------------|
| Panic in render loop | Terminal restored via hook | LOW |
| MCP connection fails | Silent failure, chat continues | MEDIUM |
| Channel overflow | Lagged events logged, continue | LOW |
| Empty MCP servers list | `unwrap()` panic | HIGH |
| Clipboard unavailable | Graceful fallback to None | LOW |
| File load failure | Silent failure | MEDIUM |
| Background task panic | Abort handle dropped | LOW |

**Overall Assessment:** The TUI is **production-ready** with minor crash risks. The panic hook provides good recovery. The main concerns are silent error handling that can confuse users.

---

## Appendix: Files Reviewed

- `mod.rs` - Entry point, panic hook (165 lines)
- `app.rs` - Main application (~4000 lines)
- `state.rs` - TUI state management (~1700 lines)
- `views/mod.rs` - View architecture
- `focus.rs` - Focus state management
- `cache.rs` - Render cache
- `chat_agent.rs` - Chat agent implementation
- Various widget files for specific patterns

---

*Generated by rust-architect subagent*
