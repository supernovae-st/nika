// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current proposal, reviewed in the conversation: while the Session
//! waits for consent on a proposal, the latest block that carries a proposal
//! identity reads as the candidate's typed facts when that identity is the
//! candidate's own. Its card then says what a yes answers, every change it
//! lands (where, over which witnessed bytes, and how many changes the faces
//! do not show), what the workflow reaches when it runs, how it was revised
//! and rehearsed, and where the Session's whole words are read.
//!
//! The review is a view: the block keeps its words, `F2` reads them whole,
//! and nothing here consents, saves or runs. Recognition is identity alone,
//! never the block's words: an untagged, held, older or other proposal, a
//! draft, an aside candidate or any other waiting state keeps the Session's
//! words as said.

use nika_display::theme::Role;
use nika_session::ProposalId;
use ratatui::text::Line;

use crate::model::{Kind, UiState, Waiting};
use crate::visual::role;
use crate::workspace::text::twins;

/// Where the card says the Session's whole words are read.
const FULL: &str = "Full proposal, in the Session's words: F2";

/// The candidate a consent can name, as the conversation reviews it: its
/// identity and the facts above every face of it, each with its role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Review {
    id: ProposalId,
    facts: Vec<(String, Role)>,
}

impl Review {
    /// The review of the candidate `id` over its typed `facts`.
    pub(crate) fn new(id: ProposalId, facts: Vec<(String, Role)>) -> Self {
        Self { id, facts }
    }

    /// The lines its card paints: the facts in the glyph column in use, then
    /// where the Session's whole words are read.
    pub(crate) fn lines(&self, color: bool, ascii: bool) -> Vec<Line<'static>> {
        let facts = (self.facts.iter())
            .map(|(words, tone)| Line::styled(twins(words, ascii), role::style(*tone, color)));
        let full = Line::styled(FULL, role::style(Role::Dim, color));
        facts.chain([full]).collect()
    }
}

/// The block of `state`'s transcript the conversation reviews, by index, and
/// its review: the latest block that carries a proposal identity, while the
/// Session waits for consent on a proposal and that identity is the
/// candidate's (`review`). `None` keeps every block's words as said.
pub(crate) fn summarized<'a>(
    state: &UiState,
    review: Option<&'a Review>,
) -> Option<(usize, &'a Review)> {
    let review = review?;
    if !matches!(state.waiting, Waiting::Proposal) {
        return None;
    }
    let (index, block) = (state.transcript.iter().enumerate())
        .rev()
        .find(|(_, block)| block.proposal_id().is_some())?;
    let current = block.kind == Kind::Proposal && block.proposal_id() == Some(&review.id);
    current.then_some((index, review))
}

/// A candidate, its proposal and a conversation waiting for consent on it,
/// shared by the proofs of every module that reads a review.
#[cfg(test)]
pub(crate) mod fixture {
    use nika_session::ProposalId;

    use crate::model::{Committed, Kind, Presentation, UiState, Waiting};
    use crate::workspace::candidate::Proposed;
    use crate::workspace::inspect::Inspected;

    /// The Session's words of the proposal, long enough to fill a panel: the
    /// cost and policy prose only the full reader shows.
    pub(crate) const PREVIEW: &str = "digest-notes · 3 steps in run order\n  1 read      ./notes/brief.md\n  2 draft     one paragraph\n  3 write     ./out/copy.md\nmoney: $0.25 USD · session default · proposal/Run ceiling · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission\nSession inference: no Session budget; qualified priced calls observed at catalog estimates, never capped; catalog estimates are not invoices\nidentity 0bf7aa9d6449 · `/show` the exact bytes · `yes` applies · `no` discards";

    /// The words of the cost prose the review leaves to the full reader.
    pub(crate) const COST: &str = "policy cap unknown";

    /// The human's request, said before the proposal.
    pub(crate) const REQUEST: &str = "nika › copy the brief into out";

    /// What each change and effect of [`candidate`] says.
    pub(crate) const CREATES: &str = "creates compiled-workflow.nika · 33 lines · new";
    pub(crate) const REPLACES: &str =
        "replaces ./notes/plan.md · 4 lines · over the bytes witnessed 0123456789ab";
    pub(crate) const READS: &str = "reads ./notes/brief.md";
    pub(crate) const REACH: &str = "api.example.com, a connected service";

    /// The candidate a consent names under `id`: two changes, one of whose
    /// bytes the faces do not show, what it reads, and a reach that leaves
    /// this machine; `aside` sets it aside.
    pub(crate) fn candidate(id: ProposalId, aside: bool) -> Proposed {
        let witness = "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned();
        let source = "nika: copy\n".to_owned();
        let look = Inspected::unjudged("compiled-workflow.nika", witness, source);
        Proposed::new(id, aside, look)
            .changing(vec![CREATES.to_owned(), REPLACES.to_owned()])
            .reaching(Some(vec![READS.to_owned()]))
            .declaring(vec![(REACH.to_owned(), true)])
            .unshown(1)
    }

    /// The identity of [`PREVIEW`], as the Session derives it.
    pub(crate) fn id() -> ProposalId {
        ProposalId::of(PREVIEW)
    }

    /// A workspace conversation: the human's request, then `proposal`, while
    /// the Session waits as `waiting`.
    pub(crate) fn state(size: (u16, u16), proposal: Committed, waiting: Waiting) -> UiState {
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state.transcript.push(Committed::new(Kind::Human, REQUEST));
        state.transcript.push(proposal);
        state.waiting = waiting;
        state
    }
}
