//! Nika - DAG workflow runner for AI tasks
//!
//! ## Architecture
//!
//! - `workflow`: YAML parsing and task definitions
//! - `runner`: DAG execution with tokio concurrency
//! - `datastore`: Thread-safe task output storage (DashMap)
//! - `flow_graph`: Dependency graph with FxHashMap optimization
//! - `template`: Single-pass `{{use.alias}}` resolution
//! - `use_bindings`: Resolved values from `use:` blocks
//! - `use_wiring`: YAML parsing for `use:` block syntax
//! - `validator`: Static workflow validation
//! - `provider`: LLM provider abstraction (Claude, OpenAI)
//! - `event_log`: Event sourcing for audit trail
//! - `interner`: String interning for task IDs
//! - `jsonpath`: Minimal JSONPath parser
//! - `task_executor`: Task action execution
//! - `task_action`: Task action type definitions
//! - `output_policy`: Output format configuration
//! - `error`: Error types with fix suggestions

pub mod datastore;
pub mod error;
pub mod event_log;
pub mod flow_graph;
pub mod interner;
pub mod jsonpath;
pub mod output_policy;
pub mod provider;
pub mod runner;
pub mod task_action;
pub mod task_executor;
pub mod template;
pub mod use_bindings;
pub mod use_wiring;
pub mod validator;
pub mod workflow;

pub use error::NikaError;
pub use event_log::{Event, EventKind, EventLog};
pub use flow_graph::FlowGraph;
pub use output_policy::{OutputFormat, OutputPolicy};
pub use runner::Runner;
pub use task_action::{ExecParams, FetchParams, InferParams, TaskAction};
pub use task_executor::TaskExecutor;
pub use use_bindings::UseBindings;
pub use use_wiring::{UseEntry, UseWiring};
pub use workflow::Workflow;
