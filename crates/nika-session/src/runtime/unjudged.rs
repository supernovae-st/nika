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
