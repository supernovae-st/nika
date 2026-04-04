//! Nika Engine — Embeddable workflow execution engine
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
//! │  binding/   Data binding (WithSpec, ResolvedBindings)        │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    INFRASTRUCTURE LAYER                      │
//! │  store/     State management (RunContext, TaskResult)         │
//! │  event/     Event sourcing (EventLog, EventKind)             │
//! │  provider/  LLM abstraction (rig-core wrapper)               │
//! └──────────────────────────────────────────────────────────────┘
//! ```

// YAML parsing alias (serde-saphyr)
pub use serde_saphyr as serde_yaml;

// Source tracking for precise error locations
pub mod source;

// Public modules (used by main.rs, tests, or benches)
pub mod ast;
pub mod binding;
pub mod config;
pub mod core;
pub mod dag;
pub mod display;
pub mod error;
pub mod error_domains;
pub mod event;
pub mod mcp;
pub mod media;
pub mod new;
pub mod provider;
pub mod registry;
pub mod runtime;
pub mod store;
pub mod tools;

// Shared internal modules (pub for cross-crate access by nika-tui)
pub mod io;
pub mod util;

// Feature-gated modules
pub mod secrets;

#[cfg(feature = "lsp")]
pub mod lsp;

// ── Public API re-exports ────────────────────────────────────

// Source tracking
pub use source::{ByteOffset, FileId, SourceFile, SourceRegistry, Span, Spanned};

// Error types
pub use error::NikaError;

// AST types
pub use ast::{
    AgentParams, ExecParams, FetchParams, InferParams, InvokeParams, Task, TaskAction, Workflow,
};

// Runtime
pub use runtime::{Runner, TaskExecutor};

// DAG
pub use dag::{validate_bindings, Dag, StableDag};

// Binding
pub use binding::{validate_task_id, BindingEntry, BindingSpec, ResolvedBindings};

// Events
pub use event::{list_traces, prune_traces, Event, EventKind, EventLog};

// Store
pub use store::{RunContext, TaskResult};

// MCP
pub use mcp::{McpClient, McpConfig};
