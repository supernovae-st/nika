//! TUI Panel Components
//!
//! Individual panels for the TUI layout:
//! - Workflow progress
//! - Graph traversal
//! - Context assembled
//! - Agent reasoning

mod progress;
mod graph;
mod context;
mod reasoning;

pub use progress::ProgressPanel;
pub use graph::GraphPanel;
pub use context::ContextPanel;
pub use reasoning::ReasoningPanel;
