// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat View - AI Agent conversation interface
//!
//! Layout (v2 - Chat UX Enrichment):
//! ```text
//! +-----------------------------------------------------------------------------+
//! | SESSION CONTEXT: tokens 1.2k/200k | cost $0.42 | MCP: ◉ novanet | ⏱ 3m 12s |
//! +-----------------------------------------------------+-----------------------+
//! | Conversation history                                | 🎯 ACTIVITY STACK     |
//! | - User messages                                     | 🔥 HOT (executing)    |
//! | - Nika responses with inline MCP/Infer boxes        | 🟡 WARM (recent)      |
//! | ╭─ 🔧 MCP CALL: novanet_describe ─────── ✅ 1.2s ─╮ | ⚪ QUEUED (waiting)   |
//! | │ 📥 params: { "entity": "qr-code" }              │ |                       |
//! | │ 📤 result: { "display_name": "QR Code" }        │ |                       |
//! | ╰─────────────────────────────────────────────────╯ |                       |
//! +-----------------------------------------------------+-----------------------+
//! | > Input field                                              [⌘K] commands   |
//! +-----------------------------------------------------------------------------+
//! ```

// Blanket #![allow(dead_code)] removed — Phase 1 Batch 1.1.2
// Per-item allows added where needed for staged code.

use std::time::Instant;

use chrono::Local;

use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{ListState, Paragraph, Widget},
    Frame,
};
use tui_input::Input;

use super::view_trait::View;
use super::ViewAction;

/// Check if the "command" modifier is pressed (Ctrl on Linux/Windows, Cmd on macOS)
/// On macOS, Cmd key maps to SUPER modifier in crossterm
pub(super) fn is_cmd_pressed(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::SUPER)
}
use crate::edit_history::EditHistory;
use crate::state::{ChatPanel, PanelScrollState, TuiState};
use crate::theme::{Theme, VerbColor};
use nika_engine::runtime::chat_workflow::{ChatWorkflow, Role as WorkflowRole};

// PERF: Pre-computed constants to avoid allocations in render loop
const SEPARATOR_52: &str = "╰───────────────────────────────────────────────────╯"; // MCP box bottom
                                                                                    // PERF: 200-char separator for dynamic slicing (avoids .repeat() allocation)
const SEPARATOR_200: &str = "────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
// tui utils moved to messages.rs
// Chat DAG Widgets (moved to views/chat/widgets/)
use self::widgets::{
    ChatDagPanel, ChatNodeKind, ChatNodeState, ChatTaskQueue, ChatTaskQueueItem, ChatTaskState,
    ChatTaskVerb, DagEdgeData, DagNodeData,
};
use crate::widgets::{
    centered_rect,
    // Task Box widgets
    task_box::{BoxState, RenderMode, TaskBox},
    ActivityItem,
    ActivityTemp,
    AgentPhase,
    AgentPhaseIndicator,
    ChatModeIndicator,
    CommandPalette,
    CommandPaletteState,
    // Confirmation dialog
    ConfirmAction,
    ConfirmDialog,
    ContextItem,
    CurrentVerb,
    DecryptVerb,
    HelpOverlay,
    HelpOverlayState,
    McpCallData,
    McpCallStatus,
    McpServerInfo,
    McpStatus,
    MemoryFile,
    Mention,
    MentionAutocomplete,
    MentionAutocompleteState,
    MentionSuggestion,
    MentionType,
    MissionControlPanel,
    ParsedInput,
    ProStatusBar,
    ProviderModal,
    ProviderModalState,
    SessionContext,
    SessionMetrics,
    StreamingDecrypt,
    SystemCommand,
    TurnMetrics,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Submodules
// ═══════════════════════════════════════════════════════════════════════════════

mod activity;
mod command;
mod cursor;
mod dag_panel;
mod helpers;
mod hints;
mod input;
mod keys;
mod layout;
mod mcp_tracking;
mod mentions;
mod message_ops;
mod messages;
mod mode_config;
mod mouse;
mod render;
mod scroll;
mod search;
mod selection;
mod session;
mod streaming;
mod task_boxes;
mod types;
pub mod widgets;

use helpers::categorize_error;
pub use types::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Chat View State
// ═══════════════════════════════════════════════════════════════════════════════

/// Chat view state
pub struct ChatView {
    /// Conversation history
    pub messages: Vec<ChatMessage>,
    /// Current input buffer (tui-input for proper cursor handling)
    pub input: Input,
    /// System clipboard for Ctrl+C/V (optional - may fail on headless)
    clipboard: Option<Clipboard>,
    /// Command history (for up/down navigation)
    pub history: Vec<String>,
    /// History navigation index
    pub history_index: Option<usize>,
    /// Whether streaming response is in progress
    pub is_streaming: bool,
    /// Partial response accumulated during streaming
    pub partial_response: String,
    /// Active LLM provider (consolidated: id, name, model, kind)
    pub provider: ActiveProvider,

    // === Chat UX Enrichment (v2) ===
    /// Session context with tokens, cost, MCP status
    pub session_context: SessionContext,
    /// Activity stack items (hot/warm/queued)
    pub activity_items: Vec<ActivityItem>,
    /// Command palette state (⌘K)
    pub command_palette: CommandPaletteState,
    /// Provider modal state (⌘P)
    pub provider_modal: ProviderModalState,
    // Native model management is handled via nika commands (nika model list/pull/info)
    /// Inline content for current streaming (MCP calls, infer boxes)
    pub inline_content: Vec<InlineContent>,
    /// Animation frame counter (for spinners)
    pub frame: u8,

    // === Chat Mode Indicators (v2.1 - Claude Code-like UX) ===
    /// Current chat mode (Chat or Agent)
    pub chat_mode: ChatMode,
    /// Whether deep thinking (extended_thinking) is enabled
    pub deep_thinking: bool,

    // === Thinking Accumulation ===
    /// Accumulated thinking content during streaming
    /// Attached to the final message when stream completes
    pub pending_thinking: Option<String>,

    // === UX Hints ===
    /// Whether the @mention hint has been shown (show once per session)
    pub shown_file_hint: bool,

    // === Panel Navigation & Scroll ===
    /// Currently focused panel for Tab navigation
    pub focused_panel: ChatPanel,
    /// Conversation panel scroll state (messages)
    pub conversation_scroll: PanelScrollState,
    /// Activity panel scroll state (activity items)
    pub activity_scroll: PanelScrollState,
    /// Cached panel rects for mouse click detection
    pub panel_rects: std::collections::HashMap<ChatPanel, Rect>,
    /// List state for conversation (ratatui StatefulWidget)
    pub conversation_list_state: ListState,

    /// Index of last copied message (for flash effect)
    pub copy_flash_index: Option<usize>,
    /// Frame when copy happened (for flash duration)
    pub copy_flash_start: u8,
    /// Matrix decrypt effect for streaming text (chaos → reveal)
    pub streaming_decrypt: StreamingDecrypt,
    /// Whether matrix decrypt effect is enabled
    pub matrix_effect_enabled: bool,
    /// Current agent execution phase (for real-time status)
    pub agent_phase: AgentPhase,
    /// Phase indicator widget with Matrix effect
    pub phase_indicator: AgentPhaseIndicator,
    /// Tool name currently being invoked (for Invoking phase)
    pub agent_phase_tool: Option<String>,

    /// Context items loaded via @ mentions
    pub context_items: Vec<ContextItem>,
    /// Memory files (CLAUDE.md, session memory)
    pub memory_files: Vec<MemoryFile>,
    /// Current verb being executed (for runtime display)
    pub current_verb: CurrentVerb,
    /// Runtime metrics for current turn
    pub turn_metrics: TurnMetrics,
    /// Session metrics for ProStatusBar
    pub session_metrics: SessionMetrics,

    /// Show messages as YAML tasks instead of chat bubbles
    pub show_yaml: bool,

    /// Text selection state (for copy support)
    pub text_selection: Option<ChatMessageSelection>,
    /// Whether a mouse drag selection is in progress
    pub is_selecting: bool,
    /// Cached line content for hit testing during selection
    /// Maps (message_index, line_in_message) -> (start_x, text_content)
    pub line_positions: Vec<ChatLinePosition>,

    /// Help overlay state (toggle with ? or F1)
    pub help_overlay: HelpOverlayState,

    /// Pending confirmation dialog (shown before destructive actions)
    pub pending_confirm: Option<ConfirmAction>,

    /// Edit history for input field (Ctrl+Z/Ctrl+Y)
    pub edit_history: EditHistory,

    /// Input scroll offset when content exceeds max visible lines
    pub input_scroll_offset: usize,
    /// Max visible lines for input area (excluding borders)
    pub input_max_lines: usize,

    /// Whether user is "at the bottom" of conversation
    /// When true, new messages auto-scroll. When false (user scrolled up), they don't.
    /// Reset to true when user sends a message or manually scrolls to bottom.
    pub user_at_bottom: bool,

    /// Set of message IDs with toggled thinking state (differs from default)
    /// Toggle individual messages with 't', toggle all with 'T'
    /// Uses stable message IDs instead of indices for stability
    pub thinking_collapsed: std::collections::HashSet<u64>,
    /// Whether thinking sections are expanded by default
    /// false = collapsed by default (show summary), true = expanded by default
    pub thinking_expanded_default: bool,
    /// Counter for generating unique message IDs
    message_id_counter: u64,

    /// State for the @ mention autocomplete popup
    pub mention_autocomplete: MentionAutocompleteState,

    /// Last failed MCP call for retry with Ctrl+R
    pub last_failed_mcp: Option<McpRetryInfo>,

    /// Whether search mode is active
    pub search_mode: bool,
    /// Current search query
    pub search_query: String,
    /// Indices of messages matching the search
    pub search_results: Vec<usize>,
    /// Current search result index (for navigation)
    pub search_current: usize,

    /// Current scroll velocity (lines per tick, can be fractional)
    pub scroll_velocity: f32,
    /// Accumulated fractional scroll offset (for sub-line precision)
    pub scroll_accumulator: f32,
    /// Whether smooth scrolling animation is active
    pub scroll_animating: bool,

    /// Whether DAG panel is visible (toggle with Ctrl+D)
    pub show_dag_panel: bool,
    /// DAG nodes (chat messages as graph nodes)
    pub dag_nodes: Vec<DagNodeData>,
    /// DAG edges (@ references between messages)
    pub dag_edges: Vec<DagEdgeData>,
    /// Task queue items (running/pending/completed tasks)
    pub task_queue: Vec<ChatTaskQueueItem>,
    /// Selected DAG node index (for navigation)
    pub dag_selected: Option<usize>,
    /// Cached DAG panel — rebuilt only when dag_nodes/dag_edges change
    cached_dag_panel: Option<ChatDagPanel>,

    /// Current render mode for inline TaskBoxes (Compact/Expanded/Full)
    /// Toggle with `m` key when not in input mode
    pub task_box_render_mode: RenderMode,

    /// Runtime workflow DAG that mirrors chat messages
    /// Used for /export yaml and unified execution with YAML workflows
    pub workflow: ChatWorkflow,

    // === PERF: Message render cache ===
    /// Cached rendered message items (rebuilt only on data change)
    cached_msg_items: Vec<ratatui::widgets::ListItem<'static>>,
    /// Length of phase-1 (message) items in cached_msg_items.
    /// Used to truncate back before re-appending phases 2-4 each frame.
    cached_base_len: usize,
    /// Message count when cache was built
    cached_msg_count: usize,
    /// Content width when cache was built
    cached_msg_width: usize,
    /// Streaming content length when cache was built (for incremental updates)
    cached_streaming_len: usize,
    /// Dirty flag: set when copy_flash, thinking_collapsed, or text_selection change
    /// Forces cache rebuild on next frame even if msg_count/width are unchanged
    pub(crate) cached_msg_dirty: bool,
}

impl ChatView {
    pub fn new() -> Self {
        // Detect initial model and provider from environment via catalog
        use nika_core::catalogs::{default_model_for_provider, providers::KNOWN_PROVIDERS};
        let (initial_model, provider_name, provider_id) = KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.requires_key)
            .find(|p| std::env::var(p.env_var).is_ok_and(|v| !v.is_empty()))
            .map(|p| {
                let model = default_model_for_provider(p.id).unwrap_or("unknown");
                (model.to_string(), p.name.to_string(), p.id.to_string())
            })
            .unwrap_or_else(|| {
                (
                    "No API Key".to_string(),
                    "None".to_string(),
                    "none".to_string(),
                )
            });

        // Initialize session context with detected MCP servers
        let mut session_context = SessionContext::new();
        session_context
            .mcp_servers
            .push(McpServerInfo::new("novanet"));

        // Clean welcome banner with verb examples and provider status
        let no_provider = provider_id == "none";
        let provider_warning = if no_provider {
            "\n  \u{26a0} No API key detected \u{2014} press Ctrl+P to configure a provider\n"
        } else {
            ""
        };
        let welcome_banner = format!(
            r#"
  ███╗   ██╗██╗██╗  ██╗ █████╗
  ████╗  ██║██║██║ ██╔╝██╔══██╗
  ██╔██╗ ██║██║█████╔╝ ███████║
  ██║╚██╗██║██║██╔═██╗ ██╔══██║
  ██║ ╚████║██║██║  ██╗██║  ██║
  ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝

  🦋 Workflow Engine  ·  💫 Semantic AI  ·  🦀 Rust

  · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·

  Type a message or use a slash command:

    /infer "prompt"      — LLM generation
    /exec  "command"     — Shell execution
    /fetch url           — HTTP request
    /invoke tool params  — MCP tool call
    /agent "goal"        — Multi-turn agent
{provider_warning}
  Press i to start typing, ? for help
"#
        );

        // Initialize ChatWorkflow with welcome message for DAG sync
        let mut workflow = ChatWorkflow::new();
        workflow.add_message(&welcome_banner, WorkflowRole::System);

        Self {
            messages: vec![ChatMessage {
                id: 1, // First message gets ID 1
                role: MessageRole::System,
                content: welcome_banner.clone(),
                thinking: None,
                timestamp: Local::now(),
                created_at: Instant::now(),
                execution: None,
            }],
            input: Input::default(),
            clipboard: Clipboard::new().ok(), // Graceful fallback if clipboard unavailable
            history: vec![],
            history_index: None,
            is_streaming: false,
            partial_response: String::new(),
            provider: ActiveProvider::new(&provider_id, &provider_name, &initial_model),

            // Chat UX Enrichment (v2)
            session_context,
            activity_items: vec![],
            command_palette: CommandPaletteState::new(),
            provider_modal: {
                let mut modal = ProviderModalState::default();
                // Set active provider based on detected provider
                if provider_id != "none" {
                    modal.set_active_provider(&provider_id);
                    modal.set_active_model(&initial_model);
                }
                modal
            },
            inline_content: vec![],
            frame: 0,

            // Chat Mode Indicators (v2.1)
            chat_mode: ChatMode::default(),
            deep_thinking: false,

            // Thinking Accumulation
            pending_thinking: None,

            // UX Hints
            shown_file_hint: false,

            // Panel Navigation & Scroll
            focused_panel: ChatPanel::Input, // Start with input focused (typing)
            conversation_scroll: PanelScrollState::new(),
            activity_scroll: PanelScrollState::new(),
            panel_rects: std::collections::HashMap::new(),
            conversation_list_state: ListState::default(),

            // WOW Effects - Matrix Streaming Decrypt
            copy_flash_index: None,
            copy_flash_start: 0,
            // Matrix decrypt: chaos emoji → text reveal during streaming
            streaming_decrypt: StreamingDecrypt::new()
                .with_reveal_speed(0.5)
                .with_wave_factor(2.0)
                .with_initial_chaos(15),
            matrix_effect_enabled: true, // Enable by default

            agent_phase: AgentPhase::Idle,
            phase_indicator: AgentPhaseIndicator::new(AgentPhase::Idle),
            agent_phase_tool: None,

            // Mission Control
            context_items: vec![],
            memory_files: Self::detect_memory_files(),
            current_verb: CurrentVerb::None,
            turn_metrics: TurnMetrics::default(),
            session_metrics: SessionMetrics::new(),
            show_yaml: false,

            // Text Selection
            text_selection: None,
            is_selecting: false,
            line_positions: Vec::new(),

            // Help Overlay
            help_overlay: HelpOverlayState::new(),
            pending_confirm: None,

            // Edit History (Undo/Redo)
            edit_history: EditHistory::default(),

            // Dynamic Input Height
            input_scroll_offset: 0,
            input_max_lines: 10, // Max visible lines (excluding borders)

            // Smart Auto-Scroll
            user_at_bottom: true, // Start at bottom

            // Thinking Toggle
            thinking_collapsed: std::collections::HashSet::new(),
            thinking_expanded_default: true, // Expanded by default
            message_id_counter: 1,           // Start at 1 (initial message has ID 1)

            mention_autocomplete: MentionAutocompleteState::new(),
            last_failed_mcp: None,

            search_mode: false,
            search_query: String::new(),
            search_results: vec![],
            search_current: 0,

            scroll_velocity: 0.0,
            scroll_accumulator: 0.0,
            scroll_animating: false,

            // Chat DAG Visualization (enabled by default, toggle with Ctrl+D)
            show_dag_panel: true,
            dag_nodes: Vec::new(),
            dag_edges: Vec::new(),
            task_queue: Vec::new(),
            dag_selected: None,
            cached_dag_panel: None,

            // TaskBox RenderMode (default to Expanded for rich inline display)
            task_box_render_mode: RenderMode::Expanded,

            // ChatWorkflow DAG (unified execution)
            workflow,

            // PERF: Message render cache
            cached_msg_items: Vec::new(),
            cached_base_len: 0,
            cached_msg_count: 0,
            cached_msg_width: 0,
            cached_streaming_len: 0,
            cached_msg_dirty: false,
        }
    }

    /// Detect available memory files (CLAUDE.md, etc.)
    fn detect_memory_files() -> Vec<MemoryFile> {
        use crate::widgets::MemoryKind;
        let mut files = vec![];

        // Check for CLAUDE.md in current directory (project root)
        if std::path::Path::new("CLAUDE.md").exists() {
            files.push(MemoryFile::project("CLAUDE.md"));
        }

        // Check for .claude/CLAUDE.md (per-project Claude Code context)
        if std::path::Path::new(".claude/CLAUDE.md").exists() {
            files.push(MemoryFile::project(".claude/CLAUDE.md"));
        }

        // Check for global ~/.claude/CLAUDE.md (user global context)
        if let Some(home) = dirs::home_dir() {
            let global_claude = home.join(".claude/CLAUDE.md");
            if global_claude.exists() {
                files.push(MemoryFile {
                    name: "~/.claude/CLAUDE.md".to_string(),
                    kind: MemoryKind::System,
                });
            }
        }

        // Check for .nika/memory.json (session memory)
        if std::path::Path::new(".nika/memory.json").exists() {
            files.push(MemoryFile::session(".nika/memory.json"));
        }

        // Check for .nika/context/ directory files
        if let Ok(entries) = std::fs::read_dir(".nika/context") {
            for entry in entries.flatten() {
                // Security: skip symlinks
                if entry.file_type().ok().is_some_and(|ft| ft.is_symlink()) {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".md") || name.ends_with(".yaml") {
                        files.push(MemoryFile::session(format!(".nika/context/{}", name)));
                    }
                }
            }
        }

        files
    }

    /// Add a memory file from @ mention resolution
    pub fn add_memory_file(&mut self, file: MemoryFile) {
        // Avoid duplicates
        if !self.memory_files.iter().any(|f| f.name == file.name) {
            self.memory_files.push(file);
        }
    }

    /// Refresh memory files (re-scan filesystem)
    pub fn refresh_memory_files(&mut self) {
        self.memory_files = Self::detect_memory_files();
    }

    // Mode methods extracted to mode_config.rs:
    // - toggle_mode, toggle_deep_thinking, set_chat_mode, set_provider
    // - get_chat_state, tick_flash, update_scroll_totals
    //
    // Mouse Support methods extracted to mouse.rs
    // Selection methods extracted to selection.rs
    // MCP tracking methods extracted to mcp_tracking.rs

    /// Sync DAG if panel is visible (avoid unnecessary work)
    pub(super) fn maybe_sync_dag(&mut self) {
        if self.show_dag_panel {
            self.sync_dag_from_messages();
        }
    }

    // === Chat UX Enrichment (v2) Methods ===

    /// Tick animation frame (call at 10Hz for smooth animations)
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Update inline content animation frames
        for content in &mut self.inline_content {
            match content {
                InlineContent::McpCall(data) => data.tick(),
                InlineContent::InferStream(data) => data.tick(),
                InlineContent::Task(task_box) => task_box.tick(),
            }
        }
        // Update pulse intensity for running TaskBoxes
        let frame = self.frame as u64;
        for content in &mut self.inline_content {
            if let InlineContent::Task(task_box) = content {
                if task_box.state().is_running() {
                    let intensity = ((frame as f32 / 30.0).sin() + 1.0) / 2.0;
                    task_box.set_pulse_intensity(intensity);
                }
            }
        }
        // WOW: Tick flash effects
        self.tick_flash();
        // Tick matrix decrypt animation (reveal progress)
        if self.is_streaming && self.matrix_effect_enabled {
            self.streaming_decrypt.tick();
        }

        // Tick phase indicator animation (icon swap + chaos decay)
        self.tick_phase_indicator();
        self.update_scroll_animation();
        // Tick provider modal animation (active indicator cycling)
        if self.provider_modal.visible {
            self.provider_modal.tick_animation();
        }
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ChatView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Calculate dynamic input height and ensure cursor is visible
        let input_height = self.calculate_input_height(area.width);
        let cursor_line = self.get_cursor_line(area.width);
        let total_lines = self.calculate_input_lines(area.width);
        self.ensure_input_cursor_visible(cursor_line, total_lines);

        // Show warning bar when no API key is configured
        let no_provider = self.provider.is_none();
        let warning_height: u16 = if no_provider { 1 } else { 0 };

        // Layout v4: ProStatusBar (2 lines) | [Warning] | Messages + Mission Control | Input (dynamic) + Hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // ProStatusBar (2 lines - Claude Code inspired)
                Constraint::Length(warning_height), // API key warning (0 or 1)
                Constraint::Min(10),   // Main content area
                Constraint::Length(input_height), // Dynamic input height (1-10 lines + borders)
                Constraint::Length(1), // Command hints
            ])
            .split(area);

        // 1. Pro Status Bar (Claude Code-inspired 2-line display)
        let chat_mode_indicator = match self.chat_mode {
            ChatMode::Infer => ChatModeIndicator::Infer,
            ChatMode::Agent => ChatModeIndicator::Agent,
        };

        ProStatusBar::new(&self.provider.model, &self.session_metrics)
            .mode(chat_mode_indicator)
            .thinking(self.deep_thinking)
            .streaming(self.is_streaming)
            .render(chunks[0], frame.buffer_mut());

        // 1b. API key warning bar (amber, only when no provider configured)
        if no_provider {
            let warning = Paragraph::new(Line::from(vec![
                Span::styled(
                    " \u{26a0} No API key configured ",
                    Style::default()
                        .fg(Color::Rgb(251, 191, 36)) // amber-400
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "\u{2014} press Ctrl+P to set up a provider",
                    Style::default().fg(Color::Rgb(245, 158, 11)), // amber-500
                ),
            ]));
            frame.render_widget(warning, chunks[1]);
        }

        // 2. Main content: Messages (65%) | Mission Control (35%)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(chunks[2]);

        // Messages panel with inline MCP/Infer boxes
        self.render_messages_v2(frame, main_chunks[0], theme);

        // Right panel - DAG Panel or Mission Control (toggle with Ctrl+D)
        if self.show_dag_panel {
            // DAG visualization panel
            self.render_dag_panel(frame, main_chunks[1], theme);
        } else {
            // Mission Control panel
            // Now includes Activity section with hot/warm/queued tasks
            MissionControlPanel::new(&self.session_context.mcp_servers)
                .context(&self.context_items)
                .memory(&self.memory_files)
                .turns(
                    self.messages
                        .iter()
                        .filter(|m| m.role == MessageRole::User)
                        .count(),
                )
                .verb(self.current_verb)
                .metrics(self.turn_metrics.clone())
                .activities(&self.activity_items) // Activity items
                .frame(self.frame) // Animation frame for spinners
                .focused(self.focused_panel == ChatPanel::Activity)
                .render(main_chunks[1], frame.buffer_mut());
        }

        // 3. Input panel
        self.render_input(frame, chunks[3], theme);

        // 4. Command hints
        self.render_hints(frame, chunks[4], theme);

        // 5. Command palette overlay (if visible)
        if self.command_palette.visible {
            let palette_area = centered_rect(60, 50, area);
            CommandPalette::new(&self.command_palette)
                .with_frame(self.frame)
                .render(palette_area, frame.buffer_mut());
        }

        // 6. Provider Modal overlay (if visible) - ⌘P
        if self.provider_modal.visible {
            // Pass &mut for cloud_tab_label caching
            // Pass theme for consistent styling
            ProviderModal::new(&mut self.provider_modal, theme).render(area, frame.buffer_mut());
        }

        // 7. Help overlay (if visible) - ? or F1
        if self.help_overlay.visible {
            let help_area = centered_rect(70, 80, area);
            HelpOverlay::new(&mut self.help_overlay).render(help_area, frame.buffer_mut());
        }

        // 8. Mention autocomplete popup (if visible)
        if self.mention_autocomplete.visible {
            // Position popup above the input area, clamped to content bounds
            let max_popup = chunks[2].height.min(10); // Don't exceed content area
            let popup_height =
                ((self.mention_autocomplete.suggestions.len().min(8) + 2) as u16).min(max_popup);
            // Clamp to stay within content area (chunks[2].y is top of main content)
            let popup_y = chunks[3].y.saturating_sub(popup_height).max(chunks[2].y);
            let popup_area = Rect::new(
                chunks[3].x + 2, // Slight offset from left
                popup_y,
                chunks[3].width.min(50).saturating_sub(4),
                popup_height,
            );
            MentionAutocomplete::new(&self.mention_autocomplete)
                .render(popup_area, frame.buffer_mut());
        }

        // 9. Confirmation dialog overlay (highest z-order — blocks all input)
        if let Some(ref action) = self.pending_confirm {
            ConfirmDialog::new(action).render(area, frame.buffer_mut());
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // ───────────────────────────────────────────────────────────────────────────
        // Overlay dispatches (highest priority)
        // ───────────────────────────────────────────────────────────────────────────

        // Handle confirmation dialog when visible (blocks all other input)
        if self.pending_confirm.is_some() {
            return self.handle_confirm_dialog_key(key);
        }

        // Handle help overlay when visible
        if self.help_overlay.visible {
            return self.handle_help_overlay_key(key);
        }

        // Handle command palette when visible
        if self.command_palette.visible {
            return self.handle_palette_key(key);
        }

        // Handle Provider Modal when visible
        if self.provider_modal.visible {
            return self.handle_provider_modal_key(key);
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Modal modes (search, mention autocomplete)
        // ───────────────────────────────────────────────────────────────────────────

        // Handle search mode when active
        if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.exit_search();
                    return ViewAction::None;
                }
                KeyCode::Enter | KeyCode::Down => {
                    self.next_search_result();
                    return ViewAction::None;
                }
                KeyCode::Up => {
                    self.prev_search_result();
                    return ViewAction::None;
                }
                KeyCode::Backspace => {
                    self.search_input_backspace();
                    return ViewAction::None;
                }
                KeyCode::Char(c) => {
                    self.search_input_char(c);
                    return ViewAction::None;
                }
                _ => {}
            }
        }

        // Handle mention autocomplete when visible
        // Dismiss on any non-navigation key to prevent input trapping
        if self.mention_autocomplete.visible {
            match key.code {
                KeyCode::Tab | KeyCode::Enter => {
                    self.accept_mention_suggestion();
                    return ViewAction::None;
                }
                KeyCode::Down => {
                    self.mention_autocomplete.next();
                    return ViewAction::None;
                }
                KeyCode::Up => {
                    self.mention_autocomplete.prev();
                    return ViewAction::None;
                }
                KeyCode::Esc => {
                    self.mention_autocomplete.hide();
                    return ViewAction::None;
                }
                // Any other key: dismiss autocomplete and let the key be processed normally
                _ => {
                    self.mention_autocomplete.hide();
                    // Fall through to main key handling
                }
            }
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Main key handling (delegated to keys.rs)
        // ───────────────────────────────────────────────────────────────────────────

        if let Some(action) = self.handle_main_keys(key) {
            return action;
        }

        ViewAction::None
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let streaming_status = if self.is_streaming {
            " | Streaming..."
        } else {
            ""
        };
        format!(
            "{} msgs | {} | {}{}",
            self.messages.len(),
            self.provider.name,
            self.provider.model,
            streaming_status
        )
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Lifecycle Hooks
    // ─────────────────────────────────────────────────────────────────────────────

    /// Called when ChatView becomes active
    ///
    /// Initializes focus state and resets any stale overlay state.
    fn on_enter(&mut self, _state: &mut TuiState) {
        // Reset focused panel to default (Input for chat)
        self.focused_panel = ChatPanel::Input;

        // Dismiss any lingering overlays from previous session
        self.mention_autocomplete.hide();
        self.command_palette.visible = false;
        self.help_overlay.visible = false;

        // Don't dismiss provider_modal - it's intentional if open

        tracing::debug!("ChatView on_enter: focus reset to Input panel");
    }

    /// Called when ChatView becomes inactive
    ///
    /// Cleans up focus state and dismisses modals to prevent input trapping.
    fn on_leave(&mut self, _state: &mut TuiState) {
        // Reset focused panel to avoid stale focus state
        self.focused_panel = ChatPanel::Conversation;

        // Dismiss overlays that could trap input
        self.mention_autocomplete.hide();
        self.command_palette.visible = false;
        self.help_overlay.visible = false;
        self.provider_modal.visible = false;

        // Exit search mode if active
        if self.search_mode {
            self.exit_search();
        }

        tracing::debug!("ChatView on_leave: overlays dismissed, focus reset");
    }
}
// DAG panel methods extracted to dag_panel.rs
// char_to_byte_offset moved to selection.rs

// centered_rect moved to widgets/utils.rs (shared utility)

#[cfg(test)]
mod tests;
