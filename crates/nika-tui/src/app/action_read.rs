// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A native read-only act opens the existing reader on that turn's exact reply.
//! The Session still validates and executes its own protocol; this projection
//! never sends a second line, answers the waiting decision or restores a pick.
//!
//! A command (`/meaning`, `/why`, `/show`…) sent beside a waiting decision or
//! question is answered by the Session while the same choice waits again: the
//! band that choice holds hides the conversation, so the words its turn said
//! open in the same reader ([`Command`]), or the hint row says it said none.

use nika_session::ProposalId;

use super::commands::diagnostic::Diagnostic;
use super::said::Said;
use crate::model::{Beat, Committed, Kind, Shape, Turn, Waiting};
use crate::workspace::candidate::Proposed;

pub(super) struct Request {
    key: &'static str,
    witness: String,
    kind: Kind,
    proposal: Option<ProposalId>,
}

impl Request {
    pub(super) fn capture(
        said: &Said,
        waiting: &Waiting,
        candidate: Option<&Proposed>,
    ) -> Option<Self> {
        let (
            Said::Answer { text, witness }
            | Said::Pick {
                keys: text,
                witness,
            },
            Waiting::Action { key, asked },
        ) = (said, waiting)
        else {
            return None;
        };
        let Shape::Choice(offers) = &asked.shape else {
            return None;
        };
        if witness != &asked.witness
            || offers.iter().filter(|offer| &offer.key == text).count() != 1
        {
            return None;
        }
        let (key, kind, proposal) = match (key.as_str(), text.as_str()) {
            ("save", "/show") => (
                "save",
                Kind::Proposal,
                Some(candidate.filter(|shown| !shown.aside())?.id().clone()),
            ),
            ("run_cost", "details") => ("run_cost", Kind::Question, None),
            ("unknown_cost", "details") => ("unknown_cost", Kind::Question, None),
            ("gate", "why?") => ("gate", Kind::Reply, None),
            _ => return None,
        };
        Some(Self {
            key,
            witness: witness.clone(),
            kind,
            proposal,
        })
    }

    /// Only a successful read beside the same waiting decision opens a reader.
    /// Refusals, interruption, a handoff or a changed proposal keep their normal UI.
    pub(super) fn completed(
        &self,
        turn: &Turn,
        candidate: Option<&Proposed>,
    ) -> Option<Diagnostic> {
        if turn.handoff.is_some()
            || turn.beats.iter().any(|beat| {
                matches!(beat, Beat::NotTaken(_) | Beat::Quit | Beat::Cancelled(_))
                    || matches!(beat, Beat::Say(block) if block.kind == Kind::Refusal)
            })
        {
            return None;
        }
        let waiting = turn.beats.iter().rev().find_map(|beat| match beat {
            Beat::Wait(waiting) => Some(waiting),
            _ => None,
        });
        let Some(Waiting::Action { key, asked }) = waiting else {
            return None;
        };
        if key != self.key || asked.witness == self.witness {
            return None;
        }
        if let Some(id) = &self.proposal
            && !candidate.is_some_and(|shown| !shown.aside() && shown.id() == id)
        {
            return None;
        }
        let block = turn.beats.iter().rev().find_map(|beat| match beat {
            Beat::Say(block) if block.kind == self.kind => Some(block),
            _ => None,
        })?;
        // Held previews intentionally lack a proposal tag. A tagged block, if
        // supplied, must agree with the visible candidate checked above.
        if let Some(id) = block.proposal_id()
            && self.proposal.as_ref() != Some(id)
        {
            return None;
        }
        Some(Diagnostic::of(block))
    }
}

/// The choice a band holds, by what waits and its key: the same band comes
/// back when the Session asks the same choice again, under any identity.
#[derive(Debug, PartialEq, Eq)]
enum Band {
    Action(String),
    Question(String),
    Proposal(String, String),
    Knowledge,
}

impl Band {
    fn of(waiting: &Waiting) -> Option<Self> {
        Some(match waiting {
            Waiting::Action { key, .. } => Self::Action(key.clone()),
            Waiting::QuestionDocument { key, .. } => Self::Question(key.clone()),
            Waiting::ProposalChoice { proposal, key, .. } => {
                Self::Proposal(proposal.clone(), key.clone())
            }
            Waiting::Knowledge { .. } => Self::Knowledge,
            _ => return None,
        })
    }
}

/// What a command's turn showed beside the band it was sent from.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Shown {
    /// Its words, for the reader.
    Read(Diagnostic),
    /// It said nothing: the hint row says so.
    Nothing(String),
}

/// A command line (`/…`) sent while a decision or a question holds the band.
pub(super) struct Command {
    words: String,
    band: Band,
}

impl Command {
    pub(super) fn capture(said: &Said, waiting: &Waiting) -> Option<Self> {
        let words = said.text().trim();
        if !words.starts_with('/') {
            return None;
        }
        Some(Self {
            words: words.to_owned(),
            band: Band::of(waiting)?,
        })
    }

    /// The command's answer once its turn returned with the same band: the
    /// words it said, or that it said nothing. A turn that left the band, was
    /// refused or handed off keeps its ordinary path.
    pub(super) fn completed(&self, turn: &Turn) -> Option<Shown> {
        if turn.handoff.is_some()
            || (turn.beats.iter())
                .any(|beat| matches!(beat, Beat::NotTaken(_) | Beat::Quit | Beat::Cancelled(_)))
        {
            return None;
        }
        let waits = turn.beats.iter().rev().find_map(|beat| match beat {
            Beat::Wait(waiting) => Some(waiting),
            _ => None,
        });
        // No new wait: the same choice still holds the band.
        if waits.is_some_and(|waiting| Band::of(waiting).as_ref() != Some(&self.band)) {
            return None;
        }
        let said: Vec<&Committed> = (turn.beats.iter())
            .filter_map(|beat| match beat {
                Beat::Say(block) => Some(block),
                _ => None,
            })
            .collect();
        Some(if said.is_empty() {
            Shown::Nothing(format!("nothing to show for {}", self.words))
        } else {
            Shown::Read(Diagnostic::answer(&self.words, &said))
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
