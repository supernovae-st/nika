// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event identity + delivery seams — [`Stamper`] mints `(id, timestamp)`
//! pairs · [`EventSink`] receives the stream.
//!
//! The runtime never touches a wall clock or an RNG directly: the
//! composer chooses determinism (tests · replay) or production stamps
//! (wall clock + `UUIDv7`). [`DeterministicStamper`] ships here because
//! replay-stability is part of THIS crate's test contract (the
//! `EventPen` idiom from the L3 rehearsal · seq + 10ms), and
//! [`SystemStamper`] joined it 2026-07-22 (the run-verb descent — the
//! stamper family reads in one home).

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

/// Mints real event identities: `UUIDv7` ids (time-ordered · globally
/// unique) + wall-clock timestamps. Unlike the deterministic stamper
/// this is NOT replay-stable — it is the LIVE lane (a real run).
/// Descended from the run verb 2026-07-22 (the stamper family is one
/// home: the composer picks determinism or production, both read here).
#[derive(Debug, Default)]
pub struct SystemStamper;

impl SystemStamper {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Stamper for SystemStamper {
    fn next(&mut self) -> (EventId, Timestamp) {
        // UUIDv7 is itself time-ordered — two events in the same run sort
        // by id the same way they sort by ts (the journal's natural
        // order). EventId::generate() mints v7 (ADR-033).
        let id = EventId::generate();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        (id, Timestamp::from_unix_ms(ms))
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

    #[test]
    fn system_stamper_every_id_is_unique() {
        let mut stamper = SystemStamper::new();
        let ids: Vec<EventId> = (0..1000).map(|_| stamper.next().0).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable_by_key(|id| id.uuid);
        sorted.dedup_by_key(|id| id.uuid);
        assert_eq!(sorted.len(), ids.len(), "1000 v7 ids · zero collision");
    }

    #[test]
    fn system_stamper_timestamp_is_a_plausible_wall_time() {
        let mut stamper = SystemStamper::new();
        let (_, ts) = stamper.next();
        // After 2020-01-01 and before 2100 — a sanity window, not a
        // clock test (the seam is std SystemTime).
        let ms = ts.unix_ms();
        assert!(ms > 1_577_836_800_000, "after 2020: {ms}");
        assert!(ms < 4_102_444_800_000, "before 2100: {ms}");
    }

    #[test]
    fn system_stamper_ids_are_time_ordered_within_a_run() {
        // v7 ids embed a millisecond timestamp — across a real interval
        // the ids sort in mint order. We don't sleep (hermetic); we just
        // pin that the type IS v7 (version nibble 7).
        let mut stamper = SystemStamper::new();
        let (id, _) = stamper.next();
        assert_eq!(id.uuid.get_version_num(), 7, "ADR-033 · UUIDv7 ids");
    }
}
