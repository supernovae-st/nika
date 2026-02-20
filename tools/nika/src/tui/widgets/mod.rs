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
//! - TabBar: Tab bar for panel switching
//! - Sparkline: Mini chart for metrics
//! - ScrollIndicator: Vertical scrollbar for panels
//! - BigText: FIGlet-style headers (planned)

// Allow unused code in widgets - many are planned for future TUI enhancements
#![allow(dead_code)]

mod agent_turns;
mod dag;
mod gauge;
mod header;
mod mcp_log;
mod scroll_indicator;
mod sparkline;
mod spinner;
mod status_bar;
mod tabs;
mod timeline;
mod yaml_view;

pub use agent_turns::{AgentTurns, TurnEntry};
// DAG widgets - some exported for future TUI features
#[allow(unused_imports)]
pub use dag::{Dag, DagNode, EdgeState, VerbType};
pub use gauge::Gauge;
// Header widget - exported for future view use
#[allow(unused_imports)]
pub use header::Header;
// StatusBar widget - exported for future view use
pub use mcp_log::{McpEntry, McpLog};
#[allow(unused_imports)]
pub use status_bar::{ConnectionStatus, KeyHint, Provider, StatusBar, StatusMetrics};
// Sparkline widgets - some exported for future panel use
#[allow(unused_imports)]
pub use sparkline::{BorderedSparkline, LatencyHistory, LatencySparkline, MiniSparkline};
// Spinner widgets - exported for future panel use
#[allow(unused_imports)]
pub use spinner::{
    ParticleBurst, ProgressDots, PulseText, ShakeText, Spinner, BRAILLE_SPINNER, COSMIC_SPINNER,
    DOT_SPINNER, ORBIT_SPINNER, PULSE_SPINNER, ROCKET_SPINNER, STARS_SPINNER,
};
// Tab widgets - exported for future panel use
#[allow(unused_imports)]
pub use tabs::{TabBar, TabIndicator};
pub use timeline::{Timeline, TimelineEntry};
// Scroll indicator widgets - for scrollable panels
#[allow(unused_imports)]
pub use scroll_indicator::{ScrollHint, ScrollIndicator};
// YAML view widget - exported for future panel use
#[allow(unused_imports)]
pub use yaml_view::YamlView;
