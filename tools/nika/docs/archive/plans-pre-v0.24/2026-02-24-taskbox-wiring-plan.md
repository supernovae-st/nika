# TaskBox Wiring Implementation Plan

> **Date:** 2026-02-24
> **Status:** Ready for Implementation
> **Priority:** CRITICAL - TaskBox widgets exist but are not wired to render inline

---

## Executive Summary

The TaskBox widget system is **85% implemented** but **only 30% operational**. All 5 verb boxes (InferBox, ExecBox, FetchBox, InvokeBox, AgentBox) exist with full Widget implementations, but the event handlers in `app.rs` only update the Activity panel - they never create TaskBox instances for inline rendering in Chat.

```
CURRENT STATE (BROKEN)
----------------------
StreamChunk::InferStart  → add_infer_activity()  → Activity Panel ONLY
StreamChunk::ExecStart   → add_exec_activity()   → Activity Panel ONLY
StreamChunk::FetchStart  → add_fetch_activity()  → Activity Panel ONLY
StreamChunk::AgentStart  → add_agent_activity()  → Activity Panel ONLY
StreamChunk::McpCallStart → add_mcp_call()       → McpCallData (legacy)

EXPECTED STATE (FIXED)
----------------------
StreamChunk::InferStart  → add_task_box(TaskBox::Infer)  → INLINE in Chat
StreamChunk::ExecStart   → add_task_box(TaskBox::Exec)   → INLINE in Chat
StreamChunk::FetchStart  → add_task_box(TaskBox::Fetch)  → INLINE in Chat
StreamChunk::AgentStart  → add_task_box(TaskBox::Agent)  → INLINE in Chat
StreamChunk::McpCallStart → add_task_box(TaskBox::Invoke) → INLINE in Chat
```

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  TASKBOX WIRING ARCHITECTURE                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────────┐  │
│  │ RigProvider │───►│ StreamChunk │───►│ app.rs Event Handlers           │  │
│  │ ChatAgent   │    │ Enum        │    │ (lines 833-903)                 │  │
│  └─────────────┘    └─────────────┘    └──────────────┬──────────────────┘  │
│                                                        │                    │
│                     ┌──────────────────────────────────┴────────────────┐   │
│                     │                                                   │   │
│                     ▼                                                   ▼   │
│  ┌─────────────────────────────────┐    ┌─────────────────────────────────┐ │
│  │ chat_view.add_task_box()        │    │ chat_view.add_*_activity()      │ │
│  │ (lines 1629-1645)               │    │ (lines 1714-1745)               │ │
│  │                                 │    │                                 │ │
│  │ ✅ Creates InlineContent::Task  │    │ ❌ Only creates ActivityItem    │ │
│  │ ✅ Renders inline in Chat       │    │ ❌ Only shows in Mission Control│ │
│  └─────────────────────────────────┘    └─────────────────────────────────┘ │
│                     │                                                       │
│                     ▼                                                       │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ InlineContent::Task(TaskBox)                                            ││
│  │                                                                         ││
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌─────────┐          ││
│  │  │InferBox │ │ExecBox  │ │FetchBox │ │InvokeBox │ │AgentBox │          ││
│  │  │⚡ Violet│ │📟 Amber │ │🛰️ Cyan  │ │🔌 Emerald│ │🐔 Rose  │          ││
│  │  │318 lines│ │287 lines│ │297 lines│ │311 lines │ │152 lines│          ││
│  │  └─────────┘ └─────────┘ └─────────┘ └──────────┘ └─────────┘          ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Implementation Tasks

### Phase 1: TaskBox Wiring (CRITICAL)

#### Task 1.1: Wire InferStart → InferBox

**File:** `src/tui/app.rs` (lines 852-863)

**Current Code:**
```rust
StreamChunk::InferStart { model, prompt_tokens, max_tokens } => {
    self.chat_view.start_infer_stream(&model, prompt_tokens, max_tokens);
    self.chat_view.start_streaming_with_verb(DecryptVerb::Infer);
    tracing::debug!(model = %model, prompt_tokens, max_tokens, "Infer started");
}
```

**New Code:**
```rust
StreamChunk::InferStart { model, prompt_tokens, max_tokens } => {
    // Create InferBox with running state
    let infer_box = InferBox::new(&model, "(streaming...)")
        .with_state(BoxState::running())
        .with_tokens(prompt_tokens, 0);
    self.chat_view.add_task_box(TaskBox::Infer(infer_box));

    // Keep the matrix decrypt effect for streaming visual
    self.chat_view.start_streaming_with_verb(DecryptVerb::Infer);
    tracing::debug!(model = %model, prompt_tokens, max_tokens, "Infer started with TaskBox");
}
```

**Required Import:**
```rust
use crate::tui::widgets::task_box::{TaskBox, InferBox, BoxState};
```

**Tests:**
- Verify InferBox appears inline when `/infer` command runs
- Verify spinner animation during streaming
- Verify state transitions (Running → Success/Failed)

---

#### Task 1.2: Wire ExecStart → ExecBox

**File:** `src/tui/app.rs` (lines 885-891)

**Current Code:**
```rust
StreamChunk::ExecStart { command } => {
    self.chat_view.add_exec_activity(&command);
    tracing::debug!(command = %command, "Exec started");
}
StreamChunk::ExecComplete => {
    self.chat_view.complete_exec_activity();
    tracing::debug!("Exec completed");
}
```

**New Code:**
```rust
StreamChunk::ExecStart { command } => {
    let exec_box = ExecBox::new(&command)
        .with_state(BoxState::running());
    self.chat_view.add_task_box(TaskBox::Exec(exec_box));
    tracing::debug!(command = %command, "Exec started with TaskBox");
}
StreamChunk::ExecOutput { stdout, stderr } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Exec(ref mut exec) = task_box {
            if let Some(out) = stdout {
                exec.append_stdout(&out);
            }
            if let Some(err) = stderr {
                exec.append_stderr(&err);
            }
        }
    });
}
StreamChunk::ExecComplete { exit_code, duration_ms } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Exec(ref mut exec) = task_box {
            exec.exit_code = Some(exit_code);
            if exit_code == 0 {
                exec.state = BoxState::success(duration_ms);
            } else {
                exec.state = BoxState::failed(format!("Exit code: {}", exit_code), duration_ms);
            }
        }
    });
    tracing::debug!(exit_code, "Exec completed");
}
```

**Required Import:**
```rust
use crate::tui::widgets::task_box::ExecBox;
```

**Note:** Need to add `ExecOutput` variant to StreamChunk if not exists, or use existing output mechanism.

---

#### Task 1.3: Wire FetchStart → FetchBox

**File:** `src/tui/app.rs` (lines 893-900)

**Current Code:**
```rust
StreamChunk::FetchStart { url, method } => {
    self.chat_view.add_fetch_activity(&url, &method);
    tracing::debug!(url = %url, method = %method, "Fetch started");
}
StreamChunk::FetchComplete => {
    self.chat_view.complete_fetch_activity();
    tracing::debug!("Fetch completed");
}
```

**New Code:**
```rust
StreamChunk::FetchStart { url, method } => {
    let fetch_box = FetchBox::new(&method, &url)
        .with_state(BoxState::running());
    self.chat_view.add_task_box(TaskBox::Fetch(fetch_box));
    tracing::debug!(url = %url, method = %method, "Fetch started with TaskBox");
}
StreamChunk::FetchComplete { status_code, body, duration_ms } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Fetch(ref mut fetch) = task_box {
            fetch.status_code = Some(status_code);
            if let Some(b) = body {
                fetch.response_body = Some(b);
            }
            if status_code >= 200 && status_code < 400 {
                fetch.state = BoxState::success(duration_ms);
            } else {
                fetch.state = BoxState::failed(format!("HTTP {}", status_code), duration_ms);
            }
        }
    });
    tracing::debug!(status_code, "Fetch completed");
}
```

**Required Import:**
```rust
use crate::tui::widgets::task_box::FetchBox;
```

---

#### Task 1.4: Wire AgentStart → AgentBox

**File:** `src/tui/app.rs` (lines 901-907)

**Current Code:**
```rust
StreamChunk::AgentStart { goal } => {
    self.chat_view.add_agent_activity(&goal);
    tracing::debug!(goal = %goal, "Agent started");
}
StreamChunk::AgentComplete => {
    self.chat_view.complete_agent_activity();
    tracing::debug!("Agent completed");
}
```

**New Code:**
```rust
StreamChunk::AgentStart { goal, max_turns } => {
    let agent_box = AgentBox::new("agent", &goal)
        .with_state(BoxState::running())
        .with_turn(0, max_turns.unwrap_or(10));
    self.chat_view.add_task_box(TaskBox::Agent(agent_box));
    tracing::debug!(goal = %goal, "Agent started with TaskBox");
}
StreamChunk::AgentTurn { turn, tokens_in, tokens_out } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Agent(ref mut agent) = task_box {
            agent.next_turn();
            agent.tokens_in += tokens_in;
            agent.tokens_out += tokens_out;
        }
    });
}
StreamChunk::AgentComplete { final_response, duration_ms } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Agent(ref mut agent) = task_box {
            agent.final_response = Some(final_response);
            agent.state = BoxState::success(duration_ms);
        }
    });
    tracing::debug!("Agent completed");
}
```

**Required Import:**
```rust
use crate::tui::widgets::task_box::AgentBox;
```

---

#### Task 1.5: Migrate McpCallStart → InvokeBox

**File:** `src/tui/app.rs` (lines 833-841)

**Current Code:**
```rust
StreamChunk::McpCallStart { tool, server, params } => {
    self.chat_view.add_mcp_call(&tool, &server, &params);
    tracing::debug!(tool = %tool, server = %server, "MCP call started");
}
```

**New Code:**
```rust
StreamChunk::McpCallStart { tool, server, params } => {
    let invoke_box = InvokeBox::new(&tool, &server)
        .with_params_str(&params)
        .with_state(BoxState::running());
    self.chat_view.add_task_box(TaskBox::Invoke(invoke_box));
    tracing::debug!(tool = %tool, server = %server, "MCP call started with InvokeBox");
}
StreamChunk::McpCallComplete { result } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Invoke(ref mut invoke) = task_box {
            invoke.result = Some(serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result.clone())));
            invoke.state = BoxState::success(0); // Duration tracked elsewhere
        }
    });
}
StreamChunk::McpCallFailed { error } => {
    self.chat_view.update_last_task_box(|task_box| {
        if let TaskBox::Invoke(ref mut invoke) = task_box {
            invoke.error = Some(error.clone());
            invoke.state = BoxState::failed(&error, 0);
        }
    });
}
```

**Required Import:**
```rust
use crate::tui::widgets::task_box::InvokeBox;
```

---

#### Task 1.6: Remove InlineContent::McpCall (Cleanup)

**File:** `src/tui/views/chat.rs` (lines 253-260)

**Current Code:**
```rust
pub enum InlineContent {
    McpCall(McpCallData),
    InferStream(InferStreamData),
    Task(TaskBox),
}
```

**New Code:**
```rust
pub enum InlineContent {
    InferStream(InferStreamData),  // Keep for streaming decrypt effect
    Task(TaskBox),                  // Unified for all 5 verbs
}
```

**Also Remove:**
- `add_mcp_call()` method (line 1565-1588)
- `complete_mcp_call()` method (line 1591-1605)
- `fail_mcp_call()` method (line 1606-1619)
- `McpCallData` struct and related imports
- Rendering code for `InlineContent::McpCall` (lines 4476-4526)

---

### Phase 2: Fix Tokio Test Failures

#### Task 2.1: Fix Non-Async Tests (lines 4442, 4522)

**File:** `src/tui/app.rs`

**Change:**
```rust
// Line 4442: Remove async, change attribute
#[test]  // was #[tokio::test]
fn test_convert_view_action_send_chat_message() {  // was async fn
    // ... test body unchanged
}

// Line 4522: Same pattern
#[test]
fn test_chat_overlay_send_action() {
    // ... test body unchanged
}
```

---

#### Task 2.2: Fix Async Tests with block_in_place (lines 4701, 4720, 4742)

**File:** `src/tui/app.rs`

**Pattern:**
```rust
#[tokio::test]
async fn test_spawn_tracked_adds_handle() {
    // Wrap sync operations in block_in_place
    let (mut app, _temp_dir) = tokio::task::block_in_place(|| {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();
        (app, temp_dir)
    });

    // Continue with async operations
    app.spawn_tracked(async {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    });

    // ... rest of test
}
```

**Apply to:**
- `test_spawn_tracked_adds_handle` (line 4701)
- `test_spawn_tracked_multiple_tasks` (line 4720)
- `test_cancel_background_tasks_aborts_all` (line 4742)

---

### Phase 3: Performance Fixes

#### Task 3.1: LRU Cache for Format Loop

**File:** `src/tui/state.rs` (line 1551)

**Current Code:**
```rust
let keys: Vec<String> = self.cache.keys().take(to_remove).cloned().collect();
```

**Fix:** Replace HashMap with `lru` crate:

```rust
// In Cargo.toml
lru = "0.12"

// In state.rs
use lru::LruCache;

pub struct FormatCache {
    cache: LruCache<String, String>,
}

impl FormatCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    pub fn get_or_insert(&mut self, key: &str, f: impl FnOnce() -> String) -> &str {
        if !self.cache.contains(key) {
            let value = f();
            self.cache.put(key.to_string(), value);
        }
        self.cache.get(key).unwrap()
    }
}
```

---

#### Task 3.2: Buffer Reuse in Home View

**File:** `src/tui/views/home.rs` (lines 157, 286, 298)

**Current Code:**
```rust
.map(|n| n.to_string_lossy().to_string())
```

**Fix:** Cache in struct:
```rust
pub struct HomeView {
    // Add cached buffer
    filename_cache: Vec<String>,
}

impl HomeView {
    fn refresh_filenames(&mut self) {
        self.filename_cache.clear();
        for entry in entries {
            if let Some(name) = entry.file_name().to_str() {
                self.filename_cache.push(name.to_string());
            }
        }
    }
}
```

---

#### Task 3.3: Line Number Cache in Studio

**File:** `src/tui/views/studio.rs` (line 639)

**Current Code:**
```rust
format!("{:4} ", line_num)
```

**Fix:** Pre-generate line numbers:
```rust
pub struct StudioView {
    line_number_cache: Vec<String>,
}

impl StudioView {
    fn ensure_line_cache(&mut self, max_line: usize) {
        while self.line_number_cache.len() < max_line {
            let n = self.line_number_cache.len() + 1;
            self.line_number_cache.push(format!("{:4} ", n));
        }
    }

    fn get_line_number(&self, line: usize) -> &str {
        &self.line_number_cache[line - 1]
    }
}
```

---

#### Task 3.4: Reduce MCP Cloning

**File:** `src/tui/app.rs` (lines 2437-2447)

**Current Code:**
```rust
let tool_name = tool.clone();
let server_name_clone = server_name.clone();
// ... more clones inside
```

**Fix:** Clone once, reuse:
```rust
let tool_name = tool.clone();
let server = server_name.clone();
self.spawn_tracked({
    let mcp_cache = Arc::clone(&self.mcp_client_cache);
    let configs = Arc::clone(&self.mcp_configs);
    async move {
        // Use server directly, no more clones
        let client = mcp_cache
            .entry(server.clone())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();
        // ...
    }
});
```

---

#### Task 3.5: Cache Status String

**File:** `src/tui/app.rs` (line 1102)

**Current Code:**
```rust
format!("Tasks: {}/{}", completed, task_count)
```

**Fix:** Cache with change detection:
```rust
pub struct App {
    status_cache: Option<(usize, usize, String)>,
}

fn get_status_string(&mut self, completed: usize, task_count: usize) -> &str {
    if self.status_cache.as_ref().map(|(c, t, _)| (*c, *t)) != Some((completed, task_count)) {
        let s = format!("Tasks: {}/{}", completed, task_count);
        self.status_cache = Some((completed, task_count, s));
    }
    &self.status_cache.as_ref().unwrap().2
}
```

---

### Phase 4: Commit Strategy

#### Commit 1: TaskBox Wiring (Priority 1)
```bash
git add src/tui/app.rs src/tui/views/chat.rs src/tui/widgets/task_box/
git commit -m "feat(tui): wire TaskBox inline rendering for all 5 verbs

- InferStart → TaskBox::Infer(InferBox)
- ExecStart → TaskBox::Exec(ExecBox)
- FetchStart → TaskBox::Fetch(FetchBox)
- AgentStart → TaskBox::Agent(AgentBox)
- McpCallStart → TaskBox::Invoke(InvokeBox)

Removes InlineContent::McpCall in favor of unified TaskBox enum.
All verbs now render with animated boxes inline in Chat view.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

#### Commit 2: Tokio Test Fixes (Priority 2)
```bash
git add src/tui/app.rs
git commit -m "fix(tui): resolve tokio runtime drop in 5 async tests

- test_convert_view_action_send_chat_message: #[tokio::test] → #[test]
- test_chat_overlay_send_action: #[tokio::test] → #[test]
- test_spawn_tracked_*: wrap App::new() in block_in_place()

Fixes: Cannot drop a runtime in a context where blocking is not allowed

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

#### Commit 3: Performance Fixes (Priority 3)
```bash
git add src/tui/state.rs src/tui/views/home.rs src/tui/views/studio.rs src/tui/app.rs Cargo.toml
git commit -m "perf(tui): fix 5 MEDIUM allocation issues in render path

- state.rs: Replace HashMap with LRU cache for format loop
- home.rs: Reuse filename buffers instead of per-frame allocation
- studio.rs: Pre-generate line number strings
- app.rs: Reduce String clones in MCP spawn
- app.rs: Cache status string with change detection

Estimated savings: ~150µs per complex frame

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Verification Checklist

### After Phase 1 (TaskBox Wiring)
- [ ] Run `nika chat` and type `/infer test` - see InferBox inline
- [ ] Run `/exec date` - see ExecBox with command and output
- [ ] Run `/fetch https://httpbin.org/get` - see FetchBox with status
- [ ] Run `/agent hello` - see AgentBox with turns
- [ ] Verify spinner animation during running state
- [ ] Verify green border on success, red on failure

### After Phase 2 (Tokio Tests)
- [ ] `cargo test --features tui` - all 5 previously failing tests pass
- [ ] No "Cannot drop runtime" errors

### After Phase 3 (Performance)
- [ ] `cargo bench` shows improvement (if benchmarks exist)
- [ ] No visible frame drops during typing in Home view search
- [ ] Studio scrolling remains smooth

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing MCP call display | HIGH | Keep McpCallData rendering as fallback during transition |
| Streaming decrypt effect conflict | MEDIUM | Keep InferStream for visual effect, TaskBox for data |
| Performance regression from TaskBox | LOW | TaskBox uses efficient Widget trait, not more allocations |
| Test flakiness from tokio changes | LOW | block_in_place is standard pattern for this issue |

---

## Dependencies

- `lru = "0.12"` - For Phase 3 cache optimization
- No other new dependencies required

---

## Timeline Estimate

| Phase | Tasks | Estimated Time |
|-------|-------|----------------|
| Phase 1 | TaskBox Wiring (6 tasks) | 2-3 hours |
| Phase 2 | Tokio Test Fixes (2 tasks) | 30 minutes |
| Phase 3 | Performance Fixes (5 tasks) | 1-2 hours |
| Phase 4 | Commits & Verification | 30 minutes |
| **Total** | **14 tasks** | **4-6 hours** |

---

## Success Criteria

1. All 5 verbs render TaskBox inline in Chat view
2. All tests pass (including the 5 previously failing)
3. No clippy warnings
4. Performance benchmarks show no regression
5. Visual verification: boxes appear with correct colors, spinners, and states
