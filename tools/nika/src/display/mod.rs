//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `legacy` — Original display functions (to be gradually replaced)
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers
//! - `check` — Pre-flight validation checklist for `nika check`

pub mod check;
pub mod colors;
pub mod dag;
pub mod detail;
pub mod header;
pub mod icons;
mod legacy;
pub mod renderer;

// Re-export legacy API so nothing breaks
pub use check::{
    print_check_header, print_check_summary, print_mcp_validation, print_phase,
    print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError, PhaseResult,
};
pub use detail::DetailLevel;
pub use legacy::*;
pub use renderer::CliRenderer;

#[cfg(test)]
mod tests;
