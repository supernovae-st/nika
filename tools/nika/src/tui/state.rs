//! TUI State Management
//!
//! Central state for the TUI application.
//! Updated by events from the runtime, queried by panels for rendering.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::config::NikaConfig;
use crate::event::{ContextSource, EventKind, ExcludedItem};

use super::theme::{MissionPhase, TaskStatus, ThemeMode};
use super::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

/// Panel identifier for focus management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    /// Panel 1: Mission Control / Progress
    Progress,
    /// Panel 2: DAG Execution
    Dag,
    /// Panel 3: NovaNet Context
    NovaNet,
    /// Panel 4: Agent Reasoning
    Agent,
}

impl PanelId {
    /// Get all panels in order
    pub fn all() -> &'static [PanelId] {
        &[
            PanelId::Progress,
            PanelId::Dag,
            PanelId::NovaNet,
            PanelId::Agent,
        ]
    }

    /// Get next panel (wrapping)
    pub fn next(&self) -> PanelId {
        match self {
            PanelId::Progress => PanelId::Dag,
            PanelId::Dag => PanelId::NovaNet,
            PanelId::NovaNet => PanelId::Agent,
            PanelId::Agent => PanelId::Progress,
        }
    }

    /// Get previous panel (wrapping)
    pub fn prev(&self) -> PanelId {
        match self {
            PanelId::Progress => PanelId::Agent,
            PanelId::Dag => PanelId::Progress,
            PanelId::NovaNet => PanelId::Dag,
            PanelId::Agent => PanelId::NovaNet,
        }
    }

    /// Get panel number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            PanelId::Progress => 1,
            PanelId::Dag => 2,
            PanelId::NovaNet => 3,
            PanelId::Agent => 4,
        }
    }

    /// Get panel title
    pub fn title(&self) -> &'static str {
        match self {
            PanelId::Progress => "MISSION CONTROL",
            PanelId::Dag => "DAG EXECUTION",
            PanelId::NovaNet => "NOVANET STATION",
            PanelId::Agent => "AGENT REASONING",
        }
    }

    /// Get panel icon
    pub fn icon(&self) -> &'static str {
        match self {
            PanelId::Progress => "◉",
            PanelId::Dag => "⎔",
            PanelId::NovaNet => "⊛",
            PanelId::Agent => "⊕",
        }
    }
}

/// TUI interaction mode
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TuiMode {
    /// Default navigation mode
    #[default]
    Normal,
    /// Live agent output streaming
    Streaming,
    /// Viewing task output (inspect modal)
    Inspect(String),
    /// Modifying task output (edit modal)
    Edit(String),
    /// Search mode
    Search,
    /// Help overlay
    Help,
    /// Metrics overlay
    Metrics,
    /// Settings overlay (API keys, provider config)
    Settings,
    /// Chat overlay (contextual AI assistance)
    ChatOverlay,
}

/// Workflow execution state
#[derive(Debug, Clone)]
pub struct WorkflowState {
    /// Workflow file path
    pub path: String,
    /// Current mission phase
    pub phase: MissionPhase,
    /// Total task count
    pub task_count: usize,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Start time
    pub started_at: Option<Instant>,
    /// Elapsed time in ms (updated on render)
    pub elapsed_ms: u64,
    /// Generation ID
    pub generation_id: Option<String>,
    /// Final output JSON (when completed)
    pub final_output: Option<Arc<Value>>,
    /// Total duration from workflow event (when completed)
    pub total_duration_ms: Option<u64>,
    /// Error message (when failed)
    pub error_message: Option<String>,
}

impl WorkflowState {
    pub fn new(path: String) -> Self {
        Self {
            path,
            phase: MissionPhase::Preflight,
            task_count: 0,
            tasks_completed: 0,
            started_at: None,
            elapsed_ms: 0,
            generation_id: None,
            final_output: None,
            total_duration_ms: None,
            error_message: None,
        }
    }

    /// Calculate progress percentage
    pub fn progress_pct(&self) -> f32 {
        if self.task_count == 0 {
            0.0
        } else {
            (self.tasks_completed as f32 / self.task_count as f32) * 100.0
        }
    }
}

/// Individual task state
#[derive(Debug, Clone)]
pub struct TaskState {
    /// Task ID
    pub id: String,
    /// Task status
    pub status: TaskStatus,
    /// Task type (infer, exec, fetch, invoke, agent)
    pub task_type: Option<String>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Start time
    pub started_at: Option<Instant>,
    /// Duration in ms (when completed)
    pub duration_ms: Option<u64>,
    /// Input (when started) - for TUI Task I/O display
    pub input: Option<Arc<Value>>,
    /// Output (when completed)
    pub output: Option<Arc<Value>>,
    /// Error message (when failed)
    pub error: Option<String>,
    /// Token count (for infer/agent tasks)
    pub tokens: Option<u32>,
}

impl TaskState {
    pub fn new(id: String, dependencies: Vec<String>) -> Self {
        Self {
            id,
            status: TaskStatus::Pending,
            task_type: None,
            dependencies,
            started_at: None,
            duration_ms: None,
            input: None,
            output: None,
            error: None,
            tokens: None,
        }
    }
}

/// MCP call record (enhanced v0.5.2 with full params/response)
#[derive(Debug, Clone)]
pub struct McpCall {
    /// Unique call ID for correlation with McpResponse
    pub call_id: String,
    /// Call sequence number
    pub seq: usize,
    /// Server name
    pub server: String,
    /// Tool name (if tool call)
    pub tool: Option<String>,
    /// Resource URI (if resource read)
    pub resource: Option<String>,
    /// Task that made the call
    pub task_id: String,
    /// Response received
    pub completed: bool,
    /// Output length in bytes
    pub output_len: Option<usize>,
    /// Call timestamp
    pub timestamp_ms: u64,
    /// Parameters passed to MCP tool (for TUI display)
    pub params: Option<serde_json::Value>,
    /// Full response JSON (for TUI display)
    pub response: Option<serde_json::Value>,
    /// Whether the MCP call returned an error
    pub is_error: bool,
    /// Duration of MCP call in milliseconds
    pub duration_ms: Option<u64>,
}

/// Context assembly state
#[derive(Debug, Clone, Default)]
pub struct ContextAssembly {
    /// Sources included in context
    pub sources: Vec<ContextSource>,
    /// Items excluded
    pub excluded: Vec<ExcludedItem>,
    /// Total tokens
    pub total_tokens: u32,
    /// Budget used percentage
    pub budget_used_pct: f32,
    /// Was truncated
    pub truncated: bool,
}

/// Agent turn record
#[derive(Debug, Clone)]
pub struct AgentTurnState {
    /// Turn index (0-based)
    pub index: u32,
    /// Turn status
    pub status: String,
    /// Cumulative tokens
    pub tokens: Option<u32>,
    /// Tool calls made this turn
    pub tool_calls: Vec<String>,
    /// Extended thinking content (v0.4+)
    /// Captured from Claude's reasoning process when extended_thinking is enabled
    pub thinking: Option<String>,
    /// Response text from the agent turn
    pub response_text: Option<String>,
}

/// Spawned sub-agent state (v0.5 MVP 8)
///
/// Tracks nested agents spawned via spawn_agent tool.
#[derive(Debug, Clone)]
pub struct SpawnedAgent {
    /// ID of the parent task that spawned this agent
    pub parent_task_id: String,
    /// ID of the child task
    pub child_task_id: String,
    /// Nesting depth (1 = root agent spawning first child)
    pub depth: u32,
}

/// Breakpoint definition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Breakpoint {
    /// Break before task starts
    BeforeTask(String),
    /// Break after task completes
    AfterTask(String),
    /// Break on error
    OnError(String),
    /// Break on any MCP call
    OnMcp(String),
    /// Break on agent turn N
    OnAgentTurn(String, u32),
}

/// Metrics aggregation
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// Total tokens consumed
    pub total_tokens: u32,
    /// Total input tokens
    pub input_tokens: u32,
    /// Total output tokens
    pub output_tokens: u32,
    /// Total cache-read tokens (prompt caching)
    pub cache_read_tokens: u32,
    /// Total cost in USD
    pub cost_usd: f64,
    /// MCP call count by tool
    pub mcp_calls: HashMap<String, usize>,
    /// Token history (for sparkline)
    pub token_history: Vec<u32>,
    /// Latency history in ms (for sparkline)
    pub latency_history: Vec<u64>,
}

// ═══════════════════════════════════════════
// TIER 3.4: NOTIFICATIONS
// ═══════════════════════════════════════════

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Informational (workflow complete, etc.)
    Info,
    /// Warning (task slow, high token usage)
    Warning,
    /// Alert (critical issues)
    Alert,
    /// Success (workflow completed successfully)
    Success,
    /// Error (workflow failed)
    Error,
}

impl NotificationLevel {
    /// Get icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            NotificationLevel::Info => "ℹ",
            NotificationLevel::Warning => "⚠",
            NotificationLevel::Alert => "🔔",
            NotificationLevel::Success => "✓",
            NotificationLevel::Error => "✗",
        }
    }
}

/// A system notification (TIER 3.4)
#[derive(Debug, Clone)]
pub struct Notification {
    /// Notification level/severity
    pub level: NotificationLevel,
    /// Notification message
    pub message: String,
    /// Timestamp (ms since workflow start)
    pub timestamp_ms: u64,
    /// Whether this notification has been dismissed
    pub dismissed: bool,
}

impl Notification {
    /// Create a new notification
    pub fn new(level: NotificationLevel, message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp_ms,
            dismissed: false,
        }
    }

    /// Create an info notification
    pub fn info(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Info, message, timestamp_ms)
    }

    /// Create a warning notification
    pub fn warning(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Warning, message, timestamp_ms)
    }

    /// Create an alert notification
    pub fn alert(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Alert, message, timestamp_ms)
    }

    /// Create a success notification
    pub fn success(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Success, message, timestamp_ms)
    }

    /// Create an error notification
    pub fn error(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Error, message, timestamp_ms)
    }
}

/// Settings overlay field identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsField {
    /// Anthropic API key field
    #[default]
    AnthropicKey,
    /// OpenAI API key field
    OpenAiKey,
    /// Default provider selector
    Provider,
    /// Default model selector
    Model,
}

impl SettingsField {
    /// Get all fields in order
    pub fn all() -> &'static [SettingsField] {
        &[
            SettingsField::AnthropicKey,
            SettingsField::OpenAiKey,
            SettingsField::Provider,
            SettingsField::Model,
        ]
    }

    /// Get next field (wrapping)
    pub fn next(&self) -> SettingsField {
        match self {
            SettingsField::AnthropicKey => SettingsField::OpenAiKey,
            SettingsField::OpenAiKey => SettingsField::Provider,
            SettingsField::Provider => SettingsField::Model,
            SettingsField::Model => SettingsField::AnthropicKey,
        }
    }

    /// Get previous field (wrapping)
    pub fn prev(&self) -> SettingsField {
        match self {
            SettingsField::AnthropicKey => SettingsField::Model,
            SettingsField::OpenAiKey => SettingsField::AnthropicKey,
            SettingsField::Provider => SettingsField::OpenAiKey,
            SettingsField::Model => SettingsField::Provider,
        }
    }

    /// Get field label for display
    pub fn label(&self) -> &'static str {
        match self {
            SettingsField::AnthropicKey => "Anthropic API Key",
            SettingsField::OpenAiKey => "OpenAI API Key",
            SettingsField::Provider => "Default Provider",
            SettingsField::Model => "Default Model",
        }
    }
}

/// Settings overlay state
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Currently focused field
    pub focus: SettingsField,
    /// Edit mode active (typing in field)
    pub editing: bool,
    /// Input buffer for current field
    pub input_buffer: String,
    /// Cursor position in input buffer
    pub cursor: usize,
    /// Loaded configuration
    pub config: NikaConfig,
    /// Has unsaved changes
    pub dirty: bool,
    /// Status message (success/error feedback)
    pub status_message: Option<String>,
}

impl SettingsState {
    /// Create settings state with loaded config
    pub fn new(config: NikaConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Focus next field
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Focus previous field
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Enter edit mode for current field
    pub fn start_edit(&mut self) {
        self.editing = true;
        // Load current value into buffer
        self.input_buffer = match self.focus {
            SettingsField::AnthropicKey => {
                self.config.api_keys.anthropic.clone().unwrap_or_default()
            }
            SettingsField::OpenAiKey => self.config.api_keys.openai.clone().unwrap_or_default(),
            SettingsField::Provider => self.config.defaults.provider.clone().unwrap_or_default(),
            SettingsField::Model => self.config.defaults.model.clone().unwrap_or_default(),
        };
        self.cursor = self.input_buffer.len();
    }

    /// Cancel edit mode, discard changes
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Confirm edit, apply to config
    pub fn confirm_edit(&mut self) {
        if !self.editing {
            return;
        }

        let value = if self.input_buffer.is_empty() {
            None
        } else {
            Some(self.input_buffer.clone())
        };

        match self.focus {
            SettingsField::AnthropicKey => {
                self.config.api_keys.anthropic = value;
            }
            SettingsField::OpenAiKey => {
                self.config.api_keys.openai = value;
            }
            SettingsField::Provider => {
                self.config.defaults.provider = value;
            }
            SettingsField::Model => {
                self.config.defaults.model = value;
            }
        }

        self.dirty = true;
        self.editing = false;
        self.input_buffer.clear();
        self.cursor = 0;
    }

    /// Insert character at cursor
    pub fn insert_char(&mut self, c: char) {
        if !self.editing {
            return;
        }
        self.input_buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor
    pub fn backspace(&mut self) {
        if !self.editing || self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.input_buffer.remove(self.cursor);
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if !self.editing || self.cursor >= self.input_buffer.len() {
            return;
        }
        self.input_buffer.remove(self.cursor);
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input_buffer.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input_buffer.len();
    }

    /// Save config to file
    pub fn save(&mut self) -> Result<(), String> {
        self.config.save().map_err(|e| e.to_string())?;
        self.dirty = false;
        self.status_message = Some("Settings saved".to_string());
        Ok(())
    }

    /// Check if a key is set for display
    pub fn key_status(&self, field: SettingsField) -> (bool, String) {
        match field {
            SettingsField::AnthropicKey => {
                if let Some(ref key) = self.config.api_keys.anthropic {
                    (true, crate::config::mask_api_key(key, 12))
                } else {
                    (false, "Not set".to_string())
                }
            }
            SettingsField::OpenAiKey => {
                if let Some(ref key) = self.config.api_keys.openai {
                    (true, crate::config::mask_api_key(key, 10))
                } else {
                    (false, "Not set".to_string())
                }
            }
            SettingsField::Provider => {
                if let Some(ref provider) = self.config.defaults.provider {
                    (true, provider.clone())
                } else {
                    let auto = self.config.default_provider().unwrap_or("none");
                    (false, format!("{} (auto)", auto))
                }
            }
            SettingsField::Model => {
                if let Some(ref model) = self.config.defaults.model {
                    (true, model.clone())
                } else {
                    (false, "Default".to_string())
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CHAT OVERLAY STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// Message role in chat overlay conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatOverlayMessageRole {
    /// User input
    User,
    /// Nika/AI response
    Nika,
    /// System message (context, hints)
    System,
    /// Tool execution result
    Tool,
}

/// A message in the chat overlay
#[derive(Debug, Clone)]
pub struct ChatOverlayMessage {
    /// Who sent the message
    pub role: ChatOverlayMessageRole,
    /// Message content
    pub content: String,
}

impl ChatOverlayMessage {
    /// Create a new message
    pub fn new(role: ChatOverlayMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// Chat overlay state - contextual AI assistance panel
#[derive(Debug, Clone)]
pub struct ChatOverlayState {
    /// Conversation history
    pub messages: Vec<ChatOverlayMessage>,
    /// Current input buffer
    pub input: String,
    /// Cursor position in input buffer
    pub cursor: usize,
    /// Scroll offset in message list
    pub scroll: usize,
    /// Command history (for up/down navigation)
    pub history: Vec<String>,
    /// History navigation index (None = not navigating)
    pub history_index: Option<usize>,
    /// Whether streaming response is in progress
    pub is_streaming: bool,
    /// Partial response accumulated during streaming
    pub partial_response: String,
    /// Current model name for display
    pub current_model: String,
}

impl Default for ChatOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatOverlayState {
    /// Create new chat overlay state with welcome message
    pub fn new() -> Self {
        // Detect initial model from environment
        let initial_model = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            "claude-sonnet-4".to_string()
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            "gpt-4o".to_string()
        } else {
            "No API Key".to_string()
        };

        Self {
            messages: vec![ChatOverlayMessage::new(
                ChatOverlayMessageRole::System,
                "Chat overlay active. Ask for help with the current view.",
            )],
            input: String::new(),
            cursor: 0,
            scroll: 0,
            history: Vec::new(),
            history_index: None,
            is_streaming: false,
            partial_response: String::new(),
            current_model: initial_model,
        }
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

    /// Set the current model name
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.current_model = model.into();
    }

    /// Add a tool message
    pub fn add_tool_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::Tool,
            content,
        ));
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Add a user message and clear input
    pub fn add_user_message(&mut self) -> Option<String> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Take ownership to avoid cloning twice
        let message = std::mem::take(&mut self.input);

        // Add to history first (clone once)
        self.history.push(message.clone());
        self.history_index = None;

        // Add to messages (move)
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::User,
            &message,
        ));

        // Reset cursor
        self.cursor = 0;

        Some(message)
    }

    /// Add a Nika response message
    pub fn add_nika_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::Nika,
            content,
        ));
    }

    /// Navigate history up (previous message)
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
            self.cursor = self.input.len();
        }
    }

    /// Navigate history down (next message)
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(i) if i < self.history.len() - 1 => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
                self.cursor = self.input.len();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    /// Clear all messages except welcome
    pub fn clear(&mut self) {
        self.messages = vec![ChatOverlayMessage::new(
            ChatOverlayMessageRole::System,
            "Chat cleared.",
        )];
        self.scroll = 0;
    }

    /// Scroll up in message history
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll down in message history
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

/// Main TUI state
#[derive(Debug, Clone)]
pub struct TuiState {
    // ═══════════════════════════════════════════
    // ANIMATION STATE
    // ═══════════════════════════════════════════
    /// Frame counter (wraps at 60 for 1-second cycles at 60 FPS)
    pub frame: u8,

    // ═══════════════════════════════════════════
    // EXECUTION STATE
    // ═══════════════════════════════════════════
    /// Workflow state
    pub workflow: WorkflowState,
    /// Task states by ID
    pub tasks: HashMap<String, TaskState>,
    /// Current active task
    pub current_task: Option<String>,
    /// Task execution order (for timeline)
    pub task_order: Vec<String>,

    // ═══════════════════════════════════════════
    // MCP TRACKING
    // ═══════════════════════════════════════════
    /// MCP call log
    pub mcp_calls: Vec<McpCall>,
    /// Next MCP call sequence number
    pub mcp_seq: usize,
    /// Selected MCP call index for Full JSON view
    pub selected_mcp_idx: Option<usize>,
    /// Current context assembly
    pub context_assembly: ContextAssembly,

    // ═══════════════════════════════════════════
    // AGENT TRACKING
    // ═══════════════════════════════════════════
    /// Agent turns for current agent task
    pub agent_turns: Vec<AgentTurnState>,
    /// Streaming buffer for live output
    pub streaming_buffer: String,
    /// Max turns for current agent
    pub agent_max_turns: Option<u32>,
    /// Spawned sub-agents (v0.5 MVP 8 nested agents)
    pub spawned_agents: Vec<SpawnedAgent>,

    // ═══════════════════════════════════════════
    // UI STATE
    // ═══════════════════════════════════════════
    /// Currently focused panel
    pub focus: PanelId,
    /// Current interaction mode
    pub mode: TuiMode,
    /// Scroll offset per panel
    pub scroll: HashMap<PanelId, usize>,
    /// Settings overlay state
    pub settings: SettingsState,
    /// Chat overlay state (contextual AI assistance)
    pub chat_overlay: ChatOverlayState,
    /// Theme mode: dark or light (TIER 2.4)
    pub theme_mode: ThemeMode,

    // ═══════════════════════════════════════════
    // TAB STATE
    // ═══════════════════════════════════════════
    /// Mission Control panel tab (Progress / IO / Output)
    pub mission_tab: MissionTab,
    /// DAG panel tab (Graph / YAML)
    pub dag_tab: DagTab,
    /// NovaNet panel tab (Summary / Full JSON)
    pub novanet_tab: NovanetTab,
    /// Reasoning panel tab (Turns / Thinking)
    pub reasoning_tab: ReasoningTab,

    // ═══════════════════════════════════════════
    // DEBUG STATE
    // ═══════════════════════════════════════════
    /// Active breakpoints
    pub breakpoints: HashSet<Breakpoint>,
    /// Execution paused
    pub paused: bool,
    /// Step mode (advance one step at a time)
    pub step_mode: bool,

    // ═══════════════════════════════════════════
    // METRICS
    // ═══════════════════════════════════════════
    /// Aggregated metrics
    pub metrics: Metrics,

    // ═══════════════════════════════════════════
    // FILTER STATE
    // ═══════════════════════════════════════════
    /// Current filter/search query
    pub filter_query: String,
    /// Filter cursor position
    pub filter_cursor: usize,

    // ═══════════════════════════════════════════
    // NOTIFICATIONS (TIER 3.4)
    // ═══════════════════════════════════════════
    /// System notifications
    pub notifications: Vec<Notification>,
    /// Maximum number of notifications to keep
    pub max_notifications: usize,

    // ═══════════════════════════════════════════
    // LAZY RENDERING (TIER 4.1)
    // ═══════════════════════════════════════════
    /// Dirty flags for lazy rendering
    pub dirty: DirtyFlags,

    // ═══════════════════════════════════════════
    // JSON MEMOIZATION (TIER 4.4)
    // ═══════════════════════════════════════════
    /// Cache for formatted JSON strings
    pub json_cache: JsonFormatCache,
}

/// Dirty flags for lazy rendering (TIER 4.1)
///
/// Tracks which parts of the UI need re-rendering.
/// Set flags when state changes, clear after rendering.
#[derive(Debug, Clone, Default)]
pub struct DirtyFlags {
    /// All panels need re-render (full redraw)
    pub all: bool,
    /// Progress panel needs re-render
    pub progress: bool,
    /// DAG panel needs re-render
    pub dag: bool,
    /// NovaNet panel needs re-render
    pub novanet: bool,
    /// Reasoning panel needs re-render
    pub reasoning: bool,
    /// Status bar needs re-render
    pub status: bool,
    /// Notifications changed
    pub notifications: bool,
}

impl DirtyFlags {
    /// Mark all panels as dirty
    pub fn mark_all(&mut self) {
        self.all = true;
    }

    /// Clear all dirty flags after rendering
    pub fn clear(&mut self) {
        self.all = false;
        self.progress = false;
        self.dag = false;
        self.novanet = false;
        self.reasoning = false;
        self.status = false;
        self.notifications = false;
    }

    /// Check if any panel is dirty
    pub fn any(&self) -> bool {
        self.all
            || self.progress
            || self.dag
            || self.novanet
            || self.reasoning
            || self.status
            || self.notifications
    }

    /// Check if specific panel is dirty
    pub fn is_panel_dirty(&self, panel: PanelId) -> bool {
        if self.all {
            return true;
        }
        match panel {
            PanelId::Progress => self.progress,
            PanelId::Dag => self.dag,
            PanelId::NovaNet => self.novanet,
            PanelId::Agent => self.reasoning,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TIER 4.4: JSON FORMAT CACHE
// ═══════════════════════════════════════════════════════════════════════════════

/// Cache for formatted JSON strings to avoid repeated serde_json::to_string_pretty calls
#[derive(Debug, Clone, Default)]
pub struct JsonFormatCache {
    /// Cached formatted JSON by key (task_id, mcp_call_id, or "final_output")
    cache: HashMap<String, String>,
    /// Maximum cache entries before eviction
    max_entries: usize,
}

impl JsonFormatCache {
    /// Create a new cache with default capacity
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_entries: 50,
        }
    }

    /// Get cached JSON for a key, or format and cache it
    pub fn get_or_format<T: serde::Serialize>(&mut self, key: &str, value: &T) -> String {
        if let Some(cached) = self.cache.get(key) {
            return cached.clone();
        }

        let formatted = serde_json::to_string_pretty(value).unwrap_or_default();

        // Simple LRU-style eviction: clear oldest entries if over limit
        if self.cache.len() >= self.max_entries {
            // Remove first 10% of entries, minimum 1 (oldest by insertion order)
            let to_remove = (self.max_entries / 10).max(1);
            let keys: Vec<String> = self.cache.keys().take(to_remove).cloned().collect();
            for k in keys {
                self.cache.remove(&k);
            }
        }

        self.cache.insert(key.to_string(), formatted.clone());
        formatted
    }

    /// Invalidate specific cache entries
    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
    }

    /// Invalidate all entries starting with a prefix
    pub fn invalidate_prefix(&mut self, prefix: &str) {
        self.cache.retain(|k, _| !k.starts_with(prefix));
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache stats for debugging
    #[allow(dead_code)]
    pub fn stats(&self) -> (usize, usize) {
        (self.cache.len(), self.max_entries)
    }
}

impl TuiState {
    /// Create new TUI state for a workflow
    pub fn new(workflow_path: &str) -> Self {
        // Load config from file, merge with env vars
        let config = NikaConfig::load().unwrap_or_default().with_env();

        Self {
            frame: 0,
            workflow: WorkflowState::new(workflow_path.to_string()),
            tasks: HashMap::new(),
            current_task: None,
            task_order: Vec::new(),
            mcp_calls: Vec::new(),
            mcp_seq: 0,
            selected_mcp_idx: None,
            context_assembly: ContextAssembly::default(),
            agent_turns: Vec::new(),
            streaming_buffer: String::new(),
            agent_max_turns: None,
            spawned_agents: Vec::new(),
            focus: PanelId::Progress,
            mode: TuiMode::Normal,
            scroll: HashMap::new(),
            settings: SettingsState::new(config),
            chat_overlay: ChatOverlayState::new(),
            theme_mode: ThemeMode::default(),
            mission_tab: MissionTab::default(),
            dag_tab: DagTab::default(),
            novanet_tab: NovanetTab::default(),
            reasoning_tab: ReasoningTab::default(),
            breakpoints: HashSet::new(),
            paused: false,
            step_mode: false,
            metrics: Metrics::default(),
            filter_query: String::new(),
            filter_cursor: 0,
            notifications: Vec::new(),
            max_notifications: 10,
            dirty: DirtyFlags::default(),
            json_cache: JsonFormatCache::new(),
        }
    }

    /// Handle an event from the runtime
    pub fn handle_event(&mut self, kind: &EventKind, timestamp_ms: u64) {
        match kind {
            // ═══════════════════════════════════════════
            // WORKFLOW EVENTS
            // ═══════════════════════════════════════════
            EventKind::WorkflowStarted {
                task_count,
                generation_id,
                ..
            } => {
                self.workflow.task_count = *task_count;
                self.workflow.phase = MissionPhase::Countdown;
                self.workflow.started_at = Some(Instant::now());
                self.workflow.generation_id = Some(generation_id.clone());
                // TIER 4.1: Mark all panels dirty on workflow start
                self.dirty.mark_all();
                // TIER 4.4: Clear JSON cache on workflow start
                self.json_cache.clear();
            }

            EventKind::WorkflowCompleted {
                final_output,
                total_duration_ms,
            } => {
                self.workflow.phase = MissionPhase::MissionSuccess;
                self.workflow.final_output = Some(Arc::clone(final_output));
                self.workflow.total_duration_ms = Some(*total_duration_ms);
                self.current_task = None;

                // TIER 3.4: Add success notification
                let duration_secs = *total_duration_ms as f64 / 1000.0;
                self.add_notification(Notification::success(
                    format!(
                        "🦚 Magnificent! Warped through in {:.1}s ({}/{} tasks)",
                        duration_secs, self.workflow.tasks_completed, self.workflow.task_count
                    ),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress and status dirty
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            EventKind::WorkflowFailed { error, .. } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(error.clone());

                // TIER 3.4: Add error notification
                self.add_notification(Notification::error(
                    format!("🦖 RAWR! Mission failed: {}", error),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress, status, and notifications dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            EventKind::WorkflowAborted {
                reason,
                duration_ms,
                running_tasks,
            } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(format!("Aborted: {}", reason));
                self.workflow.total_duration_ms = Some(*duration_ms);
                self.current_task = None;

                // TIER 3.4: Add abort notification
                let task_info = if running_tasks.is_empty() {
                    String::new()
                } else {
                    format!(" ({} tasks interrupted)", running_tasks.len())
                };
                self.add_notification(Notification::warning(
                    format!("⚠️ Mission aborted: {}{}", reason, task_info),
                    timestamp_ms,
                ));
                // Mark all relevant panels dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // TASK EVENTS
            // ═══════════════════════════════════════════
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                let deps: Vec<String> = dependencies
                    .iter()
                    .map(|s: &std::sync::Arc<str>| s.to_string())
                    .collect();
                let task = TaskState::new(task_id.to_string(), deps);
                self.tasks.insert(task_id.to_string(), task);
                self.task_order.push(task_id.to_string());
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
            }

            EventKind::TaskStarted {
                task_id,
                verb,
                inputs,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Instant::now());
                    task.input = Some(Arc::new(inputs.clone()));
                    task.task_type = Some(verb.to_string());
                }
                self.current_task = Some(task_id.to_string());

                // Update phase
                if self.workflow.phase == MissionPhase::Countdown {
                    self.workflow.phase = MissionPhase::Launch;
                } else {
                    self.workflow.phase = MissionPhase::Orbital;
                }
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                // TIER 4.4: Invalidate task cache on start (will need re-format later)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Success;
                    task.duration_ms = Some(*duration_ms);
                    task.output = Some(output.clone());
                }
                self.workflow.tasks_completed += 1;

                // TIER 3.4: Notify on slow tasks
                let duration_secs = *duration_ms as f64 / 1000.0;
                if *duration_ms > 30_000 {
                    self.add_notification(Notification::alert(
                        format!(
                            "🦥 Sloth mode! '{}' crawled in at {:.1}s",
                            task_id, duration_secs
                        ),
                        timestamp_ms,
                    ));
                } else if *duration_ms > 10_000 {
                    self.add_notification(Notification::warning(
                        format!(
                            "🦩 Taking its time... '{}' at {:.1}s",
                            task_id, duration_secs
                        ),
                        timestamp_ms,
                    ));
                }

                // Clear agent state if this was an agent task
                self.agent_turns.clear();
                self.streaming_buffer.clear();
                self.agent_max_turns = None;
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                // TIER 4.4: Invalidate task cache on completion (new output)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Failed;
                    task.duration_ms = Some(*duration_ms);
                    task.error = Some(error.clone());
                }
                // TIER 4.1: Mark progress, dag, and status dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.dirty.status = true;
            }

            // ═══════════════════════════════════════════
            // MCP EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpInvoke {
                task_id,
                mcp_server,
                tool,
                resource,
                call_id,
                params,
            } => {
                let call = McpCall {
                    call_id: call_id.clone(),
                    seq: self.mcp_seq,
                    server: mcp_server.clone(),
                    tool: tool.clone(),
                    resource: resource.clone(),
                    task_id: task_id.to_string(),
                    completed: false,
                    output_len: None,
                    timestamp_ms,
                    params: params.clone(),
                    response: None,
                    is_error: false,
                    duration_ms: None,
                };
                self.mcp_calls.push(call);
                self.mcp_seq += 1;

                // Update phase
                self.workflow.phase = MissionPhase::Rendezvous;

                // Track in metrics
                if let Some(ref tool_name) = tool {
                    let entry = self.metrics.mcp_calls.entry(tool_name.clone()).or_insert(0);
                    *entry += 1;
                }
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
            }

            EventKind::McpResponse {
                task_id: _,
                output_len,
                call_id,
                duration_ms,
                cached: _,
                is_error,
                response,
            } => {
                // Find and update the matching call by call_id
                let tool_name = self
                    .mcp_calls
                    .iter()
                    .find(|c| c.call_id == *call_id)
                    .and_then(|c| c.tool.clone());

                if let Some(call) = self.mcp_calls.iter_mut().find(|c| c.call_id == *call_id) {
                    call.completed = true;
                    call.output_len = Some(*output_len);
                    call.response = response.clone();
                    call.is_error = *is_error;
                    call.duration_ms = Some(*duration_ms);
                }

                // Track MCP latency for sparkline (keep last 20 values)
                if self.metrics.latency_history.len() >= 20 {
                    self.metrics.latency_history.remove(0);
                }
                self.metrics.latency_history.push(*duration_ms);

                // TIER 3.4: Notify on slow MCP responses (> 5s)
                if *duration_ms > 5_000 {
                    let duration_secs = *duration_ms as f64 / 1000.0;
                    let tool_display = tool_name.as_deref().unwrap_or("resource");
                    self.add_notification(Notification::warning(
                        format!(
                            "🐙 Tentacles reaching... '{}' at {:.1}s",
                            tool_display, duration_secs
                        ),
                        timestamp_ms,
                    ));
                }

                // Return to orbital phase
                self.workflow.phase = MissionPhase::Orbital;
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
                // TIER 4.4: Invalidate MCP call cache on response
                self.json_cache.invalidate(&format!("mcp:{}", call_id));
            }

            // ═══════════════════════════════════════════
            // CONTEXT EVENTS
            // ═══════════════════════════════════════════
            EventKind::ContextAssembled {
                sources,
                excluded,
                total_tokens,
                budget_used_pct,
                truncated,
                ..
            } => {
                self.context_assembly = ContextAssembly {
                    sources: sources.clone(),
                    excluded: excluded.clone(),
                    total_tokens: *total_tokens,
                    budget_used_pct: *budget_used_pct,
                    truncated: *truncated,
                };
                // TIER 4.1: Mark novanet panel dirty (v0.5 fix)
                self.dirty.novanet = true;
            }

            // ═══════════════════════════════════════════
            // AGENT EVENTS
            // ═══════════════════════════════════════════
            EventKind::AgentStart { max_turns, .. } => {
                self.agent_turns.clear();
                self.streaming_buffer.clear();
                self.agent_max_turns = Some(*max_turns);
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentTurn {
                turn_index,
                kind,
                metadata,
                ..
            } => {
                // Extract tokens from metadata if present (v0.4.1)
                let tokens = metadata.as_ref().map(|m| m.total_tokens());
                // Extract thinking and response_text from metadata (v0.4 reasoning capture)
                let thinking = metadata.as_ref().and_then(|m| m.thinking.clone());
                let response_text = metadata.as_ref().map(|m| m.response_text.clone());

                let turn = AgentTurnState {
                    index: *turn_index,
                    status: kind.clone(),
                    tokens,
                    tool_calls: Vec::new(),
                    thinking,
                    response_text,
                };
                // Update or add turn
                if let Some(existing) = self.agent_turns.iter_mut().find(|t| t.index == *turn_index)
                {
                    existing.status = kind.clone();
                    existing.tokens = tokens;
                    existing.thinking = turn.thinking;
                    existing.response_text = turn.response_text;
                } else {
                    self.agent_turns.push(turn);
                }
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentComplete { turns, .. } => {
                // Update metrics
                if let Some(last_turn) = self.agent_turns.last() {
                    if let Some(tokens) = last_turn.tokens {
                        self.metrics.token_history.push(tokens);
                    }
                }
                let _ = turns; // Used for logging
                               // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentSpawned {
                parent_task_id,
                child_task_id,
                depth,
            } => {
                // Track spawned sub-agent (v0.5 MVP 8)
                self.spawned_agents.push(SpawnedAgent {
                    parent_task_id: parent_task_id.to_string(),
                    child_task_id: child_task_id.to_string(),
                    depth: *depth,
                });

                // Add notification for nested agent spawn (🐤 = subagent)
                self.add_notification(Notification::info(
                    format!(
                        "🐤 Hatching '{}' at depth {} — fly little one!",
                        child_task_id, depth
                    ),
                    timestamp_ms,
                ));

                // TIER 4.1: Mark reasoning and notifications dirty
                self.dirty.reasoning = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cost_usd,
                ttft_ms,
                ..
            } => {
                self.metrics.input_tokens += input_tokens;
                self.metrics.output_tokens += output_tokens;
                self.metrics.cache_read_tokens += cache_read_tokens;
                self.metrics.total_tokens += input_tokens + output_tokens;
                self.metrics.cost_usd += cost_usd;
                self.metrics
                    .token_history
                    .push(input_tokens + output_tokens);
                if let Some(ttft) = ttft_ms {
                    self.metrics.latency_history.push(*ttft);
                }

                // TIER 3.4: Token usage progression with cosmic pirate emojis
                // 🔋 0-50% (quiet) | 🔥 50-70% | 🧨 70-85% | ☠️ 85-95% | 💀 95%+
                const CONTEXT_WINDOW: u32 = 100_000;
                let pct = (self.metrics.total_tokens as f64 / CONTEXT_WINDOW as f64) * 100.0;

                if pct > 95.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "💀 ABANDON SHIP! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 85.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "☠️ Danger zone! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 70.0 {
                    self.add_notification(Notification::warning(
                        format!(
                            "🧨 Getting spicy! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 50.0 {
                    self.add_notification(Notification::info(
                        format!(
                            "🔥 Heating up... {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                }
                // TIER 4.1: Mark progress dirty (for metrics display)
                self.dirty.progress = true;
            }

            // Other events we don't track in state
            _ => {}
        }
    }

    /// Update elapsed time and animation frame (call on each render frame)
    pub fn tick(&mut self) {
        // Update elapsed time
        if let Some(started) = self.workflow.started_at {
            self.workflow.elapsed_ms = started.elapsed().as_millis() as u64;
        }

        // Advance animation frame (wraps at 60 for 1-second cycles)
        self.frame = self.frame.wrapping_add(1) % 60;
    }

    /// Get spinner character for current frame
    /// Uses braille spinner: ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (self.frame / 6) as usize % SPINNER.len();
        SPINNER[idx]
    }

    /// Get rocket animation character for current frame
    /// Used during Launch phase
    pub fn rocket_char(&self) -> char {
        const ROCKET: &[char] = &['🚀', '🔥', '💨', '✨'];
        let idx = (self.frame / 15) as usize % ROCKET.len();
        ROCKET[idx]
    }

    /// Check if a task is a spawned subagent
    ///
    /// Returns true if the task_id appears as a child in spawned_agents.
    /// Used to display 🐤 instead of 🐔 for nested agents.
    pub fn is_subagent(&self, task_id: &str) -> bool {
        self.spawned_agents
            .iter()
            .any(|s| s.child_task_id == task_id)
    }

    /// Get the appropriate agent icon for a task
    ///
    /// Returns 🐤 for subagents (spawned via spawn_agent)
    /// Returns 🐔 for parent agents (defined in workflow)
    pub fn agent_icon(&self, task_id: &str) -> &'static str {
        if self.is_subagent(task_id) {
            "🐤" // Spawned subagent
        } else {
            "🐔" // Parent agent
        }
    }

    /// Focus next panel
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    /// Focus previous panel
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Focus specific panel by number (1-indexed)
    pub fn focus_panel(&mut self, num: u8) {
        self.focus = match num {
            1 => PanelId::Progress,
            2 => PanelId::Dag,
            3 => PanelId::NovaNet,
            4 => PanelId::Agent,
            _ => self.focus,
        };
    }

    /// Cycle tab in the currently focused panel
    pub fn cycle_tab(&mut self) {
        match self.focus {
            PanelId::Progress => self.mission_tab = self.mission_tab.next(),
            PanelId::Dag => self.dag_tab = self.dag_tab.next(),
            PanelId::NovaNet => self.novanet_tab = self.novanet_tab.next(),
            PanelId::Agent => self.reasoning_tab = self.reasoning_tab.next(),
        }
    }

    // ═══════════════════════════════════════════
    // MCP NAVIGATION (TIER 1.3)
    // ═══════════════════════════════════════════

    /// Select previous MCP call (↑ in NovaNet panel)
    pub fn select_prev_mcp(&mut self) {
        if self.mcp_calls.is_empty() {
            return;
        }

        self.selected_mcp_idx = match self.selected_mcp_idx {
            None => Some(self.mcp_calls.len().saturating_sub(1)), // Start from last
            Some(0) => Some(0),                                   // Stay at first
            Some(idx) => Some(idx - 1),
        };
    }

    /// Select next MCP call (↓ in NovaNet panel)
    pub fn select_next_mcp(&mut self) {
        if self.mcp_calls.is_empty() {
            return;
        }

        let max_idx = self.mcp_calls.len().saturating_sub(1);
        self.selected_mcp_idx = match self.selected_mcp_idx {
            None => Some(0),                              // Start from first
            Some(idx) if idx >= max_idx => Some(max_idx), // Stay at last
            Some(idx) => Some(idx + 1),
        };
    }

    /// Select MCP call by index (for direct access)
    pub fn select_mcp(&mut self, idx: usize) {
        if idx < self.mcp_calls.len() {
            self.selected_mcp_idx = Some(idx);
        }
    }

    /// Get currently selected MCP call
    pub fn get_selected_mcp(&self) -> Option<&McpCall> {
        self.selected_mcp_idx
            .and_then(|idx| self.mcp_calls.get(idx))
    }

    // ═══════════════════════════════════════════
    // FILTER METHODS (TIER 1.5)
    // ═══════════════════════════════════════════

    /// Add character to filter query
    pub fn filter_push(&mut self, c: char) {
        self.filter_query.insert(self.filter_cursor, c);
        self.filter_cursor += 1;
    }

    /// Remove character before cursor (backspace)
    pub fn filter_backspace(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor -= 1;
            self.filter_query.remove(self.filter_cursor);
        }
    }

    /// Remove character at cursor (delete)
    pub fn filter_delete(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            self.filter_query.remove(self.filter_cursor);
        }
    }

    /// Move cursor left
    pub fn filter_cursor_left(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn filter_cursor_right(&mut self) {
        if self.filter_cursor < self.filter_query.len() {
            self.filter_cursor += 1;
        }
    }

    /// Clear filter query
    pub fn filter_clear(&mut self) {
        self.filter_query.clear();
        self.filter_cursor = 0;
    }

    /// Check if filter is active
    pub fn has_filter(&self) -> bool {
        !self.filter_query.is_empty()
    }

    /// Get filtered task IDs
    pub fn filtered_task_ids(&self) -> Vec<&String> {
        if self.filter_query.is_empty() {
            return self.task_order.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.task_order
            .iter()
            .filter(|id| {
                // Match task ID
                if id.to_lowercase().contains(&query) {
                    return true;
                }
                // Match task type
                if let Some(task) = self.tasks.get(*id) {
                    if let Some(task_type) = &task.task_type {
                        if task_type.to_lowercase().contains(&query) {
                            return true;
                        }
                    }
                }
                false
            })
            .collect()
    }

    /// Get filtered MCP calls
    pub fn filtered_mcp_calls(&self) -> Vec<&McpCall> {
        if self.filter_query.is_empty() {
            return self.mcp_calls.iter().collect();
        }

        let query = self.filter_query.to_lowercase();
        self.mcp_calls
            .iter()
            .filter(|call| {
                // Match server name
                if call.server.to_lowercase().contains(&query) {
                    return true;
                }
                // Match tool name
                if let Some(tool) = &call.tool {
                    if tool.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                // Match resource URI
                if let Some(resource) = &call.resource {
                    if resource.to_lowercase().contains(&query) {
                        return true;
                    }
                }
                false
            })
            .collect()
    }

    /// Toggle pause state
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Check if a breakpoint should trigger
    pub fn should_break(&self, kind: &EventKind) -> bool {
        if self.breakpoints.is_empty() {
            return false;
        }

        match kind {
            EventKind::TaskStarted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::BeforeTask(task_id.to_string())),
            EventKind::TaskCompleted { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string())),
            EventKind::TaskFailed { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string())),
            EventKind::McpInvoke { task_id, .. } => self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string())),
            EventKind::AgentTurn {
                task_id,
                turn_index,
                ..
            } => self
                .breakpoints
                .contains(&Breakpoint::OnAgentTurn(task_id.to_string(), *turn_index)),
            _ => false,
        }
    }

    /// Check if a task has a breakpoint set (TIER 2.3)
    pub fn has_breakpoint(&self, task_id: &str) -> bool {
        self.breakpoints
            .contains(&Breakpoint::BeforeTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::AfterTask(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnError(task_id.to_string()))
            || self
                .breakpoints
                .contains(&Breakpoint::OnMcp(task_id.to_string()))
    }

    /// Get content suitable for clipboard copy based on focused panel and current tab
    ///
    /// Returns the most relevant content for the current view:
    /// - Progress panel: Final output JSON or current task output
    /// - DAG panel: YAML content or task list
    /// - NovaNet panel: Selected MCP call (params + response)
    /// - Agent panel: Agent turns or thinking content
    pub fn get_copyable_content(&self) -> Option<String> {
        match self.focus {
            PanelId::Progress => {
                // Priority: final output > current task output > metrics summary
                if let Some(ref output) = self.workflow.final_output {
                    Some(serde_json::to_string_pretty(output.as_ref()).unwrap_or_default())
                } else if let Some(ref task_id) = self.current_task {
                    self.tasks.get(task_id).and_then(|task| {
                        task.output
                            .as_ref()
                            .map(|o| serde_json::to_string_pretty(o.as_ref()).unwrap_or_default())
                    })
                } else {
                    // Return metrics summary
                    Some(format!(
                        "Workflow: {}\nTasks: {}/{}\nTokens: {}\nMCP calls: {}",
                        self.workflow.path,
                        self.workflow.tasks_completed,
                        self.workflow.task_count,
                        self.metrics.total_tokens,
                        self.mcp_calls.len()
                    ))
                }
            }
            PanelId::Dag => {
                // Return task list with statuses
                let mut lines = vec!["# DAG Tasks".to_string()];
                for task_id in &self.task_order {
                    if let Some(task) = self.tasks.get(task_id) {
                        let status = match task.status {
                            crate::tui::theme::TaskStatus::Pending => "○",
                            crate::tui::theme::TaskStatus::Running => "◐",
                            crate::tui::theme::TaskStatus::Success => "✓",
                            crate::tui::theme::TaskStatus::Failed => "✗",
                            crate::tui::theme::TaskStatus::Paused => "⏸",
                        };
                        let deps = if task.dependencies.is_empty() {
                            String::new()
                        } else {
                            format!(" → {}", task.dependencies.join(", "))
                        };
                        lines.push(format!("{} {}{}", status, task_id, deps));
                    }
                }
                Some(lines.join("\n"))
            }
            PanelId::NovaNet => {
                // Return selected MCP call or all calls
                if let Some(idx) = self.selected_mcp_idx {
                    self.mcp_calls.get(idx).map(|call| {
                        let mut content = format!(
                            "# MCP Call #{}: {}\n\n",
                            call.seq + 1,
                            call.tool.as_deref().unwrap_or("resource")
                        );
                        content.push_str("## Request\n");
                        if let Some(ref params) = call.params {
                            content.push_str(
                                &serde_json::to_string_pretty(params).unwrap_or_default(),
                            );
                        }
                        content.push_str("\n\n## Response\n");
                        if let Some(ref response) = call.response {
                            content.push_str(
                                &serde_json::to_string_pretty(response).unwrap_or_default(),
                            );
                        } else if !call.completed {
                            content.push_str("(pending...)");
                        }
                        content
                    })
                } else if !self.mcp_calls.is_empty() {
                    // Return summary of all MCP calls
                    let mut lines = vec!["# MCP Calls".to_string()];
                    for call in &self.mcp_calls {
                        let status = if call.completed { "✓" } else { "◐" };
                        let tool = call.tool.as_deref().unwrap_or("resource");
                        let duration = call
                            .duration_ms
                            .map(|d| format!(" {}ms", d))
                            .unwrap_or_default();
                        lines.push(format!(
                            "{} #{} {}:{}{}",
                            status,
                            call.seq + 1,
                            call.server,
                            tool,
                            duration
                        ));
                    }
                    Some(lines.join("\n"))
                } else {
                    None
                }
            }
            PanelId::Agent => {
                // Return agent turns or thinking content
                if self.agent_turns.is_empty() {
                    return None;
                }

                let mut content = String::from("# Agent Turns\n\n");
                for turn in &self.agent_turns {
                    content.push_str(&format!("## Turn {}\n", turn.index + 1));
                    if let Some(ref thinking) = turn.thinking {
                        content.push_str("### Thinking\n");
                        content.push_str(thinking);
                        content.push_str("\n\n");
                    }
                    if let Some(ref response) = turn.response_text {
                        content.push_str("### Response\n");
                        content.push_str(response);
                        content.push_str("\n\n");
                    }
                    if !turn.tool_calls.is_empty() {
                        content.push_str("### Tool Calls\n");
                        for tool in &turn.tool_calls {
                            content.push_str(&format!("- {}\n", tool));
                        }
                        content.push('\n');
                    }
                }
                Some(content)
            }
        }
    }

    // ═══════════════════════════════════════════
    // RETRY SUPPORT (TIER 1.2)
    // ═══════════════════════════════════════════

    /// Check if the workflow is in a failed state (can be retried)
    pub fn is_failed(&self) -> bool {
        self.workflow.phase == MissionPhase::Abort || self.workflow.error_message.is_some()
    }

    /// Check if the workflow completed successfully
    pub fn is_success(&self) -> bool {
        self.workflow.phase == MissionPhase::MissionSuccess
    }

    /// Check if the workflow is still running
    pub fn is_running(&self) -> bool {
        matches!(
            self.workflow.phase,
            MissionPhase::Countdown
                | MissionPhase::Launch
                | MissionPhase::Orbital
                | MissionPhase::Rendezvous
        )
    }

    /// Reset state for retry - clears failed tasks and resets workflow phase
    ///
    /// Returns the list of task IDs that were reset (previously failed)
    pub fn reset_for_retry(&mut self) -> Vec<String> {
        let mut reset_tasks = Vec::new();

        // Reset workflow state
        self.workflow.phase = MissionPhase::Preflight;
        self.workflow.error_message = None;
        self.workflow.final_output = None;
        self.workflow.total_duration_ms = None;
        self.workflow.tasks_completed = 0;
        self.workflow.started_at = None;

        // Reset all failed tasks to pending
        for (task_id, task) in &mut self.tasks {
            if task.status == TaskStatus::Failed {
                task.status = TaskStatus::Pending;
                task.duration_ms = None;
                task.error = None;
                task.output = None;
                reset_tasks.push(task_id.clone());
            }
        }

        // Clear current task
        self.current_task = None;

        // Clear agent turns
        self.agent_turns.clear();

        // Reset metrics
        self.metrics = Metrics::default();

        // Clear MCP calls (keep for reference? or clear?)
        // For now, keep them as history but mark workflow as ready for retry
        self.mcp_seq = 0;

        reset_tasks
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NOTIFICATION METHODS (TIER 3.4)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Add a notification (TIER 3.4)
    ///
    /// Automatically trims old notifications when exceeding max_notifications.
    pub fn add_notification(&mut self, notification: Notification) {
        self.notifications.push(notification);

        // Trim old notifications if we exceed max
        while self.notifications.len() > self.max_notifications {
            self.notifications.remove(0);
        }

        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Get active (non-dismissed) notifications
    pub fn active_notifications(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter().filter(|n| !n.dismissed)
    }

    /// Get count of active notifications
    pub fn active_notification_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.dismissed).count()
    }

    /// Dismiss the most recent notification
    pub fn dismiss_notification(&mut self) {
        // Dismiss the most recent non-dismissed notification
        for n in self.notifications.iter_mut().rev() {
            if !n.dismissed {
                n.dismissed = true;
                // TIER 4.1: Mark notifications dirty
                self.dirty.notifications = true;
                break;
            }
        }
    }

    /// Dismiss all notifications
    pub fn dismiss_all_notifications(&mut self) {
        for n in &mut self.notifications {
            n.dismissed = true;
        }
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }

    /// Clear all notifications (removes from list entirely)
    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
        // TIER 4.1: Mark notifications dirty
        self.dirty.notifications = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Use actual package version in tests to avoid version drift
    const TEST_VERSION: &str = env!("CARGO_PKG_VERSION");

    #[test]
    fn test_panel_id_next_cycles() {
        assert_eq!(PanelId::Progress.next(), PanelId::Dag);
        assert_eq!(PanelId::Agent.next(), PanelId::Progress);
    }

    #[test]
    fn test_panel_id_prev_cycles() {
        assert_eq!(PanelId::Progress.prev(), PanelId::Agent);
        assert_eq!(PanelId::Dag.prev(), PanelId::Progress);
    }

    #[test]
    fn test_workflow_state_progress() {
        let mut ws = WorkflowState::new("test.yaml".to_string());
        ws.task_count = 10;
        ws.tasks_completed = 5;
        assert!((ws.progress_pct() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_tui_state_focus_navigation() {
        let mut state = TuiState::new("test.yaml");
        assert_eq!(state.focus, PanelId::Progress);

        state.focus_next();
        assert_eq!(state.focus, PanelId::Dag);

        state.focus_panel(4);
        assert_eq!(state.focus, PanelId::Agent);

        state.focus_prev();
        assert_eq!(state.focus, PanelId::NovaNet);
    }

    #[test]
    fn test_tui_state_cycle_tab() {
        use crate::tui::views::{DagTab, MissionTab, NovanetTab, ReasoningTab};

        let mut state = TuiState::new("test.yaml");

        // Test Mission tab cycling (Progress → TaskIO → Output → Progress)
        state.focus = PanelId::Progress;
        assert_eq!(state.mission_tab, MissionTab::Progress);
        state.cycle_tab();
        assert_eq!(state.mission_tab, MissionTab::TaskIO);
        state.cycle_tab();
        assert_eq!(state.mission_tab, MissionTab::Output);
        state.cycle_tab();
        assert_eq!(state.mission_tab, MissionTab::Progress);

        // Test Dag tab cycling (Graph ↔ Yaml)
        state.focus = PanelId::Dag;
        assert_eq!(state.dag_tab, DagTab::Graph);
        state.cycle_tab();
        assert_eq!(state.dag_tab, DagTab::Yaml);
        state.cycle_tab();
        assert_eq!(state.dag_tab, DagTab::Graph);

        // Test NovaNet tab cycling (Summary ↔ FullJson)
        state.focus = PanelId::NovaNet;
        assert_eq!(state.novanet_tab, NovanetTab::Summary);
        state.cycle_tab();
        assert_eq!(state.novanet_tab, NovanetTab::FullJson);
        state.cycle_tab();
        assert_eq!(state.novanet_tab, NovanetTab::Summary);

        // Test Reasoning tab cycling (Turns ↔ Thinking)
        state.focus = PanelId::Agent;
        assert_eq!(state.reasoning_tab, ReasoningTab::Turns);
        state.cycle_tab();
        assert_eq!(state.reasoning_tab, ReasoningTab::Thinking);
        state.cycle_tab();
        assert_eq!(state.reasoning_tab, ReasoningTab::Turns);
    }

    #[test]
    fn test_tui_state_handle_workflow_started() {
        let mut state = TuiState::new("test.yaml");

        state.handle_event(
            &EventKind::WorkflowStarted {
                task_count: 5,
                generation_id: "gen-123".to_string(),
                workflow_hash: "abc".to_string(),
                nika_version: TEST_VERSION.to_string(),
            },
            0,
        );

        assert_eq!(state.workflow.task_count, 5);
        assert_eq!(state.workflow.phase, MissionPhase::Countdown);
        assert!(state.workflow.started_at.is_some());
    }

    #[test]
    fn test_tui_state_handle_task_lifecycle() {
        let mut state = TuiState::new("test.yaml");

        // Schedule task
        state.handle_event(
            &EventKind::TaskScheduled {
                task_id: Arc::from("task1"),
                dependencies: vec![],
            },
            0,
        );
        assert!(state.tasks.contains_key("task1"));
        assert_eq!(state.tasks["task1"].status, TaskStatus::Pending);

        // Start task
        state.handle_event(
            &EventKind::TaskStarted {
                verb: "infer".into(),
                task_id: Arc::from("task1"),
                inputs: serde_json::json!({}),
            },
            100,
        );
        assert_eq!(state.tasks["task1"].status, TaskStatus::Running);
        assert_eq!(state.current_task, Some("task1".to_string()));

        // Complete task
        state.handle_event(
            &EventKind::TaskCompleted {
                task_id: Arc::from("task1"),
                output: Arc::new(serde_json::json!({"result": "ok"})),
                duration_ms: 500,
            },
            600,
        );
        assert_eq!(state.tasks["task1"].status, TaskStatus::Success);
        assert_eq!(state.workflow.tasks_completed, 1);
    }

    #[test]
    fn test_tui_state_handle_mcp_events() {
        let mut state = TuiState::new("test.yaml");

        let test_params = serde_json::json!({"entity": "qr-code"});
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: "test-call-1".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some("novanet_describe".to_string()),
                resource: None,
                params: Some(test_params.clone()),
            },
            100,
        );

        assert_eq!(state.mcp_calls.len(), 1);
        assert_eq!(state.mcp_calls[0].call_id, "test-call-1");
        assert_eq!(
            state.mcp_calls[0].tool,
            Some("novanet_describe".to_string())
        );
        assert!(!state.mcp_calls[0].completed);
        assert_eq!(state.mcp_calls[0].params, Some(test_params));

        let test_response = serde_json::json!({"name": "QR Code", "locale": "en-US"});
        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: "test-call-1".to_string(),
                output_len: 1024,
                duration_ms: 100,
                cached: false,
                is_error: false,
                response: Some(test_response.clone()),
            },
            200,
        );

        assert!(state.mcp_calls[0].completed);
        assert_eq!(state.mcp_calls[0].output_len, Some(1024));
        assert_eq!(state.mcp_calls[0].response, Some(test_response));
        assert_eq!(state.mcp_calls[0].duration_ms, Some(100));
        assert!(!state.mcp_calls[0].is_error);
    }

    #[test]
    fn test_tui_state_handle_mcp_error_response() {
        let mut state = TuiState::new("test.yaml");

        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: "error-call-1".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some("novanet_traverse".to_string()),
                resource: None,
                params: Some(serde_json::json!({"invalid": "params"})),
            },
            100,
        );

        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: "error-call-1".to_string(),
                output_len: 50,
                duration_ms: 25,
                cached: false,
                is_error: true,
                response: Some(serde_json::json!({"error": "Invalid params"})),
            },
            125,
        );

        assert!(state.mcp_calls[0].is_error);
        assert_eq!(state.mcp_calls[0].duration_ms, Some(25));
        assert_eq!(
            state.mcp_calls[0].response,
            Some(serde_json::json!({"error": "Invalid params"}))
        );
    }

    #[test]
    fn test_tui_state_handle_mcp_parallel_calls() {
        let mut state = TuiState::new("test.yaml");

        // Simulate parallel MCP calls (for_each scenario)
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: "call-fr".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some("novanet_generate".to_string()),
                resource: None,
                params: Some(serde_json::json!({"locale": "fr-FR"})),
            },
            100,
        );
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: "call-en".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some("novanet_generate".to_string()),
                resource: None,
                params: Some(serde_json::json!({"locale": "en-US"})),
            },
            110,
        );

        assert_eq!(state.mcp_calls.len(), 2);
        assert!(!state.mcp_calls[0].completed);
        assert!(!state.mcp_calls[1].completed);

        // Response for second call arrives first
        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: "call-en".to_string(),
                output_len: 500,
                duration_ms: 50,
                cached: false,
                is_error: false,
                response: Some(serde_json::json!({"content": "English content"})),
            },
            160,
        );

        // First call still pending, second completed
        assert!(!state.mcp_calls[0].completed);
        assert!(state.mcp_calls[1].completed);
        assert_eq!(state.mcp_calls[1].call_id, "call-en");

        // Response for first call arrives
        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: "call-fr".to_string(),
                output_len: 600,
                duration_ms: 120,
                cached: false,
                is_error: false,
                response: Some(serde_json::json!({"content": "French content"})),
            },
            220,
        );

        // Both completed, correct correlation
        assert!(state.mcp_calls[0].completed);
        assert_eq!(state.mcp_calls[0].call_id, "call-fr");
        assert_eq!(state.mcp_calls[0].duration_ms, Some(120));
        assert!(state.mcp_calls[1].completed);
        assert_eq!(state.mcp_calls[1].call_id, "call-en");
        assert_eq!(state.mcp_calls[1].duration_ms, Some(50));
    }

    #[test]
    fn test_breakpoint_detection() {
        let mut state = TuiState::new("test.yaml");
        state
            .breakpoints
            .insert(Breakpoint::BeforeTask("task1".to_string()));

        let event = EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: Arc::from("task1"),
            inputs: serde_json::json!({}),
        };
        assert!(state.should_break(&event));

        let event2 = EventKind::TaskStarted {
            verb: "infer".into(),
            task_id: Arc::from("task2"),
            inputs: serde_json::json!({}),
        };
        assert!(!state.should_break(&event2));
    }

    // ═══════════════════════════════════════════
    // SETTINGS STATE TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn test_settings_field_next_cycles() {
        assert_eq!(SettingsField::AnthropicKey.next(), SettingsField::OpenAiKey);
        assert_eq!(SettingsField::OpenAiKey.next(), SettingsField::Provider);
        assert_eq!(SettingsField::Provider.next(), SettingsField::Model);
        assert_eq!(SettingsField::Model.next(), SettingsField::AnthropicKey);
    }

    #[test]
    fn test_settings_field_prev_cycles() {
        assert_eq!(SettingsField::AnthropicKey.prev(), SettingsField::Model);
        assert_eq!(SettingsField::OpenAiKey.prev(), SettingsField::AnthropicKey);
        assert_eq!(SettingsField::Provider.prev(), SettingsField::OpenAiKey);
        assert_eq!(SettingsField::Model.prev(), SettingsField::Provider);
    }

    #[test]
    fn test_settings_field_all() {
        let all = SettingsField::all();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0], SettingsField::AnthropicKey);
        assert_eq!(all[3], SettingsField::Model);
    }

    #[test]
    fn test_settings_field_labels() {
        assert_eq!(SettingsField::AnthropicKey.label(), "Anthropic API Key");
        assert_eq!(SettingsField::OpenAiKey.label(), "OpenAI API Key");
        assert_eq!(SettingsField::Provider.label(), "Default Provider");
        assert_eq!(SettingsField::Model.label(), "Default Model");
    }

    #[test]
    fn test_settings_state_default() {
        let state = SettingsState::default();
        assert_eq!(state.focus, SettingsField::AnthropicKey);
        assert!(!state.editing);
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor, 0);
        assert!(!state.dirty);
    }

    #[test]
    fn test_settings_state_focus_navigation() {
        let mut state = SettingsState::default();
        assert_eq!(state.focus, SettingsField::AnthropicKey);

        state.focus_next();
        assert_eq!(state.focus, SettingsField::OpenAiKey);

        state.focus_next();
        assert_eq!(state.focus, SettingsField::Provider);

        state.focus_prev();
        assert_eq!(state.focus, SettingsField::OpenAiKey);
    }

    #[test]
    fn test_settings_state_edit_lifecycle() {
        use crate::config::ApiKeys;

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-test".to_string()),
                openai: None,
            },
            ..Default::default()
        };
        let mut state = SettingsState::new(config);

        // Start editing
        state.start_edit();
        assert!(state.editing);
        assert_eq!(state.input_buffer, "sk-ant-test");
        assert_eq!(state.cursor, 11); // Length of "sk-ant-test"

        // Modify buffer
        state.backspace();
        assert_eq!(state.input_buffer, "sk-ant-tes");

        state.insert_char('X');
        assert_eq!(state.input_buffer, "sk-ant-tesX");

        // Cancel edit - should restore
        state.cancel_edit();
        assert!(!state.editing);
        assert!(state.input_buffer.is_empty());
        assert!(!state.dirty);
    }

    #[test]
    fn test_settings_state_confirm_edit() {
        let mut state = SettingsState {
            focus: SettingsField::OpenAiKey,
            ..Default::default()
        };

        state.start_edit();
        state.input_buffer = "sk-new-key".to_string();
        state.confirm_edit();

        assert!(!state.editing);
        assert!(state.dirty);
        assert_eq!(state.config.api_keys.openai, Some("sk-new-key".to_string()));
    }

    #[test]
    fn test_settings_state_confirm_edit_empty_clears_value() {
        use crate::config::ApiKeys;

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-test".to_string()),
                openai: None,
            },
            ..Default::default()
        };
        let mut state = SettingsState::new(config);

        state.start_edit();
        state.input_buffer.clear(); // Clear to empty
        state.confirm_edit();

        assert!(state.config.api_keys.anthropic.is_none());
        assert!(state.dirty);
    }

    #[test]
    fn test_settings_state_cursor_movement() {
        let mut state = SettingsState {
            editing: true,
            input_buffer: "hello".to_string(),
            cursor: 3, // At 'l'
            ..Default::default()
        };

        state.cursor_left();
        assert_eq!(state.cursor, 2);

        state.cursor_right();
        assert_eq!(state.cursor, 3);

        state.cursor_home();
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        assert_eq!(state.cursor, 5);

        // Boundary checks
        state.cursor_home();
        state.cursor_left(); // Should stay at 0
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        state.cursor_right(); // Should stay at end
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_settings_state_key_status_displays_masked() {
        use crate::config::ApiKeys;

        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-api03-xyz123abc456".to_string()),
                openai: None,
            },
            ..Default::default()
        };
        let state = SettingsState::new(config);

        let (is_set, display) = state.key_status(SettingsField::AnthropicKey);
        assert!(is_set);
        assert!(display.contains("***"));
        assert!(display.starts_with("sk-ant-api03"));

        let (is_set, display) = state.key_status(SettingsField::OpenAiKey);
        assert!(!is_set);
        assert_eq!(display, "Not set");
    }

    #[test]
    fn test_settings_state_provider_auto_detection() {
        use crate::config::ApiKeys;

        // With anthropic key → auto-detect claude
        let config = NikaConfig {
            api_keys: ApiKeys {
                anthropic: Some("sk-ant-test".to_string()),
                openai: None,
            },
            ..Default::default()
        };
        let state = SettingsState::new(config);

        let (is_set, display) = state.key_status(SettingsField::Provider);
        assert!(!is_set); // Not explicitly set
        assert!(display.contains("claude"));
        assert!(display.contains("auto"));
    }

    #[test]
    fn test_tui_mode_settings_variant() {
        let mode = TuiMode::Settings;
        assert_eq!(mode, TuiMode::Settings);
        assert_ne!(mode, TuiMode::Normal);
        assert_ne!(mode, TuiMode::Help);
    }

    #[test]
    fn test_tui_state_has_settings() {
        let state = TuiState::new("test.yaml");
        // Settings should be initialized with loaded config
        assert_eq!(state.settings.focus, SettingsField::AnthropicKey);
        assert!(!state.settings.editing);
    }

    // ═══════════════════════════════════════════
    // RETRY TESTS (TIER 1.2)
    // ═══════════════════════════════════════════

    #[test]
    fn test_is_failed_returns_true_on_abort() {
        let mut state = TuiState::new("test.yaml");
        state.workflow.phase = MissionPhase::Abort;
        assert!(state.is_failed());
    }

    #[test]
    fn test_is_failed_returns_true_on_error_message() {
        let mut state = TuiState::new("test.yaml");
        state.workflow.error_message = Some("Something went wrong".to_string());
        assert!(state.is_failed());
    }

    #[test]
    fn test_is_failed_returns_false_on_success() {
        let mut state = TuiState::new("test.yaml");
        state.workflow.phase = MissionPhase::MissionSuccess;
        assert!(!state.is_failed());
        assert!(state.is_success());
    }

    #[test]
    fn test_is_running_returns_true_during_execution() {
        let mut state = TuiState::new("test.yaml");

        state.workflow.phase = MissionPhase::Countdown;
        assert!(state.is_running());

        state.workflow.phase = MissionPhase::Launch;
        assert!(state.is_running());

        state.workflow.phase = MissionPhase::Orbital;
        assert!(state.is_running());

        state.workflow.phase = MissionPhase::Rendezvous;
        assert!(state.is_running());
    }

    #[test]
    fn test_is_running_returns_false_when_not_executing() {
        let mut state = TuiState::new("test.yaml");

        state.workflow.phase = MissionPhase::Preflight;
        assert!(!state.is_running());

        state.workflow.phase = MissionPhase::MissionSuccess;
        assert!(!state.is_running());

        state.workflow.phase = MissionPhase::Abort;
        assert!(!state.is_running());
    }

    #[test]
    fn test_reset_for_retry_resets_workflow_state() {
        let mut state = TuiState::new("test.yaml");

        // Simulate workflow failure
        state.workflow.phase = MissionPhase::Abort;
        state.workflow.error_message = Some("Test error".to_string());
        state.workflow.task_count = 3;
        state.workflow.tasks_completed = 2;

        // Reset for retry
        let reset_tasks = state.reset_for_retry();

        // Verify reset
        assert_eq!(state.workflow.phase, MissionPhase::Preflight);
        assert!(state.workflow.error_message.is_none());
        assert!(state.workflow.final_output.is_none());
        assert_eq!(state.workflow.tasks_completed, 0);
        assert!(reset_tasks.is_empty()); // No tasks were failed in this simple test
    }

    #[test]
    fn test_reset_for_retry_resets_failed_tasks() {
        let mut state = TuiState::new("test.yaml");

        // Add tasks
        state.tasks.insert(
            "task1".to_string(),
            TaskState {
                id: "task1".to_string(),
                task_type: Some("infer".to_string()),
                status: TaskStatus::Success,
                dependencies: vec![],
                started_at: None,
                duration_ms: Some(100),
                input: None,
                output: None,
                error: None,
                tokens: None,
            },
        );
        state.tasks.insert(
            "task2".to_string(),
            TaskState {
                id: "task2".to_string(),
                task_type: Some("exec".to_string()),
                status: TaskStatus::Failed,
                dependencies: vec!["task1".to_string()],
                started_at: None,
                duration_ms: Some(50),
                input: None,
                output: None,
                error: Some("Command failed".to_string()),
                tokens: None,
            },
        );

        // Set workflow to failed
        state.workflow.phase = MissionPhase::Abort;

        // Reset for retry
        let reset_tasks = state.reset_for_retry();

        // Verify task1 unchanged (was success)
        assert_eq!(state.tasks["task1"].status, TaskStatus::Success);

        // Verify task2 reset (was failed)
        assert_eq!(state.tasks["task2"].status, TaskStatus::Pending);
        assert!(state.tasks["task2"].error.is_none());
        assert!(state.tasks["task2"].duration_ms.is_none());

        // Verify reset list
        assert_eq!(reset_tasks.len(), 1);
        assert!(reset_tasks.contains(&"task2".to_string()));
    }

    // ═══════════════════════════════════════════
    // MCP NAVIGATION TESTS (TIER 1.3)
    // ═══════════════════════════════════════════

    #[test]
    fn test_mcp_navigation_empty_list() {
        let mut state = TuiState::new("test.yaml");
        assert!(state.mcp_calls.is_empty());
        assert!(state.selected_mcp_idx.is_none());

        // Navigation on empty list should not panic
        state.select_prev_mcp();
        state.select_next_mcp();
        assert!(state.selected_mcp_idx.is_none());
    }

    #[test]
    fn test_mcp_navigation_select_prev() {
        let mut state = TuiState::new("test.yaml");

        // Add some MCP calls
        for i in 0..3 {
            state.mcp_calls.push(McpCall {
                call_id: format!("call-{}", i),
                seq: i,
                server: "novanet".to_string(),
                tool: Some(format!("tool{}", i)),
                resource: None,
                task_id: "task1".to_string(),
                completed: true,
                output_len: Some(100),
                timestamp_ms: 1000 + (i as u64) * 100,
                params: None,
                response: None,
                is_error: false,
                duration_ms: Some(10),
            });
        }

        // First prev should select last item
        state.select_prev_mcp();
        assert_eq!(state.selected_mcp_idx, Some(2));

        // Prev again should go to index 1
        state.select_prev_mcp();
        assert_eq!(state.selected_mcp_idx, Some(1));

        // Prev again should go to index 0
        state.select_prev_mcp();
        assert_eq!(state.selected_mcp_idx, Some(0));

        // Prev again should stay at 0 (boundary)
        state.select_prev_mcp();
        assert_eq!(state.selected_mcp_idx, Some(0));
    }

    #[test]
    fn test_mcp_navigation_select_next() {
        let mut state = TuiState::new("test.yaml");

        // Add some MCP calls
        for i in 0..3 {
            state.mcp_calls.push(McpCall {
                call_id: format!("call-{}", i),
                seq: i,
                server: "novanet".to_string(),
                tool: Some(format!("tool{}", i)),
                resource: None,
                task_id: "task1".to_string(),
                completed: true,
                output_len: Some(100),
                timestamp_ms: 1000 + (i as u64) * 100,
                params: None,
                response: None,
                is_error: false,
                duration_ms: Some(10),
            });
        }

        // First next should select first item
        state.select_next_mcp();
        assert_eq!(state.selected_mcp_idx, Some(0));

        // Next again should go to index 1
        state.select_next_mcp();
        assert_eq!(state.selected_mcp_idx, Some(1));

        // Next again should go to index 2
        state.select_next_mcp();
        assert_eq!(state.selected_mcp_idx, Some(2));

        // Next again should stay at 2 (boundary)
        state.select_next_mcp();
        assert_eq!(state.selected_mcp_idx, Some(2));
    }

    #[test]
    fn test_mcp_navigation_get_selected() {
        let mut state = TuiState::new("test.yaml");

        // Add MCP call
        state.mcp_calls.push(McpCall {
            call_id: "call-0".to_string(),
            seq: 0,
            server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });

        // No selection yet
        assert!(state.get_selected_mcp().is_none());

        // Select
        state.select_mcp(0);
        let selected = state.get_selected_mcp().unwrap();
        assert_eq!(selected.tool.as_deref(), Some("novanet_describe"));
    }

    // ═══════════════════════════════════════════
    // FILTER TESTS (TIER 1.5)
    // ═══════════════════════════════════════════

    #[test]
    fn test_filter_push_adds_characters() {
        let mut state = TuiState::new("test.yaml");
        assert!(state.filter_query.is_empty());
        assert_eq!(state.filter_cursor, 0);

        state.filter_push('h');
        state.filter_push('e');
        state.filter_push('l');
        state.filter_push('l');
        state.filter_push('o');

        assert_eq!(state.filter_query, "hello");
        assert_eq!(state.filter_cursor, 5);
    }

    #[test]
    fn test_filter_backspace_removes_before_cursor() {
        let mut state = TuiState::new("test.yaml");
        state.filter_query = "hello".to_string();
        state.filter_cursor = 5;

        state.filter_backspace();
        assert_eq!(state.filter_query, "hell");
        assert_eq!(state.filter_cursor, 4);

        state.filter_backspace();
        state.filter_backspace();
        assert_eq!(state.filter_query, "he");
        assert_eq!(state.filter_cursor, 2);

        // Backspace at start does nothing
        state.filter_cursor = 0;
        state.filter_backspace();
        assert_eq!(state.filter_query, "he");
        assert_eq!(state.filter_cursor, 0);
    }

    #[test]
    fn test_filter_delete_removes_at_cursor() {
        let mut state = TuiState::new("test.yaml");
        state.filter_query = "hello".to_string();
        state.filter_cursor = 0;

        state.filter_delete();
        assert_eq!(state.filter_query, "ello");
        assert_eq!(state.filter_cursor, 0);

        // Delete at end does nothing
        state.filter_cursor = state.filter_query.len();
        state.filter_delete();
        assert_eq!(state.filter_query, "ello");
    }

    #[test]
    fn test_filter_cursor_movement() {
        let mut state = TuiState::new("test.yaml");
        state.filter_query = "hello".to_string();
        state.filter_cursor = 2;

        state.filter_cursor_left();
        assert_eq!(state.filter_cursor, 1);

        state.filter_cursor_right();
        assert_eq!(state.filter_cursor, 2);

        // Boundary: left at start
        state.filter_cursor = 0;
        state.filter_cursor_left();
        assert_eq!(state.filter_cursor, 0);

        // Boundary: right at end
        state.filter_cursor = 5;
        state.filter_cursor_right();
        assert_eq!(state.filter_cursor, 5);
    }

    #[test]
    fn test_filter_clear_resets_all() {
        let mut state = TuiState::new("test.yaml");
        state.filter_query = "hello".to_string();
        state.filter_cursor = 3;

        state.filter_clear();
        assert!(state.filter_query.is_empty());
        assert_eq!(state.filter_cursor, 0);
    }

    #[test]
    fn test_has_filter() {
        let mut state = TuiState::new("test.yaml");
        assert!(!state.has_filter());

        state.filter_query = "test".to_string();
        assert!(state.has_filter());

        state.filter_clear();
        assert!(!state.has_filter());
    }

    #[test]
    fn test_filtered_task_ids_no_filter() {
        let mut state = TuiState::new("test.yaml");
        state.task_order = vec![
            "task1".to_string(),
            "task2".to_string(),
            "task3".to_string(),
        ];

        // No filter - returns all
        let filtered = state.filtered_task_ids();
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filtered_task_ids_matches_id() {
        let mut state = TuiState::new("test.yaml");
        state.task_order = vec![
            "generate".to_string(),
            "fetch_data".to_string(),
            "transform".to_string(),
        ];

        state.filter_query = "gen".to_string();
        let filtered = state.filtered_task_ids();
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "generate");
    }

    #[test]
    fn test_filtered_task_ids_matches_type() {
        let mut state = TuiState::new("test.yaml");
        state.task_order = vec!["task1".to_string(), "task2".to_string()];
        state.tasks.insert(
            "task1".to_string(),
            TaskState {
                id: "task1".to_string(),
                task_type: Some("infer".to_string()),
                status: TaskStatus::Pending,
                dependencies: vec![],
                started_at: None,
                duration_ms: None,
                input: None,
                output: None,
                error: None,
                tokens: None,
            },
        );
        state.tasks.insert(
            "task2".to_string(),
            TaskState {
                id: "task2".to_string(),
                task_type: Some("exec".to_string()),
                status: TaskStatus::Pending,
                dependencies: vec![],
                started_at: None,
                duration_ms: None,
                input: None,
                output: None,
                error: None,
                tokens: None,
            },
        );

        state.filter_query = "infer".to_string();
        let filtered = state.filtered_task_ids();
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "task1");
    }

    #[test]
    fn test_filtered_task_ids_case_insensitive() {
        let mut state = TuiState::new("test.yaml");
        state.task_order = vec!["GeneratePage".to_string()];

        state.filter_query = "page".to_string();
        let filtered = state.filtered_task_ids();
        assert_eq!(filtered.len(), 1);

        state.filter_query = "PAGE".to_string();
        let filtered = state.filtered_task_ids();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filtered_mcp_calls_no_filter() {
        let mut state = TuiState::new("test.yaml");
        state.mcp_calls.push(McpCall {
            call_id: "call-0".to_string(),
            seq: 0,
            server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });

        // No filter - returns all
        let filtered = state.filtered_mcp_calls();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filtered_mcp_calls_matches_server() {
        let mut state = TuiState::new("test.yaml");
        state.mcp_calls.push(McpCall {
            call_id: "call-0".to_string(),
            seq: 0,
            server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });
        state.mcp_calls.push(McpCall {
            call_id: "call-1".to_string(),
            seq: 1,
            server: "other_server".to_string(),
            tool: Some("other_tool".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1100,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });

        state.filter_query = "nova".to_string();
        let filtered = state.filtered_mcp_calls();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].server, "novanet");
    }

    #[test]
    fn test_filtered_mcp_calls_matches_tool() {
        let mut state = TuiState::new("test.yaml");
        state.mcp_calls.push(McpCall {
            call_id: "call-0".to_string(),
            seq: 0,
            server: "novanet".to_string(),
            tool: Some("novanet_describe".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });
        state.mcp_calls.push(McpCall {
            call_id: "call-1".to_string(),
            seq: 1,
            server: "novanet".to_string(),
            tool: Some("novanet_traverse".to_string()),
            resource: None,
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1100,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });

        state.filter_query = "describe".to_string();
        let filtered = state.filtered_mcp_calls();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].tool.as_deref(), Some("novanet_describe"));
    }

    #[test]
    fn test_filtered_mcp_calls_matches_resource() {
        let mut state = TuiState::new("test.yaml");
        state.mcp_calls.push(McpCall {
            call_id: "call-0".to_string(),
            seq: 0,
            server: "novanet".to_string(),
            tool: None,
            resource: Some("neo4j://entity/qr-code".to_string()),
            task_id: "task1".to_string(),
            completed: true,
            output_len: Some(100),
            timestamp_ms: 1000,
            params: None,
            response: None,
            is_error: false,
            duration_ms: Some(10),
        });

        state.filter_query = "qr-code".to_string();
        let filtered = state.filtered_mcp_calls();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].resource.as_ref().unwrap().contains("qr-code"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // NOTIFICATION TESTS (TIER 3.4)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_notification_level_icons() {
        assert_eq!(NotificationLevel::Info.icon(), "ℹ");
        assert_eq!(NotificationLevel::Warning.icon(), "⚠");
        assert_eq!(NotificationLevel::Alert.icon(), "🔔");
        assert_eq!(NotificationLevel::Success.icon(), "✓");
        assert_eq!(NotificationLevel::Error.icon(), "✗");
    }

    #[test]
    fn test_notification_constructors() {
        let n = Notification::info("Test info", 1000);
        assert_eq!(n.level, NotificationLevel::Info);
        assert_eq!(n.message, "Test info");
        assert_eq!(n.timestamp_ms, 1000);
        assert!(!n.dismissed);

        let n = Notification::warning("Test warning", 2000);
        assert_eq!(n.level, NotificationLevel::Warning);

        let n = Notification::alert("Test alert", 3000);
        assert_eq!(n.level, NotificationLevel::Alert);

        let n = Notification::success("Test success", 4000);
        assert_eq!(n.level, NotificationLevel::Success);

        let n = Notification::error("Test error", 5000);
        assert_eq!(n.level, NotificationLevel::Error);
    }

    #[test]
    fn test_add_notification() {
        let mut state = TuiState::new("test.yaml");
        assert_eq!(state.notifications.len(), 0);

        state.add_notification(Notification::info("Test 1", 1000));
        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].message, "Test 1");

        state.add_notification(Notification::warning("Test 2", 2000));
        assert_eq!(state.notifications.len(), 2);
    }

    #[test]
    fn test_notification_max_limit() {
        let mut state = TuiState::new("test.yaml");
        state.max_notifications = 3;

        // Add 5 notifications
        for i in 0..5 {
            state.add_notification(Notification::info(format!("Test {}", i), i * 1000));
        }

        // Should only keep last 3
        assert_eq!(state.notifications.len(), 3);
        assert_eq!(state.notifications[0].message, "Test 2");
        assert_eq!(state.notifications[1].message, "Test 3");
        assert_eq!(state.notifications[2].message, "Test 4");
    }

    #[test]
    fn test_active_notifications() {
        let mut state = TuiState::new("test.yaml");

        state.add_notification(Notification::info("Active 1", 1000));
        state.add_notification(Notification::info("Active 2", 2000));
        state.notifications[0].dismissed = true;

        let active: Vec<_> = state.active_notifications().collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].message, "Active 2");
    }

    #[test]
    fn test_active_notification_count() {
        let mut state = TuiState::new("test.yaml");

        state.add_notification(Notification::info("1", 1000));
        state.add_notification(Notification::info("2", 2000));
        state.add_notification(Notification::info("3", 3000));
        assert_eq!(state.active_notification_count(), 3);

        state.notifications[1].dismissed = true;
        assert_eq!(state.active_notification_count(), 2);
    }

    #[test]
    fn test_dismiss_notification() {
        let mut state = TuiState::new("test.yaml");

        state.add_notification(Notification::info("1", 1000));
        state.add_notification(Notification::info("2", 2000));
        state.add_notification(Notification::info("3", 3000));

        // Dismiss most recent
        state.dismiss_notification();
        assert!(state.notifications[2].dismissed);
        assert!(!state.notifications[1].dismissed);
        assert!(!state.notifications[0].dismissed);

        // Dismiss next most recent
        state.dismiss_notification();
        assert!(state.notifications[1].dismissed);
        assert!(!state.notifications[0].dismissed);
    }

    #[test]
    fn test_dismiss_all_notifications() {
        let mut state = TuiState::new("test.yaml");

        state.add_notification(Notification::info("1", 1000));
        state.add_notification(Notification::info("2", 2000));
        state.add_notification(Notification::info("3", 3000));

        state.dismiss_all_notifications();

        assert!(state.notifications.iter().all(|n| n.dismissed));
        assert_eq!(state.active_notification_count(), 0);
    }

    #[test]
    fn test_clear_notifications() {
        let mut state = TuiState::new("test.yaml");

        state.add_notification(Notification::info("1", 1000));
        state.add_notification(Notification::info("2", 2000));
        assert_eq!(state.notifications.len(), 2);

        state.clear_notifications();
        assert_eq!(state.notifications.len(), 0);
    }

    #[test]
    fn test_workflow_completed_adds_notification() {
        let mut state = TuiState::new("test.yaml");
        state.workflow.task_count = 4;
        state.workflow.tasks_completed = 4;

        state.handle_event(
            &EventKind::WorkflowCompleted {
                final_output: std::sync::Arc::new(serde_json::Value::Null),
                total_duration_ms: 5000,
            },
            5000,
        );

        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].level, NotificationLevel::Success);
        assert!(state.notifications[0].message.contains("Magnificent"));
    }

    #[test]
    fn test_workflow_failed_adds_notification() {
        let mut state = TuiState::new("test.yaml");

        state.handle_event(
            &EventKind::WorkflowFailed {
                error: "Something went wrong".to_string(),
                failed_task: None,
            },
            5000,
        );

        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].level, NotificationLevel::Error);
        assert!(state.notifications[0].message.contains("failed"));
    }

    #[test]
    fn test_slow_task_adds_warning() {
        let mut state = TuiState::new("test.yaml");

        // First create the task
        state.tasks.insert(
            "slow-task".to_string(),
            TaskState::new("slow-task".to_string(), vec![]),
        );

        // Slow task (>10s but <30s) should add warning
        state.handle_event(
            &EventKind::TaskCompleted {
                task_id: "slow-task".into(),
                output: std::sync::Arc::new(serde_json::Value::Null),
                duration_ms: 15000,
            },
            15000,
        );

        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].level, NotificationLevel::Warning);
        assert!(state.notifications[0].message.contains("15.0s"));
    }

    #[test]
    fn test_very_slow_task_adds_alert() {
        let mut state = TuiState::new("test.yaml");

        // First create the task
        state.tasks.insert(
            "very-slow-task".to_string(),
            TaskState::new("very-slow-task".to_string(), vec![]),
        );

        // Very slow task (>30s) should add alert
        state.handle_event(
            &EventKind::TaskCompleted {
                task_id: "very-slow-task".into(),
                output: std::sync::Arc::new(serde_json::Value::Null),
                duration_ms: 35000,
            },
            35000,
        );

        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].level, NotificationLevel::Alert);
        assert!(state.notifications[0].message.contains("35.0s"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TIER 4.1: Dirty Flags Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dirty_flags_default() {
        let flags = DirtyFlags::default();
        assert!(!flags.all);
        assert!(!flags.progress);
        assert!(!flags.dag);
        assert!(!flags.novanet);
        assert!(!flags.reasoning);
        assert!(!flags.status);
        assert!(!flags.notifications);
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_mark_all() {
        let mut flags = DirtyFlags::default();
        flags.mark_all();
        assert!(flags.all);
        assert!(flags.any());
    }

    #[test]
    fn test_dirty_flags_clear() {
        let mut flags = DirtyFlags {
            all: true,
            progress: true,
            dag: true,
            novanet: true,
            reasoning: true,
            status: true,
            notifications: true,
        };

        flags.clear();

        assert!(!flags.all);
        assert!(!flags.progress);
        assert!(!flags.dag);
        assert!(!flags.novanet);
        assert!(!flags.reasoning);
        assert!(!flags.status);
        assert!(!flags.notifications);
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_any() {
        let mut flags = DirtyFlags::default();
        assert!(!flags.any());

        flags.progress = true;
        assert!(flags.any());

        flags.progress = false;
        flags.dag = true;
        assert!(flags.any());
    }

    #[test]
    fn test_dirty_flags_is_panel_dirty() {
        // When all is true, all panels are dirty
        let mut flags = DirtyFlags {
            all: true,
            ..Default::default()
        };
        assert!(flags.is_panel_dirty(PanelId::Progress));
        assert!(flags.is_panel_dirty(PanelId::Dag));
        assert!(flags.is_panel_dirty(PanelId::NovaNet));
        assert!(flags.is_panel_dirty(PanelId::Agent));

        // Individual flags
        flags.all = false;
        assert!(!flags.is_panel_dirty(PanelId::Progress));

        flags.progress = true;
        assert!(flags.is_panel_dirty(PanelId::Progress));
        assert!(!flags.is_panel_dirty(PanelId::Dag));

        flags.dag = true;
        assert!(flags.is_panel_dirty(PanelId::Dag));

        flags.novanet = true;
        assert!(flags.is_panel_dirty(PanelId::NovaNet));

        flags.reasoning = true;
        assert!(flags.is_panel_dirty(PanelId::Agent));
    }

    #[test]
    fn test_workflow_started_marks_all_dirty() {
        let mut state = TuiState::new("test.yaml");

        state.handle_event(
            &EventKind::WorkflowStarted {
                task_count: 5,
                generation_id: "gen-123".to_string(),
                workflow_hash: "abc".to_string(),
                nika_version: TEST_VERSION.to_string(),
            },
            0,
        );

        assert!(state.dirty.all);
    }

    #[test]
    fn test_workflow_completed_marks_progress_status_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.dirty.clear();

        state.handle_event(
            &EventKind::WorkflowCompleted {
                final_output: std::sync::Arc::new(serde_json::Value::Null),
                total_duration_ms: 1000,
            },
            1000,
        );

        assert!(state.dirty.progress);
        assert!(state.dirty.status);
        assert!(state.dirty.notifications); // from add_notification
    }

    #[test]
    fn test_task_events_mark_progress_dag_dirty() {
        let mut state = TuiState::new("test.yaml");

        // TaskScheduled
        state.dirty.clear();
        state.handle_event(
            &EventKind::TaskScheduled {
                task_id: "task1".into(),
                dependencies: vec![],
            },
            100,
        );
        assert!(state.dirty.progress);
        assert!(state.dirty.dag);

        // TaskStarted
        state.dirty.clear();
        state.handle_event(
            &EventKind::TaskStarted {
                verb: "infer".into(),
                task_id: "task1".into(),
                inputs: serde_json::json!({}),
            },
            200,
        );
        assert!(state.dirty.progress);
        assert!(state.dirty.dag);

        // TaskCompleted
        state.dirty.clear();
        state.handle_event(
            &EventKind::TaskCompleted {
                task_id: "task1".into(),
                output: std::sync::Arc::new(serde_json::Value::Null),
                duration_ms: 500,
            },
            300,
        );
        assert!(state.dirty.progress);
        assert!(state.dirty.dag);
    }

    #[test]
    fn test_task_failed_marks_status_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.tasks.insert(
            "task1".to_string(),
            TaskState::new("task1".to_string(), vec![]),
        );
        state.dirty.clear();

        state.handle_event(
            &EventKind::TaskFailed {
                task_id: "task1".into(),
                error: "error".into(),
                duration_ms: 100,
            },
            100,
        );

        assert!(state.dirty.progress);
        assert!(state.dirty.dag);
        assert!(state.dirty.status);
    }

    #[test]
    fn test_mcp_events_mark_novanet_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.dirty.clear();

        state.handle_event(
            &EventKind::McpInvoke {
                task_id: "task1".into(),
                mcp_server: "novanet".to_string(),
                tool: Some("describe".to_string()),
                resource: None,
                call_id: "call1".to_string(),
                params: None,
            },
            100,
        );
        assert!(state.dirty.novanet);

        state.dirty.clear();
        state.handle_event(
            &EventKind::McpResponse {
                task_id: "task1".into(),
                output_len: 100,
                call_id: "call1".to_string(),
                duration_ms: 50,
                cached: false,
                is_error: false,
                response: None,
            },
            150,
        );
        assert!(state.dirty.novanet);
    }

    #[test]
    fn test_agent_events_mark_reasoning_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.dirty.clear();

        state.handle_event(
            &EventKind::AgentStart {
                task_id: "task1".into(),
                max_turns: 5,
                mcp_servers: vec![],
            },
            100,
        );
        assert!(state.dirty.reasoning);

        state.dirty.clear();
        state.handle_event(
            &EventKind::AgentTurn {
                task_id: "task1".into(),
                turn_index: 0,
                kind: "started".to_string(),
                metadata: None,
            },
            200,
        );
        assert!(state.dirty.reasoning);

        state.dirty.clear();
        state.handle_event(
            &EventKind::AgentComplete {
                task_id: "task1".into(),
                turns: 1,
                stop_reason: "natural".to_string(),
            },
            300,
        );
        assert!(state.dirty.reasoning);
    }

    #[test]
    fn test_add_notification_marks_notifications_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.dirty.clear();

        state.add_notification(Notification::info("test", 100));
        assert!(state.dirty.notifications);
    }

    #[test]
    fn test_dismiss_notification_marks_notifications_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.add_notification(Notification::info("test", 100));
        state.dirty.clear();

        state.dismiss_notification();
        assert!(state.dirty.notifications);
    }

    #[test]
    fn test_dismiss_all_marks_notifications_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.add_notification(Notification::info("test", 100));
        state.dirty.clear();

        state.dismiss_all_notifications();
        assert!(state.dirty.notifications);
    }

    #[test]
    fn test_clear_notifications_marks_dirty() {
        let mut state = TuiState::new("test.yaml");
        state.add_notification(Notification::info("test", 100));
        state.dirty.clear();

        state.clear_notifications();
        assert!(state.dirty.notifications);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // TIER 4.4: JSON FORMAT CACHE TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_json_cache_new() {
        let cache = JsonFormatCache::new();
        assert_eq!(cache.stats(), (0, 50)); // 0 entries, max 50
    }

    #[test]
    fn test_json_cache_get_or_format_caches() {
        let mut cache = JsonFormatCache::new();

        // First call should format and cache
        let value = serde_json::json!({"name": "test"});
        let result1 = cache.get_or_format("key1", &value);
        assert!(result1.contains("name"));

        // Second call should return cached
        let result2 = cache.get_or_format("key1", &value);
        assert_eq!(result1, result2);
        assert_eq!(cache.stats().0, 1); // 1 entry
    }

    #[test]
    fn test_json_cache_different_keys() {
        let mut cache = JsonFormatCache::new();

        let value1 = serde_json::json!({"a": 1});
        let value2 = serde_json::json!({"b": 2});

        cache.get_or_format("key1", &value1);
        cache.get_or_format("key2", &value2);

        assert_eq!(cache.stats().0, 2); // 2 entries
    }

    #[test]
    fn test_json_cache_invalidate() {
        let mut cache = JsonFormatCache::new();
        let value = serde_json::json!({"test": true});

        cache.get_or_format("key1", &value);
        cache.get_or_format("key2", &value);
        assert_eq!(cache.stats().0, 2);

        cache.invalidate("key1");
        assert_eq!(cache.stats().0, 1);
    }

    #[test]
    fn test_json_cache_invalidate_prefix() {
        let mut cache = JsonFormatCache::new();
        let value = serde_json::json!({"test": true});

        cache.get_or_format("task:abc", &value);
        cache.get_or_format("task:def", &value);
        cache.get_or_format("mcp:xyz", &value);
        assert_eq!(cache.stats().0, 3);

        cache.invalidate_prefix("task:");
        assert_eq!(cache.stats().0, 1); // Only mcp:xyz remains
    }

    #[test]
    fn test_json_cache_clear() {
        let mut cache = JsonFormatCache::new();
        let value = serde_json::json!({"test": true});

        cache.get_or_format("key1", &value);
        cache.get_or_format("key2", &value);
        assert_eq!(cache.stats().0, 2);

        cache.clear();
        assert_eq!(cache.stats().0, 0);
    }

    #[test]
    fn test_json_cache_eviction_on_limit() {
        let mut cache = JsonFormatCache {
            cache: HashMap::new(),
            max_entries: 5, // Small limit for testing
        };

        let value = serde_json::json!({"test": true});

        // Fill cache to limit
        for i in 0..5 {
            cache.get_or_format(&format!("key{}", i), &value);
        }
        assert_eq!(cache.stats().0, 5);

        // Adding one more should trigger eviction
        cache.get_or_format("key_new", &value);
        // Should have fewer entries due to eviction (removes ~10%)
        assert!(cache.stats().0 < 6);
    }

    #[test]
    fn test_workflow_start_clears_json_cache() {
        let mut state = TuiState::new("test.yaml");
        let value = serde_json::json!({"test": true});

        state.json_cache.get_or_format("key1", &value);
        assert_eq!(state.json_cache.stats().0, 1);

        // Workflow start should clear the cache
        state.handle_event(
            &EventKind::WorkflowStarted {
                task_count: 1,
                workflow_hash: "hash-123".into(),
                generation_id: "gen-123".into(),
                nika_version: "0.5.1".into(),
            },
            100,
        );

        assert_eq!(state.json_cache.stats().0, 0);
    }

    #[test]
    fn test_task_started_invalidates_task_cache() {
        let mut state = TuiState::new("test.yaml");
        let value = serde_json::json!({"test": true});

        // Pre-populate cache
        state.json_cache.get_or_format("task:my_task", &value);
        state.json_cache.get_or_format("task:other_task", &value);
        assert_eq!(state.json_cache.stats().0, 2);

        // Task start should invalidate only that task's cache
        state.handle_event(
            &EventKind::TaskStarted {
                verb: "infer".into(),
                task_id: "my_task".into(),
                inputs: serde_json::json!({}),
            },
            100,
        );

        assert_eq!(state.json_cache.stats().0, 1); // other_task remains
    }

    #[test]
    fn test_mcp_response_invalidates_mcp_cache() {
        let mut state = TuiState::new("test.yaml");
        let value = serde_json::json!({"test": true});

        // Pre-populate cache
        state.json_cache.get_or_format("mcp:call-123", &value);
        state.json_cache.get_or_format("mcp:call-456", &value);
        assert_eq!(state.json_cache.stats().0, 2);

        // MCP response should invalidate that call's cache
        state.handle_event(
            &EventKind::McpResponse {
                task_id: "task1".into(),
                output_len: 100,
                call_id: "call-123".into(),
                duration_ms: 50,
                cached: false,
                is_error: false,
                response: None,
            },
            100,
        );

        assert_eq!(state.json_cache.stats().0, 1); // call-456 remains
    }

    // ═══════════════════════════════════════════
    // CHAT OVERLAY STATE TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn test_chat_overlay_state_new() {
        let state = ChatOverlayState::new();
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, ChatOverlayMessageRole::System);
        assert!(state.input.is_empty());
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll, 0);
        assert!(state.history.is_empty());
        assert!(state.history_index.is_none());
        // New streaming fields
        assert!(!state.is_streaming);
        assert!(state.partial_response.is_empty());
        // Model name depends on env vars, so just check it's not empty
        assert!(!state.current_model.is_empty());
    }

    #[test]
    fn test_chat_overlay_streaming() {
        let mut state = ChatOverlayState::new();
        assert!(!state.is_streaming);

        state.start_streaming();
        assert!(state.is_streaming);
        assert!(state.partial_response.is_empty());

        state.append_streaming("Hello ");
        state.append_streaming("world!");
        assert_eq!(state.partial_response, "Hello world!");

        let result = state.finish_streaming();
        assert_eq!(result, "Hello world!");
        assert!(!state.is_streaming);
        assert!(state.partial_response.is_empty());
    }

    #[test]
    fn test_chat_overlay_set_model() {
        let mut state = ChatOverlayState::new();
        state.set_model("gpt-4o-mini");
        assert_eq!(state.current_model, "gpt-4o-mini");
    }

    #[test]
    fn test_chat_overlay_tool_message() {
        let mut state = ChatOverlayState::new();
        state.add_tool_message("Tool output: OK");
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[1].role, ChatOverlayMessageRole::Tool);
        assert_eq!(state.messages[1].content, "Tool output: OK");
    }

    #[test]
    fn test_chat_overlay_insert_char() {
        let mut state = ChatOverlayState::new();
        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.input, "hi");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_chat_overlay_backspace() {
        let mut state = ChatOverlayState::new();
        state.input = "hello".to_string();
        state.cursor = 5;

        state.backspace();
        assert_eq!(state.input, "hell");
        assert_eq!(state.cursor, 4);

        // Backspace at start does nothing
        state.cursor = 0;
        state.backspace();
        assert_eq!(state.input, "hell");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_chat_overlay_delete() {
        let mut state = ChatOverlayState::new();
        state.input = "hello".to_string();
        state.cursor = 0;

        state.delete();
        assert_eq!(state.input, "ello");
        assert_eq!(state.cursor, 0);

        // Delete at end does nothing
        state.cursor = 4;
        state.delete();
        assert_eq!(state.input, "ello");
    }

    #[test]
    fn test_chat_overlay_cursor_movement() {
        let mut state = ChatOverlayState::new();
        state.input = "hello".to_string();
        state.cursor = 3;

        state.cursor_left();
        assert_eq!(state.cursor, 2);

        state.cursor_right();
        assert_eq!(state.cursor, 3);

        state.cursor_home();
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        assert_eq!(state.cursor, 5);

        // Boundary checks
        state.cursor_home();
        state.cursor_left();
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        state.cursor_right();
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_chat_overlay_add_user_message() {
        let mut state = ChatOverlayState::new();
        state.input = "hello Nika".to_string();
        state.cursor = 10;

        let result = state.add_user_message();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "hello Nika");

        // Input should be cleared
        assert!(state.input.is_empty());
        assert_eq!(state.cursor, 0);

        // Message should be added
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[1].role, ChatOverlayMessageRole::User);
        assert_eq!(state.messages[1].content, "hello Nika");

        // History should be updated
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0], "hello Nika");
    }

    #[test]
    fn test_chat_overlay_add_user_message_empty_returns_none() {
        let mut state = ChatOverlayState::new();
        state.input = "   ".to_string(); // Just whitespace

        let result = state.add_user_message();
        assert!(result.is_none());
        assert_eq!(state.messages.len(), 1); // No new message added
    }

    #[test]
    fn test_chat_overlay_add_nika_message() {
        let mut state = ChatOverlayState::new();
        state.add_nika_message("Hello there!");

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[1].role, ChatOverlayMessageRole::Nika);
        assert_eq!(state.messages[1].content, "Hello there!");
    }

    #[test]
    fn test_chat_overlay_history_navigation() {
        let mut state = ChatOverlayState::new();

        // Add some history
        state.history = vec![
            "first message".to_string(),
            "second message".to_string(),
            "third message".to_string(),
        ];

        // Navigate up through history
        state.history_up();
        assert_eq!(state.history_index, Some(2));
        assert_eq!(state.input, "third message");

        state.history_up();
        assert_eq!(state.history_index, Some(1));
        assert_eq!(state.input, "second message");

        state.history_up();
        assert_eq!(state.history_index, Some(0));
        assert_eq!(state.input, "first message");

        // At oldest, doesn't go further
        state.history_up();
        assert_eq!(state.history_index, Some(0));

        // Navigate down
        state.history_down();
        assert_eq!(state.history_index, Some(1));
        assert_eq!(state.input, "second message");

        state.history_down();
        assert_eq!(state.history_index, Some(2));
        assert_eq!(state.input, "third message");

        // Past newest clears input
        state.history_down();
        assert!(state.history_index.is_none());
        assert!(state.input.is_empty());
    }

    #[test]
    fn test_chat_overlay_history_up_empty() {
        let mut state = ChatOverlayState::new();
        // No history
        state.history_up();
        assert!(state.history_index.is_none());
        assert!(state.input.is_empty());
    }

    #[test]
    fn test_chat_overlay_clear() {
        let mut state = ChatOverlayState::new();
        state.add_nika_message("Message 1");
        state.add_nika_message("Message 2");
        state.scroll = 5;

        state.clear();

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, ChatOverlayMessageRole::System);
        assert!(state.messages[0].content.contains("cleared"));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn test_chat_overlay_scroll() {
        let mut state = ChatOverlayState::new();
        assert_eq!(state.scroll, 0);

        state.scroll_up();
        assert_eq!(state.scroll, 1);

        state.scroll_up();
        assert_eq!(state.scroll, 2);

        state.scroll_down();
        assert_eq!(state.scroll, 1);

        state.scroll_down();
        assert_eq!(state.scroll, 0);

        // Can't go below 0
        state.scroll_down();
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn test_tui_mode_chat_overlay_variant() {
        let mode = TuiMode::ChatOverlay;
        assert_eq!(mode, TuiMode::ChatOverlay);
        assert_ne!(mode, TuiMode::Normal);
        assert_ne!(mode, TuiMode::Settings);
    }

    #[test]
    fn test_tui_state_has_chat_overlay() {
        let state = TuiState::new("test.yaml");
        // Chat overlay should be initialized with welcome message
        assert_eq!(state.chat_overlay.messages.len(), 1);
        assert!(state.chat_overlay.input.is_empty());
    }

    #[test]
    fn test_chat_overlay_message_new() {
        let msg = ChatOverlayMessage::new(ChatOverlayMessageRole::User, "test message");
        assert_eq!(msg.role, ChatOverlayMessageRole::User);
        assert_eq!(msg.content, "test message");
    }
}
