// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An opening refusal must survive restoration of the alternate screen.
//! The conversation owns the refusal; the shell only preserves its failed exit.

use std::io;

use crate::model::{Beat, Kind};

/// Only a refusal that closes the opening is fatal. Ordinary refusal cards
/// remain in the conversation, and a deliberate quit remains a normal exit.
pub(super) fn refusal(beats: &[Beat]) -> Option<io::Error> {
    if !beats.iter().any(|beat| matches!(beat, Beat::Quit)) {
        return None;
    }
    beats.iter().find_map(|beat| match beat {
        Beat::Say(block) if block.kind == Kind::Refusal => Some(io::Error::other(format!(
            "session could not open: {}",
            block.text
        ))),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Committed;

    #[test]
    fn only_a_refusal_that_closes_the_opening_is_an_error() {
        let refused = Beat::Say(Committed::new(Kind::Refusal, "history is held elsewhere"));
        assert!(refusal(std::slice::from_ref(&refused)).is_none());
        assert!(refusal(&[Beat::Quit]).is_none());
        assert!(refusal(&[]).is_none());
        assert_eq!(
            refusal(&[refused, Beat::Quit]).map(|failure| failure.to_string()),
            Some("session could not open: history is held elsewhere".to_owned())
        );
    }
}
