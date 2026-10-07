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
use crate::workspace::desk::{Desk, Got, Reading, acquire_all};
use nika_display::run_story::ExecutionId;

/// Hand what was acquired to the desk; only when the desk accepts it for the
/// reading it was asked for is a run's journal then adopted by the host (its
/// written names and child relations, for the reads that follow). A journal
/// the host does not adopt is applied again at once to that same reading,
/// the same Proof lending nothing: its verdict and reasons stay readable,
/// nothing it records is shown, and nothing is asked again until a refresh.
/// `true` when the reading was current, whatever the adoption.
pub(super) fn settle<C: Conversation + ?Sized>(
    desk: &mut Desk,
    conversation: &mut C,
    (execution, reading): (ExecutionId, Reading),
    got: Vec<Got>,
) -> bool {
    let proof = got.iter().find_map(|item| match item {
        Got::Proof(proven) if proven.events().is_some() => Some(proven.clone()),
        _ => None,
    });
    let accepted = desk.acquired(execution, reading, got);
    if accepted
        && let Some(proven) = proof
        && !conversation.adopt(&execution, &proven)
    {
        let unlent = proven.without_lending(NOT_ADOPTED);
        desk.acquired(execution, reading, vec![Got::Proof(unlent)]);
    }
    accepted
}

/// Why a verified journal's records are not shown: the host lent nothing.
const NOT_ADOPTED: &str = "this host did not adopt this reading";

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
                Ok((mut conversation, got)) => {
                    settle(&mut self.desk, &mut conversation, (execution, reading), got);
                    self.conversation = Some(conversation);
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
                    Heard::Repaint => {
                        self.repaint(broker)?;
                        shown = u64::MAX;
                    }
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod kept_tests {
    //! A run kept from an earlier session, read again on a real host: its
    //! journal is captured first whatever the face, adopted only once the
    //! desk accepts that very reading, and a refresh while a capture is on
    //! the worker leaves no earlier lending readable.

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use nika_display::run_story::{ExecutionId, RunFrame};

    use super::super::{Busy, busy_key};
    use super::settle;
    use crate::model::{Beat, Conversation, Handoff, Presentation, Turn, UiState};
    use crate::session::Live;
    use crate::session::acquire::Proven;
    use crate::session::feed::Observed;
    use crate::session::legs::legs_tests::{EXEC, reopened_closing, settled_at, written_journal};
    use crate::workspace::desk::{Desk, acquire_all};
    use crate::workspace::focus::Region;
    use crate::workspace::live::Want;
    use crate::workspace::object::Object;

    const SIZE: (u16, u16) = (120, 40);

    /// Press `code` as the shell routes keys during work.
    fn press(desk: &mut Desk, code: KeyCode) {
        let state = UiState::new(Presentation::Workspace, false, SIZE);
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(busy_key(&state, desk, key), Busy::Region, "{code:?}");
    }

    /// Acquire on `host` what the desk asks and hand it back as the shell
    /// does: what was asked, and whether it was accepted.
    fn read<C: Conversation>(desk: &mut Desk, host: &mut C) -> (Vec<Want>, bool) {
        let (execution, wants, reading) = desk.wanted().expect("something is asked");
        let got = acquire_all(host, &execution, wants.clone());
        (wants, settle(desk, host, (execution, reading), got))
    }

    /// The file the kept run wrote, as the host lends it now.
    fn copy(host: &mut Live) -> Option<Vec<u8>> {
        let id = serde_json::from_value(serde_json::json!({ "uuid": EXEC })).expect("an id");
        let fetched = host.fetch(&id, "./out/copy.md").expect("a host");
        fetched.bytes().map(<[u8]>::to_vec)
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

    /// A fresh directory for one reopened host.
    fn room(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!("nika-tui-{tag}-{}-{nanos}", std::process::id());
        std::env::temp_dir().join(name)
    }

    /// The desk a reopen shows: the kept run in view, the object focused.
    fn kept_desk(host: &Live) -> Desk {
        let mut desk = Desk::new();
        desk.view = Some(crate::model::demo_project());
        desk.kept(host.kept_run());
        desk.focus.region = Region::Object;
        desk
    }

    #[test]
    fn a_kept_run_is_read_from_its_journal_and_a_stale_reading_lends_nothing() {
        let base = room("kept");
        let calls = Arc::new(AtomicUsize::new(0));
        let close = serde_json::json!([{"key": "status", "value": "succeeded"},
            {"key": "outputs", "value": "{\"total\":5}"}]);
        let mut host = reopened_closing(&base, close, &calls);
        let mut desk = kept_desk(&host);
        // Whatever the face, the journal first; its tasks and outputs then.
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        let run = shown(&mut desk);
        assert!(
            run.contains("✔ save") && run.contains("captured bytes"),
            "{run}"
        );
        press(&mut desk, KeyCode::Right);
        let outputs = shown(&mut desk);
        assert!(
            outputs.contains("total") && outputs.contains("its journal"),
            "{outputs}"
        );
        press(&mut desk, KeyCode::Right);
        let file = vec![Want::File("./out/copy.md".to_owned())];
        assert_eq!(read(&mut desk, &mut host), (file, true));
        assert!(shown(&mut desk).contains("# Copy"), "{}", shown(&mut desk));
        // A refresh on this face asks the journal again before any file.
        press(&mut desk, KeyCode::Char('r'));
        let (execution, wants, stale) = desk.wanted().expect("asked again");
        assert_eq!(wants, [Want::Proof], "no file before a new reading");
        let late = acquire_all(&mut host, &execution, wants);
        // While that capture is on the worker: another refresh, another face.
        press(&mut desk, KeyCode::Char('r'));
        press(&mut desk, KeyCode::Left);
        assert!(
            !settle(&mut desk, &mut host, (execution, stale), late),
            "stale"
        );
        assert_eq!(copy(&mut host), None, "the capture revoked the lending");
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        assert_eq!(copy(&mut host).as_deref(), Some(&b"# Copy\n"[..]));
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no model call");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A journal an older engine wrote records no workflow outputs map: the
    /// first face opened (its outputs) asks it before any Proof, its tasks
    /// and the file it wrote are read from it, and its map stays unknown.
    #[test]
    fn an_older_journal_lends_its_tasks_and_file_and_keeps_its_map_unknown() {
        let base = room("older");
        let calls = Arc::new(AtomicUsize::new(0));
        let close = serde_json::json!([{"key": "status", "value": "succeeded"}]);
        let mut host = reopened_closing(&base, close, &calls);
        let mut desk = kept_desk(&host);
        press(&mut desk, KeyCode::Right);
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        let outputs = shown(&mut desk);
        assert!(
            outputs.contains("older") && outputs.contains("unknown") && !outputs.contains("{}"),
            "{outputs}"
        );
        press(&mut desk, KeyCode::Right);
        let file = vec![Want::File("./out/copy.md".to_owned())];
        assert_eq!(read(&mut desk, &mut host), (file, true));
        assert!(shown(&mut desk).contains("# Copy"), "{}", shown(&mut desk));
        press(&mut desk, KeyCode::Left);
        press(&mut desk, KeyCode::Left);
        assert!(shown(&mut desk).contains("✔ save"), "{}", shown(&mut desk));
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no model call");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A host whose Proof lends a verified journal's events (or is refused)
    /// but which adopts nothing, the trait's default; it counts what it is
    /// asked.
    #[derive(Default)]
    struct Unadopted {
        refused: bool,
        proofs: usize,
        submitted: usize,
    }

    impl Conversation for Unadopted {
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
        fn prove(&mut self, _execution: &ExecutionId) -> Option<Proven> {
            self.proofs += 1;
            let trace = ".nika/traces/t.ndjson";
            if self.refused {
                return Some(Proven::refused(trace, "a symlink, never followed"));
            }
            let doc = serde_json::json!({"tier": "ok", "exit": 0, "chain": {"events": 4,
                "head": "cd".repeat(32), "headline": "intact"}, "lines": []});
            let close = serde_json::json!([{"key": "status", "value": "succeeded"},
                {"key": "outputs", "value": "{\"total\":5}"}]);
            let events = (written_journal(close).0.lines())
                .filter_map(|line| match RunFrame::decode(line) {
                    Some(RunFrame::Event(event)) => Some(*event),
                    _ => None,
                })
                .collect();
            Some(Proven::judged(trace, doc, Vec::new()).lending(events))
        }
    }

    /// The desk a reopen shows for the run `EXEC` an earlier session kept.
    fn record_desk() -> Desk {
        let mut record = nika_session::KeptRun::new();
        record.workflow = Some("two.nika".to_owned());
        record.execution = Some(EXEC.to_owned());
        record.exit = Some(0);
        let mut desk = Desk::new();
        desk.view = Some(crate::model::demo_project());
        desk.kept(Some(Ok(record)));
        desk.focus.region = Region::Object;
        desk
    }

    /// A verified journal the host does not adopt: once the desk accepted
    /// the reading, its Proof stays readable with its verdict and witness,
    /// nothing it records is shown or lent, nothing is asked again on its
    /// own, and a refresh asks it anew; nothing runs.
    #[test]
    fn a_reading_the_host_does_not_adopt_keeps_its_proof_and_lends_nothing() {
        let mut host = Unadopted::default();
        let mut desk = record_desk();
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        let run = shown(&mut desk);
        assert!(!run.contains("✔ save") && run.contains("adopt"), "{run}");
        assert!(desk.wanted().is_none(), "nothing asked again on its own");
        press(&mut desk, KeyCode::Right);
        let outputs = shown(&mut desk);
        assert!(
            !outputs.contains("total") && outputs.contains("adopt"),
            "{outputs}"
        );
        press(&mut desk, KeyCode::Right);
        let files = shown(&mut desk);
        assert!(!files.contains("reported written by"), "{files}");
        assert!(desk.wanted().is_none(), "no file asked");
        press(&mut desk, KeyCode::Right);
        let proof = shown(&mut desk);
        assert!(
            proof.contains("verdict · OK") && proof.contains("captured bytes abababababab"),
            "{proof}"
        );
        assert!(desk.wanted().is_none(), "no Proof asked again");
        press(&mut desk, KeyCode::Char('r'));
        let asked = desk.wanted().map(|(_, wants, _)| wants);
        assert_eq!(asked, Some(vec![Want::Proof]), "a refresh asks anew");
        assert_eq!((host.proofs, host.submitted), (1, 0));
    }

    /// A Proof already refused keeps its own reason: its faces say why, not
    /// that the host declined it, and nothing is asked again on its own.
    #[test]
    fn a_refused_proof_keeps_its_own_reason() {
        let mut host = Unadopted {
            refused: true,
            ..Unadopted::default()
        };
        let mut desk = record_desk();
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        let run = shown(&mut desk);
        assert!(run.contains("a symlink") && !run.contains("adopt"), "{run}");
        assert!(desk.wanted().is_none());
    }

    /// A reading made late by a refresh neither adopts nor withdraws the one
    /// current since: its settle is dropped, the current projection and the
    /// host's lending stay.
    #[test]
    fn a_late_reading_neither_adopts_nor_withdraws_the_current_one() {
        let base = room("late");
        let calls = Arc::new(AtomicUsize::new(0));
        let close = serde_json::json!([{"key": "status", "value": "succeeded"}]);
        let mut host = reopened_closing(&base, close, &calls);
        let mut desk = kept_desk(&host);
        let (execution, wants, stale) = desk.wanted().expect("a first reading");
        let late = acquire_all(&mut host, &execution, wants);
        press(&mut desk, KeyCode::Char('r'));
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        assert!(
            !settle(&mut desk, &mut host, (execution, stale), late),
            "late"
        );
        assert!(shown(&mut desk).contains("✔ save"), "{}", shown(&mut desk));
        assert_eq!(copy(&mut host).as_deref(), Some(&b"# Copy\n"[..]));
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no model call");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A run followed in this session keeps the tasks its stream folded when
    /// its Proof lends events the host does not adopt: only a kept run's
    /// history is ever lent.
    #[test]
    fn a_followed_run_keeps_its_stream_when_its_proof_is_not_adopted() {
        let close = serde_json::json!([{"key": "status", "value": "succeeded"}]);
        let (raw, head, len) = written_journal(close);
        let asked = Observed::Asked {
            workflow: "two.nika".to_owned(),
            resume: false,
            typed: true,
            look: None,
        };
        let frames =
            (raw.lines()).map(|line| Observed::Frame(RunFrame::decode(line).expect("a frame")));
        let settled = Observed::Frame(settled_at(EXEC, &head, len));
        let mut desk = Desk::new();
        desk.view = Some(crate::model::demo_project());
        desk.observe(
            std::iter::once(asked)
                .chain(frames)
                .chain(std::iter::once(settled)),
        );
        desk.focus.region = Region::Object;
        assert!(shown(&mut desk).contains("✔ save"), "{}", shown(&mut desk));
        for _ in 0..3 {
            press(&mut desk, KeyCode::Right);
        }
        let mut host = Unadopted::default();
        assert_eq!(read(&mut desk, &mut host), (vec![Want::Proof], true));
        for _ in 0..3 {
            press(&mut desk, KeyCode::Left);
        }
        assert!(shown(&mut desk).contains("✔ save"), "{}", shown(&mut desk));
        assert_eq!(host.submitted, 0);
    }
}
