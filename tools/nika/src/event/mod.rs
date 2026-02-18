//! Event Module - Event Sourcing for workflow execution (v0.2, v0.3)
//!
//! Provides full audit trail with replay capability.
//! Key types:
//! - `Event`: Envelope with id + timestamp + kind
//! - `EventKind`: 16 variants across 5 levels (workflow/task/fine-grained/MCP/agent)
//! - `EventLog`: Thread-safe, append-only log
//! - `EventEmitter`: Trait for dependency injection (v0.3)
//! - `NoopEmitter`: Zero-cost no-op for testing (v0.3)
//! - `TraceWriter`: NDJSON file writer for debugging

mod emitter;
mod log;
mod trace;

// Re-export all public types
pub use emitter::{EventEmitter, NoopEmitter};
pub use log::{Event, EventKind, EventLog};
pub use trace::{
    calculate_workflow_hash, generate_generation_id, list_traces, TraceInfo, TraceWriter,
};
