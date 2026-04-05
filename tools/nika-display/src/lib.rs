//! CLI display renderers for Nika workflow engine.
//!
//! This crate provides terminal output formatting for workflow execution:
//! - `renderer` — Classic append-only CliRenderer
//! - `live` — Animated LiveRenderer with indicatif spinners and progress bars
//! - `format_event` — Event → formatted string helpers
//! - `summary` — Workflow completion summaries
//! - `bench` — Benchmark comparison display
//! - `check` — Pre-flight validation checklist
//! - `dag_render` — DAG visualization with double-line borders
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers
//! - `cli_format` — CLI formatting primitives (panels, trees, hints)
