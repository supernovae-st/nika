// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null event sink — discards all events.

use nika_error::NikaError;
use nika_kernel::event_sink::{Event, EventSink};

/// No-op event sink that discards all events.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullEventSink;

impl NullEventSink {
    /// Create a new null event sink.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

// Sealing: NullEventSink lives in nika-kernel-mock (workspace-controlled),
// so it is allowed to participate in the sealed Provider/EventSink lattice.
impl nika_kernel::sealed::Sealed for NullEventSink {}

impl EventSink for NullEventSink {
    async fn emit(&self, _event: Event) -> Result<(), NikaError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_error::id::{EventId, RunId};

    #[tokio::test]
    async fn emit_succeeds() {
        let sink = NullEventSink::new();
        let event = Event::new(
            EventId::nil(),
            RunId::nil(),
            "test",
            0,
            serde_json::Value::Null,
        );
        sink.emit(event).await.unwrap();
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_event_sink_is_send_sync() {
        _assert_send_sync::<NullEventSink>();
    }
}
