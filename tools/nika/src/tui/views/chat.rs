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

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::command::{Command, ModelProvider, HELP_TEXT};
use crate::tui::file_resolve::FileResolver;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;
use crate::tui::widgets::{
    ActivityItem, ActivityStack, ActivityTemp, CommandPalette, CommandPaletteState,
    InferStreamData, McpCallData, McpCallStatus, McpServerInfo, McpStatus, SessionContext,
    SessionContextBar,
};

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
    /// Current input buffer
    pub input: String,
    /// Input cursor position
    pub cursor: usize,
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

    // === Chat UX Enrichment (v2) ===
    /// Session context with tokens, cost, MCP status
    pub session_context: SessionContext,
    /// Activity stack items (hot/warm/queued)
    pub activity_items: Vec<ActivityItem>,
    /// Command palette state (⌘K)
    pub command_palette: CommandPaletteState,
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
            input: String::new(),
            cursor: 0,
            scroll: 0,
            history: vec![],
            history_index: None,
            is_streaming: false,
            partial_response: String::new(),
            current_model: initial_model,

            // Chat UX Enrichment (v2)
            session_context,
            activity_items: vec![],
            command_palette: CommandPaletteState::new(),
            inline_content: vec![],
            frame: 0,

            // Chat Mode Indicators (v2.1)
            chat_mode: ChatMode::default(),
            deep_thinking: false,
            provider_name,

            // Thinking Accumulation (v0.5.2)
            pending_thinking: None,
        }
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

    /// Start streaming mode
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.partial_response.clear();
    }

    /// Append chunk to partial response during streaming
    pub fn append_streaming(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
    }

    /// Finish streaming and return the full response
    pub fn finish_streaming(&mut self) -> String {
        self.is_streaming = false;
        std::mem::take(&mut self.partial_response)
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

    /// Set the current model name
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.current_model = model.into();
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
    pub fn start_infer_stream(&mut self, model: &str, tokens_in: u32, max_tokens: u32) {
        let data = InferStreamData::new(model)
            .with_tokens(tokens_in, 0)
            .with_max_tokens(max_tokens);
        self.inline_content.push(InlineContent::InferStream(data));

        // Add to activity stack as hot
        self.activity_items.push(ActivityItem::hot(
            format!("infer-{}", self.inline_content.len()),
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
    }

    /// Complete current inference stream
    pub fn complete_infer_stream(&mut self) {
        if let Some(InlineContent::InferStream(data)) = self.inline_content.last_mut() {
            data.complete();
        }
        // Move activity from hot to warm
        self.transition_activity_to_warm("infer");
    }

    /// Update session token usage
    pub fn update_tokens(&mut self, tokens_used: u64, cost: f64) {
        self.session_context.tokens_used = tokens_used;
        self.session_context.total_cost = cost;
    }

    /// Toggle command palette visibility
    pub fn toggle_command_palette(&mut self) {
        self.command_palette.toggle();
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
    }

    /// Append text to the last message (for streaming tokens)
    ///
    /// Used for Claude Code-like streaming where tokens appear in real-time.
    /// If the last message is "Thinking...", it will be replaced.
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
    pub fn submit(&mut self) -> Option<String> {
        if self.input.trim().is_empty() {
            return None;
        }
        let message = self.input.clone();
        self.add_user_message(message.clone());
        self.input.clear();
        self.cursor = 0;
        Some(message)
    }

    /// Navigate history up
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i - 1);
            }
            _ => {}
        }
        if let Some(i) = self.history_index {
            self.input = self.history[i].clone();
            self.cursor = self.input.chars().count(); // Use char count, not byte len
        }
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(i) if i < self.history.len() - 1 => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
                self.cursor = self.input.chars().count(); // Use char count, not byte len
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    /// Insert character at cursor (cursor is char index, not byte index)
    pub fn insert_char(&mut self, c: char) {
        // Convert char index to byte index for insertion
        let byte_idx = self
            .input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len());
        self.input.insert(byte_idx, c);
        self.cursor += 1;
    }

    /// Delete character before cursor (cursor is char index, not byte index)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            // Convert char index to byte index for removal
            let byte_idx = self
                .input
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(byte_idx);
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right (cursor is char index, not byte index)
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.chars().count() {
            self.cursor += 1;
        }
    }

    /// Scroll up in the message list
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down in the message list (bounded by message count)
    pub fn scroll_down(&mut self) {
        // Cap scroll at message count - 1 (so at least one message is visible)
        let max_scroll = self.messages.len().saturating_sub(1);
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    /// Scroll to bottom (most recent messages)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll = 0; // Scroll 0 = show most recent (bottom)
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ChatView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout v2: Session Context Bar | Messages + Activity Stack | Input + Hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Session context bar (compact)
                Constraint::Min(10),   // Main content area
                Constraint::Length(3), // Input field
                Constraint::Length(1), // Command hints
            ])
            .split(area);

        // 1. Session Context Bar (compact mode at top)
        SessionContextBar::new(&self.session_context)
            .compact()
            .render(chunks[0], frame.buffer_mut());

        // 2. Main content: Messages (70%) | Activity Stack (30%)
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[1]);

        // Messages panel with inline MCP/Infer boxes
        self.render_messages_v2(frame, main_chunks[0], theme);

        // Activity Stack panel
        ActivityStack::new(&self.activity_items)
            .frame(self.frame)
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
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Handle command palette when visible
        if self.command_palette.visible {
            return self.handle_palette_key(key);
        }

        // Check for Ctrl+K (command palette toggle)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.toggle_command_palette();
            return ViewAction::None;
        }

        // Check for Ctrl+T (toggle deep thinking)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') {
            self.toggle_deep_thinking();
            let status = if self.deep_thinking {
                "enabled"
            } else {
                "disabled"
            };
            self.add_system_message(format!("🧠 Deep thinking {}", status));
            return ViewAction::None;
        }

        // Check for Ctrl+M (toggle infer/agent mode)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
            self.toggle_mode();
            self.add_system_message(format!(
                "{} Switched to {} mode",
                self.chat_mode.icon(),
                self.chat_mode.label()
            ));
            return ViewAction::None;
        }

        match key.code {
            KeyCode::Char('q') if self.input.is_empty() => ViewAction::Quit,
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
                                let list_text = format!(
                                    "Available providers:\n  - openai: {} {}\n  - claude: {} {}",
                                    ModelProvider::OpenAI.name(),
                                    if ModelProvider::OpenAI.is_available() {
                                        "(available)"
                                    } else {
                                        "(missing API key)"
                                    },
                                    ModelProvider::Claude.name(),
                                    if ModelProvider::Claude.is_available() {
                                        "(available)"
                                    } else {
                                        "(missing API key)"
                                    },
                                );
                                self.add_nika_message(list_text, None);
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
            KeyCode::Up => {
                self.history_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.history_down();
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
                self.scroll_up();
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.scroll_down();
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
            KeyCode::Tab => ViewAction::SwitchView(TuiView::Home),
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
            "{} messages | {} in history | Model: {}{}",
            self.messages.len(),
            self.history.len(),
            self.current_model,
            streaming_status
        )
    }
}

impl ChatView {
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
                    self.input = format!("/{}", cmd_id);
                    self.cursor = self.input.chars().count();
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

    fn render_messages(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .flat_map(|msg| {
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
                    Span::styled("-".repeat(20), Style::default().fg(theme.text_muted)),
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
                Span::styled("-".repeat(20), Style::default().fg(theme.text_muted)),
            ])));

            // Show partial response if any
            if !self.partial_response.is_empty() {
                for line in self.partial_response.lines() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("  | ", Style::default().fg(theme.status_success)),
                        Span::raw(line.to_string()),
                    ])));
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
        // Show input with cursor (use char-based slicing for unicode safety)
        let before_cursor: String = self.input.chars().take(self.cursor).collect();
        let cursor_char = self.input.chars().nth(self.cursor).unwrap_or(' ');
        let after_cursor: String = self.input.chars().skip(self.cursor + 1).collect();

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

        // Input text with cursor
        spans.push(Span::raw(before_cursor));
        spans.push(Span::styled(
            cursor_char.to_string(),
            Style::default().bg(theme.highlight).fg(Color::Black),
        ));
        spans.push(Span::raw(after_cursor));

        let line = Line::from(spans);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render messages v2 with inline MCP/Infer boxes
    fn render_messages_v2(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .flat_map(|msg| {
                // Color-coded message bubbles based on role
                let (prefix, color) = match msg.role {
                    MessageRole::User => ("👤 You", theme.trait_retrieved),
                    MessageRole::Nika => ("🤖 AI", theme.status_success),
                    MessageRole::System => ("💡 System", theme.status_running),
                    MessageRole::Tool => ("🔧 Tool", theme.mcp_traverse),
                };

                let style = Style::default().fg(color);

                let mut lines = vec![ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", prefix), style.add_modifier(Modifier::BOLD)),
                    Span::styled("─".repeat(20), Style::default().fg(theme.text_muted)),
                ]))];

                // Wrap message content with colored prefix indicator
                for line in msg.content.lines() {
                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(color)),
                        Span::raw(line.to_string()),
                    ])));
                }

                // Add thinking display if present (v0.5.2+)
                if let Some(ref thinking) = msg.thinking {
                    // Thinking indicator header
                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(color)),
                        Span::styled(
                            "🧠 Thinking:",
                            Style::default()
                                .fg(Color::Rgb(245, 158, 11)) // amber
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ])));

                    // Truncate thinking to first 3 lines for inline display
                    let thinking_lines: Vec<&str> = thinking.lines().take(3).collect();
                    for think_line in &thinking_lines {
                        // Truncate each line to 60 chars
                        let display_line = if think_line.len() > 60 {
                            format!("{}...", &think_line[..57])
                        } else {
                            think_line.to_string()
                        };
                        lines.push(ListItem::new(Line::from(vec![
                            Span::styled("│   ", Style::default().fg(color)),
                            Span::styled(
                                display_line,
                                Style::default()
                                    .fg(Color::Rgb(156, 163, 175)) // gray-400
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
                                Style::default().fg(Color::Rgb(107, 114, 128)), // gray-500
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
                            Style::default().fg(Color::Rgb(16, 185, 129)), // Emerald
                        ),
                        Span::styled(
                            format!("{} {} ─╮", status_char, duration_str),
                            Style::default().fg(status_color),
                        ),
                    ])));

                    if !data.params.is_empty() {
                        let params_display = if data.params.len() > 40 {
                            format!("{}...", &data.params[..37])
                        } else {
                            data.params.clone()
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(Color::Rgb(16, 185, 129))),
                            Span::styled("📥 ", Style::default().fg(Color::Rgb(107, 114, 128))),
                            Span::raw(params_display),
                        ])));
                    }

                    if let Some(ref result) = data.result {
                        let result_display = if result.len() > 40 {
                            format!("{}...", &result[..37])
                        } else {
                            result.clone()
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(Color::Rgb(16, 185, 129))),
                            Span::styled("📤 ", Style::default().fg(Color::Rgb(34, 197, 94))),
                            Span::raw(result_display),
                        ])));
                    } else if let Some(ref error) = data.error {
                        let error_display = if error.len() > 40 {
                            format!("{}...", &error[..37])
                        } else {
                            error.clone()
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(Color::Rgb(16, 185, 129))),
                            Span::styled("❌ ", Style::default().fg(Color::Rgb(239, 68, 68))),
                            Span::raw(error_display),
                        ])));
                    }

                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        "╰───────────────────────────────────────────────────╯",
                        Style::default().fg(Color::Rgb(16, 185, 129)),
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
                            Style::default().fg(Color::Rgb(139, 92, 246)), // Violet
                        ),
                        Span::styled(
                            format!("{} {} ─╮", status_char, duration_str),
                            Style::default().fg(Color::Rgb(250, 204, 21)), // Yellow
                        ),
                    ])));

                    // Token info
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(Color::Rgb(139, 92, 246))),
                        Span::styled(
                            format!("📊 {} in → {} out", data.tokens_in, data.tokens_out),
                            Style::default().fg(Color::Rgb(107, 114, 128)),
                        ),
                    ])));

                    // Last lines of content
                    let content_lines: Vec<&str> = data.content.lines().collect();
                    let start = content_lines.len().saturating_sub(3);
                    for line in content_lines.iter().skip(start) {
                        let display = if line.len() > 50 {
                            format!("{}...", &line[..47])
                        } else {
                            line.to_string()
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(Color::Rgb(139, 92, 246))),
                            Span::raw(display),
                        ])));
                    }

                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        "╰───────────────────────────────────────────────────╯",
                        Style::default().fg(Color::Rgb(139, 92, 246)),
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
                Span::styled("─".repeat(20), Style::default().fg(theme.text_muted)),
            ])));

            if !self.partial_response.is_empty() {
                for line in self.partial_response.lines() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(theme.status_success)),
                        Span::raw(line.to_string()),
                    ])));
                }
            }

            // Animated thinking indicator
            let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
            let spinner = spinners[(self.frame as usize) % spinners.len()];
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.status_success)),
                Span::styled(
                    format!("{} Thinking...", spinner),
                    Style::default()
                        .fg(theme.status_running)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])));
        }

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 💬 CONVERSATION ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(list, area);
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
                " Tab ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" switch view  "),
            Span::styled(
                " Esc ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" back  "),
            Span::styled(
                " ↑↓ ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" history"),
        ]);

        let paragraph = Paragraph::new(hints);
        frame.render_widget(paragraph, area);
    }
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
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_submit() {
        let mut view = ChatView::new();
        view.input = "Hello Nika".to_string();
        view.cursor = view.input.len();

        let result = view.submit();
        assert_eq!(result, Some("Hello Nika".to_string()));
        assert!(view.input.is_empty());
        assert_eq!(view.messages.len(), 2); // Welcome + user message
    }

    #[test]
    fn test_chat_view_submit_empty() {
        let mut view = ChatView::new();
        view.input = "   ".to_string();

        let result = view.submit();
        assert_eq!(result, None);
    }

    #[test]
    fn test_chat_view_history_navigation() {
        let mut view = ChatView::new();
        view.add_user_message("First".to_string());
        view.add_user_message("Second".to_string());

        view.history_up();
        assert_eq!(view.input, "Second");

        view.history_up();
        assert_eq!(view.input, "First");

        view.history_down();
        assert_eq!(view.input, "Second");
    }

    #[test]
    fn test_chat_view_history_down_clears_input() {
        let mut view = ChatView::new();
        view.add_user_message("Test".to_string());

        view.history_up();
        assert_eq!(view.input, "Test");

        view.history_down();
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_cursor() {
        let mut view = ChatView::new();
        view.insert_char('H');
        view.insert_char('i');
        assert_eq!(view.input, "Hi");
        assert_eq!(view.cursor, 2);

        view.cursor_left();
        assert_eq!(view.cursor, 1);

        view.insert_char('e');
        assert_eq!(view.input, "Hei");

        view.backspace();
        assert_eq!(view.input, "Hi");
    }

    #[test]
    fn test_chat_view_cursor_right() {
        let mut view = ChatView::new();
        view.input = "Hello".to_string();
        view.cursor = 0;

        view.cursor_right();
        assert_eq!(view.cursor, 1);

        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        assert_eq!(view.cursor, 5);

        // Should not go past the end
        view.cursor_right();
        assert_eq!(view.cursor, 5);
    }

    #[test]
    fn test_chat_view_backspace_at_start() {
        let mut view = ChatView::new();
        view.input = "Hi".to_string();
        view.cursor = 0;

        view.backspace();
        assert_eq!(view.input, "Hi");
        assert_eq!(view.cursor, 0);
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
        assert!(status.contains("1 messages"));
        assert!(status.contains("0 in history"));
    }

    #[test]
    fn test_chat_view_default() {
        let view = ChatView::default();
        assert_eq!(view.messages.len(), 1);
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_unicode_input() {
        let mut view = ChatView::new();

        // Test emoji input (4 bytes per char)
        view.insert_char('\u{1F980}'); // Rust crab emoji
        view.insert_char('!');
        assert_eq!(view.input, "\u{1F980}!");
        assert_eq!(view.cursor, 2); // 2 chars, not 5 bytes

        // Test backspace removes whole emoji
        view.backspace();
        assert_eq!(view.input, "\u{1F980}");
        assert_eq!(view.cursor, 1);

        // Test cursor navigation with unicode
        view.insert_char('\u{1F600}'); // Grinning face emoji
        assert_eq!(view.input, "\u{1F980}\u{1F600}");
        assert_eq!(view.cursor, 2);

        view.cursor_left();
        assert_eq!(view.cursor, 1);

        // Insert in middle
        view.insert_char('A');
        assert_eq!(view.input, "\u{1F980}A\u{1F600}");
        assert_eq!(view.cursor, 2);

        // Cursor right should work correctly
        view.cursor_right();
        assert_eq!(view.cursor, 3);

        // Should not go past end
        view.cursor_right();
        assert_eq!(view.cursor, 3);
    }

    #[test]
    fn test_chat_view_unicode_history() {
        let mut view = ChatView::new();
        view.add_user_message("Hello \u{1F44B}".to_string()); // Wave emoji

        view.history_up();
        assert_eq!(view.input, "Hello \u{1F44B}");
        assert_eq!(view.cursor, 7); // 7 chars (H-e-l-l-o-space-emoji), not 10 bytes
    }

    #[test]
    fn test_chat_view_multibyte_backspace() {
        let mut view = ChatView::new();

        // Build string with mixed byte-width chars
        view.insert_char('a'); // 1 byte
        view.insert_char('\u{00E9}'); // 2 bytes (e with acute)
        view.insert_char('\u{4E2D}'); // 3 bytes (Chinese character)
        view.insert_char('\u{1F980}'); // 4 bytes (crab emoji)

        assert_eq!(view.input, "a\u{00E9}\u{4E2D}\u{1F980}");
        assert_eq!(view.cursor, 4);

        // Backspace should remove each char correctly
        view.backspace();
        assert_eq!(view.input, "a\u{00E9}\u{4E2D}");
        assert_eq!(view.cursor, 3);

        view.backspace();
        assert_eq!(view.input, "a\u{00E9}");
        assert_eq!(view.cursor, 2);

        view.backspace();
        assert_eq!(view.input, "a");
        assert_eq!(view.cursor, 1);

        view.backspace();
        assert_eq!(view.input, "");
        assert_eq!(view.cursor, 0);
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
        let view = ChatView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("Model:"));
        // Model name depends on env vars, so just check format
        assert!(status.contains("1 messages"));
        assert!(status.contains("0 in history"));
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

        assert_eq!(view.inline_content.len(), 1);
        if let InlineContent::InferStream(data) = &view.inline_content[0] {
            assert_eq!(data.model, "claude-sonnet-4");
            assert_eq!(data.tokens_in, 100);
            assert_eq!(data.max_tokens, 2000);
        } else {
            panic!("Expected InferStream");
        }

        // Should add activity item
        assert_eq!(view.activity_items.len(), 1);
        assert_eq!(view.activity_items[0].verb, "infer");
    }

    #[test]
    fn test_chat_view_append_infer_content() {
        let mut view = ChatView::new();
        view.start_infer_stream("claude-sonnet-4", 100, 2000);
        view.append_infer_content("Hello ", 10);
        view.append_infer_content("World!", 20);

        if let InlineContent::InferStream(data) = &view.inline_content[0] {
            assert_eq!(data.content, "Hello World!");
            assert_eq!(data.tokens_out, 20);
        } else {
            panic!("Expected InferStream");
        }

        // Should also update partial_response for backwards compatibility
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

    // === Scroll Tests ===

    #[test]
    fn test_chat_view_scroll_up() {
        let mut view = ChatView::new();
        view.scroll = 5;

        view.scroll_up();
        assert_eq!(view.scroll, 4);

        view.scroll_up();
        view.scroll_up();
        view.scroll_up();
        view.scroll_up();
        assert_eq!(view.scroll, 0);

        // Should not go negative
        view.scroll_up();
        assert_eq!(view.scroll, 0);
    }

    #[test]
    fn test_chat_view_scroll_down() {
        let mut view = ChatView::new();
        // ChatView starts with 1 welcome message
        assert_eq!(view.messages.len(), 1);

        // With only 1 message, can't scroll down
        view.scroll_down();
        assert_eq!(view.scroll, 0);

        // Add more messages
        view.add_user_message("Message 1".to_string());
        view.add_user_message("Message 2".to_string());
        view.add_user_message("Message 3".to_string());
        assert_eq!(view.messages.len(), 4);

        // Now can scroll down
        view.scroll_down();
        assert_eq!(view.scroll, 1);

        view.scroll_down();
        view.scroll_down();
        assert_eq!(view.scroll, 3);

        // Should cap at messages.len() - 1
        view.scroll_down();
        assert_eq!(view.scroll, 3);
    }

    #[test]
    fn test_chat_view_scroll_to_bottom() {
        let mut view = ChatView::new();
        view.scroll = 10;

        view.scroll_to_bottom();
        assert_eq!(view.scroll, 0);
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
}
