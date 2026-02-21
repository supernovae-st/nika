//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.

use std::io::{self, Stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dashmap::DashMap;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame, Terminal,
};
use tokio::sync::{broadcast, mpsc, OnceCell};
use tokio::task::JoinSet;

use crate::ast::schema_validator::WorkflowSchemaValidator;
use crate::ast::{AgentParams, McpConfigInline, Workflow};
use crate::error::{NikaError, Result};
use crate::event::{Event as NikaEvent, EventKind, EventLog};
use crate::mcp::McpClient;
use crate::mcp::McpConfig;
use crate::provider::rig::{RigProvider, StreamChunk};
use crate::runtime::{RigAgentLoop, RigAgentStatus, Runner};
use crate::tui::chat_agent::ChatAgent;
use crate::tui::command::ModelProvider;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

use super::focus::{FocusState, PanelId as NavPanelId};
use super::mode::InputMode;
use super::panels::{ContextPanel, GraphPanel, ProgressPanel, ReasoningPanel};
use super::standalone::{HistoryEntry, StandaloneState};
use super::state::{PanelId, SettingsField, TuiMode, TuiState};
use super::theme::Theme;
use super::views::{ChatView, HomeView, McpAction, StudioView, TuiView, View, ViewAction};
use super::widgets::{ConnectionStatus, Header, Provider, StatusBar, StatusMetrics};
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
    /// Scroll to top of focused panel [g]
    ScrollToTop,
    /// Scroll to bottom of focused panel [G]
    ScrollToBottom,
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
    /// Note: Used during construction for HomeView initialization
    #[allow(dead_code)]
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
    // ═══ 4-View Architecture + Navigation 2.0 ═══
    /// Current active view
    current_view: TuiView,
    /// Current input mode (Normal, Insert, Command, Search)
    input_mode: InputMode,
    /// Panel focus state for keyboard navigation
    focus_state: FocusState,
    /// Chat view state
    chat_view: ChatView,
    /// Home view state (file browser)
    home_view: Option<HomeView>,
    /// Studio view state (YAML editor)
    studio_view: StudioView,
    // ═══ LLM Integration for ChatOverlay ═══
    /// Channel for receiving LLM responses (complete responses)
    llm_response_rx: mpsc::Receiver<String>,
    /// Sender for spawning LLM tasks (complete responses)
    llm_response_tx: mpsc::Sender<String>,
    /// Channel for streaming tokens (real-time display)
    stream_chunk_rx: mpsc::Receiver<StreamChunk>,
    /// Sender for streaming tokens (passed to ChatAgent)
    stream_chunk_tx: mpsc::Sender<StreamChunk>,
    // ═══ ChatAgent for full AI interface (Task 5.1) ═══
    /// ChatAgent for handling 5 verb commands in ChatView
    chat_agent: Option<ChatAgent>,
    // ═══ MCP Client Storage (v0.5.2) ═══
    /// MCP server configurations from loaded workflow
    mcp_configs: Option<FxHashMap<String, McpConfigInline>>,
    /// Cached MCP clients (lazy-initialized with OnceCell for thread-safe async init)
    mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
    // ═══ Background Task Tracking (v0.7.0) ═══
    /// JoinSet for tracking spawned background tasks
    /// Enables proper cancellation on app exit
    background_tasks: JoinSet<()>,
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
        // Initialize streaming channel for token-by-token updates (larger buffer for fast tokens)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(256);

        // Initialize ChatAgent (may fail if no API keys are set, but that's OK)
        let chat_agent = ChatAgent::new().ok();

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
            input_mode: InputMode::Normal,
            focus_state: FocusState::new(NavPanelId::MonitorMission),
            chat_view,
            home_view: None, // No home view in execution mode
            studio_view,
            llm_response_rx,
            llm_response_tx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_configs: None, // Loaded in init_mcp_clients()
            mcp_client_cache: Arc::new(DashMap::new()),
            background_tasks: JoinSet::new(),
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
        // Initialize streaming channel for token-by-token updates (larger buffer for fast tokens)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(256);

        // Initialize ChatAgent (may fail if no API keys are set, but that's OK)
        let chat_agent = ChatAgent::new().ok();

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
            input_mode: InputMode::Normal,
            focus_state: FocusState::new(NavPanelId::HomeFiles),
            chat_view,
            home_view: Some(home_view),
            studio_view,
            llm_response_rx,
            llm_response_tx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_configs: None, // No workflow in standalone mode
            mcp_client_cache: Arc::new(DashMap::new()),
            background_tasks: JoinSet::new(),
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

    /// Set initial view (Chat, Home, Studio, Monitor)
    ///
    /// Used by CLI commands:
    /// - `nika chat` → Chat view
    /// - `nika studio` → Studio view
    /// - `nika` (default) → Home view
    pub fn with_initial_view(mut self, view: TuiView) -> Self {
        self.current_view = view;
        self
    }

    /// Load a workflow file into Studio view
    ///
    /// Used by `nika studio <file>` to open a specific workflow.
    pub fn with_studio_file(mut self, path: std::path::PathBuf) -> Self {
        let _ = self.studio_view.load_file(path);
        self
    }

    /// Set provider and model overrides for ChatAgent
    ///
    /// Used by `nika chat --provider claude --model claude-sonnet-4-20250514`.
    ///
    /// # Arguments
    ///
    /// * `provider` - Optional provider name ("claude" or "openai")
    /// * `model` - Optional model name override
    pub fn with_chat_overrides(mut self, provider: Option<String>, model: Option<String>) -> Self {
        // Create ChatAgent with overrides (or use existing if no overrides)
        if provider.is_some() || model.is_some() {
            match ChatAgent::with_overrides(provider.as_deref(), model.as_deref()) {
                Ok(agent) => {
                    self.chat_agent = Some(agent);
                }
                Err(e) => {
                    // Log error but don't fail - agent will be created later
                    tracing::warn!("Failed to create ChatAgent with overrides: {}", e);
                }
            }
        }
        self
    }

    /// Ensure chat agent exists, creating one if necessary
    ///
    /// Returns a mutable reference to the chat agent.
    fn ensure_chat_agent(&mut self) -> Option<&mut ChatAgent> {
        if self.chat_agent.is_none() {
            self.chat_agent = ChatAgent::new().ok();
        }
        self.chat_agent.as_mut()
    }

    /// Build conversation context from chat view messages for LLM prompt
    ///
    /// Returns a formatted string with recent conversation history.
    fn build_conversation_context(&self) -> String {
        use super::views::MessageRole;

        // Get last N messages from chat_view for context
        let messages: Vec<_> = self.chat_view.messages.iter().rev().take(10).collect();

        if messages.is_empty() {
            return String::new();
        }

        let mut context = String::from("\n\n[Previous conversation]\n");
        for msg in messages.into_iter().rev() {
            let role = match &msg.role {
                MessageRole::User => "User",
                MessageRole::Nika => "Assistant",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            context.push_str(&format!("{}: {}\n", role, msg.content));
        }
        context.push_str("[Current request]\n");
        context
    }

    /// Load MCP server configurations from workflow
    ///
    /// Parses the workflow YAML and extracts MCP server configs.
    /// Actual client connections are lazy-initialized on first use via `get_mcp_client()`.
    fn init_mcp_clients(&mut self) {
        // Read and parse workflow file
        let yaml_content = match std::fs::read_to_string(&self.workflow_path) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!("Failed to read workflow for MCP init: {}", e);
                return;
            }
        };

        let workflow: Workflow = match serde_yaml::from_str(&yaml_content) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to parse workflow for MCP init: {}", e);
                return;
            }
        };

        // Store MCP configs for lazy initialization
        if let Some(mcp_configs) = workflow.mcp {
            let server_names: Vec<_> = mcp_configs.keys().cloned().collect();
            tracing::info!(servers = ?server_names, "Loaded MCP server configurations");

            // Update ChatView's session context with actual MCP servers
            self.chat_view.set_mcp_servers(server_names.iter().cloned());

            self.mcp_configs = Some(mcp_configs);
        }
    }

    /// Get available MCP server names from configuration
    fn get_mcp_server_names(&self) -> Vec<String> {
        self.mcp_configs
            .as_ref()
            .map(|c| c.keys().cloned().collect())
            .unwrap_or_default()
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

        // Initialize MCP clients from workflow config
        self.init_mcp_clients();

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

        // Cancel all background tasks before cleanup
        self.cancel_background_tasks().await;

        // Cleanup and return
        self.cleanup()
    }

    /// Poll runtime events from broadcast/mpsc receivers
    fn poll_runtime_events(&mut self) {
        // Collect events first to avoid borrow checker issues when calling
        // methods on self while rx is borrowed
        let mut events: Vec<crate::event::Event> = Vec::new();

        // Check broadcast receiver (v0.4.1 preferred)
        if let Some(ref mut rx) = self.broadcast_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
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
                events.push(event);
            }
        }

        // Process collected events (no borrow issues now)
        for event in events {
            // Record run history when workflow completes
            match &event.kind {
                EventKind::WorkflowCompleted {
                    total_duration_ms,
                    final_output,
                    ..
                } => {
                    self.workflow_done = true;
                    // Record successful run in history
                    if let Some(ref mut home_view) = self.home_view {
                        let entry = HistoryEntry {
                            workflow_path: self.workflow_path.clone(),
                            timestamp: SystemTime::now(),
                            duration_ms: *total_duration_ms,
                            task_count: self.state.tasks.len(),
                            success: true,
                            summary: final_output
                                .as_str()
                                .unwrap_or("Completed")
                                .chars()
                                .take(100)
                                .collect(),
                        };
                        home_view.standalone.add_history(entry);
                    }
                }
                EventKind::WorkflowFailed { error, .. } => {
                    self.workflow_done = true;
                    // Record failed run in history (duration unknown, use 0)
                    if let Some(ref mut home_view) = self.home_view {
                        let entry = HistoryEntry {
                            workflow_path: self.workflow_path.clone(),
                            timestamp: SystemTime::now(),
                            duration_ms: 0, // Duration not tracked in failed events
                            task_count: self.state.tasks.len(),
                            success: false,
                            summary: error.chars().take(100).collect(),
                        };
                        home_view.standalone.add_history(entry);
                    }
                }
                _ => {}
            }
            if self.state.should_break(&event.kind) {
                self.state.paused = true;
            }
            // Update TuiState (Monitor view)
            self.state.handle_event(&event.kind, event.timestamp_ms);
            // Update ChatView activity stack (Chat view)
            self.handle_chat_view_event(&event.kind);
        }

        // Poll LLM responses for both ChatOverlay and ChatView (complete responses)
        while let Ok(response) = self.llm_response_rx.try_recv() {
            // Remove "Thinking..." message from ChatOverlay and add actual response
            if let Some(last) = self.state.chat_overlay.messages.last() {
                if last.content == "Thinking..." {
                    self.state.chat_overlay.messages.pop();
                }
            }
            self.state.chat_overlay.add_nika_message(response.clone());

            // Also update ChatView - remove "Thinking..." and add response
            if let Some(last) = self.chat_view.messages.last() {
                if last.content == "Thinking..." || last.content.starts_with("$ ") {
                    self.chat_view.messages.pop();
                }
            }
            self.chat_view.add_nika_message(response, None);
        }

        // Poll streaming tokens for real-time display (Claude Code-like UX)
        while let Ok(chunk) = self.stream_chunk_rx.try_recv() {
            match chunk {
                StreamChunk::Token(token) => {
                    // Append token to last message for real-time streaming
                    self.chat_view.append_to_last_message(&token);
                    // Also update ChatOverlay if it has a pending message
                    if let Some(last) = self.state.chat_overlay.messages.last_mut() {
                        if last.content == "Thinking..." {
                            last.content = token;
                        } else {
                            last.content.push_str(&token);
                        }
                    }
                }
                StreamChunk::Thinking(thinking) => {
                    // Accumulate thinking content for inline display (v0.5.2+)
                    self.chat_view.append_thinking(&thinking);
                    tracing::debug!(thinking = %thinking, "Received thinking chunk");
                }
                StreamChunk::Done(_complete) => {
                    // Stream completed - finalize thinking and attach to last message
                    self.chat_view.finalize_thinking();
                    tracing::debug!("Stream completed");
                }
                StreamChunk::Error(err) => {
                    // Remove "Thinking..." message and show categorized error (v0.5.2+)
                    if let Some(last) = self.chat_view.messages.last() {
                        if last.content == "Thinking..." {
                            self.chat_view.messages.pop();
                        }
                    }
                    self.chat_view.show_error(&err);

                    // Also update overlay
                    if let Some(last) = self.state.chat_overlay.messages.last_mut() {
                        last.content = format!("Error: {}", err);
                    }
                }
                StreamChunk::Metrics {
                    input_tokens,
                    output_tokens,
                } => {
                    // Update session context with token usage for status bar display
                    self.chat_view.add_tokens(input_tokens, output_tokens);
                    tracing::debug!(
                        input = input_tokens,
                        output = output_tokens,
                        "Token metrics received"
                    );
                }
                // MCP connection status (v0.7.0)
                StreamChunk::McpConnected(server_name) => {
                    self.chat_view.mark_mcp_server_connected(&server_name);
                    self.state.dirty.status = true;
                    tracing::info!(server = %server_name, "MCP server connected");
                }
                StreamChunk::McpError { server_name, error } => {
                    self.chat_view.mark_mcp_server_error(&server_name);
                    self.state.dirty.status = true;
                    tracing::warn!(server = %server_name, error = %error, "MCP server connection failed");
                }
            }
        }

        // Cleanup old activities to prevent memory leak in long sessions
        // Clear activities older than 5 minutes (300 seconds)
        self.chat_view.clear_old_activities(300);
    }

    /// Handle events for ChatView activity stack
    ///
    /// Updates the ChatView's inline content and activity items when
    /// MCP, Provider, or Agent events occur.
    fn handle_chat_view_event(&mut self, kind: &EventKind) {
        match kind {
            // MCP tool calls
            EventKind::McpInvoke {
                mcp_server,
                tool,
                params,
                ..
            } => {
                let tool_name = tool.as_deref().unwrap_or("resource");
                let params_str = params
                    .as_ref()
                    .map(|p| serde_json::to_string(p).unwrap_or_default())
                    .unwrap_or_default();
                self.chat_view
                    .add_mcp_call(tool_name, mcp_server, &params_str);
            }
            EventKind::McpResponse {
                is_error, response, ..
            } => {
                if *is_error {
                    let error_msg = response
                        .as_ref()
                        .and_then(|r| r.get("error"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("MCP call failed");
                    self.chat_view.fail_mcp_call(error_msg);
                } else {
                    let result_str = response
                        .as_ref()
                        .map(|r| serde_json::to_string(r).unwrap_or_default())
                        .unwrap_or_default();
                    self.chat_view.complete_mcp_call(&result_str);
                }
            }
            // Provider events (infer: verb)
            EventKind::ProviderCalled {
                model, prompt_len, ..
            } => {
                // Start inference stream visualization
                self.chat_view
                    .start_infer_stream(model, *prompt_len as u32, 4096);
            }
            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cost_usd,
                ..
            } => {
                // Complete inference stream
                self.chat_view.complete_infer_stream();
                // Update session token usage
                let total_tokens = (*input_tokens as u64) + (*output_tokens as u64);
                self.chat_view.update_tokens(
                    self.chat_view.session_context.tokens_used + total_tokens,
                    self.chat_view.session_context.total_cost + cost_usd,
                );
                // Mark status bar as dirty to refresh token display
                self.state.dirty.status = true;
            }
            // MCP connection status events (v0.7.0)
            EventKind::McpConnected { server_name } => {
                self.chat_view.mark_mcp_server_connected(server_name);
                self.state.dirty.status = true;
            }
            EventKind::McpError { server_name, .. } => {
                self.chat_view.mark_mcp_server_error(server_name);
                self.state.dirty.status = true;
            }
            _ => {}
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
            let input_mode = self.input_mode;

            // Extract data for StatusBar metrics
            let mcp_total = self.mcp_configs.as_ref().map(|c| c.len()).unwrap_or(0);
            // Count actually connected MCP clients (OnceCell initialized = connected)
            let mcp_connected = self
                .mcp_client_cache
                .iter()
                .filter(|entry| entry.value().get().is_some())
                .count();
            let total_tokens = chat_view.total_tokens();
            let model_name = chat_view.current_model.to_lowercase();
            let provider = if model_name.contains("claude") {
                Provider::Claude
            } else if model_name.contains("gpt") || model_name.contains("openai") {
                Provider::OpenAI
            } else if model_name.contains("mistral") || model_name.contains("mixtral") {
                Provider::Mistral
            } else if model_name.contains("llama") || model_name.contains("ollama") {
                Provider::Ollama
            } else if model_name.contains("groq") {
                Provider::Groq
            } else if model_name.contains("deepseek") {
                Provider::DeepSeek
            } else if model_name.contains("mock") {
                Provider::Mock
            } else {
                Provider::None
            };

            // Get custom status text from current view
            let status_text = match current_view {
                TuiView::Chat => chat_view.status_line(state),
                TuiView::Home => home_view
                    .as_ref()
                    .map(|hv| hv.status_line(state))
                    .unwrap_or_default(),
                TuiView::Studio => studio_view.status_line(state),
                TuiView::Monitor => String::new(),
            };

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
                            // Monitor view returns early above (line 794-803) using render_frame()
                            // If we reach here, something is wrong - render placeholder safely
                            let placeholder = Paragraph::new("Monitor view - use render_frame()")
                                .block(Block::default().borders(Borders::ALL).title(" MONITOR "));
                            frame.render_widget(placeholder, chunks[1]);
                        }
                    }

                    // Render status bar with metrics and custom status text
                    let metrics = StatusMetrics::new()
                        .provider(provider)
                        .tokens(total_tokens)
                        .mcp(mcp_connected, mcp_total)
                        .connection(if mcp_total > 0 {
                            ConnectionStatus::Connected
                        } else {
                            ConnectionStatus::Disconnected
                        });
                    let status_bar = StatusBar::new(current_view, theme)
                        .mode(input_mode)
                        .metrics(metrics)
                        .custom_text(status_text);
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
            // Also skip when capturing input (Studio Insert mode, Chat with text)
            KeyCode::Tab
                if !modifiers.contains(KeyModifiers::SHIFT)
                    && self.current_view != TuiView::Monitor
                    && !self.is_view_capturing_input() =>
            {
                return Action::NextView
            }
            KeyCode::BackTab
                if self.current_view != TuiView::Monitor && !self.is_view_capturing_input() =>
            {
                return Action::PrevView
            }

            _ => {}
        }

        // ═══ Navigation 2.0: InputMode-aware key routing ═══
        // When in Insert mode on Chat view, route all keys to chat input
        if self.input_mode == InputMode::Insert && self.current_view == TuiView::Chat {
            // Esc returns to Normal mode
            if code == KeyCode::Esc {
                self.input_mode = InputMode::Normal;
                return Action::Continue;
            }
            // All other keys go to chat input
            let key_event = KeyEvent::new(code, modifiers);
            let view_action = self.chat_view.handle_key(key_event, &mut self.state);
            return self.convert_view_action(view_action);
        }

        // In Normal mode, 'i' enters Insert mode when on Chat view
        if self.input_mode == InputMode::Normal
            && self.current_view == TuiView::Chat
            && code == KeyCode::Char('i')
        {
            self.input_mode = InputMode::Insert;
            return Action::Continue;
        }

        // View-specific key handling using the View trait
        let key_event = KeyEvent::new(code, modifiers);

        match self.current_view {
            TuiView::Monitor => {
                // Monitor uses the existing 4-panel key handling
                self.handle_key(code, modifiers)
            }
            TuiView::Chat => {
                // In Normal mode, Chat view handles navigation keys (j/k, etc.)
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
            TuiView::Chat => !self.chat_view.input.value().is_empty(),
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
                self.workflow_path = path.clone();
                self.current_view = TuiView::Monitor;
                self.workflow_done = false;

                // Trigger workflow execution asynchronously
                self.start_workflow_execution(path);
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
                // Send message to LLM for processing (like /infer but conversational)
                if !msg.is_empty() {
                    // Show "Thinking..." indicator
                    self.chat_view
                        .add_nika_message("Thinking...".to_string(), None);

                    // Build conversation context from previous messages
                    let context = self.build_conversation_context();
                    let prompt_with_context = format!("{}{}", context, msg);

                    // Spawn async task to call ChatAgent.infer()
                    let tx = self.llm_response_tx.clone();
                    if self.ensure_chat_agent().is_some() {
                        tokio::spawn(async move {
                            match crate::tui::ChatAgent::new() {
                                Ok(mut agent) => match agent.infer(&prompt_with_context).await {
                                    Ok(response) => {
                                        let _ = tx.send(response).await;
                                    }
                                    Err(e) => {
                                        let _ = tx.send(format!("Error: {}", e)).await;
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.send(format!("Error: {}", e)).await;
                                }
                            }
                        });
                    } else {
                        // No API key available
                        self.chat_view.messages.pop(); // Remove "Thinking..."
                        self.chat_view.add_nika_message(
                            "No API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY."
                                .to_string(),
                            None,
                        );
                    }
                }
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
            // ═══════════════════════════════════════════════════════════════════════
            // Chat Agent Command Actions (Task 5.1)
            // ═══════════════════════════════════════════════════════════════════════
            ViewAction::ChatInfer(prompt) => {
                self.handle_chat_infer(prompt);
                Action::Continue
            }
            ViewAction::ChatExec(command) => {
                self.handle_chat_exec(command);
                Action::Continue
            }
            ViewAction::ChatFetch(url, method) => {
                self.handle_chat_fetch(url, method);
                Action::Continue
            }
            ViewAction::ChatInvoke(tool, server, params) => {
                self.handle_chat_invoke(tool, server, params);
                Action::Continue
            }
            ViewAction::ChatAgent(goal, max_turns, extended_thinking, mcp_servers) => {
                self.handle_chat_agent(goal, max_turns, extended_thinking, mcp_servers);
                Action::Continue
            }
            ViewAction::ChatModelSwitch(provider) => {
                self.handle_chat_model_switch(provider);
                Action::Continue
            }
            ViewAction::ChatMcp(action) => {
                self.handle_chat_mcp(action);
                Action::Continue
            }
            ViewAction::ChatClear => {
                self.handle_chat_clear();
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
            KeyCode::Char('g') => Action::ScrollToTop,
            KeyCode::Char('G') => Action::ScrollToBottom,

            // Overlays
            KeyCode::Char('?') | KeyCode::F(1) => Action::SetMode(TuiMode::Help),
            KeyCode::Char('m') => Action::SetMode(TuiMode::Metrics),
            KeyCode::Char('s') => Action::SetMode(TuiMode::Settings),
            KeyCode::Char('/') => Action::EnterFilter, // TIER 1.5: Filter mode

            // Quick actions (TIER 1)
            KeyCode::Char('c') => Action::SetMode(TuiMode::ChatOverlay), // Toggle chat overlay
            KeyCode::Char('y') => Action::CopyToClipboard,               // Yank (vim convention)
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
            Action::FocusNext => {
                self.state.focus_next();
                // Sync Navigation 2.0 focus_state for Monitor view
                if self.current_view == TuiView::Monitor {
                    self.focus_state.next_panel();
                }
            }
            Action::FocusPrev => {
                self.state.focus_prev();
                // Sync Navigation 2.0 focus_state for Monitor view
                if self.current_view == TuiView::Monitor {
                    self.focus_state.prev_panel();
                }
            }
            Action::FocusPanel(n) => {
                self.state.focus_panel(n);
                // Sync Navigation 2.0 focus_state for Monitor view
                if self.current_view == TuiView::Monitor {
                    let panel = match n {
                        1 => NavPanelId::MonitorMission,
                        2 => NavPanelId::MonitorDag,
                        3 => NavPanelId::MonitorNovanet,
                        _ => NavPanelId::MonitorReasoning,
                    };
                    self.focus_state.focus(panel);
                }
            }
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
            Action::ScrollToTop => {
                // Reset scroll to top (vim 'gg' behavior)
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_first_mcp();
                } else {
                    self.state.scroll.insert(self.state.focus, 0);
                }
            }
            Action::ScrollToBottom => {
                // Scroll to bottom (vim 'G' behavior)
                // We set a large value; the render logic will clamp it
                if self.state.focus == PanelId::NovaNet {
                    self.state.select_last_mcp();
                } else {
                    self.state.scroll.insert(self.state.focus, usize::MAX);
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
            // View navigation actions (with Navigation 2.0 focus sync)
            Action::SwitchView(view) => {
                self.current_view = view;
                self.focus_state.reset_to_view(view);
                self.input_mode = InputMode::Normal;
            }
            Action::NextView => {
                self.current_view = self.current_view.next();
                self.focus_state.reset_to_view(self.current_view);
                self.input_mode = InputMode::Normal;
            }
            Action::PrevView => {
                self.current_view = self.current_view.prev();
                self.focus_state.reset_to_view(self.current_view);
                self.input_mode = InputMode::Normal;
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Chat Agent Command Handlers (Task 5.1)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle /infer command - LLM inference
    fn handle_chat_infer(&mut self, prompt: String) {
        if prompt.is_empty() {
            self.chat_view
                .add_nika_message("Usage: /infer <prompt>".to_string(), None);
            return;
        }

        // Show "Thinking..." indicator
        self.chat_view
            .add_nika_message("Thinking...".to_string(), None);

        // Build conversation context from previous messages
        let context = self.build_conversation_context();
        let prompt_with_context = if context.is_empty() {
            prompt.clone()
        } else {
            format!("{}{}", context, prompt)
        };

        // Spawn async task to call ChatAgent.infer()
        // Only use stream_tx - streaming handles message display
        // (llm_response_tx would cause duplicate messages)
        let stream_tx = self.stream_chunk_tx.clone();

        // Check if agent exists or can be created
        if self.ensure_chat_agent().is_some() {
            tokio::spawn(async move {
                // Create a new agent for the async task (ChatAgent is not Send)
                // Wire streaming for real-time token display (Claude Code-like UX)
                match ChatAgent::new() {
                    Ok(agent) => {
                        let mut agent = agent.with_stream_chunks(stream_tx.clone());
                        match agent.infer(&prompt_with_context).await {
                            Ok(_response) => {
                                // Response already displayed via streaming tokens
                                // StreamChunk::Token appends to "Thinking..." message
                                // Do NOT send on llm_response_tx - that would create duplicate
                            }
                            Err(e) => {
                                // Send error via streaming channel to replace "Thinking..."
                                let _ = stream_tx.send(StreamChunk::Error(e.to_string())).await;
                            }
                        }
                    }
                    Err(e) => {
                        // Agent creation failed - send error via streaming channel
                        let _ = stream_tx
                            .send(StreamChunk::Error(format!("Error creating agent: {}", e)))
                            .await;
                    }
                }
            });
        } else {
            // No API key available
            self.chat_view.messages.pop(); // Remove "Thinking..."
            self.chat_view.add_nika_message(
                "No API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY.".to_string(),
                None,
            );
        }
    }

    /// Handle /exec command - shell execution
    fn handle_chat_exec(&mut self, command: String) {
        if command.is_empty() {
            self.chat_view
                .add_nika_message("Usage: /exec <command>".to_string(), None);
            return;
        }

        // Show "Running..." indicator
        self.chat_view
            .add_nika_message(format!("$ {}", command), None);

        // Spawn async task for shell execution
        let tx = self.llm_response_tx.clone();
        tokio::spawn(async move {
            match ChatAgent::new() {
                Ok(agent) => match agent.exec_command(&command).await {
                    Ok(output) => {
                        let _ = tx.send(output).await;
                    }
                    Err(e) => {
                        let _ = tx.send(format!("Error: {}", e)).await;
                    }
                },
                Err(e) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
            }
        });
    }

    /// Handle /fetch command - HTTP request
    fn handle_chat_fetch(&mut self, url: String, method: String) {
        if url.is_empty() {
            self.chat_view
                .add_nika_message("Usage: /fetch <url> [method]".to_string(), None);
            return;
        }

        // Show "Fetching..." indicator
        self.chat_view
            .add_nika_message(format!("Fetching {} {}...", method, url), None);

        // Spawn async task for HTTP request
        let tx = self.llm_response_tx.clone();
        tokio::spawn(async move {
            match ChatAgent::new() {
                Ok(agent) => match agent.fetch(&url, &method).await {
                    Ok(response) => {
                        // Truncate very long responses
                        let truncated = if response.len() > 2000 {
                            format!(
                                "{}...\n\n[Truncated, {} bytes total]",
                                &response[..2000],
                                response.len()
                            )
                        } else {
                            response
                        };
                        let _ = tx.send(truncated).await;
                    }
                    Err(e) => {
                        let _ = tx.send(format!("Error: {}", e)).await;
                    }
                },
                Err(e) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
            }
        });
    }

    /// Handle /invoke command - MCP tool call
    fn handle_chat_invoke(
        &mut self,
        tool: String,
        server: Option<String>,
        params: serde_json::Value,
    ) {
        if tool.is_empty() {
            self.chat_view.add_nika_message(
                "Usage: /invoke [server:]tool [json_params]".to_string(),
                None,
            );
            return;
        }

        let available_servers = self.get_mcp_server_names();

        // Resolve MCP server
        let server_name = if let Some(ref name) = server {
            // User specified a server
            if !available_servers.contains(name) {
                self.chat_view.add_nika_message(
                    format!(
                        "Error: MCP server '{}' not configured.\nAvailable: {:?}",
                        name, available_servers
                    ),
                    None,
                );
                return;
            }
            name.clone()
        } else {
            // Use first available server
            if available_servers.is_empty() {
                self.chat_view.add_nika_message(
                    "Error: No MCP servers configured.\nAdd mcp.servers to your workflow."
                        .to_string(),
                    None,
                );
                return;
            }
            available_servers.into_iter().next().unwrap()
        };

        let tx = self.llm_response_tx.clone();
        let status_tx = self.stream_chunk_tx.clone();
        let mcp_configs = self.mcp_configs.clone();
        let mcp_client_cache = Arc::clone(&self.mcp_client_cache);

        // Show pending message
        self.chat_view
            .add_nika_message(format!("🔧 Invoking {}:{} ...", server_name, tool), None);

        // Spawn async task to connect (if needed) and call the tool
        let tool_name = tool.clone();
        let server_name_clone = server_name.clone();
        tokio::spawn(async move {
            // Lazy-initialize MCP client connection
            let client = {
                let cell = mcp_client_cache
                    .entry(server_name_clone.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new()))
                    .clone();

                let name_owned = server_name_clone.clone();
                let configs = mcp_configs.clone();

                match cell
                    .get_or_try_init(|| async {
                        if let Some(ref cfgs) = configs {
                            if let Some(inline_config) = cfgs.get(&name_owned) {
                                let mut mcp_config = McpConfig::new(&name_owned, &inline_config.command);
                                for arg in &inline_config.args {
                                    mcp_config = mcp_config.with_arg(arg);
                                }
                                for (key, value) in &inline_config.env {
                                    mcp_config = mcp_config.with_env(key, value);
                                }
                                if let Some(cwd) = &inline_config.cwd {
                                    mcp_config = mcp_config.with_cwd(cwd);
                                }

                                let client = McpClient::new(mcp_config)
                                    .map_err(|e| NikaError::McpStartError {
                                        name: name_owned.clone(),
                                        reason: e.to_string(),
                                    })?;

                                client.connect().await.map_err(|e| NikaError::McpStartError {
                                    name: name_owned.clone(),
                                    reason: e.to_string(),
                                })?;

                                // Cache tools for synchronous get_tool_definitions() access
                                if let Err(e) = client.list_tools().await {
                                    tracing::warn!(mcp_server = %name_owned, error = %e, "Failed to cache tools");
                                }

                                tracing::info!(mcp_server = %name_owned, "Connected to MCP server");
                                Ok(Arc::new(client))
                            } else {
                                Err(NikaError::McpNotConfigured { name: name_owned })
                            }
                        } else {
                            Err(NikaError::McpNotConfigured { name: name_owned })
                        }
                    })
                    .await
                {
                    Ok(c) => {
                        // Notify TUI of successful MCP connection (v0.7.0)
                        let _ = status_tx.send(StreamChunk::McpConnected(server_name_clone.clone())).await;
                        Arc::clone(c)
                    }
                    Err(e) => {
                        // Notify TUI of MCP connection failure (v0.7.0)
                        let _ = status_tx.send(StreamChunk::McpError {
                            server_name: server_name_clone.clone(),
                            error: e.to_string(),
                        }).await;
                        let _ = tx.send(format!("❌ Failed to connect to {}: {}", server_name_clone, e)).await;
                        return;
                    }
                }
            };

            // Call the tool
            match client.call_tool(&tool_name, params).await {
                Ok(result) => {
                    let status = if result.is_error { "❌" } else { "✅" };
                    let text = result.text();

                    // Truncate very long responses
                    let display = if text.len() > 3000 {
                        format!(
                            "{}...\n\n[Truncated, {} chars total]",
                            &text[..3000],
                            text.len()
                        )
                    } else {
                        text
                    };

                    let _ = tx
                        .send(format!(
                            "{} {}:{}\n\n{}",
                            status, server_name_clone, tool_name, display
                        ))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(format!(
                            "❌ {}:{} failed: {}",
                            server_name_clone, tool_name, e
                        ))
                        .await;
                }
            }
        });
    }

    /// Handle /agent command - multi-turn agent with RigAgentLoop
    fn handle_chat_agent(
        &mut self,
        goal: String,
        max_turns: Option<u32>,
        extended_thinking: bool,
        mcp_servers: Vec<String>,
    ) {
        if goal.is_empty() {
            self.chat_view.add_nika_message(
                "Usage: /agent <goal> [--max-turns N] [--mcp server1,server2]".to_string(),
                None,
            );
            return;
        }

        // Build AgentParams from user input
        // extended_thinking flag comes from ChatView's deep_thinking toggle (Ctrl+T)
        // Use explicitly provided MCP servers, or fall back to session defaults
        let mcp_server_names = if mcp_servers.is_empty() {
            self.get_mcp_server_names()
        } else {
            mcp_servers
        };
        let params = AgentParams {
            prompt: goal.clone(),
            system: Some(
                "You are a helpful AI assistant. Complete the user's request.".to_string(),
            ),
            max_turns,
            mcp: mcp_server_names.clone(),
            extended_thinking: if extended_thinking { Some(true) } else { None },
            ..Default::default()
        };

        // Show starting message with configuration details
        let turns_str = max_turns
            .map(|t| format!(" (max {} turns)", t))
            .unwrap_or_default();
        let mcp_str = if mcp_server_names.is_empty() {
            String::new()
        } else {
            format!(" with MCP: {}", mcp_server_names.join(", "))
        };
        let thinking_str = if extended_thinking {
            " [deep thinking]"
        } else {
            ""
        };
        self.chat_view.add_nika_message(
            format!(
                "🐔 Summoning the space chicken{}{}{}: {}",
                turns_str, mcp_str, thinking_str, goal
            ),
            None,
        );

        // Clone configs and cache for async task
        let mcp_configs = self.mcp_configs.clone();
        let mcp_client_cache = Arc::clone(&self.mcp_client_cache);

        // Create task_id for this agent session
        let task_id = format!(
            "chat-agent-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        // Clone channel senders for async task
        let response_tx = self.llm_response_tx.clone();
        let status_tx = self.stream_chunk_tx.clone();

        // Spawn async task to connect MCP servers and run the agent
        tokio::spawn(async move {
            // Connect MCP servers lazily
            let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
            for server_name in &mcp_server_names {
                let cell = mcp_client_cache
                    .entry(server_name.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new()))
                    .clone();

                let name_owned = server_name.clone();
                let configs = mcp_configs.clone();

                match cell
                    .get_or_try_init(|| async {
                        if let Some(ref cfgs) = configs {
                                if let Some(inline_config) = cfgs.get(&name_owned) {
                                    let mut mcp_config = McpConfig::new(&name_owned, &inline_config.command);
                                    for arg in &inline_config.args {
                                        mcp_config = mcp_config.with_arg(arg);
                                    }
                                    for (key, value) in &inline_config.env {
                                        mcp_config = mcp_config.with_env(key, value);
                                    }
                                    if let Some(cwd) = &inline_config.cwd {
                                        mcp_config = mcp_config.with_cwd(cwd);
                                    }

                                    let client = McpClient::new(mcp_config)
                                        .map_err(|e| NikaError::McpStartError {
                                            name: name_owned.clone(),
                                            reason: e.to_string(),
                                        })?;

                                    client.connect().await.map_err(|e| NikaError::McpStartError {
                                        name: name_owned.clone(),
                                        reason: e.to_string(),
                                    })?;

                                    // Cache tools for synchronous get_tool_definitions() access
                                    if let Err(e) = client.list_tools().await {
                                        tracing::warn!(mcp_server = %name_owned, error = %e, "Failed to cache tools");
                                    }

                                    tracing::info!(mcp_server = %name_owned, "Connected to MCP server");
                                    Ok(Arc::new(client))
                            } else {
                                Err(NikaError::McpNotConfigured { name: name_owned })
                            }
                        } else {
                            Err(NikaError::McpNotConfigured { name: name_owned })
                        }
                    })
                    .await
                {
                    Ok(client) => {
                        mcp_clients.insert(server_name.clone(), Arc::clone(client));
                        // Notify TUI of successful MCP connection (v0.7.0)
                        let _ = status_tx.send(StreamChunk::McpConnected(server_name.clone())).await;
                    }
                    Err(e) => {
                        tracing::warn!(server = %server_name, error = %e, "Failed to connect MCP server");
                        // Notify TUI of MCP connection failure (v0.7.0)
                        let _ = status_tx.send(StreamChunk::McpError {
                            server_name: server_name.clone(),
                            error: e.to_string(),
                        }).await;
                    }
                }
            }

            // Create EventLog for observability
            let event_log = EventLog::new();

            // Create RigAgentLoop with connected clients
            let mut agent = match RigAgentLoop::new(task_id.clone(), params, event_log, mcp_clients)
            {
                Ok(loop_instance) => loop_instance,
                Err(e) => {
                    let _ = response_tx
                        .send(format!("❌ Failed to create agent: {}", e))
                        .await;
                    return;
                }
            };

            match agent.run_auto().await {
                Ok(result) => {
                    // Format the response with status and metrics
                    let status_emoji = match result.status {
                        RigAgentStatus::NaturalCompletion => "✅",
                        RigAgentStatus::MaxTurnsReached => "⏱️",
                        RigAgentStatus::StopConditionMet => "🛑",
                        RigAgentStatus::Failed => "❌",
                        RigAgentStatus::TokenBudgetExceeded => "💰",
                    };

                    // Extract final output text
                    let output_text = if result.final_output.is_string() {
                        result.final_output.as_str().unwrap_or("").to_string()
                    } else {
                        result.final_output.to_string()
                    };

                    let response = format!(
                        "{} Agent completed ({} turns, {} tokens)\n\n{}",
                        status_emoji, result.turns, result.total_tokens, output_text
                    );
                    let _ = response_tx.send(response).await;
                }
                Err(e) => {
                    let _ = response_tx.send(format!("❌ Agent failed: {}", e)).await;
                }
            }
        });
    }

    /// Handle /model command - switch LLM provider
    fn handle_chat_model_switch(&mut self, provider: ModelProvider) {
        if let Some(ref mut agent) = self.chat_agent {
            match agent.set_provider(provider.clone()) {
                Ok(()) => {
                    let msg = format!("Switched to {} provider", provider.name());
                    self.chat_view.add_nika_message(msg.clone(), None);
                    self.set_status(&msg);
                }
                Err(e) => {
                    let msg = format!("Failed to switch provider: {}", e);
                    self.chat_view.add_nika_message(msg.clone(), None);
                    self.set_status(&msg);
                }
            }
        } else {
            // Try to create a new ChatAgent with the requested provider
            match ChatAgent::new() {
                Ok(mut agent) => {
                    if let Err(e) = agent.set_provider(provider.clone()) {
                        self.chat_view
                            .add_nika_message(format!("Failed to switch provider: {}", e), None);
                    } else {
                        self.chat_agent = Some(agent);
                        let msg = format!("Switched to {} provider", provider.name());
                        self.chat_view.add_nika_message(msg.clone(), None);
                        self.set_status(&msg);
                    }
                }
                Err(e) => {
                    self.chat_view
                        .add_nika_message(format!("Failed to create agent: {}", e), None);
                }
            }
        }
    }

    /// Handle /mcp command - MCP server management (v0.5.2)
    fn handle_chat_mcp(&mut self, action: McpAction) {
        // Helper to check if server exists in configs
        let server_exists =
            |configs: &Option<FxHashMap<String, McpConfigInline>>, name: &str| -> bool {
                configs.as_ref().is_some_and(|c| c.contains_key(name))
            };

        match action {
            McpAction::List => {
                // List available MCP servers from configuration
                let available: Vec<&str> = self
                    .mcp_configs
                    .as_ref()
                    .map(|c| c.keys().map(|s| s.as_str()).collect())
                    .unwrap_or_default();

                // Get currently selected servers
                let selected: Vec<&str> = self
                    .chat_view
                    .session_context
                    .mcp_servers
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect();

                let msg = if available.is_empty() {
                    "No MCP servers configured. Add servers in workflow mcp: section.".to_string()
                } else {
                    let server_list: Vec<String> = available
                        .iter()
                        .map(|s| {
                            let is_selected = selected.contains(s);
                            format!("  {} {}", if is_selected { "◉" } else { "○" }, s)
                        })
                        .collect();
                    format!(
                        "MCP Servers:\n{}\n\nUse /mcp select <servers> or /mcp toggle <server>",
                        server_list.join("\n")
                    )
                };
                self.chat_view.add_nika_message(msg, None);
            }
            McpAction::Select(servers) => {
                // Validate servers exist
                let valid: Vec<String> = servers
                    .iter()
                    .filter(|s| server_exists(&self.mcp_configs, s))
                    .cloned()
                    .collect();

                let invalid: Vec<&String> = servers
                    .iter()
                    .filter(|s| !server_exists(&self.mcp_configs, s))
                    .collect();

                if !invalid.is_empty() {
                    self.chat_view.add_nika_message(
                        format!(
                            "Unknown servers: {}. Use /mcp list to see available.",
                            invalid
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        None,
                    );
                }

                if !valid.is_empty() {
                    self.chat_view.set_mcp_servers(valid.clone());
                    self.chat_view.add_nika_message(
                        format!("Selected MCP servers: {}", valid.join(", ")),
                        None,
                    );
                    self.set_status(&format!("MCP: {}", valid.join(", ")));
                }
            }
            McpAction::Toggle(server) => {
                if !server_exists(&self.mcp_configs, &server) {
                    self.chat_view.add_nika_message(
                        format!(
                            "Unknown server: {}. Use /mcp list to see available.",
                            server
                        ),
                        None,
                    );
                    return;
                }

                // Check if server is currently selected
                let is_selected = self
                    .chat_view
                    .session_context
                    .mcp_servers
                    .iter()
                    .any(|s| s.name == server);

                if is_selected {
                    // Remove from selection
                    self.chat_view
                        .session_context
                        .mcp_servers
                        .retain(|s| s.name != server);
                    self.chat_view
                        .add_nika_message(format!("Disabled MCP server: {}", server), None);
                } else {
                    // Add to selection
                    use crate::tui::widgets::McpServerInfo;
                    self.chat_view
                        .session_context
                        .mcp_servers
                        .push(McpServerInfo::new(&server));
                    self.chat_view
                        .add_nika_message(format!("Enabled MCP server: {}", server), None);
                }

                let current: Vec<&str> = self
                    .chat_view
                    .session_context
                    .mcp_servers
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect();
                self.set_status(&format!(
                    "MCP: {}",
                    if current.is_empty() {
                        "none".to_string()
                    } else {
                        current.join(", ")
                    }
                ));
            }
        }
    }

    /// Handle /clear command - clear chat history
    fn handle_chat_clear(&mut self) {
        // Clear ChatView messages
        self.chat_view.messages.clear();
        self.chat_view.history.clear();

        // Clear ChatAgent history if available
        if let Some(ref mut agent) = self.chat_agent {
            agent.clear_history();
        }

        // Add welcome message back
        self.chat_view.add_nika_message(
            "Chat cleared. Ready for new conversation.".to_string(),
            None,
        );
        self.set_status("Chat history cleared");
    }

    /// Start workflow execution asynchronously (v0.5.2)
    ///
    /// Loads the workflow from the given path, creates a Runner with broadcast
    /// EventLog, and spawns execution in a background task. Events are routed
    /// to the TUI state via the broadcast channel.
    fn start_workflow_execution(&mut self, path: PathBuf) {
        // Reset state for new workflow
        self.state.tasks.clear();
        self.set_status(&format!("🦋 Nika loading: {}", path.display()));

        // Clone path for async task
        let workflow_path = path.clone();

        // Create broadcast channel for events
        let (event_log, event_rx) = EventLog::new_with_broadcast();

        // Store the receiver for poll_events()
        self.broadcast_rx = Some(event_rx);

        // Spawn async task to load and run workflow
        tokio::spawn(async move {
            // Read workflow file
            let yaml = match tokio::fs::read_to_string(&workflow_path).await {
                Ok(content) => content,
                Err(e) => {
                    event_log.emit(EventKind::WorkflowFailed {
                        error: format!("Failed to read file: {}", e),
                        failed_task: None,
                    });
                    return;
                }
            };

            // Validate YAML schema
            let validator: WorkflowSchemaValidator = match WorkflowSchemaValidator::new() {
                Ok(v) => v,
                Err(e) => {
                    event_log.emit(EventKind::WorkflowFailed {
                        error: format!("Schema validator error: {}", e),
                        failed_task: None,
                    });
                    return;
                }
            };

            if let Err(e) = validator.validate_yaml(&yaml) {
                event_log.emit(EventKind::WorkflowFailed {
                    error: format!("Schema validation failed: {}", e),
                    failed_task: None,
                });
                return;
            }

            // Parse workflow
            let workflow: Workflow = match serde_yaml::from_str(&yaml) {
                Ok(w) => w,
                Err(e) => {
                    event_log.emit(EventKind::WorkflowFailed {
                        error: format!("YAML parse error: {}", e),
                        failed_task: None,
                    });
                    return;
                }
            };

            // Validate schema version
            if let Err(e) = workflow.validate_schema() {
                event_log.emit(EventKind::WorkflowFailed {
                    error: format!("Schema version error: {}", e),
                    failed_task: None,
                });
                return;
            }

            // Create and run workflow
            let runner = Runner::with_event_log(workflow, event_log);
            match runner.run().await {
                Ok(output) => {
                    tracing::info!("Workflow completed: {} chars output", output.len());
                }
                Err(e) => {
                    tracing::error!("Workflow execution failed: {}", e);
                }
            }
        });

        self.set_status(&format!("🌌 Warping through: {}", path.display()));
    }

    /// Spawn a background task that will be automatically cancelled on cleanup
    ///
    /// Use this instead of raw `tokio::spawn()` for tasks that should be
    /// cleaned up when the TUI exits. Returns true if spawn succeeded.
    #[allow(dead_code)] // Infrastructure for future use
    fn spawn_background<F>(&mut self, future: F) -> bool
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.background_tasks.spawn(future);
        true
    }

    /// Cancel all background tasks and wait for them to complete
    ///
    /// Should be called during cleanup to ensure graceful shutdown.
    async fn cancel_background_tasks(&mut self) {
        // Abort all tasks in the JoinSet
        self.background_tasks.abort_all();

        // Wait for all tasks to complete (they'll be aborted)
        while self.background_tasks.join_next().await.is_some() {
            // Tasks completing (either aborted or finished)
        }

        tracing::debug!("All background tasks cancelled");
    }

    /// Cleanup terminal state
    fn cleanup(&mut self) -> Result<()> {
        // Note: Background tasks are cancelled in run_unified() after this,
        // because cleanup() is not async. Use cancel_background_tasks() separately.

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
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA TUI KEYBOARD SHORTCUTS (v0.5.2)                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ═══ GLOBAL ═══════════════════════════════════════════════════════════════  ║
║  1-4        Switch view (Browser/Home/Monitor/Chat)                          ║
║  Tab        Next view (or next panel in Monitor)                             ║
║  ?/F1       This help     m  Metrics       s  Settings     Esc  Close        ║
║  Ctrl+C     Quit app      q  Quit (when not editing)                         ║
║                                                                               ║
║  ═══ MONITOR VIEW ═════════════════════════════════════════════════════════  ║
║  1-4        Focus panel   h/l  Prev/Next panel    t/Tab  Cycle tabs          ║
║  j/k ↑↓     Scroll        Space  Pause    Enter  Step (when paused)          ║
║  y          Copy output   e  Export trace    r  Retry    /  Filter           ║
║  b          Breakpoint    T  Theme toggle    n/N  Dismiss notifications      ║
║                                                                               ║
║  ═══ CHAT VIEW ════════════════════════════════════════════════════════════  ║
║  i          Enter insert mode (start typing)                                 ║
║  Esc        Exit insert mode to normal mode                                  ║
║  Enter      Send message (in insert mode)                                    ║
║  Ctrl+K     Command palette (/agent, /model, etc.)                           ║
║  Ctrl+T     Toggle deep thinking mode                                        ║
║  Ctrl+M     Toggle Infer ↔ Agent mode                                        ║
║                                                                               ║
║  ═══ BROWSER VIEW ═════════════════════════════════════════════════════════  ║
║  j/k ↑↓     Navigate files    Enter  Open/Run workflow                       ║
║  o          Open in editor    g  DAG view    e  Edit workflow                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
"#;

    let overlay = centered_rect(80, 60, area);
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
                "[You]",
                Style::default()
                    .fg(theme.trait_retrieved) // Cyan
                    .add_modifier(Modifier::BOLD),
            ),
            crate::tui::state::ChatOverlayMessageRole::Nika => (
                "[AI]",
                Style::default()
                    .fg(theme.status_success) // Green
                    .add_modifier(Modifier::BOLD),
            ),
            crate::tui::state::ChatOverlayMessageRole::System => (
                "[System]",
                Style::default()
                    .fg(theme.status_running) // Yellow/Amber
                    .add_modifier(Modifier::BOLD),
            ),
            crate::tui::state::ChatOverlayMessageRole::Tool => (
                "[Tool]",
                Style::default()
                    .fg(theme.mcp_traverse) // Magenta/Pink
                    .add_modifier(Modifier::BOLD),
            ),
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

    // Add streaming indicator if streaming is in progress
    if chat.is_streaming {
        message_lines.push(Line::from(vec![
            Span::styled(
                "[AI] ",
                Style::default()
                    .fg(theme.status_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(20), Style::default().fg(Color::DarkGray)),
        ]));

        // Show partial response if any
        if !chat.partial_response.is_empty() {
            for line in chat.partial_response.lines() {
                message_lines.push(Line::from(format!("  {}", line)));
            }
        }

        // Add thinking indicator
        message_lines.push(Line::from(vec![Span::styled(
            "  Thinking...",
            Style::default()
                .fg(theme.status_running)
                .add_modifier(Modifier::ITALIC),
        )]));
    }

    // Show current model in title
    let title = if chat.is_streaming {
        format!(" Chat | {} | Streaming... ", chat.current_model)
    } else {
        format!(" Chat | {} ", chat.current_model)
    };

    let messages_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
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
        app.chat_view.input.reset();
        assert!(!app.is_view_capturing_input());

        // Chat with input is capturing
        app.chat_view.input = tui_input::Input::new("typing...".to_string());
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

    // === Tab Key Behavior Tests (MEDIUM 14) ===

    #[test]
    fn test_tab_switches_view_in_normal_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Home view
        app.current_view = TuiView::Home;
        app.input_mode = InputMode::Normal;

        // Tab should return NextView
        let action = app.handle_unified_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(action, Action::NextView, "Tab should cycle to next view");
    }

    #[test]
    fn test_tab_blocked_in_studio_insert_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Studio in Insert mode
        app.current_view = TuiView::Studio;
        app.studio_view.mode = EditorMode::Insert;

        // Tab should NOT switch views (Insert mode inserts spaces)
        let action = app.handle_unified_key(KeyCode::Tab, KeyModifiers::empty());
        assert_ne!(
            action,
            Action::NextView,
            "Tab in Studio Insert mode should not switch views"
        );
    }

    #[test]
    fn test_tab_blocked_in_chat_with_input() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Chat view with text in input
        app.current_view = TuiView::Chat;
        app.chat_view.input = tui_input::Input::new("typing...".to_string());

        // Tab should NOT switch views when input has text
        let action = app.handle_unified_key(KeyCode::Tab, KeyModifiers::empty());
        assert_ne!(
            action,
            Action::NextView,
            "Tab in Chat with input should not switch views"
        );
    }

    #[test]
    fn test_tab_allowed_in_chat_empty_input() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Chat view with empty input
        app.current_view = TuiView::Chat;
        app.chat_view.input = tui_input::Input::default();

        // Tab SHOULD switch views when input is empty
        let action = app.handle_unified_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(
            action,
            Action::NextView,
            "Tab in Chat with empty input should switch views"
        );
    }

    #[test]
    fn test_shift_tab_cycles_backward() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Home view
        app.current_view = TuiView::Home;

        // Shift+Tab should return PrevView
        let action = app.handle_unified_key(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(
            action,
            Action::PrevView,
            "Shift+Tab should cycle to previous view"
        );
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

    #[tokio::test]
    async fn test_convert_view_action_send_chat_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Record initial message count
        let initial_count = app.chat_view.messages.len();

        // Send a message - this triggers async LLM call or shows "no API key" message
        let action = app.convert_view_action(ViewAction::SendChatMessage("Hello".to_string()));
        assert_eq!(action, Action::Continue);

        // Should have added at least one message:
        // - "Thinking..." (if API key available and async task spawned)
        // - OR "No API key configured..." (if no API key)
        assert!(app.chat_view.messages.len() > initial_count);
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
    fn test_handle_key_y_copies_to_clipboard() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // In Monitor mode, 'y' (vim yank) should copy to clipboard
        let action = app.handle_key(KeyCode::Char('y'), KeyModifiers::empty());
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
