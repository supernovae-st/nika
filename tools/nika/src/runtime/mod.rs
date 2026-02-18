//! Runtime Module - Workflow execution (v0.2)
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch)
//! - `output`: Output format handling and schema validation
//! - `agent_loop`: Agentic execution with MCP tool calling (v0.2)
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

mod agent_loop;
mod executor;
mod output;
mod runner;

// Re-export public types
pub use agent_loop::{AgentLoop, AgentLoopResult, AgentStatus};
pub use executor::TaskExecutor;
pub use output::make_task_result;
pub use runner::Runner;
