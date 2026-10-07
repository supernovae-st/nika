// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The welcome's one-shot clock belongs to the shell, never to the renderer.
//! Only the next distinct frame wakes an idle shell: the last reveal frame is
//! the final mark, so the reveal's end draws nothing new and wakes nothing.
//! Ordinary input uses the same broker and interrupts that wait. Hiding the
//! welcome stops its wakeups without restarting the reveal when it returns.

use std::io;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use super::{Broker, Conversation, Paint, Presentation, Shell, UiEvent, project};
use crate::visual::logomark::{REVEAL_AT, REVEAL_ENDS};
use crate::workspace::focus::Region;
use crate::workspace::geometry::{self, ASIDE_MIN_WIDTH};

#[derive(Debug, Default)]
pub(super) struct Reveal {
    began: Option<Instant>,
    next: Option<Instant>,
}

impl Reveal {
    /// Start on the first visible welcome, retain that origin for the shell's
    /// life, and schedule at most the remaining reveal boundaries (never the
    /// end: the frame held from the last boundary is already the final mark).
    fn paint(&mut self, now: Instant, visible: bool, reduced: bool) -> Duration {
        if visible && self.began.is_none() {
            self.began = Some(now);
        }
        let elapsed = self.began.map_or(REVEAL_ENDS, |start| {
            now.saturating_duration_since(start).min(REVEAL_ENDS)
        });
        self.next = self.began.and_then(|start| {
            (visible && !reduced)
                .then(|| {
                    REVEAL_AT
                        .iter()
                        .copied()
                        .find(|at| *at > elapsed)
                        .map(|at| start + at)
                })
                .flatten()
        });
        elapsed
    }

    fn wait(&self, now: Instant) -> Option<Duration> {
        self.next.map(|at| at.saturating_duration_since(now))
    }
}

impl<C: Conversation + 'static> Shell<C> {
    pub(super) fn welcome_paint(&mut self) -> Paint {
        let visible = self.state.presentation == Presentation::Workspace
            && geometry::fits(self.state.size)
            && (self.state.size.0 >= ASIDE_MIN_WIDTH || self.desk.focus.region != Region::Aside)
            && project::resolve(
                self.desk.view.as_ref(),
                self.desk.opened.as_ref(),
                self.desk.candidate.as_ref(),
                self.desk.live.as_ref(),
            )
            .is_none();
        let elapsed = self
            .welcome
            .paint(Instant::now(), visible, self.options.reduced_motion);
        Paint {
            ascii: self.state.ascii,
            color: self.state.color,
            elapsed,
            reduced_motion: self.options.reduced_motion,
        }
    }

    pub(super) fn next_event(&mut self, broker: &mut Broker) -> io::Result<Option<UiEvent>> {
        loop {
            if let Some(event) = self.deferred.pop_front() {
                return Ok(Some(event));
            }
            let Some(wait) = self.welcome.wait(Instant::now()) else {
                return Ok(broker.recv());
            };
            match broker.recv_timeout(wait) {
                Ok(event) => return Ok(Some(event)),
                Err(RecvTimeoutError::Disconnected) => return Ok(None),
                Err(RecvTimeoutError::Timeout) => self.draw()?,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reveal_boundaries_wake_the_shell_then_it_blocks() {
        let start = Instant::now();
        let mut reveal = Reveal::default();
        assert_eq!(reveal.wait(start), None);
        for (index, at) in REVEAL_AT.into_iter().enumerate() {
            assert_eq!(reveal.paint(start + at, true, false), at);
            let next = REVEAL_AT.get(index + 1).map(|next| *next - at);
            assert_eq!(reveal.wait(start + at), next, "frame {index}");
        }
        // The last frame is the final mark at every size: the end wakes nothing.
        for size in crate::visual::logomark::Size::ALL {
            assert_eq!(
                size.at(REVEAL_ENDS, false),
                size.at(REVEAL_AT[REVEAL_AT.len() - 1], false)
            );
        }
        assert_eq!(reveal.paint(start + REVEAL_ENDS, true, false), REVEAL_ENDS);
        assert_eq!(reveal.wait(start + REVEAL_ENDS), None);
        assert_eq!(
            reveal.paint(start + Duration::from_secs(10), true, false),
            REVEAL_ENDS
        );
        assert_eq!(reveal.wait(start), None);
    }

    #[test]
    fn hidden_welcomes_do_not_wake_or_restart_and_reduced_motion_never_waits() {
        let start = Instant::now();
        let mut reveal = Reveal::default();
        reveal.paint(start, false, false);
        assert_eq!(reveal.wait(start), None);
        assert_eq!(reveal.paint(start, true, false), Duration::ZERO);
        reveal.paint(start + Duration::from_millis(100), false, false);
        assert_eq!(reveal.wait(start), None);
        assert_eq!(reveal.paint(start + REVEAL_ENDS, true, false), REVEAL_ENDS);
        assert_eq!(reveal.wait(start), None);
        let mut reduced = Reveal::default();
        reduced.paint(start, true, true);
        assert_eq!(reduced.wait(start), None);
    }
}
