// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Stopping a preparation, and a correction sent while a turn works.
//!
//! The conversation arms one stop per turn before it moves to its worker
//! ([`Conversation::stopper`]). While the turn runs, the first `Ctrl+C` asks the
//! preparation to stop and arms the second press, which still leaves; `Enter`
//! with words in the box asks the same stop and queues those words as a
//! correction. A command the conversation knows (`/details`, `/status`, …)
//! is not a correction: it reads and never redirects the work, so it stops
//! nothing and waits in the box for the turn's end. A Run is never stopped
//! here: the stop says so and only the usual warning applies. The Session
//! answers the stopped turn itself. Until the turn ends, a requested stop
//! keeps its words on the hint row: an event that empties the row (a reader,
//! a scroll back to the latest row, a pointer, an edit) gives them back, and
//! a hint answering a key stands until the next event.
//!
//! A queued correction is sent as the next line only when the turn ended on
//! the free prompt. Any decision on screen (a proposal, a question, a gate, a
//! choice) is never answered by it: it returns to the box, unsent, with a
//! notice that the answer above came before it. Under a spending question,
//! whose answer must be typed after it shows, it is never put in the box: the
//! notice keeps it whole. A queued correction is always kept whole in the
//! composer's history, where `Up` recalls it, as any line sent with `Enter`.
//! The corrections a chain of turns queues are sent one after another, in a
//! loop ([`chain`]), never by nested calls.

use std::io;

use super::{ENTER_WAITS, Exit, Shell, Submitted};
use nika_display::activity_card::{correction_queued, correction_unsent};

use crate::events::Broker;
use crate::model::{Committed, Conversation, Kind, Stopper, Stopping, Waiting};

/// The hint once a preparation was asked to stop.
const STOPPING: &str = "stopping the preparation · your conversation stays · Ctrl+C again leaves";
/// The hint when the turn drives a Run, which this key never stops.
const RUN_KEEPS: &str = "a Run is under way and is not stopped here · Ctrl+C again leaves";
/// The hint when `Enter` cannot queue a correction because a Run is under way.
const RUN_ENTER_WAITS: &str = "a Run is under way · Enter sends when it is your turn";
/// The row the activity card shows for the human's own request.
const STOP_ASKED: &str = "stop requested by you";

/// The command a draft names by its first word, when the conversation knows
/// it: such a line reads, so it is never queued as a correction.
fn command_typed<'c>(draft: &str, commands: &'c [String]) -> Option<&'c str> {
    let first = draft.split_whitespace().next()?;
    commands
        .iter()
        .map(String::as_str)
        .find(|known| *known == first)
}

/// The hint when `Enter` keeps a command in the box while Nika works.
fn command_waits(command: &str) -> String {
    format!("Nika keeps working · {command} waits for your turn")
}

/// The notice a view change leaves while a turn works: once a stop was
/// requested (`stopping`) and the next `Ctrl+C` leaves (`armed`), the stop's
/// own words, never the first press's hint again; otherwise none.
fn view_notice(stopping: bool, armed: bool) -> Option<&'static str> {
    (stopping && armed).then_some(STOPPING)
}

/// The hint row after an event heard while a turn works: the notice the
/// event `left` stands (a hint answering that key, a completion list); a row
/// it emptied gets the requested stop's own words back ([`view_notice`]).
/// Nothing is cleared and nothing else is invented.
fn kept_notice(left: Option<String>, stopping: bool, armed: bool) -> Option<String> {
    left.or_else(|| view_notice(stopping, armed).map(str::to_owned))
}

/// One turn's stop and the correction queued during it.
#[derive(Default)]
pub(super) struct Hold {
    stopper: Option<Stopper>,
    stopping: bool,
    queued: Option<String>,
    /// The correction the ended turn sends next, taken by [`chain`].
    chained: Option<String>,
}

impl Hold {
    /// A new turn: its own stop, nothing requested, nothing queued.
    pub(super) fn arm(&mut self, stopper: Option<Stopper>) {
        *self = Self {
            stopper,
            ..Self::default()
        };
    }

    /// The preparation of the running turn was asked to stop.
    pub(super) fn stopping(&self) -> bool {
        self.stopping
    }

    /// The turn ended: its stop is spent; the correction it queued, if any.
    pub(super) fn release(&mut self) -> Option<String> {
        std::mem::take(self).queued
    }

    /// Ask the stop once; later asks repeat what the first one found. `None`
    /// when this turn has nothing to stop.
    fn request(&mut self) -> Option<Stopping> {
        if self.stopping {
            return Some(Stopping::Requested);
        }
        let found = (self.stopper.as_ref()?)();
        self.stopping = found == Stopping::Requested;
        Some(found)
    }
}

/// Where a correction queued during a turn goes once the turn has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Sent as the next line: nothing waits for a decision.
    Send,
    /// Back into the box, unsent: a decision is on screen.
    Draft,
    /// Only in the transcript: a spending question takes an answer typed
    /// after it shows, so even the box stays empty.
    Transcript,
}

/// Submit `first`, then each correction a turn queued, one after another: a
/// loop that keeps one line at a time, never a nested call, however long the
/// chain. `one` submits a line and gives the correction its turn queued.
fn chain<T>(
    first: &str,
    mut one: impl FnMut(&str) -> io::Result<(T, Option<String>)>,
) -> io::Result<T> {
    let (mut last, mut next) = one(first)?;
    while let Some(line) = next {
        (last, next) = one(&line)?;
    }
    Ok(last)
}

/// The fate of a queued correction for the turn's end: what waits, whether it
/// is a fresh spending question, whether the turn handed work off, and whether
/// the shell is closing.
fn fate(waiting: &Waiting, fresh: bool, handoff: bool, quit: bool) -> Fate {
    if fresh {
        Fate::Transcript
    } else if *waiting == Waiting::Free && !handoff && !quit {
        Fate::Send
    } else {
        Fate::Draft
    }
}

impl<C: Conversation + 'static> Shell<C> {
    /// The human sent `line`: it, then each correction its turn queued, in
    /// order ([`chain`]).
    pub(super) fn submit(&mut self, line: &str, broker: &mut Broker) -> io::Result<Submitted> {
        chain(line, |line| {
            let submitted = self.submit_one(line, broker)?;
            Ok((submitted, self.hold.chained.take()))
        })
    }

    /// The hint row once an event heard while a turn works was handled
    /// ([`kept_notice`]): a pending stop's confirmation survives whatever
    /// emptied the row (a palette, the diagnostic, a scroll back to the
    /// latest row, a pointer, an edit of the draft). `armed` is whether the
    /// next `Ctrl+C` leaves; it is read, never changed.
    pub(super) fn keep_stop_notice(&mut self, armed: bool) {
        let left = self.state.completion.take();
        self.state.completion = kept_notice(left, self.hold.stopping(), armed);
    }

    /// `Ctrl+C` while a turn works. The first press stops a preparation that
    /// can be stopped and arms the next, which leaves; on a Run, or a turn
    /// with nothing to stop, the press only arms, as before.
    pub(super) fn stop_or_arm(&mut self, armed: &mut bool) -> Option<Exit> {
        if !*armed {
            let was = self.hold.stopping();
            match self.hold.request() {
                Some(Stopping::Requested) => {
                    if !was {
                        self.note_stop();
                    }
                    self.state.completion = Some(STOPPING.to_owned());
                    *armed = true;
                    return None;
                }
                Some(Stopping::RunUnderway) => {
                    self.state.completion = Some(RUN_KEEPS.to_owned());
                }
                None => {}
            }
        }
        Self::arm(armed)
    }

    /// `Enter` while a turn works. With words in the box during a preparation
    /// that can be stopped, the words are queued as a correction and the
    /// preparation is asked to stop; otherwise the box keeps them and the hint
    /// says when `Enter` sends.
    pub(super) fn queue_correction(&mut self) {
        if self.composer.is_blank() || self.hold.stopper.is_none() {
            self.state.completion = Some(ENTER_WAITS.to_owned());
            return;
        }
        if let Some(command) = command_typed(&self.composer.text(), &self.commands) {
            self.state.completion = Some(command_waits(command));
            return;
        }
        let was = self.hold.stopping();
        if self.hold.request() != Some(Stopping::Requested) {
            self.state.completion = Some(RUN_ENTER_WAITS.to_owned());
            return;
        }
        if !was {
            self.note_stop();
        }
        // Taken as `Enter` takes a line: whole, and kept in history.
        let draft = self.composer.take();
        let replaced = self.hold.queued.replace(draft.clone()).is_some();
        let notice = correction_queued(&draft, replaced, self.state.ascii);
        crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
            state.transcript.push(Committed::new(Kind::Notice, notice));
        });
        self.state.completion = None;
    }

    /// The human's stop, as a row of the live activity card (the workspace's).
    fn note_stop(&mut self) {
        crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
            state.observe_activity(STOP_ASKED);
        });
    }

    /// The turn ended with `queued`: chained to be sent next ([`chain`]), or
    /// back in the box, or kept whole in the transcript ([`fate`]).
    pub(super) fn next_correction(
        &mut self,
        queued: Option<String>,
        fresh: bool,
        handoff: bool,
    ) -> io::Result<()> {
        let Some(queued) = queued else {
            return Ok(());
        };
        let fate = fate(&self.state.waiting, fresh, handoff, self.state.quit);
        if fate == Fate::Send {
            self.hold.chained = Some(queued);
            return Ok(());
        }
        let notice = correction_unsent(&queued, fate == Fate::Transcript, self.state.ascii);
        // A scrolled transcript keeps its reading position: the draft's rows
        // and the notice land below it.
        self.keep_reading(|state, composer| {
            if fate == Fate::Draft {
                // The correction first, then whatever was typed after it.
                let rest = composer.text();
                composer.clear();
                composer.paste(&queued);
                if !rest.trim().is_empty() {
                    composer.paste("\n");
                    composer.paste(&rest);
                }
            }
            state.transcript.push(Committed::new(Kind::Notice, notice));
        });
        self.commit_inline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counting(found: Stopping) -> (Stopper, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let stopper: Stopper = Box::new(move || {
            seen.fetch_add(1, Ordering::Relaxed);
            found
        });
        (stopper, calls)
    }

    /// A stop is asked of the conversation once per turn; later asks repeat its
    /// answer, and the turn's end spends it with whatever was queued.
    #[test]
    fn a_turn_asks_its_stop_once_and_its_end_spends_it() {
        let mut hold = Hold::default();
        assert_eq!(hold.request(), None, "no stop armed: nothing to ask");
        let (stopper, calls) = counting(Stopping::Requested);
        hold.arm(Some(stopper));
        assert!(!hold.stopping());
        assert_eq!(hold.request(), Some(Stopping::Requested));
        assert_eq!(hold.request(), Some(Stopping::Requested));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(hold.stopping());
        hold.queued = Some("use the CSV instead".to_owned());
        assert_eq!(hold.release().as_deref(), Some("use the CSV instead"));
        assert!(!hold.stopping());
        assert_eq!(hold.request(), None, "a spent stop is gone");
        let (stopper, _) = counting(Stopping::Requested);
        hold.arm(Some(stopper));
        assert_eq!(hold.release(), None, "a new turn queues nothing");
    }

    /// A Run is never marked as stopping: each press asks again and is told so.
    #[test]
    fn a_run_under_way_is_never_marked_as_stopping() {
        let mut hold = Hold::default();
        let (stopper, calls) = counting(Stopping::RunUnderway);
        hold.arm(Some(stopper));
        assert_eq!(hold.request(), Some(Stopping::RunUnderway));
        assert_eq!(hold.request(), Some(Stopping::RunUnderway));
        assert!(!hold.stopping());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    /// A correction is sent only at a free prompt with nothing handed off; any
    /// decision keeps it in the box; a spending question keeps it out of the box.
    #[test]
    fn a_queued_correction_never_answers_a_decision() {
        assert_eq!(fate(&Waiting::Free, false, false, false), Fate::Send);
        assert_eq!(fate(&Waiting::Free, false, true, false), Fate::Draft);
        assert_eq!(fate(&Waiting::Free, false, false, true), Fate::Draft);
        for waiting in [
            Waiting::Proposal,
            Waiting::Gate,
            Waiting::Choosing,
            Waiting::Question {
                key: "const.source_path".to_owned(),
            },
        ] {
            assert_eq!(
                fate(&waiting, false, false, false),
                Fate::Draft,
                "{waiting:?}"
            );
        }
        for waiting in [
            Waiting::Free,
            Waiting::Question {
                key: "unknown_cost".to_owned(),
            },
        ] {
            assert_eq!(fate(&waiting, true, false, false), Fate::Transcript);
        }
    }

    /// A command typed while a preparation works is never a correction: it reads, so it stops
    /// nothing and waits. Words that only mention a command, or a path that starts with a
    /// slash, still correct the work.
    #[test]
    fn a_command_typed_during_a_preparation_is_never_a_correction() {
        let commands = ["/details", "/status", "/show"].map(str::to_owned);
        assert_eq!(command_typed("/details", &commands), Some("/details"));
        assert_eq!(command_typed("  /status  ", &commands), Some("/status"));
        assert_eq!(command_typed("/show now", &commands), Some("/show"));
        for correction in [
            "write it to /tmp/out.md instead",
            "/tmp/out.md is the output",
            "show /details of the plan",
            "/detailsx",
        ] {
            assert_eq!(command_typed(correction, &commands), None, "{correction}");
        }
        let hint = command_waits("/intelligence");
        assert!(hint.chars().count() <= 80, "{hint}");
    }

    /// After a stop request whose next press leaves, a view change keeps the
    /// stop's own words, which say so; before any request, or while a press
    /// would only arm, it leaves no notice of its own.
    #[test]
    fn a_view_change_keeps_a_requested_stop_on_the_hint_row() {
        assert_eq!(view_notice(true, true), Some(STOPPING));
        assert!(STOPPING.contains("Ctrl+C again leaves"), "{STOPPING}");
        assert!(!STOPPING.contains("requests Stop"), "{STOPPING}");
        for (stopping, armed) in [(false, false), (false, true), (true, false)] {
            assert_eq!(
                view_notice(stopping, armed),
                None,
                "stopping={stopping} armed={armed}"
            );
        }
    }

    /// After any event heard while a turn works, a row the event emptied gets a
    /// requested stop's words back, a hint the event set stands, and nothing is
    /// cleared or invented: a correction queued without `Ctrl+C` (stopping, not
    /// armed) claims no leaving press, and a Run's press restores nothing.
    #[test]
    fn an_emptied_hint_row_gets_a_pending_stop_back_and_a_set_hint_stands() {
        assert_eq!(kept_notice(None, true, true).as_deref(), Some(STOPPING));
        for set in [ENTER_WAITS, RUN_ENTER_WAITS, "/details  /status"] {
            for (stopping, armed) in [(true, true), (true, false), (false, true), (false, false)] {
                assert_eq!(
                    kept_notice(Some(set.to_owned()), stopping, armed).as_deref(),
                    Some(set),
                    "stopping={stopping} armed={armed}"
                );
            }
        }
        for (stopping, armed) in [(true, false), (false, true), (false, false)] {
            assert_eq!(
                kept_notice(None, stopping, armed),
                None,
                "stopping={stopping} armed={armed}"
            );
        }
    }

    /// The stop's hints fit one 80-column row and never claim a Run stops.
    #[test]
    fn the_stop_hints_fit_a_row_and_never_claim_a_run_stops() {
        for hint in [STOPPING, RUN_KEEPS, RUN_ENTER_WAITS] {
            assert!(hint.chars().count() <= 80, "{hint}");
            assert!(!hint.contains("Run is stopped"), "{hint}");
        }
    }

    /// A long chain of corrections is sent one line at a time, in order, by a loop: no call
    /// waits on the next one. The first failure ends the chain and is returned.
    #[test]
    #[allow(clippy::expect_used)]
    fn a_long_correction_chain_runs_in_a_loop() {
        const LONG: usize = 200_000;
        let mut sent = 0_usize;
        let last = chain("first", |line| {
            sent += 1;
            let expected = if sent == 1 {
                "first".to_owned()
            } else {
                format!("correction {}", sent - 1)
            };
            assert_eq!(line, expected);
            Ok((sent, (sent < LONG).then(|| format!("correction {sent}"))))
        })
        .expect("every line sent");
        assert_eq!((last, sent), (LONG, LONG));
        let mut tried = 0;
        let failed = chain("first", |_| {
            tried += 1;
            if tried == 3 {
                Err(io::Error::other("the terminal closed"))
            } else {
                Ok(((), Some("again".to_owned())))
            }
        });
        assert!(failed.is_err());
        assert_eq!(tried, 3, "nothing is sent after a failure");
    }
}
