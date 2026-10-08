// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal kept across a close, and its deterministic re-proposal. The kept draft is
//! evidence, never authority: restoring it restores no consent, budget or run. Proposing it
//! again rebuilds a fresh proposal from its exact bytes through the same change primitive a
//! compiled candidate uses, checked against the project as it is now, and that proposal
//! needs a fresh review and a fresh consent. No model is called.
//!
//! Record (`Saved.pending`, JSON, schema 1): `{"schema": 1, "proposal", "goal", "files":
//! [{"path", "kind": "create_workflow" | "update_workflow" | "other", "before", "witness",
//! "text"}]}`. A value this engine cannot read (another schema, a malformed value, no schema)
//! is kept byte for byte, rides every later record unchanged and is never used.
//!
//! The record and its rebuild are [`nika_session_change::draft`]; the re-proposal act is
//! the Session's, here.

#[cfg(test)]
pub(super) use nika_session_change::draft::{DraftKind, PendingDraft};
pub(super) use nika_session_change::draft::{Restored, capture, rebuild, restored_line};

use super::history::Operation;
use super::{Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::outcome::ProposalId;

/// How the re-proposal act appears in the conversation record.
const ACT: &str = "(propose the kept draft again)";

impl SessionRuntime {
    /// The identity of the kept draft when it is available now: this engine reads it, its
    /// kept text is its proposed bytes, and the project still holds the base it was proposed
    /// over. Re-checked read-only on every call with the checks
    /// [`Self::repropose_restored_draft`] makes (nothing is written); `None` otherwise, while
    /// the draft itself stays kept and the restore notice says why. Available means the same
    /// bytes over the same base, not proof that they still mean what the request meant.
    #[must_use]
    pub fn restored_draft_id(&self) -> Option<&str> {
        let draft = self.restored_draft.as_ref()?.read().ok()?;
        rebuild(&self.snapshot.root, draft)
            .is_ok()
            .then_some(draft.proposal.as_str())
    }

    /// Propose the kept draft again, deterministically and without a model call. Its exact
    /// bytes are rebuilt through the change primitive a compiled candidate uses (a contained,
    /// canonical workflow path; the `nika check` audit; the destination witnessed now), and
    /// refused when the kept text differs from the proposed bytes (redacted, altered or
    /// absent in an old record) or the project changed under it. The result is a fresh proposal: it needs a
    /// fresh review and a fresh consent, and nothing of the earlier consent, budget or run
    /// is restored. Recorded like a turn.
    pub fn repropose_restored_draft(&mut self) -> TurnOutcome {
        self.recorded(Operation::Turn, ACT, Self::repropose_unrecorded)
    }

    fn repropose_unrecorded(&mut self) -> TurnOutcome {
        let kept = self.restored_draft.as_ref().map(Restored::read);
        let draft = match self.kept_to_use(kept, "draft", "propose again") {
            Ok(draft) => draft.clone(),
            Err(refusal) => return refusal,
        };
        match rebuild(&self.snapshot.root, &draft) {
            Ok(set) => {
                let preview = self.draft_preview(&set);
                let id = ProposalId::of(&preview);
                self.remember(
                    ACT,
                    &format!("(proposed {id} again from kept draft {})", draft.proposal),
                );
                self.bind_proposal_money(&id);
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            Err(why) => refused(
                RefusalClass::NotAllowed,
                format!(
                    "the kept draft {} cannot be proposed again: {why} · it stays kept; state the request again instead",
                    draft.proposal
                ),
            ),
        }
    }

    /// The kept draft or round (`what`) a restore act uses now, or the act's refusal while
    /// something else waits, when this engine cannot read the kept value, or when none is kept
    /// (`act`: what the act does): the kept value stays kept either way.
    pub(super) fn kept_to_use<'k, T>(
        &self,
        kept: Option<Result<&'k T, &'k str>>,
        what: &str,
        act: &str,
    ) -> Result<&'k T, TurnOutcome> {
        let waits = self.pending.is_some()
            || self.pending_gate.is_some()
            || self.authoring.is_some()
            || self.waiting_cost_choice();
        let (class, text) = match kept {
            _ if waits => (
                RefusalClass::WrongState,
                format!(
                    "something already waits for you; answer or discard it first · the kept {what} stays kept"
                ),
            ),
            Some(Ok(value)) => return Ok(value),
            Some(Err(why)) => (
                RefusalClass::NotAllowed,
                format!(
                    "the kept {what} cannot be read by this engine ({why}); it stays kept unchanged"
                ),
            ),
            None => (RefusalClass::WrongState, format!("no kept {what} to {act}")),
        };
        Err(refused(class, text))
    }
}

pub(super) fn refused(class: RefusalClass, text: String) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(class, text))
}
