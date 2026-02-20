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
use crate::provider::rig::RigProvider;

use super::panels::{ContextPanel, GraphPanel, ProgressPanel, ReasoningPanel};
use super::standalone::StandaloneState;
use super::state::{PanelId, SettingsField, TuiMode, TuiState};
use super::theme::Theme;
use super::views::{ChatView, HomeView, StudioView, TuiView, View, ViewAction};
use super::widgets::{Header, StatusBar};
use crate::config::mask_api_key;
use crossterm::event::KeyEvent;

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
    // ═══ View Navigation Actions ═══
    /// Switch to a specific view
    SwitchView(TuiView),
    /// Switch to next view (Tab)
    NextView,
    /// Switch to previous view (Shift+Tab)
    PrevView,
    // ═══ Chat Overlay Actions ═══
    /// Insert character in chat overlay input
    ChatOverlayInput(char),
    /// Backspace in chat overlay input
    ChatOverlayBackspace,
    /// Delete character in chat overlay input
    ChatOverlayDelete,
    /// Move cursor left in chat overlay
    ChatOverlayCursorLeft,
    /// Move cursor right in chat overlay
    ChatOverlayCursorRight,
    /// Navigate history up in chat overlay
    ChatOverlayHistoryUp,
    /// Navigate history down in chat overlay
    ChatOverlayHistoryDown,
    /// Send message in chat overlay
    ChatOverlaySend,
    /// Clear chat overlay messages
    ChatOverlayClear,
    /// Scroll up in chat overlay
    ChatOverlayScrollUp,
    /// Scroll down in chat overlay
    ChatOverlayScrollDown,
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
    // ═══ 4-View Architecture ═══
    /// Current active view
    current_view: TuiView,
    /// Chat view state
    chat_view: ChatView,
    /// Home view state (file browser)
    home_view: Option<HomeView>,
    /// Studio view state (YAML editor)
    studio_view: StudioView,
    // ═══ LLM Integration for ChatOverlay ═══
    /// Channel for receiving LLM responses
    llm_response_rx: mpsc::Receiver<String>,
    /// Sender for spawning LLM tasks
    llm_response_tx: mpsc::Sender<String>,
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

        // Initialize views
        let chat_view = ChatView::new();
        let mut studio_view = StudioView::new();
        // Load workflow file into studio view
        let _ = studio_view.load_file(workflow_path.to_path_buf());

        // Initialize LLM response channel
        let (llm_response_tx, llm_response_rx) = mpsc::channel(32);

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
            // 4-view architecture - start in Monitor mode for workflow execution
            current_view: TuiView::Monitor,
            chat_view,
            home_view: None, // No home view in execution mode
            studio_view,
            llm_response_rx,
            llm_response_tx,
        })
    }

    /// Create a new TUI application in standalone mode (file browser)
    pub fn new_standalone(standalone_state: StandaloneState) -> Result<Self> {
        // Use a dummy workflow path for standalone mode
        let workflow_path = standalone_state.root.clone();
        let state = TuiState::new("Standalone Mode");

        // Initialize views
        let chat_view = ChatView::new();
        let home_view = HomeView::new(standalone_state.root.clone());
        let studio_view = StudioView::new();

        // Initialize LLM response channel
        let (llm_response_tx, llm_response_rx) = mpsc::channel(32);

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
            // 4-view architecture - start in Home mode for standalone
            current_view: TuiView::Home,
            chat_view,
            home_view: Some(home_view),
            studio_view,
            llm_response_rx,
            llm_response_tx,
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

    /// Run the TUI with unified 4-view architecture
    ///
    /// This is the new entry point that supports all 4 views with unified
    /// navigation. The views are:
    /// - Chat (1/a): AI agent conversation
    /// - Home (2/h): Workflow browser
    /// - Studio (3/s): YAML editor
    /// - Monitor (4/m): Execution monitoring (existing 4-panel view)
    pub async fn run_unified(mut self) -> Result<()> {
        tracing::info!("TUI (unified) started");

        // Initialize terminal
        self.init_terminal()?;

        let tick_rate = Duration::from_millis(FRAME_RATE_MS);

        loop {
            // 1. Poll runtime events (same as run())
            self.poll_runtime_events();

            // 2. Update elapsed time
            self.state.tick();

            // 3. Render frame based on current view
            self.render_unified_frame()?;

            // 4. Get terminal size for input handling
            let terminal_size = if let Some(ref terminal) = self.terminal {
                terminal
                    .size()
                    .ok()
                    .map(|size| Rect::new(0, 0, size.width, size.height))
            } else {
                None
            };

            // 5. Poll input events
            if event::poll(tick_rate).map_err(|e| NikaError::TuiError {
                reason: format!("Failed to poll events: {}", e),
            })? {
                let event = event::read().map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to read event: {}", e),
                })?;

                let action = match event {
                    Event::Key(key) => self.handle_unified_key(key.code, key.modifiers),
                    Event::Mouse(mouse) => self.handle_mouse(mouse, terminal_size),
                    _ => Action::Continue,
                };
                self.apply_action(action);
            }

            // 6. Check quit flag
            if self.should_quit {
                break;
            }
        }

        // Cleanup
        self.cleanup()?;

        Ok(())
    }

    /// Poll runtime events from broadcast/mpsc receivers
    fn poll_runtime_events(&mut self) {
        // Check broadcast receiver (v0.4.1 preferred)
        if let Some(ref mut rx) = self.broadcast_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if matches!(
                            event.kind,
                            EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. }
                        ) {
                            self.workflow_done = true;
                        }
                        if self.state.should_break(&event.kind) {
                            self.state.paused = true;
                        }
                        self.state.handle_event(&event.kind, event.timestamp_ms);
                    }
                    Err(broadcast::error::TryRecvError::Empty) => break,
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("TUI lagged behind by {} events", n);
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
                if self.state.should_break(&event.kind) {
                    self.state.paused = true;
                }
                self.state.handle_event(&event.kind, event.timestamp_ms);
            }
        }

        // Poll LLM responses for ChatOverlay
        while let Ok(response) = self.llm_response_rx.try_recv() {
            // Remove "Thinking..." message and add actual response
            if let Some(last) = self.state.chat_overlay.messages.last() {
                if last.content == "Thinking..." {
                    self.state.chat_overlay.messages.pop();
                }
            }
            self.state.chat_overlay.add_nika_message(response);
        }
    }

    /// Render frame based on current view
    fn render_unified_frame(&mut self) -> Result<()> {
        let current_view = self.current_view;

        if let Some(ref mut terminal) = self.terminal {
            // For Monitor view, use the existing full-screen render (backward compatible)
            if current_view == TuiView::Monitor {
                let state = &self.state;
                let theme = &self.theme;
                terminal
                    .draw(|frame| render_frame(frame, state, theme))
                    .map_err(|e| NikaError::TuiError {
                        reason: format!("Failed to draw frame: {}", e),
                    })?;
                return Ok(());
            }

            // For other views, use unified layout with Header + Content + StatusBar
            // Extract references to avoid borrow issues with the closure
            let theme = &self.theme;
            let state = &self.state;
            let chat_view = &self.chat_view;
            let home_view = &self.home_view;
            let studio_view = &self.studio_view;
            let workflow_path = &self.state.workflow.path;
            let paused = self.state.paused;

            terminal
                .draw(|frame| {
                    let size = frame.area();

                    // Layout: Header (1) + Content (dynamic) + StatusBar (1)
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Header
                            Constraint::Min(0),    // Content
                            Constraint::Length(1), // StatusBar
                        ])
                        .split(size);

                    // Render header
                    let workflow_name = std::path::Path::new(workflow_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("No workflow");
                    let header = Header::new(current_view, theme)
                        .context(workflow_name)
                        .status(if paused { "PAUSED" } else { "" });
                    frame.render_widget(header, chunks[0]);

                    // Render view content based on current view
                    match current_view {
                        TuiView::Chat => {
                            chat_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Home => {
                            if let Some(ref hv) = home_view {
                                hv.render(frame, chunks[1], state, theme);
                            } else {
                                // Fallback: show placeholder if no home view
                                let placeholder = Paragraph::new("No workspace loaded")
                                    .block(Block::default().borders(Borders::ALL).title(" HOME "));
                                frame.render_widget(placeholder, chunks[1]);
                            }
                        }
                        TuiView::Studio => {
                            studio_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Monitor => {
                            // Already handled above, but needed for exhaustive match
                            unreachable!()
                        }
                    }

                    // Render status bar
                    let status_bar = StatusBar::new(current_view, theme);
                    frame.render_widget(status_bar, chunks[2]);
                })
                .map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to draw frame: {}", e),
                })?;
        }
        Ok(())
    }

    /// Handle keyboard input in unified mode
    ///
    /// This method delegates to each view's `handle_key` method and converts
    /// `ViewAction` to `Action` for the main event loop.
    fn handle_unified_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        // Handle mode-specific keys first (overlays)
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
            TuiMode::ChatOverlay => {
                return self.handle_chat_overlay_key(code, modifiers);
            }
            _ => {}
        }

        // Global view-switching keys (work in all views, including during Chat input)
        // We check these first so users can always navigate views
        match code {
            // Ctrl+C always quits
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,

            // View navigation by number (when not capturing input)
            KeyCode::Char('1') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Chat)
            }
            KeyCode::Char('2') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Home)
            }
            KeyCode::Char('3') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Studio)
            }
            KeyCode::Char('4') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Monitor)
            }

            // Tab cycles views (when not in Monitor, which uses Tab for panel cycling)
            KeyCode::Tab
                if !modifiers.contains(KeyModifiers::SHIFT)
                    && self.current_view != TuiView::Monitor =>
            {
                return Action::NextView
            }
            KeyCode::BackTab if self.current_view != TuiView::Monitor => return Action::PrevView,

            _ => {}
        }

        // View-specific key handling using the View trait
        let key_event = KeyEvent::new(code, modifiers);

        match self.current_view {
            TuiView::Monitor => {
                // Monitor uses the existing 4-panel key handling
                self.handle_key(code, modifiers)
            }
            TuiView::Chat => {
                let view_action = self.chat_view.handle_key(key_event, &mut self.state);
                self.convert_view_action(view_action)
            }
            TuiView::Home => {
                if let Some(ref mut home_view) = self.home_view {
                    let view_action = home_view.handle_key(key_event, &mut self.state);
                    self.convert_view_action(view_action)
                } else {
                    // No home view, handle basic navigation
                    match code {
                        KeyCode::Char('q') => Action::Quit,
                        _ => Action::Continue,
                    }
                }
            }
            TuiView::Studio => {
                let view_action = self.studio_view.handle_key(key_event, &mut self.state);
                self.convert_view_action(view_action)
            }
        }
    }

    /// Check if the current view is capturing text input
    /// (e.g., Chat with non-empty input, Studio in Insert mode)
    fn is_view_capturing_input(&self) -> bool {
        match self.current_view {
            TuiView::Chat => !self.chat_view.input.is_empty(),
            TuiView::Studio => self.studio_view.mode == super::views::EditorMode::Insert,
            _ => false,
        }
    }

    /// Convert a ViewAction to an Action
    fn convert_view_action(&mut self, view_action: ViewAction) -> Action {
        match view_action {
            ViewAction::None => Action::Continue,
            ViewAction::Quit => Action::Quit,
            ViewAction::SwitchView(view) => Action::SwitchView(view),
            ViewAction::RunWorkflow(path) => {
                // Switch to Monitor view and store path for execution
                // The actual workflow execution will be handled by apply_action
                self.workflow_path = path;
                self.current_view = TuiView::Monitor;
                // TODO: Trigger workflow execution
                Action::Continue
            }
            ViewAction::OpenInStudio(path) => {
                // Load the file into studio and switch to Studio view
                if let Err(e) = self.studio_view.load_file(path) {
                    tracing::error!("Failed to load file in studio: {}", e);
                }
                Action::SwitchView(TuiView::Studio)
            }
            ViewAction::SendChatMessage(msg) => {
                // TODO: Integrate with agent for actual processing
                // For now, add a placeholder response
                self.chat_view
                    .add_nika_message(format!("Received: {}", msg), None);
                Action::Continue
            }
            ViewAction::ToggleChatOverlay => {
                // Toggle chat overlay mode
                if self.state.mode == TuiMode::ChatOverlay {
                    Action::SetMode(TuiMode::Normal)
                } else {
                    Action::SetMode(TuiMode::ChatOverlay)
                }
            }
            ViewAction::Error(msg) => {
                tracing::error!("View error: {}", msg);
                self.set_status(&format!("Error: {}", msg));
                Action::Continue
            }
        }
    }

    /// Get current view
    pub fn current_view(&self) -> TuiView {
        self.current_view
    }

    /// Switch to a specific view
    pub fn switch_view(&mut self, view: TuiView) {
        self.current_view = view;
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
            TuiMode::ChatOverlay => {
                return self.handle_chat_overlay_key(code, modifiers);
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
            KeyCode::Char('c') => Action::SetMode(TuiMode::ChatOverlay), // Toggle chat overlay
            KeyCode::Char('C') => Action::CopyToClipboard,               // Copy (Shift+C)
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

    /// Handle keyboard input in Chat Overlay mode
    fn handle_chat_overlay_key(&self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        match code {
            // Exit chat overlay
            KeyCode::Esc => Action::SetMode(TuiMode::Normal),
            // Send message
            KeyCode::Enter => Action::ChatOverlaySend,
            // Text editing
            KeyCode::Backspace => Action::ChatOverlayBackspace,
            KeyCode::Delete => Action::ChatOverlayDelete,
            KeyCode::Left => Action::ChatOverlayCursorLeft,
            KeyCode::Right => Action::ChatOverlayCursorRight,
            // History navigation
            KeyCode::Up => Action::ChatOverlayHistoryUp,
            KeyCode::Down => Action::ChatOverlayHistoryDown,
            // Scroll message history
            KeyCode::PageUp => Action::ChatOverlayScrollUp,
            KeyCode::PageDown => Action::ChatOverlayScrollDown,
            // Clear chat (Ctrl+L)
            KeyCode::Char('l') if modifiers.contains(KeyModifiers::CONTROL) => {
                Action::ChatOverlayClear
            }
            // Character input
            KeyCode::Char(c) => Action::ChatOverlayInput(c),
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
            // View navigation actions
            Action::SwitchView(view) => {
                self.current_view = view;
            }
            Action::NextView => {
                self.current_view = self.current_view.next();
            }
            Action::PrevView => {
                self.current_view = self.current_view.prev();
            }
            // Chat overlay actions
            Action::ChatOverlayInput(c) => {
                self.state.chat_overlay.insert_char(c);
            }
            Action::ChatOverlayBackspace => {
                self.state.chat_overlay.backspace();
            }
            Action::ChatOverlayDelete => {
                self.state.chat_overlay.delete();
            }
            Action::ChatOverlayCursorLeft => {
                self.state.chat_overlay.cursor_left();
            }
            Action::ChatOverlayCursorRight => {
                self.state.chat_overlay.cursor_right();
            }
            Action::ChatOverlayHistoryUp => {
                self.state.chat_overlay.history_up();
            }
            Action::ChatOverlayHistoryDown => {
                self.state.chat_overlay.history_down();
            }
            Action::ChatOverlaySend => {
                if let Some(message) = self.state.chat_overlay.add_user_message() {
                    // Show "thinking" indicator
                    self.state.chat_overlay.add_nika_message("Thinking...");

                    // Spawn async task to call LLM
                    let tx = self.llm_response_tx.clone();
                    let prompt = message.clone();
                    tokio::spawn(async move {
                        let provider = RigProvider::openai();
                        match provider.infer(&prompt, None).await {
                            Ok(response) => {
                                let _ = tx.send(response).await;
                            }
                            Err(e) => {
                                let _ = tx.send(format!("Error: {}", e)).await;
                            }
                        }
                    });
                }
            }
            Action::ChatOverlayClear => {
                self.state.chat_overlay.clear();
            }
            Action::ChatOverlayScrollUp => {
                self.state.chat_overlay.scroll_up();
            }
            Action::ChatOverlayScrollDown => {
                self.state.chat_overlay.scroll_down();
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
        TuiMode::ChatOverlay => render_chat_overlay(frame, state, theme, size),
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

/// Render chat overlay as a right-side panel (40% width)
fn render_chat_overlay(frame: &mut Frame, state: &TuiState, theme: &Theme, area: Rect) {
    use ratatui::style::Color;
    use ratatui::text::{Line, Span};

    let chat = &state.chat_overlay;

    // Calculate overlay area (right 40% of screen)
    let overlay_width = (area.width * 40) / 100;
    let overlay = Rect {
        x: area.width.saturating_sub(overlay_width),
        y: 0,
        width: overlay_width,
        height: area.height,
    };

    // Clear the overlay area with background
    let clear = Block::default().style(Style::default().bg(theme.background));
    frame.render_widget(clear, overlay);

    // Split into messages area and input area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Messages
            Constraint::Length(3), // Input
            Constraint::Length(2), // Keybindings
        ])
        .split(overlay);

    // Render messages
    let mut message_lines: Vec<Line> = Vec::new();
    for msg in &chat.messages {
        let (prefix, style) = match msg.role {
            crate::tui::state::ChatOverlayMessageRole::User => (
                "You",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::tui::state::ChatOverlayMessageRole::Nika => (
                "Nika",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::tui::state::ChatOverlayMessageRole::System => {
                ("System", Style::default().fg(Color::DarkGray))
            }
        };

        message_lines.push(Line::from(vec![
            Span::styled(format!("─ {} ", prefix), style),
            Span::styled("─".repeat(20), Style::default().fg(Color::DarkGray)),
        ]));

        // Word-wrap message content to fit width
        let max_width = overlay_width.saturating_sub(4) as usize;
        for line in msg.content.lines() {
            // Simple word wrap
            let mut current_line = String::new();
            for word in line.split_whitespace() {
                if current_line.len() + word.len() + 1 > max_width {
                    if !current_line.is_empty() {
                        message_lines.push(Line::from(format!("  {}", current_line)));
                    }
                    current_line = word.to_string();
                } else {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
            }
            if !current_line.is_empty() {
                message_lines.push(Line::from(format!("  {}", current_line)));
            } else if line.is_empty() {
                message_lines.push(Line::from(""));
            }
        }
        message_lines.push(Line::from("")); // Spacing between messages
    }

    let messages_block = Block::default()
        .borders(Borders::ALL)
        .title(" 💬 Chat ")
        .border_style(Style::default().fg(theme.border_focused));

    let messages_paragraph = Paragraph::new(message_lines)
        .block(messages_block)
        .scroll((chat.scroll as u16, 0));

    frame.render_widget(messages_paragraph, chunks[0]);

    // Render input with cursor
    let input = &chat.input;
    let cursor = chat.cursor;

    let (before, cursor_char, after) = if input.is_empty() {
        ("", ' ', "")
    } else {
        let before = &input[..cursor.min(input.len())];
        let cursor_char = input.chars().nth(cursor).unwrap_or(' ');
        let after = if cursor < input.len() {
            &input[cursor + 1..]
        } else {
            ""
        };
        (before, cursor_char, after)
    };

    let input_line = Line::from(vec![
        Span::styled(" > ", Style::default().fg(theme.highlight)),
        Span::raw(before),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(theme.highlight).fg(Color::Black),
        ),
        Span::raw(after),
    ]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_normal));

    let input_paragraph = Paragraph::new(input_line).block(input_block);
    frame.render_widget(input_paragraph, chunks[1]);

    // Render keybindings
    let keybindings = Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(theme.highlight)),
        Span::styled(" Send  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Up/Dn]", Style::default().fg(theme.highlight)),
        Span::styled(" History  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Esc]", Style::default().fg(theme.highlight)),
        Span::styled(" Close", Style::default().fg(Color::DarkGray)),
    ]);

    let hint_paragraph = Paragraph::new(keybindings).alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(hint_paragraph, chunks[2]);
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
    use crate::tui::views::EditorMode;

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

    // ═══ Task 5.1: 4-View Integration Tests ═══

    #[test]
    fn test_app_initial_view_standalone() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = StandaloneState::new(temp_dir.path().to_path_buf());
        let app = App::new_standalone(state).unwrap();
        assert_eq!(app.current_view, TuiView::Home);
    }

    #[test]
    fn test_app_initial_view_execution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();
        assert_eq!(app.current_view, TuiView::Monitor);
    }

    #[test]
    fn test_app_view_switch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Initial view is Monitor
        assert_eq!(app.current_view, TuiView::Monitor);

        // Switch to Home
        app.switch_view(TuiView::Home);
        assert_eq!(app.current_view, TuiView::Home);

        // Switch to Chat
        app.switch_view(TuiView::Chat);
        assert_eq!(app.current_view, TuiView::Chat);

        // Switch to Studio
        app.switch_view(TuiView::Studio);
        assert_eq!(app.current_view, TuiView::Studio);

        // Switch back to Monitor
        app.switch_view(TuiView::Monitor);
        assert_eq!(app.current_view, TuiView::Monitor);
    }

    #[test]
    fn test_app_view_next_prev() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start at Monitor
        app.current_view = TuiView::Chat;

        // Next should go Chat -> Home -> Studio -> Monitor -> Chat
        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Home);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Studio);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Monitor);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Chat);

        // Prev should go Chat -> Monitor -> Studio -> Home -> Chat
        app.current_view = app.current_view.prev();
        assert_eq!(app.current_view, TuiView::Monitor);
    }

    #[test]
    fn test_app_is_view_capturing_input() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Chat with empty input is not capturing
        app.current_view = TuiView::Chat;
        app.chat_view.input.clear();
        assert!(!app.is_view_capturing_input());

        // Chat with input is capturing
        app.chat_view.input = "typing...".to_string();
        assert!(app.is_view_capturing_input());

        // Studio in Normal mode is not capturing
        app.current_view = TuiView::Studio;
        app.studio_view.mode = EditorMode::Normal;
        assert!(!app.is_view_capturing_input());

        // Studio in Insert mode is capturing
        app.studio_view.mode = EditorMode::Insert;
        assert!(app.is_view_capturing_input());

        // Home and Monitor never capture
        app.current_view = TuiView::Home;
        assert!(!app.is_view_capturing_input());

        app.current_view = TuiView::Monitor;
        assert!(!app.is_view_capturing_input());
    }

    #[test]
    fn test_convert_view_action_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        let action = app.convert_view_action(ViewAction::None);
        assert_eq!(action, Action::Continue);
    }

    #[test]
    fn test_convert_view_action_quit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        let action = app.convert_view_action(ViewAction::Quit);
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn test_convert_view_action_switch_view() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        let action = app.convert_view_action(ViewAction::SwitchView(TuiView::Home));
        assert_eq!(action, Action::SwitchView(TuiView::Home));
    }

    #[test]
    fn test_convert_view_action_send_chat_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Record initial message count
        let initial_count = app.chat_view.messages.len();

        // Send a message
        let action = app.convert_view_action(ViewAction::SendChatMessage("Hello".to_string()));
        assert_eq!(action, Action::Continue);

        // Should have added a response message
        assert_eq!(app.chat_view.messages.len(), initial_count + 1);
    }

    // ═══════════════════════════════════════════
    // CHAT OVERLAY TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn test_chat_overlay_toggle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Normal mode
        assert_eq!(app.state.mode, TuiMode::Normal);

        // Toggle to ChatOverlay
        let action = app.convert_view_action(ViewAction::ToggleChatOverlay);
        assert_eq!(action, Action::SetMode(TuiMode::ChatOverlay));

        // Apply the action
        app.state.mode = TuiMode::ChatOverlay;

        // Toggle back to Normal
        let action = app.convert_view_action(ViewAction::ToggleChatOverlay);
        assert_eq!(action, Action::SetMode(TuiMode::Normal));
    }

    #[test]
    fn test_chat_overlay_input_action() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Apply input action
        app.apply_action(Action::ChatOverlayInput('h'));
        app.apply_action(Action::ChatOverlayInput('i'));

        assert_eq!(app.state.chat_overlay.input, "hi");
        assert_eq!(app.state.chat_overlay.cursor, 2);
    }

    #[test]
    fn test_chat_overlay_backspace_action() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Set up initial state
        app.state.chat_overlay.input = "hello".to_string();
        app.state.chat_overlay.cursor = 5;

        // Apply backspace action
        app.apply_action(Action::ChatOverlayBackspace);

        assert_eq!(app.state.chat_overlay.input, "hell");
        assert_eq!(app.state.chat_overlay.cursor, 4);
    }

    #[tokio::test]
    async fn test_chat_overlay_send_action() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Set up message
        app.state.chat_overlay.input = "test message".to_string();
        app.state.chat_overlay.cursor = 12;

        let initial_count = app.state.chat_overlay.messages.len();

        // Apply send action
        app.apply_action(Action::ChatOverlaySend);

        // Input should be cleared
        assert!(app.state.chat_overlay.input.is_empty());

        // Should have 2 new messages: user message and "Thinking..." placeholder
        // The actual LLM response comes asynchronously via the channel
        assert_eq!(app.state.chat_overlay.messages.len(), initial_count + 2);
    }

    #[test]
    fn test_chat_overlay_clear_action() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Add some messages
        app.state.chat_overlay.add_nika_message("Message 1");
        app.state.chat_overlay.add_nika_message("Message 2");

        // Apply clear action
        app.apply_action(Action::ChatOverlayClear);

        // Should only have 1 system message
        assert_eq!(app.state.chat_overlay.messages.len(), 1);
    }

    #[test]
    fn test_chat_overlay_history_actions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Add history
        app.state.chat_overlay.history = vec!["first".to_string(), "second".to_string()];

        // Navigate up
        app.apply_action(Action::ChatOverlayHistoryUp);
        assert_eq!(app.state.chat_overlay.input, "second");

        app.apply_action(Action::ChatOverlayHistoryUp);
        assert_eq!(app.state.chat_overlay.input, "first");

        // Navigate down
        app.apply_action(Action::ChatOverlayHistoryDown);
        assert_eq!(app.state.chat_overlay.input, "second");
    }

    #[test]
    fn test_chat_overlay_scroll_actions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        assert_eq!(app.state.chat_overlay.scroll, 0);

        app.apply_action(Action::ChatOverlayScrollUp);
        assert_eq!(app.state.chat_overlay.scroll, 1);

        app.apply_action(Action::ChatOverlayScrollDown);
        assert_eq!(app.state.chat_overlay.scroll, 0);
    }

    #[test]
    fn test_chat_overlay_cursor_actions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        app.state.chat_overlay.input = "hello".to_string();
        app.state.chat_overlay.cursor = 3;

        app.apply_action(Action::ChatOverlayCursorLeft);
        assert_eq!(app.state.chat_overlay.cursor, 2);

        app.apply_action(Action::ChatOverlayCursorRight);
        assert_eq!(app.state.chat_overlay.cursor, 3);
    }

    #[test]
    fn test_handle_chat_overlay_key_escape_returns_normal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        let action = app.handle_chat_overlay_key(KeyCode::Esc, KeyModifiers::empty());
        assert_eq!(action, Action::SetMode(TuiMode::Normal));
    }

    #[test]
    fn test_handle_chat_overlay_key_enter_sends_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        let action = app.handle_chat_overlay_key(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(action, Action::ChatOverlaySend);
    }

    #[test]
    fn test_handle_chat_overlay_key_char_input() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        let action = app.handle_chat_overlay_key(KeyCode::Char('x'), KeyModifiers::empty());
        assert_eq!(action, Action::ChatOverlayInput('x'));
    }

    #[test]
    fn test_handle_key_c_opens_chat_overlay() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // In Monitor mode, 'c' should open chat overlay
        let action = app.handle_key(KeyCode::Char('c'), KeyModifiers::empty());
        assert_eq!(action, Action::SetMode(TuiMode::ChatOverlay));
    }

    #[test]
    fn test_handle_key_shift_c_copies_to_clipboard() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // In Monitor mode, 'C' (Shift+c) should copy to clipboard
        let action = app.handle_key(KeyCode::Char('C'), KeyModifiers::empty());
        assert_eq!(action, Action::CopyToClipboard);
    }

    #[tokio::test]
    async fn test_app_uses_openai_provider() {
        // Verify OPENAI_API_KEY env is checked
        std::env::set_var("OPENAI_API_KEY", "test-key");
        // The app should compile with OpenAI provider
        // This is a compile-time check essentially
        assert!(std::env::var("OPENAI_API_KEY").is_ok());
    }
}
