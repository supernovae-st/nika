// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The PRODUCTION stamper + sinks — the L4 composer's half of the
//! runtime's `Stamper`/`EventSink` seams.
//!
//! The runtime ships [`nika_runtime::DeterministicStamper`] (seq·10ms ·
//! replay-stable · for tests + replay) and [`nika_runtime::VecSink`]
//! (collect). Production stamps are "the composer's concern, L4" (the
//! crate spec §2): real `UUIDv7` identity + real wall time here.

use std::time::{SystemTime, UNIX_EPOCH};

use nika_runtime::Stamper;
use nika_types::id::EventId;
use nika_types::timestamp::Timestamp;

/// Mints real event identities: `UUIDv7` ids (time-ordered · globally
/// unique) + wall-clock timestamps. Unlike the deterministic stamper
/// this is NOT replay-stable — it is the LIVE lane (a real run).
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
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        (id, Timestamp::from_unix_ms(ms))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_unique() {
        let mut stamper = SystemStamper::new();
        let ids: Vec<EventId> = (0..1000).map(|_| stamper.next().0).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable_by_key(|id| id.uuid);
        sorted.dedup_by_key(|id| id.uuid);
        assert_eq!(sorted.len(), ids.len(), "1000 v7 ids · zero collision");
    }

    #[test]
    fn timestamp_is_a_plausible_wall_time() {
        let mut stamper = SystemStamper::new();
        let (_, ts) = stamper.next();
        // After 2020-01-01 and before 2100 — a sanity window, not a
        // clock test (the seam is std SystemTime).
        let ms = ts.unix_ms();
        assert!(ms > 1_577_836_800_000, "after 2020: {ms}");
        assert!(ms < 4_102_444_800_000, "before 2100: {ms}");
    }

    #[test]
    fn ids_are_time_ordered_within_a_run() {
        // v7 ids embed a millisecond timestamp — across a real interval
        // the ids sort in mint order. We don't sleep (hermetic); we just
        // pin that the type IS v7 (version nibble 7).
        let mut stamper = SystemStamper::new();
        let (id, _) = stamper.next();
        assert_eq!(id.uuid.get_version_num(), 7, "ADR-033 · UUIDv7 ids");
    }
}
