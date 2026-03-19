//! Event Module - Event Sourcing for workflow execution
//!
//! Provides full audit trail with replay capability.
//! Key types:
//! - `Event`: Envelope with id + timestamp + kind
//! - `EventKind`: 37 variants across 12 categories (workflow/task/fine-grained/MCP/context/agent/guardrail/builtin/artifact/media/structured-output/media-cleanup)
//! - `EventLog`: Thread-safe, append-only log
//! - `EventEmitter`: Trait for dependency injection
//! - `NoopEmitter`: Zero-cost no-op for testing
//! - `TraceWriter`: NDJSON file writer for debugging
//! - `AgentTurnMetadata`: Agent turn response metadata

mod emitter;
mod log;
mod trace;

// Re-export all public types
pub use emitter::{EventEmitter, NoopEmitter};
pub use log::{AgentTurnMetadata, ContextSource, Event, EventKind, EventLog, ExcludedItem};
pub use trace::{
    calculate_workflow_hash, generate_generation_id, list_traces, prune_traces, TraceInfo,
    TraceWriter,
};
