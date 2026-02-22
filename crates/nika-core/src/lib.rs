//! nika-core — Core types and AST for Nika workflow engine
//!
//! This crate provides the foundational types for Nika:
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
//! │  dag/       DAG structure (FlowGraph, validate)              │
//! │  binding/   Data binding (WiringSpec, ResolvedBindings)      │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    INFRASTRUCTURE LAYER                      │
//! │  store/     State management (DataStore, TaskResult)         │
//! │  event/     Event sourcing (EventLog, EventKind)             │
//! │  util/      Utilities (interner, jsonpath)                   │
//! └──────────────────────────────────────────────────────────────┘
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

// ═══════════════════════════════════════════════════════════════
// DOMAIN MODEL - YAML → Rust types
// ═══════════════════════════════════════════════════════════════
pub mod ast;

// ═══════════════════════════════════════════════════════════════
// APPLICATION LAYER - DAG and binding logic
// ═══════════════════════════════════════════════════════════════
pub mod binding;
pub mod dag;

// ═══════════════════════════════════════════════════════════════
// INFRASTRUCTURE LAYER - Storage, events, utilities
// ═══════════════════════════════════════════════════════════════
pub mod event;
pub mod store;
pub mod util;

// ═══════════════════════════════════════════════════════════════
// CROSS-CUTTING - Error handling, configuration, MCP types
// ═══════════════════════════════════════════════════════════════
pub mod config;
pub mod error;
pub mod mcp_types;

// ═══════════════════════════════════════════════════════════════
// TEST UTILITIES (cfg(test) or cfg(feature = "test-fixtures"))
// ═══════════════════════════════════════════════════════════════
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

#[cfg(any(test, feature = "test-fixtures"))]
// Note: test_utils moved to nika-runtime (needs TaskExecutor)

// ═══════════════════════════════════════════════════════════════
// PUBLIC API RE-EXPORTS
// ═══════════════════════════════════════════════════════════════

// Error types
pub use error::NikaError;

// Config types
pub use config::{NikaConfig, mask_api_key};

// AST types (Domain Model)
pub use ast::{
    AgentParams, DecomposeSpec, ExecParams, FetchParams, Flow, InferParams, InvokeParams,
    OutputFormat, OutputPolicy, Task, TaskAction, Workflow,
};

// DAG types
pub use dag::{FlowGraph, validate_use_wiring};

// Binding types
pub use binding::{ResolvedBindings, UseEntry, WiringSpec, validate_task_id};

// Event types
pub use event::{
    Event, EventEmitter, EventKind, EventLog, NoopEmitter, TraceInfo, TraceWriter,
    calculate_workflow_hash, generate_generation_id, list_traces,
};

// Store types
pub use store::{DataStore, TaskResult, TaskStatus};
