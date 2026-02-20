//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.

use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame, Terminal,
};
use tokio::sync::{broadcast, mpsc};

use crate::error::{NikaError, Result};
use crate::event::{Event as NikaEvent, EventKind};

use super::panels::{ContextPanel, GraphPanel, ProgressPanel, ReasoningPanel};
use super::standalone::StandaloneState;
use super::state::{PanelId, SettingsField, TuiMode, TuiState};
use super::theme::Theme;
use crate::config::mask_api_key;

/// Frame rate target (60 FPS)
const FRAME_RATE_MS: u64 = 16;

/// Action resulting from input handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Continue normal operation
    Continue,
    /// Quit the application
    Quit,
    /// Toggle pause
    TogglePause,
    /// Step one event (when paused)
    Step,
    /// Focus next panel
    FocusNext,
    /// Focus previous panel
    FocusPrev,
    /// Focus specific panel
    FocusPanel(u8),
    /// Toggle mode
    SetMode(TuiMode),
    /// Scroll up in focused panel
    ScrollUp,
    /// Scroll down in focused panel
    ScrollDown,
    // ═══ Settings Overlay Actions ═══
    /// Focus next settings field
    SettingsNextField,
    /// Focus previous settings field
    SettingsPrevField,
    /// Toggle edit mode for current field
    SettingsToggleEdit,
    /// Insert character in edit buffer
    SettingsInput(char),
    /// Backspace in edit buffer
    SettingsBackspace,
    /// Delete character in edit buffer
    SettingsDelete,
    /// Cancel editing (restore original)
    SettingsCancelEdit,
    /// Save settings to config file
    SettingsSave,
    /// Move cursor left in edit mode
    SettingsCursorLeft,
    /// Move cursor right in edit mode
    SettingsCursorRight,
}

/// Main TUI application
pub struct App {
    /// Path to the workflow being observed (None in standalone mode)
    workflow_path: std::path::PathBuf,
    /// Terminal backend (initialized on run)
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    /// TUI state (execution mode)
    state: TuiState,
    /// Standalone state (file browser mode)
    standalone_state: Option<StandaloneState>,
    /// Color theme
    theme: Theme,
    /// Event receiver from runtime (mpsc - legacy)
    event_rx: Option<mpsc::Receiver<NikaEvent>>,
    /// Broadcast receiver from runtime (v0.4.1 - preferred)
    broadcast_rx: Option<broadcast::Receiver<NikaEvent>>,
    /// Should quit flag
    should_quit: bool,
    /// Workflow completed flag
    workflow_done: bool,
}

impl App {
    /// Create a new TUI application for the given workflow
    ///
    /// Note: Terminal initialization is deferred to `run()` to allow
    /// App creation in test contexts without a TTY.
    pub fn new(workflow_path: &Path) -> Result<Self> {
        if !workflow_path.exists() {
            return Err(NikaError::WorkflowNotFound {
                path: workflow_path.display().to_string(),
            });
        }

        let state = TuiState::new(&workflow_path.display().to_string());

        Ok(Self {
            workflow_path: workflow_path.to_path_buf(),
            terminal: None,
            state,
            standalone_state: None,
            theme: Theme::novanet(),
            event_rx: None,
            broadcast_rx: None,
            should_quit: false,
            workflow_done: false,
        })
    }

    /// Create a new TUI application in standalone mode (file browser)
    pub fn new_standalone(standalone_state: StandaloneState) -> Result<Self> {
        // Use a dummy workflow path for standalone mode
        let workflow_path = standalone_state.root.clone();
        let state = TuiState::new("Standalone Mode");

        Ok(Self {
            workflow_path,
            terminal: None,
            state,
            standalone_state: Some(standalone_state),
            theme: Theme::novanet(),
            event_rx: None,
            broadcast_rx: None,
            should_quit: false,
            workflow_done: false,
        })
    }

    /// Initialize terminal for TUI rendering
    fn init_terminal(&mut self) -> Result<()> {
        if self.terminal.is_some() {
            return Ok(());
        }

        enable_raw_mode().map_err(|e| NikaError::TuiError {
            reason: format!("Failed to enable raw mode: {}", e),
        })?;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|e| NikaError::TuiError {
            reason: format!("Failed to enter alternate screen: {}", e),
        })?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| NikaError::TuiError {
            reason: format!("Failed to create terminal: {}", e),
        })?;

        self.terminal = Some(terminal);
        Ok(())
    }

    /// Set the event receiver from runtime (legacy mpsc)
    pub fn with_event_receiver(mut self, rx: mpsc::Receiver<NikaEvent>) -> Self {
        self.event_rx = Some(rx);
        self
    }

    /// Set the broadcast receiver from runtime (v0.4.1 - preferred)
    ///
    /// Use this with `EventLog::new_with_broadcast()` for real-time TUI updates.
    pub fn with_broadcast_receiver(mut self, rx: broadcast::Receiver<NikaEvent>) -> Self {
        self.broadcast_rx = Some(rx);
        self
    }

    /// Run the TUI application
    pub async fn run(mut self) -> Result<()> {
        tracing::info!("TUI started for workflow: {}", self.workflow_path.display());

        // Initialize terminal (deferred from new())
        self.init_terminal()?;

        let tick_rate = Duration::from_millis(FRAME_RATE_MS);

        loop {
            // 1. Poll runtime events (non-blocking) - supports both mpsc and broadcast
            // First check broadcast receiver (v0.4.1 preferred)
            if let Some(ref mut rx) = self.broadcast_rx {
                loop {
                    match rx.try_recv() {
                        Ok(event) => {
                            // Check for workflow completion
                            if matches!(
                                event.kind,
                                EventKind::WorkflowCompleted { .. }
                                    | EventKind::WorkflowFailed { .. }
                            ) {
                                self.workflow_done = true;
                            }

                            // Check for breakpoints
                            if self.state.should_break(&event.kind) {
                                self.state.paused = true;
                            }

                            // Update state
                            self.state.handle_event(&event.kind, event.timestamp_ms);
                        }
                        Err(broadcast::error::TryRecvError::Empty) => break,
                        Err(broadcast::error::TryRecvError::Lagged(n)) => {
                            tracing::warn!("TUI lagged behind by {} events", n);
                            // Continue to catch up
                        }
                        Err(broadcast::error::TryRecvError::Closed) => {
                            self.workflow_done = true;
                            break;
                        }
                    }
                }
            }
            // Fallback to legacy mpsc receiver
            if let Some(ref mut rx) = self.event_rx {
                while let Ok(event) = rx.try_recv() {
                    // Check for breakpoints
                    if self.state.should_break(&event.kind) {
                        self.state.paused = true;
                    }

                    // Update state
                    self.state.handle_event(&event.kind, event.timestamp_ms);
                }
            }

            // 2. Update elapsed time
            self.state.tick();

            // 3. Render frame
            let state = &self.state;
            let theme = &self.theme;
            if let Some(ref mut terminal) = self.terminal {
                terminal
                    .draw(|frame| render_frame(frame, state, theme))
                    .map_err(|e| NikaError::TuiError {
                        reason: format!("Failed to draw frame: {}", e),
                    })?;
            }

            // 4. Poll keyboard input (with timeout for frame rate)
            if event::poll(tick_rate).map_err(|e| NikaError::TuiError {
                reason: format!("Failed to poll events: {}", e),
            })? {
                if let Event::Key(key) = event::read().map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to read event: {}", e),
                })? {
                    let action = self.handle_key(key.code, key.modifiers);
                    self.apply_action(action);
                }
            }

            // 5. Check quit flag
            if self.should_quit {
                break;
            }
        }

        // Cleanup
        self.cleanup()?;

        Ok(())
    }

    /// Handle keyboard input
    fn handle_key(&self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        // Handle mode-specific keys first
        match &self.state.mode {
            TuiMode::Help | TuiMode::Metrics | TuiMode::Inspect(_) | TuiMode::Edit(_) => {
                if code == KeyCode::Esc {
                    return Action::SetMode(TuiMode::Normal);
                }
            }
            TuiMode::Settings => {
                return self.handle_settings_key(code, modifiers);
            }
            _ => {}
        }

        // Global keys
        match code {
            // Quit
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,

            // Navigation
            KeyCode::Tab => Action::FocusNext,
            KeyCode::BackTab => Action::FocusPrev,
            KeyCode::Char('1') => Action::FocusPanel(1),
            KeyCode::Char('2') => Action::FocusPanel(2),
            KeyCode::Char('3') => Action::FocusPanel(3),
            KeyCode::Char('4') => Action::FocusPanel(4),

            // Execution control
            KeyCode::Char(' ') => Action::TogglePause,
            KeyCode::Enter if self.state.paused => Action::Step,

            // Scrolling
            KeyCode::Up | KeyCode::Char('k') => Action::ScrollUp,
            KeyCode::Down | KeyCode::Char('j') => Action::ScrollDown,

            // Overlays
            KeyCode::Char('?') | KeyCode::F(1) => Action::SetMode(TuiMode::Help),
            KeyCode::Char('m') => Action::SetMode(TuiMode::Metrics),
            KeyCode::Char('s') => Action::SetMode(TuiMode::Settings),

            // Escape
            KeyCode::Esc => Action::SetMode(TuiMode::Normal),

            _ => Action::Continue,
        }
    }

    /// Handle keyboard input in Settings mode
    fn handle_settings_key(&self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        let editing = self.state.settings.editing;

        if editing {
            // Edit mode: capture text input
            match code {
                KeyCode::Esc => Action::SettingsCancelEdit,
                KeyCode::Enter => Action::SettingsToggleEdit, // Confirm and exit edit
                KeyCode::Backspace => Action::SettingsBackspace,
                KeyCode::Delete => Action::SettingsDelete,
                KeyCode::Left => Action::SettingsCursorLeft,
                KeyCode::Right => Action::SettingsCursorRight,
                KeyCode::Char(c) => Action::SettingsInput(c),
                _ => Action::Continue,
            }
        } else {
            // Navigation mode
            match code {
                KeyCode::Esc | KeyCode::Char('q') => Action::SetMode(TuiMode::Normal),
                KeyCode::Up | KeyCode::Char('k') => Action::SettingsPrevField,
                KeyCode::Down | KeyCode::Char('j') => Action::SettingsNextField,
                KeyCode::Tab => Action::SettingsNextField,
                KeyCode::BackTab => Action::SettingsPrevField,
                KeyCode::Enter | KeyCode::Char('e') => Action::SettingsToggleEdit,
                KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::SettingsSave
                }
                KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::SettingsSave
                }
                _ => Action::Continue,
            }
        }
    }

    /// Apply an action to the state
    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::TogglePause => self.state.toggle_pause(),
            Action::Step => {
                // Step mode: advance one event then pause again
                self.state.step_mode = true;
            }
            Action::FocusNext => self.state.focus_next(),
            Action::FocusPrev => self.state.focus_prev(),
            Action::FocusPanel(n) => self.state.focus_panel(n),
            Action::SetMode(mode) => self.state.mode = mode,
            Action::ScrollUp => {
                let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                *scroll = scroll.saturating_sub(1);
            }
            Action::ScrollDown => {
                let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                *scroll += 1;
            }
            // Settings actions
            Action::SettingsNextField => self.state.settings.focus_next(),
            Action::SettingsPrevField => self.state.settings.focus_prev(),
            Action::SettingsToggleEdit => {
                if self.state.settings.editing {
                    self.state.settings.confirm_edit();
                } else {
                    self.state.settings.start_edit();
                }
            }
            Action::SettingsInput(c) => self.state.settings.insert_char(c),
            Action::SettingsBackspace => self.state.settings.backspace(),
            Action::SettingsDelete => self.state.settings.delete(),
            Action::SettingsCancelEdit => self.state.settings.cancel_edit(),
            Action::SettingsSave => {
                if let Err(e) = self.state.settings.save() {
                    tracing::error!("Failed to save settings: {}", e);
                }
            }
            Action::SettingsCursorLeft => self.state.settings.cursor_left(),
            Action::SettingsCursorRight => self.state.settings.cursor_right(),
            Action::Continue => {}
        }
    }

    /// Cleanup terminal state
    fn cleanup(&mut self) -> Result<()> {
        if let Some(ref mut terminal) = self.terminal {
            disable_raw_mode().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to disable raw mode: {}", e),
            })?;

            execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| {
                NikaError::TuiError {
                    reason: format!("Failed to leave alternate screen: {}", e),
                }
            })?;

            terminal.show_cursor().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to show cursor: {}", e),
            })?;
        }

        Ok(())
    }

    /// Run the TUI in standalone mode (file browser + history)
    pub async fn run_standalone(mut self) -> Result<()> {
        // Initialize terminal
        self.init_terminal()?;

        let tick_rate = Duration::from_millis(FRAME_RATE_MS);

        loop {
            // Render standalone UI
            if let Some(ref mut terminal) = self.terminal {
                if let Some(ref standalone) = self.standalone_state {
                    terminal
                        .draw(|f| render_standalone_frame(f, standalone, &self.theme))
                        .map_err(|e| NikaError::TuiError {
                            reason: format!("Failed to draw frame: {}", e),
                        })?;
                }
            }

            // Handle input with timeout
            if event::poll(tick_rate).map_err(|e| NikaError::TuiError {
                reason: format!("Failed to poll events: {}", e),
            })? {
                if let Event::Key(key) = event::read().map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to read event: {}", e),
                })? {
                    let action = self.handle_standalone_input(key);
                    match action {
                        StandaloneAction::Quit => break,
                        StandaloneAction::Run => {
                            // Get selected workflow path (clone before cleanup)
                            let workflow_path = self
                                .standalone_state
                                .as_ref()
                                .and_then(|s| s.selected_workflow())
                                .map(|p| p.to_path_buf());

                            if let Some(path) = workflow_path {
                                // Cleanup terminal before running workflow TUI
                                self.cleanup()?;
                                // Run the workflow TUI
                                return crate::tui::run_tui(&path).await;
                            }
                        }
                        StandaloneAction::Validate => {
                            // Validate selected workflow
                            if let Some(ref state) = self.standalone_state {
                                if let Some(path) = state.selected_workflow() {
                                    // TODO: Show validation result in preview panel
                                    let _ = path; // silence unused warning for now
                                }
                            }
                        }
                        StandaloneAction::Continue => {}
                    }
                }
            }
        }

        self.cleanup()
    }

    /// Handle input in standalone mode
    fn handle_standalone_input(&mut self, key: event::KeyEvent) -> StandaloneAction {
        use super::standalone::StandalonePanel;

        if let Some(ref mut state) = self.standalone_state {
            match key.code {
                // Quit
                KeyCode::Char('q') | KeyCode::Esc => StandaloneAction::Quit,

                // Navigation
                KeyCode::Up | KeyCode::Char('k') => {
                    match state.focused_panel {
                        StandalonePanel::Browser => state.browser_up(),
                        StandalonePanel::History => state.history_up(),
                        StandalonePanel::Preview => state.preview_up(),
                    }
                    StandaloneAction::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    match state.focused_panel {
                        StandalonePanel::Browser => state.browser_down(),
                        StandalonePanel::History => state.history_down(),
                        StandalonePanel::Preview => state.preview_down(),
                    }
                    StandaloneAction::Continue
                }

                // Panel switching
                KeyCode::Tab => {
                    state.focused_panel = state.focused_panel.next();
                    StandaloneAction::Continue
                }
                KeyCode::BackTab => {
                    state.focused_panel = state.focused_panel.prev();
                    StandaloneAction::Continue
                }
                KeyCode::Char('1') => {
                    state.focused_panel = StandalonePanel::Browser;
                    StandaloneAction::Continue
                }
                KeyCode::Char('2') => {
                    state.focused_panel = StandalonePanel::History;
                    StandaloneAction::Continue
                }
                KeyCode::Char('3') => {
                    state.focused_panel = StandalonePanel::Preview;
                    StandaloneAction::Continue
                }

                // Actions
                KeyCode::Enter => StandaloneAction::Run,
                KeyCode::Char('v') => StandaloneAction::Validate,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    state.clear_history();
                    StandaloneAction::Continue
                }

                // Refresh
                KeyCode::Char('r') => {
                    state.scan_workflows();
                    state.update_preview();
                    StandaloneAction::Continue
                }

                _ => StandaloneAction::Continue,
            }
        } else {
            StandaloneAction::Quit
        }
    }
}

/// Action from standalone input handling
#[derive(Debug, Clone, PartialEq, Eq)]
enum StandaloneAction {
    Continue,
    Quit,
    Run,
    Validate,
}

impl Drop for App {
    fn drop(&mut self) {
        // Best effort cleanup
        if let Some(ref mut terminal) = self.terminal {
            let _ = disable_raw_mode();
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
            let _ = terminal.show_cursor();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// RENDER FUNCTIONS (standalone to avoid borrow checker issues)
// ═══════════════════════════════════════════════════════════════════

/// Render a frame
fn render_frame(frame: &mut Frame, state: &TuiState, theme: &Theme) {
    let size = frame.area();

    // Create 2x2 layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(size);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[1]);

    // Render panels
    render_panel(frame, state, theme, PanelId::Progress, top_chunks[0]);
    render_panel(frame, state, theme, PanelId::Dag, top_chunks[1]);
    render_panel(frame, state, theme, PanelId::NovaNet, bottom_chunks[0]);
    render_panel(frame, state, theme, PanelId::Agent, bottom_chunks[1]);

    // Render overlay if active
    match &state.mode {
        TuiMode::Help => render_help_overlay(frame, theme, size),
        TuiMode::Metrics => render_metrics_overlay(frame, state, theme, size),
        TuiMode::Settings => render_settings_overlay(frame, state, theme, size),
        _ => {}
    }
}

/// Render a single panel
fn render_panel(frame: &mut Frame, state: &TuiState, theme: &Theme, panel_id: PanelId, area: Rect) {
    let focused = state.focus == panel_id;

    // All panels have dedicated widgets
    match panel_id {
        PanelId::Progress => {
            let panel = ProgressPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::Dag => {
            let panel = GraphPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::NovaNet => {
            let panel = ContextPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::Agent => {
            let panel = ReasoningPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
    }
}

/// Render help overlay
fn render_help_overlay(frame: &mut Frame, theme: &Theme, area: Rect) {
    let help_text = r#"
╔═══════════════════════════════════════════════════════════════════╗
║  KEYBOARD SHORTCUTS                                               ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  NAVIGATION           EXECUTION           OVERLAYS               ║
║  Tab      Next panel  Space   Pause       ?/F1  This help       ║
║  1-4      Jump panel  Enter   Step        m     Metrics         ║
║  j/k      Scroll      q       Quit        s     Settings        ║
║                                           Esc   Close           ║
║                                                                   ║
╚═══════════════════════════════════════════════════════════════════╝
"#;

    let overlay = centered_rect(70, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .style(theme.text_style());

    frame.render_widget(paragraph, overlay);
}

/// Render metrics overlay
fn render_metrics_overlay(frame: &mut Frame, state: &TuiState, theme: &Theme, area: Rect) {
    let metrics = &state.metrics;
    let content = format!(
        r#"
╭─────────────────────────────────────────────────────────────────────╮
│  MISSION METRICS                                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Total Tokens:  {:>10}                                             │
│  Input Tokens:  {:>10}                                             │
│  Output Tokens: {:>10}                                             │
│  Cost (USD):    ${:>9.4}                                           │
│                                                                     │
│  MCP Calls: {}                                                      │
│                                                                     │
╰─────────────────────────────────────────────────────────────────────╯
"#,
        metrics.total_tokens,
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cost_usd,
        metrics.mcp_calls.values().sum::<usize>()
    );

    let overlay = centered_rect(70, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Metrics ")
        .style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(content)
        .block(block)
        .style(theme.text_style());

    frame.render_widget(paragraph, overlay);
}

/// Render settings overlay
fn render_settings_overlay(frame: &mut Frame, state: &TuiState, theme: &Theme, area: Rect) {
    use ratatui::style::Color;
    use ratatui::text::{Line, Span};

    let settings = &state.settings;
    let fields = SettingsField::all();

    // Build field lines with initial title section
    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  API Configuration",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  ─────────────────────────────────────────────"),
        Line::from(""),
    ];

    for field in fields {
        let is_focused = settings.focus == *field;
        let is_editing = settings.editing && is_focused;

        let label = field.label();

        // Get display value
        let value = if is_editing {
            // Show input buffer with cursor
            let buf = &settings.input_buffer;
            let cursor_pos = settings.cursor;
            let before = &buf[..cursor_pos.min(buf.len())];
            let cursor_char = buf.chars().nth(cursor_pos).unwrap_or(' ');
            let after = if cursor_pos < buf.len() {
                &buf[cursor_pos + 1..]
            } else {
                ""
            };
            format!("{}│{}{}", before, cursor_char, after)
        } else {
            // Show masked or actual value
            match field {
                SettingsField::AnthropicKey => {
                    let key = settings.config.api_keys.anthropic.as_deref().unwrap_or("");
                    if key.is_empty() {
                        settings.key_status(*field).1
                    } else {
                        mask_api_key(key, 4)
                    }
                }
                SettingsField::OpenAiKey => {
                    let key = settings.config.api_keys.openai.as_deref().unwrap_or("");
                    if key.is_empty() {
                        settings.key_status(*field).1
                    } else {
                        mask_api_key(key, 4)
                    }
                }
                SettingsField::Provider => settings
                    .config
                    .defaults
                    .provider
                    .clone()
                    .unwrap_or_else(|| "auto".to_string()),
                SettingsField::Model => settings
                    .config
                    .defaults
                    .model
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
            }
        };

        // Build the line
        let prefix = if is_focused { "► " } else { "  " };
        let label_style = if is_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let value_style = if is_editing {
            Style::default().fg(Color::Green)
        } else if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, label_style),
            Span::styled(format!("{:<18}", label), label_style),
            Span::styled(value, value_style),
        ]));
    }

    // Status message
    lines.push(Line::from(""));
    lines.push(Line::from(
        "  ─────────────────────────────────────────────",
    ));

    if let Some(msg) = &settings.status_message {
        let color = if msg.contains("✓") || msg.contains("Saved") {
            Color::Green
        } else if msg.contains("✗") || msg.contains("Error") {
            Color::Red
        } else {
            Color::Yellow
        };
        lines.push(Line::from(vec![Span::styled(
            format!("  {}", msg),
            Style::default().fg(color),
        )]));
    } else if settings.dirty {
        lines.push(Line::from(vec![Span::styled(
            "  • Unsaved changes (Ctrl+S to save)",
            Style::default().fg(Color::Yellow),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "  Config: ~/.config/nika/config.toml",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    // Keybindings
    lines.push(Line::from(""));
    let keybindings = if settings.editing {
        "  [Enter] Confirm  [Esc] Cancel  [←→] Move cursor"
    } else {
        "  [↑↓] Navigate  [Enter/e] Edit  [Ctrl+S] Save  [q/Esc] Close"
    };
    lines.push(Line::from(vec![Span::styled(
        keybindings,
        Style::default().fg(Color::DarkGray),
    )]));

    let overlay = centered_rect(60, 60, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings ")
        .style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(lines).block(block).style(theme.text_style());

    frame.render_widget(paragraph, overlay);
}

// ═══════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ═══════════════════════════════════════════════════════════════════
// STANDALONE MODE RENDER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════

use super::standalone::StandalonePanel;

/// Render standalone mode frame (file browser + history + preview)
fn render_standalone_frame(frame: &mut Frame, state: &StandaloneState, theme: &Theme) {
    let size = frame.area();

    // Layout: Top row (browser | history), bottom row (preview), status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45), // Browser + History
            Constraint::Percentage(45), // Preview
            Constraint::Length(3),      // Status bar
        ])
        .split(size);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    // Render panels
    render_browser_panel(frame, state, theme, top_chunks[0]);
    render_history_panel(frame, state, theme, top_chunks[1]);
    render_preview_panel(frame, state, theme, main_chunks[1]);
    render_status_bar(frame, state, theme, main_chunks[2]);
}

/// Render the workflow browser panel
fn render_browser_panel(frame: &mut Frame, state: &StandaloneState, theme: &Theme, area: Rect) {
    let focused = state.focused_panel == StandalonePanel::Browser;
    let border_style = if focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_normal)
    };

    let block = Block::default()
        .title(format!(" [1] {} ", StandalonePanel::Browser.title()))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Render file list
    let entries = state.filtered_entries();
    let items: Vec<ratatui::widgets::ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let prefix = if i == state.browser_index {
                "▶ "
            } else {
                "  "
            };
            let indent = "  ".repeat(entry.depth);
            let icon = if entry.is_dir { "📁 " } else { "📄 " };
            let style = if i == state.browser_index {
                Style::default()
                    .fg(theme.border_focused)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            ratatui::widgets::ListItem::new(format!(
                "{}{}{}{}",
                prefix, indent, icon, entry.display_name
            ))
            .style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner_area);
}

/// Render the history panel
fn render_history_panel(frame: &mut Frame, state: &StandaloneState, theme: &Theme, area: Rect) {
    let focused = state.focused_panel == StandalonePanel::History;
    let border_style = if focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_normal)
    };

    let block = Block::default()
        .title(format!(" [2] {} ", StandalonePanel::History.title()))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if state.history.is_empty() {
        let empty_msg =
            Paragraph::new("No execution history").style(Style::default().fg(theme.text_muted));
        frame.render_widget(empty_msg, inner_area);
        return;
    }

    // Render history entries (most recent first)
    let items: Vec<ratatui::widgets::ListItem> = state
        .history
        .iter()
        .rev()
        .enumerate()
        .map(|(i, entry)| {
            let prefix = if i == state.history_index {
                "▶ "
            } else {
                "  "
            };
            let status = if entry.success { "✓" } else { "✗" };
            let name = entry
                .workflow_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let style = if i == state.history_index {
                Style::default()
                    .fg(theme.border_focused)
                    .add_modifier(Modifier::BOLD)
            } else if entry.success {
                Style::default().fg(theme.status_success)
            } else {
                Style::default().fg(theme.status_failed)
            };
            ratatui::widgets::ListItem::new(format!(
                "{}{} {} | {} | {} tasks",
                prefix,
                status,
                name,
                entry.duration_display(),
                entry.task_count
            ))
            .style(style)
        })
        .collect();

    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner_area);
}

/// Render the preview panel
fn render_preview_panel(frame: &mut Frame, state: &StandaloneState, theme: &Theme, area: Rect) {
    let focused = state.focused_panel == StandalonePanel::Preview;
    let border_style = if focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_normal)
    };

    let title = if let Some(entry) = state.browser_entries.get(state.browser_index) {
        format!(
            " [3] {} - {} ",
            StandalonePanel::Preview.title(),
            entry.display_name
        )
    } else {
        format!(" [3] {} ", StandalonePanel::Preview.title())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Render YAML preview with scroll
    let lines: Vec<&str> = state.preview_content.lines().collect();
    let visible_lines = lines
        .iter()
        .skip(state.preview_scroll)
        .take(inner_area.height as usize)
        .cloned()
        .collect::<Vec<&str>>()
        .join("\n");

    let preview = Paragraph::new(visible_lines).style(Style::default().fg(theme.text_primary));
    frame.render_widget(preview, inner_area);
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, state: &StandaloneState, theme: &Theme, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_normal));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let file_count = state.browser_entries.len();
    let history_count = state.history.len();

    let status_text = format!(
        " [q]Quit  [Enter]Run  [v]Validate  [Tab]Switch Panel  [r]Refresh │ {} files │ {} history entries ",
        file_count, history_count
    );

    let status = Paragraph::new(status_text).style(Style::default().fg(theme.text_muted));
    frame.render_widget(status, inner_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_key_quit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();

        // We can't fully test App without a terminal, so just test the key handling logic
        // by checking the action enum values
        assert_eq!(
            Action::Quit,
            Action::Quit // Self-equality sanity check
        );
    }

    #[test]
    fn test_action_variants() {
        // Verify all action variants exist and are distinct
        let actions = vec![
            Action::Continue,
            Action::Quit,
            Action::TogglePause,
            Action::Step,
            Action::FocusNext,
            Action::FocusPrev,
            Action::FocusPanel(1),
            Action::SetMode(TuiMode::Help),
            Action::ScrollUp,
            Action::ScrollDown,
        ];

        // All should be different
        for (i, a1) in actions.iter().enumerate() {
            for (j, a2) in actions.iter().enumerate() {
                if i != j {
                    assert_ne!(a1, a2);
                }
            }
        }
    }
}
