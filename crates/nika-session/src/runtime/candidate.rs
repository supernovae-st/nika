// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate under review, borrowed for a host to draw. Reading it audits nothing, reads no
//! file and decides nothing; the next turn may replace it.

use super::SessionRuntime;
use crate::change::ProjectChangeSet;
use crate::outcome::ProposalId;

/// The candidate under review ([`SessionRuntime::candidate`]).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Candidate<'a> {
    /// The identity a consent names ([`SessionRuntime::consent_to`]).
    pub id: ProposalId,
    /// The exact changes a yes lands, their destinations and the Session's audits.
    pub set: &'a ProjectChangeSet,
    /// The words of the rehearsal proof bound to `id`; `None` when no proof binds it.
    pub rehearsed: Option<&'a str>,
    /// Set aside while a revision's question waits: it is not consentable meanwhile.
    pub aside: bool,
}

impl SessionRuntime {
    /// The proposal a consent would answer now, else the one a revision's question set aside
    /// (under the identity it waits again with); `None` when neither waits.
    #[must_use]
    pub fn candidate(&self) -> Option<Candidate<'_>> {
        // An aside set waits again under its draft identity (`keep_revising`).
        let (set, aside, id) = match (&self.pending, &self.revising) {
            (Some(set), _) => (set, false, self.proposal_id(set)),
            (None, Some((set, _))) => (set, true, ProposalId::of(&self.draft_preview(set))),
            (None, None) => return None,
        };
        let rehearsed = self.rehearsals.lines_of(&id);
        Some(Candidate {
            id,
            set,
            rehearsed,
            aside,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
