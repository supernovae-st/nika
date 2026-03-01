//! Nika - DAG workflow runner for AI tasks (v0.1)
//!
//! ## Module Architecture (DDD-Inspired)
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
//! │  dag/       DAG structure (Dag, validate)              │
//! │  binding/   Data binding (WiringSpec, ResolvedBindings)       │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    INFRASTRUCTURE LAYER                      │
//! │  store/     State management (DataStore, TaskResult)         │
//! │  event/     Event sourcing (EventLog, EventKind)             │
//! │  provider/  LLM abstraction (rig-core v0.31 wrapper)         │
//! │  util/      Utilities (interner, jsonpath)                   │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Module Responsibilities
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`ast`] | YAML parsing → `Workflow`, `Task`, `TaskAction`, `OutputPolicy` |
//! | [`runtime`] | DAG execution with tokio concurrency |
//! | [`dag`] | Dependency graph with FxHashMap optimization |
//! | [`binding`] | Use block system: entry, resolve, template |
//! | [`store`] | Thread-safe task output storage (DashMap) |
//! | [`event`] | Event sourcing for audit trail |
//! | [`provider`] | LLM provider abstraction (rig-core v0.31) |
//! | [`util`] | String interning, JSONPath parser |
//! | [`error`] | Error types with fix suggestions |

// ═══════════════════════════════════════════════════════════════
// YAML PARSING - serde-saphyr (replaces deprecated serde_yaml)
// Migration: 2026-02-27 - serde_yaml v0.9.34 is deprecated
// This alias allows all modules to continue using `serde_yaml::` syntax
// See: docs/plans/2026-02-27-v014-complete-plan.md Section 14.1
// ═══════════════════════════════════════════════════════════════
pub use serde_saphyr as serde_yaml;

// ═══════════════════════════════════════════════════════════════
// DOMAIN MODEL - YAML → Rust types
// ═══════════════════════════════════════════════════════════════
pub mod ast;

// ═══════════════════════════════════════════════════════════════
// APPLICATION LAYER - Execution logic
// ═══════════════════════════════════════════════════════════════
pub mod binding;
pub mod dag;
pub mod runtime;

// ═══════════════════════════════════════════════════════════════
// INFRASTRUCTURE LAYER - Storage, events, providers
// ═══════════════════════════════════════════════════════════════
pub mod event;
pub mod mcp;
pub mod provider;
pub mod registry;
pub mod store;
pub mod tools;
pub mod tui;
pub mod util;

// ═══════════════════════════════════════════════════════════════
// CROSS-CUTTING - Error handling, configuration
// ═══════════════════════════════════════════════════════════════
pub mod config;
pub mod error;

// ═══════════════════════════════════════════════════════════════
// JOBS DAEMON - Background workflow scheduler (feature-gated)
// ═══════════════════════════════════════════════════════════════
#[cfg(feature = "jobs")]
pub mod jobs;

// ═══════════════════════════════════════════════════════════════
// TEST UTILITIES (cfg(test) or cfg(feature = "test-fixtures"))
// ═══════════════════════════════════════════════════════════════
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_utils;

// ═══════════════════════════════════════════════════════════════
// PUBLIC API RE-EXPORTS
// ═══════════════════════════════════════════════════════════════

// Error types
pub use error::NikaError;

// Config types
pub use config::{mask_api_key, NikaConfig};

// AST types (Domain Model)
pub use ast::{
    AgentParams, ExecParams, FetchParams, Flow, InferParams, InvokeParams, OutputFormat,
    OutputPolicy, Task, TaskAction, Workflow,
};

// Runtime types (Application Layer)
pub use runtime::{Runner, TaskExecutor};

// DAG types
pub use dag::{validate_use_wiring, Dag};

// StableGraph types (v0.9.0) - Stable NodeIndex after deletion
pub use dag::{DagEdge, StableDag};
pub use petgraph::stable_graph::{EdgeIndex as StableEdgeIndex, NodeIndex as StableNodeIndex};

// Binding types
pub use binding::{validate_task_id, ResolvedBindings, UseEntry, WiringSpec};

// Event types
pub use event::{
    calculate_workflow_hash, generate_generation_id, list_traces, Event, EventEmitter, EventKind,
    EventLog, NoopEmitter, TraceInfo, TraceWriter,
};

// Store types
pub use store::{DataStore, TaskResult, TaskStatus};

// MCP types (v0.2)
pub use mcp::{
    ContentBlock, McpClient, McpConfig, ResourceContent, ToolCallRequest, ToolCallResult,
    ToolDefinition,
};
