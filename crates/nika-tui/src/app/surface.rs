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

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::commands::Caught;
use super::said::{Said, routed};
use super::{Broker, Shell, Step, Submitted};
use crate::composer::answer::Offered;
use crate::model::Conversation;
use crate::render::own;

/// How long after a surface's activation returns a bare `Enter` still counts
/// as its repeat (a double press), unless another key comes first.
const REPEAT: Duration = Duration::from_millis(500);

/// One activation, one act: what a repeated `Enter` meets after `Enter`
/// activated a surface (an answer bound to its question, a command run).
#[derive(Debug, Default)]
pub(super) struct Guard {
    /// The activation's turn is working: its repeat is taken, and no draft
    /// waiting out of view is queued as a correction by it.
    pub(super) working: bool,
    /// Until when a bare `Enter` is that activation's repeat once its turn
    /// returned.
    until: Option<Instant>,
}

impl Guard {
    /// The activation's turn returned at `now`: a bare `Enter` within
    /// [`REPEAT`] is its repeat.
    fn returned(&mut self, now: Instant) {
        *self = Self {
            working: false,
            until: Some(now + REPEAT),
        };
    }

    /// Whether `key`, read at `now`, repeats the last activation: a bare
    /// `Enter` within its window, taken once. Any key ends the window.
    fn repeats(&mut self, key: &KeyEvent, now: Instant) -> bool {
        let Some(until) = self.until.take() else {
            return false;
        };
        key.code == KeyCode::Enter && key.modifiers == KeyModifiers::NONE && now < until
    }
}

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

    /// `Enter` on a command the palette lists: it runs once, as its line, the
    /// draft untouched; while Nika works it waits in the box instead,
    /// inserted, never sent.
    pub(super) fn run_command(&mut self, words: String) -> Caught {
        if self.state.busy.is_some() {
            self.keep_reading(|_, composer| composer.insert_command(&words));
            self.state.completion = Some(super::stop::command_waits(&words));
            return Caught::Read;
        }
        self.state.interrupt_armed = false;
        self.state.completion = None;
        Caught::Sent(Said::Line(words))
    }

    /// Send what a surface's activation sent, once: a repeated `Enter` is
    /// taken while its turn works and for [`REPEAT`] after it returns.
    pub(super) fn send_activated(
        &mut self,
        said: Said,
        broker: &mut Broker,
    ) -> std::io::Result<Step> {
        self.guard.working = true;
        let step = self.send(said, broker);
        self.guard.returned(Instant::now());
        step
    }

    /// Whether `key` is the repeat of the last activation (a bare `Enter`
    /// within [`REPEAT`] of its return): taken, nothing sent. Any key ends
    /// the window.
    pub(super) fn repeated_enter(&mut self, key: &KeyEvent) -> bool {
        self.guard.repeats(key, Instant::now())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    /// A bare `Enter` within the window after an activation returned is its
    /// repeat, taken once; a second one is a new act. Any other key first, a
    /// modified `Enter`, or the window past, ends it and takes nothing.
    #[test]
    fn a_repeated_enter_counts_once_and_any_other_key_ends_the_window() {
        let enter = key(KeyCode::Enter, KeyModifiers::NONE);
        let now = Instant::now();
        let mut guard = Guard::default();
        assert!(!guard.repeats(&enter, now), "no activation, no repeat");
        guard.returned(now);
        assert!(guard.repeats(&enter, now + Duration::from_millis(120)));
        assert!(
            !guard.repeats(&enter, now + Duration::from_millis(130)),
            "once"
        );
        guard.returned(now);
        assert!(!guard.repeats(&key(KeyCode::Char('x'), KeyModifiers::NONE), now));
        assert!(!guard.repeats(&enter, now), "another key ended the window");
        guard.returned(now);
        assert!(!guard.repeats(&key(KeyCode::Enter, KeyModifiers::ALT), now));
        guard.returned(now);
        assert!(!guard.repeats(&enter, now + REPEAT), "the window is past");
        assert!(!guard.working);
    }

    /// The shell's own keys keep their place under a typed choice: Stop, the
    /// commands, the presentation, a repaint, the function keys and the focus
    /// going back; every other key is the choice's.
    #[test]
    fn the_shell_keeps_its_own_keys_under_a_typed_choice() {
        for (code, modifiers) in [
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
            (KeyCode::Char('o'), KeyModifiers::CONTROL),
            (KeyCode::Char('t'), KeyModifiers::CONTROL),
            (KeyCode::Char('l'), KeyModifiers::CONTROL),
            (KeyCode::F(2), KeyModifiers::NONE),
            (KeyCode::F(6), KeyModifiers::NONE),
            (KeyCode::BackTab, KeyModifiers::SHIFT),
        ] {
            assert!(shell_key(&key(code, modifiers)), "{code:?} {modifiers:?}");
        }
        for (code, modifiers) in [
            (KeyCode::Char('c'), KeyModifiers::NONE),
            (KeyCode::Enter, KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::PageUp, KeyModifiers::NONE),
            (KeyCode::End, KeyModifiers::NONE),
            (KeyCode::Tab, KeyModifiers::NONE),
            (KeyCode::Char('k'), KeyModifiers::CONTROL),
        ] {
            assert!(!shell_key(&key(code, modifiers)), "{code:?} {modifiers:?}");
        }
    }
}
