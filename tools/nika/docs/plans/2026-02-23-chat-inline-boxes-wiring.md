# Chat Inline Boxes Wiring Plan

**Date:** 2026-02-23
**Status:** In Progress
**Goal:** Wire all existing inline visualization widgets to chat commands

## Problem Statement

The TUI has rich inline visualization widgets (MCP call boxes, Infer stream boxes, activity stack) that are:
- **Created and exported** in `src/tui/widgets/`
- **Rendered** in `render_messages_v2()` when data exists
- **NOT triggered** by chat commands `/invoke` and `/infer`

The widgets work for workflow execution (via EventLog events) but NOT for direct chat commands.

## Root Cause

```
WORKFLOW EXECUTION (works):
  Executor → emits McpInvoke event → handle_chat_view_event() → add_mcp_call() → rendered

CHAT /invoke (broken):
  handle_chat_invoke() → MCP call → tx.send(result) → NO add_mcp_call() → NOT rendered
```

## Implementation Plan

### Phase 1: Wire /invoke Inline Boxes (P1)

**File:** `src/tui/app.rs`
**Function:** `handle_chat_invoke()`

**Changes:**

1. **Before MCP call** (after line 2155):
   ```rust
   // Add inline MCP call visualization BEFORE the async call
   self.chat_view.add_mcp_call(&tool, &server_name, &serde_json::to_string(&params).unwrap_or_default());
   ```

2. **On success** (after line 2233):
   ```rust
   // Complete the inline MCP call box
   // Need to send via channel since we're in async context
   let _ = status_tx.send(StreamChunk::McpCallComplete {
       result: display.clone()
   }).await;
   ```

3. **On error** (after line 2256):
   ```rust
   // Fail the inline MCP call box
   let _ = status_tx.send(StreamChunk::McpCallFailed {
       error: e.to_string()
   }).await;
   ```

4. **Handle new StreamChunk variants** in main loop:
   ```rust
   StreamChunk::McpCallComplete { result } => {
       self.chat_view.complete_mcp_call(&result);
   }
   StreamChunk::McpCallFailed { error } => {
       self.chat_view.fail_mcp_call(&error);
   }
   ```

**Tests:**
- `test_invoke_creates_inline_mcp_box`
- `test_invoke_success_completes_box`
- `test_invoke_failure_fails_box`

### Phase 2: Wire /infer Inline Boxes (P2)

**File:** `src/tui/app.rs`
**Function:** `handle_chat_infer()`

**Changes:**

1. **Before LLM call**:
   ```rust
   // Start infer stream visualization
   self.chat_view.start_infer_stream(&self.chat_view.current_model, prompt.len() as u32, 4096);
   ```

2. **On streaming chunks** (in stream handler):
   ```rust
   // Update token count during streaming
   self.chat_view.update_infer_tokens(tokens_so_far);
   ```

3. **On completion**:
   ```rust
   // Complete infer stream
   self.chat_view.complete_infer_stream();
   ```

**Tests:**
- `test_infer_creates_inline_stream_box`
- `test_infer_updates_tokens_during_stream`
- `test_infer_completion_closes_box`

### Phase 3: Wire Activity Items (P3)

**File:** `src/tui/app.rs`

**Changes:**

1. **In handle_chat_invoke** - add hot activity:
   ```rust
   let activity_id = format!("invoke-{}", self.activity_counter);
   self.activity_counter += 1;
   self.chat_view.activity_items.push(ActivityItem::hot(activity_id.clone(), "invoke"));
   ```

2. **On completion** - transition to warm:
   ```rust
   self.chat_view.mark_activity_warm(&activity_id, duration);
   ```

3. **In handle_chat_infer** - same pattern for "infer" verb

4. **In handle_chat_agent** - same pattern for "agent" verb

**Tests:**
- `test_invoke_adds_hot_activity`
- `test_invoke_completion_marks_warm`
- `test_activity_stack_shows_multiple_concurrent`

### Phase 4: Enable WOW Effects (P4)

#### 4.1 Matrix Rain on Home View

**File:** `src/tui/views/home.rs`

Add background matrix rain effect when idle:
```rust
use crate::tui::widgets::MatrixRain;

// In render():
if self.idle_frames > 60 {
    MatrixRain::new().render(background_area, frame.buffer_mut());
}
```

#### 4.2 Matrix Decrypt for Streaming Text

**File:** `src/tui/views/chat.rs`

Use StreamingDecrypt for incoming LLM responses:
```rust
// Already has streaming_decrypt field
// Enable matrix effect during streaming
if self.is_streaming && self.matrix_effect_enabled {
    self.streaming_decrypt.tick();
    // Render decrypt effect on latest message
}
```

#### 4.3 BigText for View Titles

**File:** `src/tui/views/home.rs`

Use BigText for "NIKA" title:
```rust
use crate::tui::widgets::{BigText, BigTextGradient, GradientType};

BigText::new("NIKA")
    .gradient(BigTextGradient::new(GradientType::Solarized))
    .render(title_area, frame.buffer_mut());
```

**Tests:**
- `test_matrix_rain_renders`
- `test_streaming_decrypt_reveals_text`
- `test_big_text_gradient`

## Implementation Order

```
1. P1: /invoke inline boxes    [30 min] [TDD]
2. P2: /infer inline boxes     [30 min] [TDD]
3. P3: activity items          [20 min] [TDD]
4. P4.1: matrix rain           [10 min]
5. P4.2: matrix decrypt        [10 min]
6. P4.3: big text              [10 min]
7. Integration test            [15 min]
8. Manual validation           [10 min]
```

## Success Criteria

- [ ] `/invoke tool` shows inline MCP box with params/result
- [ ] `/infer prompt` shows inline stream box with token progress
- [ ] Activity stack updates during command execution
- [ ] Matrix rain visible on Home view after idle
- [ ] BigText "NIKA" title on Home view
- [ ] All tests pass
- [ ] Zero clippy warnings

## Files to Modify

| File | Changes |
|------|---------|
| `src/tui/app.rs` | Wire inline boxes, add StreamChunk variants |
| `src/tui/chat_agent.rs` | Add activity item tracking |
| `src/tui/views/chat.rs` | Enable matrix decrypt |
| `src/tui/views/home.rs` | Add matrix rain, big text |
| `src/tui/state.rs` | Add activity_counter if needed |

## Rollback Plan

All changes are additive. If issues arise:
1. Revert to previous commit
2. Features degrade gracefully (widgets just don't show)
