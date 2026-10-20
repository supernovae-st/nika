// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The proposal the person discarded: nothing of it was written, and a later change starts from
//! what is saved, never from the discarded draft. Live gauntlet r08, 2026-10-16: after « Discard »
//! of an unrequested rewrite that renamed the workflow, the next fix edited the discarded draft,
//! which came back as a second workflow beside the saved one.

use nika_session_change::work::Binding;

use super::{Candidate, Conversation};

/// What a discard leaves for the author's next turn.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Discarded {
    /// No proposal was shown: there was nothing to discard.
    Nothing,
    /// The candidate is the saved workflow's bytes again, as revision `revision`.
    Saved {
        /// The revision the discarded proposal showed.
        discarded: u64,
        /// The revision that holds the saved bytes now.
        revision: u64,
        /// The saved workflow, project-relative.
        path: String,
    },
    /// Nothing is saved: no candidate stands any more.
    Unsaved {
        /// The revision the discarded proposal showed.
        discarded: u64,
    },
}

impl Discarded {
    /// What the author is told with the person's next line, `None` when nothing was discarded.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        match self {
            Self::Nothing => None,
            Self::Saved {
                discarded,
                revision,
                path,
            } => Some(format!(
                "The person discarded the proposal of revision {discarded}: nothing of it was written. The candidate is again the saved workflow `{path}` as it stands (revision {revision}): a change the person asks revises it there, under its own name, and nothing of the discarded revision comes back unless they ask for it."
            )),
            Self::Unsaved { discarded } => Some(format!(
                "The person discarded the proposal of revision {discarded}: nothing of it was written and no candidate stands now. Write one only for what the person asks next, and nothing of the discarded revision unless they ask for it."
            )),
        }
    }
}

impl Conversation {
    /// The person discarded the proposal shown. When this conversation saved a workflow, its
    /// bytes as they stand (`saved`) are the candidate again, as a next revision with the values
    /// they still carry, so a later change revises the saved workflow; else no candidate stands.
    /// Either way no proposal is shown any more, and none of its choices waits.
    pub fn discarded(&mut self, saved: Option<String>) -> Discarded {
        let Some(shown) = self.shown.take() else {
            return Discarded::Nothing;
        };
        self.open.clear();
        self.picked_other.clear();
        self.changed.clear();
        self.unchanged = false;
        let last = self.candidate.as_ref().map_or(shown.number, |c| c.number);
        let path = self.saved.as_ref().map(|(path, _)| path.clone());
        let (Some(source), Some(path)) = (saved, path) else {
            self.discarded = last;
            self.candidate = None;
            return Discarded::Unsaved {
                discarded: shown.number,
            };
        };
        let (bindings, accepted) = {
            let kept = |list: &[Binding]| -> Vec<Binding> {
                (list.iter())
                    .filter(|b| self.carries(&source, &b.value))
                    .cloned()
                    .collect()
            };
            (kept(&self.bindings), kept(&self.accepted))
        };
        self.bindings = bindings;
        self.accepted = accepted;
        self.seen.clone_from(&self.accepted);
        let revision = last + 1;
        self.candidate = Some(Candidate {
            number: revision,
            source,
            summary: "the saved workflow, as it stands".to_owned(),
            rows: Vec::new(),
        });
        Discarded::Saved {
            discarded: shown.number,
            revision,
            path,
        }
    }
}
