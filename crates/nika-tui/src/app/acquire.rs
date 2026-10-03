// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the run in view's face needs (a file it wrote, read now; its journal,
//! captured and verified) acquired on a worker thread, as a turn is computed:
//! the shell keeps hearing keys and drawing meanwhile (the composer takes
//! words, a bare Enter waits, the page keys scroll, `Ctrl+C` twice leaves).
//! The result is applied only to the leg and the reading it was asked for: a
//! leg read again (`r`) since, or another leg, leaves it unapplied. Nothing
//! here runs, answers or calls a model.

use std::io;
use std::sync::mpsc;

use super::{BUSY_POLL, Broker, Exit, Heard, Shell, busy_text_with, spinner_frame};
use crate::model::Conversation;
use crate::workspace::desk::acquire_all;

/// The busy row while the run's face is acquired.
const ACQUIRING: &str = "● reading what the run left";

impl<C: Conversation + 'static> Shell<C> {
    /// Acquire what the run in view's face still needs, when it needs
    /// something and the conversation is here; `Some(exit)` when the human
    /// left meanwhile.
    pub(super) fn acquire_wanted(&mut self, broker: &mut Broker) -> io::Result<Option<Exit>> {
        let Some((execution, wants, generation)) = self.desk.wanted() else {
            return Ok(None);
        };
        let Some(mut conversation) = self.conversation.take() else {
            return Ok(None);
        };
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("nika-tui-acquire".to_owned())
            .spawn(move || {
                let got = acquire_all(&mut conversation, &execution, wants);
                let _ = done_tx.send((conversation, got));
            })?;
        let started = std::time::Instant::now();
        let mut armed = false;
        let mut shown = u64::MAX;
        loop {
            match done_rx.recv_timeout(BUSY_POLL) {
                Ok((conversation, got)) => {
                    self.conversation = Some(conversation);
                    self.desk.acquired(execution, generation, got);
                    self.state.busy = None;
                    self.typed_live = false;
                    if self.state.completion.as_deref() == Some(super::ENTER_WAITS) {
                        self.state.completion = None;
                    }
                    // The main loop waits for the next event: what was read
                    // is drawn now, not when a key comes.
                    self.draw()?;
                    return Ok(None);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return match worker.join() {
                        Ok(()) => Err(io::Error::other("the reading ended without a result")),
                        Err(panic) => std::panic::resume_unwind(panic),
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let was_armed = armed;
            while let Some(event) = broker.try_recv() {
                match self.hear(event, &mut armed) {
                    Heard::Leave(exit) => return Ok(Some(exit)),
                    Heard::Redraw => shown = u64::MAX,
                    Heard::Nothing => {}
                }
            }
            let (secs, frame) = if self.options.reduced_motion {
                (0, None)
            } else {
                let elapsed = started.elapsed();
                (elapsed.as_secs(), Some(spinner_frame(elapsed)))
            };
            if secs != shown || frame != self.state.spinner || armed != was_armed {
                shown = secs;
                self.state.spinner = frame;
                self.state.busy = Some(busy_text_with(None, Some(ACQUIRING), secs, armed));
                self.draw()?;
            }
        }
    }
}
