//! TUI Widgets
//!
//! Reusable UI components for the TUI panels.
//!
//! - Timeline: Task execution timeline with markers
//! - Gauge: Progress bar with label
//! - Dag: Task dependency graph visualization
//! - McpLog: MCP call history display
//! - AgentTurns: Agent turn history display
//! - Spinner: Animated spinner for loading states
//! - Sparkline: Mini chart for metrics (planned)
//! - BigText: FIGlet-style headers (planned)

mod agent_turns;
mod dag;
mod gauge;
mod mcp_log;
mod spinner;
mod timeline;

pub use agent_turns::{AgentTurns, TurnEntry};
pub use dag::{Dag, DagNode};
pub use gauge::Gauge;
pub use mcp_log::{McpEntry, McpLog};
// Spinner widgets - exported for future panel use
pub use spinner::{ProgressDots, Spinner, BRAILLE_SPINNER, DOT_SPINNER, PULSE_SPINNER};
pub use timeline::{Timeline, TimelineEntry};
