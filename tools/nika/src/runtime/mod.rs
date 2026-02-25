//! Runtime Module - Workflow execution (v0.5)
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch, invoke, agent)
//!   - Includes decompose: modifier expansion (v0.5 MVP 8 Phase 4)
//! - `output`: Output format handling and schema validation
//! - `rig_agent_loop`: Rig-based agentic execution (v0.3+)
//! - `spawn`: Nested agent spawning (v0.5 MVP 8 Phase 2)
//! - `chat_workflow`: Chat-as-DAG wrapper (v0.9.1)
//! - `builtin`: Builtin nika:* tools (v0.9.3)
//!
//! This module represents the "how" - runtime execution.
//! For static structure, see the `ast` module.

pub mod builtin;
pub mod chat_workflow;
mod executor;
mod output;
mod rig_agent_loop;
mod runner;
pub mod spawn;

// Re-export public types
pub use builtin::BuiltinTool;
pub use chat_workflow::{ChatMessage, ChatWorkflow, Role};
pub use executor::TaskExecutor;
pub use output::make_task_result;
pub use rig_agent_loop::{RigAgentLoop, RigAgentLoopResult, RigAgentStatus};
pub use runner::Runner;
pub use spawn::{SpawnAgentParams, SpawnAgentTool};
