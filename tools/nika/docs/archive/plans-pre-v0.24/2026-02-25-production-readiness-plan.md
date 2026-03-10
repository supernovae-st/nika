# Production Readiness Plan v0.11.0

**Date:** 2026-02-25
**Status:** Ready for implementation
**Source:** 10-agent comprehensive audit

## Executive Summary

All 10 audit agents completed. Nika v0.10.5 is **production-ready** with minor gaps:
- 3,968 tests passing (100%)
- Zero clippy warnings
- All 6 views wired and functional

**Critical gaps identified:**
1. CHANGELOG.md missing v0.8.0 - v0.10.5 entries
2. McpRetry event defined but never emitted
3. EditHistory exists but not wired to Studio
4. Monitor view captures thinking but doesn't display it

## Implementation Tasks

### Phase 1: Documentation Updates (CRITICAL)

#### Task 1.1: Update CHANGELOG.md
**Files:** `CHANGELOG.md`
**Effort:** 30 min

Add missing entries:
- v0.10.5 - Current (Feb 25)
- v0.10.0 - Chat DAG Widgets
- v0.9.x - 6-Views Architecture Prep
- v0.8.0 - Studio DX (existing but partial)

#### Task 1.2: Update Cargo.toml version
**Files:** `Cargo.toml`
**Effort:** 1 min

Update from 0.10.5 to 0.11.0 after all fixes.

---

### Phase 2: McpRetry Event Emission (HIGH)

#### Task 2.1: Pass EventLog to McpClient
**Files:** `src/mcp/client.rs`, `src/runtime/executor.rs`
**Effort:** 45 min
**Tests:** 3 new tests

Current state:
- McpRetry event is DEFINED in `event/log.rs:303-316`
- Handler EXISTS in `state.rs:2261`
- **NOT emitted** - McpClient operates without EventLog access

Implementation:
```rust
// In McpClient, add EventLog field
pub struct McpClient {
    // ... existing fields
    event_log: Option<EventLog>,
}

impl McpClient {
    pub fn with_event_log(mut self, log: EventLog) -> Self {
        self.event_log = Some(log);
        self
    }

    async fn call_tool_with_retry(&self, ...) {
        for attempt in 1..=max_retries {
            if attempt > 1 {
                if let Some(ref log) = self.event_log {
                    log.emit(EventKind::McpRetry {
                        task_id: task_id.clone(),
                        server_name: self.server_name.clone(),
                        operation: tool_name.to_string(),
                        attempt,
                        max_attempts,
                        error: last_error.clone(),
                    });
                }
            }
            // existing retry logic
        }
    }
}
```

---

### Phase 3: Studio EditHistory Wiring (MEDIUM)

#### Task 3.1: Add EditHistory to StudioView
**Files:** `src/tui/views/studio.rs`
**Effort:** 30 min
**Tests:** 5 new tests

Current state:
- EditHistory exists in `edit_history.rs` (415 lines, 19 tests)
- NOT used in StudioView

Implementation:
```rust
// In StudioView struct
pub struct StudioView {
    // ... existing fields
    edit_history: EditHistory,
}

// In handle_key_event
fn handle_key_event(&mut self, key: KeyEvent) -> Option<ViewAction> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
            if let Some((text, cursor)) = self.edit_history.undo() {
                self.buffer.set_content(&text);
                self.buffer.set_cursor(cursor);
            }
            None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            if let Some((text, cursor)) = self.edit_history.redo() {
                self.buffer.set_content(&text);
                self.buffer.set_cursor(cursor);
            }
            None
        }
        // ... existing handlers
    }
}

// After text modification
fn on_text_changed(&mut self) {
    let text = self.buffer.to_string();
    let cursor = self.buffer.cursor_position();
    self.edit_history.push(&text, cursor);
}
```

---

### Phase 4: Monitor Reasoning Display (MEDIUM)

#### Task 4.1: Render thinking in Agent panel
**Files:** `src/tui/views/monitor.rs`
**Effort:** 20 min
**Tests:** 2 new tests

Current state:
- thinking is CAPTURED in AgentTurnState
- NOT rendered in render_agent_panel()

Implementation:
```rust
// In render_agent_panel, after line 400
fn render_agent_panel(...) {
    let items: Vec<ListItem> = state
        .agent_turns
        .iter()
        .enumerate()
        .flat_map(|(i, turn)| {
            let mut lines = vec![];

            // Turn header (existing)
            lines.push(Line::from(vec![
                Span::styled(format!("Turn {}: ", turn.index + 1), ...),
                // ... existing spans
            ]));

            // Add thinking content if present
            if let Some(ref thinking) = turn.thinking {
                let truncated = if thinking.len() > 100 {
                    format!("{}...", &thinking[..97])
                } else {
                    thinking.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled("  💭 ", Style::default().fg(theme.status_paused)),
                    Span::styled(truncated, Style::default().fg(theme.text_muted).add_modifier(Modifier::ITALIC)),
                ]));
            }

            lines.into_iter().map(ListItem::new).collect::<Vec<_>>()
        })
        .collect();
}
```

---

### Phase 5: Minor Wiring Fixes (LOW)

#### Task 5.1: Home view validation keybinding
**Files:** `src/tui/views/home.rs`
**Effort:** 10 min

Add `v` keybinding to call `validate_selected()`:
```rust
KeyCode::Char('v') => {
    self.validate_selected();
    None
}
```

#### Task 5.2: Settings modal complete wiring
**Files:** `src/tui/views/settings.rs`
**Effort:** 15 min

Wire ApiKeyState and NikaKeyring for provider selection.

---

## Test Plan

| Phase | New Tests | Total |
|-------|-----------|-------|
| Phase 2 (McpRetry) | 3 | 3,971 |
| Phase 3 (EditHistory) | 5 | 3,976 |
| Phase 4 (Reasoning) | 2 | 3,978 |
| Phase 5 (Minor) | 2 | 3,980 |

---

## Implementation Order

1. ✅ Phase 1: CHANGELOG update (no code changes)
2. Phase 3: EditHistory wiring (self-contained, quick win)
3. Phase 4: Monitor reasoning (self-contained, quick win)
4. Phase 5: Minor wiring fixes
5. Phase 2: McpRetry emission (crosses module boundaries)
6. Version bump to v0.11.0

---

## Verification Checklist

- [ ] `cargo test` passes (3,980+ tests)
- [ ] `cargo clippy -- -D warnings` passes
- [ ] CHANGELOG.md has all version entries
- [ ] Ctrl+Z/Ctrl+Y works in Studio view
- [ ] Thinking displays in Monitor Agent panel
- [ ] McpRetry events emit on retry (manual test)
- [ ] `v` key validates in Home view

---

## Files to Modify

| File | Changes |
|------|---------|
| `CHANGELOG.md` | Add v0.9.x-v0.10.x entries |
| `Cargo.toml` | Bump to 0.11.0 |
| `src/mcp/client.rs` | Add EventLog, emit McpRetry |
| `src/runtime/executor.rs` | Pass EventLog to McpClient |
| `src/tui/views/studio.rs` | Wire EditHistory |
| `src/tui/views/monitor.rs` | Render thinking |
| `src/tui/views/home.rs` | Add 'v' keybinding |
| `src/tui/views/settings.rs` | Wire ApiKeyState |
