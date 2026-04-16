// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event sink trait for structured event emission.
//!
//! `EventSink` is for telemetry — events may be sampled/dropped.
//! `BillingSink` (separate trait) is for billing — never dropped.

use nika_error::NikaError;
use nika_error::id::{EventId, RunId, TraceId};
use serde::{Deserialize, Serialize};

/// A structured event emitted during workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Event {
    /// Event identifier.
    pub id: EventId,
    /// Run this event belongs to.
    pub run_id: RunId,
    /// Trace context.
    pub trace_id: Option<TraceId>,
    /// Event kind (e.g., `"task.started"`, `"infer.complete"`).
    pub kind: String,
    /// Timestamp in Unix epoch milliseconds.
    pub timestamp_ms: u64,
    /// Event payload.
    pub payload: serde_json::Value,
}

impl Event {
    /// Create a new event.
    #[must_use]
    pub fn new(
        id: EventId,
        run_id: RunId,
        kind: impl Into<String>,
        timestamp_ms: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id,
            run_id,
            trace_id: None,
            kind: kind.into(),
            timestamp_ms,
            payload,
        }
    }
}

/// Sink for structured events (telemetry — may be sampled/dropped).
///
/// Production: file, NDJSON, OTLP, etc.
/// Tests: `NullEventSink` (discards).
///
/// For billing (never dropped), see `BillingSink`.
#[trait_variant::make(EventSinkDyn: Send)]
pub trait EventSink: Send + Sync + crate::sealed::Sealed {
    /// Emit an event.
    async fn emit(&self, event: Event) -> Result<(), NikaError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_error::id::EventId;

    #[test]
    fn event_new() {
        let e = Event::new(
            EventId::nil(),
            RunId::nil(),
            "task.started",
            1_700_000_000_000,
            serde_json::json!({"task": "research"}),
        );
        assert_eq!(e.kind, "task.started");
        assert!(e.trace_id.is_none());
    }

    #[test]
    fn event_serde_roundtrip() {
        let e = Event::new(
            EventId::nil(),
            RunId::nil(),
            "test",
            0,
            serde_json::Value::Null,
        );
        let json = serde_json::to_string(&e).expect("serialize");
        let back: Event = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(e.kind, back.kind);
    }

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn event_types_are_send_sync() {
        _assert_send_sync::<Event>();
    }
}
