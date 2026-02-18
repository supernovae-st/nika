//! TUI Panel Components
//!
//! Individual panels for the TUI layout:
//! - Workflow progress
//! - Graph traversal
//! - Context assembled
//! - Agent reasoning

// Allow dead_code and unused imports in TUI panels - scaffolding for MVP 5
#![allow(dead_code)]
#![allow(unused_imports)]

mod context;
mod graph;
mod progress;
mod reasoning;

pub use context::ContextPanel;
pub use graph::GraphPanel;
pub use progress::ProgressPanel;
pub use reasoning::ReasoningPanel;
