// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a waiting human answers: the whole-line consent words, the one
//! grammar a fresh spending decision reads (the Session's one-time cost
//! choice and a Run cost decision), the slash commands every waiting state
//! answers from the session's own facts, and the typed « not run » of a
//! declined Run review. None of it grants anything beyond the one decision
//! shown: a changed candidate or a next Run asks afresh.

use super::history::Operation;
use super::{SessionRuntime, TurnOutcome};

pub use nika_session_change::decision::{DecisionAnswer, decision_answer};
pub(super) use nika_session_change::decision::{
    is_gate_token, is_no, is_save_and_run, is_yes, local_command_of,
};

impl SessionRuntime {
    /// A slash command answered from the session's own facts while something
    /// waits (a proposal, a gate, a choice): the waiting state is untouched.
    pub(super) fn answer_locally(&mut self, command: &str) -> TurnOutcome {
        match command {
            "/help" => TurnOutcome::Help(self.help_card()),
            "/status" => TurnOutcome::Facts(self.status()),
            "/details" => TurnOutcome::Facts(self.details()),
            "/meaning" => self.meaning_unrecorded(),
            "/proof" => self.proof_unrecorded(),
            "/restore" => self.restore_while_waiting(),
            _ => self.explain_pending(),
        }
    }

    /// A Run whose fresh cost decision was declined, or interrupted before
    /// any answer: nothing was sent and nothing ran, so there is no exit to
    /// observe — the last run and the status stay as they were. Recorded as
    /// an observation and typed as a fact, never as an exit code.
    pub fn observe_declined_run(&mut self) -> TurnOutcome {
        self.recorded(Operation::Observation, "(run declined)", |s| {
            let line = "not run · the Run cost decision was declined · nothing sent, nothing written · « run it » asks afresh";
            s.remember("(run declined)", line);
            TurnOutcome::Facts(line.to_owned())
        })
    }
}
