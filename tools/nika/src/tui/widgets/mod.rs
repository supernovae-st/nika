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
// v0.22: Kept blanket allow - 40+ widgets in pre-built library
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

pub use agent_turns::{AgentTurns, TurnEntry};
// Animation utilities (v0.10.4)
pub use animation::{AnimationState, AnimationTicker, Easing};
// Chat DAG widgets (v0.10.0-v0.10.4)
pub use chat_dag_panel::{ChatDagPanel, DagEdgeData, DagNodeData};
pub use chat_edge_line::{ChatEdgeLine, ChatPosition};
pub use chat_node_box::{ChatNodeBox, ChatNodeKind, ChatNodeState};
pub use chat_task_queue::{ChatTaskQueue, ChatTaskQueueItem, ChatTaskState, ChatTaskVerb};
pub use provider_modal::*;
// Ollama health check (v0.8.2) + Verification status + MCP display
// Note: ProviderSelector/ProviderSelectorState removed in v0.8.8 (replaced by Provider Modal)
pub use provider_selector::{
    check_ollama_available, check_ollama_available_async, McpServerDisplay, ModelInfo,
    ProviderInfo, SelectorSection, VerifyStatus,
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
// Mission Control panel for Chat view (v0.7.3)
pub use mission_control::{
    ContextItem, ContextStatus, CurrentVerb, MemoryFile, MemoryKind, MissionControlPanel,
    TurnMetrics,
};
// Verb input system for Chat view (v0.7.3)
pub use verb_input::{ChatVerb, ParsedInput, SystemCommand, VerbIndicator, VerbPrompt};
// Mention system for Chat view (v0.7.3)
pub use mention_system::{
    highlight_mentions, Mention, MentionAutocomplete, MentionAutocompleteState, MentionSuggestion,
    MentionTrigger, MentionType,
};
// Agent steps for Claude Code-like feedback (v0.7.3, enhanced v0.8)
// v0.8.1: Added AgentPhase and AgentPhaseIndicator for real-time status
pub use agent_steps::{
    AgentPhase, AgentPhaseIndicator, AgentStep, AgentStepGroup, AgentStepsWidget, StepStatus,
    TokenUsage, ToolCallMetadata,
};
// Terminal size handling (v0.8 - graceful degradation)
pub use terminal_size::{
    check_terminal_size, LayoutMode, TerminalTooSmallOverlay, COMPACT_WIDTH, MIN_HEIGHT, MIN_WIDTH,
    WIDE_WIDTH,
};
// Help overlay (v0.8 - ? or F1)
pub use help_overlay::{HelpOverlay, HelpOverlayState, HelpSection, HELP_SECTIONS};
// Status messages (v0.8 - user feedback)
pub use status_message::{StatusLevel, StatusMessage, StatusMessageWidget, StatusQueue};
// Matrix Rain effect (v0.8 - WOW dither effect for background)
pub use matrix_rain::MatrixRain;
// Nika Intro animation (v0.9.1 - ASCII art explosion into rain)
pub use nika_intro::{IntroPhase, NikaIntro, NikaIntroState};
// Matrix Decrypt effect (v0.8 - streaming text reveal with verb themes)
pub use matrix_decrypt::{DecryptVerb, MatrixDecrypt, MultiLineDecrypt, StreamingDecrypt};
// Task Box widgets (v0.8.2 - structured verb boxes)
pub use task_box::{
    exit, http, status, AgentBox, BoxState, ExecBox, FetchBox, InferBox, InvokeBox, TaskBox,
    TaskBoxWidget, VerbColor, BRAILLE_SPINNER,
};
// Tree widget (v0.20 - VS Code-like file tree)
pub use tree::{GitStatus, NodeId, NodeKind, TreeAction, TreeColors, TreeNode, TreeState};
// Which-key popup (v0.21.2 - vim-style leader key hints)
pub use which_key::{WhichKey, WhichKeyBinding, WhichKeyGroup, WhichKeyState};
