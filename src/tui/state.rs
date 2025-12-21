//! AppState - Domain Layer
//!
//! Central state management with selectors for deriving view state.
//! Updated for v4.5 architecture with 7 keywords.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::workflow::{TaskKeyword, Workflow};

// ─────────────────────────────────────────────────────────────────────────────
// Core State
// ─────────────────────────────────────────────────────────────────────────────

/// Main application state
#[derive(Debug)]
pub struct AppState {
    // Workflow info
    pub workflow_name: String,
    pub workflow_path: Option<String>,

    // Execution state
    pub status: WorkflowStatus,
    pub start_time: Option<Instant>,
    pub elapsed: Duration,

    // Tasks
    pub tasks: HashMap<String, TaskState>,
    pub task_order: Vec<String>,

    // Agents
    pub agents: HashMap<String, AgentState>,

    // Context tracking
    pub tokens: TokenUsage,
    pub context_history: Vec<ContextSnapshot>,

    // Connections
    pub mcp_servers: Vec<McpServerState>,
    pub skills: Vec<SkillInfo>,

    // Activity log
    pub events: VecDeque<ActivityEvent>,
    pub max_events: usize,

    // UI state
    pub focus: PanelFocus,
    pub scroll_positions: HashMap<Panel, usize>,
    pub should_quit: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workflow_name: String::from("No workflow loaded"),
            workflow_path: None,
            status: WorkflowStatus::Idle,
            start_time: None,
            elapsed: Duration::ZERO,
            tasks: HashMap::new(),
            task_order: Vec::new(),
            agents: HashMap::new(),
            tokens: TokenUsage::default(),
            context_history: Vec::new(),
            mcp_servers: Vec::new(),
            skills: Vec::new(),
            events: VecDeque::new(),
            max_events: 100,
            focus: PanelFocus::Dag,
            scroll_positions: HashMap::new(),
            should_quit: false,
        }
    }
}

impl AppState {
    /// Create new state with a workflow
    pub fn with_workflow(workflow: &Workflow, path: &str) -> Self {
        // Extract workflow name from path
        let workflow_name = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path)
            .to_string();

        let mut state = Self {
            workflow_name,
            workflow_path: Some(path.to_string()),
            ..Self::default()
        };

        // Initialize tasks (v4.5 - keyword-based)
        for task in &workflow.tasks {
            state.task_order.push(task.id.clone());
            let keyword = task.keyword();
            let task_type_str = keyword.as_ref().map(|k| format!("{}", k)).unwrap_or_else(|| "unknown".to_string());
            let paradigm = Paradigm::from_keyword(keyword.as_ref());
            state.tasks.insert(
                task.id.clone(),
                TaskState {
                    id: task.id.clone(),
                    task_type: task_type_str,
                    paradigm,
                    status: TaskStatus::Pending,
                    progress: 0.0,
                    output: None,
                    error: None,
                },
            );
        }

        state
    }

    /// Add an activity event
    pub fn push_event(&mut self, event: ActivityEvent) {
        self.events.push_front(event);
        if self.events.len() > self.max_events {
            self.events.pop_back();
        }
    }

    /// Update elapsed time
    pub fn tick(&mut self) {
        if let Some(start) = self.start_time {
            self.elapsed = start.elapsed();
        }
    }

    /// Get completed task count
    pub fn completed_tasks(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .count()
    }

    /// Get total task count
    pub fn total_tasks(&self) -> usize {
        self.tasks.len()
    }

    /// Get task progress as percentage
    pub fn task_progress(&self) -> f32 {
        if self.tasks.is_empty() {
            return 0.0;
        }
        (self.completed_tasks() as f32 / self.total_tasks() as f32) * 100.0
    }

    /// Get context usage as percentage
    pub fn context_usage(&self) -> f32 {
        if self.tokens.limit == 0 {
            return 0.0;
        }
        (self.tokens.total as f32 / self.tokens.limit as f32) * 100.0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow Status
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowStatus {
    Idle,
    Loading,
    Running,
    Paused,
    Completed,
    Failed,
}

impl std::fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "IDLE"),
            Self::Loading => write!(f, "LOADING"),
            Self::Running => write!(f, "RUNNING"),
            Self::Paused => write!(f, "PAUSED"),
            Self::Completed => write!(f, "COMPLETED"),
            Self::Failed => write!(f, "FAILED"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Task State
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TaskState {
    pub id: String,
    pub task_type: String,
    pub paradigm: Paradigm,
    pub status: TaskStatus,
    pub progress: f32,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paradigm {
    Pure,     // ⚡
    Context,  // 🧠
    Isolated, // 🤖
    Unknown,
}

impl Paradigm {
    /// Determine paradigm from v4.5 keyword
    pub fn from_keyword(keyword: Option<&TaskKeyword>) -> Self {
        match keyword {
            Some(TaskKeyword::Agent) => Self::Context,    // 🧠 Main context
            Some(TaskKeyword::Subagent) => Self::Isolated, // 🤖 Separate context
            Some(_) => Self::Pure,                         // ⚡ All tool keywords
            None => Self::Unknown,
        }
    }

    /// Determine paradigm from task type string (legacy/extended types)
    pub fn from_type(task_type: &str) -> Self {
        match task_type {
            "agent" | "prompt" | "context" => Self::Context,
            "subagent" | "spawn" | "isolated" => Self::Isolated,
            "shell" | "http" | "mcp" | "function" | "llm" | "pure" | "data" | "action" | "tool" => Self::Pure,
            t if t.starts_with("nika/") => {
                // Extended types - check known mappings
                match t {
                    "nika/transform" | "nika/validate" | "nika/format" | "nika/parse"
                    | "nika/merge" | "nika/split" | "nika/filter" | "nika/map" => Self::Pure,
                    "nika/analyze" | "nika/generate" | "nika/review" | "nika/summarize"
                    | "nika/decide" => Self::Context,
                    "nika/code" | "nika/test" | "nika/research" => Self::Isolated,
                    _ => Self::Unknown,
                }
            }
            _ => Self::Unknown,
        }
    }

    /// Get icon for paradigm
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pure => "⚡",
            Self::Context => "🧠",
            Self::Isolated => "🤖",
            Self::Unknown => "?",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent State
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentState {
    pub id: String,
    pub name: String,
    pub paradigm: Paradigm,
    pub status: AgentStatus,
    pub activity: f32, // 0.0 - 100.0
    pub last_message: Option<String>,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Streaming,
    ToolUse,
    Terminated,
}

// ─────────────────────────────────────────────────────────────────────────────
// Token Usage
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub total: u32,
    pub limit: u32,
    pub cost: f64,
}

impl TokenUsage {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    pub timestamp: Instant,
    pub tokens: u32,
    pub event: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Connections
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct McpServerState {
    pub name: String,
    pub status: ConnectionStatus,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub loaded: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Activity Events
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub timestamp: Instant,
    pub event_type: ActivityEventType,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityEventType {
    Info,
    TaskStart,
    TaskComplete,
    TaskError,
    AgentMessage,
    ToolUse,
    Warning,
    Error,
}

impl ActivityEvent {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            timestamp: Instant::now(),
            event_type: ActivityEventType::Info,
            message: message.into(),
        }
    }

    pub fn task_start(task_id: &str) -> Self {
        Self {
            timestamp: Instant::now(),
            event_type: ActivityEventType::TaskStart,
            message: format!("Started task: {}", task_id),
        }
    }

    pub fn task_complete(task_id: &str) -> Self {
        Self {
            timestamp: Instant::now(),
            event_type: ActivityEventType::TaskComplete,
            message: format!("Completed task: {}", task_id),
        }
    }

    pub fn task_error(task_id: &str, error: &str) -> Self {
        Self {
            timestamp: Instant::now(),
            event_type: ActivityEventType::TaskError,
            message: format!("Task {} failed: {}", task_id, error),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self.event_type {
            ActivityEventType::Info => "ℹ️",
            ActivityEventType::TaskStart => "▶️",
            ActivityEventType::TaskComplete => "✅",
            ActivityEventType::TaskError => "❌",
            ActivityEventType::AgentMessage => "💬",
            ActivityEventType::ToolUse => "🔧",
            ActivityEventType::Warning => "⚠️",
            ActivityEventType::Error => "❌",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UI State
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelFocus {
    Dag,
    Session,
    Subagents,
    Activity,
    Connections,
    Skills,
    Memory,
    Context,
}

impl PanelFocus {
    pub fn next(&self) -> Self {
        match self {
            Self::Dag => Self::Session,
            Self::Session => Self::Subagents,
            Self::Subagents => Self::Activity,
            Self::Activity => Self::Connections,
            Self::Connections => Self::Skills,
            Self::Skills => Self::Memory,
            Self::Memory => Self::Context,
            Self::Context => Self::Dag,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Dag => Self::Context,
            Self::Session => Self::Dag,
            Self::Subagents => Self::Session,
            Self::Activity => Self::Subagents,
            Self::Connections => Self::Activity,
            Self::Skills => Self::Connections,
            Self::Memory => Self::Skills,
            Self::Context => Self::Memory,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Dag,
    Session,
    Subagents,
    Activity,
    Connections,
    Skills,
    Memory,
    Context,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paradigm_from_type() {
        assert_eq!(Paradigm::from_type("context"), Paradigm::Context);
        assert_eq!(Paradigm::from_type("isolated"), Paradigm::Isolated);
        assert_eq!(Paradigm::from_type("pure"), Paradigm::Pure);
        assert_eq!(Paradigm::from_type("nika/transform"), Paradigm::Pure);
        assert_eq!(Paradigm::from_type("nika/analyze"), Paradigm::Context);
        assert_eq!(Paradigm::from_type("nika/code"), Paradigm::Isolated);
    }

    #[test]
    fn test_workflow_status_display() {
        assert_eq!(format!("{}", WorkflowStatus::Running), "RUNNING");
        assert_eq!(format!("{}", WorkflowStatus::Completed), "COMPLETED");
    }

    #[test]
    fn test_panel_focus_cycle() {
        let focus = PanelFocus::Dag;
        assert_eq!(focus.next(), PanelFocus::Session);
        assert_eq!(focus.prev(), PanelFocus::Context);
    }

    #[test]
    fn test_app_state_defaults() {
        let state = AppState::default();
        assert_eq!(state.status, WorkflowStatus::Idle);
        assert_eq!(state.task_progress(), 0.0);
        assert_eq!(state.context_usage(), 0.0);
    }
}
