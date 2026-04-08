// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event Module - Event Sourcing for workflow execution
//!
//! Provides full audit trail with replay capability.
//! Key types:
//! - `Event`: Envelope with id + timestamp + kind
//! - `EventKind`: 58 variants across 15 categories
//! - `EventLog`: Thread-safe, append-only log
//! - `EventEmitter`: Trait for dependency injection
//! - `NoopEmitter`: Zero-cost no-op for testing
//! - `TraceWriter`: NDJSON file writer for debugging
//! - `AgentTurnMetadata`: Agent turn response metadata

mod emitter;
pub mod error;
mod log;
mod trace;
pub mod types;

// Re-export all public types
pub use emitter::{EventEmitter, EventSink, NoopEmitter};
pub use error::EventError;
pub use log::{AgentTurnMetadata, ContextSource, Event, EventKind, EventLog, ExcludedItem};
pub use trace::{
    calculate_workflow_hash, generate_generation_id, list_traces, prune_traces, TraceInfo,
    TraceWriter,
};
pub use types::{AgentStopReason, AgentTurnKind, FinishReason, GuardrailType, Severity};
