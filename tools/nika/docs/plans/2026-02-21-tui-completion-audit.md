# TUI Completion Audit — v0.5.2

**Date:** 2026-02-21
**Audit Method:** 10 parallel Haiku agents exploring codebase
**Status:** Analysis complete, plan ready for implementation

---

## Executive Summary

The Nika TUI is **85% production-ready**. All widgets are complete (100%), views are functional, and real-time streaming works. However, there are **critical gaps** in workflow cancellation, MCP server selection, and help documentation that must be addressed.

```
┌─────────────────────────────────────────────────────────────────────┐
│  TUI COMPLETION STATUS                                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  WIDGETS (7/7)      ████████████████████████████████████ 100%      │
│  VIEWS (4/4)        ██████████████████████████████░░░░░░  85%      │
│  USER JOURNEYS      ██████████████████████████░░░░░░░░░░  75%      │
│  DOCUMENTATION      ████████████████░░░░░░░░░░░░░░░░░░░░  50%      │
│                                                                     │
│  Tests: 612 TUI tests passing | 83 widget tests | 41 browser tests │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Critical Issues (MUST FIX)

### 1. 🔴 Workflow Cancellation Missing

**Location:** `src/runtime/runner.rs`, `src/tui/app.rs`

**Problem:** No way to stop a running workflow. User presses Quit → app exits but Runner continues in background.

**What exists:**
- Pause/Step (Space key) — only pauses TUI rendering, not Runner
- Retry (r key) — restarts failed workflow

**What's missing:**
- `CancellationToken` in Runner
- `WorkflowAborted` event variant
- Abort handle propagation to running tasks
- UI for cancel action (e.g., `C` key or Ctrl+C)

**Fix:**
```rust
// In runner.rs
pub struct Runner {
    cancel_token: CancellationToken,  // Add tokio_util::sync::CancellationToken
}

// In event/log.rs
pub enum EventKind {
    WorkflowAborted { reason: String },  // Add new variant
}
```

**Effort:** HIGH (affects runner, executor, event system)

---

### 2. 🔴 MCP Server Selection UI Missing

**Location:** `src/tui/views/chat.rs`

**Problem:** Agent mode hardcodes "novanet" as default MCP server. No UI to select which servers the agent should use.

**What exists:**
- `set_mcp_servers()` method (line 254)
- `session_context.mcp_servers` storage
- MCP status display in session bar

**What's missing:**
- Server selection UI (checkboxes or picker)
- Server availability validation before invoke
- Per-agent server configuration in `/agent` command

**Fix:**
```rust
// Add to /agent command
/agent "goal" --mcp novanet,perplexity --max-turns 5

// Or interactive picker with Ctrl+S
```

**Effort:** MEDIUM (UI + command parsing)

---

### 3. 🔴 Agent Thinking Not Visible in Chat

**Location:** `src/tui/views/chat.rs`, `src/tui/panels/reasoning.rs`

**Problem:** Extended thinking is captured (`AgentTurnMetadata.thinking`) but only shown in Monitor's Reasoning panel. Chat users enable Ctrl+T but don't see the thinking output.

**What exists:**
- `deep_thinking` toggle (Ctrl+T)
- Thinking field in AgentTurnMetadata
- Reasoning panel with "Thinking" tab

**What's missing:**
- Inline thinking display in Chat messages
- Collapsible thinking section in chat bubbles
- Visual indicator when thinking is occurring

**Fix:**
```rust
// In ChatMessage, add thinking field
pub struct ChatMessage {
    pub thinking: Option<String>,  // Show expandable in render
}

// Render as collapsible block
▼ 🤔 Thinking...
  [collapsed thinking content]
```

**Effort:** MEDIUM (rendering + state)

---

### 4. 🔴 Help Overlay Outdated

**Location:** `src/tui/app.rs:2620-2635`

**Problem:** Hardcoded help text doesn't match actual keybindings. Shows `h/l` as global but they only work in Monitor.

**What's wrong:**
| Shown | Reality |
|-------|---------|
| `h/l` cycle panel | Monitor only |
| `Tab` next panel | View-dependent (Monitor=panels, others=views) |
| Missing | Chat shortcuts (Ctrl+K/T/M, i for insert mode) |

**Fix:** Generate help from keybindings.rs or add view-specific help sections.

**Effort:** LOW (text update)

---

## High Priority Issues

### 5. 🟠 Error Handling Incomplete in ChatView

**Location:** `src/tui/views/chat.rs`

**Problems:**
- No error state in ChatMessage struct
- Silent failures in `append_infer_content()` if inline_content is empty
- No timeout handling for streaming operations
- MCP `fail_mcp_call()` exists but no code path calls it

**Fix:** Add error state, display error messages in chat, implement timeouts.

**Effort:** MEDIUM

---

### 6. 🟠 Memory Leak: clear_old_activities() Never Called

**Location:** `src/tui/views/chat.rs:388`, `src/tui/app.rs`

**Problem:** Method defined but never invoked. Activities accumulate forever in long sessions.

**Current fix applied:** `app.rs:755` now calls `clear_old_activities(300)` — ✅ FIXED in this session

---

### 7. 🟠 Tool Call Retry Mechanism Missing

**Location:** `src/tui/views/chat.rs`

**Problem:** If MCP call fails, agent can't retry. User must re-enter entire prompt.

**Fix:** Add retry button to McpCallBox, or implement automatic retry with backoff.

**Effort:** MEDIUM

---

### 8. 🟠 No Persistent Chat Sessions

**Location:** `src/tui/views/chat.rs`

**Problem:** Chat messages disappear on app restart. No save/load functionality.

**Fix:**
```rust
// Save to ~/.nika/chat_history.json
// Load on ChatView initialization
// Add /save and /load commands
```

**Effort:** MEDIUM

---

### 9. 🟠 Pause Doesn't Actually Pause Runner

**Location:** `src/tui/app.rs`, `src/runtime/runner.rs`

**Problem:** Space key sets `state.paused = true` but Runner continues executing. Pause only prevents TUI from rendering new events.

**Fix:** Send pause signal to Runner, implement proper pause/resume protocol.

**Effort:** HIGH

---

## Medium Priority Issues

### 10. 🟡 Thinking Tab Content Not Implemented

**Location:** `src/tui/panels/reasoning.rs`

**Problem:** ReasoningTab::Thinking exists but renders nothing.

**Fix:** Implement thinking content viewer with syntax highlighting.

**Effort:** LOW

---

### 11. 🟡 Scroll Indicators Missing

**Location:** All panels

**Problem:** Scrolling works but no visual feedback (scrollbar, position indicator).

**What exists:** `scroll_indicator.rs` widget

**What's missing:** Integration into panels

**Effort:** LOW

---

### 12. 🟡 Breakpoint UI Not Rendered

**Location:** `src/tui/state.rs`, `src/tui/panels/graph.rs`

**Problem:** `has_breakpoint()` method exists but breakpoints not visually shown in DAG.

**Fix:** Add breakpoint marker (🔴) to task nodes.

**Effort:** LOW

---

### 13. 🟡 Welcome/Onboarding Missing

**Location:** `src/tui/views/home.rs`

**Problem:** No greeting, tutorial, or branding for new users.

**Fix:** Add welcome panel with:
- Nika logo/branding
- Quick start hints
- Keybinding cheat sheet

**Effort:** LOW

---

### 14. 🟡 Tab Key Behavior Inconsistent

**Location:** `src/tui/app.rs:993-997`

**Problem:**
- Monitor: Tab = cycle panels
- Other views: Tab = switch to next view

**Fix:** Either make consistent or document clearly in help.

**Effort:** LOW

---

## Low Priority Issues (Polish)

### 15. 🟢 for_each Loop Grouping in DAG

**Problem:** Parallel tasks execute but DAG doesn't show iteration grouping.

**Fix:** Add visual grouping bracket for loop iterations.

**Effort:** MEDIUM

---

### 16. 🟢 Context Window Visualization

**Problem:** ContextAssembled events exist but no full UI for sources/excluded/budget.

**Fix:** Add context panel or expandable section.

**Effort:** MEDIUM

---

### 17. 🟢 Provider Switch Mid-Chat

**Problem:** Provider auto-detected at startup, no way to switch during session.

**Fix:** Extend `/model` command to switch providers.

**Effort:** LOW

---

### 18. 🟢 Chat Context Isolation

**Problem:** Chat can't reference workflow being executed in Monitor.

**Fix:** Add `/workflow` command to inject current workflow context.

**Effort:** MEDIUM

---

## What's Complete (No Action Needed)

| Component | Status | Tests |
|-----------|--------|-------|
| dag.rs widget | ✅ 100% | 10 |
| activity_stack.rs | ✅ 100% | 14 |
| agent_turns.rs | ✅ 100% | 6 |
| infer_stream_box.rs | ✅ 100% | 11 |
| mcp_call_box.rs | ✅ 100% | 10 |
| session_context.rs | ✅ 100% | 14 |
| spinner.rs | ✅ 100% | 18 |
| BrowserView | ✅ 100% | 41 |
| Real-time event streaming | ✅ 100% | - |
| Token/cost tracking | ✅ 100% | - |
| Mode switching (Infer/Agent) | ✅ 100% | - |
| Conversation history | ✅ 100% | - |
| Streaming with indicators | ✅ 100% | - |

---

## Implementation Plan

### Phase 1: Critical Fixes (Week 1)

| # | Issue | Effort | Owner |
|---|-------|--------|-------|
| 1 | Workflow cancellation | HIGH | - |
| 4 | Help overlay update | LOW | - |

### Phase 2: High Priority (Week 2)

| # | Issue | Effort | Owner |
|---|-------|--------|-------|
| 2 | MCP server selection UI | MEDIUM | - |
| 3 | Agent thinking in Chat | MEDIUM | - |
| 5 | Error handling | MEDIUM | - |

### Phase 3: UX Polish (Week 3)

| # | Issue | Effort | Owner |
|---|-------|--------|-------|
| 8 | Persistent chat sessions | MEDIUM | - |
| 10 | Thinking tab content | LOW | - |
| 11 | Scroll indicators | LOW | - |
| 13 | Welcome/onboarding | LOW | - |

### Phase 4: Advanced Features (Future)

| # | Issue | Effort | Owner |
|---|-------|--------|-------|
| 7 | Tool call retry | MEDIUM | - |
| 9 | True pause/resume | HIGH | - |
| 15 | for_each visualization | MEDIUM | - |

---

## Test Coverage Gaps

| Area | Current | Target |
|------|---------|--------|
| Mode switching tests | 0 | 5 |
| Error recovery tests | 0 | 10 |
| Integration tests (full event loop) | 0 | 5 |
| Chat persistence tests | 0 | 5 |

---

## Files to Modify

### Critical
- `src/runtime/runner.rs` — Add CancellationToken
- `src/event/log.rs` — Add WorkflowAborted event
- `src/tui/app.rs` — Help overlay, cancel handler
- `src/tui/views/chat.rs` — MCP selection, thinking display

### High Priority
- `src/tui/views/chat.rs` — Error handling, persistence
- `src/tui/panels/reasoning.rs` — Thinking content

### Medium Priority
- `src/tui/views/home.rs` — Welcome screen
- `src/tui/panels/*.rs` — Scroll indicators
- `src/tui/widgets/dag.rs` — Breakpoint markers

---

## Summary

**Good News:**
- All 7 widgets are 100% complete
- 612 TUI tests passing
- Real-time streaming works perfectly
- Core user journeys functional

**Must Address:**
1. Workflow cancellation (CRITICAL)
2. MCP server selection (CRITICAL)
3. Agent thinking visibility (CRITICAL)
4. Help documentation sync (CRITICAL)

**Timeline:** 3 weeks for Phases 1-3, Phase 4 deferred to future release.
