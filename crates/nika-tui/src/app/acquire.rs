// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the run in view's face needs (a file it wrote, read now; its journal,
//! captured and verified) acquired on a worker thread, as a turn is computed:
//! the shell keeps hearing keys and drawing meanwhile (the composer takes
//! words, a bare Enter waits, the page keys scroll, `Ctrl+C` twice leaves).
//! The result is applied only to the leg and the reading it was asked for: a
//! leg read again (`r`) since, or another leg, leaves it unapplied, and a
//! child's journal applies only to the opening that asked it (closed and
//! opened again meanwhile, the new opening asks its own). Nothing here runs,
//! answers or calls a model.

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
        let Some((execution, wants, reading)) = self.desk.wanted() else {
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
                    self.desk.acquired(execution, reading, got);
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    //! A child journal read while the shell keeps hearing keys: closing and
    //! reopening the same child meanwhile is a new opening, which never takes
    //! the earlier opening's answer and asks its own once.

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use nika_display::run_story::{ChildRun, ExecutionId, RunFrame};

    use super::super::{Busy, busy_key};
    use crate::model::{Beat, Conversation, Handoff, Presentation, Turn, UiState};
    use crate::session::acquire::ChildRead;
    use crate::session::feed::Observed;
    use crate::workspace::desk::{Desk, acquire_all};
    use crate::workspace::focus::Region;
    use crate::workspace::object::Object;

    const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
    const SIZE: (u16, u16) = (120, 40);

    /// A conversation that answers each child read with its own number.
    #[derive(Default)]
    struct Reader {
        children: usize,
        submitted: usize,
    }

    impl Conversation for Reader {
        fn open(&mut self) -> Vec<Beat> {
            Vec::new()
        }
        fn submit(&mut self, _line: &str) -> Turn {
            self.submitted += 1;
            Turn {
                beats: Vec::new(),
                handoff: None,
            }
        }
        fn perform(&mut self, _handoff: &Handoff) -> Vec<Beat> {
            Vec::new()
        }
        fn child(
            &mut self,
            _execution: &ExecutionId,
            _task: &str,
            relation: &ChildRun,
        ) -> Option<ChildRead> {
            self.children += 1;
            let trace = relation.trace_id.clone().unwrap_or_default();
            Some(ChildRead::refused(
                &trace,
                format!("read number {}", self.children),
            ))
        }
    }

    fn frame(n: u32, kind: &str, fields: &str) -> Observed {
        let line = format!(
            r#"{{"correlation":null,"execution":{{"uuid":"{EXEC}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        );
        Observed::Frame(RunFrame::decode(&line).expect("a runtime event"))
    }

    /// A settled run whose task `call` names a child journal.
    fn desk() -> Desk {
        let row = serde_json::json!({"target": "./child.nika", "trace_id": "child.ndjson",
            "chain_head": "ab", "def_hash": "cd", "outcome": "success"})
        .to_string();
        let child = format!(
            r#"{{"key":"task","value":"call"}},{{"key":"child","value":{}}}"#,
            serde_json::to_string(&row).expect("json")
        );
        let mut desk = Desk::new();
        desk.view = Some(crate::model::demo_project());
        desk.observe(
            [
                Observed::Asked {
                    workflow: "parent.nika".to_owned(),
                    resume: false,
                    typed: true,
                    look: None,
                },
                frame(1, "workflow_started", ""),
                frame(2, "task_started", r#"{"key":"task","value":"call"}"#),
                frame(3, "task_completed", &child),
            ]
            .into_iter(),
        );
        desk.focus.region = Region::Object;
        desk
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// The object in view's rows, prepared as the shell does before a frame.
    fn shown(desk: &mut Desk) -> String {
        desk.prepare(SIZE, false, false);
        let Object::Workflow { title, body } = desk.screen(false).object else {
            panic!("the run is in view");
        };
        std::iter::once(title)
            .chain(body)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// While the first opening's read is on the worker, Backspace then Enter
    /// (routed as the shell routes keys during work) close and reopen the
    /// same child, with no change of relation or reading. The first answer
    /// then arrives: it is not this opening's, so it is not shown, and the
    /// new opening asks its own read, applied once.
    #[test]
    fn a_reopened_child_never_takes_the_earlier_opening_s_answer() {
        let state = UiState::new(Presentation::Workspace, false, SIZE);
        let mut desk = desk();
        let mut reader = Reader::default();
        assert_eq!(
            busy_key(&state, &mut desk, key(KeyCode::Enter)),
            Busy::Region
        );
        assert_eq!(
            busy_key(&state, &mut desk, key(KeyCode::Enter)),
            Busy::Region
        );
        let (execution, wants, reading) = desk.wanted().expect("the first opening asks");
        let first = acquire_all(&mut reader, &execution, wants);
        assert_eq!(
            busy_key(&state, &mut desk, key(KeyCode::Backspace)),
            Busy::Region
        );
        assert!(!shown(&mut desk).contains("child ./child.nika"), "closed");
        assert_eq!(
            busy_key(&state, &mut desk, key(KeyCode::Enter)),
            Busy::Region
        );
        assert!(shown(&mut desk).contains("child ./child.nika"), "reopened");
        desk.acquired(execution, reading, first);
        let text = shown(&mut desk);
        assert!(
            !text.contains("read number 1"),
            "the earlier opening's answer is not this one's: {text}"
        );
        let (execution, wants, reading) = desk.wanted().expect("the new opening asks its own");
        let second = acquire_all(&mut reader, &execution, wants);
        assert!(desk.acquired(execution, reading, second));
        assert!(
            shown(&mut desk).contains("read number 2"),
            "{}",
            shown(&mut desk)
        );
        assert!(desk.wanted().is_none(), "read once");
        assert_eq!((reader.children, reader.submitted), (2, 0));
    }

    /// The opening never stands in for the rest of a reading's key: an
    /// answer for this very opening but another leg, or a reading made stale
    /// by `r`, is dropped all the same.
    #[test]
    fn an_opening_s_answer_still_needs_its_leg_and_reading() {
        let state = UiState::new(Presentation::Workspace, false, SIZE);
        let mut desk = desk();
        let mut reader = Reader::default();
        busy_key(&state, &mut desk, key(KeyCode::Enter));
        busy_key(&state, &mut desk, key(KeyCode::Enter));
        let (execution, wants, reading) = desk.wanted().expect("asked");
        let other: ExecutionId = serde_json::from_value(
            serde_json::json!({"uuid": "01a0ef11-0212-70de-a8b3-99de94270000"}),
        )
        .expect("an execution");
        let foreign = acquire_all(&mut reader, &execution, wants.clone());
        assert!(!desk.acquired(other, reading, foreign), "another leg");
        let late = acquire_all(&mut reader, &execution, wants);
        assert_eq!(
            busy_key(&state, &mut desk, key(KeyCode::Char('r'))),
            Busy::Region
        );
        assert!(!desk.acquired(execution, reading, late), "a stale reading");
        assert!(
            !shown(&mut desk).contains("read number"),
            "{}",
            shown(&mut desk)
        );
        let (execution, wants, reading) = desk.wanted().expect("asked again");
        let answer = acquire_all(&mut reader, &execution, wants);
        assert!(desk.acquired(execution, reading, answer));
        assert!(
            shown(&mut desk).contains("read number 3"),
            "{}",
            shown(&mut desk)
        );
    }
}
