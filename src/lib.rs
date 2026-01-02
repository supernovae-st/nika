//! Nika - DAG workflow runner for AI tasks

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
