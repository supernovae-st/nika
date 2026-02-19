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

use super::theme::{MissionPhase, TaskStatus};

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
            output: None,
            error: None,
            tokens: None,
        }
    }
}

/// MCP call record
#[derive(Debug, Clone)]
pub struct McpCall {
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
    /// Total cost in USD
    pub cost_usd: f64,
    /// MCP call count by tool
    pub mcp_calls: HashMap<String, usize>,
    /// Token history (for sparkline)
    pub token_history: Vec<u32>,
    /// Latency history in ms (for sparkline)
    pub latency_history: Vec<u64>,
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
            context_assembly: ContextAssembly::default(),
            agent_turns: Vec::new(),
            streaming_buffer: String::new(),
            agent_max_turns: None,
            focus: PanelId::Progress,
            mode: TuiMode::Normal,
            scroll: HashMap::new(),
            settings: SettingsState::new(config),
            breakpoints: HashSet::new(),
            paused: false,
            step_mode: false,
            metrics: Metrics::default(),
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
            }

            EventKind::WorkflowCompleted { .. } => {
                self.workflow.phase = MissionPhase::MissionSuccess;
                self.current_task = None;
            }

            EventKind::WorkflowFailed { .. } => {
                self.workflow.phase = MissionPhase::Abort;
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
            }

            EventKind::TaskStarted { task_id, .. } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Instant::now());
                }
                self.current_task = Some(task_id.to_string());

                // Update phase
                if self.workflow.phase == MissionPhase::Countdown {
                    self.workflow.phase = MissionPhase::Launch;
                } else {
                    self.workflow.phase = MissionPhase::Orbital;
                }
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

                // Clear agent state if this was an agent task
                self.agent_turns.clear();
                self.streaming_buffer.clear();
                self.agent_max_turns = None;
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
            }

            // ═══════════════════════════════════════════
            // MCP EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpInvoke {
                task_id,
                mcp_server,
                tool,
                resource,
                call_id: _,
            } => {
                let call = McpCall {
                    seq: self.mcp_seq,
                    server: mcp_server.clone(),
                    tool: tool.clone(),
                    resource: resource.clone(),
                    task_id: task_id.to_string(),
                    completed: false,
                    output_len: None,
                    timestamp_ms,
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
            }

            EventKind::McpResponse {
                task_id: _,
                output_len,
                call_id: _,
                duration_ms: _,
                cached: _,
                is_error: _,
            } => {
                // Mark last call as completed
                if let Some(call) = self.mcp_calls.last_mut() {
                    call.completed = true;
                    call.output_len = Some(*output_len);
                }

                // Return to orbital phase
                self.workflow.phase = MissionPhase::Orbital;
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
            }

            // ═══════════════════════════════════════════
            // AGENT EVENTS
            // ═══════════════════════════════════════════
            EventKind::AgentStart { max_turns, .. } => {
                self.agent_turns.clear();
                self.streaming_buffer.clear();
                self.agent_max_turns = Some(*max_turns);
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
            }

            EventKind::AgentComplete { turns, .. } => {
                // Update metrics
                if let Some(last_turn) = self.agent_turns.last() {
                    if let Some(tokens) = last_turn.tokens {
                        self.metrics.token_history.push(tokens);
                    }
                }
                let _ = turns; // Used for logging
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cost_usd,
                ttft_ms,
                ..
            } => {
                self.metrics.input_tokens += input_tokens;
                self.metrics.output_tokens += output_tokens;
                self.metrics.total_tokens += input_tokens + output_tokens;
                self.metrics.cost_usd += cost_usd;
                self.metrics
                    .token_history
                    .push(input_tokens + output_tokens);
                if let Some(ttft) = ttft_ms {
                    self.metrics.latency_history.push(*ttft);
                }
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

        state.handle_event(
            &EventKind::McpInvoke {
                task_id: Arc::from("task1"),
                call_id: "test-call-1".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some("novanet_describe".to_string()),
                resource: None,
            },
            100,
        );

        assert_eq!(state.mcp_calls.len(), 1);
        assert_eq!(
            state.mcp_calls[0].tool,
            Some("novanet_describe".to_string())
        );
        assert!(!state.mcp_calls[0].completed);

        state.handle_event(
            &EventKind::McpResponse {
                task_id: Arc::from("task1"),
                call_id: "test-call-1".to_string(),
                output_len: 1024,
                duration_ms: 100,
                cached: false,
                is_error: false,
            },
            200,
        );

        assert!(state.mcp_calls[0].completed);
        assert_eq!(state.mcp_calls[0].output_len, Some(1024));
    }

    #[test]
    fn test_breakpoint_detection() {
        let mut state = TuiState::new("test.yaml");
        state
            .breakpoints
            .insert(Breakpoint::BeforeTask("task1".to_string()));

        let event = EventKind::TaskStarted {
            task_id: Arc::from("task1"),
            inputs: serde_json::json!({}),
        };
        assert!(state.should_break(&event));

        let event2 = EventKind::TaskStarted {
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
}
