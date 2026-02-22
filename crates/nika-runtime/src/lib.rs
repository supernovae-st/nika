//! nika-runtime — Runtime execution engine for Nika workflow engine
//!
//! Contains the runtime execution components:
//! - `runner`: DAG execution with tokio concurrency
//! - `executor`: Individual task execution (infer, exec, fetch, invoke, agent)
//!   - Includes decompose: modifier expansion (v0.5 MVP 8)
//! - `output`: Output format handling and schema validation
//! - `rig_agent_loop`: Rig-based agentic execution (v0.3+)
//! - `spawn`: Nested agent spawning (v0.5 MVP 8)
//! - `tools`: Built-in agent tools (read, write, edit, grep, glob)
//!
//! ## 5 Semantic Verbs
//!
//! | Verb | Purpose |
//! |------|---------|
//! | `infer:` | LLM text generation |
//! | `exec:` | Shell command execution |
//! | `fetch:` | HTTP request |
//! | `invoke:` | MCP tool call |
//! | `agent:` | Multi-turn agentic loop |
//!
//! ## Architecture
//!
//! ```text
//! nika-runtime/
//! ├── runner.rs         # Workflow orchestration
//! ├── executor.rs       # Task dispatch (5 verbs + for_each)
//! ├── rig_agent_loop.rs # Agentic execution (rig-core)
//! ├── spawn.rs          # SpawnAgentTool (rig::ToolDyn)
//! ├── output.rs         # Output format handling
//! └── tools/            # Built-in agent tools
//!     ├── read.rs       # File reading
//!     ├── write.rs      # File writing
//!     ├── edit.rs       # File editing
//!     ├── grep.rs       # Content search
//!     └── glob.rs       # File pattern matching
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

mod executor;
mod output;
mod rig_agent_loop;
mod runner;
pub mod spawn;
pub mod tools;

// Re-export public types
pub use executor::TaskExecutor;
pub use output::make_task_result;
pub use rig_agent_loop::{RigAgentLoop, RigAgentLoopResult, RigAgentStatus};
pub use runner::Runner;
pub use spawn::{SpawnAgentParams, SpawnAgentTool};

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_utils;
