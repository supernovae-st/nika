# NIKA Studio TUI - 4 Views Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform Nika TUI from 2 views (Browser/Monitor) to 4 views (Chat/Home/Studio/Monitor) with unified navigation, contextual chat overlay, and AI agent capabilities.

**Architecture:** Extend `TuiView` enum to 4 variants. Each view is a separate module implementing a common `View` trait. Chat overlay is a shared component toggleable from any view. The Chat view is a full agent interface that can control all Nika operations.

**Tech Stack:** ratatui 0.30, crossterm 0.29, tui-textarea (new), rig-core 0.31 for agent

---

## Phase 1: Foundation

### Task 1.1: Add tui-textarea dependency

**Files:**
- Modify: `Cargo.toml:67-71`

**Step 1: Add dependency**

```toml
# TUI (feature-gated)
ratatui = { version = "0.30", optional = true }
crossterm = { version = "0.29", optional = true }
tui-textarea = { version = "0.7", optional = true }  # YAML editor
arboard = { version = "3.4", optional = true }
notify = { version = "8", optional = true }
```

**Step 2: Update feature gate**

```toml
tui = ["dep:ratatui", "dep:crossterm", "dep:tui-textarea", "dep:arboard", "dep:notify"]
```

**Step 3: Verify compilation**

Run: `cargo check --features tui`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "feat(tui): add tui-textarea dependency for YAML editor"
```

---

### Task 1.2: Extend TuiView enum to 4 variants

**Files:**
- Modify: `src/tui/views/mod.rs`
- Test: `src/tui/views/mod.rs` (inline tests)

**Step 1: Write failing tests**

Add to `src/tui/views/mod.rs` in `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_tui_view_all_four_variants() {
    let views = TuiView::all();
    assert_eq!(views.len(), 4);
    assert_eq!(views[0], TuiView::Chat);
    assert_eq!(views[1], TuiView::Home);
    assert_eq!(views[2], TuiView::Studio);
    assert_eq!(views[3], TuiView::Monitor);
}

#[test]
fn test_tui_view_next_cycles() {
    assert_eq!(TuiView::Chat.next(), TuiView::Home);
    assert_eq!(TuiView::Home.next(), TuiView::Studio);
    assert_eq!(TuiView::Studio.next(), TuiView::Monitor);
    assert_eq!(TuiView::Monitor.next(), TuiView::Chat);
}

#[test]
fn test_tui_view_prev_cycles() {
    assert_eq!(TuiView::Chat.prev(), TuiView::Monitor);
    assert_eq!(TuiView::Home.prev(), TuiView::Chat);
    assert_eq!(TuiView::Studio.prev(), TuiView::Home);
    assert_eq!(TuiView::Monitor.prev(), TuiView::Studio);
}

#[test]
fn test_tui_view_number() {
    assert_eq!(TuiView::Chat.number(), 1);
    assert_eq!(TuiView::Home.number(), 2);
    assert_eq!(TuiView::Studio.number(), 3);
    assert_eq!(TuiView::Monitor.number(), 4);
}

#[test]
fn test_tui_view_shortcut() {
    assert_eq!(TuiView::Chat.shortcut(), 'a');
    assert_eq!(TuiView::Home.shortcut(), 'h');
    assert_eq!(TuiView::Studio.shortcut(), 's');
    assert_eq!(TuiView::Monitor.shortcut(), 'm');
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --features tui tui_view_ -- --nocapture`
Expected: FAIL - methods don't exist

**Step 3: Implement TuiView enum**

Replace the `TuiView` enum and impl in `src/tui/views/mod.rs`:

```rust
/// Active view in the TUI - 4 views navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Chat agent - command Nika conversationally
    Chat,
    /// Home browser - browse and select workflows
    #[default]
    Home,
    /// Studio editor - edit YAML with validation
    Studio,
    /// Monitor execution - real-time 4-panel display
    Monitor,
}

impl TuiView {
    /// Get all views in order
    pub fn all() -> &'static [TuiView] {
        &[TuiView::Chat, TuiView::Home, TuiView::Studio, TuiView::Monitor]
    }

    /// Get next view (cycling)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Home,
            TuiView::Home => TuiView::Studio,
            TuiView::Studio => TuiView::Monitor,
            TuiView::Monitor => TuiView::Chat,
        }
    }

    /// Get previous view (cycling)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Monitor,
            TuiView::Home => TuiView::Chat,
            TuiView::Studio => TuiView::Home,
            TuiView::Monitor => TuiView::Studio,
        }
    }

    /// Get view number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Chat => 1,
            TuiView::Home => 2,
            TuiView::Studio => 3,
            TuiView::Monitor => 4,
        }
    }

    /// Get keyboard shortcut
    pub fn shortcut(&self) -> char {
        match self {
            TuiView::Chat => 'a',    // [a]gent
            TuiView::Home => 'h',    // [h]ome
            TuiView::Studio => 's',  // [s]tudio
            TuiView::Monitor => 'm', // [m]onitor
        }
    }

    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Chat => "NIKA AGENT",
            TuiView::Home => "NIKA HOME",
            TuiView::Studio => "NIKA STUDIO",
            TuiView::Monitor => "NIKA MONITOR",
        }
    }

    /// Get the icon for the view
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Chat => "◆",
            TuiView::Home => "◆",
            TuiView::Studio => "◆",
            TuiView::Monitor => "◆",
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --features tui tui_view_ -- --nocapture`
Expected: All 5 new tests PASS

**Step 5: Commit**

```bash
git add src/tui/views/mod.rs
git commit -m "feat(tui): extend TuiView to 4 variants (Chat/Home/Studio/Monitor)"
```

---

### Task 1.3: Add ViewAction variants for new views

**Files:**
- Modify: `src/tui/views/mod.rs`

**Step 1: Write failing test**

```rust
#[test]
fn test_view_action_switch_to_all_views() {
    let actions = vec![
        ViewAction::SwitchView(TuiView::Chat),
        ViewAction::SwitchView(TuiView::Home),
        ViewAction::SwitchView(TuiView::Studio),
        ViewAction::SwitchView(TuiView::Monitor),
    ];
    assert_eq!(actions.len(), 4);
}

#[test]
fn test_view_action_open_in_studio() {
    let action = ViewAction::OpenInStudio(std::path::PathBuf::from("test.nika.yaml"));
    match action {
        ViewAction::OpenInStudio(path) => assert_eq!(path.to_str(), Some("test.nika.yaml")),
        _ => panic!("Expected OpenInStudio"),
    }
}

#[test]
fn test_view_action_send_chat_message() {
    let action = ViewAction::SendChatMessage("Hello Nika".to_string());
    match action {
        ViewAction::SendChatMessage(msg) => assert_eq!(msg, "Hello Nika"),
        _ => panic!("Expected SendChatMessage"),
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --features tui view_action_ -- --nocapture`
Expected: FAIL - variants don't exist

**Step 3: Extend ViewAction enum**

```rust
/// Result of handling a key event in a view
#[derive(Debug, Clone)]
pub enum ViewAction {
    /// No action needed
    None,
    /// Quit the TUI
    Quit,
    /// Switch to a different view
    SwitchView(TuiView),
    /// Run a workflow at the given path
    RunWorkflow(std::path::PathBuf),
    /// Open a workflow in Studio for editing
    OpenInStudio(std::path::PathBuf),
    /// Send a message to the chat agent
    SendChatMessage(String),
    /// Toggle chat overlay
    ToggleChatOverlay,
    /// Show an error message
    Error(String),
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --features tui view_action_ -- --nocapture`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add src/tui/views/mod.rs
git commit -m "feat(tui): add ViewAction variants for Studio and Chat"
```

---

### Task 1.4: Create View trait for polymorphic rendering

**Files:**
- Create: `src/tui/views/trait_view.rs`
- Modify: `src/tui/views/mod.rs`

**Step 1: Create trait file with tests**

Create `src/tui/views/trait_view.rs`:

```rust
//! View trait for polymorphic TUI views

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Trait for TUI views
///
/// Each view (Chat, Home, Studio, Monitor) implements this trait
/// for consistent rendering and input handling.
pub trait View {
    /// Render the view to the frame
    fn render(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme);

    /// Handle a key event, returning an action
    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction;

    /// Get the view's status line text (for footer)
    fn status_line(&self, state: &TuiState) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock view for testing
    struct MockView {
        render_called: std::cell::Cell<bool>,
    }

    impl MockView {
        fn new() -> Self {
            Self {
                render_called: std::cell::Cell::new(false),
            }
        }
    }

    impl View for MockView {
        fn render(&self, _frame: &mut Frame, _area: Rect, _state: &TuiState, _theme: &Theme) {
            self.render_called.set(true);
        }

        fn handle_key(&mut self, _key: KeyEvent, _state: &mut TuiState) -> ViewAction {
            ViewAction::None
        }

        fn status_line(&self, _state: &TuiState) -> String {
            "[Test] Mock view".to_string()
        }
    }

    #[test]
    fn test_mock_view_status_line() {
        let view = MockView::new();
        let state = TuiState::default();
        assert_eq!(view.status_line(&state), "[Test] Mock view");
    }
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test --features tui mock_view -- --nocapture`
Expected: PASS

**Step 3: Export from mod.rs**

Add to `src/tui/views/mod.rs`:

```rust
mod trait_view;

pub use trait_view::View;
```

**Step 4: Commit**

```bash
git add src/tui/views/trait_view.rs src/tui/views/mod.rs
git commit -m "feat(tui): add View trait for polymorphic rendering"
```

---

### Task 1.5: Create unified header widget

**Files:**
- Create: `src/tui/widgets/header.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Write failing test**

Create `src/tui/widgets/header.rs`:

```rust
//! Unified header widget for all views
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  ◆ NIKA [VIEW] › [context]              [status]        1 2 3 4    [?] [×] │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Header configuration
pub struct Header<'a> {
    /// Current active view
    pub view: TuiView,
    /// Optional context string (file name, workflow name)
    pub context: Option<&'a str>,
    /// Optional status string
    pub status: Option<&'a str>,
    /// Theme for colors
    pub theme: &'a Theme,
}

impl<'a> Header<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            context: None,
            status: None,
            theme,
        }
    }

    pub fn context(mut self, ctx: &'a str) -> Self {
        self.context = Some(ctx);
        self
    }

    pub fn status(mut self, status: &'a str) -> Self {
        self.status = Some(status);
        self
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build left side: ◆ NIKA [VIEW] › context
        let mut left_spans = vec![
            Span::styled(" ◆ ", Style::default().fg(self.theme.accent)),
            Span::styled(
                self.view.title(),
                Style::default()
                    .fg(self.theme.fg_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if let Some(ctx) = self.context {
            left_spans.push(Span::raw(" › "));
            left_spans.push(Span::styled(ctx, Style::default().fg(self.theme.fg_secondary)));
        }

        // Build right side: status  1 2 3 4  [?] [×]
        let mut right_spans = vec![];

        if let Some(status) = self.status {
            right_spans.push(Span::styled(status, Style::default().fg(self.theme.fg_secondary)));
            right_spans.push(Span::raw("  "));
        }

        // View tabs: 1 2 3 4 (active one highlighted)
        for v in TuiView::all() {
            let num = v.number().to_string();
            if *v == self.view {
                right_spans.push(Span::styled(
                    format!("[{}]", num),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                right_spans.push(Span::styled(
                    format!(" {} ", num),
                    Style::default().fg(self.theme.fg_muted),
                ));
            }
        }

        right_spans.push(Span::raw("  "));
        right_spans.push(Span::styled("[?]", Style::default().fg(self.theme.fg_muted)));
        right_spans.push(Span::raw(" "));
        right_spans.push(Span::styled("[×]", Style::default().fg(self.theme.fg_muted)));
        right_spans.push(Span::raw(" "));

        // Calculate widths
        let left_width: usize = left_spans.iter().map(|s| s.content.len()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.content.len()).sum();
        let padding = area.width.saturating_sub(left_width as u16 + right_width as u16);

        // Combine with padding
        let mut all_spans = left_spans;
        all_spans.push(Span::raw(" ".repeat(padding as usize)));
        all_spans.extend(right_spans);

        let line = Line::from(all_spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.theme.bg_secondary));

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_new() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Home, &theme);
        assert_eq!(header.view, TuiView::Home);
        assert!(header.context.is_none());
        assert!(header.status.is_none());
    }

    #[test]
    fn test_header_with_context() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Studio, &theme).context("workflow.nika.yaml");
        assert_eq!(header.context, Some("workflow.nika.yaml"));
    }

    #[test]
    fn test_header_with_status() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Monitor, &theme).status("▶ Running 2/3");
        assert_eq!(header.status, Some("▶ Running 2/3"));
    }
}
```

**Step 2: Run tests**

Run: `cargo test --features tui header_ -- --nocapture`
Expected: All tests PASS

**Step 3: Export from mod.rs**

Add to `src/tui/widgets/mod.rs`:

```rust
mod header;

pub use header::Header;
```

**Step 4: Commit**

```bash
git add src/tui/widgets/header.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add unified Header widget for all views"
```

---

### Task 1.6: Create unified status bar widget

**Files:**
- Create: `src/tui/widgets/status_bar.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create status bar widget**

Create `src/tui/widgets/status_bar.rs`:

```rust
//! Unified status bar widget showing contextual keybindings
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ [↑↓] Navigate  [Enter] Run  [e] Edit  [c] Chat  [/] Search  [q] Quit       │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Key hint for status bar
#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: &'static str,
    pub action: &'static str,
}

impl KeyHint {
    pub const fn new(key: &'static str, action: &'static str) -> Self {
        Self { key, action }
    }
}

/// Status bar configuration
pub struct StatusBar<'a> {
    /// Current view (determines which hints to show)
    pub view: TuiView,
    /// Optional custom hints (overrides defaults)
    pub hints: Option<Vec<KeyHint>>,
    /// Theme for colors
    pub theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            hints: None,
            theme,
        }
    }

    pub fn hints(mut self, hints: Vec<KeyHint>) -> Self {
        self.hints = Some(hints);
        self
    }

    fn default_hints(&self) -> Vec<KeyHint> {
        match self.view {
            TuiView::Chat => vec![
                KeyHint::new("Enter", "Send"),
                KeyHint::new("↑↓", "History"),
                KeyHint::new("Tab", "Views"),
                KeyHint::new("Ctrl+L", "Clear"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Home => vec![
                KeyHint::new("↑↓", "Navigate"),
                KeyHint::new("Enter", "Run"),
                KeyHint::new("e", "Edit"),
                KeyHint::new("n", "New"),
                KeyHint::new("/", "Search"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Studio => vec![
                KeyHint::new("i", "Insert"),
                KeyHint::new("Esc", "Normal"),
                KeyHint::new("F5", "Run"),
                KeyHint::new("Ctrl+S", "Save"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Back"),
            ],
            TuiView::Monitor => vec![
                KeyHint::new("1-4", "Focus"),
                KeyHint::new("Tab", "Cycle"),
                KeyHint::new("Space", "Pause"),
                KeyHint::new("r", "Restart"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Stop"),
            ],
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let hints = self.hints.unwrap_or_else(|| self.default_hints());

        let mut spans = vec![Span::raw(" ")];

        for (i, hint) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                format!("[{}]", hint.key),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                hint.action,
                Style::default().fg(self.theme.fg_secondary),
            ));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.theme.bg_secondary));

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_default_hints_home() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Home, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "Enter" && h.action == "Run"));
        assert!(hints.iter().any(|h| h.key == "e" && h.action == "Edit"));
    }

    #[test]
    fn test_status_bar_default_hints_studio() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Studio, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "F5" && h.action == "Run"));
        assert!(hints.iter().any(|h| h.key == "Ctrl+S" && h.action == "Save"));
    }

    #[test]
    fn test_status_bar_custom_hints() {
        let theme = Theme::dark();
        let custom = vec![KeyHint::new("x", "Custom")];
        let bar = StatusBar::new(TuiView::Chat, &theme).hints(custom);
        assert!(bar.hints.is_some());
        assert_eq!(bar.hints.unwrap().len(), 1);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --features tui status_bar_ -- --nocapture`
Expected: All tests PASS

**Step 3: Export from mod.rs**

Add to `src/tui/widgets/mod.rs`:

```rust
mod status_bar;

pub use status_bar::{KeyHint, StatusBar};
```

**Step 4: Commit**

```bash
git add src/tui/widgets/status_bar.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add StatusBar widget with contextual keybindings"
```

---

## Phase 2: Home View

### Task 2.1: Create HomeView structure

**Files:**
- Create: `src/tui/views/home.rs`
- Modify: `src/tui/views/mod.rs`

**Step 1: Create HomeView with tests**

Create `src/tui/views/home.rs`:

```rust
//! Home View - Workflow browser with file tree and preview
//!
//! Layout:
//! ```text
//! ┌───────────────────────────────────┬─────────────────────────────────────────────┐
//! │ 📂 FILES                          │ 📄 PREVIEW                                  │
//! │ Tree view of .nika.yaml files     │ YAML syntax highlighted                     │
//! ├───────────────────────────────────┴─────────────────────────────────────────────┤
//! │ 📜 HISTORY: recent workflow runs                                                │
//! └─────────────────────────────────────────────────────────────────────────────────┘
//! ```

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::standalone::{BrowserEntry, HistoryEntry, StandaloneState};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Home view state
pub struct HomeView {
    /// File browser state (from standalone)
    pub standalone: StandaloneState,
    /// List state for file selection
    pub list_state: ListState,
    /// Whether history bar is expanded
    pub history_expanded: bool,
}

impl HomeView {
    pub fn new(root: PathBuf) -> Self {
        let standalone = StandaloneState::new(root);
        let mut list_state = ListState::default();
        if !standalone.entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            standalone,
            list_state,
            history_expanded: false,
        }
    }

    /// Get currently selected entry
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.standalone.entries.get(i))
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected > 0 {
                self.list_state.select(Some(selected - 1));
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.standalone.entries.len().saturating_sub(1) {
                self.list_state.select(Some(selected + 1));
            }
        }
    }

    /// Toggle folder open/closed
    pub fn toggle_folder(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            self.standalone.toggle_folder(selected);
        }
    }
}

impl View for HomeView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: Files (40%) | Preview (60%) above, History bar below
        let history_height = if self.history_expanded { 6 } else { 3 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(history_height),
            ])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        // Files panel
        self.render_files(frame, main_chunks[0], theme);

        // Preview panel
        self.render_preview(frame, main_chunks[1], theme);

        // History bar
        self.render_history(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            KeyCode::Char('q') => ViewAction::Quit,
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                ViewAction::None
            }
            KeyCode::Enter => {
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir {
                        self.toggle_folder();
                        ViewAction::None
                    } else {
                        ViewAction::RunWorkflow(entry.path.clone())
                    }
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Char('e') => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::OpenInStudio(entry.path.clone());
                    }
                }
                ViewAction::None
            }
            KeyCode::Char('h') => {
                self.history_expanded = !self.history_expanded;
                ViewAction::None
            }
            KeyCode::Char('c') => ViewAction::ToggleChatOverlay,
            KeyCode::Char('1') | KeyCode::Char('a') => ViewAction::SwitchView(TuiView::Chat),
            KeyCode::Char('3') | KeyCode::Char('s') => ViewAction::SwitchView(TuiView::Studio),
            KeyCode::Char('4') | KeyCode::Char('m') => ViewAction::SwitchView(TuiView::Monitor),
            KeyCode::Tab => ViewAction::SwitchView(TuiView::Studio),
            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "{} workflows | {} in history",
            self.standalone.entries.iter().filter(|e| !e.is_dir).count(),
            self.standalone.history.len()
        )
    }
}

impl HomeView {
    fn render_files(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .standalone
            .entries
            .iter()
            .map(|entry| {
                let icon = if entry.is_dir {
                    if entry.is_open { "▼ 📁" } else { "▶ 📁" }
                } else {
                    "  📄"
                };
                let indent = "  ".repeat(entry.depth);
                ListItem::new(format!("{}{} {}", indent, icon, entry.name))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 📂 FILES ")
                    .border_style(Style::default().fg(theme.border)),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        frame.render_stateful_widget(list, area, &mut self.list_state.clone());
    }

    fn render_preview(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let content = if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                "Select a workflow file to preview".to_string()
            } else {
                std::fs::read_to_string(&entry.path).unwrap_or_else(|_| "Error reading file".to_string())
            }
        } else {
            "No file selected".to_string()
        };

        // Add line numbers
        let lines: Vec<Line> = content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:4}│ ", i + 1),
                        Style::default().fg(theme.fg_muted),
                    ),
                    Span::raw(line),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 📄 PREVIEW ")
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_history(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<Span> = self
            .standalone
            .history
            .iter()
            .take(if self.history_expanded { 10 } else { 5 })
            .map(|h| {
                let status = if h.success { "✓" } else { "✗" };
                let color = if h.success { theme.success } else { theme.error };
                Span::styled(
                    format!(" {} {} {} ", status, h.workflow_name, h.relative_time()),
                    Style::default().fg(color),
                )
            })
            .collect();

        let toggle_hint = if self.history_expanded { "▲" } else { "▼" };
        let title = format!(" 📜 HISTORY [h] {} ", toggle_hint);

        let content = Line::from(items);
        let paragraph = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_view_new() {
        let view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);
    }

    #[test]
    fn test_home_view_select_navigation() {
        let mut view = HomeView::new(PathBuf::from("."));
        // Add some mock entries
        view.standalone.entries.push(BrowserEntry {
            name: "test1.nika.yaml".to_string(),
            path: PathBuf::from("test1.nika.yaml"),
            is_dir: false,
            is_open: false,
            depth: 0,
        });
        view.standalone.entries.push(BrowserEntry {
            name: "test2.nika.yaml".to_string(),
            path: PathBuf::from("test2.nika.yaml"),
            is_dir: false,
            is_open: false,
            depth: 0,
        });
        view.list_state.select(Some(0));

        view.select_next();
        assert_eq!(view.list_state.selected(), Some(1));

        view.select_prev();
        assert_eq!(view.list_state.selected(), Some(0));
    }

    #[test]
    fn test_home_view_history_toggle() {
        let mut view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);

        view.history_expanded = true;
        assert!(view.history_expanded);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --features tui home_view -- --nocapture`
Expected: All tests PASS

**Step 3: Export from mod.rs**

Add to `src/tui/views/mod.rs`:

```rust
mod home;

pub use home::HomeView;
```

**Step 4: Commit**

```bash
git add src/tui/views/home.rs src/tui/views/mod.rs
git commit -m "feat(tui): add HomeView with file tree and preview"
```

---

## Phase 3: Studio View

### Task 3.1: Create StudioView with tui-textarea

**Files:**
- Create: `src/tui/views/studio.rs`
- Modify: `src/tui/views/mod.rs`

**Step 1: Create StudioView**

Create `src/tui/views/studio.rs`:

```rust
//! Studio View - YAML editor with validation and task DAG
//!
//! Layout:
//! ```text
//! ┌─────────────────────────────────────────────────────┬───────────────────────┐
//! │ 📝 EDITOR                                           │ 📋 STRUCTURE          │
//! │ YAML with line numbers and syntax highlighting      │ Task DAG mini-view    │
//! ├─────────────────────────────────────────────────────┴───────────────────────┤
//! │ ✅ Valid YAML │ ✅ Schema OK │ ⚠️ 1 warning                                  │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_textarea::{Input, TextArea};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Editor mode (vim-like)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub yaml_valid: bool,
    pub schema_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            yaml_valid: true,
            schema_valid: true,
            warnings: vec![],
            errors: vec![],
        }
    }
}

/// Studio view state
pub struct StudioView {
    /// File path being edited
    pub path: Option<PathBuf>,
    /// Text editor
    pub editor: TextArea<'static>,
    /// Editor mode
    pub mode: EditorMode,
    /// Validation result
    pub validation: ValidationResult,
    /// Whether file has unsaved changes
    pub modified: bool,
}

impl StudioView {
    pub fn new() -> Self {
        let mut editor = TextArea::default();
        editor.set_line_number_style(Style::default().fg(Color::DarkGray));
        editor.set_cursor_line_style(Style::default().bg(Color::Rgb(40, 40, 40)));

        Self {
            path: None,
            editor,
            mode: EditorMode::Normal,
            validation: ValidationResult::default(),
            modified: false,
        }
    }

    /// Load a file into the editor
    pub fn load_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.editor = TextArea::from(content.lines());
        self.editor.set_line_number_style(Style::default().fg(Color::DarkGray));
        self.path = Some(path);
        self.modified = false;
        self.validate();
        Ok(())
    }

    /// Save the file
    pub fn save_file(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = &self.path {
            let content = self.editor.lines().join("\n");
            std::fs::write(path, content)?;
            self.modified = false;
        }
        Ok(())
    }

    /// Validate the YAML content
    pub fn validate(&mut self) {
        let content = self.editor.lines().join("\n");

        // Check YAML validity
        match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(_) => {
                self.validation.yaml_valid = true;
                self.validation.errors.clear();
            }
            Err(e) => {
                self.validation.yaml_valid = false;
                self.validation.errors = vec![e.to_string()];
            }
        }

        // TODO: Schema validation with jsonschema crate
        self.validation.schema_valid = self.validation.yaml_valid;
    }

    /// Get current line number (1-indexed)
    pub fn current_line(&self) -> usize {
        self.editor.cursor().0 + 1
    }

    /// Get current column (1-indexed)
    pub fn current_col(&self) -> usize {
        self.editor.cursor().1 + 1
    }
}

impl Default for StudioView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for StudioView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: Editor (70%) | Structure (30%) above, Validation bar below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[0]);

        // Editor panel
        self.render_editor(frame, main_chunks[0], theme);

        // Structure panel
        self.render_structure(frame, main_chunks[1], theme);

        // Validation bar
        self.render_validation(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match self.mode {
            EditorMode::Normal => self.handle_normal_mode(key),
            EditorMode::Insert => self.handle_insert_mode(key),
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let mode = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        let modified = if self.modified { " ●" } else { "" };
        format!(
            "{} | Ln {}, Col {}{}",
            mode,
            self.current_line(),
            self.current_col(),
            modified
        )
    }
}

impl StudioView {
    fn handle_normal_mode(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Char('i') => {
                self.mode = EditorMode::Insert;
                ViewAction::None
            }
            KeyCode::Char('c') => ViewAction::ToggleChatOverlay,
            KeyCode::F(5) => {
                if let Some(path) = &self.path {
                    ViewAction::RunWorkflow(path.clone())
                } else {
                    ViewAction::Error("No file loaded".to_string())
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(e) = self.save_file() {
                    ViewAction::Error(format!("Save failed: {}", e))
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Char('1') | KeyCode::Char('a') => ViewAction::SwitchView(TuiView::Chat),
            KeyCode::Char('2') | KeyCode::Char('h') => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Char('4') | KeyCode::Char('m') => ViewAction::SwitchView(TuiView::Monitor),
            KeyCode::Up | KeyCode::Char('k') => {
                self.editor.input(Input::from(key));
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.editor.input(Input::from(key));
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = EditorMode::Normal;
                ViewAction::None
            }
            _ => {
                self.editor.input(Input::from(key));
                self.modified = true;
                self.validate();
                ViewAction::None
            }
        }
    }

    fn render_editor(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mode_indicator = match self.mode {
            EditorMode::Normal => "",
            EditorMode::Insert => " [INSERT]",
        };
        let title = format!(
            " 📝 EDITOR{} ",
            mode_indicator
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border));

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(self.editor.widget(), inner);
    }

    fn render_structure(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // TODO: Parse YAML and show task DAG
        let content = "Task structure\n(coming soon)";

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 📋 STRUCTURE ")
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_validation(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let yaml_status = if self.validation.yaml_valid {
            Span::styled("✅ Valid YAML", Style::default().fg(theme.success))
        } else {
            Span::styled("❌ Invalid YAML", Style::default().fg(theme.error))
        };

        let schema_status = if self.validation.schema_valid {
            Span::styled("✅ Schema OK", Style::default().fg(theme.success))
        } else {
            Span::styled("❌ Schema Error", Style::default().fg(theme.error))
        };

        let warning_count = self.validation.warnings.len();
        let warning_status = if warning_count > 0 {
            Span::styled(
                format!("⚠️ {} warning(s)", warning_count),
                Style::default().fg(theme.warning),
            )
        } else {
            Span::styled("✅ No warnings", Style::default().fg(theme.success))
        };

        let line = Line::from(vec![
            Span::raw(" "),
            yaml_status,
            Span::raw("  │  "),
            schema_status,
            Span::raw("  │  "),
            warning_status,
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_studio_view_new() {
        let view = StudioView::new();
        assert_eq!(view.mode, EditorMode::Normal);
        assert!(!view.modified);
        assert!(view.path.is_none());
    }

    #[test]
    fn test_studio_view_mode_switch() {
        let mut view = StudioView::new();
        assert_eq!(view.mode, EditorMode::Normal);

        view.mode = EditorMode::Insert;
        assert_eq!(view.mode, EditorMode::Insert);
    }

    #[test]
    fn test_studio_view_validation() {
        let mut view = StudioView::new();

        // Valid YAML
        view.editor = TextArea::from(["key: value"].iter().map(|s| s.to_string()));
        view.validate();
        assert!(view.validation.yaml_valid);

        // Invalid YAML
        view.editor = TextArea::from(["key: [unclosed"].iter().map(|s| s.to_string()));
        view.validate();
        assert!(!view.validation.yaml_valid);
    }

    #[test]
    fn test_studio_view_cursor_position() {
        let view = StudioView::new();
        assert_eq!(view.current_line(), 1);
        assert_eq!(view.current_col(), 1);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --features tui studio_view -- --nocapture`
Expected: All tests PASS

**Step 3: Export from mod.rs**

Add to `src/tui/views/mod.rs`:

```rust
mod studio;

pub use studio::{EditorMode, StudioView, ValidationResult};
```

**Step 4: Commit**

```bash
git add src/tui/views/studio.rs src/tui/views/mod.rs
git commit -m "feat(tui): add StudioView with tui-textarea YAML editor"
```

---

## Phase 4: Chat View

### Task 4.1: Create ChatMessage and ChatState

**Files:**
- Create: `src/tui/views/chat.rs`

**Step 1: Create chat types**

Create `src/tui/views/chat.rs`:

```rust
//! Chat View - AI Agent conversation interface
//!
//! Layout:
//! ```text
//! ┌─────────────────────────────────────────────────────┬───────────────────────┐
//! │ Conversation history                                │ 📊 SESSION            │
//! │ - User messages                                     │ Actions & context     │
//! │ - Nika responses with inline results                │                       │
//! ├─────────────────────────────────────────────────────┴───────────────────────┤
//! │ > Input field                                                               │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Nika,
    System,
}

/// A chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Instant,
    /// Optional inline execution result
    pub execution: Option<ExecutionResult>,
}

/// Inline execution result in chat
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub tasks_completed: usize,
    pub tasks_total: usize,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Session info sidebar
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub workflow_count: usize,
    pub last_run: Option<String>,
    pub recent_actions: Vec<String>,
    pub current_context: Option<String>,
}

/// Chat view state
pub struct ChatView {
    /// Conversation history
    pub messages: Vec<ChatMessage>,
    /// Current input buffer
    pub input: String,
    /// Input cursor position
    pub cursor: usize,
    /// Scroll offset in message list
    pub scroll: usize,
    /// Session info
    pub session: SessionInfo,
    /// Command history (for ↑/↓ navigation)
    pub history: Vec<String>,
    /// History navigation index
    pub history_index: Option<usize>,
}

impl ChatView {
    pub fn new() -> Self {
        Self {
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content: "Welcome to Nika Agent. How can I help you?".to_string(),
                timestamp: Instant::now(),
                execution: None,
            }],
            input: String::new(),
            cursor: 0,
            scroll: 0,
            session: SessionInfo::default(),
            history: vec![],
            history_index: None,
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.clone(),
            timestamp: Instant::now(),
            execution: None,
        });
        self.history.push(content);
        self.history_index = None;
    }

    /// Add a Nika response
    pub fn add_nika_message(&mut self, content: String, execution: Option<ExecutionResult>) {
        self.messages.push(ChatMessage {
            role: MessageRole::Nika,
            content,
            timestamp: Instant::now(),
            execution,
        });
    }

    /// Submit current input
    pub fn submit(&mut self) -> Option<String> {
        if self.input.trim().is_empty() {
            return None;
        }
        let message = self.input.clone();
        self.add_user_message(message.clone());
        self.input.clear();
        self.cursor = 0;
        Some(message)
    }

    /// Navigate history up
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i - 1);
            }
            _ => {}
        }
        if let Some(i) = self.history_index {
            self.input = self.history[i].clone();
            self.cursor = self.input.len();
        }
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(i) if i < self.history.len() - 1 => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
                self.cursor = self.input.len();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    /// Insert character at cursor
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ChatView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: Messages (80%) | Session (20%) above, Input below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(chunks[0]);

        // Messages panel
        self.render_messages(frame, main_chunks[0], theme);

        // Session panel
        self.render_session(frame, main_chunks[1], theme);

        // Input panel
        self.render_input(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            KeyCode::Char('q') if self.input.is_empty() => ViewAction::Quit,
            KeyCode::Enter => {
                if let Some(message) = self.submit() {
                    ViewAction::SendChatMessage(message)
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Up => {
                self.history_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.history_down();
                ViewAction::None
            }
            KeyCode::Left => {
                self.cursor_left();
                ViewAction::None
            }
            KeyCode::Right => {
                self.cursor_right();
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.backspace();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.insert_char(c);
                ViewAction::None
            }
            KeyCode::Tab => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Esc => ViewAction::SwitchView(TuiView::Home),
            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!("{} messages | {} in history", self.messages.len(), self.history.len())
    }
}

impl ChatView {
    fn render_messages(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .messages
            .iter()
            .flat_map(|msg| {
                let (prefix, style) = match msg.role {
                    MessageRole::User => ("You", Style::default().fg(theme.accent)),
                    MessageRole::Nika => ("Nika", Style::default().fg(theme.success)),
                    MessageRole::System => ("System", Style::default().fg(theme.fg_muted)),
                };

                let mut lines = vec![ListItem::new(Line::from(vec![
                    Span::styled(format!("─ {} ", prefix), style.add_modifier(Modifier::BOLD)),
                    Span::raw("─".repeat(20)),
                ]))];

                // Wrap message content
                for line in msg.content.lines() {
                    lines.push(ListItem::new(format!("  {}", line)));
                }

                // Add execution result if present
                if let Some(exec) = &msg.execution {
                    let status_icon = match exec.status {
                        ExecutionStatus::Running => "▶️",
                        ExecutionStatus::Completed => "✅",
                        ExecutionStatus::Failed => "❌",
                    };
                    lines.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!(
                                "╭─ {} {} ({}/{}) ",
                                status_icon,
                                exec.workflow_name,
                                exec.tasks_completed,
                                exec.tasks_total
                            ),
                            Style::default().fg(theme.fg_secondary),
                        ),
                    ])));
                }

                lines.push(ListItem::new("")); // spacing
                lines
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 💬 CONVERSATION ")
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(list, area);
    }

    fn render_session(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Workflows: ", Style::default().fg(theme.fg_muted)),
                Span::raw(self.session.workflow_count.to_string()),
            ]),
            Line::from(""),
            Line::styled("─── Actions ───", Style::default().fg(theme.fg_muted)),
        ];

        for action in &self.session.recent_actions {
            lines.push(Line::from(format!("✓ {}", action)));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 📊 SESSION ")
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Show input with cursor
        let before_cursor = &self.input[..self.cursor];
        let cursor_char = self.input.chars().nth(self.cursor).unwrap_or(' ');
        let after_cursor = if self.cursor < self.input.len() {
            &self.input[self.cursor + 1..]
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::raw(" ▸ "),
            Span::raw(before_cursor),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(theme.accent).fg(Color::Black),
            ),
            Span::raw(after_cursor),
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        );

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_view_new() {
        let view = ChatView::new();
        assert_eq!(view.messages.len(), 1); // Welcome message
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_submit() {
        let mut view = ChatView::new();
        view.input = "Hello Nika".to_string();
        view.cursor = view.input.len();

        let result = view.submit();
        assert_eq!(result, Some("Hello Nika".to_string()));
        assert!(view.input.is_empty());
        assert_eq!(view.messages.len(), 2); // Welcome + user message
    }

    #[test]
    fn test_chat_view_history_navigation() {
        let mut view = ChatView::new();
        view.add_user_message("First".to_string());
        view.add_user_message("Second".to_string());

        view.history_up();
        assert_eq!(view.input, "Second");

        view.history_up();
        assert_eq!(view.input, "First");

        view.history_down();
        assert_eq!(view.input, "Second");
    }

    #[test]
    fn test_chat_view_cursor() {
        let mut view = ChatView::new();
        view.insert_char('H');
        view.insert_char('i');
        assert_eq!(view.input, "Hi");
        assert_eq!(view.cursor, 2);

        view.cursor_left();
        assert_eq!(view.cursor, 1);

        view.insert_char('e');
        assert_eq!(view.input, "Hei");

        view.backspace();
        assert_eq!(view.input, "Hi");
    }
}
```

**Step 2: Run tests**

Run: `cargo test --features tui chat_view -- --nocapture`
Expected: All tests PASS

**Step 3: Export from mod.rs**

Add to `src/tui/views/mod.rs`:

```rust
mod chat;

pub use chat::{ChatMessage, ChatView, ExecutionResult, ExecutionStatus, MessageRole, SessionInfo};
```

**Step 4: Commit**

```bash
git add src/tui/views/chat.rs src/tui/views/mod.rs
git commit -m "feat(tui): add ChatView with conversation and agent interface"
```

---

## Phase 5: Integration

### Task 5.1: Update App to use 4 views

**Files:**
- Modify: `src/tui/app.rs`

This task integrates all views into the main App struct. Due to the complexity, this will be done incrementally with the executing-plans skill.

**Key changes:**
1. Add `current_view: TuiView` field
2. Add view instances: `chat_view`, `home_view`, `studio_view`
3. Update `render()` to dispatch to current view
4. Update `handle_input()` to dispatch to current view
5. Add view switching with [1-4] and shortcuts

---

## Phase 6: Chat Overlay

### Task 6.1: Create ChatOverlay component

**Files:**
- Create: `src/tui/components/chat_overlay.rs`

The chat overlay is a slide-in panel that can be toggled from any view with [c]. It provides contextual assistance based on the current view.

---

## Checklist

- [ ] Phase 1: Foundation
  - [ ] Task 1.1: Add tui-textarea dependency
  - [ ] Task 1.2: Extend TuiView enum
  - [ ] Task 1.3: Add ViewAction variants
  - [ ] Task 1.4: Create View trait
  - [ ] Task 1.5: Create Header widget
  - [ ] Task 1.6: Create StatusBar widget
- [ ] Phase 2: Home View
  - [ ] Task 2.1: Create HomeView
- [ ] Phase 3: Studio View
  - [ ] Task 3.1: Create StudioView
- [ ] Phase 4: Chat View
  - [ ] Task 4.1: Create ChatView
- [ ] Phase 5: Integration
  - [ ] Task 5.1: Update App
- [ ] Phase 6: Chat Overlay
  - [ ] Task 6.1: Create ChatOverlay

---

## Notes

- **Testing:** Each task includes unit tests. Run `cargo test --features tui` after each task.
- **Commits:** Atomic commits per task with conventional commit format.
- **Dependencies:** tui-textarea v0.7 is added in Phase 1.
- **Existing code:** MonitorView already exists and works. HomeView refactors StandaloneState.
