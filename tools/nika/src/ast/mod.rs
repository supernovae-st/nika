//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! Contains parsed Rust types from YAML workflow definitions:
//! - `workflow`: Workflow, Task, Flow, FlowEndpoint
//! - `action`: TaskAction, InferParams, ExecParams, FetchParams
//! - `invoke`: InvokeParams (v0.2 - MCP integration)
//! - `agent`: AgentParams (v0.2 - Agentic execution)
//! - `output`: OutputPolicy, OutputFormat
//!
//! These types represent the "what" - static structure parsed from YAML.
//! For runtime execution, see the `runtime` module.

mod action;
mod agent;
pub mod decompose;
mod invoke;
mod output;
pub mod schema_validator;
mod workflow;

// Re-export all public types
pub use action::{ExecParams, FetchParams, InferParams, TaskAction};
// AgentParams is defined in agent.rs (v0.2 - Agentic execution)
pub use agent::AgentParams;
// InvokeParams is defined in invoke.rs and re-exported here
// (also used by action.rs for TaskAction::Invoke variant)
pub use invoke::InvokeParams;
pub use output::{OutputFormat, OutputPolicy};
pub use workflow::{
    Flow, FlowEndpoint, McpConfigInline, Task, Workflow, SCHEMA_V01, SCHEMA_V02, SCHEMA_V03,
    SCHEMA_V04, SCHEMA_V05,
};
// DecomposeSpec is defined in decompose.rs (v0.5 - Runtime DAG expansion)
pub use decompose::{DecomposeSpec, DecomposeStrategy};
