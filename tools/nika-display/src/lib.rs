//! CLI display renderers for Nika workflow engine.
//!
//! ## Module structure
//! - `bench` — Benchmark comparison display (`nika bench`)
//! - `summary` — Workflow completion and doctor diagnostic summaries
//! - `dag_render` — DAG visualization with double-line borders and status badges
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers
//! - `check` — Pre-flight validation checklist for `nika check`
//! - `live` — Animated LiveRenderer with indicatif spinners and progress bars
//! - `run_renderer` — Factory functions: auto/classic/live renderer selection
//! - `spinner` — Spinner constants and progress bar templates

pub mod bench;
pub mod bench_cache;
pub mod check;
pub mod cli_format;
pub mod colors;
pub mod dag;
mod dag_render;
pub mod detail;
pub mod format_event;
pub mod header;
pub mod icons;
pub mod live;
pub mod renderer;
pub mod run_renderer;
pub mod spinner;
mod summary;

// Re-export check API
pub use bench::{
    format_bench_header, format_bench_summary, format_cost_section, format_profile_section,
    format_quality_section, format_speed_section, print_bench_header, print_bench_summary,
    print_cost_section, print_profile_section, print_quality_section, print_speed_section,
    BenchProviderResult, BenchTaskTiming, Percentiles, QualityScore,
};
pub use check::{
    print_check_header, print_check_summary, print_check_warnings, print_mcp_validation,
    print_phase, print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError,
    PhaseResult,
};
pub use cli_format::{
    hint, key_value, key_value_width, panel, panel_with_content, section_header,
    section_header_with_subtitle, separator, status_line, status_line_with_hint, terminal_width,
    tree_connector, StatusIcon, TREE_BRANCH, TREE_LAST, TREE_PIPE,
};
pub use dag_render::{render_dag, DagTask, DagTaskStatus};
pub use detail::DetailLevel;
pub use live::LiveRenderer;
pub use renderer::{CliRenderer, Renderer};
pub use run_renderer::{auto_renderer, classic_renderer, live_renderer};
pub use renderer::{CanaryCounters, ShieldStats, SpotlightCounters, TrustCounters};
pub use summary::{
    format_doctor_header, format_doctor_summary, format_done_summary, format_run_summary,
    print_doctor_header, print_doctor_summary, print_done_summary, print_run_summary,
    SecuritySummary,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_shield_summary;
