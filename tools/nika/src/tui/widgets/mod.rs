//! TUI Widgets
//!
//! Reusable UI components for the TUI panels.
//!
//! - Timeline: Task execution timeline with markers
//! - Gauge: Progress bar with label
//! - Dag: Task dependency graph visualization
//! - Sparkline: Mini chart for metrics
//! - ScrollIndicator: Vertical scrollbar for panels
//! - StatusBar: Bottom status bar with provider/MCP status
//! - Header: Top header with view title and navigation hints

// Pre-built widget library for TUI views.
// Per-module #[allow(dead_code)] for staged widgets awaiting TUI redesign integration.

#[allow(dead_code)]
mod activity_stack;
mod agent_steps;
mod animation;
mod chat_dag_panel;
mod chat_edge_line;
mod chat_node_box;
mod chat_task_queue;
mod command_palette;
mod dag_ascii;
#[allow(dead_code)]
mod dag_edge;
#[allow(dead_code)]
mod dag_layout;
#[allow(dead_code)]
mod dag_node_box;
mod gauge;
mod header;
mod help_overlay;
mod infer_stream_box;
#[allow(dead_code)]
mod matrix_decrypt;
mod matrix_rain;
mod mcp_call_box;
mod mention_system;
mod mission_control;
mod nika_intro;
mod pro_status_bar;
pub mod provider_modal;
#[allow(dead_code)]
mod provider_selector;
mod scroll_indicator;
mod session_context;
#[allow(dead_code)]
mod sparkline;
mod status_bar;
mod status_message;
pub mod task_box;
mod terminal_size;
mod timeline;
pub mod tree;
mod utils;
mod verb_input;
mod verb_type;
mod which_key;

// Micro-widgets for F1-style inline telemetry (staged for TUI redesign Phase 6)
#[allow(dead_code)]
pub mod micro;

// Shared panel components for view composition
pub mod panels;

// Progress widgets for M5 UX Polish
pub mod progress;

// Animation utilities
pub use animation::{AnimationState, AnimationTicker, Easing};
// Chat DAG widgets
pub use chat_dag_panel::{ChatDagPanel, DagEdgeData, DagNodeData};
pub use chat_edge_line::{ChatEdgeLine, ChatPosition};
pub use chat_node_box::{ChatNodeBox, ChatNodeKind, ChatNodeState};
pub use chat_task_queue::{ChatTaskQueue, ChatTaskQueueItem, ChatTaskState, ChatTaskVerb};
pub use provider_modal::*;
// Provider selector types + Verification status + MCP display
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
// Hot/warm/cold activity data types (ActivityStack widget is dead, data types used by ChatView)
pub use activity_stack::{ActivityItem, ActivityTemp};
// Command palette (⌘K)
pub use command_palette::{default_commands, CommandPalette, CommandPaletteState, PaletteCommand};
// Verb type (extracted from deprecated dag.rs)
pub use verb_type::VerbType;
// DAG node box widget for individual task rendering
pub use dag_node_box::{NodeBox, NodeBoxData, NodeBoxMode};
// Complete DAG ASCII visualization widget
pub use dag_ascii::DagAscii;
// Note: DagLayout and DagEdge are kept as modules but not re-exported.
// They're internal implementation details for dag_ascii.
pub use gauge::Gauge;
pub use header::Header;
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
// Verb input parsing for Chat view
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
// Progress widgets (DownloadProgress + TaskProgress removed — never rendered)
// Shared utilities
pub use utils::centered_rect;
