//! Runtime Bridge - Connector Layer
//!
//! Abstracts workflow execution for the TUI.

mod mock;

pub use mock::MockRuntime;

use async_trait::async_trait;
use std::path::Path;
use tokio_stream::Stream;

use super::state::Paradigm;

// ─────────────────────────────────────────────────────────────────────────────
// Runtime Bridge Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Bridge trait for workflow execution
#[async_trait]
pub trait RuntimeBridge: Send + Sync {
    /// Load a workflow file
    async fn load_workflow(&self, path: &Path) -> anyhow::Result<WorkflowInfo>;

    /// Start workflow execution
    async fn start(&self) -> anyhow::Result<()>;

    /// Pause execution
    async fn pause(&self) -> anyhow::Result<()>;

    /// Resume execution
    async fn resume(&self) -> anyhow::Result<()>;

    /// Abort execution
    async fn abort(&self) -> anyhow::Result<()>;

    /// Get event stream
    fn events(&self) -> Box<dyn Stream<Item = RuntimeEvent> + Send + Unpin>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow Info
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorkflowInfo {
    pub name: String,
    pub path: String,
    pub task_count: usize,
    pub flow_count: usize,
    pub tasks: Vec<TaskInfo>,
    pub flows: Vec<FlowInfo>,
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: String,
    pub task_type: String,
    pub paradigm: Paradigm,
}

#[derive(Debug, Clone)]
pub struct FlowInfo {
    pub source: String,
    pub target: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime Events
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    // Workflow lifecycle
    WorkflowStarted {
        id: String,
        name: String,
    },
    WorkflowCompleted {
        duration_ms: u64,
        tasks_completed: usize,
        total_tokens: u32,
    },
    WorkflowError {
        error: String,
    },
    WorkflowPaused,
    WorkflowResumed,

    // Task events
    TaskStarted {
        id: String,
        paradigm: Paradigm,
    },
    TaskProgress {
        id: String,
        progress: f32,
    },
    TaskCompleted {
        id: String,
        output: Option<String>,
    },
    TaskError {
        id: String,
        error: String,
    },

    // Agent events
    AgentSpawned {
        id: String,
        name: String,
        paradigm: Paradigm,
    },
    AgentMessage {
        id: String,
        content: String,
    },
    AgentThinking {
        id: String,
    },
    AgentToolUse {
        id: String,
        tool: String,
    },
    AgentTerminated {
        id: String,
    },

    // Context events
    TokensUsed {
        input: u32,
        output: u32,
        total: u32,
        cost: f64,
    },
    ContextSummarized {
        before: u32,
        after: u32,
    },

    // Connection events
    McpConnected {
        server: String,
        tools: Vec<String>,
    },
    McpDisconnected {
        server: String,
    },
    SkillLoaded {
        name: String,
    },
}

impl RuntimeEvent {
    /// Get a short description of the event for logging
    pub fn description(&self) -> String {
        match self {
            Self::WorkflowStarted { name, .. } => format!("Workflow started: {}", name),
            Self::WorkflowCompleted {
                tasks_completed, ..
            } => {
                format!("Workflow completed ({} tasks)", tasks_completed)
            }
            Self::WorkflowError { error } => format!("Workflow error: {}", error),
            Self::WorkflowPaused => "Workflow paused".to_string(),
            Self::WorkflowResumed => "Workflow resumed".to_string(),
            Self::TaskStarted { id, paradigm } => {
                format!("Task started: {} {}", paradigm.icon(), id)
            }
            Self::TaskProgress { id, progress } => format!("Task {}: {:.0}%", id, progress),
            Self::TaskCompleted { id, .. } => format!("Task completed: {}", id),
            Self::TaskError { id, error } => format!("Task {} failed: {}", id, error),
            Self::AgentSpawned { name, paradigm, .. } => {
                format!("Agent spawned: {} {}", paradigm.icon(), name)
            }
            Self::AgentMessage { id, content } => {
                let preview = if content.len() > 50 {
                    format!("{}...", &content[..50])
                } else {
                    content.clone()
                };
                format!("Agent {}: {}", id, preview)
            }
            Self::AgentThinking { id } => format!("Agent {} thinking...", id),
            Self::AgentToolUse { id, tool } => format!("Agent {} using tool: {}", id, tool),
            Self::AgentTerminated { id } => format!("Agent terminated: {}", id),
            Self::TokensUsed { total, cost, .. } => format!("Tokens: {} (${:.4})", total, cost),
            Self::ContextSummarized { before, after } => {
                format!("Context summarized: {} → {}", before, after)
            }
            Self::McpConnected { server, .. } => format!("MCP connected: {}", server),
            Self::McpDisconnected { server } => format!("MCP disconnected: {}", server),
            Self::SkillLoaded { name } => format!("Skill loaded: {}", name),
        }
    }
}
