// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What an idle shell waits for: the next event, a welcome frame due, or its light
//! clock's ask of the conversation ([`Conversation::idle`]), about once a minute, of what
//! changed without a turn (a schedule's trigger that fired while the door stood open). The
//! ask is a read only, never made while a turn holds the conversation or work is busy; what
//! it answers is said as a turn's beats are, so it shows without a keystroke. Every batch
//! of beats restarts the wait: the conversation has just read the same news.

use std::io;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use super::{Broker, Conversation, Shell, UiEvent};

/// How often an idle shell asks by default ([`super::Options::idle_every`]).
pub(super) const EVERY: Duration = Duration::from_secs(60);

/// The idle shell's light clock: when it next asks the conversation what changed.
#[derive(Debug, Default)]
pub(super) struct Idle {
    due: Option<Instant>,
}

impl Idle {
    /// Wait `every` again from `now`; `None` never asks.
    pub(super) fn rearm(&mut self, now: Instant, every: Option<Duration>) {
        self.due = every.and_then(|every| now.checked_add(every));
    }

    /// How long the shell may wait for input from `now` before it asks.
    fn wait(&self, now: Instant) -> Option<Duration> {
        self.due.map(|at| at.saturating_duration_since(now))
    }

    /// Whether the ask is due at `now`.
    fn due(&self, now: Instant) -> bool {
        self.due.is_some_and(|at| now >= at)
    }
}

impl<C: Conversation + 'static> Shell<C> {
    /// The next event: one typed ahead, or read from the broker, waking meanwhile for a
    /// welcome frame due (drawn) and for the idle ask (what it says, drawn). `None` once
    /// every sender is gone.
    pub(super) fn next_event(&mut self, broker: &mut Broker) -> io::Result<Option<UiEvent>> {
        loop {
            if let Some(event) = self.deferred.pop_front() {
                return Ok(Some(event));
            }
            let now = Instant::now();
            let wait = match (self.welcome.wait(now), self.idle.wait(now)) {
                (Some(frame), Some(ask)) => Some(frame.min(ask)),
                (frame, ask) => frame.or(ask),
            };
            let Some(wait) = wait else {
                return Ok(broker.recv());
            };
            match broker.recv_timeout(wait) {
                Ok(event) => return Ok(Some(event)),
                Err(RecvTimeoutError::Disconnected) => return Ok(None),
                Err(RecvTimeoutError::Timeout) => {
                    let frame = (self.welcome.wait(Instant::now())).is_some_and(|w| w.is_zero());
                    if self.idle_news()? || frame {
                        self.draw()?;
                    }
                }
            }
        }
    }

    /// Ask the conversation what changed while idle, once the ask is due, while the shell
    /// holds it and no work is busy, and say what it answers; `true` when it said anything.
    /// The clock waits again either way.
    fn idle_news(&mut self) -> io::Result<bool> {
        let now = Instant::now();
        if !self.idle.due(now) {
            return Ok(false);
        }
        self.idle.rearm(now, self.options.idle_every);
        if self.state.busy.is_some() {
            return Ok(false);
        }
        let Some(conversation) = self.conversation.as_mut() else {
            return Ok(false);
        };
        let beats = conversation.idle();
        if beats.is_empty() {
            return Ok(false);
        }
        self.apply_all(beats)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock asks `every` after the last batch, never before, and again `every` after
    /// each ask; without a period it never asks.
    #[test]
    fn the_idle_clock_asks_a_period_after_the_last_news() {
        let start = Instant::now();
        let every = Duration::from_secs(60);
        let mut idle = Idle::default();
        assert_eq!(idle.wait(start), None, "nothing armed before the opening");
        assert!(!idle.due(start));
        idle.rearm(start, Some(every));
        assert_eq!(idle.wait(start), Some(every));
        assert!(!idle.due(start + Duration::from_secs(59)));
        assert_eq!(
            idle.wait(start + Duration::from_secs(45)),
            Some(Duration::from_secs(15))
        );
        assert!(idle.due(start + every));
        assert_eq!(
            idle.wait(start + Duration::from_secs(61)),
            Some(Duration::ZERO)
        );
        idle.rearm(start + every, Some(every));
        assert!(!idle.due(start + every + Duration::from_secs(1)));
        assert!(idle.due(start + every * 2));
        idle.rearm(start, None);
        assert_eq!(idle.wait(start + every * 3), None, "never asks");
        assert!(!idle.due(start + every * 3));
    }
}
