//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! Contains parsed Rust types from YAML workflow definitions:
//! - `workflow`: Workflow, Task, Flow, FlowEndpoint
//! - `action`: TaskAction, InferParams, ExecParams, FetchParams
//! - `invoke`: InvokeParams (v0.2 - MCP integration)
//! - `output`: OutputPolicy, OutputFormat
//!
//! These types represent the "what" - static structure parsed from YAML.
//! For runtime execution, see the `runtime` module.

mod action;
mod invoke;
mod output;
mod workflow;

// Re-export all public types
pub use action::{ExecParams, FetchParams, InferParams, TaskAction};
pub use invoke::InvokeParams;
pub use output::{OutputFormat, OutputPolicy};
pub use workflow::{Flow, FlowEndpoint, Task, Workflow, SCHEMA_V01};
