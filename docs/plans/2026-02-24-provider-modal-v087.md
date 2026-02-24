# Provider Modal v0.8.7 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete Provider Modal v2 to 100% functional with live data, ConfigTab, and TUI integration.

**Architecture:** Tabs render live data from ModalLoader/OllamaClient via ProviderModalState. Event handler dispatches keyboard input to appropriate tab actions.

**Tech Stack:** Rust, ratatui, tokio channels (mpsc), reqwest

---

## Task 1: ConfigTab Implementation

**Files:**
- Create: `src/tui/widgets/provider_modal/tabs/config.rs`
- Modify: `src/tui/widgets/provider_modal/tabs/mod.rs`
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Write the failing test**

```rust
// In tabs/config.rs
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_config_tab_renders_settings() {
        let tab = ConfigTab::new(0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Default Provider"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika config_tab -- --nocapture`
Expected: FAIL with "module not found"

**Step 3: Write ConfigTab implementation**

```rust
//! ConfigTab - Configuration preferences

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Configuration entry for display
pub struct ConfigEntry {
    pub label: &'static str,
    pub value: String,
    pub description: &'static str,
}

impl ConfigEntry {
    pub fn new(label: &'static str, value: impl Into<String>, description: &'static str) -> Self {
        Self {
            label,
            value: value.into(),
            description,
        }
    }
}

/// Config tab widget
pub struct ConfigTab {
    entries: Vec<ConfigEntry>,
    selected_idx: usize,
}

impl ConfigTab {
    pub fn new(selected_idx: usize) -> Self {
        let entries = vec![
            ConfigEntry::new("Default Provider", "Claude", "Primary provider for infer: tasks"),
            ConfigEntry::new("Default Model", "claude-sonnet-4-6", "Model for new tasks"),
            ConfigEntry::new("Theme", "Solarized Dark", "TUI color theme"),
            ConfigEntry::new("Auto-save Sessions", "Enabled", "Persist editor state"),
            ConfigEntry::new("MCP Timeout", "30s", "Timeout for MCP operations"),
        ];
        Self { entries, selected_idx }
    }

    /// Get item count for navigation
    pub fn item_count(&self) -> usize {
        self.entries.len()
    }
}

impl Widget for ConfigTab {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }

        let label_style = Style::default().fg(Color::Rgb(156, 163, 175));
        let value_style = Style::default()
            .fg(Color::Rgb(129, 140, 248))
            .add_modifier(Modifier::BOLD);
        let selected_style = Style::default()
            .fg(Color::Rgb(229, 231, 235))
            .bg(Color::Rgb(55, 65, 81));
        let desc_style = Style::default().fg(Color::Rgb(107, 114, 128));

        let mut y = area.y;

        for (idx, entry) in self.entries.iter().enumerate() {
            if y >= area.y + area.height - 1 {
                break;
            }

            let is_selected = idx == self.selected_idx;
            let line_style = if is_selected { selected_style } else { Style::default() };

            // Clear the line if selected
            if is_selected {
                for x in area.x..area.x + area.width {
                    buf.get_mut(x, y).set_style(line_style);
                }
            }

            // Render label
            let label_x = area.x + 2;
            buf.set_string(label_x, y, entry.label, if is_selected { selected_style } else { label_style });

            // Render value (right-aligned within first 40 chars)
            let value_x = area.x + 25;
            buf.set_string(value_x, y, &entry.value, if is_selected { selected_style.add_modifier(Modifier::BOLD) } else { value_style });

            y += 1;

            // Render description on next line
            if y < area.y + area.height {
                let desc = format!("  └─ {}", entry.description);
                buf.set_string(area.x + 2, y, &desc, desc_style);
                y += 1;
            }

            // Add spacing
            y += 1;
        }
    }
}
```

**Step 4: Update tabs/mod.rs**

```rust
mod config;
pub use config::*;
```

**Step 5: Update main mod.rs to use ConfigTab**

Replace the "coming soon" placeholder with ConfigTab rendering.

**Step 6: Run tests**

Run: `cargo test -p nika provider_modal`
Expected: All tests pass

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(tui): implement ConfigTab for Provider Modal v2

- Add ConfigEntry struct for setting display
- ConfigTab with 5 default settings (provider, model, theme, etc.)
- Selection highlighting with keyboard navigation support
- Tests for rendering and item count"
```

---

## Task 2: ProviderModal Event Handler

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`
- Create: `src/tui/widgets/provider_modal/handler.rs`
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Write the failing test**

```rust
// In handler.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_handle_tab_key_switches_tab() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        state.active_tab = ProviderModalTab::Cloud;

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = ModalEventHandler::handle(&mut state, key);

        assert!(result.consumed);
        assert_eq!(state.active_tab, ProviderModalTab::Ollama);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika handle_tab_key -- --nocapture`
Expected: FAIL

**Step 3: Implement ModalEventHandler**

```rust
//! Event handler for provider modal

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use super::state::{ProviderModalState, ProviderModalTab};

/// Result of event handling
#[derive(Debug, Clone)]
pub struct HandleResult {
    /// Whether the event was consumed
    pub consumed: bool,
    /// Action to perform (if any)
    pub action: Option<ModalAction>,
}

impl HandleResult {
    pub fn consumed() -> Self {
        Self { consumed: true, action: None }
    }

    pub fn consumed_with_action(action: ModalAction) -> Self {
        Self { consumed: true, action: Some(action) }
    }

    pub fn ignored() -> Self {
        Self { consumed: false, action: None }
    }
}

/// Actions that require async handling
#[derive(Debug, Clone)]
pub enum ModalAction {
    /// Check provider connection
    CheckProvider { provider: &'static str },
    /// Test API key
    TestApiKey { provider: &'static str },
    /// Pull Ollama model
    PullModel { model: String },
    /// Delete Ollama model
    DeleteModel { model: String },
    /// Refresh Ollama models
    RefreshOllamaModels,
    /// Close modal
    Close,
}

/// Event handler for modal
pub struct ModalEventHandler;

impl ModalEventHandler {
    /// Handle keyboard event
    pub fn handle(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        if !state.visible {
            return HandleResult::ignored();
        }

        // In input mode, handle text input
        if state.key_input_mode {
            return Self::handle_input_mode(state, key);
        }

        match key.code {
            // Close modal
            KeyCode::Esc => {
                state.close();
                HandleResult::consumed_with_action(ModalAction::Close)
            }

            // Tab navigation
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    state.prev_tab();
                } else {
                    state.next_tab();
                }
                HandleResult::consumed()
            }

            // Number key tab switching
            KeyCode::Char(c @ '1'..='4') => {
                if let Some(tab) = ProviderModalTab::from_key(c) {
                    state.switch_tab(tab);
                }
                HandleResult::consumed()
            }

            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                state.navigate_up();
                HandleResult::consumed()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.navigate_down();
                HandleResult::consumed()
            }

            // Tab-specific actions
            KeyCode::Enter => Self::handle_enter(state),
            KeyCode::Char('p') if state.active_tab == ProviderModalTab::Ollama => {
                HandleResult::consumed_with_action(ModalAction::PullModel {
                    model: "llama3.2".to_string(), // Would get from selection
                })
            }
            KeyCode::Char('d') if state.active_tab == ProviderModalTab::Ollama => {
                HandleResult::consumed_with_action(ModalAction::DeleteModel {
                    model: "selected".to_string(), // Would get from selection
                })
            }
            KeyCode::Char('t') if state.active_tab == ProviderModalTab::Keys => {
                HandleResult::consumed_with_action(ModalAction::TestApiKey {
                    provider: "selected", // Would get from selection
                })
            }
            KeyCode::Char('r') => {
                HandleResult::consumed_with_action(ModalAction::RefreshOllamaModels)
            }

            _ => HandleResult::ignored(),
        }
    }

    fn handle_input_mode(state: &mut ProviderModalState, key: KeyEvent) -> HandleResult {
        match key.code {
            KeyCode::Esc => {
                state.key_input_mode = false;
                state.key_input_buffer.clear();
                HandleResult::consumed()
            }
            KeyCode::Enter => {
                // Would save the key here
                state.key_input_mode = false;
                state.key_input_buffer.clear();
                HandleResult::consumed()
            }
            KeyCode::Backspace => {
                state.key_input_buffer.pop();
                HandleResult::consumed()
            }
            KeyCode::Char(c) => {
                state.key_input_buffer.push(c);
                HandleResult::consumed()
            }
            _ => HandleResult::ignored(),
        }
    }

    fn handle_enter(state: &mut ProviderModalState) -> HandleResult {
        match state.active_tab {
            ProviderModalTab::Keys => {
                state.key_input_mode = true;
                HandleResult::consumed()
            }
            ProviderModalTab::Cloud => {
                // Would select as default provider
                HandleResult::consumed()
            }
            _ => HandleResult::consumed(),
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p nika provider_modal`
Expected: All pass

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): add ModalEventHandler for Provider Modal keyboard input

- HandleResult with consumed flag and optional action
- ModalAction enum for async operations (test key, pull model, etc.)
- Tab/Shift+Tab navigation
- Number keys 1-4 for direct tab access
- j/k or arrows for list navigation
- Tab-specific actions (p=pull, d=delete, t=test, r=refresh)"
```

---

## Task 3: Wire ModalLoader to CloudTab (Live Data)

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`
- Modify: `src/tui/widgets/provider_modal/tabs/cloud.rs`
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Add provider_statuses to ProviderModalState**

```rust
// In state.rs, add to ProviderModalState
pub provider_statuses: Vec<(&'static str, ConnectionStatus)>,
```

**Step 2: Update CloudTab to accept statuses**

```rust
impl<'a> CloudTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        // Build cards from provider_statuses
    }
}
```

**Step 3: Test with mock data**

**Step 4: Commit**

---

## Task 4: Wire OllamaClient to OllamaTab (Live Data)

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Add ollama_models to ProviderModalState**

```rust
// In state.rs
pub ollama_models: Vec<OllamaModelInfo>,
```

**Step 2: Update OllamaTab rendering to use state models**

**Step 3: Test**

**Step 4: Commit**

---

## Task 5: Integration - Connect all pieces

**Files:**
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Update ProviderModal::render to pass live data to tabs**

**Step 2: Integration test**

**Step 3: Commit**

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | ConfigTab | ~8 |
| 2 | ModalEventHandler | ~15 |
| 3 | CloudTab live data | ~5 |
| 4 | OllamaTab live data | ~5 |
| 5 | Integration | ~3 |
| **Total** | | **~36 new tests** |

Expected test count after completion: 121 + 36 = **~157 provider_modal tests**
