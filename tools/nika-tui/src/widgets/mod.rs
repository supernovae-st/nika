// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

mod activity_stack;
mod agent_steps;
mod animation;
mod command_palette;
mod confirm_dialog;
mod dag;
mod gauge;
mod header;
mod help_overlay;
mod infer_stream_box;
mod matrix_decrypt;
mod mcp_call_box;
mod mention_system;
mod mission_control;
mod pro_status_bar;
pub mod provider_modal;
mod provider_selector;
mod scroll_indicator;
mod session_context;
pub mod stars;
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

// Micro-widgets: small inline visual elements for TaskBox rendering
pub mod micro;

// Shared panel components for view composition
pub mod panels;

// Animation utilities
pub use animation::{AnimationState, AnimationTicker, Easing};
// Chat DAG widgets (canonical location: views/chat/widgets/, re-exported for public API)
pub use crate::views::chat_widgets::{
    ChatDagPanel, ChatNodeKind, ChatNodeState, ChatTaskQueue, ChatTaskQueueItem, ChatTaskState,
    ChatTaskVerb, DagEdgeData, DagNodeData,
};
// Confirmation dialog
pub use confirm_dialog::{ConfirmAction, ConfirmDialog};
pub use provider_modal::*;
pub use provider_selector::VerifyStatus;

// === Chat UX Enrichment Widgets ===
// Session context bar for tokens, cost, MCP status
pub use session_context::{
    ActiveOperation, McpServerInfo, McpStatus, SessionContext, SessionContextBar,
};
// MCP call visualization (data types + widget for ChatView)
pub use mcp_call_box::{McpCallBox, McpCallData, McpCallStatus, DEFAULT_MAX_RETRIES};
// Streaming inference display (data types for ChatView)
pub use infer_stream_box::{InferStatus, InferStreamData};
// Hot/warm/cold activity data types (ActivityStack widget is dead, data types used by ChatView)
pub use activity_stack::{ActivityItem, ActivityTemp};
// Command palette
pub use command_palette::{default_commands, CommandPalette, CommandPaletteState, PaletteCommand};
// Verb type (extracted from deprecated dag.rs)
pub use verb_type::VerbType;
// DAG visualization subsystem (layout, edges, node boxes, composite widget)
pub use dag::{DagAscii, NodeBox, NodeBoxData, NodeBoxMode};
pub use gauge::Gauge;
pub use header::Header;
pub use scroll_indicator::{ScrollHint, ScrollIndicator};
pub use status_bar::{
    ConnectionStatus, Provider, StatusBar, StatusMetrics, StatusSeverity, WorkflowPhase,
};
pub use timeline::{Timeline, TimelineEntry};
// Pro status bar for Chat view (Claude Code-inspired)
pub use pro_status_bar::{ChatModeIndicator, ProStatusBar, SessionMetrics};
// Mission Control panel for Chat view
pub use mission_control::{
    ContextItem, ContextStatus, CurrentVerb, MemoryFile, MemoryKind, MissionControlPanel,
    TurnMetrics,
};
// Verb input parsing for Chat view (data types only, widget rendering stripped)
pub use verb_input::{ChatVerb, ParsedInput, SystemCommand};
// Mention system for Chat view
pub use mention_system::{
    highlight_mentions, Mention, MentionAutocomplete, MentionAutocompleteState, MentionSuggestion,
    MentionTrigger, MentionType,
};
// Agent phase indicator
pub use agent_steps::{AgentPhase, AgentPhaseIndicator};
// Terminal size handling
pub use terminal_size::{
    check_terminal_size, LayoutMode, TerminalTooSmallOverlay, COMPACT_WIDTH, MIN_HEIGHT, MIN_WIDTH,
    WIDE_WIDTH,
};
// Help overlay
pub use help_overlay::{HelpOverlay, HelpOverlayState, HelpSection, HELP_SECTIONS};
// Status messages
pub use status_message::{StatusLevel, StatusMessage, StatusMessageWidget, StatusQueue};
// Matrix Decrypt effect (StreamingDecrypt wraps MatrixDecrypt for stateful rendering)
pub use matrix_decrypt::{DecryptVerb, StreamingDecrypt};
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
// Progress widgets (DownloadProgress + TaskProgress removed -- never rendered)
// Shared utilities
pub use stars::{star_tick, StarField};
pub use utils::centered_rect;
