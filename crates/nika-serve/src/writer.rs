// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The writer's stamp — who last wrote a resident store (ADR-132 · #1352).
//!
//! A resident's durable stores (jobs · schedules) outlive the binary that
//! wrote them: an `npm update` or a `brew upgrade` leaves a 0.117 resident
//! firing schedules while a 0.118 SDK does manual runs, and nothing said
//! so. Every store now carries the engine that last wrote it and the
//! machine-protocol generation it spoke. Opening a store stamps it with
//! this engine; a store last written by a NEWER protocol refuses to open
//! (fail closed: the newer engine's state is not ours to reinterpret);
//! `nika doctor` reads the stamp beside the resident's lease and says
//! whether the running resident is this binary.

use serde::{Deserialize, Serialize};

/// The engine that last wrote a resident store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterStamp {
    /// The Cargo workspace version of the writer (`0.118.0`).
    pub engine_version: String,
    /// The machine-protocol generation the writer spoke.
    pub machine_protocol_version: u32,
}

impl WriterStamp {
    /// This engine's stamp.
    #[must_use]
    pub fn this_engine() -> Self {
        let identity = nika_runtime::engine_identity();
        Self {
            engine_version: identity.engine_version().to_owned(),
            machine_protocol_version: identity.machine_protocol_version(),
        }
    }

    /// `Some(reason)` when the writer spoke a NEWER protocol than this
    /// engine — the state is not ours to reinterpret (#1352 · fail closed).
    #[must_use]
    pub fn newer_than_this_engine(&self) -> Option<String> {
        let mine = Self::this_engine();
        (self.machine_protocol_version > mine.machine_protocol_version).then(|| {
            format!(
                "the store was last written by engine {} (machine protocol {}) — newer than this resident ({} · protocol {}); this resident refuses to serve it: upgrade the binary, or stop the newer resident and let it own the store",
                self.engine_version,
                self.machine_protocol_version,
                mine.engine_version,
                mine.machine_protocol_version
            )
        })
    }

    /// Whether `self` names another engine than this binary.
    #[must_use]
    pub fn skews_from_this_engine(&self) -> bool {
        *self != Self::this_engine()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_engine_never_skews_from_itself_and_a_newer_protocol_refuses() {
        let mine = WriterStamp::this_engine();
        assert!(!mine.skews_from_this_engine());
        assert!(mine.newer_than_this_engine().is_none());
        let older = WriterStamp {
            engine_version: "0.1.0".to_owned(),
            machine_protocol_version: mine.machine_protocol_version,
        };
        assert!(older.skews_from_this_engine(), "another version skews");
        assert!(
            older.newer_than_this_engine().is_none(),
            "an older writer on the same protocol is served"
        );
        let newer = WriterStamp {
            engine_version: "99.0.0".to_owned(),
            machine_protocol_version: mine.machine_protocol_version + 1,
        };
        let reason = newer
            .newer_than_this_engine()
            .expect("a newer protocol refuses");
        assert!(
            reason.contains("99.0.0") && reason.contains("refuses"),
            "{reason}"
        );
    }
}
