//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `summary` — Workflow completion and doctor diagnostic summaries
//! - `dag_render` — DAG visualization with double-line borders and status badges
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers
//! - `check` — Pre-flight validation checklist for `nika check`

pub mod check;
pub mod colors;
pub mod dag;
mod dag_render;
pub mod detail;
pub mod header;
pub mod icons;
pub mod renderer;
mod summary;

// Re-export check API
pub use check::{
    print_check_header, print_check_summary, print_mcp_validation, print_phase,
    print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError, PhaseResult,
};
pub use dag_render::{render_dag, DagTask, DagTaskStatus};
pub use detail::DetailLevel;
pub use renderer::CliRenderer;
pub use summary::{print_doctor_header, print_doctor_summary, print_done_summary};

#[cfg(test)]
mod tests;
