// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.
//!
//! # Module Structure
//!
//! - `mod.rs` - App struct + run_unified event loop
//! - `types.rs` - Action enum and shared types
//! - `lifecycle.rs` - Constructors, initialization, cleanup
//! - `events.rs` - Key/mouse handling, event polling
//! - `routing.rs` - View switching, action dispatch
//! - `commands.rs` - Chat verb handlers (/infer, /exec, etc.)
//! - `render.rs` - Frame rendering

mod commands;
mod events;
mod lifecycle;
mod render;
mod routing;
mod routing_models;
mod types;

// Re-export Action enum for external use
pub use types::Action;

use std::io::{self, Stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

// PERF: Use parking_lot::Mutex instead of std::sync::Mutex
// - No poisoning (simpler error handling)
// - Faster lock acquisition for short critical sections
use parking_lot::Mutex;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;

use crate::chat_agent::ChatAgent;
use nika_engine::error::{NikaError, Result};
use nika_engine::event::Event as NikaEvent;
use nika_engine::mcp::McpClientPool;
use nika_engine::provider::rig::StreamChunk;

use super::config::{ThemeName, TuiConfig};
use super::cosmic_theme::CosmicTheme;
// FocusState import removed — field was never read (Phase 1 cleanup)
use super::mode::InputMode;
use super::standalone::StandaloneState;
use super::startup;
use super::state::TuiState;
use super::theme::Theme;
use super::verification::VerificationCache;
use super::views::{CommandView, ControlView, StudioView, TuiView, View};

// Note: Frame rate is now adaptive - see FAST_TICK_MS/SLOW_TICK_MS in run_unified()

/// Main TUI application
pub struct App {
    /// Path to the workflow being observed (None in standalone mode)
    pub(crate) workflow_path: std::path::PathBuf,
    /// Terminal backend (initialized on run)
    pub(crate) terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    /// TUI state (execution mode)
    pub(crate) state: TuiState,
    /// Standalone state (file browser mode)
    pub(crate) standalone_state: Option<StandaloneState>,
    /// Cosmic theme
    pub(crate) cosmic_theme: CosmicTheme,
    /// Color theme
    pub(crate) theme: Theme,
    /// Event receiver from runtime
    pub(crate) event_rx: Option<mpsc::Receiver<NikaEvent>>,
    /// Broadcast receiver from runtime
    pub(crate) broadcast_rx: Option<broadcast::Receiver<NikaEvent>>,
    /// Should quit flag
    pub(crate) should_quit: bool,
    /// Last Ctrl+C press time (for double-tap quit like Claude Code)
    pub(crate) last_ctrl_c: Option<std::time::Instant>,
    /// Workflow completed flag
    pub(crate) workflow_done: bool,
    /// Status message for feedback (clipboard copy, export, etc.)
    pub(crate) status_message: Option<(String, std::time::Instant)>,
    /// Retry requested flag (TIER 1.2) - caller should re-run workflow
    pub(crate) retry_requested: bool,
    /// Launch wizard flag
    pub(crate) should_launch_wizard: bool,
    // ═══ 3-View Architecture + Navigation 2.0 ═══
    /// Current active view
    pub(crate) current_view: TuiView,
    /// Current input mode (Normal, Insert, Command, Search)
    pub(crate) input_mode: InputMode,
    // focus_state removed — was never read (Phase 1 cleanup)
    /// Command view (wraps ChatView + MonitorView)
    pub(crate) command_view: CommandView,
    /// Studio view state
    pub(crate) studio_view: StudioView,
    /// Control view (wraps SettingsView)
    pub(crate) control_view: ControlView,
    // ═══ LLM Integration for ChatView ═══
    /// Channel for receiving LLM responses (complete responses)
    pub(crate) llm_response_rx: mpsc::Receiver<String>,
    // llm_response_tx removed — sender held but never used (Phase 1 cleanup)
    /// Channel for streaming tokens (real-time display)
    pub(crate) stream_chunk_rx: mpsc::Receiver<StreamChunk>,
    /// Sender for streaming tokens (passed to ChatAgent)
    pub(crate) stream_chunk_tx: mpsc::Sender<StreamChunk>,
    // ═══ ChatAgent for full AI interface (Task 5.1) ═══
    /// ChatAgent for handling 5 verb commands in ChatView
    pub(crate) chat_agent: Option<ChatAgent>,
    // ═══ MCP Client Pool ═══
    /// Centralized MCP client pool for lazy init, config management, and shutdown.
    /// Replaces the previous mcp_client_cache + mcp_configs pair.
    pub(crate) mcp_pool: McpClientPool,
    // ═══ Background Task Tracking ═══
    /// AbortHandles for tracked background tasks
    /// Enables proper cancellation on app exit via abort_all()
    pub(crate) background_handles: Arc<Mutex<Vec<AbortHandle>>>,
    // ═══ Session Persistence ═══
    /// Current session ID (for save/load)
    pub(crate) session_id: Option<String>,
    // ═══ TUI Config ═══
    // config removed — loaded but never read (Phase 1 cleanup)
    // ═══ Performance: Reusable Event Buffer ═══
    /// PERF: Pre-allocated buffer for poll_runtime_events to avoid
    /// allocating a new Vec on every frame (60 FPS = 60 allocations/sec saved)
    pub(crate) event_buffer: Vec<NikaEvent>,
    // ═══ Connection Verification Cache ═══
    /// TTL-based cache for provider and MCP server verification results.
    /// Prevents redundant API calls when opening/refreshing the provider selector.
    pub(crate) verification_cache: Arc<Mutex<VerificationCache>>,
    // ═══ Startup Loading State ═══
    /// True until on_enter() completes — shows loading indicator in first frame
    pub(crate) loading: bool,
    // ═══ Star Field ═══
    /// Pre-computed star positions for O(stars) rendering instead of O(w*h)
    pub(crate) star_field: super::widgets::StarField,
    /// PERF(M7): Last user interaction time — star animation decays after 5s idle
    pub(crate) last_interaction: std::time::Instant,
    // ═══ First-Launch Welcome Hint ═══
    /// Shown on first launch, auto-dismisses after 10s or on first keypress
    pub(crate) welcome_hint_until: Option<std::time::Instant>,
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

        // Load TUI configuration from nika.toml (or .nika/config.toml fallback)
        let config = TuiConfig::load_or_default();

        // Initialize views
        let mut command_view = CommandView::new();
        // Apply config defaults to ChatView
        Self::apply_chat_config(&mut command_view, &config);

        let mut studio_view = StudioView::new();
        // Load workflow file into studio view
        if let Err(e) = studio_view.load_file(workflow_path.to_path_buf()) {
            tracing::error!(
                "Failed to load workflow file {}: {}",
                workflow_path.display(),
                e
            );
        }
        let control_view = ControlView::new();

        // Initialize LLM response channel
        let (_llm_response_tx, llm_response_rx) = mpsc::channel(32);
        // P1 Fix: Increase buffer from 256 to 512 for fast providers like Groq (~200 tok/s)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(512);

        // PERF: Defer ChatAgent creation to first /infer or /exec command.
        // ChatAgent::new() initializes reqwest::Client + TLS backend (5-20ms).
        let chat_agent: Option<ChatAgent> = None;

        // Initialize cosmic theme from config
        let theme_variant = match config.tui.theme {
            ThemeName::Dark => crate::tokens::CosmicVariant::CosmicDark,
            ThemeName::Light => crate::tokens::CosmicVariant::CosmicLight,
            ThemeName::Solarized => crate::tokens::CosmicVariant::CosmicViolet,
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
            should_launch_wizard: false,
            // 3-view architecture - start in Monitor mode for workflow execution
            current_view: TuiView::Command,
            input_mode: InputMode::Normal,
            command_view,
            studio_view,
            control_view,
            llm_response_rx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_pool: McpClientPool::new(nika_engine::event::EventLog::new()),
            background_handles: Arc::new(Mutex::new(Vec::new())),
            session_id: None,
            event_buffer: Vec::with_capacity(64), // PERF: Pre-allocated buffer
            verification_cache: Arc::new(Mutex::new(VerificationCache::default())),
            loading: true,
            star_field: super::widgets::StarField::new(),
            last_interaction: std::time::Instant::now(),
            welcome_hint_until: Self::check_first_launch(),
        })
    }

    /// Create a new TUI application in standalone mode (file browser)
    pub fn new_standalone(standalone_state: StandaloneState) -> Result<Self> {
        // Use a dummy workflow path for standalone mode
        let workflow_path = standalone_state.root.clone();
        let state = TuiState::new("Standalone Mode");

        // Load TUI configuration from nika.toml (or .nika/config.toml fallback)
        let config = TuiConfig::load_or_default();

        // Initialize views
        let mut command_view = CommandView::new();
        // Apply config defaults to ChatView
        Self::apply_chat_config(&mut command_view, &config);

        let studio_view = StudioView::new();
        let control_view = ControlView::new();

        // Initialize LLM response channel
        let (_llm_response_tx, llm_response_rx) = mpsc::channel(32);
        // P1 Fix: Increase buffer from 256 to 512 for fast providers like Groq (~200 tok/s)
        let (stream_chunk_tx, stream_chunk_rx) = mpsc::channel(512);

        // PERF: Defer ChatAgent creation to first /infer or /exec command.
        // ChatAgent::new() initializes reqwest::Client + TLS backend (5-20ms).
        let chat_agent: Option<ChatAgent> = None;

        // Initialize cosmic theme from config
        let theme_variant = match config.tui.theme {
            ThemeName::Dark => crate::tokens::CosmicVariant::CosmicDark,
            ThemeName::Light => crate::tokens::CosmicVariant::CosmicLight,
            ThemeName::Solarized => crate::tokens::CosmicVariant::CosmicViolet,
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
            should_launch_wizard: false,
            // 3-view architecture - start in Studio mode for standalone
            current_view: TuiView::Studio,
            input_mode: InputMode::Normal,
            command_view,
            studio_view,
            control_view,
            llm_response_rx,
            stream_chunk_rx,
            stream_chunk_tx,
            chat_agent,
            mcp_pool: McpClientPool::new(nika_engine::event::EventLog::new()),
            background_handles: Arc::new(Mutex::new(Vec::new())),
            session_id: None,
            event_buffer: Vec::with_capacity(64), // PERF: Pre-allocated buffer
            verification_cache: Arc::new(Mutex::new(VerificationCache::default())),
            loading: true,
            star_field: super::widgets::StarField::new(),
            last_interaction: std::time::Instant::now(),
            welcome_hint_until: Self::check_first_launch(),
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
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )
        .map_err(|e| NikaError::TuiError {
            reason: format!("Failed to enter alternate screen: {}", e),
        })?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| NikaError::TuiError {
            reason: format!("Failed to create terminal: {}", e),
        })?;

        self.terminal = Some(terminal);
        Ok(())
    }

    /// Set the event receiver from runtime
    pub fn with_event_receiver(mut self, rx: mpsc::Receiver<NikaEvent>) -> Self {
        self.event_rx = Some(rx);
        self
    }

    /// Set the broadcast receiver from runtime
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
        if view == TuiView::Command {
            self.input_mode = InputMode::Insert;
        }
        self
    }

    /// Load a workflow file into Studio view
    ///
    /// Used by `nika studio <file>` to open a specific workflow.
    pub fn with_studio_file(mut self, path: std::path::PathBuf) -> Self {
        if let Err(e) = self.studio_view.load_file(path.clone()) {
            tracing::error!("Failed to load studio file {}: {}", path.display(), e);
            self.command_view.chat.add_system_message(format!(
                "Failed to load file {}: {}",
                path.display(),
                e
            ));
        }
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

    /// Run the TUI with unified 3-view architecture
    ///
    /// This is the new entry point that supports all 3 views with unified
    /// navigation. The views are:
    /// - Studio (1/s): YAML editor + file browser
    /// - Command (2/c): Chat + Monitor modes (Ctrl+M to toggle)
    /// - Control (3/x): Configuration and preferences
    ///
    /// Returns `Ok(true)` if the wizard should be launched after TUI exit
    pub fn run_unified(mut self) -> Result<bool> {
        tracing::info!("TUI (unified) started");

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

        // Results are cached for 30s to avoid redundant API calls
        self.spawn_provider_verification();
        self.spawn_provider_verification_timeout();
        self.spawn_mcp_verification();

        // Initialize terminal
        self.init_terminal()?;

        // Render skeleton frame BEFORE on_enter() so users see the UI
        // immediately instead of a blank screen during blocking I/O
        // (on_enter does git status + tree build = 100-700ms)
        self.render_unified_frame()?;

        // Now populate view state (blocking: git cache, tree build)
        self.call_view_on_enter(self.current_view);
        self.loading = false;

        // PERF: Adaptive frame rate
        // - Fast (60 FPS) when streaming or animations active
        // - Slow (10 FPS) when idle to save CPU
        const FAST_TICK_MS: u64 = 16; // 60 FPS for smooth animations
        const SLOW_TICK_MS: u64 = 50; // 20 FPS when idle (50ms max input lag)

        // Track if user had recent input (stays fast for one frame after input)
        let mut had_recent_input = true; // Start fast on first frame

        loop {
            // 1. Poll runtime events (same as run())
            self.poll_runtime_events();

            // 2. Determine if we need fast rendering
            let is_streaming = self.command_view.chat.is_streaming;
            let has_inline_content = !self.command_view.chat.inline_content.is_empty();
            let is_workflow_active = self.state.is_running();
            let needs_fast_render =
                is_streaming || has_inline_content || had_recent_input || is_workflow_active;
            had_recent_input = false; // Consume the flag

            // 3. Update elapsed time and animations
            self.state.tick();

            // PERF: Only tick the active view (saves ~3 view ticks per frame)
            match self.current_view {
                TuiView::Studio => self.studio_view.tick(&mut self.state),
                TuiView::Command => self.command_view.tick(&mut self.state),
                TuiView::Control => self.control_view.tick(&mut self.state),
            }
            // Background tick: ChatView needs ticking for streaming animations
            // even when user is on a different view
            if self.current_view != TuiView::Command && self.command_view.chat.is_streaming {
                self.command_view.chat.tick();
            }

            // 4. PERF: Skip render when nothing changed (DirtyFlags guard).
            // PERF(M7): Star animation only renders for 5s after last interaction,
            // then goes idle. Prevents perpetual 10 FPS rendering when user is AFK.
            let star_active = self.last_interaction.elapsed().as_secs() < 5;
            let needs_render = self.state.dirty.any()
                || needs_fast_render
                || (star_active && self.state.frame % 6 == 0);
            if needs_render {
                self.render_unified_frame()?;
            }

            // 5. Poll input events (adaptive tick rate)
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

                // PERF(M7): Reset interaction timer on any input
                self.last_interaction = std::time::Instant::now();

                let action = match event {
                    Event::Key(key) => self.handle_unified_key(key.code, key.modifiers),
                    Event::Mouse(mouse) => {
                        // Dispatch mouse events to the active view
                        if self.current_view == TuiView::Command {
                            let size = self
                                .terminal
                                .as_ref()
                                .map(|t| t.size().unwrap_or_default())
                                .unwrap_or_default();
                            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                            self.command_view.chat.handle_mouse(
                                mouse.kind,
                                mouse.column,
                                mouse.row,
                                area,
                            );
                        }
                        self.state.dirty.mark_all();
                        Action::Continue
                    }
                    Event::Resize(_, _) => {
                        // Force full re-render on terminal resize
                        self.state.dirty.mark_all();
                        Action::Continue
                    }
                    _ => Action::Continue,
                };
                self.apply_action(action);

                // After input/resize, render at full speed on next frame
                had_recent_input = true;
            }

            // 7. Check quit flag
            if self.should_quit {
                // Save chat session before quitting
                self.save_current_session();
                break;
            }
        }

        // Cancel all background tasks before cleanup
        self.cancel_background_tasks();

        // Save wizard flag before cleanup
        let launch_wizard = self.should_launch_wizard;

        // Cleanup and return wizard flag
        self.cleanup()?;
        Ok(launch_wizard)
    }

    /// Get current view
    pub fn current_view(&self) -> TuiView {
        self.current_view
    }

    /// Switch to a different view
    pub fn switch_view(&mut self, view: TuiView) {
        self.current_view = view;
    }

    /// Check if retry was requested
    pub fn wants_retry(&self) -> bool {
        self.retry_requested
    }

    /// Clear retry request flag
    pub fn clear_retry_request(&mut self) {
        self.retry_requested = false;
    }

    /// Check if this is the first TUI launch; returns Some(deadline) if first launch
    ///
    /// Uses `.nika/tui_first_run` marker file. On first launch, creates the marker
    /// and returns a 10-second deadline for the welcome hint.
    fn check_first_launch() -> Option<std::time::Instant> {
        let marker = std::path::Path::new(".nika/tui_first_run");
        if marker.exists() {
            return None;
        }
        // Create marker (ignore errors — non-critical)
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(marker, "1").ok();
        Some(std::time::Instant::now() + Duration::from_secs(10))
    }

    /// Apply chat-related config settings to the ChatView
    fn apply_chat_config(command_view: &mut CommandView, config: &TuiConfig) {
        // Wire chat.default_provider and chat.default_model
        if let Some(ref provider_id) = config.chat.default_provider {
            use nika_core::catalogs::{default_model_for_provider, find_provider};
            let catalog_model =
                default_model_for_provider(provider_id).unwrap_or(provider_id.as_str());
            let display_name = find_provider(provider_id)
                .map(|p| p.name)
                .unwrap_or(provider_id.as_str());
            // Only override if user hasn't set a model override too
            let model = config
                .chat
                .default_model
                .as_deref()
                .unwrap_or(catalog_model);
            command_view.chat.set_model(model);
            command_view.chat.set_provider(display_name);
            command_view.chat.provider.id = provider_id.clone();
        } else if let Some(ref model) = config.chat.default_model {
            command_view.chat.set_model(model.as_str());
        }

        // Wire chat.deep_thinking
        if config.chat.deep_thinking {
            command_view.chat.deep_thinking = true;
        }
    }

    /// Dismiss the welcome hint (called on first keypress)
    pub(crate) fn dismiss_welcome_hint(&mut self) {
        self.welcome_hint_until = None;
    }
}

// Implementations split into submodules:
// - lifecycle.rs: spawn_*, cleanup, save_current_session, toggle_theme, set_status
// - events.rs: poll_runtime_events, handle_unified_key, handle_mouse, handle_ctrl_c
// - routing.rs: apply_action, switch_to_view, handle_scroll_*
// - commands.rs: ensure_chat_agent, build_conversation_context, init_mcp_clients
// - render.rs: render_unified_frame

impl Drop for App {
    fn drop(&mut self) {
        // Cancel background tasks BEFORE terminal cleanup to prevent leaks
        self.cancel_background_tasks();
        // Ensure terminal is cleaned up
        if self.terminal.is_some() {
            let _ = self.cleanup();
        }
    }
}
