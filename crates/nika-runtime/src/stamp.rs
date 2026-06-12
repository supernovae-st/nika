// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event identity + delivery seams — [`Stamper`] mints `(id, timestamp)`
//! pairs · [`EventSink`] receives the stream.
//!
//! The runtime never touches a wall clock or an RNG directly: the
//! composer chooses determinism (tests · replay) or production stamps
//! (kernel clock + `UUIDv7` · an L4 concern). [`DeterministicStamper`]
//! ships here because replay-stability is part of THIS crate's test
//! contract (the `EventPen` idiom from the L3 rehearsal · seq + 10ms).

use nika_event::Event;
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;
use uuid::Uuid;

/// Mints the identity pair for the next event.
pub trait Stamper {
    /// Next `(id, timestamp)` · monotonic per run.
    fn next(&mut self) -> (EventId, Timestamp);
}

/// Receives each event as it is emitted (display folds · collectors ·
/// future journal writers).
pub trait EventSink {
    /// Deliver one event. Infallible by contract — a sink that can fail
    /// buffers its error internally (the run's verdict never depends on
    /// delivery).
    fn emit(&mut self, event: Event);
}

/// Replay-stable stamper · `seq` becomes the UUID (`from_u128`) · time
/// advances 10ms per event from zero. Same stream in · same bytes out ·
/// forever.
#[derive(Debug, Default)]
pub struct DeterministicStamper {
    seq: u128,
    ms: u64,
}

impl DeterministicStamper {
    /// Fresh stamper at (seq 0 · t0).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Stamper for DeterministicStamper {
    fn next(&mut self) -> (EventId, Timestamp) {
        self.seq += 1;
        self.ms += 10;
        (
            EventId::new(Uuid::from_u128(self.seq)),
            Timestamp::from_unix_ms(self.ms),
        )
    }
}

/// Collecting sink · the test/replay surface.
#[derive(Debug, Default)]
pub struct VecSink {
    events: Vec<Event>,
}

impl VecSink {
    /// Fresh empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The collected stream, in emission order.
    #[must_use]
    pub fn into_events(self) -> Vec<Event> {
        self.events
    }

    /// Borrow the collected stream.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

impl EventSink for VecSink {
    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_stamper_is_replay_stable() {
        let run = || {
            let mut stamper = DeterministicStamper::new();
            (0..3).map(|_| stamper.next()).collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "same stream in · same bytes out");
    }

    #[test]
    fn deterministic_stamper_is_monotonic() {
        let mut stamper = DeterministicStamper::new();
        let (id1, t1) = stamper.next();
        let (id2, t2) = stamper.next();
        assert!(t2 > t1);
        assert_ne!(id1, id2, "every event gets its own identity");
    }

    #[test]
    fn vec_sink_preserves_emission_order() {
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        for kind in [
            nika_event::EventKind::WorkflowStarted,
            nika_event::EventKind::WorkflowCompleted,
        ] {
            let (id, ts) = stamper.next();
            sink.emit(Event::new(id, ts, kind));
        }
        assert_eq!(sink.events().len(), 2, "borrow view sees the pushes");
        let events = sink.into_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, nika_event::EventKind::WorkflowStarted);
        assert_eq!(events[1].kind, nika_event::EventKind::WorkflowCompleted);
    }
}
