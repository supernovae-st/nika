//! Event Module - Event Sourcing for workflow execution (v0.1)
//!
//! Provides full audit trail with replay capability.
//! Key types:
//! - `Event`: Envelope with id + timestamp + kind
//! - `EventKind`: 10 variants across 3 levels (workflow/task/fine-grained)
//! - `EventLog`: Thread-safe, append-only log

mod log;

// Re-export all public types
pub use log::{Event, EventKind, EventLog};
