// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema version types for event and trace format versioning.

use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Event schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct EventSchemaVersion {
    /// Version number.
    pub version: u16,
}

impl EventSchemaVersion {
    /// Current schema version.
    pub const CURRENT: Self = Self { version: 1 };

    /// Create a schema version.
    #[must_use]
    pub fn new(version: u16) -> Self {
        Self { version }
    }
}

impl fmt::Display for EventSchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event-schema-v{}", self.version)
    }
}

/// Trace format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct TraceFormatVersion {
    /// Version number.
    pub version: u16,
}

impl TraceFormatVersion {
    /// Current trace format version — `trace_format: 2` (spec 13 §trace):
    /// terminal task events carry `outcome: {class, cause}` + the payload
    /// per class. Format 1 is the pre-W5 photograph; engines do not emit
    /// it past that chapter. Parity-pinned against the vendored pack's
    /// `outcome_transitions.trace_format` by the runtime's outcome tests.
    pub const CURRENT: Self = Self { version: 2 };

    /// Create a trace format version.
    #[must_use]
    pub fn new(version: u16) -> Self {
        Self { version }
    }
}

impl fmt::Display for TraceFormatVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trace-format-v{}", self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_schema_current() {
        assert_eq!(EventSchemaVersion::CURRENT.version, 1);
    }

    #[test]
    fn event_schema_display() {
        assert_eq!(EventSchemaVersion::new(2).to_string(), "event-schema-v2");
    }

    #[test]
    fn event_schema_ordering() {
        assert!(EventSchemaVersion::new(1) < EventSchemaVersion::new(2));
    }

    #[test]
    fn trace_format_current() {
        // Spec 13 (W5): the cause axis is a semantic change under
        // readable JSON — the graph_format: 3 precedent — so the format
        // bumped once, to 2.
        assert_eq!(TraceFormatVersion::CURRENT.version, 2);
    }

    #[test]
    fn trace_format_display() {
        assert_eq!(TraceFormatVersion::new(3).to_string(), "trace-format-v3");
    }

    #[test]
    fn serde_roundtrip() {
        let v = EventSchemaVersion::new(5);
        let json = serde_json::to_string(&v).expect("serialize");
        let back: EventSchemaVersion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn schema_types_are_send_sync() {
        _assert_send_sync::<EventSchemaVersion>();
        _assert_send_sync::<TraceFormatVersion>();
    }
}
