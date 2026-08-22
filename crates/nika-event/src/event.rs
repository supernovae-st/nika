// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`Event`] — the canonical event envelope.
//!
//! An `Event` is a **pure value** (L0 · zero I/O · zero async). The
//! timestamp is **supplied by the caller** — this crate never reads a
//! clock (that's an L1 effect, `nika-clock`). IDs are likewise supplied,
//! keeping the type deterministic and trivially testable.

use nika_types::id::{CorrelationId, EventId, ExecutionId, RunId};
use nika_types::resource::KeyValue;
use nika_types::timestamp::Timestamp;

use crate::kind::EventKind;

/// A single emitted event in the engine runtime chronicle.
///
/// Construct via [`Event::new`] then chain `with_*` setters (consuming
/// builder · no partial-construction footgun). All fields are public for
/// downstream projection (e.g. the future `nika-connectome` ingest).
///
/// ```
/// use nika_event::{Event, EventKind};
/// use nika_types::id::EventId;
/// use nika_types::timestamp::Timestamp;
/// use uuid::Uuid;
///
/// let ev = Event::new(
///     EventId::new(Uuid::nil()),
///     Timestamp::from_unix_ms(0),
///     EventKind::WorkflowStarted,
/// );
/// assert_eq!(ev.kind, EventKind::WorkflowStarted);
/// assert!(ev.run.is_none());
/// ```
// `Eq` intentionally omitted: `KeyValue`/`Value` carry a float variant
// (PartialEq but not Eq), so the structured-fields vec is only PartialEq.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Event {
    /// Unique identifier for this event (caller-supplied · `UUIDv7` canonical).
    pub id: EventId,
    /// When the event occurred (caller-supplied · L0 never reads a clock).
    pub timestamp: Timestamp,
    /// The taxonomy classification.
    pub kind: EventKind,
    /// The workflow run this event belongs to, if any.
    pub run: Option<RunId>,
    /// The admitted execution this event belongs to, if any.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub execution: Option<ExecutionId>,
    /// Correlation id linking causally-related events across runs, if any.
    pub correlation: Option<CorrelationId>,
    /// Structured key/value fields (provenance · NEVER raw secrets, per
    /// `supernovae-alignment.md` Rule 1 — callers hash sensitive payloads).
    pub fields: Vec<KeyValue>,
}

impl Event {
    /// Construct a minimal event with no run, correlation, or fields.
    #[must_use]
    pub fn new(id: EventId, timestamp: Timestamp, kind: EventKind) -> Self {
        Self {
            id,
            timestamp,
            kind,
            run: None,
            execution: None,
            correlation: None,
            fields: Vec::new(),
        }
    }

    /// Attach a [`RunId`] (consuming builder).
    #[must_use]
    pub fn with_run(mut self, run: RunId) -> Self {
        self.run = Some(run);
        self
    }

    /// Attach an [`ExecutionId`] (consuming builder).
    #[must_use]
    pub fn with_execution(mut self, execution: ExecutionId) -> Self {
        self.execution = Some(execution);
        self
    }

    /// Attach a [`CorrelationId`] (consuming builder).
    #[must_use]
    pub fn with_correlation(mut self, correlation: CorrelationId) -> Self {
        self.correlation = Some(correlation);
        self
    }

    /// Append a single structured field (consuming builder · chainable).
    #[must_use]
    pub fn with_field(mut self, field: KeyValue) -> Self {
        self.fields.push(field);
        self
    }

    /// Replace all fields at once (consuming builder).
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<KeyValue>) -> Self {
        self.fields = fields;
        self
    }

    /// Whether this event marks a terminal workflow state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.kind.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_types::id::ExecutionId;
    use uuid::Uuid;

    fn event() -> Event {
        Event::new(
            EventId::new(Uuid::nil()),
            Timestamp::from_unix_ms(0),
            EventKind::WorkflowStarted,
        )
    }

    #[test]
    fn execution_is_absent_until_attached() {
        assert!(event().execution.is_none());
    }

    #[test]
    fn execution_identity_roundtrips_on_the_event_wire() {
        let execution = ExecutionId::from_bytes([9; 16]);
        let encoded = serde_json::to_string(&event().with_execution(execution)).expect("serialize");
        let decoded: Event = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded.execution, Some(execution));
    }

    #[test]
    fn legacy_event_wire_omits_and_defaults_execution_identity() {
        let encoded = serde_json::to_value(event()).expect("serialize");
        assert!(encoded.get("execution").is_none(), "None stays additive");
        let decoded: Event = serde_json::from_value(encoded).expect("legacy shape decodes");
        assert!(decoded.execution.is_none());
    }
}
