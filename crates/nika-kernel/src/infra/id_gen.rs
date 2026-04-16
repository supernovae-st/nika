// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ID generation trait for deterministic testing.
//!
//! Production: `UUIDv7` + random bytes.
//! Tests: deterministic sequences for reproducibility.

use nika_error::id::{EventId, RunId, SpanId, TraceId};

/// Generator for unique identifiers.
pub trait IdGenerator: Send + Sync {
    /// Generate a new run ID.
    fn new_run_id(&self) -> RunId;
    /// Generate a new event ID.
    fn new_event_id(&self) -> EventId;
    /// Generate a new trace ID.
    fn new_trace_id(&self) -> TraceId;
    /// Generate a new span ID.
    fn new_span_id(&self) -> SpanId;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn id_generator_is_send_sync() {
        _assert_send_sync::<dyn IdGenerator>();
    }
}
