//! Runtime Module - Workflow execution (v0.3)
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch)
//! - `output`: Output format handling and schema validation
//! - `agent_loop`: Agentic execution with MCP tool calling (v0.2, deprecated)
//! - `rig_agent_loop`: Rig-based agentic execution (v0.3)
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

mod agent_loop;
mod executor;
mod output;
mod rig_agent_loop;
mod runner;

// Re-export public types
// Legacy agent loop (deprecated in v0.3.1)
pub use agent_loop::{AgentLoop, AgentLoopResult, AgentStatus};
// New rig-based agent loop (v0.3)
pub use rig_agent_loop::{RigAgentLoop, RigAgentLoopResult, RigAgentStatus};
pub use executor::TaskExecutor;
pub use output::make_task_result;
pub use runner::Runner;
