//! Nika - DAG workflow runner for AI tasks (v0.1)
//!
//! ## Core Components
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`workflow`] | YAML parsing → `Workflow`, `Task`, `Flow` |
//! | [`runner`] | DAG execution with tokio concurrency |
//! | [`task_executor`] | Individual task execution (infer, exec, fetch) |
//! | [`datastore`] | Thread-safe task output storage (DashMap) |
//! | [`flow_graph`] | Dependency graph with FxHashMap optimization |
//!
//! ## Use Block System (`use:`)
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`use_wiring`] | YAML spec for `use:` block (3 forms) |
//! | [`use_bindings`] | Runtime resolution → `{{use.alias}}` values |
//! | [`validator`] | DAG validation of use: references |
//! | [`template`] | Single-pass `{{use.alias}}` substitution |
//! | [`jsonpath`] | Minimal JSONPath parser ($.a.b, $.a[0]) |
//!
//! ## Infrastructure
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`provider`] | LLM abstraction (Claude, OpenAI, Mock) |
//! | [`event_log`] | Event sourcing for audit trail |
//! | [`interner`] | String interning for task IDs |
//! | [`output`] | Output format handling |
//! | [`output_policy`] | Output format configuration |
//! | [`error`] | Error types with fix suggestions |
//! | [`task_action`] | Task action type definitions |

pub mod datastore;
pub mod error;
pub mod event_log;
pub mod flow_graph;
pub mod interner;
pub mod jsonpath;
pub mod output;
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
