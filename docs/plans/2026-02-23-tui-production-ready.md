# TUI Production-Ready Plan

**Date:** 2026-02-23
**Version:** v0.8.0
**Status:** ✅ COMPLETE - v0.8.0 Released (Studio DX Features Implemented)

## Overview

Make Nika TUI production-ready with stability, UX, and polish improvements.

**Current State:** 55K lines, 847 tests, 257 unwraps
**Target:** Zero panics, graceful degradation, professional UX

---

## Phase 1: Error Handling (P0) - 4h

### 1.1 Audit unwrap() locations

```bash
# Categories of unwraps to fix:
grep -rn "unwrap()" src/tui/ --include="*.rs" | head -50
```

**Strategy by category:**

| Category | Count | Fix |
|----------|-------|-----|
| `Option::unwrap()` | ~100 | `unwrap_or_default()` or `?` |
| `Result::unwrap()` | ~80 | `?` operator or `.ok()` |
| `expect("msg")` | ~50 | Keep if truly invariant, else `?` |
| Lock unwraps | ~20 | `parking_lot` (no poison) |
| Parse unwraps | ~7 | Return `Result` |

### 1.2 Files to fix (priority order)

1. **app.rs** - Main event loop (crashes = bad)
2. **views/chat.rs** - User-facing (visible errors)
3. **views/home.rs** - File operations (IO errors)
4. **views/studio.rs** - Editor (data loss risk)
5. **widgets/*.rs** - Rendering (visual glitches ok)

### 1.3 Pattern replacements

```rust
// BEFORE
let value = map.get("key").unwrap();

// AFTER (option 1: default)
let value = map.get("key").unwrap_or(&default);

// AFTER (option 2: propagate)
let value = map.get("key").ok_or(TuiError::MissingKey)?;

// AFTER (option 3: skip)
let Some(value) = map.get("key") else { return; };
```

### 1.4 New TuiError variants

```rust
pub enum TuiError {
    // Existing...

    // New for graceful handling
    ClipboardUnavailable,
    TerminalTooSmall { min_width: u16, min_height: u16 },
    RenderError(String),
    ConfigParseError(String),
    SessionLoadError(String),
}
```

---

## Phase 2: Graceful Terminal Resize (P0) - 2h

### 2.1 Minimum size detection

```rust
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 15;

fn check_terminal_size(frame: &Frame) -> Result<(), TuiError> {
    let size = frame.area();
    if size.width < MIN_WIDTH || size.height < MIN_HEIGHT {
        return Err(TuiError::TerminalTooSmall {
            min_width: MIN_WIDTH,
            min_height: MIN_HEIGHT,
        });
    }
    Ok(())
}
```

### 2.2 Fallback UI for small terminals

```
┌─────────────────────────────┐
│  ⚠️  Terminal too small     │
│                             │
│  Minimum: 60x15             │
│  Current: 40x10             │
│                             │
│  Please resize your         │
│  terminal window.           │
└─────────────────────────────┘
```

### 2.3 Responsive layouts

| Width | Layout |
|-------|--------|
| < 60 | Error overlay |
| 60-80 | Compact (no Activity panel) |
| 80-120 | Standard (all panels) |
| > 120 | Wide (side-by-side) |

### 2.4 Dynamic panel hiding

```rust
fn layout_for_size(area: Rect) -> Layout {
    if area.width < 80 {
        // Hide Activity panel, expand Conversation
        Layout::compact()
    } else if area.width < 120 {
        // Standard 3-panel layout
        Layout::standard()
    } else {
        // Wide layout with side panels
        Layout::wide()
    }
}
```

---

## Phase 3: Help Overlay (P1) - 3h

### 3.1 Help content structure

```rust
pub struct HelpSection {
    title: &'static str,
    keybindings: Vec<(&'static str, &'static str)>,
}

const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: "Navigation",
        keybindings: &[
            ("Tab / Shift+Tab", "Cycle panels"),
            ("h / j / k / l", "Home / Chat / Studio / Monitor"),
            ("Ctrl+P", "Command palette"),
            ("?", "Toggle help"),
        ],
    },
    HelpSection {
        title: "Chat Panel",
        keybindings: &[
            ("Enter", "Send message"),
            ("↑ / ↓", "History navigation"),
            ("j / k", "Scroll messages"),
            ("y / Y", "Copy message / text only"),
            ("g / G", "Top / Bottom"),
        ],
    },
    // ... more sections
];
```

### 3.2 Help overlay widget

```rust
pub struct HelpOverlay {
    visible: bool,
    scroll: usize,
    search: Option<String>,
}

impl HelpOverlay {
    pub fn toggle(&mut self) { self.visible = !self.visible; }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible { return; }

        // Semi-transparent background
        let overlay = Block::default()
            .style(Style::default().bg(Color::Rgb(0, 0, 0)));

        // Centered help box
        let help_area = centered_rect(80, 80, area);
        // ... render sections
    }
}
```

### 3.3 Keybindings

- `?` or `F1` - Toggle help
- `Escape` - Close help
- `j/k` or `↑/↓` - Scroll help
- `/` - Search keybindings

---

## Phase 4: Config File (P1) - 4h

### 4.1 Config structure

```toml
# .nika/config.toml

[tui]
theme = "dark"  # dark, light, solarized, custom
mouse = true
animations = true

[tui.keybindings]
# Override defaults
send = "Ctrl+Enter"  # default: Enter
copy = "Ctrl+C"      # default: y

[chat]
default_provider = "claude"
default_model = "claude-sonnet-4"
history_size = 100
show_thinking = true

[studio]
auto_save = true
auto_save_interval = 30  # seconds
tab_width = 2
line_numbers = true

[paths]
workflows = "./workflows"
traces = "./.nika/traces"
```

### 4.2 Config loading

```rust
pub struct TuiConfig {
    pub theme: Theme,
    pub mouse: bool,
    pub animations: bool,
    pub keybindings: KeyBindings,
    pub chat: ChatConfig,
    pub studio: StudioConfig,
}

impl TuiConfig {
    pub fn load() -> Result<Self, TuiError> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            toml::from_str(&content).map_err(TuiError::ConfigParseError)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<(), TuiError> {
        let content = toml::to_string_pretty(self)?;
        atomic_write(Self::config_path(), content.as_bytes())?;
        Ok(())
    }
}
```

### 4.3 Config UI (in Settings view)

```
┌─ Settings ──────────────────────────────────────┐
│                                                 │
│  Theme:     [●] Dark  [ ] Light  [ ] Solarized │
│  Mouse:     [✓] Enabled                        │
│  Animations:[✓] Enabled                        │
│                                                 │
│  ─── Chat ───                                   │
│  Provider:  [Claude ▼]                         │
│  Model:     [claude-sonnet-4 ▼]                │
│  History:   [100] messages                     │
│                                                 │
│  ─── Studio ───                                 │
│  Auto-save: [✓] Every [30] seconds             │
│  Tab width: [2]                                │
│                                                 │
│  [Save]  [Reset to Defaults]  [Cancel]         │
└─────────────────────────────────────────────────┘
```

---

## Phase 5: Status Messages (P1) - 2h

### 5.1 Status message types

```rust
pub enum StatusLevel {
    Info,     // Blue - informational
    Success,  // Green - operation completed
    Warning,  // Yellow - non-blocking issue
    Error,    // Red - operation failed
}

pub struct StatusMessage {
    pub level: StatusLevel,
    pub message: String,
    pub timestamp: Instant,
    pub duration: Duration,  // Auto-dismiss after
}
```

### 5.2 Status bar integration

```rust
impl App {
    pub fn show_status(&mut self, level: StatusLevel, msg: impl Into<String>) {
        self.status_message = Some(StatusMessage {
            level,
            message: msg.into(),
            timestamp: Instant::now(),
            duration: Duration::from_secs(3),
        });
    }

    pub fn tick_status(&mut self) {
        if let Some(ref status) = self.status_message {
            if status.timestamp.elapsed() > status.duration {
                self.status_message = None;
            }
        }
    }
}
```

### 5.3 Usage examples

```rust
// Success
app.show_status(Success, "Message copied to clipboard");
app.show_status(Success, "Workflow saved");

// Warning
app.show_status(Warning, "Large file (>1MB) - may be slow");

// Error
app.show_status(Error, "Failed to connect to MCP server");
```

### 5.4 Visual rendering

```
┌─────────────────────────────────────────────────┐
│ [Status bar content...]                         │
│ ✓ Message copied to clipboard          [3s] ──│
└─────────────────────────────────────────────────┘
```

---

## Phase 6: Theme System (P2) - 3h

### 6.1 Theme definition

```rust
pub struct Theme {
    pub name: &'static str,

    // Base colors
    pub bg: Color,
    pub fg: Color,
    pub fg_muted: Color,

    // Accent colors
    pub primary: Color,
    pub secondary: Color,
    pub highlight: Color,

    // Semantic colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // UI elements
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,
}
```

### 6.2 Built-in themes

```rust
pub const DARK_THEME: Theme = Theme {
    name: "dark",
    bg: Color::Rgb(30, 30, 46),      // Catppuccin base
    fg: Color::Rgb(205, 214, 244),   // Catppuccin text
    // ...
};

pub const LIGHT_THEME: Theme = Theme {
    name: "light",
    bg: Color::Rgb(239, 241, 245),   // Catppuccin latte
    fg: Color::Rgb(76, 79, 105),
    // ...
};

pub const SOLARIZED_THEME: Theme = Theme {
    name: "solarized",
    bg: Color::Rgb(0, 43, 54),       // Solarized base03
    fg: Color::Rgb(131, 148, 150),   // Solarized base0
    // ...
};
```

### 6.3 Theme switching

```rust
impl App {
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        // Trigger full redraw
        self.needs_redraw = true;
    }

    pub fn cycle_theme(&mut self) {
        self.theme = match self.theme.name {
            "dark" => LIGHT_THEME,
            "light" => SOLARIZED_THEME,
            _ => DARK_THEME,
        };
    }
}
```

---

## Phase 7: Undo/Redo (P2) - 2h

### 7.1 Edit history

```rust
pub struct EditHistory {
    undos: Vec<EditState>,
    redos: Vec<EditState>,
    max_history: usize,
}

pub struct EditState {
    content: String,
    cursor: usize,
    timestamp: Instant,
}

impl EditHistory {
    pub fn push(&mut self, state: EditState) {
        self.undos.push(state);
        self.redos.clear();  // New edit clears redo stack
        if self.undos.len() > self.max_history {
            self.undos.remove(0);
        }
    }

    pub fn undo(&mut self) -> Option<EditState> {
        let state = self.undos.pop()?;
        self.redos.push(state.clone());
        self.undos.last().cloned()
    }

    pub fn redo(&mut self) -> Option<EditState> {
        let state = self.redos.pop()?;
        self.undos.push(state.clone());
        Some(state)
    }
}
```

### 7.2 Input integration

```rust
impl Input {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (Ctrl, Char('z')) => self.undo(),
            (Ctrl | Shift, Char('z')) | (Ctrl, Char('y')) => self.redo(),
            _ => {
                // Save state before edit
                self.history.push(self.current_state());
                // Apply edit...
            }
        }
    }
}
```

### 7.3 Visual indicator

```
┌─ Input ─────────────────────────── [↶ 5] [↷ 2] ┐
│ Your message here...                            │
└─────────────────────────────────────────────────┘
```

---

## Phase 8: Session Persistence (P2) - 3h

### 8.1 Session file format

```json
// .nika/sessions/chat-2026-02-23-14-30.json
{
  "version": "1.0",
  "created": "2026-02-23T14:30:00Z",
  "updated": "2026-02-23T15:45:00Z",
  "provider": "claude",
  "model": "claude-sonnet-4",
  "messages": [
    {
      "role": "user",
      "content": "Hello",
      "timestamp": "2026-02-23T14:30:00Z"
    },
    {
      "role": "assistant",
      "content": "Hi! How can I help?",
      "timestamp": "2026-02-23T14:30:02Z",
      "thinking": "User greeting, respond warmly",
      "tokens": { "input": 10, "output": 15 }
    }
  ],
  "context": {
    "mcp_servers": ["novanet"],
    "files_mentioned": ["CLAUDE.md"]
  },
  "metrics": {
    "total_tokens": 1250,
    "total_cost": 0.0025,
    "turns": 12
  }
}
```

### 8.2 Auto-save

```rust
impl ChatView {
    pub fn auto_save(&self) -> Result<(), TuiError> {
        if !self.config.auto_save { return Ok(()); }

        let session = Session::from_chat(self);
        let path = format!(
            ".nika/sessions/chat-{}.json",
            chrono::Local::now().format("%Y-%m-%d-%H-%M")
        );
        session.save(&path)?;
        Ok(())
    }
}
```

### 8.3 Session picker UI

```
┌─ Recent Sessions ───────────────────────────────┐
│                                                 │
│  ▸ Today                                        │
│    • chat-2026-02-23-14-30  12 turns  $0.02    │
│    • chat-2026-02-23-10-15   8 turns  $0.01    │
│                                                 │
│  ▸ Yesterday                                    │
│    • chat-2026-02-22-16-45  25 turns  $0.05    │
│                                                 │
│  [Enter] Load  [d] Delete  [n] New  [Esc] Close│
└─────────────────────────────────────────────────┘
```

### 8.4 Session commands

- `Ctrl+S` - Save current session
- `Ctrl+O` - Open session picker
- `Ctrl+N` - New session
- Auto-save every 30 seconds (configurable)

---

## Execution Order

**STATUS: All phases COMPLETE in v0.8.0** ✅

```
Phase 1: Error Handling     ████████████████████ 4h ✅ DONE
Phase 2: Terminal Resize    ██████████           2h ✅ DONE
Phase 3: Help Overlay       ███████████████      3h ✅ DONE
Phase 4: Config File        ████████████████████ 4h ✅ DONE
Phase 5: Status Messages    ██████████           2h ✅ DONE
Phase 6: Theme System       ███████████████      3h ✅ DONE (Solarized added)
Phase 7: Undo/Redo          ██████████           2h ✅ DONE
Phase 8: Session Persist    ███████████████      3h ✅ DONE
                            ─────────────────────
                            Total: 23h ✅ COMPLETE
```

### Delivered Features (v0.8.0)

- ✅ **Edit History** (Phase 7): Undo/Redo with Ctrl+Z/Ctrl+Y, intelligent 500ms coalescing
- ✅ **Session Persistence** (Phase 8): `.nika/sessions/*.json` autosave with cursor/scroll restoration
- ✅ **Solarized Theme** (Phase 6 enhancement): Light/Dark/Solarized unified across TUI + Studio
- ✅ **Config System** (Phase 4): `.nika/config.toml` with TUI, Chat, Studio, Path settings
- ✅ **Help Overlay** (Phase 3): Built-in help accessible via ? or F1
- ✅ **Status Messages** (Phase 5): Visual feedback for all user actions
- ✅ **Terminal Resize Handling** (Phase 2): Graceful UI at 60x15 minimum
- ✅ **Error Handling** (Phase 1): Unwrap audit and graceful degradation patterns

## Success Criteria

- [x] Zero panics from unwrap() in production paths ✅
- [x] Graceful UI at 60x15 minimum terminal size ✅
- [x] Help accessible via ? or F1 ✅
- [x] Config persisted in .nika/config.toml ✅
- [x] Visual feedback for all user actions ✅
- [x] 3 built-in themes (dark, light, solarized) ✅
- [x] Undo/redo with Ctrl+Z/Ctrl+Y ✅
- [x] Sessions auto-saved and loadable ✅

## Testing

Each phase includes:
1. Unit tests for new functions ✅
2. Integration tests for user flows ✅
3. Manual testing checklist ✅

**Test Count:** 1,879 tests passing (up from 806 TUI tests in v0.7.2)

## Summary

**v0.8.0 TUI Production-Ready** - All 8 phases completed and integrated:

### Architecture Changes (Studio DX)
- **Edit History Module** (`src/tui/edit_history.rs`): 19 tests for intelligent coalescing
- **Session Persistence** (`src/tui/session.rs`): 13 tests for auto-save/recovery
- **Config System** (`src/tui/config.rs`): Type-safe TOML with serde
- **Theme System**: ThemeMode enum with Dark/Light/Solarized variants

### Key Features Deployed
1. Edit history with 500ms coalescing to preserve user intent
2. Atomic session persistence with cursor/scroll state
3. Solarized color palette for accessibility
4. Config file at `.nika/config.toml` for persistent preferences
5. Help overlay with searchable keybindings
6. Status message system with auto-dismiss
7. Graceful terminal resize handling
8. Comprehensive error handling with graceful degradation

### Production Readiness
- ✅ 1,879 tests passing with zero failures
- ✅ Zero clippy warnings with strict `-D warnings` flag
- ✅ Full keyboard support for accessibility
- ✅ Mouse support for convenience
- ✅ Theme persistence across sessions
- ✅ Session history with cost tracking
- ✅ Professional UX with visual feedback

**Release Date:** 2026-02-23
**Status:** PRODUCTION READY
