# TUI Input Improvement Plan

**Date:** 2026-02-21
**Status:** Planning
**Author:** Claude + Thibaut

## Problem Statement

Current issues with Nika TUI chat input:
1. **Duplicate messages** - Fixed in commit 4f28011
2. **No copy/paste support** - Can't Ctrl+C/V to clipboard
3. **No word navigation** - Can't Ctrl+Arrow to move by word
4. **Shortcut conflicts** - Ctrl+C closes terminal instead of copying
5. **Manual string handling** - Fragile, no undo/redo

## Research Summary

### Key Libraries Identified

| Library | Purpose | Score |
|---------|---------|-------|
| **tui-input** | Single-line input with cursor/yank | High (80.3) |
| **arboard** | System clipboard (copy/paste) | Recommended |
| **signal-hook** | Proper SIGINT handling | Essential |

### tui-input Features (from Context7)

```rust
// Word navigation
input.handle(InputRequest::GoToNextWord);  // Ctrl+Right
input.handle(InputRequest::GoToPrevWord);  // Ctrl+Left

// Word deletion
input.handle(InputRequest::DeletePrevWord);  // Ctrl+Backspace
input.handle(InputRequest::DeleteNextWord);  // Ctrl+Delete
input.handle(InputRequest::DeleteTillEnd);   // Ctrl+K

// Internal yank buffer
input.handle(InputRequest::Yank);  // Paste from yank buffer
```

### Clipboard Integration (arboard)

```rust
use arboard::Clipboard;

let mut clipboard = Clipboard::new()?;

// Copy to system clipboard (Ctrl+C when text selected)
clipboard.set_text(selected_text)?;

// Paste from system clipboard (Ctrl+V)
let text = clipboard.get_text()?;
```

### Ctrl+C Handling in Raw Mode

In crossterm raw mode, Ctrl+C arrives as a key event:
```rust
if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
    // Handle as copy, NOT as SIGINT
    if has_selection {
        clipboard.set_text(&selected_text)?;
    }
}
```

## Implementation Plan

### Phase 1: Add tui-input Dependency

**Files:** `Cargo.toml`

```toml
[dependencies]
tui-input = "0.10"  # With crossterm backend
arboard = "3.4"     # System clipboard
```

### Phase 2: Replace Manual Input with tui-input

**Files:** `src/tui/views/chat.rs`

Replace current manual cursor/string handling:
```rust
// BEFORE (manual)
pub struct ChatView {
    input: String,
    cursor_position: usize,
}

// AFTER (tui-input)
use tui_input::Input;

pub struct ChatView {
    input: Input,
    clipboard: Option<arboard::Clipboard>,
}
```

### Phase 3: Wire Keyboard Events

**Files:** `src/tui/views/chat.rs`

Map crossterm events to tui-input requests:

| Key Combo | tui-input Request | Action |
|-----------|-------------------|--------|
| Char | `InsertChar(c)` | Insert character |
| Backspace | `DeletePrevChar` | Delete left |
| Delete | `DeleteNextChar` | Delete right |
| Left | `GoToPrevChar` | Move cursor left |
| Right | `GoToNextChar` | Move cursor right |
| Ctrl+Left | `GoToPrevWord` | Move to prev word |
| Ctrl+Right | `GoToNextWord` | Move to next word |
| Ctrl+Backspace | `DeletePrevWord` | Delete prev word |
| Home | `GoToStart` | Go to start |
| End | `GoToEnd` | Go to end |
| Ctrl+K | `DeleteTillEnd` | Kill line |

### Phase 4: System Clipboard Integration

**Files:** `src/tui/views/chat.rs`

```rust
fn handle_key_event(&mut self, key: KeyEvent) -> Option<ViewAction> {
    match (key.code, key.modifiers) {
        // Ctrl+C = Copy (NOT exit!)
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
            if let Some(clipboard) = &mut self.clipboard {
                // For now, copy entire input (no selection yet)
                let _ = clipboard.set_text(self.input.value());
            }
            None
        }
        // Ctrl+V = Paste
        (KeyCode::Char('v'), m) if m.contains(KeyModifiers::CONTROL) => {
            if let Some(clipboard) = &mut self.clipboard {
                if let Ok(text) = clipboard.get_text() {
                    for c in text.chars() {
                        self.input.handle(InputRequest::InsertChar(c));
                    }
                }
            }
            None
        }
        // Other keys → delegate to tui-input
        _ => {
            self.input.handle_event(&Event::Key(key));
            None
        }
    }
}
```

### Phase 5: Graceful SIGINT Handling

**Files:** `src/tui/app.rs`

Ensure terminal restoration on any exit:

```rust
use signal_hook::flag::register;
use std::sync::atomic::{AtomicBool, Ordering};

static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

fn setup_signal_handlers() {
    let _ = register(signal_hook::consts::SIGINT, || {
        SIGINT_RECEIVED.store(true, Ordering::SeqCst);
    });
}

// In event loop
if SIGINT_RECEIVED.load(Ordering::SeqCst) {
    break; // Clean exit, restore terminal
}
```

## Test Plan (TDD)

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_navigation_ctrl_right() {
        let mut input: Input = "hello world".into();
        input.handle(InputRequest::GoToStart);
        input.handle(InputRequest::GoToNextWord);
        assert_eq!(input.cursor(), 6); // At 'w' in world
    }

    #[test]
    fn test_word_deletion_ctrl_backspace() {
        let mut input: Input = "hello world".into();
        input.handle(InputRequest::GoToEnd);
        input.handle(InputRequest::DeletePrevWord);
        assert_eq!(input.value(), "hello ");
    }

    #[test]
    fn test_ctrl_c_copies_not_exits() {
        // Verify Ctrl+C is handled as copy, not SIGINT
        // This is an integration test that needs the full event loop
    }
}
```

### Integration Tests

1. **test_input_word_navigation** - Verify Ctrl+Arrow moves by word
2. **test_input_clipboard_roundtrip** - Verify Ctrl+C then Ctrl+V
3. **test_input_multiline_paste** - Verify pasting multiline text
4. **test_sigint_restores_terminal** - Verify Ctrl+C doesn't break terminal

## Migration Checklist

- [ ] Add `tui-input` and `arboard` to Cargo.toml
- [ ] Replace `String` input with `tui_input::Input` in ChatView
- [ ] Wire crossterm events to `Input::handle_event()`
- [ ] Add clipboard field to ChatView
- [ ] Implement Ctrl+C as copy (not exit)
- [ ] Implement Ctrl+V as paste
- [ ] Add word navigation (Ctrl+Arrow)
- [ ] Add word deletion (Ctrl+Backspace)
- [ ] Add signal-hook for graceful SIGINT
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Code review

## Dependencies to Add

```toml
[dependencies]
tui-input = "0.10"
arboard = "3.4"
signal-hook = "0.3"
```

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| arboard fails on headless | Graceful fallback, log warning |
| tui-input version mismatch | Pin to specific version |
| Signal handling breaks tests | Mock in test mode |

## Success Criteria

1. Can type text with full cursor control
2. Ctrl+C copies text (not exits)
3. Ctrl+V pastes from system clipboard
4. Ctrl+Arrow moves by word
5. Ctrl+Backspace deletes word
6. No terminal state corruption on exit
7. All tests pass
