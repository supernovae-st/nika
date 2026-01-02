//! Engine Module - Workflow execution (v0.1)
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch)
//! - `output`: Output format handling and schema validation
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

mod executor;
mod output;
mod runner;

// Re-export public types
pub use executor::TaskExecutor;
pub use output::make_task_result;
pub use runner::Runner;
