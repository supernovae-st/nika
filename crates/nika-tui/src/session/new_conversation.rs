// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/new` and `Ctrl+N`: a new conversation in the same project, through the Session's own door
//! ([`nika_session::SessionRuntime::begin_new_conversation`], NIK-197). The Session seals this
//! conversation, keeps it, and says what carries over (the workflows, schedules and runs, a gate
//! still waiting, a charge still unknown); this host then paints the opening again: the banner,
//! the Session's words, the rail and the prompt. While a decision waits, the Session refuses in
//! its own words and nothing changes.

use nika_session::runtime::TurnOutcome;

use super::Live;
use crate::model::{Beat, Committed, Kind, Turn};

/// The line, and the palette's command, that begins a new conversation.
pub(super) const NEW: &str = "/new";

impl Live {
    /// Begin a new conversation: the Session's words and the opening again, or its refusal.
    pub(super) fn new_conversation(&mut self) -> Turn {
        let Some(runtime) = self.runtime.as_mut() else {
            return Turn::new(vec![Beat::Quit], None);
        };
        let said = match runtime.begin_new_conversation() {
            Ok(said) => said,
            Err(refusal) => {
                let (beats, handoff) = self.map(TurnOutcome::Refusal(refusal));
                return Turn::new(beats, handoff);
            }
        };
        let banner = runtime.banner();
        // What this host held of the conversation that ended goes with it; the project's facts
        // (its schedules, the run an earlier session kept) stay.
        self.pending = None;
        self.action = None;
        self.ran_here = false;
        self.picker_guide = None;
        self.fold_candidate();
        let mut beats = vec![
            Beat::Say(Committed::new(Kind::Banner, banner)),
            Beat::Say(Committed::new(Kind::Notice, said)),
        ];
        beats.extend(self.footer());
        beats.push(self.wait());
        Turn::new(beats, None)
    }
}
