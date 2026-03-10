# TODO Remediation Plan - v0.9.5

## Overview

This plan addresses the 6 remaining TODOs identified in the v0.9.x audit.
All are non-blocking enhancements that improve observability and UX.

---

## TODO 1: Status Bar Height Calculation

**File:** `src/tui/app.rs:1951`
**Current:** Returns full terminal size, ignores status bar
**Impact:** LOW - cosmetic layout issue

### Analysis
```rust
fn compute_view_area(&self, terminal_size: Rect) -> Rect {
    // TODO: Subtract status bar height if needed
    terminal_size
}
```

### Implementation
```rust
fn compute_view_area(&self, terminal_size: Rect) -> Rect {
    // Status bar takes 1 line at the bottom
    const STATUS_BAR_HEIGHT: u16 = 1;

    if terminal_size.height > STATUS_BAR_HEIGHT {
        Rect {
            x: terminal_size.x,
            y: terminal_size.y,
            width: terminal_size.width,
            height: terminal_size.height - STATUS_BAR_HEIGHT,
        }
    } else {
        terminal_size
    }
}
```

### Tests
- `test_compute_view_area_subtracts_status_bar()`
- `test_compute_view_area_handles_small_terminal()`

**Effort:** 15 minutes

---

## TODO 2: Wire Token Tracking to Session

**File:** `src/tui/widgets/provider_modal/state.rs:696`
**Current:** `tokens_used: 0` hardcoded
**Impact:** MEDIUM - users can't see actual token usage

### Analysis
```rust
SessionStats {
    tokens_used: 0,  // TODO: Wire to actual session token tracking
    ...
}
```

### Solution
The `ChatView` already tracks `session_context.tokens_used`. We need to pass this
to `ProviderModalState::calculate_session_stats()`.

### Implementation Steps

1. **Add token parameter to `calculate_session_stats()`:**
```rust
pub fn calculate_session_stats(&self, session_tokens: u64) -> SessionStats {
    // ...
    SessionStats {
        tokens_used: session_tokens,  // Use parameter instead of 0
        // ...
    }
}
```

2. **Update call site in app.rs:**
```rust
let session_tokens = self.chat_view.total_tokens();
let stats = self.provider_modal_state.calculate_session_stats(session_tokens);
```

### Tests
- `test_session_stats_uses_provided_tokens()`
- `test_session_stats_defaults_to_zero()`

**Effort:** 30 minutes

---

## TODO 3: Wire MCP Connection Status

**File:** `src/tui/widgets/provider_modal/state.rs:697`
**Current:** `mcp_connections: 0` hardcoded
**Impact:** MEDIUM - users can't see MCP server status

### Analysis
```rust
SessionStats {
    mcp_connections: 0,  // TODO: Wire to MCP client status
    ...
}
```

### Solution
The `ChatView.session_context.mcp_servers` already has `McpServerStatus` entries.
Count the connected ones.

### Implementation Steps

1. **Add mcp_count parameter to `calculate_session_stats()`:**
```rust
pub fn calculate_session_stats(&self, session_tokens: u64, mcp_connected: usize) -> SessionStats {
    // ...
    SessionStats {
        mcp_connections: mcp_connected,
        // ...
    }
}
```

2. **Update call site in app.rs:**
```rust
let session_tokens = self.chat_view.total_tokens();
let mcp_connected = self.chat_view.session_context.mcp_servers
    .iter()
    .filter(|s| matches!(s.status, McpStatus::Hot | McpStatus::Warm | McpStatus::Connected))
    .count();
let stats = self.provider_modal_state.calculate_session_stats(session_tokens, mcp_connected);
```

### Tests
- `test_session_stats_counts_mcp_connections()`
- `test_session_stats_excludes_cold_mcp()`

**Effort:** 30 minutes

---

## TODO 4: Document rig-core Streaming Limitation

**File:** `src/runtime/rig_agent_loop.rs:918`
**Current:** TODO comment about future rig-core feature
**Impact:** NONE - documentation only, waiting on upstream

### Analysis
```rust
/// **TODO:** When rig-core adds a streaming agent API, migrate to full streaming.
```

### Implementation
This is NOT a code change - it's waiting on rig-core upstream.

**Action:** Convert TODO to tracking issue reference:
```rust
/// **NOTE:** Waiting on rig-core streaming agent API.
/// Tracking: https://github.com/0xPlaygrounds/rig/issues/XXX
/// When available, migrate from `agent.prompt()` to streaming.
```

**Effort:** 5 minutes (just documentation)

---

## TODO 5: Full Workflow Execution Integration

**File:** `src/runtime/builtin/run.rs:122`
**Current:** Returns placeholder response
**Impact:** HIGH - `run` builtin doesn't actually execute workflows

### Analysis
```rust
// TODO: Full workflow execution integration (Task 3.9)
// For now, return a placeholder response
```

### Implementation Steps

1. **Import Runner and related types:**
```rust
use crate::runtime::runner::Runner;
use crate::ast::parse_workflow_from_file;
```

2. **Execute workflow with Runner:**
```rust
async fn call_impl(params: RunParams) -> Result<ToolResponse, ToolError> {
    // Validate extension
    let path = std::path::Path::new(&params.workflow);
    if !Self::has_valid_extension(&params.workflow) {
        return Err(ToolError::InvalidParams { ... });
    }

    // Parse workflow
    let workflow = parse_workflow_from_file(path)
        .map_err(|e| ToolError::ExecutionError {
            reason: format!("Failed to parse workflow: {e}")
        })?;

    // Create runner with context
    let mut runner = Runner::new(workflow);
    if let Some(ctx) = params.context {
        runner = runner.with_context(ctx);
    }

    // Execute
    let result = runner.run().await
        .map_err(|e| ToolError::ExecutionError {
            reason: format!("Workflow execution failed: {e}")
        })?;

    // Return result
    Ok(ToolResponse::Success(serde_json::json!({
        "workflow": params.workflow,
        "status": "completed",
        "result": result
    })))
}
```

### Tests
- `test_run_builtin_executes_workflow()`
- `test_run_builtin_validates_extension()`
- `test_run_builtin_passes_context()`

**Effort:** 2 hours

---

## TODO 6: Interactive HITL Mode

**File:** `src/runtime/builtin/prompt.rs:163`
**Current:** Uses default value or errors in interactive mode
**Impact:** MEDIUM - no way to prompt user during workflow

### Analysis
```rust
// TODO: Interactive mode with HITL channel
// For now, interactive mode also uses default or errors
```

### Design Decision Needed

**Option A: Channel-based HITL**
```rust
pub struct HitlChannel {
    tx: mpsc::Sender<HitlRequest>,
    rx: mpsc::Receiver<HitlResponse>,
}

pub struct HitlRequest {
    pub prompt: String,
    pub options: Option<Vec<String>>,
    pub timeout: Option<Duration>,
}
```

**Option B: Callback-based HITL**
```rust
pub trait HitlHandler: Send + Sync {
    async fn prompt(&self, message: &str, default: Option<&str>) -> Result<String, HitlError>;
}
```

### Recommendation
**Option B (Callback-based)** is simpler and integrates better with TUI:
- ChatView implements `HitlHandler`
- Runner holds `Arc<dyn HitlHandler>`
- Prompt builtin calls `handler.prompt()`

### Implementation Steps

1. **Define HitlHandler trait in `src/runtime/hitl.rs`:**
```rust
#[async_trait]
pub trait HitlHandler: Send + Sync {
    async fn prompt(&self, message: &str, default: Option<&str>) -> Result<String>;
}
```

2. **Update Runner to accept HitlHandler:**
```rust
impl Runner {
    pub fn with_hitl(mut self, handler: Arc<dyn HitlHandler>) -> Self {
        self.hitl = Some(handler);
        self
    }
}
```

3. **Update prompt builtin:**
```rust
if let Some(handler) = &self.hitl {
    let response = handler.prompt(&params.message, params.default.as_deref()).await?;
    return Ok(ToolResponse::Success(json!({ "response": response })));
}
```

4. **Implement HitlHandler for ChatView:**
```rust
impl HitlHandler for ChatView {
    async fn prompt(&self, message: &str, default: Option<&str>) -> Result<String> {
        // Show prompt in chat, wait for user input
        // This requires async channel integration
    }
}
```

### Tests
- `test_prompt_builtin_uses_hitl_handler()`
- `test_prompt_builtin_falls_back_to_default()`
- `test_hitl_handler_timeout()`

**Effort:** 4 hours (includes trait, impl, tests)

---

## Implementation Order

| Priority | TODO | Effort | Impact |
|----------|------|--------|--------|
| 1 | TODO 1: Status bar height | 15 min | LOW |
| 2 | TODO 2+3: Wire tokens/MCP | 1 hour | MEDIUM |
| 3 | TODO 4: Document rig-core | 5 min | NONE |
| 4 | TODO 5: Workflow execution | 2 hours | HIGH |
| 5 | TODO 6: HITL mode | 4 hours | MEDIUM |

**Total estimated effort: ~7.5 hours**

---

## Version Strategy

- **v0.9.5**: TODOs 1-4 (quick wins, ~1.5 hours)
- **v0.10.0**: TODO 5 (workflow execution)
- **v0.11.0**: TODO 6 (HITL mode - requires design review)

---

## Acceptance Criteria

- [ ] All 6 TODOs either implemented or converted to tracking issues
- [ ] No new `todo!()` or `unimplemented!()` macros introduced
- [ ] All new code has tests (80%+ coverage)
- [ ] `cargo test --lib` passes (2,881+ tests)
- [ ] `cargo clippy -- -D warnings` passes
