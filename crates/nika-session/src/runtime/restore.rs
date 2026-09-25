// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/restore`: how a person finds the proposal kept from the last session. The command
//! proposes the kept draft again through the durable draft's own rebuild, shows the request
//! it answered first, and grants nothing: no model call, no approval, nothing written until
//! a fresh yes. It is advertised (help card, completion, recovery notice) only while a kept
//! draft can be proposed again.

use super::{HELP, Refusal, RefusalClass, SLASH_COMMANDS, SessionRuntime, TurnOutcome, draft};

/// The help line of `/restore`, shown only while a kept draft can be proposed again.
pub(super) const RESTORE_HELP: &str = "/restore            see again the proposal kept from your last session (its request, a fresh preview) · no AI asked · nothing is written until you say yes";

/// The recovery notice's pointer to `/restore`, after the kept draft's own line.
pub(super) const RESTORE_HINT: &str =
    "\n  → type /restore to review it again · no AI asked · nothing is written until you say yes";

/// Where to go when there is nothing to restore.
const NOTHING_KEPT: &str = " · only a proposal still waiting for your yes or no when a session closes is kept · describe what you want instead";

impl SessionRuntime {
    /// The help card, with `/restore` only while a kept draft can be proposed again.
    #[must_use]
    pub fn help_card(&self) -> String {
        match self.restored_draft_id() {
            Some(_) => HELP.replacen("\n/help ", &format!("\n{RESTORE_HELP}\n/help "), 1),
            None => HELP.to_owned(),
        }
    }

    /// The slash commands a door completes now: [`SLASH_COMMANDS`], and `/restore` only
    /// while a kept draft can be proposed again.
    #[must_use]
    pub fn slash_commands(&self) -> Vec<&'static str> {
        let mut commands = SLASH_COMMANDS.to_vec();
        if self.restored_draft_id().is_some() {
            commands.insert(2, "/restore");
        }
        commands
    }

    /// `/restore`: the kept draft proposed again for a fresh review, its original request
    /// shown first. No model is called and nothing is written or approved: the fresh
    /// proposal waits for its own yes, and every refusal says why and what to do.
    pub fn restore_draft(&mut self) -> TurnOutcome {
        let goal = match &self.restored_draft {
            Some(draft::Restored::Usable { draft, .. }) => Some(draft.goal.trim().to_owned()),
            _ => None,
        };
        let nothing_kept = self.restored_draft.is_none();
        match self.repropose_restored_draft() {
            TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
                id,
                preview: format!(
                    "your request from the last session: « {} »\nproposed again from the kept draft, checked against the project now · review it: yes writes it, no discards it\n{preview}",
                    goal.unwrap_or_default()
                ),
            },
            TurnOutcome::Refusal(mut refusal) if nothing_kept => {
                refusal.text.push_str(NOTHING_KEPT);
                TurnOutcome::Refusal(refusal)
            }
            other => other,
        }
    }

    /// `/restore` while something waits (a proposal, a gate, a choice): refused, and what
    /// waits stays exactly as it was. The kept draft stays kept.
    pub(super) fn restore_while_waiting(&self) -> TurnOutcome {
        let text = if self.restored_draft_id().is_some() {
            "something already waits for you; answer or discard it first, then type /restore again · the kept draft stays kept"
        } else {
            "no kept draft this engine can propose again · what waits for you is unchanged"
        };
        TurnOutcome::Refusal(Refusal::new(RefusalClass::WrongState, text))
    }
}
