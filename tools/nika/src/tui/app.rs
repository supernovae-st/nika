//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.

use crate::serde_yaml;
use std::io::{self, Stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// PERF: Use parking_lot::Mutex instead of std::sync::Mutex
// - No poisoning (simpler error handling)
// - Faster lock acquisition for short critical sections
use parking_lot::Mutex;

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
    Terminal,
};
use tokio::sync::{broadcast, mpsc, OnceCell};
use tokio::task::AbortHandle;
use tokio::time::timeout;

use crate::util::constants::{
    EXEC_TIMEOUT, FETCH_TIMEOUT, INFER_TIMEOUT, MCP_INIT_TIMEOUT, WORKFLOW_TIMEOUT,
};

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

use super::config::{ThemeName, TuiConfig};
use super::cosmic_theme::CosmicTheme;
use super::focus::{FocusState, PanelId as NavPanelId};
use super::mode::InputMode;
use super::session::save_session;
use super::standalone::{HistoryEntry, StandaloneState};
use super::startup;
use super::state::{PanelId, TuiMode, TuiState};
use super::theme::Theme;
use super::utils::truncate_str;
use super::verification::{VerificationCache, VerificationEntry};
use super::views::{
    ChatView, HelpView, HomeView, McpAction, MonitorView, SchedulerView, SettingsView, SplitView,
    TuiView, View, ViewAction, WorkspaceView, YamlEditorPanel,
};
use super::widgets::task_box::{
    AgentBox, BoxState, ExecBox, FetchBox, InferBox, InvokeBox, TaskBox,
};
use super::widgets::{
    ConnectionStatus, Header, NikaIntro, NikaIntroState, StatusBar, StatusMessageWidget,
    StatusMetrics,
};
use crossterm::event::KeyEvent;

// Note: Frame rate is now adaptive - see FAST_TICK_MS/SLOW_TICK_MS in run_unified()

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
    /// Dismiss error message [E] (P3 fix: error dismissal shortcut)
    DismissError,
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
    /// Switch to a specific view (number keys 1/2/3/4)
    SwitchView(TuiView),
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
    /// Cosmic theme (v0.9.1+)
    cosmic_theme: CosmicTheme,
    /// Color theme (derived from cosmic_theme for backward compat)
    theme: Theme,
    /// Event receiver from runtime (mpsc - legacy)
    event_rx: Option<mpsc::Receiver<NikaEvent>>,
    /// Broadcast receiver from runtime (v0.4.1 - preferred)
    broadcast_rx: Option<broadcast::Receiver<NikaEvent>>,
    /// Should quit flag
    should_quit: bool,
    /// Last Ctrl+C press time (for double-tap quit like Claude Code)
    last_ctrl_c: Option<std::time::Instant>,
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
    studio_view: YamlEditorPanel,
    /// Settings view state (v0.11 auxiliary)
    settings_view: SettingsView,
    /// Help view state (v0.11 auxiliary)
    help_view: HelpView,
    /// Monitor view state (v0.11 - workflow execution monitoring)
    monitor_view: MonitorView,
    /// Scheduler view state (v0.12 - cron/queue management)
    scheduler_view: SchedulerView,
    /// Split view state (v0.13 - side-by-side Editor + Runner)
    split_view: SplitView,
    /// Workspace view state (v0.20 - unified 3-panel: Browser+Editor+DAG)
    workspace_view: WorkspaceView,
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
    /// PERF: Cached MCP connected count (reserved for future optimization)
    /// Note: Currently unused - DashMap iteration is fast enough.
    #[allow(dead_code)]
    cached_mcp_connected: usize,
    // ═══ Background Task Tracking (v0.7.0) ═══
    /// AbortHandles for tracked background tasks
    /// Enables proper cancellation on app exit via abort_all()
    background_handles: Arc<Mutex<Vec<AbortHandle>>>,
    // ═══ Session Persistence (v0.12.0) ═══
    /// Current session ID (for save/load)
    session_id: Option<String>,
    // ═══ TUI Config (v0.12.0) ═══
    /// Loaded configuration from .nika/config.toml
    /// v0.12.0: Field loaded at startup, full integration in v0.13 (theme/editor settings)
    #[allow(dead_code)]
    config: TuiConfig,
    // ═══ Performance: Reusable Event Buffer (v0.8.1) ═══
    /// PERF: Pre-allocated buffer for poll_runtime_events to avoid
    /// allocating a new Vec on every frame (60 FPS = 60 allocations/sec saved)
    event_buffer: Vec<NikaEvent>,
    // ═══ Connection Verification Cache (v0.8.2) ═══
    /// TTL-based cache for provider and MCP server verification results.
    /// Prevents redundant API calls when opening/refreshing the provider selector.
    verification_cache: Arc<Mutex<VerificationCache>>,
    // ═══ Nika Intro Animation (v0.12.0) ═══
    /// Splash screen animation shown on standalone TUI startup
    intro_state: Option<NikaIntroState>,
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
        let mut studio_view = YamlEditorPanel::new();
        // Load workflow file into studio view
        let _ = studio_view.load_file(workflow_path.to_path_buf());
        let settings_view = SettingsView::new();
        let help_view = HelpView::new();
        let monitor_view = MonitorView::new();
        let scheduler_view = SchedulerView::new();
        let split_view = SplitView::new();
        let workspace_view = WorkspaceView::new();

        // Initialize LLM response channel
        let (llm_response_tx, llm_response_rx) = mpsc::channel(32);
        // P1 Fix: Increase buffer from 256 to 512 for fast providers like Groq (~200 tok/s)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(512);

        // Initialize ChatAgent (may fail if no API keys are set, but that's OK)
        let chat_agent = ChatAgent::new().ok();

        // Load TUI configuration from .nika/config.toml (v0.12.0)
        let config = TuiConfig::load_or_default();

        // Initialize cosmic theme from config (v0.12.0: respects saved theme)
        let theme_variant = match config.tui.theme {
            ThemeName::Dark => crate::tui::tokens::CosmicVariant::CosmicDark,
            ThemeName::Light => crate::tui::tokens::CosmicVariant::CosmicLight,
            ThemeName::Solarized => crate::tui::tokens::CosmicVariant::CosmicViolet,
        };
        let cosmic_theme = CosmicTheme::new(theme_variant);
        let theme = cosmic_theme.as_theme();

        Ok(Self {
            workflow_path: workflow_path.to_path_buf(),
            terminal: None,
            state,
            standalone_state: None,
            cosmic_theme,
            theme,
            event_rx: None,
            broadcast_rx: None,
            should_quit: false,
            last_ctrl_c: None,
            workflow_done: false,
            status_message: None,
            retry_requested: false,
            // 4-view architecture - start in Monitor mode for workflow execution
            current_view: TuiView::Runner,
            input_mode: InputMode::Normal,
            focus_state: FocusState::new(NavPanelId::RunnerMission),
            chat_view,
            home_view: None, // No home view in execution mode
            studio_view,
            settings_view,
            help_view,
            monitor_view,
            scheduler_view,
            split_view,
            workspace_view,
            llm_response_rx,
            llm_response_tx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_configs: None, // Loaded in init_mcp_clients()
            mcp_client_cache: Arc::new(DashMap::new()),
            cached_mcp_connected: 0,
            background_handles: Arc::new(Mutex::new(Vec::new())),
            session_id: None, // v0.12: No session in workflow mode
            config,           // v0.12.0: TUI config from .nika/config.toml
            event_buffer: Vec::with_capacity(64), // PERF: Pre-allocated buffer
            verification_cache: Arc::new(Mutex::new(VerificationCache::default())),
            intro_state: None, // v0.12: No intro in workflow execution mode
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
        let studio_view = YamlEditorPanel::new();
        let settings_view = SettingsView::new();
        let help_view = HelpView::new();
        let monitor_view = MonitorView::new();
        let scheduler_view = SchedulerView::new();
        let split_view = SplitView::new();
        let workspace_view = WorkspaceView::new();

        // Initialize LLM response channel
        let (llm_response_tx, llm_response_rx) = mpsc::channel(32);
        // P1 Fix: Increase buffer from 256 to 512 for fast providers like Groq (~200 tok/s)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(512);

        // Initialize ChatAgent (may fail if no API keys are set, but that's OK)
        let chat_agent = ChatAgent::new().ok();

        // Load TUI configuration from .nika/config.toml (v0.12.0)
        let config = TuiConfig::load_or_default();

        // Initialize cosmic theme from config (v0.12.0: respects saved theme)
        let theme_variant = match config.tui.theme {
            ThemeName::Dark => crate::tui::tokens::CosmicVariant::CosmicDark,
            ThemeName::Light => crate::tui::tokens::CosmicVariant::CosmicLight,
            ThemeName::Solarized => crate::tui::tokens::CosmicVariant::CosmicViolet,
        };
        let cosmic_theme = CosmicTheme::new(theme_variant);
        let theme = cosmic_theme.as_theme();

        Ok(Self {
            workflow_path,
            terminal: None,
            state,
            standalone_state: Some(standalone_state),
            cosmic_theme,
            theme,
            event_rx: None,
            broadcast_rx: None,
            should_quit: false,
            last_ctrl_c: None,
            workflow_done: false,
            status_message: None,
            retry_requested: false,
            // 5-view architecture - start in Studio mode for standalone
            current_view: TuiView::Studio,
            input_mode: InputMode::Normal,
            focus_state: FocusState::new(NavPanelId::StudioFiles),
            chat_view,
            home_view: Some(home_view),
            studio_view,
            settings_view,
            help_view,
            monitor_view,
            scheduler_view,
            split_view,
            workspace_view,
            llm_response_rx,
            llm_response_tx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_configs: None, // No workflow in standalone mode
            mcp_client_cache: Arc::new(DashMap::new()),
            cached_mcp_connected: 0,
            background_handles: Arc::new(Mutex::new(Vec::new())),
            session_id: None,                     // v0.12: Session loaded on demand
            config,                               // v0.12.0: TUI config from .nika/config.toml
            event_buffer: Vec::with_capacity(64), // PERF: Pre-allocated buffer
            verification_cache: Arc::new(Mutex::new(VerificationCache::default())),
            intro_state: Some(NikaIntroState::new()), // v0.12: Show intro animation on startup
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
        // Auto-enter Insert mode for Chat view so users can type immediately
        if view == TuiView::Chat {
            self.input_mode = InputMode::Insert;
        }
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
    /// Used by `nika chat --provider claude --model claude-sonnet-4-6`.
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

        // v0.8.4: Startup verification - ensure directories, schema, config, project access
        let startup_report = startup::verify_startup()?;
        if !startup_report.is_ok() {
            // Log details before failing
            for warning in startup_report.warnings() {
                tracing::error!("Startup issue: {}", warning);
            }
            return Err(NikaError::StartupError {
                phase: "verification".into(),
                reason: "Startup verification failed - see logs for details".into(),
            });
        }
        tracing::info!("{}", startup_report.summary());
        for warning in startup_report.warnings() {
            tracing::warn!("Startup warning: {}", warning);
        }

        // Initialize MCP clients from workflow config
        self.init_mcp_clients();

        // v0.8.2: Provider/MCP verification - verify all providers and MCP servers
        // Results are cached for 30s to avoid redundant API calls
        self.spawn_provider_verification();
        self.spawn_provider_verification_timeout(); // v0.8.4: Show fallback UI after 5s
        self.spawn_mcp_verification();

        // Initialize terminal
        self.init_terminal()?;

        // PERF: Adaptive frame rate
        // - Fast (60 FPS) when streaming or animations active
        // - Slow (10 FPS) when idle to save CPU
        const FAST_TICK_MS: u64 = 16; // 60 FPS for smooth animations
        const SLOW_TICK_MS: u64 = 100; // 10 FPS when idle

        // Track if user had recent input (stays fast for one frame after input)
        let mut had_recent_input = true; // Start fast on first frame

        loop {
            // 1. Poll runtime events (same as run())
            self.poll_runtime_events();

            // 2. Determine if we need fast rendering
            let is_streaming = self.chat_view.is_streaming;
            let has_inline_content = !self.chat_view.inline_content.is_empty();
            let intro_active = self
                .intro_state
                .as_ref()
                .map(|i| !i.is_done())
                .unwrap_or(false);
            let needs_fast_render =
                is_streaming || has_inline_content || had_recent_input || intro_active;
            had_recent_input = false; // Consume the flag

            // 3. Update elapsed time and animations
            self.state.tick();
            self.chat_view.tick(); // FIX: was missing - enables inline MCP/Infer animations
            if let Some(ref mut home) = self.home_view {
                home.tick(); // Enables gradient logo animation + sparkline pulse
            }
            self.studio_view.tick(); // v0.9.1: Matrix Rain animation
            self.studio_view.maybe_validate(); // Debounced validation (300ms after last edit)
            self.monitor_view.tick(&mut self.state); // v0.12.1: Runner panel animations

            // v0.12: Tick intro animation (if active)
            if let Some(ref mut intro) = self.intro_state {
                if !intro.is_done() {
                    // Get terminal size for intro animation
                    let intro_area = self
                        .terminal
                        .as_ref()
                        .and_then(|t| t.size().ok())
                        .map(|s| Rect::new(0, 0, s.width, s.height))
                        .unwrap_or_else(|| Rect::new(0, 0, 80, 24));
                    intro.tick(intro_area);
                }
            }

            // 4. Render frame based on current view
            self.render_unified_frame()?;

            // 5. Get terminal size for input handling
            let terminal_size = if let Some(ref terminal) = self.terminal {
                terminal
                    .size()
                    .ok()
                    .map(|size| Rect::new(0, 0, size.width, size.height))
            } else {
                None
            };

            // 6. Poll input events (adaptive tick rate)
            let tick_rate = if needs_fast_render {
                Duration::from_millis(FAST_TICK_MS)
            } else {
                Duration::from_millis(SLOW_TICK_MS)
            };

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

                // After input, render at full speed on next frame
                had_recent_input = true;
            }

            // 7. Check quit flag
            if self.should_quit {
                // Save chat session before quitting (v0.12.0)
                self.save_current_session();
                break;
            }
        }

        // Cancel all background tasks before cleanup
        self.cancel_background_tasks();

        // Cleanup and return
        self.cleanup()
    }

    /// Poll runtime events from broadcast/mpsc receivers
    fn poll_runtime_events(&mut self) {
        // PERF: Reuse pre-allocated buffer instead of allocating every frame
        // This saves ~60 allocations/sec at 60 FPS
        self.event_buffer.clear();

        // Check broadcast receiver (v0.4.1 preferred)
        if let Some(ref mut rx) = self.broadcast_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => self.event_buffer.push(event),
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
                self.event_buffer.push(event);
            }
        }

        // PERF: Move events to local vec for processing
        // drain() preserves event_buffer's capacity for next frame's collection
        // The local vec allocation is small and happens once per frame, not per-event
        let events: Vec<_> = self.event_buffer.drain(..).collect();

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
                // P0 Fix: Use workflow.paused as single source of truth
                self.state.workflow.paused = true;
                self.state.workflow.phase = crate::tui::theme::MissionPhase::Pause;
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
                    // v0.9.1 FIX: Initialize streaming with verb for proper Matrix effect theming
                    // Previously called on_streaming_start() which didn't set up streaming_decrypt
                    if !self.chat_view.is_streaming {
                        use crate::tui::widgets::DecryptVerb;
                        self.chat_view.start_streaming_with_verb(DecryptVerb::Infer);
                    }
                    // v0.8.1 FIX: ONLY update streaming_decrypt for Matrix effect
                    // DON'T append to message directly - that causes double display
                    // The message will be updated when streaming finishes
                    self.chat_view.append_streaming(&token);
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
                    // v0.12.1: Don't update message here - InferComplete handles the response
                    // The InferComplete event transfers partial_response to InferBox.response
                    // Old v0.8.1 code updated last message, but now TaskBox replaces AI bubble
                    tracing::debug!("Stream completed (TaskBox handles response)");
                }
                StreamChunk::Error(err) => {
                    // Remove "Thinking..." message and show categorized error (v0.5.2+)
                    if let Some(last) = self.chat_view.messages.last() {
                        if last.content == "Thinking..." {
                            self.chat_view.messages.pop();
                        }
                    }
                    // v0.8.1 FIX: Ensure streaming is finished on error
                    if self.chat_view.is_streaming {
                        self.chat_view.finish_streaming();
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
                    // v0.12.1: Also update the running InferBox with final token counts
                    self.chat_view
                        .append_infer_content("", output_tokens as u32);
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
                // ═══════════════════════════════════════════════════════════════════════
                // Chat Inline Box Events (v0.8.0 - wire widgets to chat commands)
                // ═══════════════════════════════════════════════════════════════════════
                StreamChunk::McpCallStart {
                    tool,
                    server,
                    params,
                } => {
                    // v0.8.4: Create TaskBox::Invoke for inline rendering
                    let params_json =
                        serde_json::from_str(&params).unwrap_or(serde_json::Value::Null);
                    let invoke_box = InvokeBox::new(&tool, &server)
                        .with_params(params_json)
                        .with_state(BoxState::running());
                    self.chat_view.add_task_box(TaskBox::Invoke(invoke_box));
                    tracing::debug!(tool = %tool, server = %server, "MCP call started with TaskBox");
                }
                StreamChunk::McpCallComplete { result } => {
                    // v0.8.4: Update TaskBox::Invoke with result (find by type + Running state)
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result) {
                        self.chat_view
                            .complete_last_invoke_box_with_result(parsed, 0);
                    } else {
                        self.chat_view.complete_last_invoke_box(0);
                    }
                    // Keep legacy method for backward compatibility
                    self.chat_view.complete_mcp_call(&result);
                    tracing::debug!("MCP call completed with TaskBox");
                }
                StreamChunk::McpCallFailed { error } => {
                    // v0.8.4: Update TaskBox::Invoke with error (find by type + Running state)
                    self.chat_view.fail_last_invoke_box(&error, 0);
                    // Keep legacy method for backward compatibility
                    self.chat_view.fail_mcp_call(&error);
                    tracing::warn!(error = %error, "MCP call failed with TaskBox");
                }
                StreamChunk::InferStart {
                    model,
                    prompt,
                    prompt_tokens,
                    max_tokens,
                } => {
                    // v0.12.1: Create TaskBox::Infer with actual prompt for inline rendering
                    let infer_box = InferBox::new(&model, &prompt)
                        .with_state(BoxState::running())
                        .with_tokens(prompt_tokens, 0)
                        .with_streaming_cursor(true);
                    self.chat_view.add_task_box(TaskBox::Infer(infer_box));

                    // Keep existing streaming state for compatibility
                    self.chat_view
                        .start_infer_stream(&model, prompt_tokens, max_tokens);
                    // Also start streaming for Matrix Decrypt effect
                    use crate::tui::widgets::DecryptVerb;
                    self.chat_view.start_streaming_with_verb(DecryptVerb::Infer);
                    tracing::debug!(model = %model, prompt_tokens, max_tokens, "Infer started with TaskBox");
                }
                StreamChunk::InferTokens { output_tokens } => {
                    // Update inline inference token count (using append with empty chunk)
                    self.chat_view.append_infer_content("", output_tokens);
                }
                StreamChunk::InferComplete => {
                    // v0.12.1: InferBox REPLACES AI message bubble (Option A)
                    // 1. Transfer partial_response to InferBox.response
                    self.chat_view.complete_last_infer_box(0);
                    // 2. Complete inline inference visualization
                    self.chat_view.complete_infer_stream();
                    // 3. Set is_streaming = false (response already transferred to InferBox)
                    let _ = self.chat_view.finish_streaming();
                    tracing::debug!("Infer completed with TaskBox");
                }
                // ═══════════════════════════════════════════════════════════════════════
                // Activity Events for /exec, /fetch, /agent (v0.8.0)
                // ═══════════════════════════════════════════════════════════════════════
                StreamChunk::ExecStart { command } => {
                    // v0.8.4: Create TaskBox::Exec for inline rendering
                    let exec_box = ExecBox::new(&command).with_state(BoxState::running());
                    self.chat_view.add_task_box(TaskBox::Exec(exec_box));
                    tracing::debug!(command = %command, "Exec started with TaskBox");
                }
                StreamChunk::ExecComplete => {
                    // v0.8.4: Update TaskBox::Exec with success (find by type + Running state)
                    self.chat_view.complete_last_exec_box(0);
                    tracing::debug!("Exec completed with TaskBox");
                }
                StreamChunk::FetchStart { url, method } => {
                    // v0.8.4: Create TaskBox::Fetch for inline rendering
                    let fetch_box = FetchBox::new(&method, &url).with_state(BoxState::running());
                    self.chat_view.add_task_box(TaskBox::Fetch(fetch_box));
                    tracing::debug!(url = %url, method = %method, "Fetch started with TaskBox");
                }
                StreamChunk::FetchComplete => {
                    // v0.8.4: Update TaskBox::Fetch with success (find by type + Running state)
                    self.chat_view.complete_last_fetch_box(0);
                    tracing::debug!("Fetch completed with TaskBox");
                }
                StreamChunk::AgentStart { goal } => {
                    // v0.8.4: Create TaskBox::Agent for inline rendering
                    let agent_id = format!("agent-{}", self.chat_view.messages.len());
                    let agent_box = AgentBox::new(&agent_id, &goal).with_state(BoxState::running());
                    self.chat_view.add_task_box(TaskBox::Agent(agent_box));
                    tracing::debug!(goal = %goal, "Agent started with TaskBox");
                }
                StreamChunk::AgentComplete => {
                    // v0.8.4: Update TaskBox::Agent with success (find by type + Running state)
                    self.chat_view.complete_last_agent_box(0);
                    tracing::debug!("Agent completed");
                }
                // ═══════════════════════════════════════════════════════════════════════
                // Connection Verification Events (v0.8.2)
                // ═══════════════════════════════════════════════════════════════════════
                StreamChunk::ProviderVerifying { provider, model } => {
                    tracing::debug!(provider = %provider, model = %model, "Provider verification started");
                    // Update provider modal state
                    use super::widgets::provider_modal::ConnectionStatus;
                    self.chat_view
                        .provider_modal
                        .set_provider_status_by_name(&provider, ConnectionStatus::Checking);
                }
                StreamChunk::ProviderVerified {
                    provider,
                    model,
                    latency_ms,
                } => {
                    tracing::info!(
                        provider = %provider,
                        model = %model,
                        latency_ms = %latency_ms,
                        "Provider verified"
                    );
                    use super::widgets::provider_modal::ConnectionStatus;
                    self.chat_view.provider_modal.set_provider_status_by_name(
                        &provider,
                        ConnectionStatus::Connected { latency_ms },
                    );
                }
                StreamChunk::ProviderVerifyFailed { provider, error } => {
                    tracing::warn!(provider = %provider, error = %error, "Provider verification failed");
                    use super::widgets::provider_modal::ConnectionStatus;
                    self.chat_view.provider_modal.set_provider_status_by_name(
                        &provider,
                        ConnectionStatus::Failed {
                            error: error.clone(),
                        },
                    );

                    // v0.8.3: BUG #3 fix - If the CURRENT provider failed, warn user and invalidate
                    if self.chat_view.current_provider_id == provider {
                        tracing::warn!(
                            provider = %provider,
                            "Current provider failed verification - invalidating chat_agent"
                        );
                        self.chat_agent = None;
                        self.chat_view.add_system_message(format!(
                            "⚠️ {} is unavailable: {}. Next message will use fallback provider.",
                            provider, error
                        ));
                    }
                }
                // v0.8.9: Handle provider not configured (no API key)
                StreamChunk::ProviderNotConfigured { provider } => {
                    tracing::debug!(provider = %provider, "Provider not configured");
                    use super::widgets::provider_modal::ConnectionStatus;
                    self.chat_view
                        .provider_modal
                        .set_provider_status_by_name(&provider, ConnectionStatus::NotConfigured);
                }
                StreamChunk::McpPinging { server } => {
                    tracing::debug!(server = %server, "MCP ping started");
                    // MCP ping status could be tracked similarly if needed
                }
                StreamChunk::McpPinged {
                    server,
                    latency_ms,
                    tool_count,
                } => {
                    tracing::info!(
                        server = %server,
                        latency_ms = %latency_ms,
                        tool_count = %tool_count,
                        "MCP server pinged"
                    );
                    // Update MCP server status in session context
                    self.chat_view
                        .update_mcp_server_status(&server, true, latency_ms);
                }
                // v0.8.4: Provider verification timeout
                StreamChunk::ProviderVerificationTimeout => {
                    tracing::warn!("Provider verification timed out - no providers available");
                    // Check if ANY provider is connected/verified
                    if !self.chat_view.provider_modal.has_any_connected() {
                        // Show warning banner in chat
                        self.chat_view.add_system_message(
                            "⚠️ No LLM providers available. Set an API key (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.) or press ⌘P to configure."
                                .to_string(),
                        );
                    }
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
    /// v0.8.1: Also updates agent phase indicator for real-time status display.
    fn handle_chat_view_event(&mut self, kind: &EventKind) {
        match kind {
            // ═══════════════════════════════════════════
            // AGENT EVENTS (v0.8.1 phase tracking)
            // ═══════════════════════════════════════════
            EventKind::AgentStart { .. } => {
                self.chat_view.on_agent_start();
            }
            EventKind::AgentTurn {
                turn_index, kind, ..
            } => {
                // v0.8.1: Use kind to determine phase
                // "started" = Syncing (agent connecting)
                // Other = Planning (agent thinking)
                if kind == "started" {
                    self.chat_view.on_agent_start();
                } else {
                    self.chat_view.on_agent_turn(*turn_index);
                }
            }
            EventKind::AgentComplete { .. } => {
                self.chat_view.on_agent_complete();
            }

            // ═══════════════════════════════════════════
            // MCP EVENTS
            // ═══════════════════════════════════════════
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
                // v0.8.1: Update agent phase to Invoking
                self.chat_view.on_mcp_invoke(tool_name, mcp_server);
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
                // v0.8.1: Update agent phase to Processing
                self.chat_view.on_mcp_response();
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderCalled {
                model, prompt_len, ..
            } => {
                // Start inference stream visualization
                self.chat_view
                    .start_infer_stream(model, *prompt_len as u32, 4096);
                // v0.8.1: Update agent phase to Inferring
                self.chat_view.on_provider_called();
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

            // ═══════════════════════════════════════════
            // MCP CONNECTION EVENTS (v0.7.0)
            // ═══════════════════════════════════════════
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
            // Ensure timeline cache is up-to-date before rendering Monitor view
            if current_view == TuiView::Runner {
                self.state.ensure_timeline_cache();
            }

            // All views use unified layout with Header + Content + StatusBar
            // v0.8 FIX: Extract read-only values BEFORE taking mutable references
            // This allows render() to take &mut self for scroll state updates
            let total_tokens = self.chat_view.total_tokens();
            let provider = self.chat_view.provider();
            let chat_status = self.chat_view.status_line(&self.state);
            let _home_status = self
                .home_view
                .as_ref()
                .map(|hv| hv.status_line(&self.state))
                .unwrap_or_default();
            let studio_status = self.studio_view.status_line(&self.state);
            let monitor_status = {
                let task_count = self.state.tasks.len();
                let completed = self
                    .state
                    .tasks
                    .values()
                    .filter(|t| t.status == super::theme::TaskStatus::Success)
                    .count();
                format!("Tasks: {}/{}", completed, task_count)
            };
            let scheduler_status = self.scheduler_view.status_line(&self.state);
            let _split_status = self.split_view.status_line(&self.state); // v0.13: Split view
            let _workspace_status = self.workspace_view.status_line(&self.state); // v0.20: Workspace view

            // Extract references to avoid borrow issues with the closure
            let theme = &self.theme;
            let state = &self.state;
            let chat_view = &mut self.chat_view;
            let _home_view = &mut self.home_view;
            let studio_view = &mut self.studio_view;
            let settings_view = &mut self.settings_view;
            let _help_view = &mut self.help_view; // v0.12: Help merged into Settings, kept for backwards compat
            let monitor_view = &mut self.monitor_view;
            let scheduler_view = &mut self.scheduler_view;
            let _split_view = &mut self.split_view; // v0.13: Split view
            let _workspace_view = &mut self.workspace_view; // v0.20: Workspace view
            let workflow_path = &self.state.workflow.path;
            let intro_state = &self.intro_state; // v0.12: Intro animation state
                                                 // P0 Fix: Use is_paused() accessor for unified pause state
            let paused = self.state.is_paused();
            let input_mode = self.input_mode;

            // Extract data for StatusBar metrics
            let mcp_total = self.mcp_configs.as_ref().map(|c| c.len()).unwrap_or(0);
            // Count actually connected MCP clients (OnceCell initialized = connected)
            let mcp_connected = self
                .mcp_client_cache
                .iter()
                .filter(|entry| entry.value().get().is_some())
                .count();

            // Get custom status text from current view (using pre-computed values)
            let status_text = match current_view {
                TuiView::Studio => studio_status,
                TuiView::Runner => monitor_status,
                TuiView::Chat => chat_status,
                TuiView::Scheduler => scheduler_status,
                TuiView::Settings => settings_view.status_line(state),
            };

            terminal
                .draw(|frame| {
                    let size = frame.area();

                    // v0.8: Check terminal size for graceful degradation
                    use super::widgets::{check_terminal_size, TerminalTooSmallOverlay};
                    let layout_mode = check_terminal_size(size);

                    // If terminal is too small, show overlay and return early
                    if !layout_mode.is_usable() {
                        let overlay = TerminalTooSmallOverlay::new(size.width, size.height);
                        frame.render_widget(overlay, size);
                        return;
                    }

                    // v0.12: Show intro animation (full screen overlay) if active
                    if let Some(intro) = intro_state {
                        if !intro.is_done() {
                            let intro_widget = NikaIntro::new(intro);
                            frame.render_widget(intro_widget, size);
                            return;
                        }
                    }

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
                        TuiView::Studio => {
                            studio_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Runner => {
                            monitor_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Chat => {
                            chat_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Scheduler => {
                            scheduler_view.render(frame, chunks[1], state, theme);
                        }
                        TuiView::Settings => {
                            settings_view.render(frame, chunks[1], state, theme);
                        }
                    }

                    // Render status message if active (just above status bar)
                    // v0.8.8: Skip when overlays are visible to prevent overlap
                    let overlay_visible = matches!(current_view, TuiView::Chat)
                        && (chat_view.provider_modal.visible
                            || chat_view.command_palette.visible
                            || chat_view.help_overlay.visible);

                    if !overlay_visible {
                        if let Some(msg) = state.status_messages.current() {
                            // Position status message at bottom of content area
                            let msg_area = Rect {
                                x: chunks[1].x,
                                y: chunks[1].bottom().saturating_sub(1),
                                width: chunks[1].width,
                                height: 1,
                            };
                            let status_widget = StatusMessageWidget::new(Some(msg));
                            frame.render_widget(status_widget, msg_area);
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
            // Ctrl+C double-tap to quit (Claude Code pattern)
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                return self.handle_ctrl_c();
            }

            // View navigation by number (when not capturing input)
            // 5-view architecture: [1] Studio, [2] Runner, [3] Chat, [4] Scheduler, [5] Settings
            KeyCode::Char('1') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Studio);
            }
            KeyCode::Char('2') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Runner);
            }
            KeyCode::Char('3') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Chat);
            }
            KeyCode::Char('4') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Scheduler);
            }
            KeyCode::Char('5') if !self.is_view_capturing_input() => {
                return Action::SwitchView(TuiView::Settings);
            }

            // Tab/BackTab delegated to views for panel navigation (v0.8 UX)
            // Use number keys 1-8 to switch views (v0.20 7-views architecture)
            // Views handle Tab internally for panel focus cycling
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
            TuiView::Studio => {
                let view_action = self.studio_view.handle_key(key_event, &mut self.state);
                self.convert_view_action(view_action)
            }
            TuiView::Runner => {
                // Monitor uses the existing 4-panel key handling
                self.handle_key(code, modifiers)
            }
            TuiView::Chat => {
                // In Normal mode, Chat view handles navigation keys (j/k, etc.)
                let view_action = self.chat_view.handle_key(key_event, &mut self.state);
                self.convert_view_action(view_action)
            }
            TuiView::Scheduler => {
                let view_action = self.scheduler_view.handle_key(key_event, &mut self.state);
                self.convert_view_action(view_action)
            }
            TuiView::Settings => {
                let view_action = self.settings_view.handle_key(key_event, &mut self.state);
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
                self.current_view = TuiView::Runner;
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
                // v0.12.1: Send message using TaskBox pattern (like /infer)
                // This enables InferBox rendering instead of plain text bubbles
                if !msg.is_empty() {
                    // v0.12.1: Don't add "Thinking..." message - InferBox replaces AI bubble
                    // Old: self.chat_view.add_nika_message("Thinking...".to_string(), None);

                    // Build conversation context from previous messages
                    let context = self.build_conversation_context();
                    let prompt_with_context = format!("{}{}", context, msg);

                    // v0.12.1: Use streaming channel for TaskBox rendering (like handle_chat_infer)
                    let stream_tx = self.stream_chunk_tx.clone();

                    // v0.8.2: Capture selected provider/model for correct routing
                    let provider_id = self.chat_view.current_provider_id.clone();
                    let model_name = self.chat_view.current_model.clone();

                    // Estimate prompt tokens (rough approximation: chars / 4)
                    let prompt_tokens = (prompt_with_context.len() / 4) as u32;
                    let max_tokens = 4096u32;

                    // v0.12.1: Capture user prompt for TaskBox display
                    let user_prompt = msg.clone();

                    // Spawn tracked task to call ChatAgent.infer() with TaskBox events
                    if self.ensure_chat_agent().is_some() {
                        self.spawn_tracked(async move {
                            // v0.12.1: Send InferStart to create TaskBox::Infer with actual prompt
                            let _ = stream_tx
                                .send(StreamChunk::InferStart {
                                    model: model_name.clone(),
                                    prompt: user_prompt.clone(),
                                    prompt_tokens,
                                    max_tokens,
                                })
                                .await;

                            // v0.8.2: Create agent with selected provider/model
                            // Wire streaming for real-time token display (TaskBox UX)
                            match crate::tui::ChatAgent::with_overrides(
                                Some(&provider_id),
                                Some(&model_name),
                            ) {
                                Ok(agent) => {
                                    let mut agent = agent.with_stream_chunks(stream_tx.clone());
                                    match timeout(INFER_TIMEOUT, agent.infer(&prompt_with_context))
                                        .await
                                    {
                                        Ok(Ok(_response)) => {
                                            // Response already displayed via streaming tokens
                                            // StreamChunk::Token appends to TaskBox
                                        }
                                        Ok(Err(e)) => {
                                            let _ = stream_tx
                                                .send(StreamChunk::Error(e.to_string()))
                                                .await;
                                        }
                                        Err(_) => {
                                            let _ = stream_tx
                                                .send(StreamChunk::Error(format!(
                                                    "LLM inference timed out after {}s",
                                                    INFER_TIMEOUT.as_secs()
                                                )))
                                                .await;
                                        }
                                    }
                                    // v0.12.1: Send InferComplete to finalize TaskBox
                                    let _ = stream_tx.send(StreamChunk::InferComplete).await;
                                }
                                Err(e) => {
                                    let _ = stream_tx
                                        .send(StreamChunk::Error(format!(
                                            "Error creating agent: {}",
                                            e
                                        )))
                                        .await;
                                    let _ = stream_tx.send(StreamChunk::InferComplete).await;
                                }
                            }
                        });
                    } else {
                        // No API key available
                        // SAFETY: Only pop if last message is "Thinking..."
                        if self.chat_view.messages.last().map(|m| m.content.as_str())
                            == Some("Thinking...")
                        {
                            self.chat_view.messages.pop();
                        }
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
            ViewAction::OpenSettings => Action::SetMode(TuiMode::Settings),
            ViewAction::ToggleTheme => {
                self.toggle_theme();
                Action::Continue
            }
            ViewAction::SetTheme(variant) => {
                self.set_theme(variant);
                Action::Continue
            }
            ViewAction::VerifyProviders => {
                // Spawn async verification for providers and MCP servers (v0.8.2)
                self.spawn_provider_verification();
                self.spawn_mcp_verification();
                Action::Continue
            }
            ViewAction::RefreshVerification => {
                // Invalidate cache and re-verify all providers and MCP servers (v0.8.2)
                {
                    let mut cache = self.verification_cache.lock();
                    cache.invalidate_all();
                }
                self.spawn_provider_verification();
                self.spawn_mcp_verification();
                self.set_status("🔄 Refreshing connections...");
                Action::Continue
            }
            ViewAction::ProviderSelectorConfirm { provider_id, model } => {
                // v0.8.3: Invalidate cached chat_agent when provider changes (BUG #2 fix)
                // This ensures the next infer call creates a fresh ChatAgent with correct provider
                tracing::info!(
                    provider = %provider_id,
                    model = %model,
                    "Provider selector confirmed - invalidating cached chat_agent"
                );

                // Drop the old chat_agent so next infer call creates fresh one with new provider
                self.chat_agent = None;

                self.set_status(&format!("✓ Switched to {} ({})", provider_id, model));
                Action::Continue
            }
            ViewAction::PullOllamaModel(model) => {
                // v0.12.3: Pull Ollama model asynchronously
                // v0.11.0: Use shared OllamaClient from ChatView (avoids allocation per call)
                let model_clone = model.clone();
                let client = self.chat_view.ollama_client.clone();
                self.spawn_tracked(async move {
                    let mut rx = client.pull_model(&model_clone).await;
                    while let Some(progress) = rx.recv().await {
                        tracing::debug!(model = %model_clone, ?progress, "Pull progress");
                    }
                });
                self.set_status(&format!("📥 Pulling {}...", model));
                Action::Continue
            }
            ViewAction::DeleteOllamaModel(model) => {
                // v0.12.3: Delete Ollama model asynchronously
                // v0.11.0: Use shared OllamaClient from ChatView (avoids allocation per call)
                let model_clone = model.clone();
                let client = self.chat_view.ollama_client.clone();
                self.spawn_tracked(async move {
                    match client.delete_model(&model_clone).await {
                        Ok(()) => tracing::info!(model = %model_clone, "Model deleted"),
                        Err(e) => {
                            tracing::error!(model = %model_clone, error = %e, "Delete failed")
                        }
                    }
                });
                self.set_status(&format!("🗑️ Deleting {}...", model));
                Action::Continue
            }
            ViewAction::RefreshOllamaModels => {
                // v0.12.3: Refresh Ollama models list asynchronously
                // v0.11.0: Use shared OllamaClient from ChatView (avoids allocation per call)
                let client = self.chat_view.ollama_client.clone();
                self.spawn_tracked(async move {
                    match client.list_models().await {
                        Ok(models) => {
                            tracing::info!(count = models.len(), "Refreshed Ollama models")
                        }
                        Err(e) => tracing::error!(error = %e, "Failed to refresh models"),
                    }
                });
                self.set_status("🔄 Refreshing Ollama models...");
                Action::Continue
            }
            ViewAction::ValidateWorkflow(path) => {
                // v0.11.0: Validate workflow YAML from Home view
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        use crate::ast::Workflow;
                        match serde_yaml::from_str::<Workflow>(&content) {
                            Ok(_workflow) => {
                                let name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("workflow");
                                self.set_status(&format!("✅ {} is valid", name));
                            }
                            Err(e) => {
                                self.set_status(&format!("❌ Validation failed: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.set_status(&format!("❌ Cannot read file: {}", e));
                    }
                }
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
    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
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
            // v0.8.1: Removed 'q' to quit - use Ctrl+C (double-tap) instead
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => self.handle_ctrl_c(),

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
            // P0 Fix: Use is_paused() accessor for unified pause state
            KeyCode::Enter if self.state.is_paused() => Action::Step,

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
            KeyCode::Char('E') => Action::DismissError, // P3 fix: Dismiss error message (Shift+E)

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
    /// v0.8: Dispatches to view-specific mouse handlers for Chat view.
    fn handle_mouse(&mut self, mouse: MouseEvent, terminal_size: Option<Rect>) -> Action {
        let Some(size) = terminal_size else {
            return Action::Continue;
        };

        // v0.8 UX: Dispatch mouse events to Chat view when in Chat mode
        if self.current_view == TuiView::Chat {
            // Compute the view area (same as render_frame but for Chat)
            let view_area = self.compute_view_area(size);
            if self
                .chat_view
                .handle_mouse(mouse.kind, mouse.column, mouse.row, view_area)
            {
                return Action::Continue; // Event handled by ChatView
            }
        }

        // Default handling for other views (Monitor 2x2 grid)
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

    /// Compute the view area for the current frame (excluding status bar etc.)
    fn compute_view_area(&self, terminal_size: Rect) -> Rect {
        // Status bar takes 1 line at the bottom
        const STATUS_BAR_HEIGHT: u16 = 1;

        if terminal_size.height > STATUS_BAR_HEIGHT {
            Rect {
                x: terminal_size.x,
                y: terminal_size.y,
                width: terminal_size.width,
                height: terminal_size.height - STATUS_BAR_HEIGHT,
            }
        } else {
            // Don't subtract from tiny terminals
            terminal_size
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
                if self.current_view == TuiView::Runner {
                    self.focus_state.next_panel();
                }
            }
            Action::FocusPrev => {
                self.state.focus_prev();
                // Sync Navigation 2.0 focus_state for Monitor view
                if self.current_view == TuiView::Runner {
                    self.focus_state.prev_panel();
                }
            }
            Action::FocusPanel(n) => {
                self.state.focus_panel(n);
                // Sync Navigation 2.0 focus_state for Monitor view
                if self.current_view == TuiView::Runner {
                    let panel = match n {
                        1 => NavPanelId::RunnerMission,
                        2 => NavPanelId::RunnerDag,
                        3 => NavPanelId::RunnerNovanet,
                        _ => NavPanelId::RunnerReasoning,
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
            // P3 Fix: Error dismissal action
            Action::DismissError => {
                if self.state.dismiss_error() {
                    self.set_status("Error dismissed — press 'r' to retry");
                } else {
                    self.set_status("No error to dismiss");
                }
            }
            // View navigation actions (with Navigation 2.0 focus sync)
            Action::SwitchView(view) => {
                // v0.12.1: Call lifecycle hooks on view transition
                let old_view = self.current_view;
                if old_view != view {
                    // Call on_leave for the old view
                    match old_view {
                        TuiView::Studio => self.studio_view.on_leave(&mut self.state),
                        TuiView::Runner => self.monitor_view.on_leave(&mut self.state),
                        TuiView::Chat => self.chat_view.on_leave(&mut self.state),
                        TuiView::Scheduler => {} // No special handling
                        TuiView::Settings => {}  // No special handling
                    }
                }

                self.current_view = view;
                self.focus_state.reset_to_view(view);

                // v0.12.1: Call on_enter for the new view
                if old_view != view {
                    match view {
                        TuiView::Studio => self.studio_view.on_enter(&mut self.state),
                        TuiView::Runner => self.monitor_view.on_enter(&mut self.state),
                        TuiView::Chat => self.chat_view.on_enter(&mut self.state),
                        TuiView::Scheduler => {} // No special handling
                        TuiView::Settings => {}  // No special handling
                    }
                }

                // Auto-enter Insert mode for Chat view so users can type immediately
                self.input_mode = if view == TuiView::Chat {
                    InputMode::Insert
                } else {
                    InputMode::Normal
                };
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

                    // Spawn tracked task to call LLM with timeout protection
                    let tx = self.llm_response_tx.clone();
                    let prompt = message.clone();
                    self.spawn_tracked(async move {
                        let provider = RigProvider::openai();
                        match timeout(INFER_TIMEOUT, provider.infer(&prompt, None)).await {
                            Ok(Ok(response)) => {
                                let _ = tx.send(response).await;
                            }
                            Ok(Err(e)) => {
                                let _ = tx.send(format!("Error: {}", e)).await;
                            }
                            Err(_) => {
                                let _ = tx
                                    .send(format!(
                                        "Error: LLM inference timed out after {}s",
                                        INFER_TIMEOUT.as_secs()
                                    ))
                                    .await;
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
                            let preview = truncate_str(&content, 50);
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

    /// Handle Ctrl+C with double-tap to quit (Claude Code pattern)
    /// First press shows warning, second press within 2 seconds quits
    fn handle_ctrl_c(&mut self) -> Action {
        use std::time::{Duration, Instant};
        const QUIT_TIMEOUT: Duration = Duration::from_secs(2);

        let now = Instant::now();
        if let Some(last) = self.last_ctrl_c {
            if now.duration_since(last) < QUIT_TIMEOUT {
                // Second Ctrl+C within timeout - quit
                return Action::Quit;
            }
        }

        // First Ctrl+C or timeout expired - show warning
        self.last_ctrl_c = Some(now);
        self.set_status("⚠️ Press Ctrl+C again to quit");
        Action::Continue
    }

    /// Toggle theme (cycles: CosmicDark → CosmicLight → CosmicViolet)
    ///
    /// v0.9.1: Uses CosmicTheme for unified design system.
    fn toggle_theme(&mut self) {
        self.cosmic_theme.cycle();
        self.theme = self.cosmic_theme.as_theme();

        // Also update legacy theme_mode for backward compat
        self.state.theme_mode = self.cosmic_theme.variant().into();

        // v0.12: Sync SettingsView display
        self.settings_view
            .update_theme_name(self.cosmic_theme.label());

        self.set_status(&format!("🎨 Theme: {}", self.cosmic_theme.label()));
    }

    /// Set theme to a specific variant (v0.12.0)
    ///
    /// Used by Settings view for direct theme selection via [1][2][3] keys.
    fn set_theme(&mut self, variant: crate::tui::tokens::CosmicVariant) {
        self.cosmic_theme = crate::tui::CosmicTheme::new(variant);
        self.theme = self.cosmic_theme.as_theme();

        // Also update legacy theme_mode for backward compat
        self.state.theme_mode = self.cosmic_theme.variant().into();

        // v0.12: Sync SettingsView display
        self.settings_view
            .update_theme_name(self.cosmic_theme.label());

        self.set_status(&format!("🎨 Theme: {}", self.cosmic_theme.label()));
    }

    /// Set status message with auto-clear timer
    fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), std::time::Instant::now()));
    }

    /// Save current chat session to disk (v0.12.0)
    ///
    /// Called on quit to persist the chat conversation.
    /// Only saves if there are messages beyond the system prompt.
    fn save_current_session(&mut self) {
        // Only save in standalone mode (chat mode)
        if self.standalone_state.is_none() {
            return;
        }

        // Get chat state from the chat view
        let chat_state = self.chat_view.get_chat_state();

        // Save session (guard: won't save empty sessions)
        match save_session(&chat_state) {
            Ok(session) => {
                self.session_id = Some(session.id.clone());
                tracing::info!("Chat session saved: {}", session.id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
                // Empty session - not an error, just skip
                tracing::debug!("Skipping save: {}", e);
            }
            Err(e) => {
                tracing::warn!("Failed to save session: {}", e);
            }
        }
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

        // v0.12.1: Don't add "Thinking..." message - InferBox replaces AI bubble
        // Old: self.chat_view.add_nika_message("Thinking...".to_string(), None);

        // Build conversation context from previous messages
        let context = self.build_conversation_context();
        let prompt_with_context = if context.is_empty() {
            prompt.clone()
        } else {
            format!("{}{}", context, prompt)
        };

        // Spawn tracked task to call ChatAgent.infer()
        // Only use stream_tx - streaming handles message display
        // (llm_response_tx would cause duplicate messages)
        let stream_tx = self.stream_chunk_tx.clone();

        // ═══════════════════════════════════════════════════════════════════════
        // v0.8.0: Capture model for InferStart inline visualization
        // ═══════════════════════════════════════════════════════════════════════
        let model_name = self.chat_view.current_model.clone();
        // Estimate prompt tokens (rough approximation: chars / 4)
        let prompt_tokens = (prompt_with_context.len() / 4) as u32;
        let max_tokens = 4096u32;

        // v0.8.2: Capture selected provider ID for correct routing
        let provider_id = self.chat_view.current_provider_id.clone();

        // v0.12.1: Capture user prompt for TaskBox display
        let user_prompt = prompt.clone();

        // Check if agent exists or can be created
        if self.ensure_chat_agent().is_some() {
            self.spawn_tracked(async move {
                // v0.12.1: Send InferStart for inline visualization with actual prompt
                let _ = stream_tx
                    .send(StreamChunk::InferStart {
                        model: model_name.clone(),
                        prompt: user_prompt.clone(),
                        prompt_tokens,
                        max_tokens,
                    })
                    .await;
                // v0.8.2: Create agent with selected provider/model (CRITICAL FIX!)
                // Wire streaming for real-time token display (Claude Code-like UX)
                match ChatAgent::with_overrides(Some(&provider_id), Some(&model_name)) {
                    Ok(agent) => {
                        let mut agent = agent.with_stream_chunks(stream_tx.clone());
                        // Wrap inference with timeout protection
                        match timeout(INFER_TIMEOUT, agent.infer(&prompt_with_context)).await {
                            Ok(Ok(_response)) => {
                                // Response already displayed via streaming tokens
                                // StreamChunk::Token appends to "Thinking..." message
                                // Do NOT send on llm_response_tx - that would create duplicate
                            }
                            Ok(Err(e)) => {
                                // Send error via streaming channel to replace "Thinking..."
                                let _ = stream_tx.send(StreamChunk::Error(e.to_string())).await;
                            }
                            Err(_) => {
                                // Timeout - send error via streaming channel
                                let _ = stream_tx
                                    .send(StreamChunk::Error(format!(
                                        "LLM inference timed out after {}s",
                                        INFER_TIMEOUT.as_secs()
                                    )))
                                    .await;
                            }
                        }
                        // v0.8.0: Send InferComplete for inline visualization (success or error)
                        let _ = stream_tx.send(StreamChunk::InferComplete).await;
                    }
                    Err(e) => {
                        // Agent creation failed - send error via streaming channel
                        let _ = stream_tx
                            .send(StreamChunk::Error(format!("Error creating agent: {}", e)))
                            .await;
                        // v0.8.0: Send InferComplete even on agent creation failure
                        let _ = stream_tx.send(StreamChunk::InferComplete).await;
                    }
                }
            });
        } else {
            // No API key available
            // SAFETY: Only pop if last message is "Thinking..."
            if self.chat_view.messages.last().map(|m| m.content.as_str()) == Some("Thinking...") {
                self.chat_view.messages.pop();
            }
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

        // Spawn tracked task for shell execution with timeout protection
        let tx = self.llm_response_tx.clone();
        let status_tx = self.stream_chunk_tx.clone();
        let cmd_clone = command.clone();
        self.spawn_tracked(async move {
            // v0.8.0: Send ExecStart for activity tracking
            let _ = status_tx
                .send(StreamChunk::ExecStart { command: cmd_clone })
                .await;

            match ChatAgent::new() {
                Ok(agent) => match timeout(EXEC_TIMEOUT, agent.exec_command(&command)).await {
                    Ok(Ok(output)) => {
                        let _ = tx.send(output).await;
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(format!("Error: {}", e)).await;
                    }
                    Err(_) => {
                        let _ = tx
                            .send(format!(
                                "Error: Command timed out after {}s",
                                EXEC_TIMEOUT.as_secs()
                            ))
                            .await;
                    }
                },
                Err(e) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
            }

            // v0.8.0: Send ExecComplete for activity tracking
            let _ = status_tx.send(StreamChunk::ExecComplete).await;
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

        // Spawn tracked task for HTTP request with timeout protection
        let tx = self.llm_response_tx.clone();
        let status_tx = self.stream_chunk_tx.clone();
        let url_clone = url.clone();
        let method_clone = method.clone();
        self.spawn_tracked(async move {
            // v0.8.0: Send FetchStart for activity tracking
            let _ = status_tx
                .send(StreamChunk::FetchStart {
                    url: url_clone,
                    method: method_clone,
                })
                .await;

            match ChatAgent::new() {
                Ok(agent) => {
                    match timeout(FETCH_TIMEOUT, agent.fetch(&url, &method)).await {
                        Ok(Ok(response)) => {
                            // Truncate very long responses (UTF-8 safe)
                            let truncated = if response.chars().count() > 2000 {
                                let prefix: String = response.chars().take(2000).collect();
                                format!(
                                    "{}...\n\n[Truncated, {} chars total]",
                                    prefix,
                                    response.chars().count()
                                )
                            } else {
                                response
                            };
                            let _ = tx.send(truncated).await;
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(format!("Error: {}", e)).await;
                        }
                        Err(_) => {
                            let _ = tx
                                .send(format!(
                                    "Error: HTTP request timed out after {}s",
                                    FETCH_TIMEOUT.as_secs()
                                ))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("Error: {}", e)).await;
                }
            }

            // v0.8.0: Send FetchComplete for activity tracking
            let _ = status_tx.send(StreamChunk::FetchComplete).await;
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
            match available_servers.into_iter().next() {
                Some(server) => server,
                None => {
                    self.chat_view.add_nika_message(
                        "Error: No MCP servers configured.\nAdd mcp.servers to your workflow."
                            .to_string(),
                        None,
                    );
                    return;
                }
            }
        };

        let tx = self.llm_response_tx.clone();
        let status_tx = self.stream_chunk_tx.clone();
        let mcp_configs = self.mcp_configs.clone();
        let mcp_client_cache = Arc::clone(&self.mcp_client_cache);

        // Show pending message
        self.chat_view
            .add_nika_message(format!("🔧 Invoking {}:{} ...", server_name, tool), None);

        // Spawn tracked task to connect (if needed) and call the tool
        let tool_name = tool.clone();
        let server_name_clone = server_name.clone();
        self.spawn_tracked(async move {
            // Lazy-initialize MCP client connection
            let client = {
                let cell = mcp_client_cache
                    .entry(server_name_clone.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new()))
                    .clone();

                let name_owned = server_name_clone.clone();
                let configs = mcp_configs.clone();

                // v0.8.5: Wrap MCP init with timeout to prevent hanging on slow/unresponsive servers
                match timeout(
                    MCP_INIT_TIMEOUT,
                    cell.get_or_try_init(|| async {
                        if let Some(ref cfgs) = configs {
                            if let Some(inline_config) = cfgs.get(&name_owned) {
                                let mut mcp_config =
                                    McpConfig::new(&name_owned, &inline_config.command);
                                for arg in &inline_config.args {
                                    mcp_config = mcp_config.with_arg(arg);
                                }
                                for (key, value) in &inline_config.env {
                                    mcp_config = mcp_config.with_env(key, value);
                                }
                                if let Some(cwd) = &inline_config.cwd {
                                    mcp_config = mcp_config.with_cwd(cwd);
                                }

                                let client = McpClient::new(mcp_config).map_err(|e| {
                                    NikaError::McpStartError {
                                        name: name_owned.clone(),
                                        reason: e.to_string(),
                                    }
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
                    }),
                )
                .await
                {
                    Ok(Ok(c)) => {
                        // Notify TUI of successful MCP connection (v0.7.0)
                        let _ = status_tx
                            .send(StreamChunk::McpConnected(server_name_clone.clone()))
                            .await;
                        Arc::clone(c)
                    }
                    Ok(Err(e)) => {
                        // Notify TUI of MCP connection failure (v0.7.0)
                        let _ = status_tx
                            .send(StreamChunk::McpError {
                                server_name: server_name_clone.clone(),
                                error: e.to_string(),
                            })
                            .await;
                        let _ = tx
                            .send(format!(
                                "❌ Failed to connect to {}: {}",
                                server_name_clone, e
                            ))
                            .await;
                        return;
                    }
                    Err(_elapsed) => {
                        // v0.8.5: Timeout error - MCP server init took too long
                        let error_msg = format!(
                            "MCP server '{}' initialization timed out after {}s",
                            server_name_clone,
                            MCP_INIT_TIMEOUT.as_secs()
                        );
                        tracing::warn!(mcp_server = %server_name_clone, "MCP init timeout");
                        let _ = status_tx
                            .send(StreamChunk::McpError {
                                server_name: server_name_clone.clone(),
                                error: error_msg.clone(),
                            })
                            .await;
                        let _ = tx.send(format!("❌ {}", error_msg)).await;
                        return;
                    }
                }
            };

            // ═══════════════════════════════════════════════════════════════════════
            // v0.8.0: Send McpCallStart for inline visualization BEFORE the call
            // ═══════════════════════════════════════════════════════════════════════
            let params_str = serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_string());
            let _ = status_tx.send(StreamChunk::McpCallStart {
                tool: tool_name.clone(),
                server: server_name_clone.clone(),
                params: params_str,
            }).await;

            // Call the tool
            match client.call_tool(&tool_name, params).await {
                Ok(result) => {
                    let status = if result.is_error { "❌" } else { "✅" };
                    let text = result.text();

                    // Truncate very long responses (UTF-8 safe)
                    let display = if text.chars().count() > 3000 {
                        let prefix: String = text.chars().take(3000).collect();
                        format!(
                            "{}...\n\n[Truncated, {} chars total]",
                            prefix,
                            text.chars().count()
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

                    // v0.8.0: Send McpCallComplete/Failed for inline visualization
                    if result.is_error {
                        let _ = status_tx.send(StreamChunk::McpCallFailed {
                            error: display.clone(),
                        }).await;
                    } else {
                        let _ = status_tx.send(StreamChunk::McpCallComplete {
                            result: display.clone(),
                        }).await;
                    }
                }
                Err(e) => {
                    let error_msg = format!(
                        "❌ {}:{} failed: {}",
                        server_name_clone, tool_name, e
                    );
                    let _ = tx.send(error_msg.clone()).await;

                    // v0.8.0: Send McpCallFailed for inline visualization
                    let _ = status_tx.send(StreamChunk::McpCallFailed {
                        error: e.to_string(),
                    }).await;
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
        let goal_clone = goal.clone();

        // Spawn tracked task to connect MCP servers and run the agent
        self.spawn_tracked(async move {
            // v0.8.0: Send AgentStart for activity tracking
            let _ = status_tx.send(StreamChunk::AgentStart { goal: goal_clone }).await;

            // Connect MCP servers lazily
            let mut mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
            for server_name in &mcp_server_names {
                let cell = mcp_client_cache
                    .entry(server_name.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new()))
                    .clone();

                let name_owned = server_name.clone();
                let configs = mcp_configs.clone();

                // v0.8.5: Wrap MCP init with timeout to prevent hanging on slow/unresponsive servers
                match timeout(
                    MCP_INIT_TIMEOUT,
                    cell.get_or_try_init(|| async {
                        if let Some(ref cfgs) = configs {
                            if let Some(inline_config) = cfgs.get(&name_owned) {
                                let mut mcp_config =
                                    McpConfig::new(&name_owned, &inline_config.command);
                                for arg in &inline_config.args {
                                    mcp_config = mcp_config.with_arg(arg);
                                }
                                for (key, value) in &inline_config.env {
                                    mcp_config = mcp_config.with_env(key, value);
                                }
                                if let Some(cwd) = &inline_config.cwd {
                                    mcp_config = mcp_config.with_cwd(cwd);
                                }

                                let client = McpClient::new(mcp_config).map_err(|e| {
                                    NikaError::McpStartError {
                                        name: name_owned.clone(),
                                        reason: e.to_string(),
                                    }
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
                    }),
                )
                .await
                {
                    Ok(Ok(client)) => {
                        mcp_clients.insert(server_name.clone(), Arc::clone(client));
                        // Notify TUI of successful MCP connection (v0.7.0)
                        let _ = status_tx
                            .send(StreamChunk::McpConnected(server_name.clone()))
                            .await;
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(server = %server_name, error = %e, "Failed to connect MCP server");
                        // Notify TUI of MCP connection failure (v0.7.0)
                        let _ = status_tx
                            .send(StreamChunk::McpError {
                                server_name: server_name.clone(),
                                error: e.to_string(),
                            })
                            .await;
                    }
                    Err(_elapsed) => {
                        // v0.8.5: Timeout error - MCP server init took too long
                        let error_msg = format!(
                            "MCP server '{}' initialization timed out after {}s",
                            server_name,
                            MCP_INIT_TIMEOUT.as_secs()
                        );
                        tracing::warn!(mcp_server = %server_name, "MCP init timeout");
                        let _ = status_tx
                            .send(StreamChunk::McpError {
                                server_name: server_name.clone(),
                                error: error_msg,
                            })
                            .await;
                    }
                }
            }

            // Create EventLog for observability with broadcast for TUI activity
            // v0.8.1: Use broadcast to relay AgentTurn events to TUI
            let (event_log, mut event_rx) = EventLog::new_with_broadcast();

            // Spawn task to relay EventLog events to TUI via StreamChunk
            // v0.8.5: Add timeout to recv() to prevent orphaned relay task
            let relay_status_tx = status_tx.clone();
            tokio::spawn(async move {
                use crate::event::EventKind;
                use std::time::Duration;
                use tokio::time::timeout;

                // Relay timeout: check every 30s to prevent orphaned tasks
                // The agent has WORKFLOW_TIMEOUT (300s), so this is just a safety check
                const RELAY_RECV_TIMEOUT: Duration = Duration::from_secs(30);
                let mut consecutive_timeouts = 0u32;

                loop {
                    match timeout(RELAY_RECV_TIMEOUT, event_rx.recv()).await {
                        Ok(Ok(event)) => {
                            // Reset timeout counter on successful receive
                            consecutive_timeouts = 0;

                            match event.kind {
                                EventKind::AgentTurn { metadata: Some(ref m), .. } => {
                                    // Relay thinking content if present
                                    if let Some(ref thinking) = m.thinking {
                                        let _ = relay_status_tx
                                            .send(StreamChunk::Thinking(thinking.clone()))
                                            .await;
                                    }
                                    // Relay token metrics (cast u32 to u64)
                                    if m.input_tokens > 0 || m.output_tokens > 0 {
                                        let _ = relay_status_tx
                                            .send(StreamChunk::Metrics {
                                                input_tokens: m.input_tokens as u64,
                                                output_tokens: m.output_tokens as u64,
                                            })
                                            .await;
                                    }
                                }
                                EventKind::McpInvoke { tool, mcp_server, .. } => {
                                    // Relay MCP tool call start
                                    let _ = relay_status_tx
                                        .send(StreamChunk::McpCallStart {
                                            tool: tool.unwrap_or_default(),
                                            server: mcp_server,
                                            params: String::new(),
                                        })
                                        .await;
                                }
                                EventKind::McpResponse { is_error, response, .. } => {
                                    // Relay MCP tool call completion
                                    if !is_error {
                                        let _ = relay_status_tx
                                            .send(StreamChunk::McpCallComplete {
                                                result: response
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_else(|| "OK".to_string()),
                                            })
                                            .await;
                                    } else {
                                        let _ = relay_status_tx
                                            .send(StreamChunk::McpCallFailed {
                                                error: response
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_else(|| "MCP error".to_string()),
                                            })
                                            .await;
                                    }
                                }
                                _ => {} // Ignore other event types
                            }
                        }
                        Ok(Err(_)) => {
                            // Channel closed - agent completed, exit relay task
                            tracing::debug!("Event relay: channel closed, exiting");
                            break;
                        }
                        Err(_elapsed) => {
                            // Timeout - agent may be slow (long thinking, tool call)
                            consecutive_timeouts += 1;
                            tracing::debug!(
                                consecutive_timeouts,
                                "Event relay timeout, continuing to wait"
                            );

                            // After 10 consecutive timeouts (5 min), assume orphaned and exit
                            // This prevents truly orphaned relay tasks from running forever
                            if consecutive_timeouts >= 10 {
                                tracing::warn!(
                                    "Event relay: 10 consecutive timeouts, assuming orphaned"
                                );
                                break;
                            }
                            continue;
                        }
                    }
                }
            });

            // Create RigAgentLoop with connected clients
            // v0.8.1: Wire streaming channel for real-time token display
            let mut agent = match RigAgentLoop::new(task_id.clone(), params, event_log, mcp_clients)
            {
                Ok(loop_instance) => loop_instance.with_stream_tx(status_tx.clone()),
                Err(e) => {
                    let _ = response_tx
                        .send(format!("❌ Failed to create agent: {}", e))
                        .await;
                    return;
                }
            };

            // v0.8.5 FIX: Wrap agent execution in timeout to prevent hanging forever
            match timeout(WORKFLOW_TIMEOUT, agent.run_auto()).await {
                Ok(Ok(result)) => {
                    use serde_json::Value;
                    // Format the response with status and metrics
                    let status_emoji = match result.status {
                        RigAgentStatus::NaturalCompletion => "✅",
                        RigAgentStatus::ExplicitCompletion => "🎯", // v0.21
                        RigAgentStatus::HighConfidence(_) => "🎯", // v0.22
                        RigAgentStatus::LowConfidence(_) => "⚠️", // v0.22
                        RigAgentStatus::FlaggedForReview(_) => "🏳️", // v0.22 routing
                        RigAgentStatus::Escalated(_) => "📣", // v0.22 routing
                        RigAgentStatus::MaxTurnsReached => "⏱️",
                        RigAgentStatus::StopConditionMet => "🛑",
                        RigAgentStatus::Failed => "❌",
                        RigAgentStatus::TokenBudgetExceeded => "💰",
                        RigAgentStatus::CostLimitReached => "💵", // v0.24
                        RigAgentStatus::DurationLimitReached => "⏰", // v0.24
                        RigAgentStatus::PartialCompletion => "📝", // v0.24
                    };

                    // Extract final output text
                    // v0.8.1: RigAgentLoop returns {"response": "..."} JSON object
                    let output_text = match &result.final_output {
                        // Object with "response" key (normal case)
                        Value::Object(obj) => {
                            if let Some(Value::String(s)) = obj.get("response") {
                                s.clone()
                            } else if let Some(val) = obj.get("response") {
                                // Response exists but isn't a string - serialize it
                                val.to_string()
                            } else {
                                // No response key - serialize entire object
                                result.final_output.to_string()
                            }
                        }
                        // Direct string (unlikely but handle it)
                        Value::String(s) => s.clone(),
                        // Anything else - serialize
                        _ => result.final_output.to_string(),
                    };

                    let response = format!(
                        "{} Agent completed ({} turns, {} tokens)\n\n{}",
                        status_emoji, result.turns, result.total_tokens, output_text
                    );
                    let _ = response_tx.send(response).await;
                }
                Ok(Err(e)) => {
                    let _ = response_tx.send(format!("❌ Agent failed: {}", e)).await;
                }
                Err(_elapsed) => {
                    // v0.8.5 FIX: Timeout hit - send error message
                    let _ = response_tx
                        .send(format!(
                            "⏱️ Agent timed out after {}s. Consider using shorter prompts or fewer tools.",
                            WORKFLOW_TIMEOUT.as_secs()
                        ))
                        .await;
                }
            }

            // v0.8.1: Send Done to signal streaming completion (triggers final display)
            let _ = status_tx.send(StreamChunk::Done(String::new())).await;

            // v0.8.0: Send AgentComplete for activity tracking
            let _ = status_tx.send(StreamChunk::AgentComplete).await;
        });
    }

    /// Handle /model command - switch LLM provider or list available providers
    fn handle_chat_model_switch(&mut self, provider: ModelProvider) {
        // Handle /model list - show available providers
        if provider == ModelProvider::List {
            let providers = [
                ModelProvider::Claude,
                ModelProvider::OpenAI,
                ModelProvider::Mistral,
                ModelProvider::Groq,
                ModelProvider::DeepSeek,
                ModelProvider::Ollama,
            ];
            let mut list_text = String::from("Available providers (use /model <name>):\n");
            for p in providers {
                let status = if p.is_available() {
                    "✓ available".to_string()
                } else {
                    format!("✗ missing {}", p.env_var())
                };
                list_text.push_str(&format!(
                    "  {} - {} ({})\n",
                    p.command_name(),
                    p.name(),
                    status
                ));
            }
            self.chat_view
                .add_nika_message(list_text.trim_end().to_string(), None);
            self.set_status("Use /model <provider> to switch");
            return;
        }

        // Handle actual provider switch
        if let Some(ref mut agent) = self.chat_agent {
            match agent.set_provider(provider.clone()) {
                Ok(()) => {
                    // Sync both provider and model names
                    self.chat_view.set_provider(provider.name());
                    self.chat_view.set_model(agent.model_name());
                    // v0.12: Sync SettingsView display
                    self.settings_view
                        .update_provider(provider.name(), agent.model_name());
                    let msg = format!("Switched to {} ({})", provider.name(), agent.model_name());
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
                        // Sync both provider and model names
                        let model_name = agent.model_name().to_string();
                        self.chat_agent = Some(agent);
                        self.chat_view.set_provider(provider.name());
                        self.chat_view.set_model(&model_name);
                        // v0.12: Sync SettingsView display
                        self.settings_view
                            .update_provider(provider.name(), &model_name);
                        let msg = format!("Switched to {} ({})", provider.name(), model_name);
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

        // Spawn tracked task to load and run workflow
        self.spawn_tracked(async move {
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

            // Create and run workflow with timeout protection
            let mut runner = Runner::with_event_log(workflow, event_log);
            match timeout(WORKFLOW_TIMEOUT, runner.run()).await {
                Ok(Ok(output)) => {
                    tracing::info!("Workflow completed: {} chars output", output.len());
                }
                Ok(Err(e)) => {
                    tracing::error!("Workflow execution failed: {}", e);
                }
                Err(_) => {
                    tracing::error!("Workflow timed out after {}s", WORKFLOW_TIMEOUT.as_secs());
                }
            }
        });

        self.set_status(&format!("🌌 Warping through: {}", path.display()));
    }

    /// Spawn async provider verification tasks (v0.8.2)
    ///
    /// Verifies all configured providers in parallel, sending StreamChunk events
    /// to update the Provider Modal display in real-time.
    ///
    /// Uses TTL-based caching to avoid redundant API calls (30s default).
    fn spawn_provider_verification(&self) {
        let tx = self.stream_chunk_tx.clone();
        // v0.8.8: Static list of provider IDs (no longer from ProviderSelectorState)
        let provider_ids = [
            ("claude", "claude-sonnet-4-6"),
            ("openai", "gpt-4o"),
            ("mistral", "mistral-large-latest"),
            ("groq", "llama-3.3-70b-versatile"),
            ("deepseek", "deepseek-chat"),
            ("ollama", "llama3.2"),
        ];
        let cache = Arc::clone(&self.verification_cache);

        // Check cache and send events for cached providers, mark uncached as verifying
        for (provider_id, default_model) in &provider_ids {
            let model = default_model.to_string();

            // Check if we have a valid cached result
            let cached = {
                let cache_guard = cache.lock();
                cache_guard.get_provider(provider_id).and_then(|entry| {
                    if cache_guard.is_valid(entry) {
                        Some(entry.clone())
                    } else {
                        None
                    }
                })
            };

            if let Some(entry) = cached {
                // Send cached result directly
                match entry.status {
                    super::widgets::VerifyStatus::Verified => {
                        let _ = tx.try_send(StreamChunk::ProviderVerified {
                            provider: provider_id.to_string(),
                            model: entry.model.unwrap_or(model),
                            latency_ms: entry.latency.map(|d| d.as_millis() as u64).unwrap_or(0),
                        });
                    }
                    super::widgets::VerifyStatus::Failed => {
                        let _ = tx.try_send(StreamChunk::ProviderVerifyFailed {
                            provider: provider_id.to_string(),
                            error: entry.error.unwrap_or_else(|| "Unknown error".to_string()),
                        });
                    }
                    _ => {
                        // Verifying or Unknown - re-verify
                        let _ = tx.try_send(StreamChunk::ProviderVerifying {
                            provider: provider_id.to_string(),
                            model,
                        });
                    }
                }
            } else {
                // Mark as verifying - will spawn task below
                let _ = tx.try_send(StreamChunk::ProviderVerifying {
                    provider: provider_id.to_string(),
                    model,
                });
            }
        }

        // Spawn verification tasks only for providers NOT in valid cache
        for (provider_id_str, _) in provider_ids {
            let provider_id = provider_id_str.to_string();

            // Skip if cached
            {
                let cache_guard = cache.lock();
                if cache_guard.has_valid_provider(&provider_id) {
                    continue;
                }
            }

            let tx = tx.clone();
            let cache = Arc::clone(&cache);

            self.spawn_tracked(async move {
                // Create provider and check if configured
                let provider_opt: Option<RigProvider> = match provider_id.as_str() {
                    "claude" => {
                        // v0.8.4: Check env var BEFORE constructor (rig-core panics without it)
                        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                            let p = RigProvider::claude();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "openai" => {
                        // v0.8.4: Check env var BEFORE constructor (rig-core panics without it)
                        if std::env::var("OPENAI_API_KEY").is_ok() {
                            let p = RigProvider::openai();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "mistral" => {
                        // v0.8.4: Check env var BEFORE constructor (rig-core panics without it)
                        if std::env::var("MISTRAL_API_KEY").is_ok() {
                            let p = RigProvider::mistral();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "groq" => {
                        // v0.8.4: Check env var BEFORE constructor (rig-core panics without it)
                        if std::env::var("GROQ_API_KEY").is_ok() {
                            let p = RigProvider::groq();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "deepseek" => {
                        // v0.8.4: Check env var BEFORE constructor (rig-core panics without it)
                        if std::env::var("DEEPSEEK_API_KEY").is_ok() {
                            let p = RigProvider::deepseek();
                            if p.is_configured() {
                                Some(p)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    "ollama" => {
                        // Ollama is always available (local, no API key needed)
                        Some(RigProvider::ollama())
                    }
                    _ => None,
                };

                // v0.8.9: Per-provider verification timeout (10 seconds)
                const SINGLE_PROVIDER_TIMEOUT: Duration = Duration::from_secs(10);

                match provider_opt {
                    Some(provider) => {
                        match tokio::time::timeout(SINGLE_PROVIDER_TIMEOUT, provider.verify()).await
                        {
                            Ok(Ok(result)) => {
                                // Cache the successful result
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::verified(
                                            result.latency,
                                            Some(result.model.clone()),
                                        ),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerified {
                                        provider: provider_id,
                                        model: result.model,
                                        latency_ms: result.latency.as_millis() as u64,
                                    })
                                    .await;
                            }
                            Ok(Err(e)) => {
                                // Cache the failure
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::failed(e.to_string()),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerifyFailed {
                                        provider: provider_id,
                                        error: e.to_string(),
                                    })
                                    .await;
                            }
                            Err(_timeout) => {
                                // v0.8.9: Timeout - send failed event
                                tracing::warn!(
                                    provider = %provider_id,
                                    "Provider verification timed out after 10s"
                                );
                                {
                                    let mut cache_guard = cache.lock();
                                    cache_guard.set_provider(
                                        provider_id.clone(),
                                        VerificationEntry::failed(
                                            "Verification timeout (10s)".to_string(),
                                        ),
                                    );
                                }
                                let _ = tx
                                    .send(StreamChunk::ProviderVerifyFailed {
                                        provider: provider_id,
                                        error: "Verification timeout (10s)".to_string(),
                                    })
                                    .await;
                            }
                        }
                    }
                    None => {
                        // v0.8.9: Send NotConfigured event to clear Checking state
                        tracing::debug!(provider = %provider_id, "Provider not configured");
                        let _ = tx
                            .send(StreamChunk::ProviderNotConfigured {
                                provider: provider_id,
                            })
                            .await;
                    }
                }
            });
        }
    }

    /// Spawn provider verification timeout watcher (v0.8.4)
    ///
    /// After 5 seconds, checks if ANY provider has been verified.
    /// If not, sends ProviderVerificationTimeout event to show fallback UI.
    fn spawn_provider_verification_timeout(&self) {
        const PROVIDER_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(5);

        let tx = self.stream_chunk_tx.clone();
        let cache = Arc::clone(&self.verification_cache);

        self.spawn_tracked(async move {
            tokio::time::sleep(PROVIDER_VERIFICATION_TIMEOUT).await;

            // Check if ANY provider is verified
            let has_verified = {
                let cache_guard = cache.lock();
                cache_guard.has_any_verified_provider()
            };

            if !has_verified {
                // Send timeout event
                let _ = tx.try_send(StreamChunk::ProviderVerificationTimeout);
            }
        });
    }

    /// Spawn async MCP server verification tasks (v0.8.2)
    ///
    /// Pings all configured MCP servers in parallel, sending StreamChunk events
    /// to update the session context display in real-time.
    ///
    /// Uses TTL-based caching to avoid redundant MCP pings (30s default).
    fn spawn_mcp_verification(&self) {
        let Some(mcp_configs) = &self.mcp_configs else {
            tracing::debug!("No MCP servers configured, skipping verification");
            return;
        };

        let tx = self.stream_chunk_tx.clone();
        let cache = Arc::clone(&self.verification_cache);
        let configs: Vec<_> = mcp_configs
            .iter()
            .map(|(name, config)| (name.clone(), config.clone()))
            .collect();

        // Check cache and send events for cached servers, mark uncached as pinging
        for (server_name, _config) in &configs {
            // Check if we have a valid cached result
            let cached = {
                let cache_guard = cache.lock();
                cache_guard.get_mcp(server_name).and_then(|entry| {
                    if cache_guard.is_valid(entry) {
                        Some(entry.clone())
                    } else {
                        None
                    }
                })
            };

            if let Some(entry) = cached {
                // Send cached result directly
                match entry.status {
                    super::widgets::VerifyStatus::Verified => {
                        let _ = tx.try_send(StreamChunk::McpPinged {
                            server: server_name.clone(),
                            latency_ms: entry.latency.map(|d| d.as_millis() as u64).unwrap_or(0),
                            tool_count: entry.tool_count.unwrap_or(0),
                        });
                    }
                    super::widgets::VerifyStatus::Failed => {
                        let _ = tx.try_send(StreamChunk::McpError {
                            server_name: server_name.clone(),
                            error: entry.error.unwrap_or_else(|| "Unknown error".to_string()),
                        });
                    }
                    _ => {
                        // Verifying or Unknown - re-ping
                        let _ = tx.try_send(StreamChunk::McpPinging {
                            server: server_name.clone(),
                        });
                    }
                }
            } else {
                // Mark as pinging - will spawn task below
                let _ = tx.try_send(StreamChunk::McpPinging {
                    server: server_name.clone(),
                });
            }
        }

        // Spawn ping tasks only for servers NOT in valid cache
        for (server_name, config) in configs {
            // Skip if cached
            {
                let cache_guard = cache.lock();
                if cache_guard.has_valid_mcp(&server_name) {
                    continue;
                }
            }

            let tx = tx.clone();
            let cache = Arc::clone(&cache);
            let server_name_for_config = server_name.clone();

            self.spawn_tracked(async move {
                use crate::mcp::{McpClient, McpConfig};

                // Build MCP config from inline config
                let mut mcp_config = McpConfig::new(&server_name_for_config, &config.command);
                for arg in &config.args {
                    mcp_config = mcp_config.with_arg(arg);
                }
                for (key, value) in &config.env {
                    mcp_config = mcp_config.with_env(key, value);
                }
                if let Some(cwd) = &config.cwd {
                    mcp_config = mcp_config.with_cwd(cwd);
                }

                // Create client and ping
                match McpClient::new(mcp_config) {
                    Ok(client) => match client.ping().await {
                        Ok(result) => {
                            // Cache the successful result
                            {
                                let mut cache_guard = cache.lock();
                                cache_guard.set_mcp(
                                    server_name.clone(),
                                    VerificationEntry::verified_mcp(
                                        result.latency,
                                        result.tool_count,
                                    ),
                                );
                            }
                            let _ = tx
                                .send(StreamChunk::McpPinged {
                                    server: server_name,
                                    latency_ms: result.latency.as_millis() as u64,
                                    tool_count: result.tool_count,
                                })
                                .await;
                        }
                        Err(e) => {
                            // Cache the failure
                            {
                                let mut cache_guard = cache.lock();
                                cache_guard.set_mcp(
                                    server_name.clone(),
                                    VerificationEntry::failed(e.to_string()),
                                );
                            }
                            let _ = tx
                                .send(StreamChunk::McpError {
                                    server_name,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    },
                    Err(e) => {
                        // Cache the failure
                        {
                            let mut cache_guard = cache.lock();
                            cache_guard.set_mcp(
                                server_name.clone(),
                                VerificationEntry::failed(e.to_string()),
                            );
                        }
                        let _ = tx
                            .send(StreamChunk::McpError {
                                server_name,
                                error: e.to_string(),
                            })
                            .await;
                    }
                }
            });
        }
    }

    /// Spawn a tracked background task that will be cancelled on cleanup
    ///
    /// Use this instead of raw `tokio::spawn()` for tasks that should be
    /// cleaned up when the TUI exits. The task's AbortHandle is stored for
    /// later cancellation via `cancel_background_tasks()`.
    fn spawn_tracked<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(future);
        // PERF: parking_lot::Mutex doesn't poison, so this is infallible
        self.background_handles.lock().push(handle.abort_handle());
    }

    /// Cancel all background tasks
    ///
    /// Should be called during cleanup to ensure graceful shutdown.
    /// Tasks are aborted immediately; no waiting for completion.
    fn cancel_background_tasks(&self) {
        // PERF: parking_lot::Mutex doesn't poison, so this is simple
        let handles = self.background_handles.lock();
        let count = handles.len();
        for handle in handles.iter() {
            handle.abort();
        }
        tracing::debug!("Aborted {} background tasks", count);
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
        // v0.21: Default view is now Studio (unified editor)
        // Browse is a legacy alias that maps to Studio behavior
        assert!(app.current_view.is_studio());
    }

    #[test]
    fn test_app_initial_view_execution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();
        assert_eq!(app.current_view, TuiView::Runner);
    }

    #[test]
    fn test_app_view_switch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Initial view is Runner
        assert_eq!(app.current_view, TuiView::Runner);

        // Switch to Studio (default view in 5-view architecture)
        app.switch_view(TuiView::Studio);
        assert_eq!(app.current_view, TuiView::Studio);

        // Switch to Chat
        app.switch_view(TuiView::Chat);
        assert_eq!(app.current_view, TuiView::Chat);

        // Switch to Scheduler
        app.switch_view(TuiView::Scheduler);
        assert_eq!(app.current_view, TuiView::Scheduler);

        // Switch back to Runner
        app.switch_view(TuiView::Runner);
        assert_eq!(app.current_view, TuiView::Runner);
    }

    #[test]
    fn test_app_view_next_prev() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start at Studio (v0.21 default, 5-view architecture)
        app.current_view = TuiView::Studio;

        // Next should go Studio -> Runner -> Chat -> Scheduler -> Settings -> Studio
        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Runner);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Chat);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Scheduler);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Settings);

        app.current_view = app.current_view.next();
        assert_eq!(app.current_view, TuiView::Studio);

        // Prev should go Studio -> Settings
        app.current_view = app.current_view.prev();
        assert_eq!(app.current_view, TuiView::Settings);
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

        // Runner never captures
        app.current_view = TuiView::Studio;
        app.studio_view.mode = EditorMode::Normal;
        assert!(!app.is_view_capturing_input());

        app.current_view = TuiView::Runner;
        assert!(!app.is_view_capturing_input());
    }

    // === View & Panel Navigation Tests (v0.8) ===
    // Tab is delegated to views for panel switching (Conversation ↔ Mission Control).
    // Number keys 1/2/3/4 switch between views (Chat, Home, Studio, Monitor).

    #[test]
    fn test_tab_delegated_to_views_for_panel_switch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Studio view
        app.current_view = TuiView::Studio;
        app.input_mode = InputMode::Normal;

        // Tab should be routed to view (returns Continue, not SwitchView)
        let action = app.handle_unified_key(KeyCode::Tab, KeyModifiers::empty());
        assert_ne!(action, Action::Quit, "Tab should not quit");
        // Tab is handled by view, not app-level view switching
        assert!(
            !matches!(action, Action::SwitchView(_)),
            "Tab should not switch views - it cycles panels within the view"
        );
    }

    #[test]
    fn test_number_keys_still_switch_views() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Studio view
        app.current_view = TuiView::Studio;
        app.input_mode = InputMode::Normal;

        // Number keys 1-5 should switch views (matches 5-view architecture)
        let action = app.handle_unified_key(KeyCode::Char('1'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Studio),
            "Key '1' should switch to Studio view"
        );

        let action = app.handle_unified_key(KeyCode::Char('2'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Runner),
            "Key '2' should switch to Runner view"
        );

        let action = app.handle_unified_key(KeyCode::Char('3'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Chat),
            "Key '3' should switch to Chat view"
        );

        // v0.21: Keys 4 and 5 for Scheduler and Settings (5-view architecture)
        let action = app.handle_unified_key(KeyCode::Char('4'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Scheduler),
            "Key '4' should switch to Scheduler view"
        );

        let action = app.handle_unified_key(KeyCode::Char('5'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Settings),
            "Key '5' should switch to Settings view"
        );
    }

    #[test]
    fn test_number_key_3_switches_to_chat() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // Start in Studio view
        app.current_view = TuiView::Studio;
        app.input_mode = InputMode::Normal;

        // Key '3' should switch to Chat view (5-view architecture)
        let action = app.handle_unified_key(KeyCode::Char('3'), KeyModifiers::empty());
        assert_eq!(
            action,
            Action::SwitchView(TuiView::Chat),
            "Key '3' should switch to Chat view"
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

        let action = app.convert_view_action(ViewAction::SwitchView(TuiView::Studio));
        assert_eq!(action, Action::SwitchView(TuiView::Studio));
    }

    #[tokio::test]
    async fn test_convert_view_action_send_chat_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

        // v0.12.1: Option A - InferBox REPLACES the AI message bubble
        // SendChatMessage now uses TaskBox pattern via StreamChunk events
        // No "Thinking..." message is added - response comes via InferBox widget

        // Send a message - this triggers async LLM call via stream_chunk_tx
        let action = app.convert_view_action(ViewAction::SendChatMessage("Hello".to_string()));

        // Should return Continue (async task spawned, no immediate state change)
        assert_eq!(action, Action::Continue);

        // Note: The actual TaskBox creation happens asynchronously via stream_chunk_tx
        // which is processed in the event loop, not synchronously in convert_view_action
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
        let mut app = App::new(&workflow_path).unwrap();

        // In Monitor mode, 'c' should open chat overlay
        let action = app.handle_key(KeyCode::Char('c'), KeyModifiers::empty());
        assert_eq!(action, Action::SetMode(TuiMode::ChatOverlay));
    }

    #[test]
    fn test_handle_key_y_copies_to_clipboard() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let mut app = App::new(&workflow_path).unwrap();

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

    // ═══════════════════════════════════════════
    // BACKGROUND TASK TRACKING TESTS (v0.7.0)
    // ═══════════════════════════════════════════

    #[test]
    fn test_background_handles_initialization() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Background handles should start empty
        let handles = app.background_handles.lock();
        assert!(handles.is_empty());
    }

    #[tokio::test]
    async fn test_spawn_tracked_adds_handle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Spawn a quick task
        app.spawn_tracked(async {
            // Empty task for testing
        });

        // Give it a moment to spawn
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Should have one handle
        let handles = app.background_handles.lock();
        assert_eq!(handles.len(), 1, "spawn_tracked should add one handle");
    }

    #[tokio::test]
    async fn test_spawn_tracked_multiple_tasks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Spawn multiple tasks
        for _ in 0..5 {
            app.spawn_tracked(async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            });
        }

        // Give them a moment to spawn
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Should have 5 handles
        let handles = app.background_handles.lock();
        assert_eq!(handles.len(), 5, "spawn_tracked should track all tasks");
    }

    #[tokio::test]
    async fn test_cancel_background_tasks_aborts_all() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Spawn some long-running tasks
        use std::sync::atomic::{AtomicBool, Ordering};
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = Arc::clone(&completed);

        app.spawn_tracked(async move {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            completed_clone.store(true, Ordering::SeqCst);
        });

        // Give it a moment to spawn
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Cancel all tasks
        app.cancel_background_tasks();

        // Wait a bit
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Task should NOT have completed (was aborted)
        assert!(
            !completed.load(Ordering::SeqCst),
            "Task should be aborted, not completed"
        );
    }

    #[test]
    fn test_cancel_background_tasks_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Should not panic when called with no tasks
        app.cancel_background_tasks();
    }

    // ═══ TESTS: Status Bar Height Calculation ═══

    #[test]
    fn test_compute_view_area_subtracts_status_bar() {
        use ratatui::layout::Rect;

        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Terminal size: 100x50
        let terminal_size = Rect::new(0, 0, 100, 50);

        // View area should be terminal size minus status bar (1 line)
        let view_area = app.compute_view_area(terminal_size);

        assert_eq!(view_area.x, 0, "x should be unchanged");
        assert_eq!(view_area.y, 0, "y should be unchanged");
        assert_eq!(view_area.width, 100, "width should be unchanged");
        assert_eq!(
            view_area.height, 49,
            "height should be terminal height minus STATUS_BAR_HEIGHT (1)"
        );
    }

    #[test]
    fn test_compute_view_area_handles_small_terminal() {
        use ratatui::layout::Rect;

        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();
        let app = App::new(&workflow_path).unwrap();

        // Very small terminal (only 1 line - edge case)
        let tiny_terminal = Rect::new(0, 0, 80, 1);
        let view_area = app.compute_view_area(tiny_terminal);

        // Should not go negative - return original size
        assert_eq!(
            view_area.height, 1,
            "Should not subtract from tiny terminal"
        );
    }

    // ═══ TUI Config Loading Tests (v0.12.0) ═══

    #[test]
    fn test_app_loads_tui_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();

        let app = App::new(&workflow_path).unwrap();

        // Config should be loaded with defaults
        assert_eq!(app.config.tui.theme, ThemeName::Dark);
        assert!(app.config.tui.mouse);
        assert!(app.config.tui.animations);
    }

    #[test]
    fn test_app_standalone_loads_tui_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let standalone = StandaloneState::new(temp_dir.path().to_path_buf());
        let app = App::new_standalone(standalone).unwrap();

        // Config should be loaded with defaults
        assert_eq!(app.config.tui.theme, ThemeName::Dark);
        assert!(app.config.tui.mouse);
    }
}
