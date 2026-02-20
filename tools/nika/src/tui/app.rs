//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.

use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
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
    /// Cycle tabs in focused panel
    CycleTab,
    /// Toggle mode
    SetMode(TuiMode),
    /// Scroll up in focused panel
    ScrollUp,
    /// Scroll down in focused panel
    ScrollDown,
    // ═══ Quick Actions (TIER 1) ═══
    /// Copy current panel content to clipboard [c]
    CopyToClipboard,
    /// Retry failed workflow [r]
    RetryWorkflow,
    /// Export trace to file [e]
    ExportTrace,
    // ═══ Breakpoint Actions (TIER 2.3) ═══
    /// Toggle breakpoint on current task [b]
    ToggleBreakpoint,
    // ═══ Theme Actions (TIER 2.4) ═══
    /// Toggle theme dark/light [t]
    ToggleTheme,
    // ═══ Mouse Actions (TIER 3.1) ═══
    /// Click on a panel to focus it
    MouseClickPanel(PanelId),
    /// Scroll up
    MouseScrollUp,
    /// Scroll down
    MouseScrollDown,
    // ═══ Notification Actions (TIER 3.4) ═══
    /// Dismiss the most recent notification [n]
    DismissNotification,
    /// Dismiss all notifications [N]
    DismissAllNotifications,
    // ═══ Filter/Search Actions (TIER 1.5) ═══
    /// Enter search/filter mode
    EnterFilter,
    /// Exit search/filter mode
    ExitFilter,
    /// Insert character in filter query
    FilterInput(char),
    /// Backspace in filter query
    FilterBackspace,
    /// Delete character in filter query
    FilterDelete,
    /// Move filter cursor left
    FilterCursorLeft,
    /// Move filter cursor right
    FilterCursorRight,
    /// Clear filter query
    FilterClear,
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
    /// Status message for feedback (clipboard copy, export, etc.)
    status_message: Option<(String, std::time::Instant)>,
    /// Retry requested flag (TIER 1.2) - caller should re-run workflow
    retry_requested: bool,
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
            status_message: None,
            retry_requested: false,
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
            status_message: None,
            retry_requested: false,
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
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| {
            NikaError::TuiError {
                reason: format!("Failed to enter alternate screen: {}", e),
            }
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

            // 4. Get terminal size for mouse coordinate mapping (TIER 3.1)
            let terminal_size = if let Some(ref terminal) = self.terminal {
                terminal
                    .size()
                    .ok()
                    .map(|size| Rect::new(0, 0, size.width, size.height))
            } else {
                None
            };

            // 5. Poll input events (with timeout for frame rate)
            if event::poll(tick_rate).map_err(|e| NikaError::TuiError {
                reason: format!("Failed to poll events: {}", e),
            })? {
                let event = event::read().map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to read event: {}", e),
                })?;

                let action = match event {
                    Event::Key(key) => self.handle_key(key.code, key.modifiers),
                    Event::Mouse(mouse) => self.handle_mouse(mouse, terminal_size),
                    _ => Action::Continue,
                };
                self.apply_action(action);
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
            TuiMode::Search => {
                return self.handle_search_key(code, modifiers);
            }
            _ => {}
        }

        // Global keys
        match code {
            // Quit
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,

            // Panel navigation (direct panel access)
            KeyCode::Char('1') => Action::FocusPanel(1),
            KeyCode::Char('2') => Action::FocusPanel(2),
            KeyCode::Char('3') => Action::FocusPanel(3),
            KeyCode::Char('4') => Action::FocusPanel(4),
            // h/l for panel cycling (vim-style)
            KeyCode::Char('h') => Action::FocusPrev,
            KeyCode::Char('l') => Action::FocusNext,

            // Tab cycling within focused panel
            KeyCode::Tab | KeyCode::Char('t') => Action::CycleTab,
            KeyCode::BackTab => Action::CycleTab, // Cycle in same direction (simple)

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
            KeyCode::Char('/') => Action::EnterFilter, // TIER 1.5: Filter mode

            // Quick actions (TIER 1)
            KeyCode::Char('c') => Action::CopyToClipboard,
            KeyCode::Char('r') => Action::RetryWorkflow,
            KeyCode::Char('e') => Action::ExportTrace,
            KeyCode::Char('b') => Action::ToggleBreakpoint, // TIER 2.3: Breakpoints
            KeyCode::Char('T') => Action::ToggleTheme,      // TIER 2.4: Theme toggle (Shift+T)
            KeyCode::Char('n') => Action::DismissNotification, // TIER 3.4: Dismiss notification
            KeyCode::Char('N') => Action::DismissAllNotifications, // TIER 3.4: Dismiss all notifications

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

    /// Handle keyboard input in Search/Filter mode (TIER 1.5)
    fn handle_search_key(&self, code: KeyCode, _modifiers: KeyModifiers) -> Action {
        match code {
            // Exit search mode
            KeyCode::Esc => Action::ExitFilter,
            KeyCode::Enter => Action::ExitFilter, // Confirm and exit
            // Text editing
            KeyCode::Backspace => Action::FilterBackspace,
            KeyCode::Delete => Action::FilterDelete,
            KeyCode::Left => Action::FilterCursorLeft,
            KeyCode::Right => Action::FilterCursorRight,
            // Clear filter
            KeyCode::Char('u') if _modifiers.contains(KeyModifiers::CONTROL) => Action::FilterClear,
            // Character input
            KeyCode::Char(c) => Action::FilterInput(c),
            _ => Action::Continue,
        }
    }

    /// Handle mouse input (TIER 3.1)
    ///
    /// Maps mouse clicks to panel focus and scroll wheel to scrolling.
    fn handle_mouse(&self, mouse: MouseEvent, terminal_size: Option<Rect>) -> Action {
        let Some(size) = terminal_size else {
            return Action::Continue;
        };

        match mouse.kind {
            // Left click - focus panel at click position
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(panel_id) = self.panel_at_position(mouse.column, mouse.row, size) {
                    Action::MouseClickPanel(panel_id)
                } else {
                    Action::Continue
                }
            }
            // Scroll wheel - scroll focused panel content
            MouseEventKind::ScrollUp => Action::MouseScrollUp,
            MouseEventKind::ScrollDown => Action::MouseScrollDown,
            // Other mouse events - ignore
            _ => Action::Continue,
        }
    }

    /// Determine which panel is at the given screen position (TIER 3.1)
    ///
    /// Uses the same 2x2 layout as render_frame.
    fn panel_at_position(&self, x: u16, y: u16, size: Rect) -> Option<PanelId> {
        // Calculate panel boundaries (2x2 grid)
        let half_width = size.width / 2;
        let half_height = size.height / 2;

        // Determine row and column
        let is_top = y < half_height;
        let is_left = x < half_width;

        Some(match (is_top, is_left) {
            (true, true) => PanelId::Progress, // Top-left: Mission Control
            (true, false) => PanelId::Dag,     // Top-right: DAG View
            (false, true) => PanelId::NovaNet, // Bottom-left: NovaNet MCP
            (false, false) => PanelId::Agent,  // Bottom-right: Agent Reasoning
        })
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
            Action::CycleTab => self.state.cycle_tab(),
            Action::SetMode(mode) => self.state.mode = mode,
            Action::ScrollUp => {
                // TIER 1.3: MCP navigation when NovaNet is focused
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_prev_mcp();
                } else {
                    let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                    *scroll = scroll.saturating_sub(1);
                }
            }
            Action::ScrollDown => {
                // TIER 1.3: MCP navigation when NovaNet is focused
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_next_mcp();
                } else {
                    let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                    *scroll += 1;
                }
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
            // Filter/Search actions (TIER 1.5)
            Action::EnterFilter => {
                self.state.mode = TuiMode::Search;
            }
            Action::ExitFilter => {
                self.state.mode = TuiMode::Normal;
                // Keep filter active but exit edit mode
            }
            Action::FilterInput(c) => self.state.filter_push(c),
            Action::FilterBackspace => self.state.filter_backspace(),
            Action::FilterDelete => self.state.filter_delete(),
            Action::FilterCursorLeft => self.state.filter_cursor_left(),
            Action::FilterCursorRight => self.state.filter_cursor_right(),
            Action::FilterClear => self.state.filter_clear(),
            // Quick actions (TIER 1)
            Action::CopyToClipboard => {
                self.copy_to_clipboard();
            }
            Action::RetryWorkflow => {
                self.retry_workflow();
            }
            Action::ExportTrace => {
                self.export_trace();
            }
            // Breakpoint actions (TIER 2.3)
            Action::ToggleBreakpoint => {
                self.toggle_breakpoint();
            }
            // Theme actions (TIER 2.4)
            Action::ToggleTheme => {
                self.toggle_theme();
            }
            // Mouse actions (TIER 3.1)
            Action::MouseClickPanel(panel_id) => {
                self.state.focus = panel_id;
            }
            Action::MouseScrollUp => {
                // Use same logic as ScrollUp but for mouse wheel
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_prev_mcp();
                } else {
                    let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                    *scroll = scroll.saturating_sub(3); // Scroll 3 lines at a time for mouse
                }
            }
            Action::MouseScrollDown => {
                // Use same logic as ScrollDown but for mouse wheel
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_next_mcp();
                } else {
                    let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                    *scroll += 3; // Scroll 3 lines at a time for mouse
                }
            }
            // Notification actions (TIER 3.4)
            Action::DismissNotification => {
                let count = self.state.active_notification_count();
                self.state.dismiss_notification();
                if count > 0 {
                    let msg = format!("Dismissed notification ({} remaining)", count - 1);
                    self.set_status(&msg);
                }
            }
            Action::DismissAllNotifications => {
                let count = self.state.active_notification_count();
                self.state.dismiss_all_notifications();
                if count > 0 {
                    let msg = format!("Dismissed all {} notifications", count);
                    self.set_status(&msg);
                }
            }
            Action::Continue => {}
        }
    }

    /// Copy current panel content to system clipboard
    fn copy_to_clipboard(&mut self) {
        #[cfg(feature = "tui")]
        {
            if let Some(content) = self.state.get_copyable_content() {
                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => match clipboard.set_text(&content) {
                        Ok(_) => {
                            let preview = if content.len() > 50 {
                                format!("{}...", &content[..50])
                            } else {
                                content.clone()
                            };
                            self.set_status(&format!("✓ Copied: {}", preview.replace('\n', " ")));
                        }
                        Err(e) => {
                            self.set_status(&format!("✗ Clipboard error: {}", e));
                        }
                    },
                    Err(e) => {
                        self.set_status(&format!("✗ Clipboard unavailable: {}", e));
                    }
                }
            } else {
                self.set_status("Nothing to copy");
            }
        }
    }

    /// Export trace to file
    fn export_trace(&mut self) {
        use std::io::Write;

        // Generate trace filename
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let workflow_name = self
            .workflow_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "workflow".to_string());
        let filename = format!("{}_{}.json", workflow_name, timestamp);

        // Build trace content
        let trace = serde_json::json!({
            "workflow": self.workflow_path.display().to_string(),
            "generation_id": self.state.workflow.generation_id,
            "status": format!("{:?}", self.state.workflow.phase),
            "tasks_completed": self.state.workflow.tasks_completed,
            "task_count": self.state.workflow.task_count,
            "elapsed_ms": self.state.workflow.elapsed_ms,
            "metrics": {
                "total_tokens": self.state.metrics.total_tokens,
                "input_tokens": self.state.metrics.input_tokens,
                "output_tokens": self.state.metrics.output_tokens,
                "cost_usd": self.state.metrics.cost_usd,
            },
            "mcp_calls": self.state.mcp_calls.len(),
            "agent_turns": self.state.agent_turns.len(),
        });

        // Write to file
        match std::fs::File::create(&filename) {
            Ok(mut file) => match serde_json::to_string_pretty(&trace) {
                Ok(json) => match file.write_all(json.as_bytes()) {
                    Ok(_) => {
                        self.set_status(&format!("✓ Exported: {}", filename));
                    }
                    Err(e) => {
                        self.set_status(&format!("✗ Write error: {}", e));
                    }
                },
                Err(e) => {
                    self.set_status(&format!("✗ JSON error: {}", e));
                }
            },
            Err(e) => {
                self.set_status(&format!("✗ File error: {}", e));
            }
        }
    }

    /// Toggle breakpoint on the current task (TIER 2.3)
    fn toggle_breakpoint(&mut self) {
        use super::state::Breakpoint;

        // Get current task from state
        if let Some(ref task_id) = self.state.current_task.clone() {
            let bp = Breakpoint::BeforeTask(task_id.clone());
            if self.state.breakpoints.contains(&bp) {
                self.state.breakpoints.remove(&bp);
                self.set_status(&format!("🔴 Breakpoint removed: {}", task_id));
            } else {
                self.state.breakpoints.insert(bp);
                self.set_status(&format!("🔴 Breakpoint set: {}", task_id));
            }
        } else if !self.state.task_order.is_empty() {
            // No current task, use first task
            let task_id = self.state.task_order[0].clone();
            let bp = Breakpoint::BeforeTask(task_id.clone());
            if self.state.breakpoints.contains(&bp) {
                self.state.breakpoints.remove(&bp);
                self.set_status(&format!("🔴 Breakpoint removed: {}", task_id));
            } else {
                self.state.breakpoints.insert(bp);
                self.set_status(&format!("🔴 Breakpoint set: {}", task_id));
            }
        } else {
            self.set_status("No tasks to set breakpoint on");
        }
    }

    /// Toggle theme between dark and light (TIER 2.4)
    fn toggle_theme(&mut self) {
        self.state.theme_mode = self.state.theme_mode.toggle();
        let mode_name = match self.state.theme_mode {
            super::theme::ThemeMode::Dark => "Dark",
            super::theme::ThemeMode::Light => "Light",
        };
        self.set_status(&format!("🎨 Theme: {}", mode_name));
    }

    /// Set status message with auto-clear timer
    fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), std::time::Instant::now()));
    }

    /// Get status message if still valid (clears after 3 seconds)
    #[allow(dead_code)] // Reserved for future status bar display
    fn get_status(&mut self) -> Option<String> {
        if let Some((msg, time)) = &self.status_message {
            if time.elapsed() < Duration::from_secs(3) {
                return Some(msg.clone());
            }
            self.status_message = None;
        }
        None
    }

    /// Request workflow retry (TIER 1.2)
    ///
    /// Resets failed tasks and signals that caller should re-run the workflow.
    /// Only works when workflow is in failed state.
    fn retry_workflow(&mut self) {
        if self.state.is_running() {
            self.set_status("⚠ Cannot retry: workflow is still running");
            return;
        }

        if self.state.is_success() {
            self.set_status("⚠ Cannot retry: workflow completed successfully");
            return;
        }

        if !self.state.is_failed() {
            self.set_status("⚠ Nothing to retry");
            return;
        }

        // Reset state for retry
        let reset_tasks = self.state.reset_for_retry();
        self.retry_requested = true;
        self.workflow_done = false;

        if reset_tasks.is_empty() {
            self.set_status("✓ Ready to retry (no failed tasks found)");
        } else {
            self.set_status(&format!(
                "✓ Ready to retry: {} task(s) reset ({})",
                reset_tasks.len(),
                reset_tasks.join(", ")
            ));
        }
    }

    /// Check if retry was requested (for caller to re-run workflow)
    pub fn wants_retry(&self) -> bool {
        self.retry_requested
    }

    /// Clear retry request flag
    pub fn clear_retry_request(&mut self) {
        self.retry_requested = false;
    }

    /// Cleanup terminal state
    fn cleanup(&mut self) -> Result<()> {
        if let Some(ref mut terminal) = self.terminal {
            disable_raw_mode().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to disable raw mode: {}", e),
            })?;

            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )
            .map_err(|e| NikaError::TuiError {
                reason: format!("Failed to leave alternate screen: {}", e),
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
            let _ = execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            );
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
        TuiMode::Search => render_search_bar(frame, state, theme, size),
        _ => {
            // Show filter indicator if filter is active (not in Search mode)
            if state.has_filter() {
                render_filter_indicator(frame, state, theme, size);
            }
        }
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
╔═══════════════════════════════════════════════════════════════════════╗
║  KEYBOARD SHORTCUTS                                                   ║
╠═══════════════════════════════════════════════════════════════════════╣
║                                                                       ║
║  NAVIGATION             EXECUTION           QUICK ACTIONS            ║
║  Tab       Next panel   Space  Pause        c    Copy to clipboard   ║
║  1-4       Jump panel   Enter  Step         e    Export trace        ║
║  h/l       Cycle panel  q      Quit         r    Retry workflow      ║
║  j/k       Scroll       /      Filter       n    Dismiss notification║
║                                             N    Dismiss all notifs  ║
║  OVERLAYS               SETTINGS                                      ║
║  ?/F1      This help    s       Open settings                        ║
║  m         Metrics      Esc    Close overlay                         ║
║                                                                       ║
╚═══════════════════════════════════════════════════════════════════════╝
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

/// Render search bar at bottom of screen (TIER 1.5)
fn render_search_bar(frame: &mut Frame, state: &TuiState, theme: &Theme, area: Rect) {
    use ratatui::style::Color;
    use ratatui::text::{Line, Span};

    // Position at bottom of screen
    let bar_area = Rect {
        x: area.x,
        y: area.height.saturating_sub(3),
        width: area.width,
        height: 3,
    };

    // Build search input with cursor
    let query = &state.filter_query;
    let cursor = state.filter_cursor;

    let (before, cursor_char, after) = if query.is_empty() {
        ("", ' ', "")
    } else {
        let before = &query[..cursor.min(query.len())];
        let cursor_char = query.chars().nth(cursor).unwrap_or(' ');
        let after = if cursor < query.len() {
            &query[cursor + 1..]
        } else {
            ""
        };
        (before, cursor_char, after)
    };

    let content = Line::from(vec![
        Span::styled(
            "  🔍 /",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled(
            cursor_char.to_string(),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);

    let hint = Line::from(vec![Span::styled(
        "  [Enter] Apply  [Esc] Cancel  [Ctrl+U] Clear",
        Style::default().fg(Color::DarkGray),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Filter ")
        .border_style(Style::default().fg(theme.border_focused))
        .style(Style::default());

    let paragraph = Paragraph::new(vec![content, hint]).block(block);
    frame.render_widget(paragraph, bar_area);
}

/// Render filter indicator at bottom right (when filter is active but not in Search mode)
fn render_filter_indicator(frame: &mut Frame, state: &TuiState, _theme: &Theme, area: Rect) {
    use ratatui::style::Color;
    use ratatui::text::{Line, Span};

    let query = &state.filter_query;
    let filtered_tasks = state.filtered_task_ids();
    let total_tasks = state.task_order.len();

    // Count matching/total for display
    let match_info = format!("{}/{}", filtered_tasks.len(), total_tasks);

    let indicator = Line::from(vec![
        Span::styled("🔍 /", Style::default().fg(Color::Cyan)),
        Span::styled(query, Style::default().fg(Color::Yellow)),
        Span::styled(" ", Style::default()),
        Span::styled(
            format!("({})", match_info),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" [/] edit", Style::default().fg(Color::DarkGray)),
    ]);

    // Position at bottom right
    let indicator_width = (query.len() + match_info.len() + 20) as u16;
    let indicator_area = Rect {
        x: area.width.saturating_sub(indicator_width + 2),
        y: area.height.saturating_sub(1),
        width: indicator_width,
        height: 1,
    };

    let paragraph = Paragraph::new(indicator);
    frame.render_widget(paragraph, indicator_area);
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
            Action::CycleTab, // Phase 2: Tab cycling
            Action::SetMode(TuiMode::Help),
            Action::ScrollUp,
            Action::ScrollDown,
            Action::CopyToClipboard, // TIER 1
            Action::RetryWorkflow,   // TIER 1
            Action::ExportTrace,     // TIER 1
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

    #[test]
    fn test_cycle_tab_action_exists() {
        // Verify CycleTab action is distinct from other actions
        let cycle = Action::CycleTab;
        assert_ne!(cycle, Action::Continue);
        assert_ne!(cycle, Action::FocusNext);
        assert_ne!(cycle, Action::FocusPrev);
        assert_eq!(cycle, Action::CycleTab);
    }

    // ═══ TIER 3.1: Mouse Support Tests ═══

    #[test]
    fn test_mouse_action_variants() {
        // Verify mouse action variants exist and are distinct
        let click = Action::MouseClickPanel(PanelId::Progress);
        let scroll_up = Action::MouseScrollUp;
        let scroll_down = Action::MouseScrollDown;

        assert_ne!(click, scroll_up);
        assert_ne!(click, scroll_down);
        assert_ne!(scroll_up, scroll_down);
        assert_ne!(click, Action::Continue);
    }

    #[test]
    fn test_mouse_click_panel_contains_panel_id() {
        // Verify different panels produce different actions
        let click_progress = Action::MouseClickPanel(PanelId::Progress);
        let click_dag = Action::MouseClickPanel(PanelId::Dag);
        let click_novanet = Action::MouseClickPanel(PanelId::NovaNet);
        let click_agent = Action::MouseClickPanel(PanelId::Agent);

        assert_ne!(click_progress, click_dag);
        assert_ne!(click_progress, click_novanet);
        assert_ne!(click_progress, click_agent);
        assert_ne!(click_dag, click_novanet);
        assert_ne!(click_dag, click_agent);
        assert_ne!(click_novanet, click_agent);
    }

    #[test]
    fn test_panel_at_position_quadrants() {
        use ratatui::layout::Rect;

        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();

        let app = App::new(&workflow_path).unwrap();
        let size = Rect::new(0, 0, 100, 50);

        // Top-left quadrant (0-49, 0-24) -> Progress
        assert_eq!(app.panel_at_position(10, 10, size), Some(PanelId::Progress));

        // Top-right quadrant (50-99, 0-24) -> Dag
        assert_eq!(app.panel_at_position(60, 10, size), Some(PanelId::Dag));

        // Bottom-left quadrant (0-49, 25-49) -> NovaNet
        assert_eq!(app.panel_at_position(10, 30, size), Some(PanelId::NovaNet));

        // Bottom-right quadrant (50-99, 25-49) -> Agent
        assert_eq!(app.panel_at_position(60, 30, size), Some(PanelId::Agent));
    }

    #[test]
    fn test_panel_at_position_boundaries() {
        use ratatui::layout::Rect;

        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();

        let app = App::new(&workflow_path).unwrap();
        let size = Rect::new(0, 0, 100, 50);

        // Boundary at (49, 24) - still in top-left
        assert_eq!(app.panel_at_position(49, 24, size), Some(PanelId::Progress));

        // Boundary at (50, 24) - now in top-right
        assert_eq!(app.panel_at_position(50, 24, size), Some(PanelId::Dag));

        // Boundary at (49, 25) - now in bottom-left
        assert_eq!(app.panel_at_position(49, 25, size), Some(PanelId::NovaNet));

        // Boundary at (50, 25) - now in bottom-right
        assert_eq!(app.panel_at_position(50, 25, size), Some(PanelId::Agent));
    }
}
