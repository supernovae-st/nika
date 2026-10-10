// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The keys of the typed choice that holds the line (choice mode,
//! `crate::render::surface`), read before the ordinary precedence so none
//! reaches the transcript hidden under the band, the decision rows it hides
//! or the presentation a bare `Esc` would leave. The arrows, `Home` and `End`
//! select an offer; a character, a correction or a line break writes the own
//! reply's field; `Enter` sends the own reply when it holds words, else the
//! selected offer's key, bound to the question painted, and the offers stay
//! inert until that answer's turn returns. With nothing to send, `Enter`
//! says so and sends nothing. Every other key is taken and does nothing,
//! except the shell's own (`Ctrl+C`, `Ctrl+O`, `Ctrl+T`, `Ctrl+L`, the
//! function keys, `Shift+Tab`), so Stop, the commands, the reader, the layout
//! and the focus routes to the project and the object stay reachable.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::commands::Caught;
use super::said::{Said, routed};
use super::{Broker, Shell, Step, Submitted};
use crate::composer::answer::Offered;
use crate::model::Conversation;
use crate::render::own;

/// The hint when `Enter` finds nothing to send under a typed choice.
const NOTHING_CHOSEN: &str = "nothing sent · ↑↓ choose an answer, or type your own reply";

/// The shell's own keys, which keep their place under a typed choice.
fn shell_key(key: &KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::F(_) | KeyCode::BackTab => true,
        KeyCode::Char(c) => ctrl && matches!(c, 'c' | 'o' | 't' | 'l'),
        _ => false,
    }
}

impl<C: Conversation + 'static> Shell<C> {
    /// A key while the typed choice holds the line, at rest with the keys on
    /// the conversation: `Some` with what was read or sent, `None` for the
    /// shell's own keys and whenever choice mode does not hold.
    pub(super) fn choice_key(&mut self, key: KeyEvent) -> Option<Caught> {
        let holds = self.conversation.is_some()
            && self.state.busy.is_none()
            && self.composer_has_keys()
            && self.composer.choosing(&self.state.waiting);
        if !holds || shell_key(&key) {
            return None;
        }
        // Any key keeps the session, as at the ordinary path.
        self.state.interrupt_armed = false;
        self.state.completion = None;
        match self.composer.offer_key(&self.state.waiting, key) {
            Offered::Moved => return Some(Caught::Read),
            Offered::Answer(text) => {
                return Some(Caught::Sent(routed(&self.state.waiting, text)));
            }
            Offered::Pass => {}
        }
        if !self.composer.own_key(key)
            && key.code == KeyCode::Enter
            && key.modifiers == KeyModifiers::NONE
        {
            self.state.completion = Some(own(NOTHING_CHOSEN, self.state.ascii));
        }
        Some(Caught::Read)
    }

    /// Send `said` through its one door ([`Shell::submit`]) and step on.
    pub(super) fn send(&mut self, said: Said, broker: &mut Broker) -> std::io::Result<Step> {
        match self.submit(said, broker)? {
            Submitted::Left(exit) => Ok(Step::Leave(exit)),
            Submitted::Handoff(Some(handoff)) => Ok(Step::Handoff(handoff)),
            Submitted::Handoff(None) => Ok(Step::Stay),
        }
    }
}
