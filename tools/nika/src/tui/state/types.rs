//! Core TUI State Types
//!
//! Fundamental types for TUI state management including:
//! - Animation frame constants
//! - Panel identifiers
//! - Workflow and task state
//! - MCP call tracking
//! - Agent turn state
//! - Metrics aggregation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::event::{ContextSource, ExcludedItem};

use super::super::theme::{MissionPhase, TaskStatus};
use super::super::widgets::task_box::TokenVelocity;

// ═══════════════════════════════════════════════════════════════════════════════
// ANIMATION FRAME CONSTANTS (v0.9.x)
// ═══════════════════════════════════════════════════════════════════════════════
//
// These constants define the animation frame standard for the TUI.
// Some are reserved for future use as widgets migrate to the standard.

/// Target FPS for smooth animations
#[allow(dead_code)]
pub const TARGET_FPS: u8 = 60;

/// TuiState.frame modulo (1-second cycle at 60fps)
pub const FRAME_CYCLE: u8 = 60;

/// Frame divisor for fast spinners (~20 FPS)
#[allow(dead_code)]
pub const FRAME_DIV_FAST: u8 = 3;

/// Frame divisor for standard spinners (~15 FPS)
#[allow(dead_code)]
pub const FRAME_DIV_STANDARD: u8 = 4;

/// Frame divisor for normal spinners (~10 FPS)
pub const FRAME_DIV_NORMAL: u8 = 6;

/// Frame divisor for cursor blink (~7.5 FPS)
#[allow(dead_code)]
pub const FRAME_DIV_BLINK: u8 = 8;

/// Frame divisor for slow animations (~6 FPS)
#[allow(dead_code)]
pub const FRAME_DIV_SLOW: u8 = 10;

/// Frame divisor for very slow animations (~4 FPS)
pub const FRAME_DIV_GLACIAL: u8 = 15;

// ═══════════════════════════════════════════════════════════════════════════════
// PANEL IDENTIFIERS
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// CHAT PANEL
// ═══════════════════════════════════════════════════════════════════════════════

/// Panel identifiers for Chat View focus management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChatPanel {
    /// Conversation history panel (left main area)
    #[default]
    Conversation,
    /// Activity stack panel (right sidebar)
    Activity,
    /// Input field (bottom)
    Input,
}

#[allow(dead_code)] // v0.8: API methods for future use
impl ChatPanel {
    /// Get all panels in order for Tab cycling
    pub fn all() -> &'static [ChatPanel] {
        &[
            ChatPanel::Conversation,
            ChatPanel::Activity,
            ChatPanel::Input,
        ]
    }

    /// Get next panel (Tab key)
    pub fn next(self) -> ChatPanel {
        match self {
            ChatPanel::Conversation => ChatPanel::Activity,
            ChatPanel::Activity => ChatPanel::Input,
            ChatPanel::Input => ChatPanel::Conversation,
        }
    }

    /// Get previous panel (Shift+Tab)
    pub fn prev(self) -> ChatPanel {
        match self {
            ChatPanel::Conversation => ChatPanel::Input,
            ChatPanel::Activity => ChatPanel::Conversation,
            ChatPanel::Input => ChatPanel::Activity,
        }
    }

    /// Panel title for display
    pub fn title(&self) -> &'static str {
        match self {
            ChatPanel::Conversation => "CONVERSATION",
            ChatPanel::Activity => "ACTIVITY STACK",
            ChatPanel::Input => "INPUT",
        }
    }

    /// Panel icon
    pub fn icon(&self) -> &'static str {
        match self {
            ChatPanel::Conversation => "💬",
            ChatPanel::Activity => "🎯",
            ChatPanel::Input => "✏️",
        }
    }

    /// Check if this panel supports scrolling
    pub fn is_scrollable(&self) -> bool {
        matches!(self, ChatPanel::Conversation | ChatPanel::Activity)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TUI MODE
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// WORKFLOW STATE
// ═══════════════════════════════════════════════════════════════════════════════

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
    /// Workflow is paused
    pub paused: bool,
    /// P2 Fix: Phase before pause (for proper restoration on resume)
    pub phase_before_pause: Option<MissionPhase>,
    /// Request to execute one step while paused (single-step debugging)
    pub step_requested: bool,
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
            paused: false,
            phase_before_pause: None,
            step_requested: false,
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

// ═══════════════════════════════════════════════════════════════════════════════
// TASK STATE
// ═══════════════════════════════════════════════════════════════════════════════

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
    /// Provider name (e.g., "claude", "openai")
    pub provider: Option<String>,
    /// Model name (e.g., "claude-sonnet-4-6")
    pub model: Option<String>,
    /// Prompt length in chars
    pub prompt_len: Option<usize>,
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
            provider: None,
            model: None,
            prompt_len: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MCP CALL
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// CONTEXT ASSEMBLY
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// AGENT TURN STATE
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// SPAWNED AGENT
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// BREAKPOINT
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// METRICS
// ═══════════════════════════════════════════════════════════════════════════════

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
    /// Provider call count
    pub provider_calls: usize,
    /// Last model used (for status display)
    pub last_model: Option<String>,
    /// Token velocity tracker for sparkline display (v0.13)
    pub token_velocity: TokenVelocity,
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEMPLATE RESOLUTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Template resolution record for observability
#[derive(Debug, Clone)]
pub struct TemplateResolution {
    /// Task ID where resolution occurred
    pub task_id: String,
    /// Original template expression (e.g., "{{use.ctx}}")
    pub template: String,
    /// Resolved value (truncated for display)
    pub result: String,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DIRTY FLAGS (TIER 4.1)
// ═══════════════════════════════════════════════════════════════════════════════

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_id_navigation() {
        assert_eq!(PanelId::Progress.next(), PanelId::Dag);
        assert_eq!(PanelId::Dag.next(), PanelId::NovaNet);
        assert_eq!(PanelId::NovaNet.next(), PanelId::Agent);
        assert_eq!(PanelId::Agent.next(), PanelId::Progress);

        assert_eq!(PanelId::Progress.prev(), PanelId::Agent);
        assert_eq!(PanelId::Dag.prev(), PanelId::Progress);
    }

    #[test]
    fn test_panel_id_metadata() {
        assert_eq!(PanelId::Progress.number(), 1);
        assert_eq!(PanelId::Dag.number(), 2);
        assert_eq!(PanelId::Progress.title(), "MISSION CONTROL");
        assert_eq!(PanelId::Progress.icon(), "◉");
    }

    #[test]
    fn test_chat_panel_navigation() {
        assert_eq!(ChatPanel::Conversation.next(), ChatPanel::Activity);
        assert_eq!(ChatPanel::Activity.next(), ChatPanel::Input);
        assert_eq!(ChatPanel::Input.next(), ChatPanel::Conversation);

        assert_eq!(ChatPanel::Conversation.prev(), ChatPanel::Input);
        assert!(ChatPanel::Conversation.is_scrollable());
        assert!(!ChatPanel::Input.is_scrollable());
    }

    #[test]
    fn test_tui_mode_default() {
        assert_eq!(TuiMode::default(), TuiMode::Normal);
    }

    #[test]
    fn test_workflow_state_progress() {
        let mut state = WorkflowState::new("test.nika.yaml".to_string());
        assert_eq!(state.progress_pct(), 0.0);

        state.task_count = 10;
        state.tasks_completed = 5;
        assert_eq!(state.progress_pct(), 50.0);
    }

    #[test]
    fn test_workflow_state_step_requested() {
        let mut state = WorkflowState::new("test.nika.yaml".to_string());

        // Initial state: not paused, no step requested
        assert!(!state.paused);
        assert!(!state.step_requested);

        // Pause workflow
        state.paused = true;

        // Request a step while paused
        state.step_requested = true;
        assert!(state.step_requested);

        // After step is consumed, reset the flag
        state.step_requested = false;
        assert!(!state.step_requested);

        // Verify step_requested can be set independently of paused
        state.paused = false;
        state.step_requested = true;
        assert!(state.step_requested);
    }

    #[test]
    fn test_task_state_new() {
        let task = TaskState::new("test-task".to_string(), vec!["dep1".to_string()]);
        assert_eq!(task.id, "test-task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.dependencies, vec!["dep1".to_string()]);
    }

    #[test]
    fn test_context_assembly_default() {
        let ctx = ContextAssembly::default();
        assert!(ctx.sources.is_empty());
        assert_eq!(ctx.total_tokens, 0);
        assert!(!ctx.truncated);
    }

    #[test]
    fn test_metrics_default() {
        let metrics = Metrics::default();
        assert_eq!(metrics.total_tokens, 0);
        assert_eq!(metrics.cost_usd, 0.0);
        assert!(metrics.mcp_calls.is_empty());
    }
}
