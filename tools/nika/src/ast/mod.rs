//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! Contains parsed Rust types from YAML workflow definitions:
//! - `workflow`: Workflow, Task, Flow, FlowEndpoint
//! - `action`: TaskAction, InferParams, ExecParams, FetchParams
//! - `invoke`: InvokeParams (v0.2 - MCP integration)
//! - `agent`: AgentParams (v0.2 - Agentic execution)
//! - `output`: OutputPolicy, OutputFormat
//! - `context`: ContextConfig (v0.9 - File loading at workflow start)
//! - `agent_def`: AgentDef (v0.6 - Reusable agent configurations)
//! - `skill_def`: SkillDef, SkillRef (v0.6 - Prompt augmentation)
//! - `include`: IncludeSpec (v0.9 - DAG fusion)
//!
//! These types represent the "what" - static structure parsed from YAML.
//! For runtime execution, see the `runtime` module.

mod action;
mod agent;
mod agent_def;
pub mod context;
pub mod decompose;
pub mod include;
pub mod include_loader;
mod invoke;
pub mod loader;
mod output;
pub mod schema_validator;
mod skill_def;
mod workflow;

// Re-export all public types
pub use action::{ExecParams, FetchParams, InferParams, TaskAction};
// AgentParams + ToolChoice are defined in agent.rs
pub use agent::{AgentParams, ToolChoice};
// AgentDef is defined in agent_def.rs (v0.6 - Reusable agent configurations)
pub use agent_def::AgentDef;
// InvokeParams is defined in invoke.rs and re-exported here
// (also used by action.rs for TaskAction::Invoke variant)
pub use invoke::InvokeParams;
// ContextConfig is defined in context.rs (v0.9 - File loading at workflow start)
pub use context::ContextConfig;
// IncludeSpec is defined in include.rs (v0.9 - DAG fusion)
pub use include::IncludeSpec;
pub use output::{OutputFormat, OutputPolicy};
// SkillDef + SkillRef are defined in skill_def.rs (v0.6 - Prompt augmentation)
pub use skill_def::{SkillDef, SkillRef};
pub use workflow::{
    Flow, FlowEndpoint, McpConfigInline, Task, Workflow, SCHEMA_V01, SCHEMA_V02, SCHEMA_V03,
    SCHEMA_V04, SCHEMA_V05, SCHEMA_V06, SCHEMA_V07, SCHEMA_V08, SCHEMA_V09,
};
// DecomposeSpec is defined in decompose.rs (v0.5 - Runtime DAG expansion)
pub use decompose::{DecomposeSpec, DecomposeStrategy};
// Loader is defined in loader.rs (v0.13 - Multi-format agent/skill loading)
pub use loader::{discover_definitions, load_definition, DefinitionKind, LoadedDefinition};
// Include loader is defined in include_loader.rs (v0.14.2 - DAG fusion)
pub use include_loader::expand_includes;
