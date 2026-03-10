# TUI Gap Remediation Plan v2

**Date:** 2026-02-21
**Status:** In Progress
**Context:** Post-10-agent analysis

## Gap Analysis Summary

10 explorer agents analyzed the TUI implementation. Results:

| Category | Status | Gaps Found |
|----------|--------|------------|
| Chat View | 85% | scroll methods, /invoke stub |
| Keybindings | 90% | 'c' key conflict |
| DAG Preview | 100% | Fully functional |
| Browser/Home | 70% | RunWorkflow not wired |
| Monitor View | 100% | Fully functional |
| Verb Handlers | 80% | /invoke is stub |
| Studio Editor | 70% | Schema validation missing |
| State Sync | 80% | task_type never updated |

## Critical Gaps (Must Fix)

### 1. `/invoke` Handler - MCP Tool Calls

**File:** `src/tui/app.rs:1828-1874`
**Current:** Shows "Coming in Phase 6" message
**Required:** Actually call MCP tools via existing `self.mcp_clients`

```rust
// Current (STUB)
fn handle_chat_invoke(&mut self, tool: String, server: Option<String>, params: serde_json::Value) {
    self.chat_view.add_nika_message(
        format!("🔧 /invoke {} - Coming in Phase 6", tool), None);
}

// Required implementation:
// 1. Resolve server (use first available if None)
// 2. Get McpClient from self.mcp_clients
// 3. Call client.call_tool(tool, params).await
// 4. Display result in chat
```

### 2. `RunWorkflow` Action - Spawn Execution

**File:** `src/tui/app.rs:1003-1009`
**Current:** TODO comment, no execution
**Required:** Spawn workflow execution when Enter pressed on file

```rust
// Current (STUB)
ViewAction::RunWorkflow(path) => {
    // TODO: Trigger workflow execution
    self.state.status_message = format!("Running: {}", path.display());
}

// Required:
// 1. Load workflow from path
// 2. Create Executor with MCP clients
// 3. Spawn async execution
// 4. Route events to state/chat
```

### 3. `TaskState.task_type` - Verb Info

**File:** `src/tui/state.rs:176`
**Current:** Field exists but never set
**Required:** Extract verb from TaskStarted event

```rust
// In handle_event() TaskStarted branch:
// Parse task_id to determine verb OR
// Add verb field to TaskStarted EventKind
```

## High Priority Gaps

### 4. Keybinding Conflicts - 'c' Key

**Files:**
- `src/tui/views/home.rs:301` - 'c' = ToggleChatOverlay
- `src/tui/keybindings.rs:198-202` - 'c' = Copy in Monitor

**Fix:** Change Monitor copy to Ctrl+c (already global quit, check conflict)
Or use 'y' for yank (vim convention)

## Medium Priority Gaps

### 5. Chat Scroll Methods

**File:** `src/tui/views/chat.rs`
**Current:** No scroll_up/scroll_down methods
**Required:** Add scroll methods for Normal mode j/k navigation

### 6. Studio Schema Validation

**File:** `src/tui/views/studio.rs`
**Current:** Schema validation messages but no actual validation
**Required:** Wire jsonschema crate for real-time YAML validation

## Low Priority (Technical Debt)

### 7. Dead Code Cleanup

- `YamlView` deprecated (use StudioView)
- `v1_render_activity_progress` in chat.rs
- Old monitor.rs layout code

## Implementation Order

```
1. /invoke handler      ← CRITICAL, user-facing
2. RunWorkflow action   ← CRITICAL, core functionality
3. TaskState.task_type  ← CRITICAL, observability
4. Keybinding conflicts ← HIGH, UX
5. Chat scroll          ← MEDIUM, UX
6. Schema validation    ← MEDIUM, DX
7. Dead code cleanup    ← LOW, tech debt
```

## Success Criteria

- [ ] `/invoke perplexity_search` works in Chat
- [ ] Enter on workflow file starts execution
- [ ] DAG panel shows verb icons during execution
- [ ] No keybinding conflicts
- [ ] j/k scrolls chat in Normal mode
- [ ] Studio shows real validation errors
- [ ] No compiler warnings about dead code
