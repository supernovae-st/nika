//! Event Module - Event Sourcing for workflow execution (v0.2)
//!
//! Provides full audit trail with replay capability.
//! Key types:
//! - `Event`: Envelope with id + timestamp + kind
//! - `EventKind`: 12 variants across 4 levels (workflow/task/fine-grained/MCP)
//! - `EventLog`: Thread-safe, append-only log

mod log;

// Re-export all public types
pub use log::{Event, EventKind, EventLog};
