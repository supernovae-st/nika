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

// Allow dead code for types that will be used when agent integration is complete
#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;

use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{ListState, Paragraph, Widget},
    Frame,
};
use serde::{Deserialize, Serialize};
use tui_input::{Input, InputRequest};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::command::{Command, ExportFormat, ModelProvider, HELP_TEXT};

/// Check if the "command" modifier is pressed (Ctrl on Linux/Windows, Cmd on macOS)
/// On macOS, Cmd key maps to SUPER modifier in crossterm
fn is_cmd_pressed(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::SUPER)
}
use crate::runtime::chat_workflow::{ChatWorkflow, Role as WorkflowRole};
use crate::tui::edit_history::EditHistory;
use crate::tui::file_resolve::FileResolver;
use crate::tui::state::{
    ChatOverlayMessage, ChatOverlayMessageRole, ChatOverlayState, ChatPanel, PanelScrollState,
    TuiState,
};
use crate::tui::theme::{Theme, VerbColor};
use crate::util::atomic_write;

// PERF: Pre-computed constants to avoid allocations in render loop
const SEPARATOR_20: &str = "────────────────────"; // 20 Unicode box chars (─), compile-time
const SEPARATOR_20_ASCII: &str = "--------------------"; // 20 ASCII dashes (-), compile-time
const SEPARATOR_52: &str = "╰───────────────────────────────────────────────────╯"; // MCP box bottom
                                                                                    // PERF: 200-char separator for dynamic slicing (avoids .repeat() allocation)
const SEPARATOR_200: &str = "────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
// tui utils moved to messages.rs
use crate::tui::views::TuiView;
use crate::tui::widgets::{
    // Task Box widgets (v0.8.1 - visual verb containers)
    task_box::{BoxState, RenderMode, TaskBox},
    ActivityItem,
    ActivityTemp,
    AgentPhase,
    AgentPhaseIndicator,
    // v0.10.0: Chat DAG Widgets (node-as-graph visualization)
    ChatDagPanel,
    ChatModeIndicator,
    ChatNodeKind,
    ChatNodeState,
    ChatTaskQueue,
    ChatTaskQueueItem,
    ChatTaskState,
    ChatTaskVerb,
    CommandPalette,
    CommandPaletteState,
    ContextItem,
    CurrentVerb,
    DagEdgeData,
    DagNodeData,
    DecryptVerb,
    HelpOverlay,
    HelpOverlayState,
    MatrixRain,
    McpCallData,
    McpCallStatus,
    McpServerInfo,
    McpStatus,
    MemoryFile,
    // v0.9 Phase 2: @ mention autocomplete
    Mention,
    MentionAutocomplete,
    MentionAutocompleteState,
    MentionSuggestion,
    MentionType,
    MissionControlPanel,
    NikaIntroState,
    OllamaClient,
    ParsedInput,
    ProStatusBar,
    Provider,
    ProviderModal,
    ProviderModalState,
    SessionContext,
    SessionMetrics,
    StreamingDecrypt,
    SystemCommand,
    TurnMetrics,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Submodules (extracted v0.21.2 - Phase A1 refactor)
// ═══════════════════════════════════════════════════════════════════════════════

mod activity;
mod helpers;
mod hints;
mod input;
mod keys;
mod layout;
mod mentions;
mod messages;
mod render;
mod scroll;
mod search;
mod session;
mod streaming;
mod task_boxes;
mod types;

use helpers::categorize_error;
use hints::detect_verb_in_input;
use layout::{compute_panel_areas, point_in_rect};
pub use types::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Serializable Chat Session (HIGH 8 - Persistent Sessions)
// ═══════════════════════════════════════════════════════════════════════════════

/// Serializable chat session for save/load
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub version: String,
    pub created_at: String,
    pub model: String,
    pub messages: Vec<SerializableMessage>,
}

impl ChatSession {
    /// Create session from ChatView
    pub fn from_view(view: &ChatView) -> Self {
        Self {
            version: "0.5.2".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            model: view.current_model.clone(),
            messages: view
                .messages
                .iter()
                .map(SerializableMessage::from)
                .collect(),
        }
    }

    /// Save session to file using atomic write (temp+rename) for data integrity
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        atomic_write(path, json.as_bytes())
    }

    /// Load session from file
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

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
    /// Scroll offset in message list
    pub scroll: usize,
    /// Command history (for up/down navigation)
    pub history: Vec<String>,
    /// History navigation index
    pub history_index: Option<usize>,
    /// Whether streaming response is in progress
    pub is_streaming: bool,
    /// Partial response accumulated during streaming
    pub partial_response: String,
    /// Current model name for display
    pub current_model: String,
    /// PERF: Cached provider detection (updated when model changes, not every frame)
    pub cached_provider: Provider,

    // === Chat UX Enrichment (v2) ===
    /// Session context with tokens, cost, MCP status
    pub session_context: SessionContext,
    /// Activity stack items (hot/warm/queued)
    pub activity_items: Vec<ActivityItem>,
    /// Command palette state (⌘K)
    pub command_palette: CommandPaletteState,
    /// Provider modal state (⌘P - v0.8.8 full management)
    pub provider_modal: ProviderModalState,
    /// Shared Ollama client for model management (v0.11.0)
    /// Reused across pull/delete/list operations to avoid repeated allocations
    pub ollama_client: Arc<OllamaClient>,
    /// Inline content for current streaming (MCP calls, infer boxes)
    pub inline_content: Vec<InlineContent>,
    /// Animation frame counter (for spinners)
    pub frame: u8,

    // === Chat Mode Indicators (v2.1 - Claude Code-like UX) ===
    /// Current chat mode (Chat or Agent)
    pub chat_mode: ChatMode,
    /// Whether deep thinking (extended_thinking) is enabled
    pub deep_thinking: bool,
    /// Current provider name for display
    pub provider_name: String,
    /// Current provider ID for inference (v0.8.2: used by ChatAgent)
    /// This is the actual provider ID (claude, openai, mistral, etc.)
    pub current_provider_id: String,

    // === Thinking Accumulation (v0.5.2+) ===
    /// Accumulated thinking content during streaming
    /// Attached to the final message when stream completes
    pub pending_thinking: Option<String>,

    // === UX Hints (v0.7.1) ===
    /// Whether the @mention hint has been shown (show once per session)
    pub shown_file_hint: bool,

    // === Panel Navigation & Scroll (v0.8 UX Enhancement) ===
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

    // === v0.8 WOW Effects ===
    /// Index of last copied message (for flash effect)
    pub copy_flash_index: Option<usize>,
    /// Frame when copy happened (for flash duration)
    pub copy_flash_start: u8,
    /// Matrix decrypt effect for streaming text (chaos → reveal)
    pub streaming_decrypt: StreamingDecrypt,
    /// Whether matrix decrypt effect is enabled
    pub matrix_effect_enabled: bool,
    /// Matrix rain background opacity (0.0 = invisible, 1.0 = full)
    /// Used for fade-in/fade-out transitions
    pub rain_opacity: f32,
    /// Whether rain effect is actively fading out
    pub rain_fading: bool,
    /// v0.9.1: NIKA intro animation state (ASCII art explosion)
    pub intro_state: NikaIntroState,
    /// v0.9.1: NIKA butterfly explosion animation frame (0=show pattern, 1+=spreading)
    pub explosion_frame: u8,
    /// v0.9.1: Whether NIKA pattern is still visible (before full explosion)
    pub nika_pattern_visible: bool,

    // === v0.8.1 Agent Phase Tracking ===
    /// Current agent execution phase (for real-time status)
    pub agent_phase: AgentPhase,
    /// Phase indicator widget with Matrix effect
    pub phase_indicator: AgentPhaseIndicator,
    /// Tool name currently being invoked (for Invoking phase)
    pub agent_phase_tool: Option<String>,

    // === v0.7.3 Mission Control ===
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

    // === v0.7.3 YAML View Toggle ===
    /// Show messages as YAML tasks instead of chat bubbles
    pub show_yaml: bool,

    // === v0.8 Text Selection ===
    /// Text selection state (for copy support)
    pub text_selection: Option<TextSelection>,
    /// Whether a mouse drag selection is in progress
    pub is_selecting: bool,
    /// Cached line content for hit testing during selection
    /// Maps (message_index, line_in_message) -> (start_x, text_content)
    pub line_positions: Vec<LinePosition>,

    // === v0.8.1 Help Overlay ===
    /// Help overlay state (toggle with ? or F1)
    pub help_overlay: HelpOverlayState,

    // === v0.16.4 Edit History (Undo/Redo) ===
    /// Edit history for input field (Ctrl+Z/Ctrl+Y)
    pub edit_history: EditHistory,

    // === v0.16.4 Dynamic Input Height ===
    /// Input scroll offset when content exceeds max visible lines
    pub input_scroll_offset: usize,
    /// Max visible lines for input area (excluding borders)
    pub input_max_lines: usize,

    // === v0.8.1 Smart Auto-Scroll ===
    /// Whether user is "at the bottom" of conversation
    /// When true, new messages auto-scroll. When false (user scrolled up), they don't.
    /// Reset to true when user sends a message or manually scrolls to bottom.
    pub user_at_bottom: bool,

    // === v0.9 Thinking Toggle ===
    /// Set of message IDs with toggled thinking state (differs from default)
    /// Toggle individual messages with 't', toggle all with 'T'
    /// v0.9 FIX: Uses stable message IDs instead of indices for stability
    pub thinking_collapsed: std::collections::HashSet<u64>,
    /// Whether thinking sections are expanded by default
    /// false = collapsed by default (show summary), true = expanded by default
    pub thinking_expanded_default: bool,
    /// v0.9: Counter for generating unique message IDs
    message_id_counter: u64,

    // === v0.9 Phase 2: @ Mention Autocomplete ===
    /// State for the @ mention autocomplete popup
    pub mention_autocomplete: MentionAutocompleteState,

    // === v0.9 Phase 2: MCP Retry ===
    /// Last failed MCP call for retry with Ctrl+R
    pub last_failed_mcp: Option<FailedMcpCall>,

    // === v0.9 Phase 3: Conversation Search (Ctrl+F) ===
    /// Whether search mode is active
    pub search_mode: bool,
    /// Current search query
    pub search_query: String,
    /// Indices of messages matching the search
    pub search_results: Vec<usize>,
    /// Current search result index (for navigation)
    pub search_current: usize,

    // === v0.9 Phase 3: Smooth Scrolling ===
    /// Current scroll velocity (lines per tick, can be fractional)
    pub scroll_velocity: f32,
    /// Accumulated fractional scroll offset (for sub-line precision)
    pub scroll_accumulator: f32,
    /// Whether smooth scrolling animation is active
    pub scroll_animating: bool,

    // === v0.10.0: Chat DAG Visualization ===
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

    // === v0.12.1: TaskBox RenderMode ===
    /// Current render mode for inline TaskBoxes (Compact/Expanded/Full)
    /// Toggle with `m` key when not in input mode
    pub task_box_render_mode: RenderMode,

    // === v0.13: ChatWorkflow DAG (unified execution) ===
    /// Runtime workflow DAG that mirrors chat messages
    /// Used for /export yaml and unified execution with YAML workflows
    pub workflow: ChatWorkflow,
}

/// Stores info about a failed MCP call for retry (v0.9 Phase 2)
#[derive(Debug, Clone)]
pub struct FailedMcpCall {
    /// Tool name that was called
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Parameters passed to the call
    pub params: serde_json::Value,
}

/// Position of a rendered line for hit testing
#[derive(Debug, Clone)]
pub struct LinePosition {
    /// Index of the message this line belongs to
    pub message_index: usize,
    /// Which line within the message (0 = first)
    pub line_in_message: usize,
    /// Y coordinate on screen
    pub screen_y: u16,
    /// Starting X coordinate (after prefix)
    pub start_x: u16,
    /// The actual text content of this line
    pub text: String,
}

/// Text selection state
#[derive(Debug, Clone)]
pub struct TextSelection {
    /// Starting position of selection
    pub start: SelectionPos,
    /// Ending position of selection (current drag position)
    pub end: SelectionPos,
}

/// Position within the chat for selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPos {
    /// Message index in the messages vec
    pub message_index: usize,
    /// Character offset within the message content
    pub char_offset: usize,
}

impl TextSelection {
    /// Create a new selection starting at the given position
    pub fn new(start: SelectionPos) -> Self {
        Self { start, end: start }
    }

    /// Get the normalized selection (start <= end)
    pub fn normalized(&self) -> (SelectionPos, SelectionPos) {
        if self.start.message_index < self.end.message_index
            || (self.start.message_index == self.end.message_index
                && self.start.char_offset <= self.end.char_offset)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: SelectionPos) -> bool {
        let (start, end) = self.normalized();

        if pos.message_index < start.message_index || pos.message_index > end.message_index {
            return false;
        }

        if pos.message_index == start.message_index && pos.message_index == end.message_index {
            // Same message: check char range
            pos.char_offset >= start.char_offset && pos.char_offset < end.char_offset
        } else if pos.message_index == start.message_index {
            // First message: from start.char_offset to end of message
            pos.char_offset >= start.char_offset
        } else if pos.message_index == end.message_index {
            // Last message: from 0 to end.char_offset
            pos.char_offset < end.char_offset
        } else {
            // Middle messages: fully selected
            true
        }
    }
}

impl ChatView {
    pub fn new() -> Self {
        // Detect initial model and provider from environment (v0.8.2: also track provider_id)
        let (initial_model, provider_name, provider_id) =
            if std::env::var("ANTHROPIC_API_KEY").is_ok_and(|v| !v.is_empty()) {
                (
                    "claude-sonnet-4-6".to_string(),
                    "Anthropic Claude".to_string(),
                    "claude".to_string(),
                )
            } else if std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty()) {
                (
                    "gpt-4o".to_string(),
                    "OpenAI".to_string(),
                    "openai".to_string(),
                )
            } else if std::env::var("MISTRAL_API_KEY").is_ok_and(|v| !v.is_empty()) {
                (
                    "mistral-large-latest".to_string(),
                    "Mistral AI".to_string(),
                    "mistral".to_string(),
                )
            } else if std::env::var("GROQ_API_KEY").is_ok_and(|v| !v.is_empty()) {
                (
                    "llama-3.3-70b-versatile".to_string(),
                    "Groq".to_string(),
                    "groq".to_string(),
                )
            } else if std::env::var("DEEPSEEK_API_KEY").is_ok_and(|v| !v.is_empty()) {
                (
                    "deepseek-chat".to_string(),
                    "DeepSeek".to_string(),
                    "deepseek".to_string(),
                )
            } else {
                // No API key found - will show error on inference attempt
                (
                    "No API Key".to_string(),
                    "None".to_string(),
                    "none".to_string(),
                )
            };

        // Initialize session context with detected MCP servers
        let mut session_context = SessionContext::new();
        session_context
            .mcp_servers
            .push(McpServerInfo::new("novanet"));

        // v0.9.4: Clean welcome banner - verb boxes rendered separately with colors
        // v0.9.5: Wider text lines (~70 chars), dotted separator for light effect
        let welcome_banner = r#"
  ███╗   ██╗██╗██╗  ██╗ █████╗
  ████╗  ██║██║██║ ██╔╝██╔══██╗
  ██╔██╗ ██║██║█████╔╝ ███████║
  ██║╚██╗██║██║██╔═██╗ ██╔══██║
  ██║ ╚████║██║██║  ██╗██║  ██║
  ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝

  🦋 Workflow Engine  ·  💫 Semantic AI  ·  🦀 Rust

  Semantic DAG runtime where everything is a workflow. Chat, pipelines, agents —
  all execute as dependency graphs. Write in YAML, stream from any LLM, orchestrate
  MCP tools. Event-sourced. Async Rust · rig-core · tokio.

  · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·

  Type a message to chat, or /help for commands.
"#;

        // v0.13: Initialize ChatWorkflow with welcome message for DAG sync
        let mut workflow = ChatWorkflow::new();
        workflow.add_message(welcome_banner, WorkflowRole::System);

        Self {
            messages: vec![ChatMessage {
                id: 1, // First message gets ID 1
                role: MessageRole::System,
                content: welcome_banner.to_string(),
                thinking: None,
                timestamp: Local::now(),
                created_at: Instant::now(),
                execution: None,
            }],
            input: Input::default(),
            clipboard: Clipboard::new().ok(), // Graceful fallback if clipboard unavailable
            scroll: 0,
            history: vec![],
            history_index: None,
            is_streaming: false,
            partial_response: String::new(),
            current_model: initial_model.clone(),
            cached_provider: Provider::from_model_name(&initial_model),

            // Chat UX Enrichment (v2)
            session_context,
            activity_items: vec![],
            command_palette: CommandPaletteState::new(),
            provider_modal: {
                let mut modal = ProviderModalState::default();
                // v0.8.8: Set active provider based on detected provider
                if provider_id != "none" {
                    modal.set_active_provider(&provider_id);
                    modal.set_active_model(&initial_model);
                }
                modal
            },
            // v0.11.0: Shared Ollama client (reused for pull/delete/list)
            ollama_client: Arc::new(OllamaClient::new()),
            inline_content: vec![],
            frame: 0,

            // Chat Mode Indicators (v2.1)
            chat_mode: ChatMode::default(),
            deep_thinking: false,
            provider_name,
            current_provider_id: provider_id,

            // Thinking Accumulation (v0.5.2)
            pending_thinking: None,

            // UX Hints (v0.7.1)
            shown_file_hint: false,

            // Panel Navigation & Scroll (v0.8 UX Enhancement)
            focused_panel: ChatPanel::Input, // Start with input focused (typing)
            conversation_scroll: PanelScrollState::new(),
            activity_scroll: PanelScrollState::new(),
            panel_rects: std::collections::HashMap::new(),
            conversation_list_state: ListState::default(),

            // v0.8.1 WOW Effects - Matrix Streaming Decrypt
            copy_flash_index: None,
            copy_flash_start: 0,
            // Matrix decrypt: chaos emoji → text reveal during streaming
            streaming_decrypt: StreamingDecrypt::new()
                .with_reveal_speed(0.5)
                .with_wave_factor(2.0)
                .with_initial_chaos(15),
            matrix_effect_enabled: true, // Enable by default
            // v0.9.1: NIKA butterfly pattern + explosion
            rain_opacity: 1.0,                  // Start visible for NIKA pattern
            rain_fading: false,                 // Pattern handles its own fade
            intro_state: NikaIntroState::new(), // Legacy (kept for compatibility)
            explosion_frame: 0,                 // v0.9.1: Start with NIKA pattern visible
            nika_pattern_visible: true,         // v0.9.1: Show NIKA pattern at start

            // v0.8.1 Agent Phase Tracking
            agent_phase: AgentPhase::Idle,
            phase_indicator: AgentPhaseIndicator::new(AgentPhase::Idle),
            agent_phase_tool: None,

            // v0.7.3 Mission Control
            context_items: vec![],
            memory_files: Self::detect_memory_files(),
            current_verb: CurrentVerb::None,
            turn_metrics: TurnMetrics::default(),
            session_metrics: SessionMetrics::new(),
            show_yaml: false,

            // v0.8 Text Selection
            text_selection: None,
            is_selecting: false,
            line_positions: Vec::new(),

            // v0.8.1 Help Overlay
            help_overlay: HelpOverlayState::new(),

            // v0.16.4 Edit History (Undo/Redo)
            edit_history: EditHistory::default(),

            // v0.16.4 Dynamic Input Height
            input_scroll_offset: 0,
            input_max_lines: 10, // Max visible lines (excluding borders)

            // v0.8.1 Smart Auto-Scroll
            user_at_bottom: true, // Start at bottom

            // v0.9 Thinking Toggle
            thinking_collapsed: std::collections::HashSet::new(),
            thinking_expanded_default: true, // Expanded by default
            message_id_counter: 1,           // Start at 1 (initial message has ID 1)

            // v0.9 Phase 2: @ Mention Autocomplete
            mention_autocomplete: MentionAutocompleteState::new(),
            last_failed_mcp: None,

            // v0.9 Phase 3: Conversation Search (Ctrl+F)
            search_mode: false,
            search_query: String::new(),
            search_results: vec![],
            search_current: 0,

            // v0.9 Phase 3: Smooth Scrolling
            scroll_velocity: 0.0,
            scroll_accumulator: 0.0,
            scroll_animating: false,

            // v0.10.0: Chat DAG Visualization (enabled by default, toggle with Ctrl+D)
            show_dag_panel: true,
            dag_nodes: Vec::new(),
            dag_edges: Vec::new(),
            task_queue: Vec::new(),
            dag_selected: None,

            // v0.12.1: TaskBox RenderMode (default to Expanded for rich inline display)
            task_box_render_mode: RenderMode::Expanded,

            // v0.13: ChatWorkflow DAG (unified execution)
            workflow,
        }
    }

    /// Detect available memory files (CLAUDE.md, etc.)
    fn detect_memory_files() -> Vec<MemoryFile> {
        use crate::tui::widgets::MemoryKind;
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

    /// Toggle between Infer and Agent modes
    pub fn toggle_mode(&mut self) {
        self.chat_mode = match self.chat_mode {
            ChatMode::Infer => ChatMode::Agent,
            ChatMode::Agent => ChatMode::Infer,
        };
    }

    /// Toggle deep thinking (extended_thinking)
    pub fn toggle_deep_thinking(&mut self) {
        self.deep_thinking = !self.deep_thinking;
    }

    /// Set chat mode directly
    pub fn set_chat_mode(&mut self, mode: ChatMode) {
        self.chat_mode = mode;
    }

    /// Set provider name for display
    pub fn set_provider(&mut self, name: impl Into<String>) {
        self.provider_name = name.into();
    }


    /// Convert ChatView state to ChatOverlayState for session persistence (v0.12.0)
    ///
    /// Maps ChatMessage (with full metadata) to ChatOverlayMessage (role + content only)
    /// for saving to .nika/sessions/
    pub fn get_chat_state(&self) -> ChatOverlayState {
        // Convert messages to overlay format (role + content only)
        let messages = self
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => ChatOverlayMessageRole::User,
                    MessageRole::Nika => ChatOverlayMessageRole::Nika,
                    MessageRole::System => ChatOverlayMessageRole::System,
                    MessageRole::Tool => ChatOverlayMessageRole::Tool,
                };
                ChatOverlayMessage::new(role, &msg.content)
            })
            .collect();

        ChatOverlayState {
            messages,
            input: self.input.value().to_string(),
            cursor: self.input.cursor(),
            scroll: self.scroll,
            history: self.history.clone(),
            history_index: self.history_index,
            is_streaming: self.is_streaming,
            partial_response: self.partial_response.clone(),
            current_model: self.current_model.clone(),
            edit_history: Default::default(), // Session doesn't persist edit history
        }
    }

    /// Copy the currently selected message to clipboard
    /// Returns true if copy succeeded
    pub fn copy_selected_message(&mut self, text_only: bool) -> bool {
        // Only works when conversation panel is focused
        if self.focused_panel != ChatPanel::Conversation {
            return false;
        }

        let cursor = self.conversation_scroll.cursor;
        if cursor >= self.messages.len() {
            return false;
        }

        let msg = &self.messages[cursor];
        let text = if text_only {
            // Just the content
            msg.content.clone()
        } else {
            // Full message with role and timestamp
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Nika => "Nika",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            format!("[{}] {}", role, msg.content)
        };

        // Copy to clipboard
        if let Some(ref mut clipboard) = self.clipboard {
            let success = clipboard.set_text(text).is_ok();
            if success {
                // v0.8 WOW: Trigger flash effect on copied message
                self.copy_flash_index = Some(cursor);
                self.copy_flash_start = self.frame;
            }
            success
        } else {
            false
        }
    }

    /// Clear flash effect after duration (called each frame in tick)
    pub fn tick_flash(&mut self) {
        // Flash lasts about 16 frames (~250ms at 60fps)
        if self.copy_flash_index.is_some() {
            let elapsed = self.frame.wrapping_sub(self.copy_flash_start);
            if elapsed > 16 {
                self.copy_flash_index = None;
            }
        }
    }

    /// Update scroll state totals from current data
    pub fn update_scroll_totals(&mut self) {
        self.conversation_scroll.set_total(self.messages.len());
        self.activity_scroll.set_total(self.activity_items.len());
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // Mouse Support (v0.8 UX Enhancement)
    // Layout helpers in layout.rs: compute_panel_areas(), point_in_rect()
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Determine which panel is at the given screen position
    pub fn panel_at_position(&self, x: u16, y: u16, area: Rect) -> Option<ChatPanel> {
        let (_, conversation, activity, input, _) = compute_panel_areas(area);

        // Check each panel (order matters for overlapping edges)
        if point_in_rect(x, y, conversation) {
            Some(ChatPanel::Conversation)
        } else if point_in_rect(x, y, activity) {
            Some(ChatPanel::Activity)
        } else if point_in_rect(x, y, input) {
            Some(ChatPanel::Input)
        } else {
            None
        }
    }

    /// Handle mouse event for Chat view
    /// Returns true if the event was handled
    pub fn handle_mouse(
        &mut self,
        kind: crossterm::event::MouseEventKind,
        x: u16,
        y: u16,
        area: Rect,
    ) -> bool {
        use crossterm::event::{MouseButton, MouseEventKind};

        match kind {
            // Left click - start text selection (no panel focus change)
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if click is in conversation area (for text selection)
                if let Some(pos) = self.screen_to_selection_pos(x, y) {
                    // Start a new selection
                    self.text_selection = Some(TextSelection::new(pos));
                    self.is_selecting = true;
                    true
                } else {
                    // Clear any existing selection when clicking elsewhere
                    // v0.8: No panel focus change on click - use Tab only
                    self.text_selection = None;
                    self.is_selecting = false;
                    self.panel_at_position(x, y, area).is_some() // Return true if within panels
                }
            }
            // Mouse drag - extend selection
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_selecting {
                    if let Some(pos) = self.screen_to_selection_pos(x, y) {
                        if let Some(ref mut selection) = self.text_selection {
                            selection.end = pos;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            // Mouse release - finalize selection
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_selecting {
                    self.is_selecting = false;
                    // Keep selection visible (it will be cleared on next click)
                    // If selection is empty (same start/end), clear it
                    if let Some(ref selection) = self.text_selection {
                        if selection.start == selection.end {
                            self.text_selection = None;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            // Scroll wheel up - smooth scroll focused panel (no panel switch)
            // v0.9 Phase 3: Use smooth scrolling with momentum for mouse wheel
            MouseEventKind::ScrollUp => {
                self.smooth_scroll(-1); // Negative = scroll up (content moves down)
                true
            }
            // Scroll wheel down - smooth scroll focused panel (no panel switch)
            MouseEventKind::ScrollDown => {
                self.smooth_scroll(1); // Positive = scroll down (content moves up)
                true
            }
            _ => false,
        }
    }

    /// Convert screen coordinates to selection position
    /// Returns None if not within a message text area
    fn screen_to_selection_pos(&self, x: u16, y: u16) -> Option<SelectionPos> {
        // Find which line is at this Y coordinate
        for line_pos in &self.line_positions {
            if line_pos.screen_y == y && x >= line_pos.start_x {
                // Calculate character offset within the line
                let x_offset = (x - line_pos.start_x) as usize;
                let char_offset = x_offset.min(line_pos.text.len());

                // Calculate total char offset in the message
                // This is simplified - we sum up characters from all previous lines in this message
                let mut total_offset = char_offset;
                for prev in &self.line_positions {
                    if prev.message_index == line_pos.message_index
                        && prev.line_in_message < line_pos.line_in_message
                    {
                        total_offset += prev.text.len();
                    }
                }

                return Some(SelectionPos {
                    message_index: line_pos.message_index,
                    char_offset: total_offset,
                });
            }
        }
        None
    }

    /// Get the selected text (if any)
    pub fn get_selected_text(&self) -> Option<String> {
        let selection = self.text_selection.as_ref()?;
        let (start, end) = selection.normalized();

        let mut result = String::new();

        for (idx, msg) in self.messages.iter().enumerate() {
            if idx < start.message_index || idx > end.message_index {
                continue;
            }

            let content = &msg.content;

            if idx == start.message_index && idx == end.message_index {
                // Single message selection
                let start_byte = char_to_byte_offset(content, start.char_offset);
                let end_byte = char_to_byte_offset(content, end.char_offset);
                if start_byte < content.len() && end_byte <= content.len() {
                    result.push_str(&content[start_byte..end_byte]);
                }
            } else if idx == start.message_index {
                // First message of multi-message selection
                let start_byte = char_to_byte_offset(content, start.char_offset);
                if start_byte < content.len() {
                    result.push_str(&content[start_byte..]);
                    result.push('\n');
                }
            } else if idx == end.message_index {
                // Last message of multi-message selection
                let end_byte = char_to_byte_offset(content, end.char_offset);
                if end_byte <= content.len() {
                    result.push_str(&content[..end_byte]);
                }
            } else {
                // Middle messages - fully selected
                result.push_str(content);
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Copy selection to clipboard
    /// Returns true if copy succeeded
    pub fn copy_selection(&mut self) -> bool {
        if let Some(text) = self.get_selected_text() {
            if let Some(ref mut clipboard) = self.clipboard {
                if clipboard.set_text(&text).is_ok() {
                    // Flash effect for feedback
                    if let Some(ref selection) = self.text_selection {
                        let (start, _) = selection.normalized();
                        self.copy_flash_index = Some(start.message_index);
                        self.copy_flash_start = self.frame;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Clear the current text selection
    pub fn clear_selection(&mut self) {
        self.text_selection = None;
        self.is_selecting = false;
    }


    /// Add a tool message
    pub fn add_tool_message(&mut self, content: String) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Tool,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        // v0.12.1: Sync DAG when messages change
        self.maybe_sync_dag();

        // v0.13: Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Tool);
    }

    /// v0.12.1: Sync DAG if panel is visible (avoid unnecessary work)
    fn maybe_sync_dag(&mut self) {
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
        // v0.8 WOW: Tick flash effects
        self.tick_flash();
        // v0.8 WOW: Tick matrix decrypt animation (reveal progress)
        if self.is_streaming && self.matrix_effect_enabled {
            self.streaming_decrypt.tick();
        }
        // v0.9.1: Tick intro animation (NIKA ASCII art explosion)
        // Note: area-dependent tick happens in render() where we have the rect
        // v0.9.1: Tick matrix rain fade effect
        if self.rain_fading && self.rain_opacity > 0.0 {
            // Fast fade (0.04 per tick = ~2 seconds to fully fade at 10Hz)
            self.rain_opacity = (self.rain_opacity - 0.04).max(0.0);
        }

        // v0.9.2: Tick NIKA butterfly explosion animation (FAST: 15 frames = ~1.5s)
        if self.nika_pattern_visible {
            self.explosion_frame = self.explosion_frame.saturating_add(1);
            // After explosion completes, quick transition to clean UI
            if self.explosion_frame >= 15 {
                self.nika_pattern_visible = false;
                self.rain_opacity = 0.2; // Very subtle rain, then fade
                self.rain_fading = true;
            }
        }
        // v0.8.1: Tick phase indicator animation (icon swap + chaos decay)
        self.tick_phase_indicator();
        // v0.9 Phase 3: Update smooth scroll animation
        self.update_scroll_animation();
        // v0.8.8: Tick provider modal animation (active indicator cycling)
        if self.provider_modal.visible {
            self.provider_modal.tick_animation();
        }
    }

    /// Add an MCP call to the inline content
    pub fn add_mcp_call(&mut self, tool: &str, server: &str, params: &str) {
        let data = McpCallData::new(tool, server).with_params(params);
        self.inline_content.push(InlineContent::McpCall(data));

        // Add to activity stack as hot
        self.activity_items.push(ActivityItem::hot(
            format!("mcp-{}", self.inline_content.len()),
            "invoke",
        ));

        // Update MCP server status to hot
        if let Some(server_info) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server)
        {
            server_info.status = McpStatus::Hot;
            server_info.last_call = Some(Instant::now());
        }

        // v0.8.2: Auto-scroll when MCP call appears
        self.auto_scroll_to_bottom();
    }

    /// Complete an MCP call with result
    pub fn complete_mcp_call(&mut self, result: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            data.result = Some(result.to_string());
            data.status = McpCallStatus::Success;
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("invoke");
        // v0.8.2: Auto-scroll when MCP result arrives
        self.auto_scroll_to_bottom();
    }

    /// Fail an MCP call with error
    ///
    /// v0.9 Phase 2: Also saves the failed call for retry with Ctrl+R
    pub fn fail_mcp_call(&mut self, error: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            // v0.9 Phase 2: Save failed MCP call for retry with Ctrl+R
            self.last_failed_mcp = Some(FailedMcpCall {
                tool: data.tool.clone(),
                server: data.server.clone(),
                params: serde_json::from_str(&data.params).unwrap_or(serde_json::Value::Null),
            });

            data.error = Some(error.to_string());
            data.status = McpCallStatus::Failed;
        }
        // v0.8.2: Auto-scroll when MCP error appears
        self.auto_scroll_to_bottom();
    }



    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        // v0.9.1: Trigger rain on first real user message (after welcome)
        let is_first_user_message = !self.messages.iter().any(|m| m.role == MessageRole::User);
        if is_first_user_message {
            self.trigger_rain_effect();
        }

        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::User,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.history.push(content.clone());
        self.history_index = None;
        // v0.8.1: When user sends a message, they want to see the response
        self.user_at_bottom = true;
        self.auto_scroll_to_bottom();
        // v0.12.1: Sync DAG when messages change
        self.maybe_sync_dag();

        // v0.13: Wire to ChatWorkflow DAG (unified execution)
        // Use add_message_with_mentions to handle @N references automatically
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::User);
    }

    /// Add a Nika response
    pub fn add_nika_message(&mut self, content: String, execution: Option<ExecutionResult>) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Nika,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution,
            thinking: None,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
                                      // v0.12.1: Sync DAG when messages change
        self.maybe_sync_dag();

        // v0.13: Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Assistant);
    }

    /// Add a Nika response with thinking content (v0.5.2+)
    pub fn add_nika_message_with_thinking(
        &mut self,
        content: String,
        thinking: Option<String>,
        execution: Option<ExecutionResult>,
    ) {
        let id = self.next_message_id();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::Nika,
            content: content.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution,
            thinking,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
                                      // v0.12.1: Sync DAG when messages change
        self.maybe_sync_dag();

        // v0.13: Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content, WorkflowRole::Assistant);
    }

    /// Add a system message (for mode changes, status updates)
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        let id = self.next_message_id();
        let content_str = content.into();
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::System,
            content: content_str.clone(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
                                      // v0.12.1: Sync DAG when messages change
        self.maybe_sync_dag();

        // v0.13: Wire to ChatWorkflow DAG (unified execution)
        let _ = self
            .workflow
            .add_message_with_mentions(&content_str, WorkflowRole::System);
    }

    /// v0.9: Export chat session to JSON file
    ///
    /// Exports all messages to a JSON file for later analysis or sharing.
    /// If no path is provided, generates a timestamped filename.
    ///
    /// # Security
    /// Path is validated to prevent directory traversal attacks:
    /// - Rejects paths with `..` components
    /// - Rejects absolute paths (must be relative to current directory)
    /// - Ensures `.json` extension
    ///
    /// # Returns
    /// The path to the exported file on success
    pub fn export_session(&self, path: Option<&str>) -> Result<String, String> {
        // Generate default filename with timestamp
        let filepath = match path {
            Some(p) => {
                // v0.9 SECURITY: Validate user-provided path
                let path = Path::new(p);

                // Reject absolute paths (must be relative to current directory)
                if path.is_absolute() {
                    return Err("Security: Absolute paths not allowed. Use a relative path.".into());
                }

                // Reject paths with parent directory traversal
                for component in path.components() {
                    if matches!(component, std::path::Component::ParentDir) {
                        return Err(
                            "Security: Path traversal (..) not allowed. Use a simple filename."
                                .into(),
                        );
                    }
                }

                // Ensure .json extension (user-friendly, prevents accidental overwrites)
                let p_str = p.to_string();
                if p_str.ends_with(".json") {
                    p_str
                } else {
                    format!("{}.json", p_str)
                }
            }
            None => format!("nika-chat-{}.json", Local::now().format("%Y%m%d-%H%M%S")),
        };

        // Build exportable session data
        #[derive(Serialize)]
        struct ExportedSession {
            exported_at: String,
            model: String,
            message_count: usize,
            messages: Vec<ExportedMessage>,
        }

        #[derive(Serialize)]
        struct ExportedMessage {
            role: String,
            content: String,
            timestamp: String,
            thinking: Option<String>,
        }

        let messages: Vec<ExportedMessage> = self
            .messages
            .iter()
            .map(|m| ExportedMessage {
                role: format!("{:?}", m.role),
                content: m.content.clone(),
                timestamp: m.timestamp.to_rfc3339(),
                thinking: m.thinking.clone(),
            })
            .collect();

        let session = ExportedSession {
            exported_at: Local::now().to_rfc3339(),
            model: self.current_model.clone(),
            message_count: messages.len(),
            messages,
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&session)
            .map_err(|e| format!("JSON serialization failed: {}", e))?;

        // Write to file using atomic_write for safety
        atomic_write(Path::new(&filepath), json.as_bytes())
            .map_err(|e| format!("File write failed: {}", e))?;

        Ok(filepath)
    }

    /// v0.13: Export chat session as a runnable Nika YAML workflow
    ///
    /// Exports the chat conversation as a YAML workflow file that can be re-executed
    /// with `nika run <file>`. User messages become `infer:` tasks, and @N mentions
    /// create DAG edges between tasks.
    pub fn export_session_yaml(&self, path: Option<&str>) -> Result<String, String> {
        // Generate default filename with timestamp
        let filepath = match path {
            Some(p) => {
                // v0.9 SECURITY: Validate user-provided path (same as JSON export)
                let path = Path::new(p);

                // Reject absolute paths (must be relative to current directory)
                if path.is_absolute() {
                    return Err("Security: Absolute paths not allowed. Use a relative path.".into());
                }

                // Reject paths with parent directory traversal
                for component in path.components() {
                    if matches!(component, std::path::Component::ParentDir) {
                        return Err(
                            "Security: Path traversal (..) not allowed. Use a simple filename."
                                .into(),
                        );
                    }
                }

                // Ensure .nika.yaml extension
                let p_str = p.to_string();
                if p_str.ends_with(".nika.yaml") {
                    p_str
                } else if p_str.ends_with(".yaml") || p_str.ends_with(".yml") {
                    // Replace .yaml/.yml with .nika.yaml
                    let base = p_str.trim_end_matches(".yaml").trim_end_matches(".yml");
                    format!("{}.nika.yaml", base)
                } else {
                    format!("{}.nika.yaml", p_str)
                }
            }
            None => format!(
                "nika-chat-{}.nika.yaml",
                Local::now().format("%Y%m%d-%H%M%S")
            ),
        };

        // Generate YAML from ChatWorkflow
        let yaml = self.workflow.to_yaml();

        // Write to file using atomic_write for safety
        atomic_write(Path::new(&filepath), yaml.as_bytes())
            .map_err(|e| format!("File write failed: {}", e))?;

        Ok(filepath)
    }

    /// v0.9: Generate a new unique message ID
    fn next_message_id(&mut self) -> u64 {
        self.message_id_counter += 1;
        self.message_id_counter
    }

    /// v0.9: Toggle thinking section visibility for a specific message
    ///
    /// Toggles the collapsed state of thinking content for the message at index.
    /// Used with 't' key to show/hide thinking for the cursor message.
    /// v0.9 FIX: Uses stable message IDs instead of indices for stability.
    pub fn toggle_thinking(&mut self, idx: usize) {
        // Only toggle if this message has thinking content
        if let Some(msg) = self.messages.get(idx) {
            if msg.thinking.is_some() {
                let msg_id = msg.id;
                if self.thinking_collapsed.contains(&msg_id) {
                    self.thinking_collapsed.remove(&msg_id);
                } else {
                    self.thinking_collapsed.insert(msg_id);
                }
            }
        }
    }

    /// v0.9: Toggle thinking visibility for all messages
    ///
    /// Toggles the default expanded state and clears individual overrides.
    /// Used with 'T' key to show/hide all thinking sections at once.
    pub fn toggle_all_thinking(&mut self) {
        self.thinking_expanded_default = !self.thinking_expanded_default;
        self.thinking_collapsed.clear();
        // Add system message to confirm the toggle
        let state = if self.thinking_expanded_default {
            "expanded"
        } else {
            "collapsed"
        };
        self.add_system_message(format!("🧠 Thinking sections now {} by default", state));
    }

    /// v0.9: Check if thinking is visible for a specific message
    ///
    /// Returns true if thinking section should be shown for message at index.
    /// The `thinking_collapsed` set tracks messages that differ from the default.
    /// If default=false (collapsed), set contains messages to SHOW.
    /// If default=true (expanded), set contains messages to HIDE.
    /// v0.9 FIX: Uses stable message IDs instead of indices for stability.
    pub fn is_thinking_visible(&self, idx: usize) -> bool {
        // Get the message ID from the index
        if let Some(msg) = self.messages.get(idx) {
            // If in the override set, return the OPPOSITE of default
            if self.thinking_collapsed.contains(&msg.id) {
                return !self.thinking_expanded_default;
            }
        }
        // Otherwise use default
        self.thinking_expanded_default
    }


    /// v0.8.1 FIX: Auto-scroll to bottom of conversation (NovaNet pattern)
    /// Called when new messages are added to keep latest content visible
    /// v0.8.1: Smart auto-scroll - only scrolls if user was at bottom
    /// This prevents jumping when user is reading history and new content arrives
    fn auto_scroll_to_bottom(&mut self) {
        // v0.8.1: Only auto-scroll if user was at the bottom
        // If they scrolled up to read history, don't interrupt them
        if !self.user_at_bottom {
            return;
        }

        // Update total immediately (don't wait for render)
        // Use estimated lines per message until render computes exact count
        let estimated_lines_per_message = 4; // header + content + spacing
        let estimated_total = self.messages.len() * estimated_lines_per_message;

        // Update scroll state total (will be refined during render)
        self.conversation_scroll.total = estimated_total;

        // Scroll to bottom using the estimated total
        let visible = self.conversation_scroll.visible.max(1);
        self.conversation_scroll.offset = estimated_total.saturating_sub(visible);
        self.conversation_scroll.cursor = estimated_total.saturating_sub(1);
    }

    /// Append text to the last message (for streaming tokens)
    ///
    /// Used for Claude Code-like streaming where tokens appear in real-time.
    /// If the last message is "Thinking...", it will be replaced.
    /// v0.8.1: Auto-scrolls to keep streaming content visible
    pub fn append_to_last_message(&mut self, token: &str) {
        if let Some(last) = self.messages.last_mut() {
            // If it's "Thinking...", replace it with the first token
            if last.content == "Thinking..." {
                last.content = token.to_string();
            } else {
                // Append token to existing content
                last.content.push_str(token);
            }
        }
        // v0.8.1: Auto-scroll during streaming to follow new content
        self.auto_scroll_to_bottom();
    }

    /// Replace the last message content (for error display)
    pub fn replace_last_message(&mut self, content: String) {
        if let Some(last) = self.messages.last_mut() {
            last.content = content;
        }
    }

    /// Display an error with recovery suggestions (v0.5.2+)
    /// Categorizes errors and provides actionable hints
    pub fn show_error(&mut self, error: &str) {
        let (category, suggestion) = categorize_error(error);
        let formatted = format!(
            "❌ {} Error: {}\n💡 {}\n\nUse /help for commands or /clear to restart.",
            category, error, suggestion
        );
        self.add_system_message(formatted);
    }

    /// Submit current input
    /// Returns Some(message) if it should be sent to the agent,
    /// or None if it was a system command handled internally
    pub fn submit(&mut self) -> Option<String> {
        if self.input.value().trim().is_empty() {
            return None;
        }
        let message = self.input.value().to_string();

        // Check for system commands (handled internally)
        match ParsedInput::parse(&message) {
            ParsedInput::System(cmd) => {
                self.handle_system_command(cmd);
                self.input.reset();
                self.update_mode_from_input(); // v0.8.1: Preserve mode after clear
                return None;
            }
            ParsedInput::PartialPrefix(_) => {
                // User typing a command prefix, don't submit
                return None;
            }
            ParsedInput::Verb { .. } => {
                // Verb command - check if explicit (starts with /)
                // If explicit, TaskBox will display the command visually,
                // so we don't need a user message bubble (avoids duplication)
            }
            _ => {
                // Empty or other - don't process
                return None;
            }
        }

        // Only add user message bubble for implicit messages (no / prefix)
        // Explicit verb commands like /exec, /infer etc. are shown in TaskBox
        let is_explicit_verb_command = message.starts_with('/');
        if !is_explicit_verb_command {
            self.add_user_message(message.clone());
        }
        self.input.reset();
        self.update_mode_from_input(); // v0.8.1: Preserve mode after submit
        Some(message)
    }

    /// Handle system commands (internal, not sent to agent)
    fn handle_system_command(&mut self, cmd: SystemCommand) {
        match cmd {
            SystemCommand::Clear => {
                self.messages.clear();
                self.add_system_message("Conversation cleared.".to_string());
            }
            SystemCommand::Help => {
                self.add_system_message(
                    "Commands:\n\
                     /clear - Clear conversation\n\
                     /help - Show this help\n\
                     /yaml - Toggle YAML view\n\
                     /thinking - Toggle deep thinking mode\n\
                     /model <name> - Change model\n\
                     /provider <name> - Change provider\n\n\
                     Verbs:\n\
                     /infer <prompt> - LLM generation (default)\n\
                     /exec <cmd> - Shell command\n\
                     /fetch <url> - HTTP request\n\
                     /invoke <tool> - MCP tool call\n\
                     /agent <prompt> - Agentic loop"
                        .to_string(),
                );
            }
            SystemCommand::Yaml => {
                self.show_yaml = !self.show_yaml;
                let status = if self.show_yaml { "ON" } else { "OFF" };
                self.add_system_message(format!("YAML view: {}", status));
            }
            SystemCommand::Thinking => {
                self.deep_thinking = !self.deep_thinking;
                let status = if self.deep_thinking { "ON" } else { "OFF" };
                self.add_system_message(format!("Deep thinking: {}", status));
            }
            SystemCommand::Model => {
                // Model change would need argument parsing
                self.add_system_message("Use ⌘P to select a model.".to_string());
            }
            SystemCommand::Provider => {
                // Provider change would need argument parsing
                self.add_system_message("Use ⌘P to select a provider.".to_string());
            }
        }
    }

    /// Navigate history up
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Safe: already checked history is not empty above
                self.history_index = Some(self.history.len().saturating_sub(1));
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i.saturating_sub(1));
            }
            _ => {}
        }
        if let Some(i) = self.history_index {
            // Safe bounds check with .get()
            if let Some(entry) = self.history.get(i) {
                self.input = Input::new(entry.clone());
                self.input.handle(InputRequest::GoToEnd);
                self.update_mode_from_input(); // v0.8.1: Sync mode indicator
            }
        }
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        // Early return if history is empty (prevents underflow on len() - 1)
        if self.history.is_empty() {
            self.history_index = None;
            return;
        }
        let last_idx = self.history.len().saturating_sub(1);
        match self.history_index {
            Some(i) if i < last_idx => {
                let next_idx = i.saturating_add(1);
                self.history_index = Some(next_idx);
                // Safe bounds check with .get()
                if let Some(entry) = self.history.get(next_idx) {
                    self.input = Input::new(entry.clone());
                    self.input.handle(InputRequest::GoToEnd);
                    self.update_mode_from_input(); // v0.8.1: Sync mode indicator
                }
            }
            Some(_) => {
                self.history_index = None;
                self.input.reset();
                self.update_mode_from_input(); // v0.8.1: Sync mode indicator
            }
            None => {}
        }
    }

    /// Insert character at cursor (delegates to tui-input)
    pub fn insert_char(&mut self, c: char) {
        self.input.handle(InputRequest::InsertChar(c));
        // v0.16.4: Track mutation in edit history (coalesces rapid keystrokes)
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input(); // v0.8.1: Sync mode indicator
        self.check_mention_trigger(); // v0.9 Phase 2: @ mention autocomplete
    }

    /// Delete character before cursor (delegates to tui-input)
    pub fn backspace(&mut self) {
        self.input.handle(InputRequest::DeletePrevChar);
        // v0.16.4: Track mutation in edit history
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input(); // v0.8.1: Sync mode indicator
        self.check_mention_trigger(); // v0.9 Phase 2: @ mention autocomplete
    }

    /// v0.8.1: Update chat_mode and current_verb based on input prefix
    /// This syncs the mode indicator with what the user is typing
    fn update_mode_from_input(&mut self) {
        let input = self.input.value();

        // Check for verb prefix in input
        if let Some((_, verb_color, is_complete, _)) = detect_verb_in_input(input) {
            // Only update if we have a complete verb (with space after)
            if is_complete {
                // Update chat_mode for Infer/Agent toggle
                match verb_color {
                    VerbColor::Agent => self.chat_mode = ChatMode::Agent,
                    VerbColor::Infer => self.chat_mode = ChatMode::Infer,
                    // Other verbs keep current chat_mode but update current_verb
                    _ => {}
                }

                // Update current_verb for MissionControlPanel
                self.current_verb = match verb_color {
                    VerbColor::Infer => CurrentVerb::Infer,
                    VerbColor::Exec => CurrentVerb::Exec,
                    VerbColor::Fetch => CurrentVerb::Fetch,
                    VerbColor::Invoke => CurrentVerb::Invoke,
                    VerbColor::Agent => CurrentVerb::Agent,
                    VerbColor::Spawn => CurrentVerb::Spawn,
                    VerbColor::User => CurrentVerb::None, // User is not a verb command
                };
            }
        } else if input.is_empty() || !input.starts_with('/') {
            // No verb prefix → reset to defaults
            self.current_verb = CurrentVerb::None;
            // Keep chat_mode as-is when no prefix (user might want to stay in Agent mode)
        }
    }

    /// Move cursor left (delegates to tui-input)
    pub fn cursor_left(&mut self) {
        self.input.handle(InputRequest::GoToPrevChar);
    }

    /// Move cursor right (delegates to tui-input)
    pub fn cursor_right(&mut self) {
        self.input.handle(InputRequest::GoToNextChar);
    }

    /// Move cursor to previous word (Ctrl+Left)
    pub fn cursor_prev_word(&mut self) {
        self.input.handle(InputRequest::GoToPrevWord);
    }

    /// Move cursor to next word (Ctrl+Right)
    pub fn cursor_next_word(&mut self) {
        self.input.handle(InputRequest::GoToNextWord);
    }

    /// Delete previous word (Ctrl+Backspace)
    pub fn delete_prev_word(&mut self) {
        // v0.16.4: Force checkpoint before word deletion (significant edit)
        self.edit_history
            .checkpoint(self.input.value(), self.input.cursor());
        self.input.handle(InputRequest::DeletePrevWord);
        self.edit_history
            .push(self.input.value(), self.input.cursor());
        self.update_mode_from_input(); // v0.8.1: Sync mode indicator
    }

    /// Go to start of input (Home)
    pub fn cursor_start(&mut self) {
        self.input.handle(InputRequest::GoToStart);
    }

    /// Go to end of input (End)
    pub fn cursor_end(&mut self) {
        self.input.handle(InputRequest::GoToEnd);
    }

    /// Copy input to clipboard (Ctrl+C)
    pub fn copy_to_clipboard(&mut self) {
        if let Some(clipboard) = &mut self.clipboard {
            let _ = clipboard.set_text(self.input.value().to_string());
        }
    }

    /// Paste from clipboard (Ctrl+V)
    pub fn paste_from_clipboard(&mut self) {
        if let Some(clipboard) = &mut self.clipboard {
            if let Ok(text) = clipboard.get_text() {
                // v0.16.4: Checkpoint before paste (significant edit)
                self.edit_history
                    .checkpoint(self.input.value(), self.input.cursor());
                for c in text.chars() {
                    self.input.handle(InputRequest::InsertChar(c));
                }
                self.edit_history
                    .push(self.input.value(), self.input.cursor());
                self.update_mode_from_input(); // v0.8.1: Sync mode indicator
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.16.4: Dynamic Input Height Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Calculate how many lines the input content requires with word wrapping
    fn calculate_input_lines(&self, available_width: u16) -> usize {
        let input_value = self.input.value();
        if input_value.is_empty() {
            return 1;
        }

        // Account for prefix (e.g., "🦋 nika > ") which takes ~12 chars
        let prefix_width = 14_u16;
        let content_width = available_width.saturating_sub(prefix_width + 2); // +2 for borders
        if content_width == 0 {
            return 1;
        }

        let mut total_lines = 0;
        for line in input_value.split('\n') {
            if line.is_empty() {
                total_lines += 1;
            } else {
                // Calculate wrapped lines for this physical line
                let char_count = line.chars().count();
                let lines_needed = (char_count as u16).div_ceil(content_width);
                total_lines += lines_needed.max(1) as usize;
            }
        }
        total_lines.max(1)
    }

    /// Ensure the cursor line is visible by adjusting scroll offset
    fn ensure_input_cursor_visible(&mut self, cursor_line: usize, total_lines: usize) {
        if total_lines <= self.input_max_lines {
            self.input_scroll_offset = 0;
            return;
        }

        // Keep a 1-line margin at top/bottom when scrolling
        let margin = 1;

        // If cursor is below visible area, scroll down
        if cursor_line >= self.input_scroll_offset + self.input_max_lines - margin {
            self.input_scroll_offset =
                cursor_line.saturating_sub(self.input_max_lines - 1 - margin);
        }

        // If cursor is above visible area, scroll up
        if cursor_line < self.input_scroll_offset + margin {
            self.input_scroll_offset = cursor_line.saturating_sub(margin);
        }

        // Clamp scroll offset to valid range
        let max_offset = total_lines.saturating_sub(self.input_max_lines);
        self.input_scroll_offset = self.input_scroll_offset.min(max_offset);
    }

    /// Calculate the dynamic input height for layout
    fn calculate_input_height(&self, available_width: u16) -> u16 {
        let content_lines = self.calculate_input_lines(available_width);
        let clamped_lines = content_lines.clamp(1, self.input_max_lines);
        (clamped_lines as u16) + 2 // Add 2 for borders
    }

    /// Get the line number where the cursor is (for scroll tracking)
    fn get_cursor_line(&self, available_width: u16) -> usize {
        let input_value = self.input.value();
        let cursor_pos = self.input.cursor();
        if input_value.is_empty() {
            return 0;
        }

        let prefix_width = 14_u16;
        let content_width = available_width.saturating_sub(prefix_width + 2);
        if content_width == 0 {
            return 0;
        }

        let mut current_line = 0;
        let mut char_index = 0;

        for line in input_value.split('\n') {
            let line_len = line.chars().count();

            if char_index + line_len >= cursor_pos {
                // Cursor is in this line - calculate wrapped position
                let pos_in_line = cursor_pos - char_index;
                let wrapped_line_offset = pos_in_line / content_width as usize;
                return current_line + wrapped_line_offset;
            }

            // Add wrapped lines for this physical line
            let lines_for_this = if line.is_empty() {
                1
            } else {
                ((line_len as u16).div_ceil(content_width)).max(1) as usize
            };

            current_line += lines_for_this;
            char_index += line_len + 1; // +1 for newline
        }

        current_line
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ChatView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // v0.16.4: Calculate dynamic input height and ensure cursor is visible
        let input_height = self.calculate_input_height(area.width);
        let cursor_line = self.get_cursor_line(area.width);
        let total_lines = self.calculate_input_lines(area.width);
        self.ensure_input_cursor_visible(cursor_line, total_lines);

        // Layout v4: ProStatusBar (2 lines) | Messages + Mission Control | Input (dynamic) + Hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // ProStatusBar (2 lines - Claude Code inspired)
                Constraint::Min(10),   // Main content area
                Constraint::Length(input_height), // v0.16.4: Dynamic input height (1-10 lines + borders)
                Constraint::Length(1),            // Command hints
            ])
            .split(area);

        // 1. Pro Status Bar (Claude Code-inspired 2-line display)
        let chat_mode_indicator = match self.chat_mode {
            ChatMode::Infer => ChatModeIndicator::Infer,
            ChatMode::Agent => ChatModeIndicator::Agent,
        };

        ProStatusBar::new(&self.current_model, &self.session_metrics)
            .mode(chat_mode_indicator)
            .thinking(self.deep_thinking)
            .streaming(self.is_streaming)
            .render(chunks[0], frame.buffer_mut());

        // 2. Main content: Messages (65%) | Mission Control (35%)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(chunks[1]);

        // v0.9.1: NIKA Intro + Matrix Rain background effect
        // Phase 1: NIKA ASCII art appears and explodes
        // Phase 2: Matrix rain fades in then out
        let effect_area = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        if self.matrix_effect_enabled {
            // v0.9.2: NIKA butterfly pattern + fast explosion (~1.5s total)
            // Phase 1: Show NIKA with butterflies (frames 0-2) with quick fade-in
            // Phase 2: Fast wave explosion from center (frames 2-15)
            // Phase 3: Quick fade to clean UI

            // Determine pattern visibility and explosion state
            let show_pattern = self.nika_pattern_visible && self.explosion_frame < 15;
            let explosion = if self.explosion_frame > 2 {
                self.explosion_frame.saturating_sub(2) // Start explosion quickly
            } else {
                0
            };

            // Calculate opacity: fast transition
            let effective_opacity = if show_pattern {
                1.0 // Widget handles its own fading via easing
            } else {
                self.rain_opacity
            };

            if effective_opacity > 0.05 {
                MatrixRain::new()
                    .frame(self.frame)
                    .density(if show_pattern { 0.015 } else { 0.04 }) // Very sparse, cleaner
                    .opacity(effective_opacity)
                    .with_mascots(true)
                    .with_nika_pattern(show_pattern)
                    .explosion_frame(explosion)
                    .render(effect_area, frame.buffer_mut());
            }
        }

        // Messages panel with inline MCP/Infer boxes
        self.render_messages_v2(frame, main_chunks[0], theme);

        // v0.10.0: Right panel - DAG Panel or Mission Control (toggle with Ctrl+D)
        if self.show_dag_panel {
            // DAG visualization panel
            self.render_dag_panel(frame, main_chunks[1], theme);
        } else {
            // Mission Control panel (v0.7.3 - replaces Activity Stack)
            // v0.8.1: Now includes Activity section with hot/warm/queued tasks
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
                .activities(&self.activity_items) // v0.8.1: Activity items
                .frame(self.frame) // v0.8.1: Animation frame for spinners
                .focused(self.focused_panel == ChatPanel::Activity)
                .render(main_chunks[1], frame.buffer_mut());
        }

        // 3. Input panel
        self.render_input(frame, chunks[2], theme);

        // 4. Command hints
        self.render_hints(frame, chunks[3], theme);

        // 5. Command palette overlay (if visible)
        if self.command_palette.visible {
            let palette_area = centered_rect(60, 50, area);
            CommandPalette::new(&self.command_palette)
                .with_frame(self.frame)
                .render(palette_area, frame.buffer_mut());
        }

        // 6. Provider Modal overlay (if visible) - ⌘P
        if self.provider_modal.visible {
            // v0.8.9: Pass &mut for cloud_tab_label caching
            // v0.9.1: Pass theme for consistent styling
            ProviderModal::new(&mut self.provider_modal, theme).render(area, frame.buffer_mut());
        }

        // 7. Help overlay (if visible) - ? or F1
        if self.help_overlay.visible {
            let help_area = centered_rect(70, 80, area);
            HelpOverlay::new(&self.help_overlay).render(help_area, frame.buffer_mut());
        }

        // 8. v0.9 Phase 2: Mention autocomplete popup (if visible)
        if self.mention_autocomplete.visible {
            // Position popup above the input area
            let popup_height = (self.mention_autocomplete.suggestions.len().min(8) + 2) as u16;
            let popup_y = chunks[2].y.saturating_sub(popup_height);
            let popup_area = Rect::new(
                chunks[2].x + 2, // Slight offset from left
                popup_y,
                chunks[2].width.min(50).saturating_sub(4),
                popup_height,
            );
            MentionAutocomplete::new(&self.mention_autocomplete)
                .render(popup_area, frame.buffer_mut());
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Handle help overlay when visible (highest priority)
        if self.help_overlay.visible {
            return self.handle_help_overlay_key(key);
        }

        // Handle command palette when visible
        if self.command_palette.visible {
            return self.handle_palette_key(key);
        }

        // Handle Provider Modal when visible (⌘P)
        if self.provider_modal.visible {
            return self.handle_provider_modal_key(key);
        }

        // v0.9 Phase 3: Handle search mode when active
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

        // v0.9 Phase 2: Handle mention autocomplete when visible
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
                // Let other keys pass through to normal handling
                _ => {}
            }
        }

        // Check for Cmd/Ctrl+K (command palette toggle)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('k') {
            self.toggle_command_palette();
            return ViewAction::None;
        }

        // Check for Cmd/Ctrl+P (provider modal toggle)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('p') {
            let opened = self.toggle_provider_modal();
            // Trigger provider verification when modal opens (v0.8.8)
            if opened {
                return ViewAction::VerifyProviders;
            }
            return ViewAction::None;
        }

        // v0.9 Phase 3: Cmd/Ctrl+F = Start conversation search
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('f') {
            self.start_search();
            return ViewAction::None;
        }

        // v0.10.0: Cmd/Ctrl+D = Toggle DAG panel
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('d') {
            self.toggle_dag_panel();
            let status = if self.show_dag_panel {
                "visible"
            } else {
                "hidden"
            };
            self.add_system_message(format!("🔀 DAG panel {}", status));
            return ViewAction::None;
        }

        // Check for Cmd/Ctrl+T (toggle deep thinking)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('t') {
            self.toggle_deep_thinking();
            let status = if self.deep_thinking {
                "enabled"
            } else {
                "disabled"
            };
            self.add_system_message(format!("🧠 Deep thinking {}", status));
            return ViewAction::None;
        }

        // Alt+T = Toggle all thinking sections visibility (v0.9)
        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('t') {
            self.toggle_all_thinking();
            return ViewAction::None;
        }

        // Check for Cmd/Ctrl+M (toggle infer/agent mode)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('m') {
            self.toggle_mode();
            self.add_system_message(format!(
                "{} Switched to {} mode",
                self.chat_mode.icon(),
                self.chat_mode.label()
            ));
            return ViewAction::None;
        }

        // Cmd/Ctrl+C = Copy selection or input to system clipboard (NOT exit!)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('c') {
            // v0.8 Text Selection: If there's a selection, copy it
            if self.text_selection.is_some() {
                if self.copy_selection() {
                    self.add_system_message("📋 Selection copied to clipboard");
                    self.clear_selection();
                }
            } else {
                // Otherwise copy the input field
                self.copy_to_clipboard();
            }
            return ViewAction::None;
        }

        // Cmd/Ctrl+V = Paste from system clipboard
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return ViewAction::None;
        }

        // v0.16.4: Ctrl+Z = Undo input edit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
            if let Some((text, cursor)) = self.edit_history.undo() {
                self.input = Input::new(text);
                // Move cursor to saved position (clamped to string length)
                let pos = cursor.min(self.input.value().len());
                self.input.handle(InputRequest::GoToStart);
                for _ in 0..pos {
                    self.input.handle(InputRequest::GoToNextChar);
                }
            }
            return ViewAction::None;
        }

        // v0.16.4: Ctrl+Y = Redo input edit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('y') {
            if let Some((text, cursor)) = self.edit_history.redo() {
                self.input = Input::new(text);
                // Move cursor to saved position (clamped to string length)
                let pos = cursor.min(self.input.value().len());
                self.input.handle(InputRequest::GoToStart);
                for _ in 0..pos {
                    self.input.handle(InputRequest::GoToNextChar);
                }
            }
            return ViewAction::None;
        }

        // v0.9 Phase 2: Ctrl+R = Retry last failed MCP call
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('r') {
            if let Some(failed) = self.last_failed_mcp.take() {
                self.add_system_message("🔄 Retrying MCP call...".to_string());
                return ViewAction::ChatInvoke(failed.tool, Some(failed.server), failed.params);
            } else {
                self.add_system_message("⚠️ No failed MCP call to retry".to_string());
            }
            return ViewAction::None;
        }

        // Cmd/Ctrl+Left = Move to previous word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Left {
            self.cursor_prev_word();
            return ViewAction::None;
        }

        // Cmd/Ctrl+Right = Move to next word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Right {
            self.cursor_next_word();
            return ViewAction::None;
        }

        // Cmd/Ctrl+Backspace = Delete previous word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Backspace {
            self.delete_prev_word();
            return ViewAction::None;
        }

        // Cmd/Ctrl+A = Go to start of input
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('a') {
            self.cursor_start();
            return ViewAction::None;
        }

        // Cmd/Ctrl+E = Go to end of input
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('e') {
            self.cursor_end();
            return ViewAction::None;
        }

        // v0.16.4: Ctrl+j/k for vim-style panel navigation (works from any panel)
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('j') {
                self.focus_next_panel();
                return ViewAction::None;
            }
            if key.code == KeyCode::Char('k') {
                self.focus_prev_panel();
                return ViewAction::None;
            }
        }

        // F1 or ? = Toggle help overlay (global, works from any panel)
        if key.code == KeyCode::F(1)
            || (key.code == KeyCode::Char('?') && self.focused_panel != ChatPanel::Input)
        {
            self.help_overlay.toggle();
            return ViewAction::None;
        }

        // ═══════════════════════════════════════════════════════════════════════════════
        // Panel Navigation (v0.8.2 UX Enhancement)
        // Left/Right arrows cycle panels when NOT in Input panel
        // Tab = autocomplete verb in Input panel
        // ═══════════════════════════════════════════════════════════════════════════════

        // Tab = Autocomplete verb when in Input panel
        if key.code == KeyCode::Tab && self.focused_panel == ChatPanel::Input {
            if let Some(completed) = self.try_verb_autocomplete() {
                self.input = Input::new(completed);
                self.input.handle(InputRequest::GoToEnd);
            }
            return ViewAction::None;
        }

        // Left/Right arrows = Focus prev/next panel (when NOT in Input)
        if self.focused_panel != ChatPanel::Input {
            if key.code == KeyCode::Right {
                self.focus_next_panel();
                return ViewAction::None;
            }
            if key.code == KeyCode::Left {
                self.focus_prev_panel();
                return ViewAction::None;
            }
        }

        // Scroll keys and vim-style navigation when NOT in Input panel
        if self.focused_panel != ChatPanel::Input {
            match key.code {
                // j/Down = Scroll down
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll_down();
                    return ViewAction::None;
                }
                // k/Up = Scroll up
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_up();
                    return ViewAction::None;
                }
                // g = Go to top
                KeyCode::Char('g') => {
                    self.scroll_to_top();
                    return ViewAction::None;
                }
                // G = Go to bottom
                KeyCode::Char('G') => {
                    self.scroll_to_bottom();
                    return ViewAction::None;
                }
                // PageUp/PageDown for page scroll
                KeyCode::PageUp => {
                    self.page_up();
                    return ViewAction::None;
                }
                KeyCode::PageDown => {
                    self.page_down();
                    return ViewAction::None;
                }
                // y = Copy full message (including metadata)
                KeyCode::Char('y') => {
                    if self.copy_selected_message(false) {
                        self.add_system_message("📋 Message copied to clipboard".to_string());
                    }
                    return ViewAction::None;
                }
                // Y = Copy text only (no metadata)
                KeyCode::Char('Y') => {
                    if self.copy_selected_message(true) {
                        self.add_system_message("📋 Text copied to clipboard".to_string());
                    }
                    return ViewAction::None;
                }
                // t = Toggle thinking for selected message (v0.9)
                KeyCode::Char('t') => {
                    let cursor = self.conversation_scroll.cursor;
                    if cursor < self.messages.len() && self.messages[cursor].thinking.is_some() {
                        self.toggle_thinking(cursor);
                        let state = if self.is_thinking_visible(cursor) {
                            "expanded"
                        } else {
                            "collapsed"
                        };
                        self.add_system_message(format!("🧠 Thinking {} for message", state));
                    }
                    return ViewAction::None;
                }
                // Enter in Conversation/Activity = Return focus to Input
                KeyCode::Enter => {
                    self.focus_panel(ChatPanel::Input);
                    return ViewAction::None;
                }
                // Escape in non-Input panel = Return to Input
                KeyCode::Esc => {
                    self.focus_panel(ChatPanel::Input);
                    return ViewAction::None;
                }
                _ => {} // Fall through to existing match
            }
        }

        match key.code {
            // NOTE: Ctrl+K is handled earlier for Command Palette (line ~1901)
            // Ctrl+J scrolls down (vi-like) - doesn't conflict with anything
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_down();
                ViewAction::None
            }
            // v0.8.1: Removed 'q' to quit - too easy to accidentally quit when trying to chat
            // Use Ctrl+C (double-tap) instead
            // 's' when empty opens Settings view (consistent with other views)
            KeyCode::Char('s') if self.input.value().is_empty() => ViewAction::OpenSettings,
            // 'm' when empty cycles TaskBox render mode (v0.12.1)
            KeyCode::Char('m') if self.input.value().is_empty() => {
                self.cycle_task_box_render_mode();
                let mode_name = match self.task_box_render_mode {
                    RenderMode::Compact => "Compact (1 line)",
                    RenderMode::Expanded => "Expanded (default)",
                    RenderMode::Full => "Full (all details)",
                };
                self.add_system_message(format!("📦 TaskBox mode: {}", mode_name));
                ViewAction::None
            }
            // Shift+T or Ctrl+t toggles theme (v0.8.1 - consistent with app.rs)
            KeyCode::Char('T') => ViewAction::ToggleTheme,
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ViewAction::ToggleTheme
            }
            // "/" at start of empty input triggers command palette with verbs
            KeyCode::Char('/') if self.input.value().is_empty() => {
                self.toggle_command_palette();
                // Pre-filter to show only verbs category
                self.command_palette.query = "/".to_string();
                self.command_palette.update_filter();
                ViewAction::None
            }
            // "@" triggers file mention hint (already resolved on submit via FileResolver)
            KeyCode::Char('@') => {
                self.insert_char('@');
                // Show hint message about file mentions (only once per session)
                if !self.shown_file_hint {
                    self.add_system_message(
                        "💡 Type @filename to include file content".to_string(),
                    );
                    self.shown_file_hint = true;
                }
                ViewAction::None
            }
            // v0.9 Phase 2: Shift+Enter inserts newline for multi-line input
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.handle(InputRequest::InsertChar('\n'));
                ViewAction::None
            }
            KeyCode::Enter => {
                if let Some(message) = self.submit() {
                    // Parse the message as a command
                    let cmd = Command::parse(&message);

                    // Handle each command type
                    match cmd {
                        Command::Help => {
                            // Show help text inline
                            self.add_nika_message(HELP_TEXT.to_string(), None);
                            ViewAction::None
                        }
                        Command::Clear => ViewAction::ChatClear,
                        Command::Exec { command } => ViewAction::ChatExec(command),
                        Command::Fetch { url, method } => ViewAction::ChatFetch(url, method),
                        Command::FetchError {
                            error,
                            hint,
                            example,
                        } => {
                            // v0.8.2: Show helpful error message for fetch mistakes
                            self.add_nika_message(
                                format!("{}\n{}\n{}", error, hint, example),
                                None,
                            );
                            ViewAction::None
                        }
                        Command::Invoke {
                            tool,
                            server,
                            params,
                        } => ViewAction::ChatInvoke(tool, server, params),
                        Command::Agent {
                            goal,
                            max_turns,
                            mcp_servers,
                        } => {
                            ViewAction::ChatAgent(goal, max_turns, self.deep_thinking, mcp_servers)
                        }
                        Command::Mcp { action } => ViewAction::ChatMcp(action),
                        Command::Model { provider } => {
                            // Handle /model list inline
                            if provider == ModelProvider::List {
                                let providers = [
                                    ModelProvider::Claude,
                                    ModelProvider::OpenAI,
                                    ModelProvider::Mistral,
                                    ModelProvider::Groq,
                                    ModelProvider::DeepSeek,
                                    ModelProvider::Ollama,
                                ];
                                let mut list_text =
                                    String::from("Available providers (use /model <name>):\n");
                                for p in providers {
                                    let status = if p.is_available() {
                                        "available"
                                    } else {
                                        "missing API key"
                                    };
                                    list_text.push_str(&format!(
                                        "  - {}: {} ({})\n",
                                        p.command_name(),
                                        p.name(),
                                        status
                                    ));
                                }
                                self.add_nika_message(list_text.trim_end().to_string(), None);
                                ViewAction::None
                            } else {
                                ViewAction::ChatModelSwitch(provider)
                            }
                        }
                        Command::Export { format, path } => {
                            // v0.9: Export chat to JSON or YAML file
                            let result = match format {
                                ExportFormat::Json => self.export_session(path.as_deref()),
                                ExportFormat::Yaml => self.export_session_yaml(path.as_deref()),
                            };
                            match result {
                                Ok(filepath) => {
                                    let format_name = match format {
                                        ExportFormat::Json => "JSON",
                                        ExportFormat::Yaml => "YAML workflow",
                                    };
                                    self.add_system_message(format!(
                                        "📤 Chat exported as {} to: {}",
                                        format_name, filepath
                                    ));
                                }
                                Err(e) => {
                                    self.add_system_message(format!("❌ Export failed: {}", e));
                                }
                            }
                            ViewAction::None
                        }
                        Command::Infer { prompt } | Command::Chat { message: prompt } => {
                            // Resolve @file mentions in the prompt
                            let base_dir = std::env::current_dir().unwrap_or_default();
                            let expanded = FileResolver::resolve(&prompt, &base_dir);
                            ViewAction::ChatInfer(expanded)
                        }
                    }
                } else {
                    ViewAction::None
                }
            }
            // v0.8.1: Up/Down ALWAYS scroll conversation (NovaNet pattern)
            // Use Ctrl+Up/Down for history navigation
            KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_up();
                ViewAction::None
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_down();
                ViewAction::None
            }
            KeyCode::Up => {
                self.scroll_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.scroll_down();
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
            KeyCode::PageUp => {
                self.page_up(); // v0.8.1: Use page scroll, not single line
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.page_down(); // v0.8.1: Use page scroll, not single line
                ViewAction::None
            }
            KeyCode::Home => {
                self.scroll = 0;
                ViewAction::None
            }
            KeyCode::End => {
                self.scroll_to_bottom();
                ViewAction::None
            }
            // Tab/Shift+Tab handled above for panel navigation
            // Esc switches to Explorer view (when in Input panel)
            KeyCode::Esc => ViewAction::SwitchView(TuiView::Studio),
            _ => ViewAction::None,
        }
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
            self.provider_name,
            self.current_model,
            streaming_status
        )
    }
}

impl ChatView {
    // Key handlers extracted to keys.rs
    // Input rendering extracted to input.rs

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.10.0: Chat DAG Visualization
    // ═══════════════════════════════════════════════════════════════════════════

    /// Render the DAG panel showing chat as a graph + task queue
    fn render_dag_panel(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        // Split into: DAG graph (top 60%) | Task queue (bottom 40%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        // 1. DAG Graph Panel (nodes + edges)
        let mut dag_panel = ChatDagPanel::new().with_title("CHAT DAG");
        for node in &self.dag_nodes {
            dag_panel.add_node(node.clone());
        }
        for edge in &self.dag_edges {
            dag_panel.add_edge(edge.clone());
        }
        if let Some(idx) = self.dag_selected {
            if let Some(node) = self.dag_nodes.get(idx) {
                dag_panel.select(&node.id);
            }
        }
        dag_panel.set_animation_tick(self.frame);
        dag_panel.render(chunks[0], frame.buffer_mut());

        // 2. Task Queue Panel (pending/running/completed)
        let mut task_queue = ChatTaskQueue::new().with_title("TASK QUEUE");
        for item in &self.task_queue {
            task_queue.add(item.clone());
        }
        task_queue.render(chunks[1], frame.buffer_mut());
    }

    /// Sync DAG state from chat messages (call when messages change)
    pub fn sync_dag_from_messages(&mut self) {
        self.dag_nodes.clear();
        self.dag_edges.clear();

        // v0.13: Use ChatWorkflow's DAG for proper dependency edges
        let messages = self.workflow.all_messages();

        for (i, (idx, msg)) in messages.iter().enumerate() {
            let kind = match msg.role {
                WorkflowRole::User => ChatNodeKind::User,
                WorkflowRole::Assistant => ChatNodeKind::Assistant,
                WorkflowRole::System => ChatNodeKind::System,
                WorkflowRole::Tool => ChatNodeKind::ToolCall,
            };

            // Create node with truncated label
            let label = if msg.content.len() > 30 {
                format!("{}...", &msg.content[..27])
            } else {
                msg.content.clone()
            };

            let node = DagNodeData::new(&msg.id, kind, i as u32)
                .with_label(&label)
                .with_state(ChatNodeState::Complete);

            self.dag_nodes.push(node);

            // v0.13: Use actual DAG edges (includes @N mention references)
            for dep_idx in self.workflow.get_dependencies(*idx) {
                if let Some(dep_msg) = self.workflow.get_message_by_index(dep_idx) {
                    self.dag_edges.push(DagEdgeData::new(&dep_msg.id, &msg.id));
                }
            }
        }
    }

    /// Add a task to the queue (call when task execution starts)
    pub fn add_task_to_queue(&mut self, id: &str, verb: ChatTaskVerb) {
        let item = ChatTaskQueueItem::new(id, verb);
        self.task_queue.push(item);
    }

    /// Update task state in queue
    pub fn update_task_state(
        &mut self,
        id: &str,
        state: ChatTaskState,
        elapsed: Option<std::time::Duration>,
    ) {
        if let Some(task) = self.task_queue.iter_mut().find(|t| t.id() == id) {
            task.set_state(state);
            if let Some(e) = elapsed {
                task.set_elapsed(e);
            }
        }
    }

    /// v0.12.1: Complete the last running task of a specific verb in the queue
    fn complete_last_running_task(&mut self, verb: ChatTaskVerb, elapsed_ms: u64) {
        // Find and update the last running task of this verb type
        if let Some(task) = self
            .task_queue
            .iter_mut()
            .rev()
            .find(|t| t.verb() == verb && t.state() == ChatTaskState::Running)
        {
            task.set_state(ChatTaskState::Complete);
            task.set_elapsed(std::time::Duration::from_millis(elapsed_ms));
            task.set_progress(1.0); // Fix: Mark as 100% complete
        }
    }

    /// v0.12.1: Fail the last running task of a specific verb in the queue
    fn fail_last_running_task(&mut self, verb: ChatTaskVerb, elapsed_ms: u64) {
        // Find and update the last running task of this verb type
        if let Some(task) = self
            .task_queue
            .iter_mut()
            .rev()
            .find(|t| t.verb() == verb && t.state() == ChatTaskState::Running)
        {
            task.set_state(ChatTaskState::Failed);
            task.set_elapsed(std::time::Duration::from_millis(elapsed_ms));
            task.set_progress(1.0); // Fix: Failed tasks are also 100% done
        }
    }

    /// Toggle DAG panel visibility (Ctrl+D)
    pub fn toggle_dag_panel(&mut self) {
        self.show_dag_panel = !self.show_dag_panel;
        if self.show_dag_panel {
            // Sync DAG state when showing
            self.sync_dag_from_messages();
        }
    }

    /// Cycle TaskBox render mode: Compact → Expanded → Full → Compact
    /// v0.12.1: Toggle with `m` key when not in input mode
    pub fn cycle_task_box_render_mode(&mut self) {
        self.task_box_render_mode = self.task_box_render_mode.cycle();
    }

    /// Render command hints bar
    fn render_hints(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // v0.8.7: Fixed labels to match actual behavior
        let hints = Line::from(vec![
            Span::styled(
                " ⌘K ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" commands  "),
            Span::styled(
                " ⌘P ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" model  "),
            Span::styled(
                " ⇧P ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" providers  "),
            Span::styled(
                " 1234 ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" views  "),
            Span::styled(
                " Esc ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" close"),
        ]);

        let paragraph = Paragraph::new(hints);
        frame.render_widget(paragraph, area);
    }
}

/// Convert character offset to byte offset in a UTF-8 string
/// This handles multi-byte characters correctly
fn char_to_byte_offset(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Helper function to create a centered rectangle for overlays
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
mod tests;
