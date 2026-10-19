// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/meaning` beside a conversation an intelligence leads: what the Session keeps of the
//! person's request, said by [`nika_session_change::work::understood`].

use super::SessionRuntime;

impl SessionRuntime {
    /// What the Session keeps of the request an intelligence leads, `None` when none leads one
    /// or nothing of it is kept yet.
    pub(super) fn understood(&self) -> Option<String> {
        self.agent.as_ref()?;
        nika_session_change::work::understood(&self.work(), self.language)
    }
}
