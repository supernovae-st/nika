# Plan: TUI Observability Gaps — 2026-02-21

## Executive Summary

Fix 4 gaps identified by audit snipers where events are emitted but not displayed in TUI.

**Streaming (P3):** Already implemented! All 6 providers have full streaming support (v0.7 ready).

---

## Priority 1: Critical Fixes

### FIX 1: WorkflowPaused/Resumed Handlers

**Problem:** User presses spacebar to pause → no visual feedback

**File:** `src/tui/state.rs`

**Location:** Line 1665 (wildcard `_ => {}`)

**Implementation:**

```rust
// After WorkflowAborted handler (line 1289), add:

EventKind::WorkflowPaused => {
    self.workflow.paused = true;
    self.workflow.phase = MissionPhase::Pause;
    self.add_notification(Notification::warning(
        "⏸️ Mission paused — press SPACE to resume",
        timestamp_ms,
    ));
    self.dirty.progress = true;
    self.dirty.status = true;
}

EventKind::WorkflowResumed => {
    self.workflow.paused = false;
    self.workflow.phase = MissionPhase::InFlight;
    self.add_notification(Notification::info(
        "▶️ Mission resumed — engines back online!",
        timestamp_ms,
    ));
    self.dirty.progress = true;
    self.dirty.status = true;
}
```

**Also need:** Add `paused: bool` field to `WorkflowState` struct and `MissionPhase::Pause` variant.

**Test:** Press spacebar during workflow → see "⏸️ Mission paused" notification

---

### FIX 2: MCP_CALL_TIMEOUT Implementation

**Problem:** `MCP_CALL_TIMEOUT = 30s` defined but never used

**File:** `src/mcp/rmcp_adapter.rs`

**Location:** `call_tool()` at line 252, `read_resource()` at line 303, `list_tools()` at line 359

**Implementation:**

```rust
// At top of file, add import:
use crate::util::MCP_CALL_TIMEOUT;
use tokio::time::timeout;

// In call_tool() (line 268), wrap service call:
let result = timeout(MCP_CALL_TIMEOUT, service.call_tool(request))
    .await
    .map_err(|_| NikaError::McpTimeout {
        name: self.name.clone(),
        operation: "call_tool".to_string(),
        timeout_secs: MCP_CALL_TIMEOUT.as_secs(),
    })?
    .map_err(|e| { /* existing error handling */ })?;

// Same pattern for read_resource() and list_tools()
```

**Also need:** Add `McpTimeout` error variant to `error.rs`

**Test:** Mock slow MCP server → get timeout error after 30s

---

## Priority 2: Nice to Have

### FIX 3: TemplateResolved Display

**Problem:** Template resolution invisible to users

**File:** `src/tui/state.rs`

**Implementation:**

```rust
// Add field to TuiState:
pub recent_templates: VecDeque<TemplateResolution>,

#[derive(Clone)]
pub struct TemplateResolution {
    pub task_id: String,
    pub template: String,
    pub result: String,
    pub timestamp_ms: u64,
}

// In handle_event():
EventKind::TemplateResolved { task_id, template, result } => {
    // Keep last 10 resolutions
    if self.recent_templates.len() >= 10 {
        self.recent_templates.pop_front();
    }
    self.recent_templates.push_back(TemplateResolution {
        task_id: task_id.to_string(),
        template: template.clone(),
        result: result.clone(),
        timestamp_ms,
    });
    self.dirty.context = true;
}
```

**Display:** Show in Context panel or new "Bindings" tab

---

### FIX 4: ProviderCalled in Monitor View

**Problem:** Only displayed in Chat view, not Monitor view

**File:** `src/tui/state.rs`

**Implementation:**

```rust
// Add to handle_event() (after ProviderResponded handler):
EventKind::ProviderCalled { task_id, provider, model, prompt_len } => {
    // Update current task's provider info
    if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
        task.provider = Some(provider.clone());
        task.model = Some(model.clone());
        task.prompt_len = Some(*prompt_len);
    }

    // Add to metrics
    self.metrics.provider_calls += 1;
    self.metrics.last_model = Some(model.clone());

    self.dirty.progress = true;
}
```

**Display:** Show provider/model in task details panel

---

## Priority 3: Future (Already Done / Deferred)

### DONE: Streaming Support for All Providers

**Status:** ✅ Already implemented in `src/provider/rig.rs`

Lines 456-592 show full streaming support for:
- Claude (with thinking capture)
- OpenAI
- Mistral
- Groq
- DeepSeek
- Ollama

The comment at line 456 says "v0.7: Full streaming support for all providers"

### DEFERRED: Trace Compression

Low priority — current NDJSON format is fine for typical workflows.

---

## Implementation Order

| Step | Fix | Time | Files | Status |
|------|-----|------|-------|--------|
| 1 | MCP_CALL_TIMEOUT | 15 min | rmcp_adapter.rs, error.rs | ✅ DONE |
| 2 | WorkflowPaused/Resumed | 20 min | state.rs, theme.rs | ✅ DONE |
| 3 | TemplateResolved display | 15 min | state.rs | ✅ DONE |
| 4 | ProviderCalled in Monitor | 10 min | state.rs | ✅ DONE |

**Total:** ~1 hour | **Completed:** 2026-02-21

---

## Verification Checklist

- [x] `nika tui workflow.yaml` + spacebar → see pause notification
- [x] Slow MCP server → timeout error after 30s
- [x] Template resolution visible in Context panel
- [x] Provider/model visible in Progress panel for each task
- [x] All tests pass (1816 tests)
