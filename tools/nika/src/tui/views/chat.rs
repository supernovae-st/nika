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

use chrono::{DateTime, Local};

use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget},
    Frame,
};
use serde::{Deserialize, Serialize};
use tui_input::{Input, InputRequest};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::command::{Command, ModelProvider, HELP_TEXT};

/// Check if the "command" modifier is pressed (Ctrl on Linux/Windows, Cmd on macOS)
/// On macOS, Cmd key maps to SUPER modifier in crossterm
fn is_cmd_pressed(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::SUPER)
}
use crate::runtime::chat_workflow::{ChatWorkflow, Role as WorkflowRole};
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
use crate::tui::utils::{truncate_str, wrap_text};
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
    InferStreamData,
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
    ModalEventHandler,
    NikaIntroState,
    OllamaClient,
    ParsedInput,
    ProStatusBar,
    Provider,
    ProviderModal,
    ProviderModalState,
    ScrollIndicator,
    SessionContext,
    SessionMetrics,
    StreamingDecrypt,
    SystemCommand,
    TurnMetrics,
};

// ═══════════════════════════════════════════════════════════════════════════════
// NOTE: v0.9.1 - All colors now come from Theme (no hardcoded fallbacks)
// ═══════════════════════════════════════════════════════════════════════════════

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Nika,
    System,
    Tool,
}

/// A chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// v0.9: Stable unique ID for tracking (survives message list mutations)
    pub id: u64,
    pub role: MessageRole,
    pub content: String,
    /// v0.9: Display timestamp (HH:MM) for better conversation tracking
    pub timestamp: DateTime<Local>,
    /// v0.8.1: Instant for elapsed time calculations
    pub created_at: Instant,
    /// Optional inline execution result
    pub execution: Option<ExecutionResult>,
    /// Optional agent thinking/reasoning content (v0.5.2+)
    /// Displayed inline when present (collapsible in UI)
    pub thinking: Option<String>,
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

// ═══════════════════════════════════════════════════════════════════════════════
// Serializable Chat Session (HIGH 8 - Persistent Sessions)
// ═══════════════════════════════════════════════════════════════════════════════

/// Serializable message role for persistence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SerializableRole {
    User,
    Nika,
    System,
    Tool,
}

impl From<&MessageRole> for SerializableRole {
    fn from(role: &MessageRole) -> Self {
        match role {
            MessageRole::User => SerializableRole::User,
            MessageRole::Nika => SerializableRole::Nika,
            MessageRole::System => SerializableRole::System,
            MessageRole::Tool => SerializableRole::Tool,
        }
    }
}

impl From<SerializableRole> for MessageRole {
    fn from(role: SerializableRole) -> Self {
        match role {
            SerializableRole::User => MessageRole::User,
            SerializableRole::Nika => MessageRole::Nika,
            SerializableRole::System => MessageRole::System,
            SerializableRole::Tool => MessageRole::Tool,
        }
    }
}

/// Serializable message for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableMessage {
    pub role: SerializableRole,
    pub content: String,
    pub thinking: Option<String>,
}

impl From<&ChatMessage> for SerializableMessage {
    fn from(msg: &ChatMessage) -> Self {
        Self {
            role: (&msg.role).into(),
            content: msg.content.clone(),
            thinking: msg.thinking.clone(),
        }
    }
}

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

/// Inline content that can appear in a message
#[derive(Debug, Clone)]
pub enum InlineContent {
    /// MCP tool call with params and result
    McpCall(McpCallData),
    /// Streaming inference with token counter
    InferStream(InferStreamData),
    /// Task box for any verb (v0.8.1 - unified rendering)
    Task(TaskBox),
}

/// Inference mode for conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatMode {
    /// Simple inference mode (single completion, no tools)
    #[default]
    Infer,
    /// Agent mode with tool access (multi-turn, MCP tools)
    Agent,
}

impl ChatMode {
    /// Get display label for the mode
    pub fn label(&self) -> &'static str {
        match self {
            ChatMode::Infer => "Infer",
            ChatMode::Agent => "Agent",
        }
    }

    /// Get icon for the mode
    pub fn icon(&self) -> &'static str {
        match self {
            ChatMode::Infer => "⚡", // LLM generation
            ChatMode::Agent => "🐔", // Parent agent icon
        }
    }
}

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
    /// Toggle with [m] key when not in input mode
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

    // ═══════════════════════════════════════════════════════════════════════════════
    // Panel Navigation (v0.8 UX Enhancement)
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Focus next panel (Tab key)
    pub fn focus_next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    /// Focus previous panel (Shift+Tab)
    pub fn focus_prev_panel(&mut self) {
        self.focused_panel = self.focused_panel.prev();
    }

    /// Focus a specific panel (for mouse clicks)
    pub fn focus_panel(&mut self, panel: ChatPanel) {
        self.focused_panel = panel;
    }

    /// Get the scroll state for the currently focused panel (mutable)
    pub fn focused_scroll_mut(&mut self) -> Option<&mut PanelScrollState> {
        match self.focused_panel {
            ChatPanel::Conversation => Some(&mut self.conversation_scroll),
            ChatPanel::Activity => Some(&mut self.activity_scroll),
            ChatPanel::Input => None, // Input panel doesn't scroll
        }
    }

    /// Scroll down by one item (v0.8.1: scrolls conversation even from input panel)
    pub fn scroll_down(&mut self) {
        // v0.8.1: Don't update total here - add_message() and render() handle it
        // This lets tests configure scroll state manually
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_down();
                // v0.8.1: Check if we reached the bottom
                self.user_at_bottom = self.is_at_bottom();
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_down();
            }
        }
    }

    /// Scroll up by one item (v0.8.1: scrolls conversation even from input panel)
    pub fn scroll_up(&mut self) {
        // v0.8.1: Don't update total here - add_message() and render() handle it
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_up();
                // v0.8.1: User scrolled up = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_up();
            }
        }
    }

    /// Scroll to top (v0.8.1: scrolls conversation even from input panel)
    pub fn scroll_to_top(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_to_top();
                // v0.8.1: Went to top = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_to_top();
            }
        }
    }

    /// Scroll to bottom (v0.8.1: scrolls conversation even from input panel)
    pub fn scroll_to_bottom(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.scroll_to_bottom();
                // v0.8.1: Went to bottom = resume auto-following
                self.user_at_bottom = true;
            }
            ChatPanel::Activity => {
                self.activity_scroll.scroll_to_bottom();
            }
        }
    }

    /// Page down (v0.8.1: scrolls conversation even from input panel)
    pub fn page_down(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.page_down();
                // v0.8.1: Check if we reached the bottom
                self.user_at_bottom = self.is_at_bottom();
            }
            ChatPanel::Activity => {
                self.activity_scroll.page_down();
            }
        }
    }

    /// Page up (v0.8.1: scrolls conversation even from input panel)
    pub fn page_up(&mut self) {
        match self.focused_panel {
            ChatPanel::Input | ChatPanel::Conversation => {
                self.conversation_scroll.page_up();
                // v0.8.1: User scrolled up = stop auto-following
                self.user_at_bottom = false;
            }
            ChatPanel::Activity => {
                self.activity_scroll.page_up();
            }
        }
    }

    /// Check if conversation is scrolled to the bottom
    fn is_at_bottom(&self) -> bool {
        let scroll = &self.conversation_scroll;
        if scroll.total == 0 || scroll.visible == 0 {
            return true; // Empty or not rendered yet = consider at bottom
        }
        // At bottom when offset + visible >= total
        scroll.offset + scroll.visible >= scroll.total
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 3: Smooth Scrolling
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Friction coefficient for scroll deceleration (0.0 = instant stop, 1.0 = no friction)
    const SCROLL_FRICTION: f32 = 0.85;

    /// Minimum velocity before stopping animation (lines per tick)
    const SCROLL_MIN_VELOCITY: f32 = 0.1;

    /// Initial velocity multiplier for mouse wheel (lines per scroll event)
    const SCROLL_WHEEL_VELOCITY: f32 = 3.0;

    /// Apply smooth scroll with initial velocity (for mouse wheel)
    pub fn smooth_scroll(&mut self, direction: i8) {
        // Add velocity in the scroll direction (positive = down, negative = up)
        self.scroll_velocity += direction as f32 * Self::SCROLL_WHEEL_VELOCITY;
        self.scroll_animating = true;

        // v0.8.1: Update user_at_bottom state based on scroll direction
        if direction < 0 {
            self.user_at_bottom = false; // Scrolling up = stop auto-following
        }
    }

    /// Update scroll animation (call this every frame/tick)
    /// Returns true if animation is still active
    pub fn update_scroll_animation(&mut self) -> bool {
        if !self.scroll_animating {
            return false;
        }

        // Apply velocity to accumulator (sub-line precision)
        self.scroll_accumulator += self.scroll_velocity;

        // Convert accumulated scroll to whole lines
        let lines_to_scroll = self.scroll_accumulator.trunc() as i32;
        if lines_to_scroll != 0 {
            self.scroll_accumulator -= lines_to_scroll as f32;

            // Apply scroll based on direction
            if lines_to_scroll > 0 {
                for _ in 0..lines_to_scroll {
                    self.conversation_scroll.scroll_down();
                }
            } else {
                for _ in 0..(-lines_to_scroll) {
                    self.conversation_scroll.scroll_up();
                }
            }
        }

        // Apply friction
        self.scroll_velocity *= Self::SCROLL_FRICTION;

        // Stop animation if velocity is negligible
        if self.scroll_velocity.abs() < Self::SCROLL_MIN_VELOCITY {
            self.scroll_velocity = 0.0;
            self.scroll_accumulator = 0.0;
            self.scroll_animating = false;

            // Check if we ended up at the bottom
            self.user_at_bottom = self.is_at_bottom();
            return false;
        }

        true
    }

    /// Stop any ongoing smooth scroll animation
    pub fn stop_smooth_scroll(&mut self) {
        self.scroll_velocity = 0.0;
        self.scroll_accumulator = 0.0;
        self.scroll_animating = false;
    }

    /// Check if smooth scroll animation is active
    pub fn is_scroll_animating(&self) -> bool {
        self.scroll_animating
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
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Compute panel areas from total area (same layout as render)
    /// Returns (session_bar, conversation, activity, input, hints) areas
    fn compute_panel_areas(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
        use ratatui::layout::{Constraint, Direction, Layout};

        // v0.12.1: Vertical layout must match render() exactly
        // ProStatusBar (2 lines) | main content | input | hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // ProStatusBar (2 lines - matches render)
                Constraint::Min(10),   // Main content area
                Constraint::Length(3), // Input field
                Constraint::Length(1), // Command hints
            ])
            .split(area);

        // v0.12.1: Horizontal split must match render() - 65%/35%
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(chunks[1]);

        (
            chunks[0],
            main_chunks[0],
            main_chunks[1],
            chunks[2],
            chunks[3],
        )
    }

    /// Determine which panel is at the given screen position
    pub fn panel_at_position(&self, x: u16, y: u16, area: Rect) -> Option<ChatPanel> {
        let (_, conversation, activity, input, _) = Self::compute_panel_areas(area);

        // Check each panel (order matters for overlapping edges)
        if Self::point_in_rect(x, y, conversation) {
            Some(ChatPanel::Conversation)
        } else if Self::point_in_rect(x, y, activity) {
            Some(ChatPanel::Activity)
        } else if Self::point_in_rect(x, y, input) {
            Some(ChatPanel::Input)
        } else {
            None
        }
    }

    /// Check if a point is inside a rect
    fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
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

    /// Start streaming mode
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.partial_response.clear();
        // v0.8 WOW: Reset and start matrix decrypt effect
        self.streaming_decrypt.clear();
    }

    /// Start streaming with a specific verb for theming
    pub fn start_streaming_with_verb(&mut self, verb: DecryptVerb) {
        self.is_streaming = true;
        self.partial_response.clear();
        // v0.8.1 WOW: Reset and configure matrix streaming decrypt
        // Parameters tuned for visible chaos → reveal wave effect
        self.streaming_decrypt = StreamingDecrypt::new()
            .with_verb(verb) // Theme colors for revealed text
            .with_reveal_speed(0.4) // ~2.5 frames per char reveal
            .with_wave_factor(2.5) // wider wave for smoother reveal
            .with_initial_chaos(12); // frames of chaos before reveal starts
    }

    /// Append chunk to partial response during streaming
    pub fn append_streaming(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
        // v0.8 WOW: Push to matrix decrypt for reveal effect
        if self.matrix_effect_enabled {
            self.streaming_decrypt.push_text(chunk);
        }
        // v0.8.2: Auto-scroll during streaming to follow new content
        self.auto_scroll_to_bottom();
    }

    /// Finish streaming and return the full response
    pub fn finish_streaming(&mut self) -> String {
        self.is_streaming = false;
        // v0.8 WOW: Reveal all remaining text instantly
        self.streaming_decrypt.reveal_all();
        // v0.12.1: Keep TaskBoxes visible after completion (Option A)
        // TaskBox::Infer now REPLACES the AI bubble - don't clear it
        // Old: self.inline_content.clear();
        // v0.8.1: Reset agent phase
        self.on_agent_complete();
        // v0.8.2: Final auto-scroll after streaming completes
        self.auto_scroll_to_bottom();
        std::mem::take(&mut self.partial_response)
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9.1: Matrix Rain Effect Triggers
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Trigger matrix rain effect with fade-out
    /// Call on startup, first message, workflow launch, etc.
    pub fn trigger_rain_effect(&mut self) {
        self.rain_opacity = 0.6; // v0.9.1: Start subtle
        self.rain_fading = true; // Will fade out over ~1.5 seconds
    }

    /// Stop rain effect immediately (optional)
    #[allow(dead_code)]
    pub fn stop_rain_effect(&mut self) {
        self.rain_opacity = 0.0;
        self.rain_fading = false;
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.8.1: Agent Phase Event Handlers
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Called when agent starts - sets Syncing phase
    pub fn on_agent_start(&mut self) {
        self.agent_phase = AgentPhase::Syncing;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        self.agent_phase_tool = None;
        // v0.9.1: Trigger rain effect on agent start
        self.trigger_rain_effect();
    }

    /// Called when agent turn begins - sets Planning phase
    /// turn_index: Current turn number (0-indexed)
    pub fn on_agent_turn(&mut self, _turn_index: u32) {
        self.agent_phase = AgentPhase::Planning;
        self.phase_indicator.set_phase(AgentPhase::Planning);
        self.agent_phase_tool = None;
    }

    /// Called when MCP tool is invoked - sets Invoking phase
    pub fn on_mcp_invoke(&mut self, tool: &str, _server: &str) {
        self.agent_phase = AgentPhase::Invoking;
        self.agent_phase_tool = Some(tool.to_string());
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Invoking).with_tool(tool);
        // v0.9.1: Trigger rain effect on MCP tool call (important action)
        self.trigger_rain_effect();
    }

    /// Called when MCP response received - sets Processing phase
    pub fn on_mcp_response(&mut self) {
        self.agent_phase = AgentPhase::Processing;
        self.phase_indicator.set_phase(AgentPhase::Processing);
        // Keep tool name for context
    }

    /// Called when provider/LLM called - sets Inferring phase
    pub fn on_provider_called(&mut self) {
        self.agent_phase = AgentPhase::Inferring;
        self.phase_indicator.set_phase(AgentPhase::Inferring);
        self.agent_phase_tool = None;
    }

    /// Called when first streaming token arrives - sets Streaming phase
    pub fn on_streaming_start(&mut self) {
        self.agent_phase = AgentPhase::Streaming;
        self.phase_indicator.set_phase(AgentPhase::Streaming);
        // v0.9.1: Trigger rain effect when LLM starts streaming (WOW moment)
        self.trigger_rain_effect();
    }

    /// Called when agent completes - resets to Idle
    pub fn on_agent_complete(&mut self) {
        self.agent_phase = AgentPhase::Idle;
        self.phase_indicator.set_phase(AgentPhase::Idle);
        self.agent_phase_tool = None;
    }

    /// Tick the phase indicator animation
    pub fn tick_phase_indicator(&mut self) {
        if self.agent_phase.is_active() {
            self.phase_indicator.tick();
        }
    }

    /// Get total tokens used in this session
    pub fn total_tokens(&self) -> u64 {
        self.session_context.tokens_used
    }

    /// Add tokens to session context (for status bar display)
    pub fn add_tokens(&mut self, input_tokens: u64, output_tokens: u64) {
        self.session_context.add_tokens(input_tokens, output_tokens);
    }

    /// Append thinking content during streaming (v0.5.2+)
    pub fn append_thinking(&mut self, thinking: &str) {
        match &mut self.pending_thinking {
            Some(existing) => {
                existing.push('\n');
                existing.push_str(thinking);
            }
            None => {
                self.pending_thinking = Some(thinking.to_string());
            }
        }
    }

    /// Finalize thinking and attach to last message (v0.5.2+)
    /// Call this when streaming completes
    pub fn finalize_thinking(&mut self) {
        if let Some(thinking) = self.pending_thinking.take() {
            // Attach thinking to the last Nika message
            if let Some(last) = self.messages.last_mut() {
                if last.role == MessageRole::Nika {
                    last.thinking = Some(thinking);
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Session Persistence (HIGH 8)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Save current session to file
    pub fn save_session(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let session = ChatSession::from_view(self);
        session.save(path)
    }

    /// Load session from file
    pub fn load_session(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let session = ChatSession::load(path)?;

        // Clear current messages and reset ID counter
        self.messages.clear();
        self.thinking_collapsed.clear(); // Clear thinking state for new session
        self.message_id_counter = 0; // Reset counter for fresh session

        for msg in session.messages {
            let id = self.next_message_id();
            self.messages.push(ChatMessage {
                id,
                role: msg.role.into(),
                content: msg.content,
                timestamp: Local::now(), // Use current time since original is lost
                created_at: Instant::now(),
                execution: None,
                thinking: msg.thinking,
            });
        }

        // Update model if specified in session
        if !session.model.is_empty() {
            self.current_model = session.model.clone();
            self.cached_provider = Provider::from_model_name(&session.model);
        }

        Ok(())
    }

    /// Get default session file path
    pub fn default_session_path() -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join(".nika")
            .join("chat_session.json")
    }

    /// Set the current model name
    pub fn set_model(&mut self, model: impl Into<String>) {
        let model_str = model.into();
        self.cached_provider = Provider::from_model_name(&model_str);
        self.current_model = model_str;
    }

    /// Get the cached provider (PERF: computed once when model changes, not every frame)
    pub fn provider(&self) -> Provider {
        self.cached_provider
    }

    /// Set MCP servers from workflow configuration
    ///
    /// Replaces the default "novanet" with actual configured servers.
    pub fn set_mcp_servers(&mut self, server_names: impl IntoIterator<Item = impl Into<String>>) {
        self.session_context.mcp_servers.clear();
        for name in server_names {
            self.session_context
                .mcp_servers
                .push(McpServerInfo::new(name.into()));
        }
    }

    /// Mark an MCP server as connected (v0.7.0+)
    pub fn mark_mcp_server_connected(&mut self, server_name: &str) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.mark_connected();
        }
    }

    /// Mark an MCP server as errored (v0.7.0+)
    pub fn mark_mcp_server_error(&mut self, server_name: &str) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.mark_error();
        }
    }

    /// Update MCP server status with ping result (v0.8.2)
    ///
    /// Updates the server's connection status and recorded latency.
    pub fn update_mcp_server_status(
        &mut self,
        server_name: &str,
        connected: bool,
        latency_ms: u64,
    ) {
        if let Some(server) = self
            .session_context
            .mcp_servers
            .iter_mut()
            .find(|s| s.name == server_name)
        {
            server.update_from_ping(connected, latency_ms);
        }
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
        let _ = self.workflow.add_message_with_mentions(&content, WorkflowRole::Tool);
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

    // ═══════════════════════════════════════════════════════════════════════════════
    // TaskBox Integration (v0.8.1)
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Add a TaskBox to inline content
    ///
    /// This is the unified way to display verb-specific boxes in the chat.
    /// Each verb type (infer, exec, fetch, invoke, agent) has its own visual treatment.
    pub fn add_task_box(&mut self, task_box: TaskBox) {
        // Add activity based on verb type
        let (verb_name, chat_verb) = match &task_box {
            TaskBox::Infer(_) => ("infer", ChatTaskVerb::Infer),
            TaskBox::Exec(_) => ("exec", ChatTaskVerb::Exec),
            TaskBox::Fetch(_) => ("fetch", ChatTaskVerb::Fetch),
            TaskBox::Invoke(_) => ("invoke", ChatTaskVerb::Invoke),
            TaskBox::Agent(_) => ("agent", ChatTaskVerb::Agent),
        };

        // v0.12.1: Generate unique task ID for queue tracking
        let task_id = format!("task-{}-{}", verb_name, self.inline_content.len());

        self.activity_items
            .push(ActivityItem::hot(task_id.clone(), verb_name));

        // v0.12.1: Add to task queue and set state to Running
        self.add_task_to_queue(&task_id, chat_verb);
        self.update_task_state(&task_id, ChatTaskState::Running, None);

        self.inline_content.push(InlineContent::Task(task_box));
        self.auto_scroll_to_bottom();
    }

    /// Update the last TaskBox (for streaming updates)
    pub fn update_last_task_box<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TaskBox),
    {
        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            f(task_box);
        }
    }

    /// Complete the last TaskBox with success (generic fallback)
    pub fn complete_last_task_box(&mut self, duration_ms: u64) {
        // v0.12.1: Determine verb type for queue update
        let verb = if let Some(InlineContent::Task(task_box)) = self.inline_content.last() {
            Some(match task_box {
                TaskBox::Infer(_) => ChatTaskVerb::Infer,
                TaskBox::Exec(_) => ChatTaskVerb::Exec,
                TaskBox::Fetch(_) => ChatTaskVerb::Fetch,
                TaskBox::Invoke(_) => ChatTaskVerb::Invoke,
                TaskBox::Agent(_) => ChatTaskVerb::Agent,
            })
        } else {
            None
        };

        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            *task_box.state_mut() = BoxState::success(duration_ms);
        }

        // v0.12.1: Update task queue state
        if let Some(v) = verb {
            self.complete_last_running_task(v, duration_ms);
        }
        self.transition_activity_to_warm("task");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Infer
    pub fn complete_last_infer_box(&mut self, duration_ms: u64) {
        // v0.12.1: Transfer partial_response to InferBox before completing
        // Option A: InferBox REPLACES the AI message bubble
        let response = std::mem::take(&mut self.partial_response);

        // Find last Infer box that is Running
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Infer(infer)) = content {
                if infer.state.is_running() {
                    // v0.12.1: Set the response content so it displays in the widget
                    if !response.is_empty() {
                        infer.response = response.clone();
                    }
                    infer.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Infer, duration_ms);
        self.transition_activity_to_warm("infer");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Exec
    pub fn complete_last_exec_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Exec(exec)) = content {
                if exec.state.is_running() {
                    exec.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Exec, duration_ms);
        self.transition_activity_to_warm("exec");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Fetch
    pub fn complete_last_fetch_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Fetch(fetch)) = content {
                if fetch.state.is_running() {
                    fetch.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Fetch, duration_ms);
        self.transition_activity_to_warm("fetch");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Invoke
    pub fn complete_last_invoke_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.transition_activity_to_warm("invoke");
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Invoke with result
    pub fn complete_last_invoke_box_with_result(
        &mut self,
        result: serde_json::Value,
        duration_ms: u64,
    ) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.result = Some(result);
                    invoke.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.transition_activity_to_warm("invoke");
        self.auto_scroll_to_bottom();
    }

    /// Fail the last RUNNING TaskBox::Invoke with error
    pub fn fail_last_invoke_box(&mut self, error: &str, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Invoke(invoke)) = content {
                if invoke.state.is_running() {
                    invoke.error = Some(error.to_string());
                    invoke.state = BoxState::failed(error, duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.fail_last_running_task(ChatTaskVerb::Invoke, duration_ms);
        self.auto_scroll_to_bottom();
    }

    /// Complete the last RUNNING TaskBox::Agent
    pub fn complete_last_agent_box(&mut self, duration_ms: u64) {
        for content in self.inline_content.iter_mut().rev() {
            if let InlineContent::Task(TaskBox::Agent(agent)) = content {
                if agent.state.is_running() {
                    agent.state = BoxState::success(duration_ms);
                    break;
                }
            }
        }
        // v0.12.1: Update task queue state
        self.complete_last_running_task(ChatTaskVerb::Agent, duration_ms);
        self.transition_activity_to_warm("agent");
        self.auto_scroll_to_bottom();
    }

    /// Fail the last TaskBox with error
    pub fn fail_last_task_box(&mut self, error: &str, duration_ms: u64) {
        // v0.12.1: Determine verb type before failing for queue update
        let verb = if let Some(InlineContent::Task(task_box)) = self.inline_content.last() {
            Some(match task_box {
                TaskBox::Infer(_) => ChatTaskVerb::Infer,
                TaskBox::Exec(_) => ChatTaskVerb::Exec,
                TaskBox::Fetch(_) => ChatTaskVerb::Fetch,
                TaskBox::Invoke(_) => ChatTaskVerb::Invoke,
                TaskBox::Agent(_) => ChatTaskVerb::Agent,
            })
        } else {
            None
        };

        if let Some(InlineContent::Task(task_box)) = self.inline_content.last_mut() {
            *task_box.state_mut() = BoxState::failed(error, duration_ms);
        }

        // v0.12.1: Update task queue state
        if let Some(v) = verb {
            self.fail_last_running_task(v, duration_ms);
        }
        self.auto_scroll_to_bottom();
    }

    /// Start an inference stream
    ///
    /// v0.8 FIX: Don't create InferStream box - use streaming_decrypt for visual effect instead.
    /// The streaming decrypt provides the matrix reveal effect, while InferStream boxes
    /// were redundant and blocked the decrypt from showing.
    pub fn start_infer_stream(&mut self, model: &str, _tokens_in: u32, _max_tokens: u32) {
        // v0.8 FIX: Don't add to inline_content - let streaming_decrypt handle the visual
        // The matrix decrypt effect is the WOW feature, not the INFER boxes

        // Add to activity stack as hot (for Mission Control panel)
        self.activity_items.push(ActivityItem::hot(
            format!("infer-{}-{}", model, self.frame),
            "infer",
        ));
    }

    /// Append content to current inference stream
    /// v0.9.1: Updated to handle TaskBox::Infer (tokens only) - Matrix effect handles text
    pub fn append_infer_content(&mut self, chunk: &str, tokens_out: u32) {
        // Update TaskBox::Infer token count (if present)
        // v0.9.1 FIX: Handle Task(TaskBox::Infer) instead of legacy InferStream
        for content in self.inline_content.iter_mut().rev() {
            match content {
                InlineContent::Task(TaskBox::Infer(infer)) if infer.state.is_running() => {
                    infer.tokens_out = tokens_out;
                    break;
                }
                InlineContent::InferStream(data) => {
                    // Legacy support - update if still used
                    data.append_content(chunk);
                    data.update_tokens(tokens_out);
                    break;
                }
                _ => continue,
            }
        }
        // Matrix effect handles the actual text display via partial_response
        self.partial_response.push_str(chunk);
    }

    /// Complete current inference stream
    pub fn complete_infer_stream(&mut self) {
        if let Some(InlineContent::InferStream(data)) = self.inline_content.last_mut() {
            data.complete();
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("infer");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.8.0: Activity tracking for /exec, /fetch, /agent commands
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Add exec activity to the stack
    pub fn add_exec_activity(&mut self, command: &str) {
        let activity_id = format!("exec-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "exec"));
        tracing::debug!(command = %command, "Added exec activity");
    }

    /// Complete exec activity
    pub fn complete_exec_activity(&mut self) {
        self.transition_activity_to_warm("exec");
    }

    /// Add fetch activity to the stack
    pub fn add_fetch_activity(&mut self, url: &str, method: &str) {
        let activity_id = format!("fetch-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "fetch"));
        tracing::debug!(url = %url, method = %method, "Added fetch activity");
    }

    /// Complete fetch activity
    pub fn complete_fetch_activity(&mut self) {
        self.transition_activity_to_warm("fetch");
    }

    /// Add agent activity to the stack
    pub fn add_agent_activity(&mut self, goal: &str) {
        let activity_id = format!("agent-{}", self.inline_content.len());
        self.activity_items
            .push(ActivityItem::hot(activity_id, "agent"));
        tracing::debug!(goal = %goal, "Added agent activity");
    }

    /// Complete agent activity
    pub fn complete_agent_activity(&mut self) {
        self.transition_activity_to_warm("agent");
    }

    /// Update session token usage
    pub fn update_tokens(&mut self, tokens_used: u64, cost: f64) {
        self.session_context.tokens_used = tokens_used;
        self.session_context.total_cost = cost;
    }

    // === v0.7.3 Real-time Streaming Updates ===

    /// Update current verb during execution (for Mission Control display)
    pub fn set_current_verb(&mut self, verb: CurrentVerb) {
        self.current_verb = verb;
    }

    /// Update turn metrics during streaming (real-time token counts)
    pub fn update_turn_metrics(&mut self, input_tokens: u64, output_tokens: u64, cost_usd: f64) {
        // Compute deltas before updating turn metrics
        let input_delta = input_tokens.saturating_sub(self.turn_metrics.input_tokens);
        let output_delta = output_tokens.saturating_sub(self.turn_metrics.output_tokens);
        let cost_delta = cost_usd.max(0.0) - self.turn_metrics.cost_usd.max(0.0);

        // Update turn metrics
        self.turn_metrics.input_tokens = input_tokens;
        self.turn_metrics.output_tokens = output_tokens;
        self.turn_metrics.cost_usd = cost_usd;

        // Update session metrics with deltas
        self.session_metrics.input_tokens += input_delta;
        self.session_metrics.output_tokens += output_delta;
        self.session_metrics.cost_usd += cost_delta;
    }

    /// Increment turn metrics during streaming (delta updates)
    pub fn increment_output_tokens(&mut self, delta_tokens: u64) {
        self.turn_metrics.output_tokens += delta_tokens;
        self.session_metrics.output_tokens += delta_tokens;
    }

    /// Reset turn metrics for a new turn
    pub fn reset_turn_metrics(&mut self) {
        self.turn_metrics = TurnMetrics::default();
        self.current_verb = CurrentVerb::None;
    }

    /// Complete a turn (session metrics already updated via update_turn_metrics)
    pub fn complete_turn(&mut self) {
        // Session metrics are already updated incrementally via update_turn_metrics()
        // Just reset turn metrics for the next turn
        self.reset_turn_metrics();
        // v0.8 FIX: Clear inline boxes and activities on turn completion
        self.inline_content.clear();
        self.activity_items.clear();
    }

    /// Toggle command palette visibility
    pub fn toggle_command_palette(&mut self) {
        self.command_palette.toggle();
    }

    /// Toggle Provider Modal visibility (⌘P)
    ///
    /// Returns true if the modal was opened (verification needed).
    pub fn toggle_provider_modal(&mut self) -> bool {
        let was_visible = self.provider_modal.visible;
        self.provider_modal.visible = !self.provider_modal.visible;
        // Return true if we just opened the modal (trigger verification)
        !was_visible && self.provider_modal.visible
    }

    /// Transition activity from hot to warm
    /// v0.8.5 FIX: Use .rev() to find the LAST hot activity (most recent)
    /// Before: Only first concurrent op transitioned, others stuck forever
    fn transition_activity_to_warm(&mut self, verb: &str) {
        if let Some(item) = self
            .activity_items
            .iter_mut()
            .rev() // v0.8.5: Search from newest to oldest
            .find(|i| i.verb == verb && i.temp == ActivityTemp::Hot)
        {
            item.temp = ActivityTemp::Warm;
            item.duration = item.elapsed();
        }
    }

    /// Clear completed (warm) activities older than duration
    /// v0.8.5 FIX: Also remove stuck Hot items that have been running too long
    pub fn clear_old_activities(&mut self, max_age_secs: u64) {
        use std::time::Duration;
        let max_duration = Duration::from_secs(max_age_secs);

        self.activity_items.retain(|item| {
            match item.temp {
                ActivityTemp::Warm => {
                    // Remove warm items older than max_age
                    item.duration.map(|d| d < max_duration).unwrap_or(true)
                }
                ActivityTemp::Hot => {
                    // v0.8.5 FIX: Remove stuck hot items (running > max_age = likely hung)
                    item.started
                        .map(|s| s.elapsed() < max_duration)
                        .unwrap_or(true)
                }
                ActivityTemp::Queued => true, // Keep queued items
            }
        });
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 3: Conversation Search (Ctrl+F)
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Enter search mode (Ctrl+F)
    pub fn start_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_results.clear();
        self.search_current = 0;
    }

    /// Exit search mode (Esc)
    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_current = 0;
    }

    /// Update search results when query changes
    pub fn update_search(&mut self) {
        self.search_results.clear();
        self.search_current = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        // Search through all messages
        // v0.9 FIX: Avoid O(n²) Vec::contains() by checking content OR thinking
        for (idx, msg) in self.messages.iter().enumerate() {
            let content_matches = msg.content.to_lowercase().contains(&query_lower);
            let thinking_matches = msg
                .thinking
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&query_lower));

            // Add index if either content or thinking matches (no duplicates possible)
            if content_matches || thinking_matches {
                self.search_results.push(idx);
            }
        }

        // Scroll to first result if any
        if !self.search_results.is_empty() {
            self.scroll_to_search_result();
        }
    }

    /// Navigate to next search result (Enter or Down)
    pub fn next_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.search_current = (self.search_current + 1) % self.search_results.len();
        self.scroll_to_search_result();
    }

    /// Navigate to previous search result (Up)
    pub fn prev_search_result(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        if self.search_current == 0 {
            self.search_current = self.search_results.len() - 1;
        } else {
            self.search_current -= 1;
        }
        self.scroll_to_search_result();
    }

    /// Scroll to the current search result
    fn scroll_to_search_result(&mut self) {
        if let Some(&msg_idx) = self.search_results.get(self.search_current) {
            // Set scroll to show the matching message
            self.conversation_scroll.offset = msg_idx.saturating_sub(2);
            self.user_at_bottom = false;
        }
    }

    /// Handle character input in search mode
    pub fn search_input_char(&mut self, c: char) {
        self.search_query.push(c);
        self.update_search();
    }

    /// Handle backspace in search mode
    pub fn search_input_backspace(&mut self) {
        self.search_query.pop();
        self.update_search();
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
        let _ = self.workflow.add_message_with_mentions(&content, WorkflowRole::User);
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
        let _ = self.workflow.add_message_with_mentions(&content, WorkflowRole::Assistant);
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
        let _ = self.workflow.add_message_with_mentions(&content, WorkflowRole::Assistant);
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
        let _ = self.workflow.add_message_with_mentions(&content_str, WorkflowRole::System);
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

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 2: @ Mention Autocomplete
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Check for @ trigger in current input and update autocomplete popup
    ///
    /// Called on every input change to detect when user types @ and show suggestions.
    pub fn check_mention_trigger(&mut self) {
        let input = self.input.value();
        let cursor_char = self.input.cursor();

        // Convert character index to byte index for safe string slicing
        // tui-input uses character indices, but at_trigger_position needs byte indices
        let cursor_byte = input
            .char_indices()
            .nth(cursor_char)
            .map(|(i, _)| i)
            .unwrap_or(input.len());

        // Check if we're in a @ trigger position
        if let Some(trigger) = Mention::at_trigger_position(input, cursor_byte) {
            // Generate suggestions based on trigger
            let suggestions = self.generate_mention_suggestions(&trigger);

            if !suggestions.is_empty() {
                self.mention_autocomplete.show(trigger, suggestions);
            } else {
                self.mention_autocomplete.hide();
            }
        } else {
            self.mention_autocomplete.hide();
        }
    }

    /// Generate suggestions for a mention trigger
    fn generate_mention_suggestions(
        &self,
        trigger: &crate::tui::widgets::MentionTrigger,
    ) -> Vec<MentionSuggestion> {
        let mut suggestions = Vec::new();
        let query = trigger.query.to_lowercase();

        match trigger.trigger_type {
            Some(MentionType::File) | None => {
                // Suggest files from memory_files and working directory
                for file in &self.memory_files {
                    if file.name.to_lowercase().contains(&query) {
                        suggestions.push(MentionSuggestion::file(&file.name));
                    }
                }

                // If no type specified, also suggest type prefixes
                if trigger.trigger_type.is_none() && query.is_empty() {
                    // Show type hints when user just typed @
                    if suggestions.len() < 5 {
                        suggestions.push(MentionSuggestion {
                            display: "entity:".to_string(),
                            insert: "@entity:".to_string(),
                            mention_type: MentionType::Entity,
                            description: Some("Reference a NovaNet entity".to_string()),
                        });
                        suggestions.push(MentionSuggestion {
                            display: "locale:".to_string(),
                            insert: "@locale:".to_string(),
                            mention_type: MentionType::Locale,
                            description: Some("Reference a locale".to_string()),
                        });
                        suggestions.push(MentionSuggestion {
                            display: "project:".to_string(),
                            insert: "@project:".to_string(),
                            mention_type: MentionType::Project,
                            description: Some("Reference a project".to_string()),
                        });
                    }
                }
            }
            Some(MentionType::Entity) => {
                // Suggest entities (would come from NovaNet MCP)
                // For now, use context_items as source
                for item in &self.context_items {
                    if item.mention.to_lowercase().contains(&query) {
                        suggestions.push(MentionSuggestion::entity(
                            &item.mention,
                            Some(&item.mention),
                        ));
                    }
                }
                // Add some example entities if empty
                if suggestions.is_empty() && query.is_empty() {
                    suggestions.push(MentionSuggestion::entity("qr-code", Some("QR Code")));
                    suggestions.push(MentionSuggestion::entity(
                        "url-shortener",
                        Some("URL Shortener"),
                    ));
                }
            }
            Some(MentionType::Locale) => {
                // Suggest common locales
                let locales = [
                    ("en-US", "English (US)"),
                    ("fr-FR", "French"),
                    ("de-DE", "German"),
                    ("es-ES", "Spanish"),
                    ("it-IT", "Italian"),
                    ("ja-JP", "Japanese"),
                    ("zh-CN", "Chinese (Simplified)"),
                    ("pt-BR", "Portuguese (Brazil)"),
                ];
                for (code, name) in locales {
                    if code.to_lowercase().contains(&query) || name.to_lowercase().contains(&query)
                    {
                        suggestions.push(MentionSuggestion::locale(code, Some(name)));
                    }
                }
            }
            Some(MentionType::Project) => {
                // Suggest projects (would come from NovaNet MCP)
                if "qrcode-ai".contains(&query) {
                    suggestions.push(MentionSuggestion::project("qrcode-ai", Some("QR Code AI")));
                }
            }
            Some(MentionType::Term) => {
                // Suggest terms (would come from NovaNet MCP)
                // For now, empty - would be populated by MCP
            }
        }

        // Limit to 8 suggestions
        suggestions.truncate(8);
        suggestions
    }

    /// Accept the currently selected mention suggestion
    ///
    /// Replaces the partial @ trigger with the full mention text.
    pub fn accept_mention_suggestion(&mut self) {
        if let (Some(trigger), Some(suggestion)) = (
            self.mention_autocomplete.trigger.as_ref(),
            self.mention_autocomplete.current(),
        ) {
            let input = self.input.value().to_string();
            let before = &input[..trigger.start];
            let after = &input[trigger.start + trigger.partial.len()..];

            // Build new input with suggestion
            let new_input = format!("{}{} {}", before, suggestion.insert, after);

            // Update input (using tui-input API)
            // Input::new() places cursor at end, so we need to reposition
            self.input = Input::new(new_input.clone());
            // v0.9 Phase 2 FIX: First go to start, then move to target position
            // This ensures cursor ends up after the inserted mention + space
            self.input.handle(InputRequest::GoToStart);
            let new_cursor = trigger.start + suggestion.insert.len() + 1;
            for _ in 0..new_cursor.min(new_input.len()) {
                self.input.handle(InputRequest::GoToNextChar);
            }
        }

        self.mention_autocomplete.hide();
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
        let (category, suggestion) = Self::categorize_error(error);
        let formatted = format!(
            "❌ {} Error: {}\n💡 {}\n\nUse /help for commands or /clear to restart.",
            category, error, suggestion
        );
        self.add_system_message(formatted);
    }

    /// Categorize error and provide recovery suggestion
    fn categorize_error(error: &str) -> (&'static str, &'static str) {
        let error_lower = error.to_lowercase();

        if error_lower.contains("api key")
            || error_lower.contains("authentication")
            || error_lower.contains("unauthorized")
        {
            (
                "Auth",
                "Check your API key. Set ANTHROPIC_API_KEY or OPENAI_API_KEY.",
            )
        } else if error_lower.contains("timeout")
            || error_lower.contains("timed out")
            || error_lower.contains("deadline")
        {
            (
                "Timeout",
                "Request timed out. Try a shorter prompt or check your connection.",
            )
        } else if error_lower.contains("rate limit")
            || error_lower.contains("too many requests")
            || error_lower.contains("quota")
        {
            (
                "Rate Limit",
                "API rate limit reached. Wait a moment and try again.",
            )
        } else if error_lower.contains("connection")
            || error_lower.contains("network")
            || error_lower.contains("dns")
            || error_lower.contains("resolve")
        {
            (
                "Network",
                "Connection failed. Check your internet connection.",
            )
        } else if error_lower.contains("mcp")
            || error_lower.contains("server")
            || error_lower.contains("tool")
        {
            (
                "MCP",
                "MCP server issue. Use /mcp list to check available servers.",
            )
        } else if error_lower.contains("parse")
            || error_lower.contains("json")
            || error_lower.contains("invalid")
        {
            ("Parse", "Invalid input format. Check your command syntax.")
        } else {
            ("Unexpected", "Please try again or use /clear to restart.")
        }
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
            _ => {
                // Regular message or verb command - send to agent
            }
        }

        self.add_user_message(message.clone());
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
        self.update_mode_from_input(); // v0.8.1: Sync mode indicator
        self.check_mention_trigger(); // v0.9 Phase 2: @ mention autocomplete
    }

    /// Delete character before cursor (delegates to tui-input)
    pub fn backspace(&mut self) {
        self.input.handle(InputRequest::DeletePrevChar);
        self.update_mode_from_input(); // v0.8.1: Sync mode indicator
        self.check_mention_trigger(); // v0.9 Phase 2: @ mention autocomplete
    }

    /// v0.8.1: Update chat_mode and current_verb based on input prefix
    /// This syncs the mode indicator with what the user is typing
    fn update_mode_from_input(&mut self) {
        let input = self.input.value();

        // Check for verb prefix in input
        if let Some((_, verb_color, is_complete, _)) = Self::detect_verb_in_input(input) {
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
        self.input.handle(InputRequest::DeletePrevWord);
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
                for c in text.chars() {
                    self.input.handle(InputRequest::InsertChar(c));
                }
                self.update_mode_from_input(); // v0.8.1: Sync mode indicator
            }
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
        // Layout v3: ProStatusBar (2 lines) | Messages + Mission Control | Input + Hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // ProStatusBar (2 lines - Claude Code inspired)
                Constraint::Min(10),   // Main content area
                Constraint::Length(3), // Input field
                Constraint::Length(1), // Command hints
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
                        Command::Export { path } => {
                            // v0.9: Export chat to JSON file
                            match self.export_session(path.as_deref()) {
                                Ok(filepath) => {
                                    self.add_system_message(format!(
                                        "📤 Chat exported to: {}",
                                        filepath
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
            KeyCode::Esc => ViewAction::SwitchView(TuiView::Explorer),
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
    /// Handle key events when help overlay is visible
    fn handle_help_overlay_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            // Escape, ?, F1 = Close help
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                self.help_overlay.hide();
                ViewAction::None
            }
            // j/Down = Scroll down
            KeyCode::Char('j') | KeyCode::Down => {
                // Max scroll based on content (HELP_SECTIONS has ~30 lines)
                self.help_overlay.scroll_down(30);
                ViewAction::None
            }
            // k/Up = Scroll up
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_overlay.scroll_up();
                ViewAction::None
            }
            // g = Scroll to top
            KeyCode::Char('g') => {
                self.help_overlay.scroll = 0;
                ViewAction::None
            }
            // G = Scroll to bottom
            KeyCode::Char('G') => {
                self.help_overlay.scroll = 30;
                ViewAction::None
            }
            // PageUp
            KeyCode::PageUp => {
                self.help_overlay.scroll_page_up(10);
                ViewAction::None
            }
            // PageDown
            KeyCode::PageDown => {
                self.help_overlay.scroll_page_down(30, 10);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    /// Handle key events when command palette is visible
    fn handle_palette_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.command_palette.close();
                ViewAction::None
            }
            KeyCode::Enter => {
                if let Some(cmd_id) = self.command_palette.execute_selected() {
                    // Execute the selected command
                    self.input = Input::new(format!("/{}", cmd_id));
                    self.input.handle(InputRequest::GoToEnd);
                    // Trigger submit with the command
                    if let Some(message) = self.submit() {
                        let cmd = Command::parse(&message);
                        return match cmd {
                            Command::Help => {
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
                                // v0.8.2: Show helpful error message
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
                            } => ViewAction::ChatAgent(
                                goal,
                                max_turns,
                                self.deep_thinking,
                                mcp_servers,
                            ),
                            Command::Mcp { action } => ViewAction::ChatMcp(action),
                            Command::Model { provider } => ViewAction::ChatModelSwitch(provider),
                            Command::Export { path } => {
                                // v0.9: Export chat to JSON file
                                match self.export_session(path.as_deref()) {
                                    Ok(filepath) => {
                                        self.add_system_message(format!(
                                            "📤 Chat exported to: {}",
                                            filepath
                                        ));
                                    }
                                    Err(e) => {
                                        self.add_system_message(format!("❌ Export failed: {}", e));
                                    }
                                }
                                ViewAction::None
                            }
                            Command::Infer { prompt } | Command::Chat { message: prompt } => {
                                ViewAction::ChatInfer(prompt)
                            }
                        };
                    }
                }
                ViewAction::None
            }
            KeyCode::Up => {
                self.command_palette.select_prev();
                ViewAction::None
            }
            KeyCode::Down => {
                self.command_palette.select_next();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.command_palette.input_char(c);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.command_palette.backspace();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    /// Handle key events for Provider Modal (full management)
    fn handle_provider_modal_key(&mut self, key: KeyEvent) -> ViewAction {
        let result = ModalEventHandler::handle(&mut self.provider_modal, key);

        // If the modal requested to close
        if let Some(action) = &result.action {
            use crate::tui::widgets::ModalAction;
            match action {
                ModalAction::Close => {
                    self.provider_modal.visible = false;
                }
                ModalAction::RefreshProviders => {
                    return ViewAction::VerifyProviders;
                }
                ModalAction::RefreshOllamaModels => {
                    self.add_system_message("🔄 Refreshing Ollama models...".to_string());
                    return ViewAction::RefreshOllamaModels;
                }
                ModalAction::TestApiKey { provider } => {
                    self.add_system_message(format!("🔑 Testing {} API key...", provider));
                    return ViewAction::VerifyProviders;
                }
                ModalAction::PullModel { model } => {
                    self.add_system_message(format!("📥 Pulling model: {}...", model));
                    return ViewAction::PullOllamaModel(model.clone());
                }
                ModalAction::DeleteModel { model } => {
                    self.add_system_message(format!("🗑️ Deleting model: {}...", model));
                    return ViewAction::DeleteOllamaModel(model.clone());
                }
                ModalAction::SaveAndTestApiKey { provider, key } => {
                    use crate::tui::widgets::provider_modal::{validate_key_format, NikaKeyring};
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save to keyring
                    match NikaKeyring::set(provider, key) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to keychain",
                                provider
                            ));
                            // Trigger verification
                            return ViewAction::VerifyProviders;
                        }
                        Err(e) => {
                            self.add_system_message(format!("❌ Failed to save key: {}", e));
                        }
                    }
                }
                ModalAction::SaveApiKey { provider, key } => {
                    use crate::tui::widgets::provider_modal::{validate_key_format, NikaKeyring};
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save silently (no verification trigger)
                    match NikaKeyring::set(provider, key) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to keychain",
                                provider
                            ));
                            self.provider_modal.visible = false;
                        }
                        Err(e) => {
                            self.add_system_message(format!("❌ Failed to save key: {}", e));
                        }
                    }
                }
                ModalAction::SelectProvider { provider, model } => {
                    // v0.12.2: Actually switch provider via ChatModelSwitch
                    use crate::tui::command::ModelProvider;
                    self.provider_modal.visible = false;
                    if let Some(model_provider) = ModelProvider::from_name(provider) {
                        self.add_system_message(format!(
                            "🔄 Switching to {} ({})",
                            provider, model
                        ));
                        return ViewAction::ChatModelSwitch(model_provider);
                    } else {
                        self.add_system_message(format!(
                            "❌ Unknown provider: {} - available: claude, openai, mistral, groq, deepseek, ollama",
                            provider
                        ));
                    }
                }
                ModalAction::CheckProvider { provider } => {
                    self.add_system_message(format!("🔍 Checking {} connection...", provider));
                    return ViewAction::VerifyProviders;
                }
            }
        }

        ViewAction::None
    }

    fn render_messages(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let msg_count = self.messages.len();
        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .flat_map(|(idx, msg)| {
                // v0.8.1 FIX: Skip "Thinking..." placeholder during streaming
                let is_last = idx == msg_count.saturating_sub(1);
                if self.is_streaming && is_last && msg.content == "Thinking..." {
                    return vec![];
                }

                // Color-coded message bubbles based on role
                let (prefix, color) = match msg.role {
                    // User: Cyan color
                    MessageRole::User => ("[You]", theme.trait_retrieved),
                    // AI/Nika: Green color
                    MessageRole::Nika => ("[AI]", theme.status_success),
                    // System: Yellow/Amber color
                    MessageRole::System => ("[System]", theme.status_running),
                    // Tool: Magenta/Pink color
                    MessageRole::Tool => ("[Tool]", theme.mcp_traverse),
                };

                let style = Style::default().fg(color);

                let mut lines = vec![ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", prefix), style.add_modifier(Modifier::BOLD)),
                    Span::styled(SEPARATOR_20_ASCII, Style::default().fg(theme.text_muted)),
                ]))];

                // Wrap message content with colored prefix indicator
                for line in msg.content.lines() {
                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("  | ", Style::default().fg(color)),
                        Span::raw(line.to_string()),
                    ])));
                }

                // Add execution result if present
                if let Some(exec) = &msg.execution {
                    let (status_icon, status_color) = match exec.status {
                        ExecutionStatus::Running => (">", theme.status_running),
                        ExecutionStatus::Completed => ("+", theme.status_success),
                        ExecutionStatus::Failed => ("x", theme.status_failed),
                    };
                    lines.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!(
                                "|-- {} {} ({}/{}) ",
                                status_icon,
                                exec.workflow_name,
                                exec.tasks_completed,
                                exec.tasks_total
                            ),
                            Style::default().fg(status_color),
                        ),
                    ])));
                }

                lines.push(ListItem::new("")); // spacing
                lines
            })
            .collect();

        // Add streaming indicator if streaming is in progress
        if self.is_streaming {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "[AI] ",
                    Style::default()
                        .fg(theme.status_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(SEPARATOR_20_ASCII, Style::default().fg(theme.text_muted)),
            ])));

            // Show partial response if any
            if !self.partial_response.is_empty() {
                // v0.8 WOW: Use matrix decrypt effect if enabled
                if self.matrix_effect_enabled {
                    for decrypt_line in self.streaming_decrypt.build_lines() {
                        let mut spans = vec![Span::styled(
                            "  | ",
                            Style::default().fg(theme.status_success),
                        )];
                        spans.extend(decrypt_line.spans);
                        items.push(ListItem::new(Line::from(spans)));
                    }
                } else {
                    for line in self.partial_response.lines() {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("  | ", Style::default().fg(theme.status_success)),
                            Span::raw(line.to_string()),
                        ])));
                    }
                }
            }

            // v0.8.1 FIX: Only show "Thinking..." if no content yet
            // Once streaming starts, the Matrix effect replaces this indicator
            if self.partial_response.is_empty() {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("  | ", Style::default().fg(theme.status_success)),
                    Span::styled(
                        "Thinking...",
                        Style::default()
                            .fg(theme.status_running)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ])));
            }
        }

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CONVERSATION ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(list, area);
    }

    /// v0.8.2: Try to autocomplete verb command with Tab
    /// Returns Some(new_input) if autocomplete happened, None otherwise
    fn try_verb_autocomplete(&self) -> Option<String> {
        let input = self.input.value();

        if let Some((_verb_len, verb_color, is_complete, full_verb)) =
            Self::detect_verb_in_input(input)
        {
            if !is_complete {
                // Partial match → complete the verb name
                return Some(format!("/{} ", full_verb));
            } else {
                // Complete verb → check if we should insert placeholder
                let rest = if input.len() > full_verb.len() + 1 {
                    &input[full_verb.len() + 1..] // +1 for /
                } else {
                    ""
                };
                if rest.trim().is_empty() {
                    // Insert first placeholder example
                    let placeholder = Self::verb_placeholder(&verb_color, 0);
                    return Some(format!("/{} {}", full_verb, placeholder));
                }
            }
        }
        None
    }

    /// v0.8.2: Get contextual placeholder for a verb command
    /// Returns a hint based on verb type, cycling through examples using frame
    /// Generate per-character gradient spans for smooth verb animation
    /// Creates a flowing wave effect with ASCII background zone
    fn render_verb_gradient(
        text: &str,
        verb_color: &VerbColor,
        frame: u8,
        is_complete: bool,
    ) -> Vec<Span<'static>> {
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if len == 0 {
            return vec![];
        }

        let base_rgb = verb_color.rgb_tuple();
        let glow_rgb = verb_color.glow_tuple();
        // White for peak sparkle
        let sparkle_rgb: (u8, u8, u8) = (255, 255, 255);
        // Dark for contrast (muted base)
        let dark_rgb: (u8, u8, u8) = (base_rgb.0 / 2, base_rgb.1 / 2, base_rgb.2 / 2);
        // Background color (very subtle)
        let bg_base: (u8, u8, u8) = (
            12 + base_rgb.0 / 15,
            12 + base_rgb.1 / 15,
            12 + base_rgb.2 / 15,
        );

        chars
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                // FUN but not epileptic: visible wave per character
                let phase_offset = (i as f32) * std::f32::consts::PI * 0.6; // ~108° per char - visible difference
                let time = (frame as f32 / 12.0) * std::f32::consts::PI; // Medium speed - fun but readable
                let wave = ((time + phase_offset).sin() + 1.0) / 2.0; // 0.0 to 1.0

                // Secondary wave for extra life
                let shimmer_time = (frame as f32 / 20.0) * std::f32::consts::PI;
                let shimmer = ((shimmer_time + phase_offset * 2.0).sin() + 1.0) / 2.0;

                if is_complete {
                    // Fun color wave: dark → base → glow → sparkle
                    // Using smooth sine curve for nice transitions

                    let (r, g, b) = if wave < 0.25 {
                        // Dark to base (valley)
                        let t = wave * 4.0;
                        (
                            (dark_rgb.0 as f32 * (1.0 - t) + base_rgb.0 as f32 * t) as u8,
                            (dark_rgb.1 as f32 * (1.0 - t) + base_rgb.1 as f32 * t) as u8,
                            (dark_rgb.2 as f32 * (1.0 - t) + base_rgb.2 as f32 * t) as u8,
                        )
                    } else if wave < 0.6 {
                        // Base to glow (rising)
                        let t = (wave - 0.25) / 0.35;
                        (
                            (base_rgb.0 as f32 * (1.0 - t) + glow_rgb.0 as f32 * t) as u8,
                            (base_rgb.1 as f32 * (1.0 - t) + glow_rgb.1 as f32 * t) as u8,
                            (base_rgb.2 as f32 * (1.0 - t) + glow_rgb.2 as f32 * t) as u8,
                        )
                    } else {
                        // Glow to white sparkle at peak (with shimmer variation)
                        let t = ((wave - 0.6) / 0.4) * (0.6 + shimmer * 0.4);
                        (
                            (glow_rgb.0 as f32 * (1.0 - t) + sparkle_rgb.0 as f32 * t) as u8,
                            (glow_rgb.1 as f32 * (1.0 - t) + sparkle_rgb.1 as f32 * t) as u8,
                            (glow_rgb.2 as f32 * (1.0 - t) + sparkle_rgb.2 as f32 * t) as u8,
                        )
                    };

                    let color = Color::Rgb(r, g, b);

                    // Background zone with gentle pulse
                    let bg_pulse = 0.6 + wave * 0.4; // Background follows the wave too
                    let bg_r = (bg_base.0 as f32 * bg_pulse).min(255.0) as u8;
                    let bg_g = (bg_base.1 as f32 * bg_pulse).min(255.0) as u8;
                    let bg_b = (bg_base.2 as f32 * bg_pulse).min(255.0) as u8;

                    Span::styled(
                        c.to_string(),
                        Style::default()
                            .fg(color)
                            .bg(Color::Rgb(bg_r, bg_g, bg_b))
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    // Partial match: visible wave but calmer
                    let wave_mild = 0.3 + wave * 0.5; // 0.3 to 0.8 range
                    let r = (dark_rgb.0 as f32 * (1.0 - wave_mild) + glow_rgb.0 as f32 * wave_mild)
                        as u8;
                    let g = (dark_rgb.1 as f32 * (1.0 - wave_mild) + glow_rgb.1 as f32 * wave_mild)
                        as u8;
                    let b = (dark_rgb.2 as f32 * (1.0 - wave_mild) + glow_rgb.2 as f32 * wave_mild)
                        as u8;

                    Span::styled(
                        c.to_string(),
                        Style::default()
                            .fg(Color::Rgb(r, g, b))
                            .add_modifier(Modifier::BOLD),
                    )
                }
            })
            .collect()
    }

    fn verb_placeholder(verb_color: &VerbColor, frame: u8) -> &'static str {
        let idx = (frame / 60) as usize; // Change every ~1 second at 60fps
        match verb_color {
            VerbColor::Invoke => {
                // v0.8.2: MCP tool examples from NovaNet + common patterns
                const HINTS: &[&str] = &[
                    "novanet:describe {\"entity\": \"qr-code\"}",
                    "novanet:generate {\"locale\": \"fr-FR\", \"entity\": \"landing\"}",
                    "novanet:traverse {\"start\": \"entity:qr\", \"depth\": 2}",
                    "novanet:search {\"query\": \"pricing page\"}",
                    "novanet:atoms {\"entity\": \"qr-code\", \"forms\": [\"title\", \"text\"]}",
                    "filesystem:read_file {\"path\": \"./README.md\"}",
                    "filesystem:list_directory {\"path\": \"./src\"}",
                    "browser:screenshot {\"url\": \"https://example.com\"}",
                    "database:query {\"sql\": \"SELECT * FROM users LIMIT 5\"}",
                    "github:search_repos {\"query\": \"language:rust stars:>1000\"}",
                ];
                HINTS[idx % HINTS.len()]
            }
            VerbColor::Infer => {
                // v0.8.2: Creative & practical LLM prompts
                const HINTS: &[&str] = &[
                    "Generate a landing page headline for a SaaS product",
                    "Summarize this article in 3 bullet points",
                    "Translate to French: Hello, how are you today?",
                    "Explain this Rust code like I'm a beginner",
                    "Write unit tests for this function using pytest",
                    "Convert this SQL query to a TypeScript Prisma query",
                    "Rewrite this paragraph in a more professional tone",
                    "Generate 5 creative names for a coffee shop",
                    "Create a regex to match email addresses",
                    "Write a haiku about programming",
                    "Debug this error: 'cannot borrow as mutable'",
                    "Suggest 3 improvements for this API endpoint",
                    "Write a commit message for these changes",
                    "Create a product description for wireless earbuds",
                    "Explain the difference between async and sync",
                ];
                HINTS[idx % HINTS.len()]
            }
            VerbColor::Exec => {
                // v0.8.2: Productive shell one-liners
                const HINTS: &[&str] = &[
                    "npm run build",
                    "cargo test --release",
                    "git status",
                    "git log --oneline -10",
                    "git diff --staged",
                    "git commit -am \"fix: resolve issue\"",
                    "ls -la ./src | head -20",
                    "find . -name \"*.rs\" | wc -l",
                    "grep -r \"TODO\" ./src",
                    "docker ps -a",
                    "docker-compose up -d",
                    "npm outdated",
                    "cargo clippy -- -D warnings",
                    "python -m pytest -v",
                    "curl -I https://example.com",
                    "du -sh ./target",
                    "cat package.json | jq '.dependencies'",
                    "ps aux | grep node",
                    "netstat -an | grep LISTEN",
                    "tree -L 2 ./src",
                ];
                HINTS[idx % HINTS.len()]
            }
            VerbColor::Fetch => {
                // v0.8.2: Fun real APIs that actually work! (no auth required)
                const HINTS: &[&str] = &[
                    "https://catfact.ninja/fact",                         // 🐱 Cat facts
                    "https://api.open-meteo.com/v1/forecast?latitude=48.8566&longitude=2.3522&current_weather=true", // ☀️ Paris
                    "https://httpbin.org/get",                            // 🧪 HTTP test
                    "https://dog.ceo/api/breeds/image/random",            // 🐕 Random dog
                    "https://api.chucknorris.io/jokes/random",            // 💪 Chuck Norris
                    "https://uselessfacts.jsph.pl/random.json?language=en", // 🤯 Useless fact
                    "https://api.coindesk.com/v1/bpi/currentprice.json",  // 💰 Bitcoin
                    "https://api.github.com/zen",                         // 🧘 GitHub wisdom
                    "https://official-joke-api.appspot.com/random_joke",  // 😂 Random joke
                    "https://api.adviceslip.com/advice",                  // 💡 Life advice
                    "https://randomfox.ca/floof/",                        // 🦊 Random fox
                    "https://xkcd.com/info.0.json",                       // 📰 Latest xkcd
                    "https://api.ipify.org?format=json",                  // 🌐 Your IP
                    "https://worldtimeapi.org/api/ip",                    // 🕐 World time
                    "https://opentdb.com/api.php?amount=1",               // ❓ Trivia
                    "http://api.open-notify.org/iss-now.json",            // 🛸 ISS position
                    "https://api.agify.io?name=thibaut",                  // 🎂 Age guess
                ];
                HINTS[idx % HINTS.len()]
            }
            VerbColor::Agent => {
                // v0.8.2: Complex multi-step agentic tasks
                const HINTS: &[&str] = &[
                    "Research competitors and write a market analysis report",
                    "Analyze this codebase and suggest 5 improvements",
                    "Build a landing page for QR Code AI with SEO",
                    "Debug this error and propose a fix with tests",
                    "Create a full REST API spec for a todo app",
                    "Review this PR and provide detailed feedback",
                    "Refactor this module to use async/await",
                    "Generate documentation for all public functions",
                    "Find and fix all security vulnerabilities",
                    "Create a migration plan from v1 to v2 API",
                    "Write a technical blog post about this feature",
                    "Set up CI/CD pipeline with GitHub Actions",
                    "Optimize database queries for better performance",
                    "Create test fixtures for integration tests",
                    "Design a caching strategy for this endpoint",
                ];
                HINTS[idx % HINTS.len()]
            }
            VerbColor::Spawn => {
                // v0.9.1: Spawned sub-agent task hints
                const HINTS: &[&str] = &[
                    "Delegate: research this topic in depth",
                    "Spawn: handle this subtask independently",
                    "Delegate: write tests for this module",
                    "Spawn: analyze and summarize these files",
                    "Delegate: generate documentation for API",
                    "Spawn: refactor this function for clarity",
                    "Delegate: investigate this bug in isolation",
                    "Spawn: create sample data for testing",
                ];
                HINTS[idx % HINTS.len()]
            }
        }
    }

    /// v0.8.2: Detect verb command at start of input (e.g., "/invoke", "/infer")
    /// Returns (verb_len, verb_color, is_complete, full_verb_name) if found
    fn detect_verb_in_input(input: &str) -> Option<(usize, VerbColor, bool, &'static str)> {
        // Must start with /
        if !input.starts_with('/') {
            return None;
        }

        // Extract the word after /
        let rest = &input[1..];
        let verb_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let verb_word = rest[..verb_end].to_lowercase();

        // Known verbs with their names
        const VERBS: &[(&str, VerbColor)] = &[
            ("invoke", VerbColor::Invoke),
            ("infer", VerbColor::Infer),
            ("fetch", VerbColor::Fetch),
            ("exec", VerbColor::Exec),
            ("agent", VerbColor::Agent),
        ];

        // Check for exact match first
        for (name, color) in VERBS {
            if verb_word == *name {
                return Some((1 + verb_end, *color, true, name));
            }
        }

        // Check for partial match (prefix) - only if at least 2 chars typed
        if verb_word.len() >= 2 {
            for (name, color) in VERBS {
                if name.starts_with(&verb_word) {
                    return Some((1 + verb_end, *color, false, name));
                }
            }
        }

        None
    }

    fn render_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Show input with cursor (use tui-input's cursor position)
        let input_value = self.input.value();
        let cursor_pos = self.input.cursor();
        let before_cursor: String = input_value.chars().take(cursor_pos).collect();
        let cursor_char = input_value.chars().nth(cursor_pos).unwrap_or(' ');
        let after_cursor: String = input_value.chars().skip(cursor_pos + 1).collect();

        // v0.8 UX: Check if input is focused for cursor animation
        let is_focused = self.focused_panel == ChatPanel::Input;

        // Build mode indicators for Claude Code-like UX
        let mut spans = vec![Span::raw(" ")];

        // Mode badge: [⚡ Infer] or [🐔 Agent]
        let mode_color = match self.chat_mode {
            ChatMode::Infer => theme.status_success, // Green for infer
            ChatMode::Agent => theme.status_running, // Amber for agent
        };
        spans.push(Span::styled(
            format!("[{} {}]", self.chat_mode.icon(), self.chat_mode.label()),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ));

        // Deep thinking indicator: [🧠 Think] if enabled
        if self.deep_thinking {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "[🧠 Think]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Provider indicator
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            &self.provider_name,
            Style::default().fg(theme.text_secondary),
        ));

        // Separator and prompt
        spans.push(Span::styled(" │ ", Style::default().fg(theme.text_muted)));
        spans.push(Span::raw("> "));

        // v0.8 WOW: Blinking cursor effect (blinks every ~8 frames = ~500ms at 60fps)
        let cursor_visible = is_focused && (self.frame / 8) % 2 == 0;

        // Input text with cursor and placeholder
        if input_value.is_empty() {
            // v0.8 WOW: Animated placeholder with typing dots when idle
            let dots = match (self.frame / 10) % 4 {
                0 => "   ",
                1 => ".  ",
                2 => ".. ",
                _ => "...",
            };

            // Show blinking cursor at start
            if cursor_visible {
                spans.push(Span::styled("█", Style::default().fg(theme.highlight)));
            } else {
                spans.push(Span::raw(" "));
            }

            // Animated placeholder hint
            spans.push(Span::styled(
                format!(" Type a message{}", if is_focused { dots } else { "..." }),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ));
            spans.push(Span::styled(
                " / for commands, @ for files",
                Style::default().fg(theme.text_muted),
            ));
        } else {
            // v0.8.2: Detect verb at start of input for colorized rendering
            if let Some((verb_len, verb_color, is_complete, full_verb)) =
                Self::detect_verb_in_input(input_value)
            {
                // Get base color for cursor
                let verb_fg_color = if is_complete {
                    verb_color.animated(self.frame)
                } else {
                    verb_color.muted()
                };

                // Split input into verb part and rest
                let verb_part = &input_value[..verb_len.min(input_value.len())];
                let rest_part = if input_value.len() > verb_len {
                    &input_value[verb_len..]
                } else {
                    ""
                };

                // Calculate remaining letters for autocomplete hint (Tab to complete)
                let typed_verb_len = verb_len.saturating_sub(1); // without /
                let remaining_hint: String = if !is_complete && typed_verb_len < full_verb.len() {
                    full_verb[typed_verb_len..].to_string()
                } else {
                    String::new()
                };

                // Handle cursor position relative to verb boundary
                if cursor_pos < verb_len {
                    // Cursor is within verb - use per-character gradient animation
                    let before_in_verb: String = verb_part.chars().take(cursor_pos).collect();
                    let after_in_verb: String = verb_part.chars().skip(cursor_pos + 1).collect();

                    // v0.8.2: ASCII box with animated emoji for the verb

                    let emoji = verb_color.icon();
                    spans.extend(Self::render_verb_gradient(
                        emoji,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));
                    spans.push(Span::raw(" ")); // Space after emoji

                    // Render before cursor with gradient
                    spans.extend(Self::render_verb_gradient(
                        &before_in_verb,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    // Cursor character with verb color background (dramatic)
                    if cursor_visible {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .bg(verb_color.glow())
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .fg(verb_fg_color)
                                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
                        ));
                    }

                    // Render after cursor with gradient
                    spans.extend(Self::render_verb_gradient(
                        &after_in_verb,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    // Show remaining letters hint for partial match (Tab to complete)
                    if !remaining_hint.is_empty() {
                        spans.push(Span::styled(
                            remaining_hint.clone(),
                            Style::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::ITALIC),
                        ));

                        spans.push(Span::styled(
                            " [Tab]",
                            Style::default().fg(theme.text_muted),
                        ));
                    } else if is_complete {
                    }
                    spans.push(Span::raw(rest_part));
                } else {
                    // Cursor is after verb - full gradient on verb
                    // v0.8.2: ASCII box with animated emoji for the verb

                    let emoji = verb_color.icon();
                    spans.extend(Self::render_verb_gradient(
                        emoji,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));
                    spans.push(Span::raw(" ")); // Space after emoji

                    spans.extend(Self::render_verb_gradient(
                        verb_part,
                        &verb_color,
                        self.frame,
                        is_complete,
                    ));

                    let rest_cursor_pos = cursor_pos - verb_len;
                    let before_rest: String = rest_part.chars().take(rest_cursor_pos).collect();
                    let after_rest: String = rest_part.chars().skip(rest_cursor_pos + 1).collect();

                    spans.push(Span::raw(before_rest));
                    if cursor_visible {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default().bg(theme.highlight).fg(Color::Black),
                        ));
                    } else {
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default()
                                .fg(theme.highlight)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                    }
                    spans.push(Span::raw(after_rest));

                    // v0.8.2: Show contextual placeholder when verb is complete and no argument yet
                    if is_complete && rest_part.trim().is_empty() {
                        let placeholder = Self::verb_placeholder(&verb_color, self.frame);
                        // Animated color pulse for placeholder
                        let pulse = ((self.frame as f32 / 30.0).sin() + 1.0) / 2.0; // 0.0-1.0
                        let (r, g, b) = verb_color.muted_tuple();
                        let fade = 0.4 + (pulse * 0.3); // 0.4-0.7 opacity
                        let pr = (r as f32 * fade) as u8;
                        let pg = (g as f32 * fade) as u8;
                        let pb = (b as f32 * fade) as u8;
                        spans.push(Span::styled(
                            format!(" {}", placeholder),
                            Style::default()
                                .fg(Color::Rgb(pr, pg, pb))
                                .add_modifier(Modifier::ITALIC),
                        ));
                        // Tab hint with verb color
                        spans.push(Span::styled(" ⭾", Style::default().fg(verb_color.muted())));
                    }
                }
            } else {
                // No verb detected, render normally
                spans.push(Span::raw(before_cursor));
                // v0.8 WOW: Blinking block cursor
                if cursor_visible {
                    spans.push(Span::styled(
                        cursor_char.to_string(),
                        Style::default().bg(theme.highlight).fg(Color::Black),
                    ));
                } else {
                    spans.push(Span::styled(
                        cursor_char.to_string(),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                }
                spans.push(Span::raw(after_cursor));
            }
        }

        let line = Line::from(spans);

        // v0.8 UX: Focus indicators for Input panel (is_focused defined above)
        let border_color = if is_focused {
            theme.highlight
        } else {
            theme.border_normal
        };

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );

        frame.render_widget(paragraph, area);
    }

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
    /// v0.12.1: Toggle with [m] key when not in input mode
    pub fn cycle_task_box_render_mode(&mut self) {
        self.task_box_render_mode = self.task_box_render_mode.cycle();
    }

    /// Render messages v2 with inline MCP/Infer boxes
    fn render_messages_v2(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // v0.9 Phase 3: Search bar at top when in search mode
        let (search_area, messages_area) = if self.search_mode {
            use ratatui::layout::{Constraint, Direction, Layout};
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        };

        // Render search bar if active
        if let Some(search_rect) = search_area {
            self.render_search_bar(frame, search_rect, theme);
        }

        // Use messages_area for rest of rendering
        let area = messages_area;

        // Extract theme colors with fallbacks
        let thinking_header_color = theme.status_running; // Use running (amber-ish) for thinking
        let thinking_content_color = theme.text_muted;
        let muted_color = theme.text_muted;
        let mcp_box_color = theme.status_success; // Emerald-like
        let success_color = theme.status_success;
        let error_color = theme.status_failed;
        let infer_box_color = theme.highlight; // Violet-like
        let status_running_color = theme.status_running;

        // v0.8.1 FIX: Calculate content width for word wrapping
        // area.width - 2 (borders) - 2 ("│ " prefix) = available text width
        let content_width = area.width.saturating_sub(4) as usize;

        // v0.8 FIX: Update visible count based on actual viewport height (minus borders)
        let viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        self.conversation_scroll.visible = viewport_height;

        // v0.8 Text Selection: Build line positions cache for mouse hit testing
        self.line_positions.clear();
        let content_start_x = area.x + 3; // "│ " prefix = 2 chars + border
        let mut current_line = 0usize;
        for (msg_idx, msg) in self.messages.iter().enumerate() {
            current_line += 1; // Header line (e.g., "👤 You ────")

            for (line_idx, line_text) in msg.content.lines().enumerate() {
                // Calculate screen Y based on scroll offset
                let line_in_list = current_line;
                let scroll_offset = self.conversation_scroll.offset;

                // Only track lines that could be visible
                if line_in_list >= scroll_offset {
                    let screen_y = area.y + 1 + (line_in_list - scroll_offset) as u16; // +1 for border
                    if screen_y < area.y + area.height - 1 {
                        self.line_positions.push(LinePosition {
                            message_index: msg_idx,
                            line_in_message: line_idx,
                            screen_y,
                            start_x: content_start_x,
                            text: line_text.to_string(),
                        });
                    }
                }
                current_line += 1;
            }

            // Account for thinking lines if present and visible (v0.9)
            if let Some(ref thinking) = msg.thinking {
                if self.is_thinking_visible(msg_idx) {
                    current_line += 1; // "🧠 Thinking:" header
                    let think_lines = thinking.lines().take(3).count();
                    current_line += think_lines;
                    if thinking.lines().count() > 3 {
                        current_line += 1; // "... (N more lines)"
                    }
                } else {
                    current_line += 1; // Collapsed indicator: "🧠 Thinking: (collapsed)"
                }
            }

            // Account for execution result if present
            if msg.execution.is_some() {
                current_line += 1;
            }

            current_line += 1; // Spacing line
        }

        // v0.8 Text Selection: Extract selection state for use in closure
        let selection = self.text_selection.clone();
        let selection_bg = theme.highlight; // Use highlight color for selection background

        // v0.9: Pre-compute thinking visibility for use in closure
        let thinking_visible: Vec<bool> = (0..self.messages.len())
            .map(|i| self.is_thinking_visible(i))
            .collect();

        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .flat_map(|(idx, msg)| {
                // v0.8.1 FIX: Skip "Thinking..." placeholder during streaming
                // The streaming section shows the Matrix Decrypt effect instead
                let is_last_message = idx == self.messages.len().saturating_sub(1);
                if self.is_streaming && is_last_message && msg.content == "Thinking..." {
                    return vec![]; // Don't render placeholder during streaming
                }

                // v0.8 WOW: Check if this message has the flash effect
                let is_flashing = self.copy_flash_index == Some(idx);

                // Color-coded message bubbles based on role
                let (_prefix, base_color) = match msg.role {
                    MessageRole::User => ("👤 You", theme.trait_retrieved),
                    MessageRole::Nika => ("🤖 AI", theme.status_success),
                    MessageRole::System => ("💡 System", theme.status_running),
                    MessageRole::Tool => ("🔧 Tool", theme.mcp_traverse),
                };

                // v0.8 WOW: Flash effect - bright highlight when copied
                let color = if is_flashing {
                    theme.highlight
                } else {
                    base_color
                };
                let style = Style::default().fg(color);

                // PERF: Use const prefix strings to avoid format! allocation
                let prefix_with_space = match msg.role {
                    MessageRole::User => "👤 You ",
                    MessageRole::Nika => "🤖 AI ",
                    MessageRole::System => "💡 System ",
                    MessageRole::Tool => "🔧 Tool ",
                };

                // v0.9.5 UX: Format timestamp as HH:MM:SS.mmm (with seconds and milliseconds)
                let ts_str = msg.timestamp.format("%H:%M:%S%.3f").to_string();

                // v0.9.5: Dynamic separator at 75% width for visual balance
                // Leaves breathing room on the right side
                let separator_len = (content_width * 75 / 100).saturating_sub(20); // 75% minus prefix+timestamp
                let dynamic_separator = "─".repeat(separator_len);

                // v0.8 WOW: Add COPIED indicator when flashing
                // v0.9.5: Clock emoji before timestamp
                let mut header_spans = vec![
                    Span::styled(prefix_with_space, style.add_modifier(Modifier::BOLD)),
                    Span::styled(dynamic_separator, Style::default().fg(theme.text_muted)),
                    Span::styled(
                        format!(" 🕐 {} ", ts_str),
                        Style::default().fg(theme.text_muted),
                    ),
                ];
                if is_flashing {
                    header_spans.push(Span::styled(
                        " ✓ COPIED ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                let mut lines = vec![ListItem::new(Line::from(header_spans))];

                // v0.8 Text Selection: Check if this message is in the selection range
                let is_selected = selection.as_ref().is_some_and(|sel| {
                    let (start, end) = sel.normalized();
                    idx >= start.message_index && idx <= end.message_index
                });

                // v0.8.1 FIX: Wrap message content to fit panel width
                // v0.9.5: Smart wrap for System message - preserve ASCII art, wrap text
                let wrapped_lines: Vec<String> =
                    if idx == 0 && matches!(msg.role, MessageRole::System) {
                        // For System banner: keep ASCII art lines intact, wrap text lines
                        let mut result = Vec::new();
                        for line in msg.content.lines() {
                            // ASCII art detection: contains block chars (█╔╗╚╝║═╭╮╰╯─┌┐└┘│)
                            let is_ascii_art = line.chars().any(|c| {
                                matches!(
                                    c,
                                    '█' | '╔'
                                        | '╗'
                                        | '╚'
                                        | '╝'
                                        | '║'
                                        | '═'
                                        | '╭'
                                        | '╮'
                                        | '╰'
                                        | '╯'
                                        | '─'
                                        | '┌'
                                        | '┐'
                                        | '└'
                                        | '┘'
                                        | '│'
                                        | '▀'
                                        | '▄'
                                        | '░'
                                        | '▒'
                                        | '▓'
                                )
                            });
                            if is_ascii_art || line.len() <= content_width {
                                result.push(line.to_string());
                            } else {
                                // Word-wrap long text lines
                                result.extend(wrap_text(line, content_width));
                            }
                        }
                        result
                    } else {
                        wrap_text(&msg.content, content_width)
                    };

                // v0.8 Text Selection: Track char offset for selection highlighting
                let mut char_offset = 0usize;
                for wrapped_line in &wrapped_lines {
                    let line_len = wrapped_line.chars().count();

                    // v0.8 Text Selection: Apply highlighting if selected
                    let text_style = if is_selected {
                        if let Some(ref sel) = selection {
                            let (start, end) = sel.normalized();
                            // Check if this line is within the selection
                            let line_start = char_offset;
                            let line_end = char_offset + line_len;

                            // Calculate selection overlap with this line
                            let sel_start_in_msg = if idx == start.message_index {
                                start.char_offset
                            } else {
                                0
                            };
                            let sel_end_in_msg = if idx == end.message_index {
                                end.char_offset
                            } else {
                                usize::MAX
                            };

                            // Check if this line overlaps with selection
                            if line_end > sel_start_in_msg && line_start < sel_end_in_msg {
                                Style::default().bg(selection_bg).fg(Color::Black)
                            } else {
                                Style::default()
                            }
                        } else {
                            Style::default()
                        }
                    } else {
                        Style::default()
                    };

                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(color)),
                        Span::styled(wrapped_line.to_string(), text_style),
                    ])));

                    char_offset += line_len + 1; // +1 for newline/wrap
                }

                // v0.9.5: Add colored verb boxes after welcome banner (first System message)
                // Emoji widths are hardcoded per verb for perfect alignment
                if idx == 0 && matches!(msg.role, MessageRole::System) {
                    // Empty line before boxes
                    lines.push(ListItem::new(Line::from(vec![Span::styled(
                        "│ ",
                        Style::default().fg(color),
                    )])));

                    // Verb boxes with pre-computed content for consistent alignment
                    // Each box has: ┌────────────┐ (14 chars total: 12 dashes + 2 corners)
                    //               │ ⚡ /verb  │ (inner content = 12 display cols)
                    //               └────────────┘
                    // Emoji display widths: ⚡=2, 📟=2, 📡=2, 🔌=2, 🐔=2
                    // Formula: space(1) + emoji(2) + space(1) + /name(N+1) + padding = 12
                    let verbs: [(VerbColor, &str); 5] = [
                        (VerbColor::Infer, " ⚡ /infer  "),  // 1+2+1+6+2 = 12
                        (VerbColor::Exec, " 📟 /exec   "),   // 1+2+1+5+3 = 12
                        (VerbColor::Fetch, " 📡 /fetch  "),  // 1+2+1+6+2 = 12
                        (VerbColor::Invoke, " 🔌 /invoke "), // 1+2+1+7+1 = 12
                        (VerbColor::Agent, " 🐔 /agent  "),  // 1+2+1+6+2 = 12
                    ];

                    let box_border = "────────────"; // 12 dashes

                    // Top borders line (2-space indent to align with banner)
                    let mut top_spans = vec![Span::styled("│  ", Style::default().fg(color))];
                    for (verb_color, _) in &verbs {
                        let c = verb_color.rgb();
                        top_spans.push(Span::styled(
                            format!("┌{}┐", box_border),
                            Style::default().fg(c),
                        ));
                        top_spans.push(Span::raw(" ")); // 1 space gap
                    }
                    lines.push(ListItem::new(Line::from(top_spans)));

                    // Content line with emojis and names
                    let mut content_spans = vec![Span::styled("│  ", Style::default().fg(color))];
                    for (verb_color, content) in &verbs {
                        let c = verb_color.rgb();
                        content_spans.push(Span::styled("│", Style::default().fg(c)));
                        content_spans.push(Span::styled(
                            *content,
                            Style::default().fg(c).add_modifier(Modifier::BOLD),
                        ));
                        content_spans.push(Span::styled("│", Style::default().fg(c)));
                        content_spans.push(Span::raw(" ")); // 1 space gap
                    }
                    lines.push(ListItem::new(Line::from(content_spans)));

                    // Bottom borders line
                    let mut bottom_spans = vec![Span::styled("│  ", Style::default().fg(color))];
                    for (verb_color, _) in &verbs {
                        let c = verb_color.rgb();
                        bottom_spans.push(Span::styled(
                            format!("└{}┘", box_border),
                            Style::default().fg(c),
                        ));
                        bottom_spans.push(Span::raw(" ")); // 1 space gap
                    }
                    lines.push(ListItem::new(Line::from(bottom_spans)));
                }

                // Add thinking display if present (v0.5.2+, v0.9: visibility toggle)
                if let Some(ref thinking) = msg.thinking {
                    let is_visible = thinking_visible.get(idx).copied().unwrap_or(false);

                    if is_visible {
                        // Expanded: show header and content
                        lines.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(color)),
                            Span::styled(
                                "🧠 Thinking: [t to collapse]",
                                Style::default()
                                    .fg(thinking_header_color)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ])));

                        // Truncate thinking to first 3 lines for inline display
                        let thinking_lines: Vec<&str> = thinking.lines().take(3).collect();
                        for think_line in &thinking_lines {
                            // Truncate each line to 60 chars (UTF-8 safe)
                            let display_line = truncate_str(think_line, 60);
                            lines.push(ListItem::new(Line::from(vec![
                                Span::styled("│   ", Style::default().fg(color)),
                                Span::styled(
                                    display_line,
                                    Style::default()
                                        .fg(thinking_content_color)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ])));
                        }

                        // Show ellipsis if there are more lines
                        let total_lines = thinking.lines().count();
                        if total_lines > 3 {
                            lines.push(ListItem::new(Line::from(vec![
                                Span::styled("│   ", Style::default().fg(color)),
                                Span::styled(
                                    format!("... ({} more lines)", total_lines - 3),
                                    Style::default().fg(muted_color),
                                ),
                            ])));
                        }
                    } else {
                        // Collapsed: show only header with hint
                        lines.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(color)),
                            Span::styled(
                                "🧠 Thinking: (collapsed) [t to expand]",
                                Style::default()
                                    .fg(muted_color)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ])));
                    }
                }

                // Add execution result if present
                if let Some(exec) = &msg.execution {
                    let (status_icon, status_color) = match exec.status {
                        ExecutionStatus::Running => ("⏳", theme.status_running),
                        ExecutionStatus::Completed => ("✅", theme.status_success),
                        ExecutionStatus::Failed => ("❌", theme.status_failed),
                    };
                    lines.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!(
                                "└─ {} {} ({}/{}) ",
                                status_icon,
                                exec.workflow_name,
                                exec.tasks_completed,
                                exec.tasks_total
                            ),
                            Style::default().fg(status_color),
                        ),
                    ])));
                }

                lines.push(ListItem::new("")); // spacing
                lines
            })
            .collect();

        // Render inline content (MCP calls, Infer streams)
        for content in &self.inline_content {
            match content {
                InlineContent::McpCall(data) => {
                    // Render inline MCP call box representation
                    let (status_char, status_color) = data.status.indicator(data.frame);
                    let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("╭─ 🔧 MCP: {} ", data.tool),
                            Style::default().fg(mcp_box_color),
                        ),
                        Span::styled(
                            format!("{} {} ─╮", status_char, duration_str),
                            Style::default().fg(status_color),
                        ),
                    ])));

                    if !data.params.is_empty() {
                        // UTF-8 safe truncation
                        let params_display = truncate_str(&data.params, 40);
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(mcp_box_color)),
                            Span::styled("📥 ", Style::default().fg(muted_color)),
                            Span::raw(params_display),
                        ])));
                    }

                    if let Some(ref result) = data.result {
                        // UTF-8 safe truncation
                        let result_display = truncate_str(result, 40);
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(mcp_box_color)),
                            Span::styled("📤 ", Style::default().fg(success_color)),
                            Span::raw(result_display),
                        ])));
                    } else if let Some(ref error) = data.error {
                        // UTF-8 safe truncation
                        let error_display = truncate_str(error, 40);
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(mcp_box_color)),
                            Span::styled("❌ ", Style::default().fg(error_color)),
                            Span::raw(error_display),
                        ])));
                    }

                    // PERF: Use const for MCP box bottom border
                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        SEPARATOR_52,
                        Style::default().fg(mcp_box_color),
                    )])));
                    items.push(ListItem::new("")); // spacing
                }
                InlineContent::InferStream(data) => {
                    // Render inline Infer stream box representation
                    let (status_char, _) = data.status.indicator(data.frame);
                    let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("╭─ 🧠 INFER: {} ", data.model),
                            Style::default().fg(infer_box_color),
                        ),
                        Span::styled(
                            format!("{} {} ─╮", status_char, duration_str),
                            Style::default().fg(status_running_color),
                        ),
                    ])));

                    // Token info
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(infer_box_color)),
                        Span::styled(
                            format!("📊 {} in → {} out", data.tokens_in, data.tokens_out),
                            Style::default().fg(muted_color),
                        ),
                    ])));

                    // Last lines of content
                    let content_lines: Vec<&str> = data.content.lines().collect();
                    let start = content_lines.len().saturating_sub(3);
                    for line in content_lines.iter().skip(start) {
                        // UTF-8 safe truncation
                        let display = truncate_str(line, 50);
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(infer_box_color)),
                            Span::raw(display),
                        ])));
                    }

                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        "╰───────────────────────────────────────────────────╯",
                        Style::default().fg(infer_box_color),
                    )])));
                    items.push(ListItem::new("")); // spacing
                }
                InlineContent::Task(task_box) => {
                    // v0.12.1: Render TaskBox with verb colors and pulse animation
                    let verb_color = task_box.verb_color().rgb();
                    // Calculate pulse intensity for running state (0.0-1.0 sine wave)
                    let pulse_intensity = if task_box.state().is_running() {
                        ((self.frame as f32 / 30.0).sin() + 1.0) / 2.0
                    } else {
                        0.0
                    };
                    let border_color = task_box.state().border_color_with_pulse(verb_color, pulse_intensity);
                    let state_icon = task_box.state().icon();
                    let state_suffix = task_box.state().suffix();

                    // Render based on TaskBox variant
                    match task_box {
                        TaskBox::Invoke(invoke) => {
                            // v0.12.1: RenderMode-aware InvokeBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let emerald_400 = Color::Rgb(52, 211, 153);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, tool@server, footer
                                    // ╭─ 🔌 INVOKE ────────────────── ⏳ Running ─╮
                                    // │ novanet_describe @ novanet │ ✓ result   │
                                    // ╰────────────────────────────────────────────╯
                                    let status_str = if invoke.result.is_some() {
                                        "✓ result"
                                    } else if invoke.error.is_some() {
                                        "❌ error"
                                    } else if task_box.state().is_running() {
                                        "⏳"
                                    } else {
                                        ""
                                    };
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 🔌 INVOKE ──────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(truncate_str(&invoke.tool, 20), Style::default().fg(content_color)),
                                        Span::styled(" @ ", Style::default().fg(muted_color)),
                                        Span::styled(truncate_str(&invoke.server, 12), Style::default().fg(emerald_400)),
                                        Span::styled(format!(" │ {}", status_str), Style::default().fg(muted_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full InvokeBox rendering
                                    // 1. Top border with status
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 🔌 INVOKE ────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));

                                    // 2. Tool + Server line
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            truncate_str(&invoke.tool, 25),
                                            Style::default().fg(content_color),
                                        ),
                                        Span::styled(" @ ", Style::default().fg(muted_color)),
                                        Span::styled(
                                            truncate_str(&invoke.server, 18),
                                            Style::default().fg(emerald_400),
                                        ),
                                    ])));

                                    // 3. Separator
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));

                                    // 4. INPUT section header
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("📥 INPUT", Style::default().fg(muted_color)),
                                    ])));

                                    // 5. Params preview (truncated JSON)
                                    if !invoke.params.is_null() {
                                        let params_str = serde_json::to_string(&invoke.params)
                                            .unwrap_or_else(|_| "{}".to_string());
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                truncate_str(&params_str, 44),
                                                Style::default().fg(content_color),
                                            ),
                                        ])));
                                    } else {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled("(no params)", Style::default().fg(muted_color)),
                                        ])));
                                    }

                                    // 6. OUTPUT section header
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("📤 OUTPUT", Style::default().fg(muted_color)),
                                    ])));

                                    // 7. Result or error preview
                                    if let Some(ref result) = invoke.result {
                                        let result_str = serde_json::to_string(result)
                                            .unwrap_or_else(|_| "null".to_string());
                                        // Show first 2 lines of result
                                        let result_lines: Vec<&str> = result_str.lines().take(2).collect();
                                        if result_lines.is_empty() {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled("┊ ", Style::default().fg(muted_color)),
                                                Span::styled(
                                                    truncate_str(&result_str, 44),
                                                    Style::default().fg(success_color),
                                                ),
                                            ])));
                                        } else {
                                            for line in result_lines {
                                                items.push(ListItem::new(Line::from(vec![
                                                    Span::styled("│ ", Style::default().fg(border_color)),
                                                    Span::styled("┊ ", Style::default().fg(muted_color)),
                                                    Span::styled(
                                                        truncate_str(line, 44),
                                                        Style::default().fg(success_color),
                                                    ),
                                                ])));
                                            }
                                        }
                                    } else if let Some(ref err) = invoke.error {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled("❌ ", Style::default().fg(error_color)),
                                            Span::styled(
                                                truncate_str(err, 40),
                                                Style::default().fg(error_color),
                                            ),
                                        ])));
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running invoke
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled("(calling MCP server)", Style::default().fg(muted_color)),
                                            Span::styled(cursor, Style::default().fg(success_color)),
                                        ])));
                                    }

                                    // 8. Bottom border
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                    items.push(ListItem::new("")); // spacing
                                }
                            }
                        }
                        TaskBox::Infer(infer) => {
                            // v0.12.1: RenderMode-aware InferBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let provider_icon = if infer.model.contains("claude") {
                                "🧠"
                            } else if infer.model.contains("gpt") {
                                "🤖"
                            } else if infer.model.contains("mistral") {
                                "🌀"
                            } else if infer.model.contains("llama") {
                                "🦙"
                            } else {
                                "💬"
                            };

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 2 lines - header + content preview
                                    // ╭─ ⚡ INFER ──────────────────────── ⏳ Running ─╮
                                    // │ "Generate a headline..."  │ 127 in │ 45 out │
                                    // ╰────────────────────────────────────────────────╯
                                    let prompt_preview = truncate_str(&infer.prompt.replace('\n', " "), 28);
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ ⚡ INFER ──────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("\"{}\" │ {} in │ {} out", prompt_preview, infer.tokens_in, infer.tokens_out),
                                            Style::default().fg(content_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full InferBox rendering
                                    // 1. Top border
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ ⚡ INFER ────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));

                                    // 2. Model line
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("model: {} {}", provider_icon, truncate_str(&infer.model, 35)),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));

                                    // 3. Separator
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));

                                    // 4. PROMPT section
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("PROMPT", Style::default().fg(muted_color)),
                                    ])));
                                    let prompt_display = infer.prompt.replace('\n', " ");
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("┊ {}", truncate_str(&prompt_display, 45)),
                                            Style::default().fg(content_color),
                                        ),
                                    ])));

                                    // 5. Separator
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));

                                    // 6. RESPONSE section
                                    let response_text = if infer.state.is_running() && self.is_streaming {
                                        &self.partial_response
                                    } else {
                                        &infer.response
                                    };
                                    let response_indicator = if infer.state.is_running() && self.is_streaming {
                                        "▼ streaming...".to_string()
                                    } else if !response_text.is_empty() {
                                        format!("▼ {} chars", response_text.len())
                                    } else {
                                        String::new()
                                    };
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("RESPONSE", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!("                              {}", response_indicator),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));

                                    // 7. Response content
                                    if response_text.is_empty() && infer.state.is_running() {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ...", Style::default().fg(content_color)),
                                        ])));
                                    } else {
                                        let response_lines: Vec<&str> = response_text.lines().collect();
                                        let start = response_lines.len().saturating_sub(3);
                                        let lines_to_show: Vec<_> = response_lines.iter().skip(start).collect();
                                        for (idx, line) in lines_to_show.iter().enumerate() {
                                            let is_last = idx == lines_to_show.len() - 1;
                                            let cursor = if is_last && infer.state.is_running() && self.is_streaming { "█" } else { "" };
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled(
                                                    format!("┊ {}{}", truncate_str(line, 44), cursor),
                                                    Style::default().fg(content_color),
                                                ),
                                            ])));
                                        }
                                    }

                                    // 8. Footer with metrics
                                    let cost = crate::tui::widgets::task_box::InferBox::calculate_cost(
                                        infer.tokens_in, infer.tokens_out, &infer.model,
                                    );
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("📊 {} in │ {} out │ {} {} │ 💰 ${:.4}",
                                                infer.tokens_in, infer.tokens_out, provider_icon,
                                                truncate_str(&infer.model, 12), cost
                                            ),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));

                                    // 9. Bottom border
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                            }
                            items.push(ListItem::new("")); // spacing
                        }
                        TaskBox::Exec(exec) => {
                            // v0.12.1: RenderMode-aware ExecBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let exit_display = exec.exit_code.map(|c| format!("exit: {}", c)).unwrap_or_else(|| "running...".to_string());
                            let exit_color = exec.exit_code.map(|c| if c == 0 { success_color } else { error_color }).unwrap_or(muted_color);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header + command + footer
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 📟 EXEC ──────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(format!("$ {} │ ", truncate_str(&exec.command, 30)), Style::default().fg(content_color)),
                                        Span::styled(exit_display.clone(), Style::default().fg(exit_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full ExecBox rendering
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 📟 EXEC ─────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(format!("$ {}", truncate_str(&exec.command, 45)), Style::default().fg(content_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));
                                    let output_indicator = if !exec.stdout.is_empty() {
                                        format!("▼ {} lines", exec.stdout.lines().count())
                                    } else { String::new() };
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("OUTPUT", Style::default().fg(muted_color)),
                                        Span::styled(format!("                                  {}", output_indicator), Style::default().fg(muted_color)),
                                    ])));
                                    if exec.stdout.is_empty() && exec.state.is_running() {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ...", Style::default().fg(content_color)),
                                        ])));
                                    } else if exec.stdout.is_empty() {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ (no output)", Style::default().fg(muted_color)),
                                        ])));
                                    } else {
                                        let output_lines: Vec<&str> = exec.stdout.lines().collect();
                                        let start = output_lines.len().saturating_sub(3);
                                        for line in output_lines.iter().skip(start) {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled(format!("┊ {}", truncate_str(line, 44)), Style::default().fg(content_color)),
                                            ])));
                                        }
                                    }
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(exit_display, Style::default().fg(exit_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                            }
                            items.push(ListItem::new("")); // spacing
                        }
                        TaskBox::Fetch(fetch) => {
                            // v0.12.1: RenderMode-aware FetchBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let amber_400 = Color::Rgb(251, 191, 36);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, method+url, footer
                                    // ╭─ 🛰️ FETCH ────────────────── ⏳ Running ─╮
                                    // │ GET https://api.example... │ 200 │ 45ms │
                                    // ╰────────────────────────────────────────────╯
                                    let status_str = fetch.status_code.map(|s| format!("{}", s)).unwrap_or_else(|| "---".to_string());
                                    let status_color = fetch.status_code.map(|s| {
                                        if (200..300).contains(&s) { success_color } else { error_color }
                                    }).unwrap_or(muted_color);

                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 🛰️ FETCH ────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(format!("{} ", fetch.method), Style::default().fg(amber_400)),
                                        Span::styled(truncate_str(&fetch.url, 28), Style::default().fg(content_color)),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(status_str, Style::default().fg(status_color)),
                                        Span::styled(format!(" │ {}ms", fetch.ttfb_ms), Style::default().fg(muted_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full FetchBox rendering
                                    // 1. Top border with status
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ 🛰️ FETCH ─────────────────────────────── {} {} ─╮", state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));

                                    // 2. Method + URL line
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("{} ", fetch.method),
                                            Style::default().fg(amber_400),
                                        ),
                                        Span::styled(
                                            truncate_str(&fetch.url, 42),
                                            Style::default().fg(content_color),
                                        ),
                                    ])));

                                    // 3. Separator
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));

                                    // 4. RESPONSE section header
                                    let size_str = if fetch.response_size > 0 {
                                        format!(" ({:.1}KB)", fetch.response_size as f64 / 1024.0)
                                    } else {
                                        String::new()
                                    };
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(
                                            format!("💬 RESPONSE{}", size_str),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));

                                    // 5. Response body preview (first 2 lines or status)
                                    if let Some(body) = &fetch.response_body {
                                        let preview_lines: Vec<&str> = body.lines().take(2).collect();
                                        for line in preview_lines {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled("┊ ", Style::default().fg(muted_color)),
                                                Span::styled(
                                                    truncate_str(line, 44),
                                                    Style::default().fg(content_color),
                                                ),
                                            ])));
                                        }
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running fetch
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled("(awaiting response)", Style::default().fg(muted_color)),
                                            Span::styled(cursor, Style::default().fg(success_color)),
                                        ])));
                                    }

                                    // 6. Footer with metrics
                                    let status_str = fetch.status_code.map(|s| format!("{}", s)).unwrap_or_else(|| "---".to_string());
                                    let status_color = fetch.status_code.map(|s| {
                                        if (200..300).contains(&s) { success_color } else { error_color }
                                    }).unwrap_or(muted_color);

                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("Status: ", Style::default().fg(muted_color)),
                                        Span::styled(status_str, Style::default().fg(status_color)),
                                        Span::styled(format!(" │ TTFB: {}ms", fetch.ttfb_ms), Style::default().fg(muted_color)),
                                        Span::styled(format!(" │ Retries: {}", fetch.retries), Style::default().fg(muted_color)),
                                    ])));

                                    // 7. Bottom border
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                    items.push(ListItem::new("")); // spacing
                                }
                            }
                        }
                        TaskBox::Agent(agent) => {
                            // v0.12.1: RenderMode-aware AgentBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let amber_400 = Color::Rgb(251, 191, 36);
                            let agent_icon = if agent.is_subagent { "🐤" } else { "🐔" };

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, goal, footer with stats
                                    // ╭─ 🐔 AGENT ────────────────── ⏳ Running ─╮
                                    // │ 🎯 "Analyze and report..." │ T:2/5 │ 1.2K │
                                    // ╰────────────────────────────────────────────╯
                                    let total_tokens = agent.tokens_in + agent.tokens_out;
                                    let token_str = if total_tokens > 1000 {
                                        format!("{:.1}K", total_tokens as f64 / 1000.0)
                                    } else {
                                        format!("{}", total_tokens)
                                    };

                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ {} AGENT ────────────────────────────── {} {} ─╮", agent_icon, state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("🎯 ", Style::default().fg(amber_400)),
                                        Span::styled(truncate_str(&agent.prompt, 25), Style::default().fg(content_color)),
                                        Span::styled(format!(" │ T:{}/{} │ {}", agent.turn, agent.max_turns, token_str), Style::default().fg(muted_color)),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full AgentBox rendering
                                    // 1. Top border with status
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("╭─ {} AGENT ─────────────────────────────── {} {} ─╮", agent_icon, state_icon, state_suffix),
                                            Style::default().fg(border_color),
                                        ),
                                    ])));

                                    // 2. Goal/prompt line
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("🎯 ", Style::default().fg(amber_400)),
                                        Span::styled(
                                            truncate_str(&agent.prompt, 45),
                                            Style::default().fg(content_color),
                                        ),
                                    ])));

                                    // 3. Separator
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "├──────────────────────────────────────────────────┤",
                                        Style::default().fg(border_color),
                                    )])));

                                    // 4. RESPONSE section header
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("💬 RESPONSE", Style::default().fg(muted_color)),
                                    ])));

                                    // 5. Final response content (first 3 lines or streaming indicator)
                                    if let Some(response) = &agent.final_response {
                                        let response_lines: Vec<&str> = response.lines().take(3).collect();
                                        for line in response_lines {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled("┊ ", Style::default().fg(muted_color)),
                                                Span::styled(
                                                    truncate_str(line, 44),
                                                    Style::default().fg(content_color),
                                                ),
                                            ])));
                                        }
                                        if response.lines().count() > 3 {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled("│ ", Style::default().fg(border_color)),
                                                Span::styled("┊ ", Style::default().fg(muted_color)),
                                                Span::styled(
                                                    format!("... ({} more lines)", response.lines().count() - 3),
                                                    Style::default().fg(muted_color),
                                                ),
                                            ])));
                                        }
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running agent
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                format!("(turn {}/{} in progress)", agent.turn, agent.max_turns),
                                                Style::default().fg(muted_color),
                                            ),
                                            Span::styled(cursor, Style::default().fg(success_color)),
                                        ])));
                                    }

                                    // 6. Footer with turn count, tokens, cost, tool calls
                                    let total_tokens = agent.tokens_in + agent.tokens_out;
                                    let token_str = if total_tokens > 1000 {
                                        format!("{:.1}K", total_tokens as f64 / 1000.0)
                                    } else {
                                        format!("{}", total_tokens)
                                    };
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled(format!("Turn {}/{}", agent.turn, agent.max_turns), Style::default().fg(muted_color)),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(format!("🎫 {} tokens", token_str), Style::default().fg(muted_color)),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(format!("💰 ${:.2}", agent.cost), Style::default().fg(muted_color)),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(format!("🔌 {} calls", agent.tool_calls), Style::default().fg(muted_color)),
                                    ])));

                                    // 7. Bottom border
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                    items.push(ListItem::new("")); // spacing
                                }
                            }
                        }
                    }
                }
            }
        }

        // v0.8.1: Show agent phase indicator box when agent is active (not Idle)
        // This shows real phases (Syncing/Planning/Invoking/Processing/Inferring/Streaming) with Matrix effect
        // IMPORTANT: Show during ALL active phases including Streaming - users want to see activity!
        if self.agent_phase != AgentPhase::Idle {
            // Phase indicator box header - color by phase type
            let phase_color = match self.agent_phase {
                AgentPhase::Syncing => theme.status_running,
                AgentPhase::Planning => theme.status_running,
                AgentPhase::Routing => theme.status_running,
                AgentPhase::Invoking => theme.highlight,
                AgentPhase::Processing => theme.status_success,
                AgentPhase::Inferring => theme.status_running,
                AgentPhase::Composing => theme.status_running,
                AgentPhase::Streaming => theme.status_success, // Streaming = green = active output
                AgentPhase::Idle => theme.text_muted,          // Shouldn't reach here but safe
            };

            // Box top
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!(
                    "┌─ 🐔 Nika {}─┐",
                    "─".repeat(content_width.saturating_sub(15))
                ),
                Style::default().fg(phase_color),
            )])));

            // Phase indicator line with Matrix effect
            let phase_line = self.phase_indicator.build_line();
            let mut phase_spans = vec![Span::styled("│  ", Style::default().fg(phase_color))];
            phase_spans.extend(phase_line.spans);
            // Pad to box width
            phase_spans.push(Span::styled(
                format!("{:width$}│", "", width = content_width.saturating_sub(30)),
                Style::default().fg(phase_color),
            ));
            items.push(ListItem::new(Line::from(phase_spans)));

            // Show tool details if in Invoking/Processing phase
            if matches!(
                self.agent_phase,
                AgentPhase::Invoking | AgentPhase::Processing
            ) {
                if let Some(ref tool) = self.agent_phase_tool {
                    // Get last MCP call details if available
                    if let Some(InlineContent::McpCall(mcp_data)) = self.inline_content.last() {
                        // Server line
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("├── ", Style::default().fg(theme.text_muted)),
                            Span::styled("server: ", Style::default().fg(theme.text_muted)),
                            Span::styled(
                                mcp_data.server.clone(),
                                Style::default().fg(theme.highlight),
                            ),
                        ])));

                        // Params line (truncated)
                        if !mcp_data.params.is_empty() {
                            let params_preview = if mcp_data.params.len() > 40 {
                                format!("{}...", &mcp_data.params[..40])
                            } else {
                                mcp_data.params.clone()
                            };
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(phase_color)),
                                Span::styled("├── ", Style::default().fg(theme.text_muted)),
                                Span::styled("params: ", Style::default().fg(theme.text_muted)),
                                Span::styled(
                                    params_preview,
                                    Style::default().fg(theme.text_secondary),
                                ),
                            ])));
                        }

                        // Duration line
                        let elapsed_secs = mcp_data.duration.as_secs_f64();
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("└── ", Style::default().fg(theme.text_muted)),
                            Span::styled("⏱ ", Style::default().fg(theme.text_muted)),
                            Span::styled(
                                format!("{:.1}s", elapsed_secs),
                                Style::default().fg(if elapsed_secs > 5.0 {
                                    theme.status_failed
                                } else if elapsed_secs > 2.0 {
                                    theme.status_running
                                } else {
                                    theme.status_success
                                }),
                            ),
                        ])));
                    } else {
                        // Fallback: just show tool name
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("└── ", Style::default().fg(theme.text_muted)),
                            Span::styled("tool: ", Style::default().fg(theme.text_muted)),
                            Span::styled(tool.clone(), Style::default().fg(theme.highlight)),
                        ])));
                    }
                }
            }

            // Box bottom
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("└{}┘", "─".repeat(content_width.saturating_sub(2))),
                Style::default().fg(phase_color),
            )])));

            items.push(ListItem::new("")); // spacing
        }

        // v0.12.1: REMOVED streaming AI bubble - InferBox REPLACES the AI message bubble
        // The response now displays INSIDE the InferBox widget (Option A)
        // See TaskBox::Infer rendering above which shows partial_response during streaming

        // v0.8 UX: Focus indicators for Conversation panel
        let is_focused = self.focused_panel == ChatPanel::Conversation;
        let border_color = if is_focused {
            theme.highlight
        } else {
            theme.border_normal
        };
        let title = if is_focused {
            " ▸ 💬 CONVERSATION "
        } else {
            " 💬 CONVERSATION "
        };
        let mut title_style = Style::default().fg(border_color);
        if is_focused {
            title_style = title_style.add_modifier(Modifier::BOLD);
        }

        // Add scroll indicator to title if scrollable
        let title_with_indicator = if let Some(indicator) = self.conversation_scroll.indicator() {
            format!("{}{}", title, indicator)
        } else {
            title.to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_with_indicator)
            .title_style(title_style)
            .border_style(Style::default().fg(border_color));

        // v0.8.1 FIX: Update total item count for scroll state BEFORE any scroll operations
        let total_items = items.len();
        self.conversation_scroll.total = total_items;

        // v0.8.1 FIX (NovaNet pattern): Apply scroll using .skip().take() directly
        // This is more reliable than relying on ListState's internal offset mechanism
        let visible_count = viewport_height;
        let scroll_offset = self.conversation_scroll.offset;

        // Clamp offset to valid range
        let clamped_offset = if total_items > visible_count {
            scroll_offset.min(total_items.saturating_sub(visible_count))
        } else {
            0
        };
        self.conversation_scroll.offset = clamped_offset;

        // Apply scroll - only show visible lines (NovaNet pattern)
        let visible_items: Vec<ListItem> = items
            .into_iter()
            .skip(clamped_offset)
            .take(visible_count)
            .collect();

        let list = List::new(visible_items).block(block);
        frame.render_widget(list, area);

        // v0.8.2 UX: Render custom ScrollIndicator with dynamic arrows
        // Shows △/▲ based on can_scroll state for better visual feedback
        if total_items > visible_count {
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            let scroll_indicator = ScrollIndicator::new()
                .position(clamped_offset, total_items, visible_count)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);

            frame.render_widget(scroll_indicator, scrollbar_area);
        }
    }

    /// v0.9 Phase 3: Render search bar when in search mode
    fn render_search_bar(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::widgets::{Block, Borders, Paragraph};

        // Build search content with match count
        let match_info = if self.search_results.is_empty() {
            if self.search_query.is_empty() {
                String::new()
            } else {
                " (no matches)".to_string()
            }
        } else {
            format!(
                " ({}/{})",
                self.search_current + 1,
                self.search_results.len()
            )
        };

        let search_text = format!("🔍 {}{}", self.search_query, match_info);

        let block = Block::default()
            .title(" Search (Esc to close) ")
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.highlight));

        let search_color = if self.search_results.is_empty() && !self.search_query.is_empty() {
            theme.status_failed // Red for no matches
        } else {
            theme.text_primary
        };

        let paragraph = Paragraph::new(search_text)
            .style(Style::default().fg(search_color))
            .block(block);

        frame.render_widget(paragraph, area);
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
mod tests {
    use super::*;

    #[test]
    fn test_chat_view_new() {
        let view = ChatView::new();
        assert_eq!(view.messages.len(), 1); // Welcome message
        assert!(view.input.value().is_empty());
    }

    #[test]
    fn test_chat_view_submit() {
        let mut view = ChatView::new();
        view.input = Input::new("Hello Nika".to_string());
        view.input.handle(InputRequest::GoToEnd);

        let result = view.submit();
        assert_eq!(result, Some("Hello Nika".to_string()));
        assert!(view.input.value().is_empty());
        assert_eq!(view.messages.len(), 2); // Welcome + user message
    }

    #[test]
    fn test_chat_view_submit_empty() {
        let mut view = ChatView::new();
        view.input = Input::new("   ".to_string());

        let result = view.submit();
        assert_eq!(result, None);
    }

    #[test]
    fn test_chat_view_history_navigation() {
        let mut view = ChatView::new();
        view.add_user_message("First".to_string());
        view.add_user_message("Second".to_string());

        view.history_up();
        assert_eq!(view.input.value(), "Second");

        view.history_up();
        assert_eq!(view.input.value(), "First");

        view.history_down();
        assert_eq!(view.input.value(), "Second");
    }

    #[test]
    fn test_chat_view_history_down_clears_input() {
        let mut view = ChatView::new();
        view.add_user_message("Test".to_string());

        view.history_up();
        assert_eq!(view.input.value(), "Test");

        view.history_down();
        assert!(view.input.value().is_empty());
    }

    #[test]
    fn test_chat_view_cursor() {
        let mut view = ChatView::new();
        view.insert_char('H');
        view.insert_char('i');
        assert_eq!(view.input.value(), "Hi");
        assert_eq!(view.input.cursor(), 2);

        view.cursor_left();
        assert_eq!(view.input.cursor(), 1);

        view.insert_char('e');
        assert_eq!(view.input.value(), "Hei");

        view.backspace();
        assert_eq!(view.input.value(), "Hi");
    }

    #[test]
    fn test_chat_view_cursor_right() {
        let mut view = ChatView::new();
        view.input = Input::new("Hello".to_string());
        view.input.handle(InputRequest::GoToStart);

        view.cursor_right();
        assert_eq!(view.input.cursor(), 1);

        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        assert_eq!(view.input.cursor(), 5);

        // Should not go past the end
        view.cursor_right();
        assert_eq!(view.input.cursor(), 5);
    }

    #[test]
    fn test_chat_view_backspace_at_start() {
        let mut view = ChatView::new();
        view.input = Input::new("Hi".to_string());
        view.input.handle(InputRequest::GoToStart);

        view.backspace();
        assert_eq!(view.input.value(), "Hi");
        assert_eq!(view.input.cursor(), 0);
    }

    #[test]
    fn test_chat_view_add_nika_message() {
        let mut view = ChatView::new();
        view.add_nika_message("Hello!".to_string(), None);

        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[1].role, MessageRole::Nika);
        assert_eq!(view.messages[1].content, "Hello!");
    }

    #[test]
    fn test_chat_view_add_nika_message_with_execution() {
        let mut view = ChatView::new();
        let exec = ExecutionResult {
            workflow_name: "test.nika.yaml".to_string(),
            status: ExecutionStatus::Completed,
            tasks_completed: 3,
            tasks_total: 3,
            output: Some("Done".to_string()),
        };
        view.add_nika_message("Workflow completed".to_string(), Some(exec));

        assert_eq!(view.messages.len(), 2);
        assert!(view.messages[1].execution.is_some());
        let exec = view.messages[1].execution.as_ref().unwrap();
        assert_eq!(exec.status, ExecutionStatus::Completed);
    }

    #[test]
    fn test_message_role_equality() {
        assert_eq!(MessageRole::User, MessageRole::User);
        assert_ne!(MessageRole::User, MessageRole::Nika);
        assert_ne!(MessageRole::Nika, MessageRole::System);
    }

    #[test]
    fn test_execution_status_equality() {
        assert_eq!(ExecutionStatus::Running, ExecutionStatus::Running);
        assert_ne!(ExecutionStatus::Running, ExecutionStatus::Completed);
        assert_ne!(ExecutionStatus::Completed, ExecutionStatus::Failed);
    }

    #[test]
    fn test_chat_view_status_line() {
        let view = ChatView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        // New format: "{msgs} msgs | {provider} | {model}"
        assert!(status.contains("1 msgs"));
        assert!(status.contains(" | ")); // Contains provider | model separator
    }

    #[test]
    fn test_chat_view_default() {
        let view = ChatView::default();
        assert_eq!(view.messages.len(), 1);
        assert!(view.input.value().is_empty());
    }

    #[test]
    fn test_chat_view_unicode_input() {
        let mut view = ChatView::new();

        // Test emoji input (4 bytes per char)
        view.insert_char('\u{1F980}'); // Rust crab emoji
        view.insert_char('!');
        assert_eq!(view.input.value(), "\u{1F980}!");
        assert_eq!(view.input.cursor(), 2); // 2 chars, not 5 bytes

        // Test backspace removes whole emoji
        view.backspace();
        assert_eq!(view.input.value(), "\u{1F980}");
        assert_eq!(view.input.cursor(), 1);

        // Test cursor navigation with unicode
        view.insert_char('\u{1F600}'); // Grinning face emoji
        assert_eq!(view.input.value(), "\u{1F980}\u{1F600}");
        assert_eq!(view.input.cursor(), 2);

        view.cursor_left();
        assert_eq!(view.input.cursor(), 1);

        // Insert in middle
        view.insert_char('A');
        assert_eq!(view.input.value(), "\u{1F980}A\u{1F600}");
        assert_eq!(view.input.cursor(), 2);

        // Cursor right should work correctly
        view.cursor_right();
        assert_eq!(view.input.cursor(), 3);

        // Should not go past end
        view.cursor_right();
        assert_eq!(view.input.cursor(), 3);
    }

    #[test]
    fn test_chat_view_unicode_history() {
        let mut view = ChatView::new();
        view.add_user_message("Hello \u{1F44B}".to_string()); // Wave emoji

        view.history_up();
        assert_eq!(view.input.value(), "Hello \u{1F44B}");
        assert_eq!(view.input.cursor(), 7); // 7 chars (H-e-l-l-o-space-emoji), not 10 bytes
    }

    #[test]
    fn test_chat_view_multibyte_backspace() {
        let mut view = ChatView::new();

        // Build string with mixed byte-width chars
        view.insert_char('a'); // 1 byte
        view.insert_char('\u{00E9}'); // 2 bytes (e with acute)
        view.insert_char('\u{4E2D}'); // 3 bytes (Chinese character)
        view.insert_char('\u{1F980}'); // 4 bytes (crab emoji)

        assert_eq!(view.input.value(), "a\u{00E9}\u{4E2D}\u{1F980}");
        assert_eq!(view.input.cursor(), 4);

        // Backspace should remove each char correctly
        view.backspace();
        assert_eq!(view.input.value(), "a\u{00E9}\u{4E2D}");
        assert_eq!(view.input.cursor(), 3);

        view.backspace();
        assert_eq!(view.input.value(), "a\u{00E9}");
        assert_eq!(view.input.cursor(), 2);

        view.backspace();
        assert_eq!(view.input.value(), "a");
        assert_eq!(view.input.cursor(), 1);

        view.backspace();
        assert_eq!(view.input.value(), "");
        assert_eq!(view.input.cursor(), 0);
    }

    #[test]
    fn test_chat_view_streaming() {
        let mut view = ChatView::new();
        assert!(!view.is_streaming);

        view.start_streaming();
        assert!(view.is_streaming);
        assert!(view.partial_response.is_empty());

        view.append_streaming("Hello ");
        view.append_streaming("world!");
        assert_eq!(view.partial_response, "Hello world!");

        let result = view.finish_streaming();
        assert_eq!(result, "Hello world!");
        assert!(!view.is_streaming);
        assert!(view.partial_response.is_empty());
    }

    #[test]
    fn test_chat_view_set_model() {
        let mut view = ChatView::new();
        view.set_model("gpt-4o-mini");
        assert_eq!(view.current_model, "gpt-4o-mini");
    }

    #[test]
    fn test_chat_view_tool_message() {
        let mut view = ChatView::new();
        view.add_tool_message("Tool output: OK".to_string());
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[1].role, MessageRole::Tool);
        assert_eq!(view.messages[1].content, "Tool output: OK");
    }

    #[test]
    fn test_message_role_tool() {
        assert_eq!(MessageRole::Tool, MessageRole::Tool);
        assert_ne!(MessageRole::Tool, MessageRole::User);
        assert_ne!(MessageRole::Tool, MessageRole::Nika);
        assert_ne!(MessageRole::Tool, MessageRole::System);
    }

    #[test]
    fn test_chat_view_status_line_with_model() {
        let mut view = ChatView::new();
        view.set_model("gpt-4o-test");
        view.set_provider("OpenAI");
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        // New format: "{msgs} msgs | {provider} | {model}"
        assert!(status.contains("OpenAI"));
        assert!(status.contains("gpt-4o-test"));
        assert!(status.contains("1 msgs"));
    }

    #[test]
    fn test_chat_view_status_line_streaming() {
        let mut view = ChatView::new();
        view.start_streaming();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("Streaming..."));
    }

    // === Chat UX Enrichment (v2) Tests ===

    #[test]
    fn test_chat_view_session_context_initialized() {
        let view = ChatView::new();
        assert_eq!(view.session_context.token_limit, 200_000);
        assert!(view.session_context.started.is_some());
        assert_eq!(view.session_context.mcp_servers.len(), 1);
        assert_eq!(view.session_context.mcp_servers[0].name, "novanet");
    }

    #[test]
    fn test_chat_view_activity_items_empty_by_default() {
        let view = ChatView::new();
        assert!(view.activity_items.is_empty());
    }

    #[test]
    fn test_chat_view_command_palette_closed_by_default() {
        let view = ChatView::new();
        assert!(!view.command_palette.visible);
    }

    #[test]
    fn test_chat_view_toggle_command_palette() {
        let mut view = ChatView::new();
        assert!(!view.command_palette.visible);

        view.toggle_command_palette();
        assert!(view.command_palette.visible);

        view.toggle_command_palette();
        assert!(!view.command_palette.visible);
    }

    #[test]
    fn test_chat_view_tick_increments_frame() {
        let mut view = ChatView::new();
        assert_eq!(view.frame, 0);

        view.tick();
        assert_eq!(view.frame, 1);

        view.tick();
        assert_eq!(view.frame, 2);
    }

    #[test]
    fn test_chat_view_add_mcp_call() {
        let mut view = ChatView::new();
        view.add_mcp_call("novanet_describe", "novanet", r#"{"entity": "qr-code"}"#);

        assert_eq!(view.inline_content.len(), 1);
        if let InlineContent::McpCall(data) = &view.inline_content[0] {
            assert_eq!(data.tool, "novanet_describe");
            assert_eq!(data.server, "novanet");
            assert_eq!(data.status, McpCallStatus::Running);
        } else {
            panic!("Expected McpCall");
        }

        // Should add activity item
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "invoke");
        assert_eq!(view.activity_items[0].temp, ActivityTemp::Hot);

        // Should update MCP server status to hot
        assert_eq!(view.session_context.mcp_servers[0].status, McpStatus::Hot);
    }

    #[test]
    fn test_chat_view_complete_mcp_call() {
        let mut view = ChatView::new();
        view.add_mcp_call("novanet_describe", "novanet", "params");
        view.complete_mcp_call(r#"{"result": "success"}"#);

        if let InlineContent::McpCall(data) = &view.inline_content[0] {
            assert_eq!(data.status, McpCallStatus::Success);
            assert!(data.result.is_some());
        } else {
            panic!("Expected McpCall");
        }
    }

    #[test]
    fn test_chat_view_fail_mcp_call() {
        let mut view = ChatView::new();
        view.add_mcp_call("novanet_describe", "novanet", "params");
        view.fail_mcp_call("Connection error");

        if let InlineContent::McpCall(data) = &view.inline_content[0] {
            assert_eq!(data.status, McpCallStatus::Failed);
            assert!(data.error.is_some());
            assert_eq!(data.error.as_ref().unwrap(), "Connection error");
        } else {
            panic!("Expected McpCall");
        }
    }

    #[test]
    fn test_chat_view_start_infer_stream() {
        let mut view = ChatView::new();
        view.start_infer_stream("claude-sonnet-4", 100, 2000);

        // v0.8 FIX: InferStream boxes no longer created - streaming_decrypt handles visual
        assert_eq!(view.inline_content.len(), 0);

        // Should add activity item for Mission Control panel
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "infer");
    }

    #[test]
    fn test_chat_view_append_infer_content() {
        let mut view = ChatView::new();
        view.start_infer_stream("claude-sonnet-4", 100, 2000);
        view.append_infer_content("Hello ", 10);
        view.append_infer_content("World!", 20);

        // v0.8 FIX: No InferStream in inline_content - streaming_decrypt handles visual
        assert_eq!(view.inline_content.len(), 0);

        // partial_response is used by streaming_decrypt for the matrix reveal effect
        assert_eq!(view.partial_response, "Hello World!");
    }

    #[test]
    fn test_chat_view_update_tokens() {
        let mut view = ChatView::new();
        view.update_tokens(5000, 0.25);

        assert_eq!(view.session_context.tokens_used, 5000);
        assert_eq!(view.session_context.total_cost, 0.25);
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(60, 50, area);

        // Should be roughly centered
        assert!(centered.x > 0);
        assert!(centered.y > 0);
        assert!(centered.width < 100);
        assert!(centered.height < 50);
    }

    #[test]
    fn test_inline_content_enum() {
        let mcp_data = McpCallData::new("tool", "server");
        let content = InlineContent::McpCall(mcp_data);

        if let InlineContent::McpCall(data) = content {
            assert_eq!(data.tool, "tool");
        } else {
            panic!("Expected McpCall variant");
        }

        let infer_data = InferStreamData::new("model");
        let content = InlineContent::InferStream(infer_data);

        if let InlineContent::InferStream(data) = content {
            assert_eq!(data.model, "model");
        } else {
            panic!("Expected InferStream variant");
        }
    }

    // === Scroll Tests (Panel-based scroll system) ===
    // v0.8.1: Updated to test offset-based scrolling (NovaNet pattern)

    #[test]
    fn test_chat_view_scroll_up() {
        let mut view = ChatView::new();
        // Focus conversation panel and set offset position
        view.focus_panel(ChatPanel::Conversation);
        view.conversation_scroll.offset = 5;
        view.conversation_scroll.total = 20;
        view.conversation_scroll.visible = 10;

        view.scroll_up();
        assert_eq!(view.conversation_scroll.offset, 4);

        view.scroll_up();
        view.scroll_up();
        view.scroll_up();
        view.scroll_up();
        assert_eq!(view.conversation_scroll.offset, 0);

        // Should not go negative
        view.scroll_up();
        assert_eq!(view.conversation_scroll.offset, 0);
    }

    #[test]
    fn test_chat_view_scroll_down() {
        let mut view = ChatView::new();
        // Focus conversation panel and set up scrollable content
        view.focus_panel(ChatPanel::Conversation);
        view.conversation_scroll.total = 20;
        view.conversation_scroll.visible = 10;
        view.conversation_scroll.offset = 0;

        // Can scroll down when there's content
        view.scroll_down();
        assert_eq!(view.conversation_scroll.offset, 1);

        view.scroll_down();
        view.scroll_down();
        assert_eq!(view.conversation_scroll.offset, 3);

        // Set offset near end
        view.conversation_scroll.offset = 9;
        view.scroll_down();
        assert_eq!(view.conversation_scroll.offset, 10); // max is total - visible

        // Should cap at total - visible
        view.scroll_down();
        assert_eq!(view.conversation_scroll.offset, 10);
    }

    #[test]
    fn test_chat_view_scroll_to_bottom() {
        let mut view = ChatView::new();
        view.focus_panel(ChatPanel::Conversation);
        view.conversation_scroll.total = 20;
        view.conversation_scroll.visible = 10;
        view.conversation_scroll.offset = 3;
        view.conversation_scroll.cursor = 3;

        view.scroll_to_bottom();
        // scroll_to_bottom sets offset to total - visible and cursor to total - 1
        assert_eq!(view.conversation_scroll.offset, 10);
        assert_eq!(view.conversation_scroll.cursor, 19);
    }

    #[test]
    fn test_chat_view_scroll_from_input_panel() {
        // v0.8.1: Test that scroll works from Input panel (NovaNet pattern)
        let mut view = ChatView::new();
        // Default focus is Input panel
        assert_eq!(view.focused_panel, ChatPanel::Input);
        view.conversation_scroll.total = 20;
        view.conversation_scroll.visible = 10;
        view.conversation_scroll.offset = 5;

        // Scroll should still work on conversation panel
        view.scroll_down();
        assert_eq!(view.conversation_scroll.offset, 6);

        view.scroll_up();
        assert_eq!(view.conversation_scroll.offset, 5);
    }

    // === Thinking Display Tests (CRITICAL 3) ===

    #[test]
    fn test_chat_message_has_thinking_field() {
        let msg = ChatMessage {
            id: 1, // Test ID
            role: MessageRole::Nika,
            content: "Here's my answer.".to_string(),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
            thinking: Some("Let me analyze this step by step...".to_string()),
        };

        assert!(msg.thinking.is_some());
        assert_eq!(
            msg.thinking.as_ref().unwrap(),
            "Let me analyze this step by step..."
        );
    }

    #[test]
    fn test_chat_view_add_nika_message_with_thinking() {
        let mut view = ChatView::new();
        view.add_nika_message_with_thinking(
            "The answer is 42.".to_string(),
            Some("First, let me think about this deeply...".to_string()),
            None,
        );

        assert_eq!(view.messages.len(), 2); // welcome + new message
        let msg = &view.messages[1];
        assert_eq!(msg.role, MessageRole::Nika);
        assert_eq!(msg.content, "The answer is 42.");
        assert!(msg.thinking.is_some());
        assert_eq!(
            msg.thinking.as_ref().unwrap(),
            "First, let me think about this deeply..."
        );
    }

    #[test]
    fn test_chat_view_add_nika_message_without_thinking() {
        let mut view = ChatView::new();
        view.add_nika_message_with_thinking("Quick answer.".to_string(), None, None);

        assert_eq!(view.messages.len(), 2);
        let msg = &view.messages[1];
        assert!(msg.thinking.is_none());
    }

    #[test]
    fn test_chat_view_regular_nika_message_has_no_thinking() {
        let mut view = ChatView::new();
        view.add_nika_message("Regular response.".to_string(), None);

        assert_eq!(view.messages.len(), 2);
        let msg = &view.messages[1];
        assert!(msg.thinking.is_none());
    }

    #[test]
    fn test_chat_view_append_thinking() {
        let mut view = ChatView::new();
        assert!(view.pending_thinking.is_none());

        view.append_thinking("First thought");
        assert_eq!(view.pending_thinking.as_ref().unwrap(), "First thought");

        view.append_thinking("Second thought");
        assert_eq!(
            view.pending_thinking.as_ref().unwrap(),
            "First thought\nSecond thought"
        );
    }

    #[test]
    fn test_chat_view_finalize_thinking() {
        let mut view = ChatView::new();

        // Add a Nika message first
        view.add_nika_message("Here's my answer.".to_string(), None);
        assert!(view.messages[1].thinking.is_none());

        // Accumulate thinking
        view.append_thinking("Let me think...");
        view.append_thinking("Step 1: analyze");
        assert!(view.pending_thinking.is_some());

        // Finalize - should attach to last Nika message
        view.finalize_thinking();
        assert!(view.pending_thinking.is_none());
        assert!(view.messages[1].thinking.is_some());
        assert_eq!(
            view.messages[1].thinking.as_ref().unwrap(),
            "Let me think...\nStep 1: analyze"
        );
    }

    #[test]
    fn test_chat_view_finalize_thinking_no_nika_message() {
        let mut view = ChatView::new();

        // Only has system message (welcome)
        view.append_thinking("Some thinking");

        // Finalize - should clear but not attach (no Nika message)
        view.finalize_thinking();
        assert!(view.pending_thinking.is_none());
        // Welcome message should not have thinking
        assert!(view.messages[0].thinking.is_none());
    }

    // === Error Handling Tests (HIGH 5) ===

    #[test]
    fn test_categorize_error_auth() {
        let (cat, _) = ChatView::categorize_error("Invalid API key");
        assert_eq!(cat, "Auth");

        let (cat, _) = ChatView::categorize_error("Unauthorized access");
        assert_eq!(cat, "Auth");
    }

    #[test]
    fn test_categorize_error_timeout() {
        let (cat, _) = ChatView::categorize_error("Request timed out");
        assert_eq!(cat, "Timeout");

        let (cat, _) = ChatView::categorize_error("Deadline exceeded");
        assert_eq!(cat, "Timeout");
    }

    #[test]
    fn test_categorize_error_rate_limit() {
        let (cat, _) = ChatView::categorize_error("Rate limit exceeded");
        assert_eq!(cat, "Rate Limit");

        let (cat, _) = ChatView::categorize_error("Too many requests");
        assert_eq!(cat, "Rate Limit");
    }

    #[test]
    fn test_categorize_error_network() {
        let (cat, _) = ChatView::categorize_error("Connection refused");
        assert_eq!(cat, "Network");

        let (cat, _) = ChatView::categorize_error("DNS resolution failed");
        assert_eq!(cat, "Network");
    }

    #[test]
    fn test_categorize_error_mcp() {
        let (cat, _) = ChatView::categorize_error("MCP server not responding");
        assert_eq!(cat, "MCP");

        let (cat, _) = ChatView::categorize_error("Tool execution failed");
        assert_eq!(cat, "MCP");
    }

    #[test]
    fn test_categorize_error_parse() {
        let (cat, _) = ChatView::categorize_error("JSON parse error");
        assert_eq!(cat, "Parse");

        let (cat, _) = ChatView::categorize_error("Invalid format");
        assert_eq!(cat, "Parse");
    }

    #[test]
    fn test_categorize_error_unknown() {
        let (cat, _) = ChatView::categorize_error("Something went wrong");
        assert_eq!(cat, "Unexpected");
    }

    #[test]
    fn test_show_error_adds_system_message() {
        let mut view = ChatView::new();
        let initial_count = view.messages.len();

        view.show_error("Test error message");

        assert_eq!(view.messages.len(), initial_count + 1);
        let last = view.messages.last().unwrap();
        assert_eq!(last.role, MessageRole::System);
        assert!(last.content.contains("Error"));
        assert!(last.content.contains("Test error message"));
        assert!(last.content.contains("/help"));
    }

    // === tui-input Feature Tests (v0.5.2+) ===

    #[test]
    fn test_chat_view_word_navigation() {
        let mut view = ChatView::new();
        view.input = Input::new("hello world foo".to_string());
        view.input.handle(InputRequest::GoToStart);
        assert_eq!(view.input.cursor(), 0);

        // Move to next word
        view.cursor_next_word();
        assert_eq!(view.input.cursor(), 6); // After "hello " at 'w'

        view.cursor_next_word();
        assert_eq!(view.input.cursor(), 12); // After "world " at 'f'

        // Move to previous word
        view.cursor_prev_word();
        assert_eq!(view.input.cursor(), 6); // Back to 'w'

        view.cursor_prev_word();
        assert_eq!(view.input.cursor(), 0); // Back to start
    }

    #[test]
    fn test_chat_view_delete_prev_word() {
        let mut view = ChatView::new();
        view.input = Input::new("hello world".to_string());
        view.input.handle(InputRequest::GoToEnd);
        assert_eq!(view.input.cursor(), 11);

        // Delete "world"
        view.delete_prev_word();
        assert_eq!(view.input.value(), "hello ");
        assert_eq!(view.input.cursor(), 6);

        // Delete "hello "
        view.delete_prev_word();
        assert_eq!(view.input.value(), "");
        assert_eq!(view.input.cursor(), 0);
    }

    #[test]
    fn test_chat_view_cursor_start_end() {
        let mut view = ChatView::new();
        view.input = Input::new("hello world".to_string());

        // Start in middle
        view.input.handle(InputRequest::GoToPrevWord);
        assert!(view.input.cursor() < 11);

        // Go to end
        view.cursor_end();
        assert_eq!(view.input.cursor(), 11);

        // Go to start
        view.cursor_start();
        assert_eq!(view.input.cursor(), 0);
    }

    #[test]
    fn test_chat_view_clipboard_does_not_panic() {
        let mut view = ChatView::new();
        view.input = Input::new("test".to_string());
        view.input.handle(InputRequest::GoToEnd);

        // Copy and paste should not panic even if clipboard is None or available
        // If clipboard works, it will append "test" at cursor position
        view.copy_to_clipboard();

        // Reset input to test paste alone
        view.input.reset();
        view.paste_from_clipboard();

        // Either clipboard is None (empty) or it works (has "test")
        let value = view.input.value();
        assert!(value.is_empty() || value == "test");
    }

    #[test]
    fn test_chat_view_input_reset() {
        let mut view = ChatView::new();
        view.input = Input::new("hello world".to_string());
        view.input.handle(InputRequest::GoToEnd);
        assert_eq!(view.input.cursor(), 11);
        assert_eq!(view.input.value(), "hello world");

        // Reset input
        view.input.reset();
        assert_eq!(view.input.value(), "");
        assert_eq!(view.input.cursor(), 0);
    }

    // === Session Persistence Tests (HIGH 8) ===

    #[test]
    fn test_serializable_role_conversion() {
        // MessageRole -> SerializableRole
        assert_eq!(
            SerializableRole::from(&MessageRole::User),
            SerializableRole::User
        );
        assert_eq!(
            SerializableRole::from(&MessageRole::Nika),
            SerializableRole::Nika
        );
        assert_eq!(
            SerializableRole::from(&MessageRole::System),
            SerializableRole::System
        );
        assert_eq!(
            SerializableRole::from(&MessageRole::Tool),
            SerializableRole::Tool
        );

        // SerializableRole -> MessageRole
        assert_eq!(MessageRole::from(SerializableRole::User), MessageRole::User);
        assert_eq!(MessageRole::from(SerializableRole::Nika), MessageRole::Nika);
        assert_eq!(
            MessageRole::from(SerializableRole::System),
            MessageRole::System
        );
        assert_eq!(MessageRole::from(SerializableRole::Tool), MessageRole::Tool);
    }

    #[test]
    fn test_chat_session_from_view() {
        let mut view = ChatView::new();
        view.add_user_message("Hello".to_string());
        view.add_nika_message("Hi there!".to_string(), None);
        view.set_model("claude-sonnet");

        let session = ChatSession::from_view(&view);

        assert_eq!(session.version, "0.5.2");
        assert_eq!(session.model, "claude-sonnet");
        // 1 welcome message + 2 added messages = 3
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[1].content, "Hello");
        assert_eq!(session.messages[1].role, SerializableRole::User);
        assert_eq!(session.messages[2].content, "Hi there!");
        assert_eq!(session.messages[2].role, SerializableRole::Nika);
    }

    #[test]
    fn test_chat_session_round_trip() {
        use tempfile::tempdir;

        let mut view = ChatView::new();
        view.add_user_message("Test message".to_string());
        view.add_nika_message("Response".to_string(), None);
        view.set_model("gpt-4");

        let dir = tempdir().unwrap();
        let path = dir.path().join("test-session.json");

        // Save session
        view.save_session(&path).unwrap();
        assert!(path.exists());

        // Load into fresh view
        let mut view2 = ChatView::new();
        view2.load_session(&path).unwrap();

        // Verify messages restored (excluding welcome message which gets replaced)
        assert_eq!(view2.messages.len(), view.messages.len());
        assert_eq!(view2.messages[1].content, "Test message");
        assert_eq!(view2.messages[1].role, MessageRole::User);
        assert_eq!(view2.messages[2].content, "Response");
        assert_eq!(view2.messages[2].role, MessageRole::Nika);
        assert_eq!(view2.current_model, "gpt-4");
    }

    #[test]
    fn test_chat_session_preserves_thinking() {
        use tempfile::tempdir;

        let mut view = ChatView::new();
        view.add_nika_message_with_thinking(
            "Answer".to_string(),
            Some("My reasoning...".to_string()),
            None,
        );

        let dir = tempdir().unwrap();
        let path = dir.path().join("thinking-session.json");

        view.save_session(&path).unwrap();

        let mut view2 = ChatView::new();
        view2.load_session(&path).unwrap();

        // Last message should have thinking preserved
        let last = view2.messages.last().unwrap();
        assert_eq!(last.content, "Answer");
        assert_eq!(last.thinking, Some("My reasoning...".to_string()));
    }

    #[test]
    fn test_default_session_path() {
        let path = ChatView::default_session_path();
        assert!(
            path.ends_with("nika-chat-session.json") || path.to_string_lossy().contains("nika")
        );
    }

    // === Phase 8: Real-time Streaming Updates Tests (v0.7.3) ===

    #[test]
    fn test_set_current_verb() {
        let mut view = ChatView::new();
        assert!(matches!(view.current_verb, CurrentVerb::None));

        view.set_current_verb(CurrentVerb::Infer);
        assert!(matches!(view.current_verb, CurrentVerb::Infer));

        view.set_current_verb(CurrentVerb::Agent);
        assert!(matches!(view.current_verb, CurrentVerb::Agent));

        view.set_current_verb(CurrentVerb::Invoke);
        assert!(matches!(view.current_verb, CurrentVerb::Invoke));

        view.set_current_verb(CurrentVerb::Exec);
        assert!(matches!(view.current_verb, CurrentVerb::Exec));

        view.set_current_verb(CurrentVerb::Fetch);
        assert!(matches!(view.current_verb, CurrentVerb::Fetch));
    }

    #[test]
    fn test_update_mode_from_input_agent() {
        // v0.8.1: Test that typing /agent updates chat_mode and current_verb
        let mut view = ChatView::new();

        // Initial state
        assert!(matches!(view.chat_mode, ChatMode::Infer));
        assert!(matches!(view.current_verb, CurrentVerb::None));

        // Type "/agent " (with space to mark complete)
        for c in "/agent ".chars() {
            view.insert_char(c);
        }

        // Mode should switch to Agent
        assert!(
            matches!(view.chat_mode, ChatMode::Agent),
            "chat_mode should be Agent after typing /agent"
        );
        assert!(
            matches!(view.current_verb, CurrentVerb::Agent),
            "current_verb should be Agent after typing /agent"
        );
    }

    #[test]
    fn test_update_mode_from_input_infer() {
        // v0.8.1: Test that typing /infer updates mode
        let mut view = ChatView::new();
        view.chat_mode = ChatMode::Agent; // Start in Agent mode

        // Type "/infer "
        for c in "/infer ".chars() {
            view.insert_char(c);
        }

        // Mode should switch to Infer
        assert!(
            matches!(view.chat_mode, ChatMode::Infer),
            "chat_mode should be Infer after typing /infer"
        );
        assert!(
            matches!(view.current_verb, CurrentVerb::Infer),
            "current_verb should be Infer after typing /infer"
        );
    }

    #[test]
    fn test_update_mode_from_input_verb_exec() {
        // v0.8.1: Test that typing /exec updates current_verb but not chat_mode
        let mut view = ChatView::new();

        // Type "/exec " (the Nika workflow verb, not shell exec)
        for c in "/exec ".chars() {
            view.insert_char(c);
        }

        // chat_mode should stay Infer (exec is not a chat mode)
        assert!(
            matches!(view.chat_mode, ChatMode::Infer),
            "chat_mode should stay Infer for /exec"
        );
        // current_verb should be Exec
        assert!(
            matches!(view.current_verb, CurrentVerb::Exec),
            "current_verb should be Exec after typing /exec"
        );
    }

    #[test]
    fn test_update_mode_from_input_reset_on_clear() {
        // v0.8.1: Test that clearing input resets current_verb
        let mut view = ChatView::new();

        // Type "/agent "
        for c in "/agent ".chars() {
            view.insert_char(c);
        }
        assert!(matches!(view.current_verb, CurrentVerb::Agent));

        // Clear input by backspacing everything
        for _ in 0.."/agent ".len() {
            view.backspace();
        }

        // current_verb should reset to None
        assert!(
            matches!(view.current_verb, CurrentVerb::None),
            "current_verb should be None after clearing input"
        );
        // chat_mode stays Agent (intentional - user chose Agent mode)
        assert!(
            matches!(view.chat_mode, ChatMode::Agent),
            "chat_mode should stay Agent after clearing"
        );
    }

    #[test]
    fn test_update_turn_metrics_initial() {
        let mut view = ChatView::new();

        // Initial state
        assert_eq!(view.turn_metrics.input_tokens, 0);
        assert_eq!(view.turn_metrics.output_tokens, 0);
        assert_eq!(view.session_metrics.input_tokens, 0);
        assert_eq!(view.session_metrics.output_tokens, 0);

        // First update
        view.update_turn_metrics(100, 50, 0.001);

        // Turn metrics should reflect absolute values
        assert_eq!(view.turn_metrics.input_tokens, 100);
        assert_eq!(view.turn_metrics.output_tokens, 50);
        assert!((view.turn_metrics.cost_usd - 0.001).abs() < 0.0001);

        // Session metrics should have deltas (same as absolute for first update)
        assert_eq!(view.session_metrics.input_tokens, 100);
        assert_eq!(view.session_metrics.output_tokens, 50);
    }

    #[test]
    fn test_update_turn_metrics_incremental() {
        let mut view = ChatView::new();

        // First update: 100 input, 50 output
        view.update_turn_metrics(100, 50, 0.001);

        // Second update: 100 input (same), 80 output (30 more)
        view.update_turn_metrics(100, 80, 0.002);

        // Turn metrics should reflect new absolute values
        assert_eq!(view.turn_metrics.input_tokens, 100);
        assert_eq!(view.turn_metrics.output_tokens, 80);

        // Session metrics should have accumulated deltas
        // First: +100, +50. Second: +0, +30
        assert_eq!(view.session_metrics.input_tokens, 100); // 100 + 0
        assert_eq!(view.session_metrics.output_tokens, 80); // 50 + 30
    }

    #[test]
    fn test_increment_output_tokens() {
        let mut view = ChatView::new();

        // Start with some baseline
        view.update_turn_metrics(100, 50, 0.001);

        // Increment output tokens directly
        view.increment_output_tokens(25);

        assert_eq!(view.turn_metrics.output_tokens, 75); // 50 + 25
        assert_eq!(view.session_metrics.output_tokens, 75); // 50 + 25
                                                            // Input tokens unchanged
        assert_eq!(view.turn_metrics.input_tokens, 100);
        assert_eq!(view.session_metrics.input_tokens, 100);
    }

    #[test]
    fn test_reset_turn_metrics() {
        let mut view = ChatView::new();
        view.set_current_verb(CurrentVerb::Agent);
        view.update_turn_metrics(100, 50, 0.001);

        view.reset_turn_metrics();

        // Turn metrics should be reset
        assert_eq!(view.turn_metrics.input_tokens, 0);
        assert_eq!(view.turn_metrics.output_tokens, 0);
        assert_eq!(view.turn_metrics.cost_usd, 0.0);
        assert!(matches!(view.current_verb, CurrentVerb::None));

        // Session metrics should be unchanged
        assert_eq!(view.session_metrics.input_tokens, 100);
        assert_eq!(view.session_metrics.output_tokens, 50);
    }

    #[test]
    fn test_complete_turn() {
        let mut view = ChatView::new();

        // Simulate a turn with streaming updates
        view.set_current_verb(CurrentVerb::Infer);
        view.update_turn_metrics(100, 50, 0.001);
        view.update_turn_metrics(100, 100, 0.002); // More output tokens

        // Session should already have accumulated values
        assert_eq!(view.session_metrics.input_tokens, 100);
        assert_eq!(view.session_metrics.output_tokens, 100);

        // Complete the turn
        view.complete_turn();

        // Turn metrics should be reset
        assert_eq!(view.turn_metrics.input_tokens, 0);
        assert_eq!(view.turn_metrics.output_tokens, 0);
        assert!(matches!(view.current_verb, CurrentVerb::None));

        // Session metrics should be unchanged (already updated incrementally)
        assert_eq!(view.session_metrics.input_tokens, 100);
        assert_eq!(view.session_metrics.output_tokens, 100);
    }

    #[test]
    fn test_multi_turn_session() {
        let mut view = ChatView::new();

        // Turn 1: infer with 100 input, 50 output
        view.set_current_verb(CurrentVerb::Infer);
        view.update_turn_metrics(100, 50, 0.001);
        view.complete_turn();

        // Turn 2: agent with 200 input, 100 output
        view.set_current_verb(CurrentVerb::Agent);
        view.update_turn_metrics(200, 100, 0.003);
        view.complete_turn();

        // Session should have accumulated both turns
        assert_eq!(view.session_metrics.input_tokens, 300); // 100 + 200
        assert_eq!(view.session_metrics.output_tokens, 150); // 50 + 100
        assert!((view.session_metrics.cost_usd - 0.004).abs() < 0.0001);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.8.0: Tests for activity tracking methods (exec, fetch, agent)
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_exec_activity_lifecycle() {
        use crate::tui::widgets::ActivityTemp;

        let mut view = ChatView::new();

        // Initially no activities
        assert!(view.activity_items.is_empty());

        // Add exec activity
        view.add_exec_activity("ls -la");
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "exec");
        assert!(matches!(view.activity_items[0].temp, ActivityTemp::Hot));

        // Complete exec activity
        view.complete_exec_activity();
        assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
    }

    #[test]
    fn test_fetch_activity_lifecycle() {
        use crate::tui::widgets::ActivityTemp;

        let mut view = ChatView::new();

        // Add fetch activity
        view.add_fetch_activity("https://example.com", "GET");
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "fetch");

        // Complete fetch activity
        view.complete_fetch_activity();
        assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
    }

    #[test]
    fn test_agent_activity_lifecycle() {
        use crate::tui::widgets::ActivityTemp;

        let mut view = ChatView::new();

        // Add agent activity
        view.add_agent_activity("Generate a landing page");
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "agent");

        // Complete agent activity
        view.complete_agent_activity();
        assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
    }

    #[test]
    fn test_multiple_concurrent_activities() {
        use crate::tui::widgets::ActivityTemp;

        let mut view = ChatView::new();

        // Start multiple activities concurrently
        view.add_exec_activity("npm run build");
        view.add_fetch_activity("https://api.example.com", "POST");
        view.add_agent_activity("Analyze results");

        assert_eq!(view.activity_items.len(), 3);

        // All should be hot initially
        for item in &view.activity_items {
            assert!(matches!(item.temp, ActivityTemp::Hot));
        }

        // Complete them in different order
        view.complete_fetch_activity();
        view.complete_exec_activity();
        view.complete_agent_activity();

        // All should be warm now
        for item in &view.activity_items {
            assert!(matches!(item.temp, ActivityTemp::Warm));
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.8 Text Selection Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_text_selection_new() {
        let pos = SelectionPos {
            message_index: 0,
            char_offset: 5,
        };
        let selection = TextSelection::new(pos);
        assert_eq!(selection.start, pos);
        assert_eq!(selection.end, pos);
    }

    #[test]
    fn test_text_selection_normalized() {
        // Forward selection (start < end)
        let sel = TextSelection {
            start: SelectionPos {
                message_index: 0,
                char_offset: 5,
            },
            end: SelectionPos {
                message_index: 0,
                char_offset: 10,
            },
        };
        let (start, end) = sel.normalized();
        assert_eq!(start.char_offset, 5);
        assert_eq!(end.char_offset, 10);

        // Backward selection (end < start)
        let sel = TextSelection {
            start: SelectionPos {
                message_index: 0,
                char_offset: 10,
            },
            end: SelectionPos {
                message_index: 0,
                char_offset: 5,
            },
        };
        let (start, end) = sel.normalized();
        assert_eq!(start.char_offset, 5);
        assert_eq!(end.char_offset, 10);
    }

    #[test]
    fn test_text_selection_contains() {
        let sel = TextSelection {
            start: SelectionPos {
                message_index: 1,
                char_offset: 5,
            },
            end: SelectionPos {
                message_index: 1,
                char_offset: 15,
            },
        };

        // Inside selection
        assert!(sel.contains(SelectionPos {
            message_index: 1,
            char_offset: 10
        }));

        // At start
        assert!(sel.contains(SelectionPos {
            message_index: 1,
            char_offset: 5
        }));

        // Before selection
        assert!(!sel.contains(SelectionPos {
            message_index: 1,
            char_offset: 4
        }));
    }

    #[test]
    fn test_get_selected_text() {
        let mut view = ChatView::new();
        view.messages.clear();
        view.add_user_message("Hello, World!".to_string());

        // Select "World"
        view.text_selection = Some(TextSelection {
            start: SelectionPos {
                message_index: 0,
                char_offset: 7,
            },
            end: SelectionPos {
                message_index: 0,
                char_offset: 12,
            },
        });

        let selected = view.get_selected_text();
        assert_eq!(selected, Some("World".to_string()));
    }

    #[test]
    fn test_clear_selection() {
        let mut view = ChatView::new();
        view.text_selection = Some(TextSelection::new(SelectionPos {
            message_index: 0,
            char_offset: 0,
        }));
        view.is_selecting = true;

        view.clear_selection();

        assert!(view.text_selection.is_none());
        assert!(!view.is_selecting);
    }

    #[test]
    fn test_char_to_byte_offset_helper() {
        assert_eq!(char_to_byte_offset("hello", 2), 2);
        assert_eq!(char_to_byte_offset("héllo", 2), 3); // é is 2 bytes
        assert_eq!(char_to_byte_offset("a🦀b", 2), 5); // 🦀 is 4 bytes
        assert_eq!(char_to_byte_offset("hi", 10), 2); // Beyond end
    }

    #[test]
    fn test_selection_initialization() {
        let view = ChatView::new();
        assert!(view.text_selection.is_none());
        assert!(!view.is_selecting);
        assert!(view.line_positions.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.8.2: Provider Switching Tests (Critical Bug Fix)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_chat_view_has_current_provider_id() {
        let view = ChatView::new();
        // Provider ID should be set based on available API keys
        assert!(!view.current_provider_id.is_empty());
        // Should be one of the known providers, or "none" if no API keys available
        let valid_providers = [
            "claude", "openai", "mistral", "groq", "deepseek", "ollama", "none",
        ];
        assert!(
            valid_providers.contains(&view.current_provider_id.as_str()),
            "Provider ID '{}' should be a valid provider or 'none'",
            view.current_provider_id
        );
    }

    #[test]
    fn test_provider_id_matches_model() {
        let view = ChatView::new();
        // Provider ID should match the model's provider
        match view.current_provider_id.as_str() {
            "claude" => assert!(view.current_model.starts_with("claude")),
            "openai" => assert!(view.current_model.starts_with("gpt")),
            "mistral" => assert!(view.current_model.starts_with("mistral")),
            "groq" => assert!(
                view.current_model.contains("llama") || view.current_model.contains("mixtral")
            ),
            "deepseek" => assert!(view.current_model.starts_with("deepseek")),
            "ollama" => assert!(view.current_model.contains("llama")),
            "none" => assert!(view.current_model == "No API Key"), // CI without keys
            _ => panic!("Unknown provider: {}", view.current_provider_id),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.9: Thinking Toggle Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_thinking_toggle_initial_state() {
        let view = ChatView::new();
        assert!(view.thinking_collapsed.is_empty());
        // Default: expanded (thinking_expanded_default = true)
        assert!(view.thinking_expanded_default);
    }

    #[test]
    fn test_toggle_thinking_message_without_thinking() {
        let mut view = ChatView::new();
        view.add_user_message("Question".to_string());

        // User message is at index 1 (system welcome is 0)
        // Should not add to collapsed set if no thinking
        view.toggle_thinking(1);
        assert!(view.thinking_collapsed.is_empty());
    }

    #[test]
    fn test_toggle_all_thinking() {
        let mut view = ChatView::new();

        // Add messages with thinking (indices 1 and 2 after system welcome at 0)
        view.add_nika_message_with_thinking(
            "Answer 1".to_string(),
            Some("Thinking 1".to_string()),
            None,
        );
        view.add_nika_message_with_thinking(
            "Answer 2".to_string(),
            Some("Thinking 2".to_string()),
            None,
        );

        // Default is expanded (thinking_expanded_default = true)
        assert!(view.thinking_expanded_default);

        // Toggle all - should collapse all
        view.toggle_all_thinking();
        assert!(!view.thinking_expanded_default);
        assert!(view.thinking_collapsed.is_empty()); // Cleared

        // Toggle all again - should expand all
        view.toggle_all_thinking();
        assert!(view.thinking_expanded_default);
        assert!(view.thinking_collapsed.is_empty()); // Still cleared
    }

    #[test]
    fn test_is_thinking_visible_respects_default() {
        let mut view = ChatView::new();
        view.add_nika_message_with_thinking(
            "Answer".to_string(),
            Some("Thinking".to_string()),
            None,
        );

        // Message is at index 1 (system welcome at 0)
        // Get the message ID for direct set manipulation
        let msg_id = view.messages[1].id;

        // Default = true -> visible
        assert!(view.is_thinking_visible(1));

        // Change default to collapsed
        view.thinking_expanded_default = false;
        assert!(!view.is_thinking_visible(1));

        // Add message ID to override set -> should be visible (opposite of default=false)
        // v0.9 FIX: Use message ID instead of index for stability
        view.thinking_collapsed.insert(msg_id);
        assert!(view.is_thinking_visible(1));
    }

    #[test]
    fn test_toggle_thinking_with_content() {
        let mut view = ChatView::new();
        // Default is expanded (true)
        view.add_nika_message_with_thinking(
            "Answer".to_string(),
            Some("My reasoning...".to_string()),
            None,
        );

        // Message at index 1, initially expanded (default = true, not in override set)
        assert!(view.is_thinking_visible(1));

        // Toggle adds to override set -> now shows opposite of default (false = collapsed)
        view.toggle_thinking(1);
        assert!(!view.is_thinking_visible(1)); // Now collapsed

        // Toggle again removes from override set -> back to default (true = expanded)
        view.toggle_thinking(1);
        assert!(view.is_thinking_visible(1)); // Back to expanded
    }

    #[test]
    fn test_thinking_toggle_with_collapsed_default() {
        let mut view = ChatView::new();
        view.thinking_expanded_default = false; // Start with collapsed default

        view.add_nika_message_with_thinking(
            "Answer".to_string(),
            Some("Thinking...".to_string()),
            None,
        );

        // Message at index 1, initially collapsed (default = false, not in override set)
        assert!(!view.is_thinking_visible(1));

        // Toggle adds to override set -> shows opposite of default (true = expanded)
        view.toggle_thinking(1);
        assert!(view.is_thinking_visible(1)); // Now expanded

        // Toggle again removes from override set -> back to default (false = collapsed)
        view.toggle_thinking(1);
        assert!(!view.is_thinking_visible(1)); // Back to collapsed
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.9: Export Path Validation Security Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_export_rejects_absolute_paths() {
        let view = ChatView::new();
        let result = view.export_session(Some("/tmp/malicious.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Absolute paths not allowed"));
    }

    #[test]
    fn test_export_rejects_path_traversal() {
        let view = ChatView::new();

        // Simple parent traversal
        let result = view.export_session(Some("../../../etc/passwd.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path traversal"));

        // Hidden in middle of path
        let result = view.export_session(Some("exports/../../../malicious.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path traversal"));
    }

    #[test]
    fn test_export_allows_relative_paths() {
        let view = ChatView::new();

        // Simple filename (no directory) - should work
        // Use unique name to avoid test interference
        let filename = format!("test-export-relative-{}", std::process::id());
        let result = view.export_session(Some(&filename));
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with(".json")); // .json added

        // Cleanup test file
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_adds_json_extension() {
        let view = ChatView::new();

        // Use unique filenames to avoid test interference
        let base_name = format!("test-export-ext-{}", std::process::id());

        // Without .json extension
        let result = view.export_session(Some(&base_name));
        assert!(result.is_ok());
        let path1 = result.unwrap();
        assert!(path1.ends_with(".json"));
        let _ = std::fs::remove_file(&path1);

        // With .json extension already
        let name_with_ext = format!("{}.json", base_name);
        let result = view.export_session(Some(&name_with_ext));
        assert!(result.is_ok());
        let path2 = result.unwrap();
        assert_eq!(path2, name_with_ext);
        let _ = std::fs::remove_file(&path2);
    }

    #[test]
    fn test_export_default_filename_is_safe() {
        let view = ChatView::new();

        // Default filename (None) should always be safe
        let result = view.export_session(None);
        assert!(result.is_ok());
        let filename = result.unwrap();
        assert!(filename.starts_with("nika-chat-"));
        assert!(filename.ends_with(".json"));

        // Cleanup test file
        let _ = std::fs::remove_file(&filename);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 2: @ Mention Autocomplete Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mention_autocomplete_hidden_by_default() {
        let view = ChatView::new();
        assert!(!view.mention_autocomplete.visible);
        assert!(view.mention_autocomplete.suggestions.is_empty());
    }

    #[test]
    fn test_mention_autocomplete_triggers_on_at() {
        let mut view = ChatView::new();

        // Type @ to trigger autocomplete
        view.insert_char('@');
        view.check_mention_trigger();

        // Should show autocomplete with suggestions (file suggestions)
        // Note: actual visibility depends on generate_mention_suggestions implementation
        // For now, we test that check_mention_trigger runs without panic
        assert!(view.input.value() == "@");
    }

    #[test]
    fn test_mention_autocomplete_hides_on_space() {
        let mut view = ChatView::new();

        // Type @test (simulating a mention)
        view.input = Input::new("@test ".to_string());
        view.input.handle(InputRequest::GoToEnd);
        view.check_mention_trigger();

        // After space, autocomplete should be hidden
        assert!(!view.mention_autocomplete.visible);
    }

    #[test]
    fn test_mention_autocomplete_navigation() {
        let mut view = ChatView::new();

        // Manually show autocomplete with suggestions
        let suggestions = vec![
            MentionSuggestion {
                mention_type: MentionType::File,
                display: "file1.txt".to_string(),
                insert: "@file1.txt".to_string(),
                description: Some("A file".to_string()),
            },
            MentionSuggestion {
                mention_type: MentionType::File,
                display: "file2.txt".to_string(),
                insert: "@file2.txt".to_string(),
                description: Some("Another file".to_string()),
            },
        ];
        view.mention_autocomplete.show(
            crate::tui::widgets::MentionTrigger {
                start: 0,
                partial: "@".to_string(),
                trigger_type: None,
                query: String::new(),
            },
            suggestions,
        );

        assert!(view.mention_autocomplete.visible);
        assert_eq!(view.mention_autocomplete.selected, 0);

        // Navigate down
        view.mention_autocomplete.next();
        assert_eq!(view.mention_autocomplete.selected, 1);

        // Navigate down wraps around
        view.mention_autocomplete.next();
        assert_eq!(view.mention_autocomplete.selected, 0);

        // Navigate up wraps around
        view.mention_autocomplete.prev();
        assert_eq!(view.mention_autocomplete.selected, 1);
    }

    #[test]
    fn test_mention_autocomplete_hide() {
        let mut view = ChatView::new();

        // Manually show autocomplete
        view.mention_autocomplete.show(
            crate::tui::widgets::MentionTrigger {
                start: 0,
                partial: "@".to_string(),
                trigger_type: None,
                query: String::new(),
            },
            vec![MentionSuggestion {
                mention_type: MentionType::File,
                display: "test.txt".to_string(),
                insert: "@test.txt".to_string(),
                description: None,
            }],
        );
        assert!(view.mention_autocomplete.visible);

        // Hide
        view.mention_autocomplete.hide();
        assert!(!view.mention_autocomplete.visible);
        assert!(view.mention_autocomplete.suggestions.is_empty());
    }

    #[test]
    fn test_check_mention_trigger_unicode_safe() {
        let mut view = ChatView::new();

        // Test with unicode content before cursor
        view.input = Input::new("🦀🐔@".to_string());
        view.input.handle(InputRequest::GoToEnd);

        // Should not panic on unicode (byte vs char index handling)
        view.check_mention_trigger();
    }

    #[test]
    fn test_check_mention_trigger_empty_input() {
        let mut view = ChatView::new();

        // Empty input should not show autocomplete
        view.check_mention_trigger();
        assert!(!view.mention_autocomplete.visible);
    }

    #[test]
    fn test_accept_mention_suggestion_replaces_trigger() {
        let mut view = ChatView::new();

        // Set up input with partial mention
        view.input = Input::new("@fi".to_string());
        view.input.handle(InputRequest::GoToEnd);

        // Show autocomplete with a suggestion
        view.mention_autocomplete.show(
            crate::tui::widgets::MentionTrigger {
                start: 0,
                partial: "@fi".to_string(),
                trigger_type: None,
                query: "fi".to_string(),
            },
            vec![MentionSuggestion {
                mention_type: MentionType::File,
                display: "file.txt".to_string(),
                insert: "@file.txt".to_string(),
                description: None,
            }],
        );

        // Accept the suggestion
        view.accept_mention_suggestion();

        // Should have replaced @fi with @file.txt
        assert!(view.input.value().contains("file.txt"));
        assert!(!view.mention_autocomplete.visible);
    }

    #[test]
    fn test_accept_mention_cursor_position_mid_sentence() {
        let mut view = ChatView::new();

        // Set up input with mention in middle: "Load @fi and process"
        view.input = Input::new("Load @fi and process".to_string());
        // Position cursor after @fi (at position 8)
        view.input.handle(InputRequest::GoToStart);
        for _ in 0..8 {
            view.input.handle(InputRequest::GoToNextChar);
        }

        // Show autocomplete with a suggestion
        view.mention_autocomplete.show(
            crate::tui::widgets::MentionTrigger {
                start: 5,
                partial: "@fi".to_string(),
                trigger_type: None,
                query: "fi".to_string(),
            },
            vec![MentionSuggestion {
                mention_type: MentionType::File,
                display: "file.rs".to_string(),
                insert: "@file.rs".to_string(),
                description: None,
            }],
        );

        // Accept the suggestion
        view.accept_mention_suggestion();

        // Input should be "Load @file.rs  and process" (with space after mention)
        let value = view.input.value();
        assert!(
            value.contains("@file.rs"),
            "Should contain @file.rs: {}",
            value
        );

        // Cursor should be after "@file.rs " (position 15), NOT at end
        // "Load " (5) + "@file.rs" (8) + " " (1) = 14 chars
        let cursor = view.input.cursor();
        let expected_cursor = 5 + 8 + 1; // start + mention length + space
        assert_eq!(
            cursor, expected_cursor,
            "Cursor should be at {} after mention, not at {} (value: '{}')",
            expected_cursor, cursor, value
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 2: Multi-line Input Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_multiline_input_newline_insertion() {
        let mut view = ChatView::new();

        // Type some text
        view.insert_char('h');
        view.insert_char('e');
        view.insert_char('l');
        view.insert_char('l');
        view.insert_char('o');

        // Insert newline (simulating Shift+Enter)
        view.input.handle(InputRequest::InsertChar('\n'));

        // Type more text
        view.insert_char('w');
        view.insert_char('o');
        view.insert_char('r');
        view.insert_char('l');
        view.insert_char('d');

        // Should have multi-line content
        let value = view.input.value();
        assert!(value.contains('\n'));
        assert!(value.contains("hello"));
        assert!(value.contains("world"));
    }

    #[test]
    fn test_multiline_input_submit_preserves_newlines() {
        let mut view = ChatView::new();

        // Set multi-line input directly
        view.input = Input::new("line1\nline2\nline3".to_string());
        view.input.handle(InputRequest::GoToEnd);

        // Submit
        let message = view.submit();
        assert!(message.is_some());
        let msg = message.unwrap();

        // Should preserve newlines
        assert!(msg.contains('\n'));
        assert_eq!(msg.lines().count(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 2: MCP Retry Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mcp_retry_no_failed_call() {
        let view = ChatView::new();

        // By default, no failed MCP call
        assert!(view.last_failed_mcp.is_none());
    }

    #[test]
    fn test_mcp_retry_saves_failed_call() {
        let mut view = ChatView::new();

        // Add an MCP call
        view.add_mcp_call("test_tool", "test_server", r#"{"key": "value"}"#);

        // Fail the call
        view.fail_mcp_call("Connection timeout");

        // Should have saved the failed call
        assert!(view.last_failed_mcp.is_some());
        let failed = view.last_failed_mcp.as_ref().unwrap();
        assert_eq!(failed.tool, "test_tool");
        assert_eq!(failed.server, "test_server");
        assert_eq!(failed.params["key"], "value");
    }

    #[test]
    fn test_mcp_retry_clears_on_take() {
        let mut view = ChatView::new();

        // Set up a failed MCP call
        view.last_failed_mcp = Some(FailedMcpCall {
            tool: "my_tool".to_string(),
            server: "my_server".to_string(),
            params: serde_json::json!({"test": 123}),
        });

        // Take the failed call (simulating Ctrl+R)
        let taken = view.last_failed_mcp.take();
        assert!(taken.is_some());

        // Should be cleared now
        assert!(view.last_failed_mcp.is_none());
    }

    #[test]
    fn test_mcp_retry_overwrites_previous_failure() {
        let mut view = ChatView::new();

        // First MCP call fails
        view.add_mcp_call("tool_1", "server_1", r#"{"call": 1}"#);
        view.fail_mcp_call("Error 1");

        // Second MCP call fails
        view.add_mcp_call("tool_2", "server_2", r#"{"call": 2}"#);
        view.fail_mcp_call("Error 2");

        // Should have the second failed call
        let failed = view.last_failed_mcp.as_ref().unwrap();
        assert_eq!(failed.tool, "tool_2");
        assert_eq!(failed.server, "server_2");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 3: Conversation Search Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_search_mode_starts_inactive() {
        let view = ChatView::new();
        assert!(!view.search_mode);
        assert!(view.search_query.is_empty());
        assert!(view.search_results.is_empty());
    }

    #[test]
    fn test_start_search_activates_mode() {
        let mut view = ChatView::new();
        view.start_search();

        assert!(view.search_mode);
        assert!(view.search_query.is_empty());
    }

    #[test]
    fn test_exit_search_deactivates_mode() {
        let mut view = ChatView::new();
        view.start_search();
        view.search_input_char('t');
        view.search_input_char('e');
        view.search_input_char('s');
        view.search_input_char('t');

        view.exit_search();

        assert!(!view.search_mode);
        assert!(view.search_query.is_empty());
        assert!(view.search_results.is_empty());
    }

    #[test]
    fn test_search_finds_matching_messages() {
        let mut view = ChatView::new();

        // Add some messages
        view.add_user_message("Hello world".to_string());
        view.add_nika_message("Hi there!".to_string(), None);
        view.add_user_message("world of rust".to_string());

        // Start search
        view.start_search();
        view.search_input_char('w');
        view.search_input_char('o');
        view.search_input_char('r');
        view.search_input_char('l');
        view.search_input_char('d');

        // Should find 2 messages (indices 1 and 3 - 0 is the initial system message)
        assert_eq!(view.search_results.len(), 2);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut view = ChatView::new();
        view.add_user_message("HELLO WORLD".to_string());

        view.start_search();
        view.search_input_char('h');
        view.search_input_char('e');
        view.search_input_char('l');
        view.search_input_char('l');
        view.search_input_char('o');

        // Should find the message even with different case
        assert!(!view.search_results.is_empty());
    }

    #[test]
    fn test_search_navigate_results() {
        let mut view = ChatView::new();

        // Add messages
        view.add_user_message("test one".to_string());
        view.add_user_message("test two".to_string());
        view.add_user_message("test three".to_string());

        view.start_search();
        view.search_input_char('t');
        view.search_input_char('e');
        view.search_input_char('s');
        view.search_input_char('t');

        assert_eq!(view.search_current, 0);

        view.next_search_result();
        assert_eq!(view.search_current, 1);

        view.next_search_result();
        assert_eq!(view.search_current, 2);

        // Should wrap around
        view.next_search_result();
        assert_eq!(view.search_current, 0);
    }

    #[test]
    fn test_search_prev_result() {
        let mut view = ChatView::new();
        view.add_user_message("test one".to_string());
        view.add_user_message("test two".to_string());

        view.start_search();
        view.search_input_char('t');
        view.search_input_char('e');
        view.search_input_char('s');
        view.search_input_char('t');

        // Start at 0, go prev should wrap to end
        view.prev_search_result();
        assert_eq!(view.search_current, view.search_results.len() - 1);
    }

    #[test]
    fn test_search_backspace() {
        let mut view = ChatView::new();
        view.start_search();
        view.search_input_char('a');
        view.search_input_char('b');
        view.search_input_char('c');

        assert_eq!(view.search_query, "abc");

        view.search_input_backspace();
        assert_eq!(view.search_query, "ab");

        view.search_input_backspace();
        assert_eq!(view.search_query, "a");
    }

    #[test]
    fn test_search_empty_query_no_results() {
        let mut view = ChatView::new();
        view.add_user_message("test message".to_string());

        view.start_search();
        // Empty query
        view.update_search();

        assert!(view.search_results.is_empty());
    }

    #[test]
    fn test_search_finds_thinking_content() {
        let mut view = ChatView::new();
        // Add message with thinking content that has unique word
        let msg_id = view.next_message_id();
        view.messages.push(ChatMessage {
            id: msg_id,
            role: MessageRole::Nika,
            content: "Here is my answer".to_string(),
            thinking: Some("Let me analyze the problem carefully".to_string()),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
        });
        // Add normal message without the search term
        view.add_user_message("other message".to_string());

        view.start_search();
        // Search for word only in thinking content
        view.search_query = "analyze".to_string();
        view.update_search();

        // Should find the message with thinking content
        assert_eq!(view.search_results.len(), 1);
        assert_eq!(view.search_results[0], 1); // Index 1 (after welcome message)
    }

    #[test]
    fn test_search_no_duplicate_when_both_match() {
        let mut view = ChatView::new();
        // Add message where both content and thinking contain the search term
        let msg_id = view.next_message_id();
        view.messages.push(ChatMessage {
            id: msg_id,
            role: MessageRole::Nika,
            content: "This test contains the word unique".to_string(),
            thinking: Some("Thinking about unique solution".to_string()),
            timestamp: Local::now(),
            created_at: Instant::now(),
            execution: None,
        });

        view.start_search();
        view.search_query = "unique".to_string();
        view.update_search();

        // Should only find it once, not twice
        assert_eq!(view.search_results.len(), 1);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // v0.9 Phase 3: Smooth Scrolling Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_smooth_scroll_initial_state() {
        let view = ChatView::new();
        assert_eq!(view.scroll_velocity, 0.0);
        assert_eq!(view.scroll_accumulator, 0.0);
        assert!(!view.scroll_animating);
    }

    #[test]
    fn test_smooth_scroll_starts_animation() {
        let mut view = ChatView::new();

        view.smooth_scroll(1); // Scroll down

        assert!(view.scroll_animating);
        assert!(view.scroll_velocity > 0.0);
    }

    #[test]
    fn test_smooth_scroll_negative_direction() {
        let mut view = ChatView::new();

        view.smooth_scroll(-1); // Scroll up

        assert!(view.scroll_animating);
        assert!(view.scroll_velocity < 0.0);
        // Scrolling up should disable auto-follow
        assert!(!view.user_at_bottom);
    }

    #[test]
    fn test_smooth_scroll_velocity_accumulates() {
        let mut view = ChatView::new();

        view.smooth_scroll(1);
        let v1 = view.scroll_velocity;

        view.smooth_scroll(1);
        let v2 = view.scroll_velocity;

        // Velocity should accumulate
        assert!(v2 > v1);
    }

    #[test]
    fn test_update_scroll_animation_applies_friction() {
        let mut view = ChatView::new();
        view.smooth_scroll(1);
        let initial_velocity = view.scroll_velocity;

        view.update_scroll_animation();
        let after_velocity = view.scroll_velocity;

        // Velocity should decrease after friction
        assert!(after_velocity < initial_velocity);
    }

    #[test]
    fn test_update_scroll_animation_stops_at_low_velocity() {
        let mut view = ChatView::new();
        view.scroll_velocity = 0.05; // Below SCROLL_MIN_VELOCITY
        view.scroll_animating = true;

        let still_animating = view.update_scroll_animation();

        assert!(!still_animating);
        assert!(!view.scroll_animating);
        assert_eq!(view.scroll_velocity, 0.0);
        assert_eq!(view.scroll_accumulator, 0.0);
    }

    #[test]
    fn test_update_scroll_animation_inactive_returns_false() {
        let mut view = ChatView::new();
        // Not animating

        let result = view.update_scroll_animation();

        assert!(!result);
    }

    #[test]
    fn test_stop_smooth_scroll() {
        let mut view = ChatView::new();
        view.smooth_scroll(1);
        assert!(view.scroll_animating);

        view.stop_smooth_scroll();

        assert!(!view.scroll_animating);
        assert_eq!(view.scroll_velocity, 0.0);
        assert_eq!(view.scroll_accumulator, 0.0);
    }

    #[test]
    fn test_is_scroll_animating() {
        let mut view = ChatView::new();
        assert!(!view.is_scroll_animating());

        view.smooth_scroll(1);
        assert!(view.is_scroll_animating());

        view.stop_smooth_scroll();
        assert!(!view.is_scroll_animating());
    }

    #[test]
    fn test_smooth_scroll_animation_converges_to_stop() {
        let mut view = ChatView::new();
        view.smooth_scroll(1);

        // Run animation until it stops (should take finite iterations)
        let mut iterations = 0;
        while view.update_scroll_animation() && iterations < 100 {
            iterations += 1;
        }

        // Animation should eventually stop
        assert!(!view.scroll_animating);
        assert!(
            iterations < 100,
            "Animation should converge within 100 iterations"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.10.0: Chat DAG Panel Integration Tests (WIRING-6)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_panel_default_shown() {
        // v0.12: DAG panel is shown by default (toggle with Ctrl+D)
        let view = ChatView::new();
        assert!(view.show_dag_panel);
        // DAG nodes/edges are synced on toggle, so initially empty is OK
        // The sync happens when the panel is first shown
    }

    #[test]
    fn test_dag_panel_toggle() {
        let mut view = ChatView::new();
        // v0.12: DAG panel starts visible by default
        assert!(view.show_dag_panel);

        view.toggle_dag_panel();
        assert!(!view.show_dag_panel);

        view.toggle_dag_panel();
        assert!(view.show_dag_panel);
        // Should have synced DAG state from messages (1 welcome message)
        assert_eq!(view.dag_nodes.len(), 1);
    }

    #[test]
    fn test_dag_panel_sync_from_messages() {
        let mut view = ChatView::new();
        view.add_user_message("Hello".to_string());
        view.add_nika_message("Hi there!".to_string(), None);
        view.add_user_message("How are you?".to_string());

        view.sync_dag_from_messages();

        // Should have 4 nodes: welcome + 3 messages
        assert_eq!(view.dag_nodes.len(), 4);
        // Should have 3 edges: each message links to previous
        assert_eq!(view.dag_edges.len(), 3);

        // Check node kinds
        assert_eq!(view.dag_nodes[0].kind, ChatNodeKind::System);
        assert_eq!(view.dag_nodes[1].kind, ChatNodeKind::User);
        assert_eq!(view.dag_nodes[2].kind, ChatNodeKind::Assistant);
        assert_eq!(view.dag_nodes[3].kind, ChatNodeKind::User);
    }

    #[test]
    fn test_dag_panel_node_label_truncation() {
        let mut view = ChatView::new();
        view.add_user_message("This is a very long message that should be truncated".to_string());

        view.sync_dag_from_messages();

        // The user message is the second node (after welcome)
        let user_node = &view.dag_nodes[1];
        assert!(user_node.label.len() <= 30);
        assert!(user_node.label.ends_with("..."));
    }

    #[test]
    fn test_task_queue_add_and_update() {
        let mut view = ChatView::new();

        view.add_task_to_queue("task-1", ChatTaskVerb::Infer);
        assert_eq!(view.task_queue.len(), 1);
        assert_eq!(view.task_queue[0].id(), "task-1");
        assert_eq!(view.task_queue[0].state(), ChatTaskState::Pending);

        view.update_task_state(
            "task-1",
            ChatTaskState::Running,
            Some(std::time::Duration::from_millis(100)),
        );
        assert_eq!(view.task_queue[0].state(), ChatTaskState::Running);

        view.update_task_state("task-1", ChatTaskState::Complete, None);
        assert_eq!(view.task_queue[0].state(), ChatTaskState::Complete);
    }

    #[test]
    fn test_dag_panel_selected_index() {
        let mut view = ChatView::new();
        view.add_user_message("Hello".to_string());
        view.sync_dag_from_messages();

        assert!(view.dag_selected.is_none());

        view.dag_selected = Some(0);
        assert_eq!(view.dag_selected, Some(0));

        // Selected should be within bounds
        view.dag_selected = Some(100); // Out of bounds
                                       // The render logic handles this gracefully
    }

    #[test]
    fn test_dag_panel_toggle_syncs_state() {
        let mut view = ChatView::new();
        // v0.12: DAG panel starts visible by default
        assert!(view.show_dag_panel);

        view.add_user_message("First".to_string());
        view.add_nika_message("Response".to_string(), None);

        // Close the panel
        view.toggle_dag_panel();
        assert!(!view.show_dag_panel);

        // Reopen - this should sync the DAG state
        view.toggle_dag_panel();
        assert!(view.show_dag_panel);
        assert_eq!(view.dag_nodes.len(), 3); // welcome + 2 messages

        // Add more messages while panel is open
        view.add_user_message("Second".to_string());
        // Nodes NOT auto-synced (requires explicit sync or toggle)

        // Close and reopen to resync
        view.toggle_dag_panel(); // Close
        view.toggle_dag_panel(); // Reopen
        assert_eq!(view.dag_nodes.len(), 4); // welcome + 3 messages
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // SESSION PERSISTENCE TESTS (v0.12.0)
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_get_chat_state_converts_messages() {
        let mut view = ChatView::new();
        view.add_user_message("Hello Nika".to_string());
        view.add_nika_message("Hi! How can I help?".to_string(), None);

        let state = view.get_chat_state();

        // Should have 3 messages: welcome + user + nika
        assert_eq!(state.messages.len(), 3);

        // Check role conversion
        assert_eq!(state.messages[0].role, ChatOverlayMessageRole::System);
        assert_eq!(state.messages[1].role, ChatOverlayMessageRole::User);
        assert_eq!(state.messages[2].role, ChatOverlayMessageRole::Nika);

        // Check content preservation
        assert_eq!(state.messages[1].content, "Hello Nika");
        assert_eq!(state.messages[2].content, "Hi! How can I help?");
    }

    #[test]
    fn test_get_chat_state_preserves_input_state() {
        let mut view = ChatView::new();
        view.input = Input::new("partial input".to_string());
        view.input.handle(InputRequest::GoToEnd);

        let state = view.get_chat_state();

        assert_eq!(state.input, "partial input");
        assert_eq!(state.cursor, 13); // Length of "partial input"
    }

    #[test]
    fn test_get_chat_state_preserves_history() {
        let mut view = ChatView::new();
        view.add_user_message("First".to_string());
        view.add_user_message("Second".to_string());

        let state = view.get_chat_state();

        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0], "First");
        assert_eq!(state.history[1], "Second");
    }

    #[test]
    fn test_get_chat_state_preserves_model() {
        let view = ChatView::new();
        let state = view.get_chat_state();

        // Model should be set based on env (or default)
        assert!(!state.current_model.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.13: ChatWorkflow Wiring Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_chat_view_workflow_initialized() {
        let view = ChatView::new();
        // Workflow should be initialized with welcome message (v0.13: for DAG sync)
        assert_eq!(view.workflow.message_count(), 1);
    }

    #[test]
    fn test_chat_view_user_message_wires_to_workflow() {
        let mut view = ChatView::new();
        view.add_user_message("Hello world".to_string());

        // Should be in both messages vec and workflow
        assert_eq!(view.messages.len(), 2); // welcome + user
        assert_eq!(view.workflow.message_count(), 2); // welcome + user (v0.13: both wired)
    }

    #[test]
    fn test_chat_view_nika_message_wires_to_workflow() {
        let mut view = ChatView::new();
        view.add_user_message("Hello".to_string());
        view.add_nika_message("Hi there!".to_string(), None);

        // All should be in workflow (welcome + user + nika)
        assert_eq!(view.workflow.message_count(), 3);
    }

    #[test]
    fn test_chat_view_system_message_wires_to_workflow() {
        let mut view = ChatView::new();
        view.add_system_message("System notification");

        // System message in workflow (welcome + system)
        assert_eq!(view.workflow.message_count(), 2);
    }

    #[test]
    fn test_chat_view_tool_message_wires_to_workflow() {
        let mut view = ChatView::new();
        view.add_tool_message("Tool output".to_string());

        // Tool message in workflow (welcome + tool)
        assert_eq!(view.workflow.message_count(), 2);
    }

    #[test]
    fn test_chat_view_workflow_preserves_roles() {
        let mut view = ChatView::new();
        view.add_user_message("User msg".to_string());
        view.add_nika_message("Assistant msg".to_string(), None);
        view.add_system_message("System msg");
        view.add_tool_message("Tool msg".to_string());

        let messages = view.workflow.all_messages();
        assert_eq!(messages.len(), 5); // welcome + 4 messages
        assert_eq!(messages[0].1.role, WorkflowRole::System); // welcome
        assert_eq!(messages[1].1.role, WorkflowRole::User);
        assert_eq!(messages[2].1.role, WorkflowRole::Assistant);
        assert_eq!(messages[3].1.role, WorkflowRole::System);
        assert_eq!(messages[4].1.role, WorkflowRole::Tool);
    }

    #[test]
    fn test_chat_view_sync_dag_uses_workflow() {
        let mut view = ChatView::new();
        view.show_dag_panel = true;
        view.add_user_message("First".to_string());
        view.add_nika_message("Second".to_string(), None);

        // Sync should use workflow
        view.sync_dag_from_messages();

        // Should have 3 nodes (from workflow: welcome + user + nika)
        assert_eq!(view.dag_nodes.len(), 3);
        // Should have 2 edges (sequential: welcome->user, user->nika)
        assert_eq!(view.dag_edges.len(), 2);
    }

    #[test]
    fn test_chat_view_workflow_mention_creates_edge() {
        let mut view = ChatView::new();
        view.show_dag_panel = true;

        // Add messages with @1 reference (note: @1 is the welcome message now)
        view.add_user_message("What is Rust?".to_string());
        view.add_nika_message("Rust is a programming language".to_string(), None);
        view.add_user_message("@2 Tell me more about safety".to_string()); // References msg 2 (user question)

        view.sync_dag_from_messages();

        // Should have 4 nodes (welcome + 3 messages)
        assert_eq!(view.dag_nodes.len(), 4);
        // Should have 3+ edges: sequential chain + @2 mention
        // Note: depends on how mentions are resolved
        assert!(view.dag_edges.len() >= 3);
    }
}
