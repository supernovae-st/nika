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
//! │  dag/       DAG structure (FlowGraph, validate)              │
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
pub mod store;
pub mod tui;
pub mod util;

// ═══════════════════════════════════════════════════════════════
// CROSS-CUTTING - Error handling, configuration
// ═══════════════════════════════════════════════════════════════
pub mod config;
pub mod error;

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
pub use dag::{validate_use_wiring, FlowGraph};

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
