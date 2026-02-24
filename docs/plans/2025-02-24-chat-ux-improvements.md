# Chat UX Improvements Implementation Plan

**Version:** v0.9.0
**Date:** 2025-02-24
**Status:** Ready for Implementation
**Total Estimated Time:** ~11 hours (11 features to implement, 4 already done)

---

## Executive Summary

15 TUI features for Nika's Chat view, organized into 3 phases based on implementation effort.

**Discovery Findings:**
- 4 features already implemented (history, word nav, mouse scroll)
- 2 widgets exist but not wired (ScrollIndicator, MentionPopup)
- 9 new features to build

---

## Feature Status Matrix

| # | Feature | Status | Effort | Phase |
|---|---------|--------|--------|-------|
| 1 | ScrollIndicator in conversation | 🔧 Wire | 30min | P1 |
| 2 | Scroll percentage in status | 🆕 New | 20min | P1 |
| 3 | Input history (↑/↓) | ✅ Done | 0min | - |
| 4 | Word navigation (Ctrl+←/→) | ✅ Done | 0min | - |
| 5 | Message timestamps | 🆕 New | 45min | P1 |
| 6 | /export command | 🆕 New | 40min | P1 |
| 7 | Thinking toggle (t/T) | 🆕 New | 60min | P1 |
| 8 | Mouse wheel scroll | ✅ Done | 0min | - |
| 9 | @ mention autocomplete | 🔧 Wire | 90min | P2 |
| 10 | Multi-line input (Shift+Enter) | 🆕 New | 75min | P2 |
| 11 | Verb colors in input | 🆕 New | 45min | P2 |
| 12 | MCP call retry (Ctrl+R) | 🆕 New | 45min | P2 |
| 13 | ActivityStack wiring | 🔧 Wire | 60min | P2 |
| 14 | Conversation search (Ctrl+F) | 🆕 New | 120min | P3 |
| 15 | Smooth/momentum scroll | 🆕 New | 90min | P3 |

---

## Phase 1 - Quick Wins (~3h)

### Feature 1: Wire ScrollIndicator

**Goal:** Display vertical scrollbar on conversation panel right edge.

**Files:**
- `src/tui/views/chat.rs`

**Code:**
```rust
// In render_messages_v2(), after List render:
use crate::tui::widgets::ScrollIndicator;

let scroll_area = Rect {
    x: area.x + area.width - 1,
    y: area.y + 1,
    width: 1,
    height: area.height.saturating_sub(2),
};

ScrollIndicator::new()
    .position(
        self.conversation_scroll.offset,
        self.conversation_scroll.total,
        self.conversation_scroll.visible,
    )
    .thumb_style(Style::default().fg(theme.highlight))
    .track_style(Style::default().fg(theme.border))
    .render(scroll_area, frame.buffer_mut());
```

**Test:** Add messages > viewport, verify scrollbar thumb reflects position.

---

### Feature 2: Scroll Percentage

**Goal:** Show "45%" or "Bot" in hints area.

**Code:**
```rust
fn scroll_percentage(&self) -> u8 {
    let total = self.conversation_scroll.total;
    let visible = self.conversation_scroll.visible;
    let offset = self.conversation_scroll.offset;

    if total <= visible { return 100; }
    let max_offset = total.saturating_sub(visible);
    ((offset as f64 / max_offset as f64) * 100.0).round() as u8
}

// In render_hints():
let pct = self.scroll_percentage();
let indicator = if pct < 100 { format!(" {}%", pct) } else { " Bot".to_string() };
```

---

### Feature 5: Message Timestamps

**Goal:** Show "10:42" next to messages.

**Dependencies:** Add `chrono = "0.4"` to Cargo.toml

**Code:**
```rust
// Change ChatMessage.timestamp from Instant to:
use chrono::{DateTime, Local};
pub timestamp: DateTime<Local>,

// In message creation:
timestamp: Local::now(),

// In render:
let ts = msg.timestamp.format("%H:%M").to_string();
```

---

### Feature 6: /export Command

**Goal:** Export chat to JSON file.

**Files:**
- `src/tui/command.rs` - Add `Export { path: Option<String> }` variant
- `src/tui/views/chat.rs` - Handle command

**Code:**
```rust
// command.rs
"/export" => Command::Export { path: if args.is_empty() { None } else { Some(args.to_string()) } },

// chat.rs
Command::Export { path } => {
    let path = path.unwrap_or_else(|| format!("nika-chat-{}.json", Local::now().format("%Y%m%d-%H%M%S")));
    self.export_session(&path)?;
    self.add_system_message(format!("📤 Exported to {}", path));
}
```

---

### Feature 7: Thinking Toggle

**Goal:** Press `t` to collapse/expand thinking on cursor message, `T` for all.

**Code:**
```rust
// State
thinking_collapsed: HashSet<usize>,
thinking_default_expanded: bool,

// Methods
pub fn toggle_thinking(&mut self, idx: usize) {
    if self.thinking_collapsed.contains(&idx) {
        self.thinking_collapsed.remove(&idx);
    } else {
        self.thinking_collapsed.insert(idx);
    }
}

// Keybinding
KeyCode::Char('t') => { self.toggle_thinking(cursor_msg_idx); }
KeyCode::Char('T') => { self.thinking_default_expanded = !self.thinking_default_expanded; }
```

---

## Phase 2 - Medium Effort (~5h)

### Feature 9: @ Mention Autocomplete

**Goal:** Show popup when typing `@` with file suggestions.

**Files:**
- `src/tui/views/chat.rs`
- Uses existing `src/tui/widgets/mention_system.rs`

**State to add:**
```rust
mention_autocomplete: MentionAutocompleteState,
```

**Integration points:**
1. On `@` char input → call `update_mention_autocomplete()`
2. Render popup above input when `mention_autocomplete.visible`
3. Handle Tab/Enter to accept, Esc to dismiss, ↑/↓ to navigate

---

### Feature 10: Multi-line Input

**Goal:** Shift+Enter inserts newline, Enter submits.

**Code:**
```rust
KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
    self.input.handle(InputRequest::InsertChar('\n'));
}
```

**Render adjustment:** Use `Paragraph` with `Wrap` for multi-line display.

---

### Feature 11: Verb Colors in Input

**Goal:** Color `/infer`, `/exec` prefixes as user types.

**Code:**
```rust
// Use existing detect_verb_in_input()
if let Some((verb, color, complete, rest)) = Self::detect_verb_in_input(input) {
    spans.push(Span::styled(verb, Style::default().fg(color.rgb()).bold()));
    spans.push(Span::raw(rest));
}
```

---

### Feature 12: MCP Retry

**Goal:** Ctrl+R retries last failed MCP call.

**State:**
```rust
last_failed_mcp: Option<FailedMcpCall>,
```

**Keybinding:**
```rust
KeyCode::Char('r') if is_cmd_pressed(key.modifiers) => {
    if let Some(failed) = self.last_failed_mcp.take() {
        return ViewAction::ChatInvoke(failed.tool, failed.server, failed.params);
    }
}
```

---

### Feature 13: ActivityStack Wiring

**Goal:** Show HOT/WARM/QUEUED tasks in Mission Control panel.

**Integration:** Wire `activity_items` population in app.rs event handlers:
```rust
// On TaskStarted
chat_view.activity_items.push(ActivityItem::hot(&task_id, &verb));

// On TaskCompleted
// Move to WARM with duration
```

---

## Phase 3 - Bigger Features (~3.5h)

### Feature 14: Conversation Search

**Goal:** Ctrl+F opens search bar, navigate with Enter/↑/↓.

**State:**
```rust
search_mode: bool,
search_query: String,
search_results: Vec<SearchResult>,
search_current: usize,
```

**UI:** Render search bar at top of messages panel with match count.

---

### Feature 15: Smooth Scrolling

**Goal:** Momentum-based scrolling with friction decay.

**State:**
```rust
scroll_velocity: f32,
scroll_target: Option<usize>,
```

**Physics:**
```rust
const SCROLL_FRICTION: f32 = 0.85;

fn update_scroll_animation(&mut self) {
    if self.scroll_velocity.abs() > 0.5 {
        self.conversation_scroll.offset += self.scroll_velocity as isize;
        self.scroll_velocity *= SCROLL_FRICTION;
    }
}
```

---

## Implementation Order

```
Day 1 (Phase 1):
├── 1. ScrollIndicator wiring (30min)
├── 2. Scroll percentage (20min)
├── 5. Timestamps (45min)
├── 6. /export command (40min)
└── 7. Thinking toggle (60min)
    Total: ~3.25h

Day 2 (Phase 2):
├── 11. Verb colors (45min)
├── 9. @ autocomplete (90min)
├── 10. Multi-line input (75min)
└── 12. MCP retry (45min)
    Total: ~4.25h

Day 3 (Phase 2 + Phase 3):
├── 13. ActivityStack wiring (60min)
├── 14. Conversation search (120min)
└── 15. Smooth scroll (90min)
    Total: ~4.5h
```

---

## Key Files

| File | Features |
|------|----------|
| `src/tui/views/chat.rs` | All 15 features |
| `src/tui/widgets/scroll_indicator.rs` | #1 (wire) |
| `src/tui/widgets/mention_system.rs` | #9 (wire) |
| `src/tui/command.rs` | #6 (/export) |
| `src/tui/app.rs` | #13 (activity wiring) |
| `Cargo.toml` | #5 (chrono dep) |

---

## Testing Checklist

- [ ] Feature 1: Scrollbar visible when messages > viewport
- [ ] Feature 2: Percentage updates on scroll
- [ ] Feature 5: Timestamps show on all messages
- [ ] Feature 6: `/export` creates valid JSON file
- [ ] Feature 7: `t`/`T` toggles thinking sections
- [ ] Feature 9: `@` shows file popup, Tab accepts
- [ ] Feature 10: Shift+Enter adds newline
- [ ] Feature 11: `/infer` colored violet in input
- [ ] Feature 12: Ctrl+R retries failed MCP
- [ ] Feature 13: Tasks show as HOT/WARM in panel
- [ ] Feature 14: Ctrl+F finds text, Enter navigates
- [ ] Feature 15: Mouse wheel has smooth deceleration

---

## Rollback Strategy

Each feature is a separate commit. Use `git revert <commit>` to undo.

Optional feature flags in `.nika/config.toml`:
```toml
[features]
scroll_indicator = true
smooth_scroll = true
mention_autocomplete = true
```
