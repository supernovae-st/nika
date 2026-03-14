//! TUI Widgets
//!
//! Reusable UI components for the TUI panels.
//!
//! - Timeline: Task execution timeline with markers
//! - Gauge: Progress bar with label
//! - Dag: Task dependency graph visualization
//! - McpLog: MCP call history display
//! - AgentTurns: Agent turn history display
//! - Sparkline: Mini chart for metrics
//! - ScrollIndicator: Vertical scrollbar for panels
//! - StatusBar: Bottom status bar with provider/MCP status
//! - Header: Top header with view title and navigation hints

// Allow unused code in widgets - many are planned for future TUI enhancements
// Kept blanket allow - 40+ widgets in pre-built library
#![allow(dead_code)]

mod activity_stack;
mod agent_steps;
mod agent_turns;
mod animation;
mod chat_dag_panel;
mod chat_edge_line;
mod chat_node_box;
mod chat_task_queue;
mod command_palette;
mod dag;
mod dag_ascii;
mod dag_edge;
mod dag_layout;
mod dag_node_box;
mod gauge;
mod header;
mod help_overlay;
mod infer_stream_box;
mod matrix_decrypt;
mod matrix_rain;
mod mcp_call_box;
mod mcp_log;
mod mention_system;
mod mission_control;
mod nika_intro;
mod pro_status_bar;
pub mod provider_modal;
mod provider_selector;
mod scroll_indicator;
mod session_context;
mod sparkline;
mod status_bar;
mod status_message;
pub mod task_box;
mod terminal_size;
mod timeline;
pub mod tree;
mod verb_input;
mod which_key;

// Shared panel components for view composition
pub mod panels;

// Progress widgets for M5 UX Polish
pub mod progress;

pub use agent_turns::{AgentTurns, TurnEntry};
// Animation utilities
pub use animation::{AnimationState, AnimationTicker, Easing};
// Chat DAG widgets
pub use chat_dag_panel::{ChatDagPanel, DagEdgeData, DagNodeData};
pub use chat_edge_line::{ChatEdgeLine, ChatPosition};
pub use chat_node_box::{ChatNodeBox, ChatNodeKind, ChatNodeState};
pub use chat_task_queue::{ChatTaskQueue, ChatTaskQueueItem, ChatTaskState, ChatTaskVerb};
pub use provider_modal::*;
// Provider selector types + Verification status + MCP display
// Note: ProviderSelector/ProviderSelectorState removed in v0.8.8 (replaced by Provider Modal)
pub use provider_selector::{
    McpServerDisplay, ModelInfo, ProviderInfo, SelectorSection, VerifyStatus,
};

// === Chat UX Enrichment Widgets ===
// Session context bar for tokens, cost, MCP status
pub use session_context::{
    ActiveOperation, McpServerInfo, McpStatus, SessionContext, SessionContextBar,
};
// MCP call visualization (data types + widget for ChatView)
pub use mcp_call_box::{McpCallBox, McpCallData, McpCallStatus, DEFAULT_MAX_RETRIES};
// Streaming inference display (data types + widget for ChatView)
pub use infer_stream_box::{InferStatus, InferStreamBox, InferStreamData};
// Hot/warm/cold activity stack
pub use activity_stack::{ActivityItem, ActivityStack, ActivityTemp};
// Command palette (⌘K)
pub use command_palette::{default_commands, CommandPalette, CommandPaletteState, PaletteCommand};
// DAG widgets
pub use dag::{Dag, DagNode, EdgeState, VerbType};
// DAG node box widget for individual task rendering
pub use dag_node_box::{NodeBox, NodeBoxData, NodeBoxMode};
// Complete DAG ASCII visualization widget
pub use dag_ascii::DagAscii;
// Note: DagLayout and DagEdge are kept as modules but not re-exported.
// They're internal implementation details for dag_ascii.
pub use gauge::Gauge;
pub use header::Header;
pub use mcp_log::{McpEntry, McpLog};
pub use scroll_indicator::{ScrollHint, ScrollIndicator};
pub use sparkline::{
    AnimatedLatencySparkline, LatencyHistory, LatencySparkline, SparklineAnimation,
};
pub use status_bar::{ConnectionStatus, Provider, StatusBar, StatusMetrics, WorkflowPhase};
pub use timeline::{Timeline, TimelineEntry};
// Pro status bar for Chat view (Claude Code-inspired)
pub use pro_status_bar::{ChatModeIndicator, ProStatusBar, SessionMetrics};
// Mission Control panel for Chat view
pub use mission_control::{
    ContextItem, ContextStatus, CurrentVerb, MemoryFile, MemoryKind, MissionControlPanel,
    TurnMetrics,
};
// Verb input system for Chat view
pub use verb_input::{ChatVerb, ParsedInput, SystemCommand, VerbIndicator, VerbPrompt};
// Mention system for Chat view
pub use mention_system::{
    highlight_mentions, Mention, MentionAutocomplete, MentionAutocompleteState, MentionSuggestion,
    MentionTrigger, MentionType,
};
// Agent steps for Claude Code-like feedback
pub use agent_steps::{
    AgentPhase, AgentPhaseIndicator, AgentStep, AgentStepGroup, AgentStepsWidget, StepStatus,
    TokenUsage, ToolCallMetadata,
};
// Terminal size handling
pub use terminal_size::{
    check_terminal_size, LayoutMode, TerminalTooSmallOverlay, COMPACT_WIDTH, MIN_HEIGHT, MIN_WIDTH,
    WIDE_WIDTH,
};
// Help overlay
pub use help_overlay::{HelpOverlay, HelpOverlayState, HelpSection, HELP_SECTIONS};
// Status messages
pub use status_message::{StatusLevel, StatusMessage, StatusMessageWidget, StatusQueue};
// Matrix Rain effect
pub use matrix_rain::MatrixRain;
// Nika Intro animation
pub use nika_intro::{IntroPhase, NikaIntro, NikaIntroState};
// Matrix Decrypt effect
pub use matrix_decrypt::{DecryptVerb, MatrixDecrypt, MultiLineDecrypt, StreamingDecrypt};
// Task Box widgets
pub use task_box::{
    exit, http, status, AgentBox, BoxState, ExecBox, FetchBox, InferBox, InvokeBox, TaskBox,
    TaskBoxWidget, VerbColor, BRAILLE_SPINNER,
};
// Tree widget
pub use tree::{GitStatus, NodeId, NodeKind, TreeAction, TreeColors, TreeNode, TreeState};
// Which-key popup
pub use which_key::{WhichKey, WhichKeyBinding, WhichKeyGroup, WhichKeyState};
// Panel components
pub use panels::{
    BrowserAction, BrowserPanel, InfoPanel, TaskBoxFlow, TaskListAction, TaskListPanel,
};
// Progress widgets
pub use progress::{DownloadProgress, DownloadStatus, TaskProgress, TaskProgressStatus};
