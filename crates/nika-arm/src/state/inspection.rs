// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One verified read of firing evidence, without claims or projection repair.

use std::io;

use super::{ArmState, Folded, LastRecord, replay};

/// Read-only projections from one verified journal snapshot.
#[non_exhaustive]
pub struct ArmInspection {
    last: Option<LastRecord>,
    folded: Option<Folded>,
    tallies: Option<(usize, usize)>,
}

impl ArmInspection {
    #[must_use]
    pub fn last(&self) -> Option<&LastRecord> {
        self.last.as_ref()
    }

    #[must_use]
    pub fn folded(&self) -> Option<&Folded> {
        self.folded.as_ref()
    }

    #[must_use]
    pub fn tallies(&self) -> Option<(usize, usize)> {
        self.tallies
    }
}

impl ArmState {
    /// Inspect evidence without creating directories, locking, repairing or writing caches.
    /// An absent sidecar yields empty projections; an invalid existing journal refuses.
    /// This is an observation, not a firing lease or a guarantee against a later change.
    ///
    /// # Errors
    /// Existing evidence is inaccessible, redirected, or fails verified replay.
    pub fn inspect(&self, label: &str, now: &jiff::Timestamp) -> io::Result<ArmInspection> {
        let dir = match self.project_dir()?.open_below(&[".nika", "arm", label]) {
            Ok(dir) => dir,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ArmInspection {
                    last: None,
                    folded: None,
                    tallies: None,
                });
            }
            Err(error) => return Err(error),
        };
        let replayed = replay::replay_safe(&dir)?;
        let folded = replay::fold_replay(&replayed, now);
        let tallies = if replayed.journals.is_empty() {
            None
        } else {
            nika_cadence::ledger::tallies(
                replayed
                    .journals
                    .iter()
                    .map(|(text, versioned)| (text.as_str(), *versioned)),
            )
        };
        Ok(ArmInspection {
            last: replayed.last,
            folded,
            tallies,
        })
    }
}
