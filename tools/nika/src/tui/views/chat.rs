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
use std::time::Instant;

use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Widget,
    },
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
use crate::tui::file_resolve::FileResolver;
use crate::tui::state::{ChatPanel, PanelScrollState, TuiState};
use crate::tui::theme::Theme;
use crate::util::atomic_write;

// PERF: Pre-computed constants to avoid allocations in render loop
const SEPARATOR_20: &str = "────────────────────"; // 20 Unicode box chars (─), compile-time
const SEPARATOR_20_ASCII: &str = "--------------------"; // 20 ASCII dashes (-), compile-time
const SEPARATOR_52: &str = "╰───────────────────────────────────────────────────╯"; // MCP box bottom
use crate::tui::utils::{truncate_str, wrap_text};
use crate::tui::views::TuiView;
use crate::tui::widgets::{
    ActivityItem, ActivityTemp, ChatModeIndicator, CommandPalette, CommandPaletteState,
    ContextItem, CurrentVerb, DecryptVerb, HelpOverlay, HelpOverlayState, InferStreamData,
    McpCallData, McpCallStatus, McpServerInfo, McpStatus, MemoryFile, MissionControlPanel,
    ParsedInput, ProStatusBar, Provider, ProviderSelector, ProviderSelectorState, SessionContext,
    SessionMetrics, StreamingDecrypt, SystemCommand, TurnMetrics,
};

// ═══════════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks when theme doesn't have specific fields)
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_THINKING_HEADER_COLOR: Color = Color::Rgb(245, 158, 11); // amber
const DEFAULT_THINKING_CONTENT_COLOR: Color = Color::Rgb(156, 163, 175); // gray-400
const DEFAULT_MUTED_COLOR: Color = Color::Rgb(107, 114, 128); // gray-500
const DEFAULT_MCP_BOX_COLOR: Color = Color::Rgb(16, 185, 129); // Emerald
const DEFAULT_SUCCESS_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_ERROR_COLOR: Color = Color::Rgb(239, 68, 68); // red
const DEFAULT_INFER_BOX_COLOR: Color = Color::Rgb(139, 92, 246); // Violet
const DEFAULT_STATUS_RUNNING_COLOR: Color = Color::Rgb(250, 204, 21); // Yellow

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
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Instant,
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
    /// Provider selector state (⌘P - v0.7.2)
    pub provider_selector: ProviderSelectorState,
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
    /// Scrollbar state for conversation panel
    pub scrollbar_conversation: ScrollbarState,
    /// Scrollbar state for activity panel
    pub scrollbar_activity: ScrollbarState,
    /// Cached panel rects for mouse click detection
    pub panel_rects: std::collections::HashMap<ChatPanel, Rect>,
    /// List state for conversation (ratatui StatefulWidget)
    pub conversation_list_state: ListState,

    // === v0.8 WOW Effects ===
    /// Index of last copied message (for flash effect)
    pub copy_flash_index: Option<usize>,
    /// Frame when copy happened (for flash duration)
    pub copy_flash_start: u8,
    /// Matrix decrypt effect for streaming text (v0.8 WOW)
    pub streaming_decrypt: StreamingDecrypt,
    /// Whether matrix decrypt effect is enabled
    pub matrix_effect_enabled: bool,

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
        // Detect initial model and provider from environment
        let (initial_model, provider_name) = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            ("claude-sonnet-4".to_string(), "Claude".to_string())
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            ("gpt-4o".to_string(), "OpenAI".to_string())
        } else {
            ("No API Key".to_string(), "None".to_string())
        };

        // Initialize session context with detected MCP servers
        let mut session_context = SessionContext::new();
        session_context
            .mcp_servers
            .push(McpServerInfo::new("novanet"));

        Self {
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content:
                    "Welcome to Nika Agent. Type a message to chat, or use /help for commands."
                        .to_string(),
                thinking: None,
                timestamp: Instant::now(),
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
            provider_selector: ProviderSelectorState::new(),
            inline_content: vec![],
            frame: 0,

            // Chat Mode Indicators (v2.1)
            chat_mode: ChatMode::default(),
            deep_thinking: false,
            provider_name,

            // Thinking Accumulation (v0.5.2)
            pending_thinking: None,

            // UX Hints (v0.7.1)
            shown_file_hint: false,

            // Panel Navigation & Scroll (v0.8 UX Enhancement)
            focused_panel: ChatPanel::Input, // Start with input focused (typing)
            conversation_scroll: PanelScrollState::new(),
            activity_scroll: PanelScrollState::new(),
            scrollbar_conversation: ScrollbarState::default(),
            scrollbar_activity: ScrollbarState::default(),
            panel_rects: std::collections::HashMap::new(),
            conversation_list_state: ListState::default(),

            // v0.8 WOW Effects
            copy_flash_index: None,
            copy_flash_start: 0,
            streaming_decrypt: StreamingDecrypt::new()
                .with_verb(DecryptVerb::Infer)
                .with_reveal_speed(0.08), // ~12 frames to reveal
            matrix_effect_enabled: true, // Enable by default

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

        // Vertical layout: session bar, main content, input, hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Session context bar
                Constraint::Min(10),   // Main content area
                Constraint::Length(3), // Input field
                Constraint::Length(1), // Command hints
            ])
            .split(area);

        // Horizontal split for main content: messages (70%) | activity (30%)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
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
            // Scroll wheel up - scroll focused panel (no panel switch)
            MouseEventKind::ScrollUp => {
                self.scroll_up();
                true
            }
            // Scroll wheel down - scroll focused panel (no panel switch)
            MouseEventKind::ScrollDown => {
                self.scroll_down();
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
        // v0.8.1 WOW: Reset and configure matrix decrypt for verb theme
        // Parameters tuned for visible chaos + cascade reveal effect
        self.streaming_decrypt = StreamingDecrypt::new()
            .with_verb(verb)
            .with_reveal_speed(0.025) // Slow reveal (~40 frames = 667ms at 60fps)
            .with_wave_factor(0.15) // Cascade: later chars reveal slower
            .with_initial_chaos(8); // ~130ms of visible chaos before reveal
    }

    /// Append chunk to partial response during streaming
    pub fn append_streaming(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
        // v0.8 WOW: Push to matrix decrypt for reveal effect
        if self.matrix_effect_enabled {
            self.streaming_decrypt.push_text(chunk);
        }
        // v0.8.1: Auto-scroll to follow streaming content
        self.auto_scroll_to_bottom();
    }

    /// Finish streaming and return the full response
    pub fn finish_streaming(&mut self) -> String {
        self.is_streaming = false;
        // v0.8 WOW: Reveal all remaining text instantly
        self.streaming_decrypt.reveal_all();
        // v0.8 FIX: Clear inline boxes when streaming completes
        // They represent the operation that just finished, not history
        self.inline_content.clear();
        std::mem::take(&mut self.partial_response)
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

        // Clear current messages and load from session
        self.messages.clear();
        for msg in session.messages {
            self.messages.push(ChatMessage {
                role: msg.role.into(),
                content: msg.content,
                timestamp: Instant::now(), // Use current time since original is lost
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

    /// Add a tool message
    pub fn add_tool_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::Tool,
            content,
            timestamp: Instant::now(),
            execution: None,
            thinking: None,
        });
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
            }
        }
        // v0.8 WOW: Tick flash effects
        self.tick_flash();
        // v0.8 WOW: Tick matrix decrypt animation (reveal progress)
        if self.is_streaming && self.matrix_effect_enabled {
            self.streaming_decrypt.tick();
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
    }

    /// Complete an MCP call with result
    pub fn complete_mcp_call(&mut self, result: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            data.result = Some(result.to_string());
            data.status = McpCallStatus::Success;
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("invoke");
    }

    /// Fail an MCP call with error
    pub fn fail_mcp_call(&mut self, error: &str) {
        if let Some(InlineContent::McpCall(data)) = self.inline_content.last_mut() {
            data.error = Some(error.to_string());
            data.status = McpCallStatus::Failed;
        }
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
    pub fn append_infer_content(&mut self, chunk: &str, tokens_out: u32) {
        if let Some(InlineContent::InferStream(data)) = self.inline_content.last_mut() {
            data.append_content(chunk);
            data.update_tokens(tokens_out);
        }
        // Also update the partial response for backwards compatibility
        self.partial_response.push_str(chunk);
        // v0.8.1: Auto-scroll to follow streaming content
        self.auto_scroll_to_bottom();
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

    /// Toggle provider selector visibility (⌘P)
    pub fn toggle_provider_selector(&mut self) {
        self.provider_selector.toggle();
    }

    /// Transition activity from hot to warm
    fn transition_activity_to_warm(&mut self, verb: &str) {
        if let Some(item) = self
            .activity_items
            .iter_mut()
            .find(|i| i.verb == verb && i.temp == ActivityTemp::Hot)
        {
            item.temp = ActivityTemp::Warm;
            item.duration = item.elapsed();
        }
    }

    /// Clear completed (warm) activities older than duration
    pub fn clear_old_activities(&mut self, max_age_secs: u64) {
        use std::time::Duration;
        self.activity_items.retain(|item| {
            item.temp != ActivityTemp::Warm
                || item
                    .duration
                    .map(|d| d < Duration::from_secs(max_age_secs))
                    .unwrap_or(true)
        });
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.clone(),
            timestamp: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.history.push(content);
        self.history_index = None;
        // v0.8.1: When user sends a message, they want to see the response
        self.user_at_bottom = true;
        self.auto_scroll_to_bottom();
    }

    /// Add a Nika response
    pub fn add_nika_message(&mut self, content: String, execution: Option<ExecutionResult>) {
        self.messages.push(ChatMessage {
            role: MessageRole::Nika,
            content,
            timestamp: Instant::now(),
            execution,
            thinking: None,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
    }

    /// Add a Nika response with thinking content (v0.5.2+)
    pub fn add_nika_message_with_thinking(
        &mut self,
        content: String,
        thinking: Option<String>,
        execution: Option<ExecutionResult>,
    ) {
        self.messages.push(ChatMessage {
            role: MessageRole::Nika,
            content,
            timestamp: Instant::now(),
            execution,
            thinking,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
    }

    /// Add a system message (for mode changes, status updates)
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatMessage {
            role: MessageRole::System,
            content: content.into(),
            timestamp: Instant::now(),
            execution: None,
            thinking: None,
        });
        self.auto_scroll_to_bottom(); // v0.8 FIX: Auto-scroll on new message
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
                }
            }
            Some(_) => {
                self.history_index = None;
                self.input.reset();
            }
            None => {}
        }
    }

    /// Insert character at cursor (delegates to tui-input)
    pub fn insert_char(&mut self, c: char) {
        self.input.handle(InputRequest::InsertChar(c));
    }

    /// Delete character before cursor (delegates to tui-input)
    pub fn backspace(&mut self) {
        self.input.handle(InputRequest::DeletePrevChar);
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

        // Messages panel with inline MCP/Infer boxes
        self.render_messages_v2(frame, main_chunks[0], theme);

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

        // 3. Input panel
        self.render_input(frame, chunks[2], theme);

        // 4. Command hints
        self.render_hints(frame, chunks[3], theme);

        // 5. Command palette overlay (if visible)
        if self.command_palette.visible {
            let palette_area = centered_rect(60, 50, area);
            CommandPalette::new(&self.command_palette).render(palette_area, frame.buffer_mut());
        }

        // 6. Provider selector overlay (if visible) - ⌘P
        if self.provider_selector.visible {
            let selector_area = centered_rect(70, 60, area);
            ProviderSelector::new(&self.provider_selector)
                .render(selector_area, frame.buffer_mut());
        }

        // 7. Help overlay (if visible) - ? or F1
        if self.help_overlay.visible {
            let help_area = centered_rect(70, 80, area);
            HelpOverlay::new(&self.help_overlay).render(help_area, frame.buffer_mut());
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

        // Handle provider selector when visible
        if self.provider_selector.visible {
            return self.handle_provider_selector_key(key);
        }

        // Check for Cmd/Ctrl+K (command palette toggle)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('k') {
            self.toggle_command_palette();
            return ViewAction::None;
        }

        // Check for Cmd/Ctrl+P (provider selector toggle)
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('p') {
            self.toggle_provider_selector();
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
        // Panel Navigation (v0.8 UX Enhancement)
        // Tab/Shift+Tab ALWAYS cycle panels: Conversation → Activity → Input → ...
        // ═══════════════════════════════════════════════════════════════════════════════

        // Tab = Focus next panel (always works, even in Input panel)
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.focus_next_panel();
            return ViewAction::None;
        }

        // Shift+Tab = Focus previous panel (always works)
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.focus_prev_panel();
            return ViewAction::None;
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
            KeyCode::Char('q') if self.input.value().is_empty() => ViewAction::Quit,
            // 's' when empty opens Settings view (consistent with other views)
            KeyCode::Char('s') if self.input.value().is_empty() => ViewAction::OpenSettings,
            // Shift+T toggles theme (v0.8.1 - consistent with app.rs)
            KeyCode::Char('T') => ViewAction::ToggleTheme,
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
            // Esc switches to Home view (when in Input panel)
            KeyCode::Esc => ViewAction::SwitchView(TuiView::Home),
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

    /// Handle key events when provider selector is visible
    fn handle_provider_selector_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.provider_selector.close();
                ViewAction::None
            }
            KeyCode::Enter => {
                // Get selected provider and model
                let provider = self.provider_selector.selected_provider();
                let model = self.provider_selector.selected_model();

                if let Some(model_info) = model {
                    // Update current model
                    self.current_model = model_info.id.clone();
                    self.cached_provider = Provider::from_model_name(&self.current_model);
                    self.provider_name = provider.name.clone();

                    // Add system message
                    let streaming_indicator = if model_info.streaming { "⚡" } else { "📄" };
                    let thinking_indicator = if model_info.thinking { " 🧠" } else { "" };
                    self.add_system_message(format!(
                        "{} {} Switched to {} {}{}",
                        streaming_indicator,
                        provider.icon,
                        provider.name,
                        model_info.name,
                        thinking_indicator
                    ));
                }

                self.provider_selector.close();
                ViewAction::None
            }
            KeyCode::Up => {
                self.provider_selector.move_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.provider_selector.move_down();
                ViewAction::None
            }
            KeyCode::Left => {
                // Exit model mode back to provider mode
                self.provider_selector.model_mode = false;
                self.provider_selector.selected_model = 0;
                ViewAction::None
            }
            KeyCode::Right | KeyCode::Tab => {
                // Enter model mode
                self.provider_selector.enter_model_mode();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
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

            // Add thinking indicator with animation
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

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CONVERSATION ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(list, area);
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

    /// Render messages v2 with inline MCP/Infer boxes
    fn render_messages_v2(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
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

            // Account for thinking lines if present
            if let Some(ref thinking) = msg.thinking {
                current_line += 1; // "🧠 Thinking:" header
                let think_lines = thinking.lines().take(3).count();
                current_line += think_lines;
                if thinking.lines().count() > 3 {
                    current_line += 1; // "... (N more lines)"
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

                // v0.8 WOW: Add COPIED indicator when flashing
                let mut header_spans = vec![
                    Span::styled(prefix_with_space, style.add_modifier(Modifier::BOLD)),
                    Span::styled(SEPARATOR_20, Style::default().fg(theme.text_muted)),
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
                // Use wrap_text for proper word wrapping
                let wrapped_lines = wrap_text(&msg.content, content_width);

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

                // Add thinking display if present (v0.5.2+)
                if let Some(ref thinking) = msg.thinking {
                    // Thinking indicator header
                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(color)),
                        Span::styled(
                            "🧠 Thinking:",
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
            }
        }

        // Add streaming indicator if streaming is in progress
        if self.is_streaming && self.inline_content.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "🤖 AI ",
                    Style::default()
                        .fg(theme.status_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(SEPARATOR_20, Style::default().fg(theme.text_muted)),
            ])));

            if !self.partial_response.is_empty() {
                // v0.8.1 FIX: Use content_width for word wrapping to prevent overflow
                // v0.8 WOW: Use matrix decrypt effect if enabled
                if self.matrix_effect_enabled {
                    for decrypt_line in self.streaming_decrypt.build_lines_wrapped(content_width) {
                        // Prepend the prefix to each line
                        let mut spans = vec![Span::styled(
                            "│ ",
                            Style::default().fg(theme.status_success),
                        )];
                        spans.extend(decrypt_line.spans);
                        items.push(ListItem::new(Line::from(spans)));
                    }
                } else {
                    // Fallback: plain text rendering with word wrap (v0.8.1)
                    let wrapped_lines = wrap_text(&self.partial_response, content_width);
                    for line in wrapped_lines {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(theme.status_success)),
                            Span::raw(line),
                        ])));
                    }
                }
            }

            // v0.8 WOW: Enhanced animated thinking indicator with typing wave
            let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
            let spinner = spinners[(self.frame as usize) % spinners.len()];

            // Typing wave effect: ●○○ → ○●○ → ○○● → ○○○ → ...
            let wave_pos = (self.frame as usize / 3) % 5;
            let wave: String = (0..3)
                .map(|i| if i == wave_pos % 3 { "●" } else { "○" })
                .collect::<Vec<_>>()
                .join("");

            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.status_success)),
                Span::styled(
                    format!("{} ", spinner),
                    Style::default().fg(theme.status_running),
                ),
                Span::styled(
                    "Generating",
                    Style::default()
                        .fg(theme.status_running)
                        .add_modifier(Modifier::ITALIC),
                ),
                Span::styled(format!(" {} ", wave), Style::default().fg(theme.highlight)),
            ])));
        }

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

        // v0.8.1 UX: Render styled scrollbar if content exceeds viewport
        // Uses Solarized-inspired colors from theme for consistent look
        if total_items > visible_count {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲")) // Nicer Unicode arrow
                .end_symbol(Some("▼")) // Nicer Unicode arrow
                .track_symbol(Some("┃")) // Bold vertical line
                .thumb_symbol("█")
                // v0.8.1: Solarized styled colors
                .style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track));

            // Compute scrollbar state (NovaNet pattern: content_length is total - visible)
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(total_items.saturating_sub(visible_count))
                .position(clamped_offset);

            // Render inside the block area (excluding borders)
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    /// Render command hints bar
    fn render_hints(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
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
                " Tab ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" view  "),
            Span::styled(
                " Esc ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" back  "),
            Span::styled(
                " ↑↓ ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" hist"),
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
            role: MessageRole::Nika,
            content: "Here's my answer.".to_string(),
            timestamp: Instant::now(),
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
}
