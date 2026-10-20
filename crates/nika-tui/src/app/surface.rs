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
//! says so and sends nothing. `Esc` steps back one level and never answers
//! a decision (`crate::composer::answer`); when it has nothing left to undo
//! there, the hint row says so. Every other key is taken and does nothing,
//! except the shell's own (`Ctrl+C`, `Ctrl+O`, `Ctrl+T`, `Ctrl+L`, the
//! function keys, `Shift+Tab`), so Stop, the commands, the reader, the layout
//! and the focus routes to the project and the object stay reachable.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::commands::Caught;
use super::said::{Said, delegated};
use super::{Broker, Shell, Step, Submitted};
use crate::composer::answer::{ESC_ANSWERS_NOTHING, Offered};
use crate::model::{Conversation, Waiting};
use crate::render::own;
use crate::render::surface::covers;

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
    /// New words are a new human act, so their Enter is no longer the
    /// repeated activation that started this turn.
    pub(super) fn edited(&mut self) {
        self.working = false;
        self.until = None;
    }

    /// A turn no `Enter` started returned (`Esc` stepping back through a list): nothing it
    /// sent can be repeated by the `Enter` pressed next.
    fn stepped(&mut self) {
        *self = Self::default();
    }

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

/// The hint while the person reads the conversation behind a card ([`crate::model::UiState`]'s
/// peek).
const PEEKING: &str = "PgUp/PgDn scroll · any other key: back to the card";

/// `Esc` with nothing left to undo on a card that covers the conversation: it never answers,
/// and `PgUp` reads what the card covers.
const ESC_ON_A_COVERING_CARD: &str = "nothing sent · Esc never answers · PgUp: history";

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
        // Behind a covering card, `PgUp` reads the conversation; the card comes back on any
        // key that does not scroll it.
        let page_up = key.code == KeyCode::PageUp && key.modifiers.is_empty();
        if self.state.peek || (page_up && covers(&self.state)) {
            self.state.peek = true;
            self.peek_key(key);
            return Some(Caught::Read);
        }
        match self.composer.offer_key(&self.state.waiting, key) {
            Offered::Moved => return Some(Caught::Read),
            Offered::Answer(text) => {
                return Some(Caught::Sent(self.offered(text)));
            }
            Offered::Delegate => {
                return Some(Caught::Sent(delegated(
                    &self.state.waiting,
                    self.state.voice(),
                )));
            }
            Offered::Notice(words) => {
                let words = if words == ESC_ANSWERS_NOTHING && covers(&self.state) {
                    ESC_ON_A_COVERING_CARD
                } else {
                    words
                };
                self.state.completion = Some(own(words, self.state.ascii));
                return Some(Caught::Read);
            }
            Offered::Pass => {}
        }
        if !self.composer.own_key(key)
            && key.code == KeyCode::Enter
            && key.modifiers == KeyModifiers::NONE
        {
            let approval =
                matches!(&self.state.waiting, Waiting::Action { key, .. } if key == "gate");
            let notice = if approval {
                crate::composer::answer::YES_OR_NO
            } else if crate::composer::answer::choice(&self.state.waiting)
                .is_some_and(|(_, asked, _)| !asked.free_text)
                && !matches!(
                    self.state.waiting,
                    Waiting::QuestionDocument { .. } | Waiting::ProposalChoice { .. }
                )
            {
                "nothing sent · ↑↓ choose, then Enter"
            } else if matches!(self.state.waiting, Waiting::Selection { .. }) {
                "nothing sent · ↑↓ choose a row, or type to filter"
            } else {
                NOTHING_CHOSEN
            };
            self.state.completion = Some(own(notice, self.state.ascii));
        }
        Some(Caught::Read)
    }

    /// A key while the person reads the conversation behind a covering card: the page keys, the
    /// arrows, `Home` and `End` scroll it; any other key brings the card back and does nothing
    /// else, so no key pressed to return answers, selects or types.
    fn peek_key(&mut self, key: KeyEvent) {
        let bare = key.modifiers.is_empty();
        let (state, desk, composer) = (&mut self.state, &self.desk, &self.composer);
        match key.code {
            KeyCode::PageUp if bare => crate::scroll::page(state, desk, composer, true),
            KeyCode::PageDown if bare => crate::scroll::page(state, desk, composer, false),
            KeyCode::Up if bare => crate::scroll::rows(state, desk, composer, true, 1),
            KeyCode::Down if bare => crate::scroll::rows(state, desk, composer, false, 1),
            KeyCode::Home if bare => crate::scroll::rows(state, desk, composer, true, usize::MAX),
            KeyCode::End if bare => state.focus_scroll = 0,
            _ => {
                // The card again, over the conversation's latest rows; the key is spent.
                state.peek = false;
                state.focus_scroll = 0;
                return;
            }
        }
        state.completion = Some(PEEKING.to_owned());
    }

    /// `Enter` on a command the palette lists: it runs once, as its line, the
    /// draft untouched; while Nika works it waits in the box instead,
    /// inserted, never sent.
    pub(super) fn run_command(&mut self, words: String) -> Caught {
        if self.state.busy.is_some() {
            self.keep_reading(|_, composer| composer.insert_command(&words));
            self.state.completion = Some(super::stop::command_waits(&words, self.state.voice()));
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

    /// Send what `Esc` sent stepping back through a list, once, as [`Self::send_activated`]
    /// does, with no repeat window after it: the `Enter` that follows is the person's next
    /// act (once the list above was painted, a quick `Enter` on « Keep current » was taken as
    /// the step back's repeat and nothing closed).
    pub(super) fn send_stepped(
        &mut self,
        said: Said,
        broker: &mut Broker,
    ) -> std::io::Result<Step> {
        self.guard.working = true;
        let step = self.send(said, broker);
        self.guard.stepped();
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

    /// A step back through a list (`Esc`) leaves no window: the `Enter` pressed right after it
    /// is the person's next act, never its repeat.
    #[test]
    fn a_step_back_leaves_no_repeat_window() {
        let enter = key(KeyCode::Enter, KeyModifiers::NONE);
        let now = Instant::now();
        let mut guard = Guard {
            working: true,
            until: None,
        };
        guard.stepped();
        assert!(!guard.working, "its turn returned");
        assert!(
            !guard.repeats(&enter, now + Duration::from_millis(50)),
            "the next Enter is a new act"
        );
    }

    #[test]
    fn editing_after_activation_lets_the_next_enter_queue_new_input_once() {
        let mut guard = Guard {
            working: true,
            until: None,
        };
        assert!(guard.working, "an immediate repeated Enter is held");
        guard.edited();
        assert!(!guard.working, "new words may queue on Enter");
        let now = Instant::now();
        guard.returned(now);
        let enter = key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(
            guard.repeats(&enter, now),
            "the next turn's activation still sends once"
        );
        guard.edited();
        assert!(
            !guard.repeats(&enter, now),
            "fresh words are never swallowed"
        );
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
