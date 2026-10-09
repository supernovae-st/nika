// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current proposal, reviewed in the conversation: while the Session
//! waits for consent on a proposal, the latest block that carries a proposal
//! identity reads as the candidate's typed facts when that identity is the
//! candidate's own. Its card then holds what changes the decision: every
//! change it lands (where, over which witnessed bytes, and how many changes
//! the faces do not show), how it was revised, the run a `save & run` asks
//! when the request itself carried one, what the workflow reaches when it
//! runs and where, and its rehearsal. The identity a consent names and the
//! key that reads the Session's whole words ride its bottom border.
//!
//! The card's model ([`Review`]) lives with the candidate in
//! [`nika_tui_view::workspace::candidate`] (ADR-143); which block it reviews
//! is read here, from the conversation's state.
//!
//! The review is a view: the block keeps its words, `F2` reads them whole,
//! and nothing here consents, saves or runs. Recognition is identity alone,
//! never the block's words: an untagged, held, older or other proposal, a
//! draft, an aside candidate or any other waiting state keeps the Session's
//! words as said.

use crate::model::{Kind, UiState, Waiting};
pub(crate) use nika_tui_view::workspace::candidate::{OR, Review};

/// The block of `state`'s transcript the Session waits on for consent, by
/// index: the latest block that carries a proposal identity, while a
/// proposal waits. Every other proposal block is history.
pub(crate) fn pending(state: &UiState) -> Option<usize> {
    if !matches!(state.waiting, Waiting::Proposal) {
        return None;
    }
    let (index, block) = (state.transcript.iter().enumerate())
        .rev()
        .find(|(_, block)| block.proposal_id().is_some())?;
    (block.kind == Kind::Proposal).then_some(index)
}

/// The block of `state`'s transcript the conversation reviews, by index, and
/// its review: the pending proposal ([`pending`]) while its identity is the
/// candidate's (`review`). `None` keeps every block's words as said.
pub(crate) fn summarized<'a>(
    state: &UiState,
    review: Option<&'a Review>,
) -> Option<(usize, &'a Review)> {
    let review = review?;
    let index = pending(state)?;
    let current = state.transcript.get(index)?.proposal_id() == Some(review.id());
    current.then_some((index, review))
}

/// The review the live area reads for the standing, the rail and the
/// decision row: the current proposal's ([`summarized`]), else none.
pub(crate) fn consent<'a>(state: &UiState, review: Option<&'a Review>) -> Option<&'a Review> {
    summarized(state, review).map(|(_, review)| review)
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

    /// What each change and effect of [`candidate`] says, as the Live host
    /// adapter words a change (a count bound to its unit).
    pub(crate) const CREATES: &str = "creates compiled-workflow.nika (33\u{a0}lines)";
    pub(crate) const REPLACES: &str =
        "replaces ./notes/plan.md (4\u{a0}lines) · over the bytes witnessed 0123456789ab";
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
