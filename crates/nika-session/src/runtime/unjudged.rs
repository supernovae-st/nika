// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A candidate kept only for its judge: no proposal, invented question or execution authority.
use super::{SessionRuntime, TurnOutcome};
use crate::authoring::{AuthoringRound, is_cancel};
pub(super) use nika_cli_host::display::front_door::recovery::{
    JUDGMENT_KEPT as KEPT, JUDGMENT_STATUS as STATUS,
};
use nika_onboard::compile::CompileOutcome;
impl SessionRuntime {
    /// The exact context of an unjudged continuation that currently owns a free line.
    /// Hosts may offer retry or discard beside a free correction, then compare this
    /// identity again before submitting the line through the existing Session door.
    /// Reading it performs no work and grants no judgment, Save or Run authority.
    /// It is a presentation identity, not a durable token or a substitute for a
    /// host's fresh presentation nonce after reopening.
    #[must_use]
    pub fn waiting_judgment(&self) -> Option<String> {
        if self.waiting() != crate::work::Waiting::Free || !self.judgment_waits() {
            return None;
        }
        let round = self.authoring.as_ref()?;
        // As for question identities, bind all exact round/context facts rather
        // than inferring a resumable state from an Incomplete status or its prose.
        let context = format!(
            "nika-session waiting-judgment 1\n{round:?}\n{:?}\n{:?}\n{:?}",
            self.seat, self.intelligence, self.authoring_context
        );
        Some(crate::change::Witness::of(context.as_bytes()).0)
    }

    pub(super) fn judgment_waits(&self) -> bool {
        self.authoring
            .as_ref()
            .is_some_and(|r| r.current().is_none() && r.continuation.is_some())
    }
    pub(super) fn keep_unjudged(
        &mut self,
        mut round: AuthoringRound,
        out: CompileOutcome,
    ) -> TurnOutcome {
        round.absorb(&out);
        self.last_outcome = Some(out);
        self.authoring = Some(round);
        TurnOutcome::Facts(KEPT.into())
    }
    pub(super) fn continue_judgment(&mut self, input: &str) -> TurnOutcome {
        let Some(round) = self.authoring.take() else {
            return TurnOutcome::Facts(KEPT.into());
        };
        if is_cancel(input) {
            return self.keep_revising(TurnOutcome::Facts(
                "The unjudged candidate was discarded; your request is kept.".into(),
            ));
        }
        if input.trim().is_empty()
            || super::authoring::is_run_verb(input)
            || super::is_yes(input)
            || input.trim().eq_ignore_ascii_case("save")
        {
            self.authoring = Some(round);
            return TurnOutcome::Facts(KEPT.into());
        }
        if matches!(
            input.trim().to_lowercase().as_str(),
            "continue" | "retry" | "reprends" | "reprend" | "réessaie"
        ) || input.trim() == round.intent.trim()
        {
            if let Err(refused) = self.admit_money(&round.intent, false, true) {
                self.authoring = Some(round);
                return refused;
            }
            let out = self.compile_again(round);
            return self.keep_revising(out);
        }
        // A correction discards the candidate, while EDIT still owns its original base.
        if let Some((_, change, _)) = &round.edit {
            let change = format!("{change}. {input}");
            if let Some((set, previous)) = self.revising.take() {
                self.last_outcome = previous;
                return self.revise_pending(set, &change);
            }
            if let Some((path, _)) = &round.target {
                return self.revise_saved(path, &change);
            }
        }
        self.restate_round(&round, input)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
