//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `summary` — Workflow completion and doctor diagnostic summaries
//! - `dag_render` — DAG visualization with double-line borders and status badges
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers
//! - `check` — Pre-flight validation checklist for `nika check`
//! - `live` — Animated LiveRenderer with indicatif spinners and progress bars
//! - `run_renderer` — Dispatch enum: Live (animated) vs Classic (append-only)
//! - `spinner` — Spinner constants and progress bar templates

pub mod check;
pub mod colors;
pub mod dag;
mod dag_render;
pub mod detail;
pub(crate) mod format_event;
pub mod header;
pub mod icons;
pub mod live;
pub mod renderer;
pub mod run_renderer;
pub mod spinner;
mod summary;

// Re-export check API
pub use check::{
    print_check_header, print_check_summary, print_mcp_validation, print_phase,
    print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError, PhaseResult,
};
pub use dag_render::{render_dag, DagTask, DagTaskStatus};
pub use detail::DetailLevel;
pub use live::LiveRenderer;
pub use renderer::CliRenderer;
pub use run_renderer::RunRenderer;
pub use summary::{
    format_doctor_header, format_doctor_summary, format_done_summary, format_run_summary,
    print_doctor_header, print_doctor_summary, print_done_summary, print_run_summary,
};

#[cfg(test)]
mod tests;
