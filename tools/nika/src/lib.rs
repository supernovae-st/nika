//! Nika - DAG workflow runner for AI tasks
//!
//! ## Module Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                        DOMAIN MODEL                          │
//! │  ast/       YAML → Rust types (Workflow, Task, TaskAction)   │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌──────────────────────────────────────────────────────────────┐
//! │                      APPLICATION LAYER                       │
//! │  runtime/   DAG execution (Runner, TaskExecutor)             │
//! │  dag/       DAG structure (Dag, validate)                    │
//! │  binding/   Data binding (WiringSpec, ResolvedBindings)      │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    INFRASTRUCTURE LAYER                      │
//! │  store/     State management (DataStore, TaskResult)         │
//! │  event/     Event sourcing (EventLog, EventKind)             │
//! │  provider/  LLM abstraction (rig-core wrapper)               │
//! └──────────────────────────────────────────────────────────────┘
//! ```

// YAML parsing alias (serde-saphyr replaces deprecated serde_yaml)
pub use serde_saphyr as serde_yaml;

// Source tracking for precise error locations
pub mod source;

// Public modules (used by main.rs, tests, or benches)
pub mod ast;
pub mod binding;
pub mod config;
pub mod core;
pub mod dag;
pub mod error;
pub mod event;
pub mod init;
pub mod mcp;
pub mod new;
pub mod provider;
pub mod registry;
pub mod runtime;
pub mod store;
pub mod tools;

// Internal modules (only used within the crate)
pub(crate) mod io;
pub(crate) mod util;

// Feature-gated modules
pub mod secrets;
#[cfg(feature = "tui")]
pub mod tui;

#[cfg(feature = "jobs")]
pub mod jobs;

#[cfg(feature = "lsp")]
pub mod lsp;

// Test utilities
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_utils;

// ── Public API re-exports ────────────────────────────────────

// Source tracking
pub use source::{ByteOffset, FileId, SourceFile, SourceRegistry, Span, Spanned};

// Error types
pub use error::NikaError;

// AST types
pub use ast::{
    AgentParams, ExecParams, FetchParams, Flow, InferParams, InvokeParams, OutputFormat,
    OutputPolicy, Task, TaskAction, Workflow,
};

// Runtime
pub use runtime::{Runner, TaskExecutor};

// DAG
pub use dag::{validate_use_wiring, validate_with_bindings, Dag, StableDag};

// Binding
pub use binding::{validate_task_id, ResolvedBindings, UseEntry, WiringSpec};

// Events
pub use event::{list_traces, Event, EventKind, EventLog};

// Store
pub use store::{DataStore, TaskResult, TaskStatus};

// MCP
pub use mcp::{McpClient, McpConfig};
