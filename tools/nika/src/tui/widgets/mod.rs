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
#![allow(dead_code)]

mod activity_stack;
mod agent_turns;
mod command_palette;
mod dag;
mod dag_ascii;
mod dag_edge;
mod dag_layout;
mod dag_node_box;
mod gauge;
mod header;
mod infer_stream_box;
mod mcp_call_box;
mod mcp_log;
mod scroll_indicator;
mod session_context;
mod sparkline;
mod status_bar;
mod timeline;

pub use agent_turns::{AgentTurns, TurnEntry};

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
pub use sparkline::LatencySparkline;
pub use status_bar::{ConnectionStatus, Provider, StatusBar, StatusMetrics};
pub use timeline::{Timeline, TimelineEntry};
