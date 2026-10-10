// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one host adapter around the actual [`SessionRuntime`] (ADR-133 · the portable session).
//!
//! One worker thread owns the runtime and runs one turn at a time through
//! [`SessionRuntime::submit`], exactly as the terminal doors do. Everything a client may read
//! or decide while a turn runs lives apart, in custody behind one short lock: the snapshot
//! published last with the very [`Waiting`] value it showed, the handles published before it,
//! the commands accepted and their recorded results, the event log, and the Stop token of the
//! turn under way. No lock is held across a provider call; reading, Stop and close never wait
//! on the turn.
//!
//! A line reaches the Session only when it names the CURRENT snapshot, and the Session receives
//! that snapshot's retained `Waiting` value, never one rebuilt from the wire: a question's
//! identity keeps the incarnation that asked it. A command identity already known answers
//! before any freshness judgment: the same bytes get the recorded result again (an original
//! still running is awaited, never run twice), other bytes a conflict. A Stop is bound to the
//! turn it found and is linearized with that turn's settlement under the custody lock: if it
//! wins, the stopped preparation's late result is withdrawn before any snapshot is published and
//! the run it requested is never admitted; if the settlement wins, it reports that there was
//! nothing left to stop. While the turn's run executes, its door's Stop handle is armed and
//! disarmed with the phase under the same lock: a Stop asks that run once to stop at its next
//! wave boundary, before its child even exists if need be, and a second Stop sends nothing more.

mod worker;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use nika_session::SessionRuntime;
use nika_session::work::{CONTRACT as WORK_CONTRACT, Waiting};
use nika_types::cancel::CancelCtx;

use crate::run::{RunDoor, RunStop, Stopping};
use crate::wire::{
    ActivityWire, Body, Busy, Command, Frame, Outcome, Refused, Snapshot, TurnPhase,
};

/// What a client's command became.
#[derive(Debug)]
#[non_exhaustive]
pub enum Dispatch {
    /// A direct reply, outside the log: a refusal, a replayed result or a read.
    Reply(Frame),
    /// A frame the log records (a Stop's receipt): a door that streams the log writes it there.
    Logged(Frame),
    /// The command reached the Session; its result will be an event of the log.
    Accepted {
        /// The command identity whose result to await.
        command: String,
        /// The same command was already running under that identity with the same bytes.
        repeated: bool,
    },
    /// The Session is closing: the `closed` frame ends its log.
    Closing,
}

/// One Session behind a wire.
pub struct SessionHost {
    shared: Arc<Shared>,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
}

impl std::fmt::Debug for SessionHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHost")
            .field("session", &self.shared.session)
            .finish_non_exhaustive()
    }
}

/// What the worker is asked, in order.
enum Job {
    Submit {
        command: String,
        line: String,
        shown: Waiting,
    },
    Close,
}

/// One published snapshot, kept with the `Waiting` value it showed.
struct Published {
    handle: String,
    seq: u64,
    waiting: Waiting,
    work: serde_json::Value,
    details: String,
}

/// One command identity, bound to the digest of its bytes.
enum Entry {
    Running([u8; 32]),
    Done([u8; 32], Box<Frame>),
}

/// The turn under way.
struct Turn {
    command: String,
    phase: TurnPhase,
    token: Option<CancelCtx>,
    stop: bool,
    /// The Stop of the run the turn executes, armed while it runs, when its door can stop it.
    run: Option<Arc<dyn RunStop>>,
}

impl Turn {
    /// A Stop for the run the turn executes: one request through its door's handle; a later
    /// Stop is told where that request stands and sends nothing more (never an abort).
    fn stop_run(&mut self) -> &'static str {
        let Some(run) = &self.run else {
            return "run_underway";
        };
        if self.stop {
            return match self.phase {
                TurnPhase::Stopping => "run_stopping",
                _ => "stop_requested",
            };
        }
        match run.stop() {
            Stopping::Signalled => {
                self.stop = true;
                self.phase = TurnPhase::Stopping;
                "run_stopping"
            }
            Stopping::Pending => {
                self.stop = true;
                "stop_requested"
            }
            Stopping::Ended => "nothing_to_stop",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    Open,
    Closing,
    Closed,
}

struct Custody {
    life: Life,
    current: Published,
    past: BTreeSet<String>,
    publishes: u64,
    events: Vec<Frame>,
    ledger: BTreeMap<String, Entry>,
    turn: Option<Turn>,
}

pub(crate) struct Shared {
    session: String,
    custody: Mutex<Custody>,
    changed: Condvar,
    notify: tokio::sync::Notify,
    #[cfg(test)]
    pause: Mutex<Option<Pause>>,
}

/// A test's hold on the worker at a named point of a turn.
#[cfg(test)]
pub(crate) type Pause = Arc<dyn Fn(&'static str) + Send + Sync>;

/// 128 random bits, hex.
fn random_hex() -> Option<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(
        bytes
            .iter()
            .fold(String::with_capacity(32), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            }),
    )
}

/// Handles minted without entropy, numbered once per process.
static FALLBACK: AtomicU64 = AtomicU64::new(0);

/// The runtime's state as one snapshot publishes it; [`Shared::settle`] numbers it.
fn publish(runtime: &SessionRuntime, session: &str) -> Published {
    let work = serde_json::to_value(runtime.work()).unwrap_or_else(
        |error| serde_json::json!({"contract": WORK_CONTRACT, "error": error.to_string()}),
    );
    // A handle never repeats within its Session; without entropy it still names its Session.
    let handle = random_hex().map_or_else(
        || {
            let n = FALLBACK.fetch_add(1, Ordering::Relaxed);
            format!("snp_{}_{n}", session.trim_start_matches("ses_"))
        },
        |hex| format!("snp_{hex}"),
    );
    Published {
        handle,
        seq: 0,
        waiting: runtime.waiting(),
        work,
        details: runtime.details(),
    }
}

impl Custody {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            handle: self.current.handle.clone(),
            seq: self.current.seq,
            busy: (self.turn.as_ref()).map(|turn| Busy::new(&turn.command, turn.phase, turn.stop)),
            work: self.current.work.clone(),
        }
    }

    /// Append `body` to the log as its next event.
    fn log(&mut self, session: &str, body: Body) -> Frame {
        let event = u64::try_from(self.events.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let frame = Frame::new(session, Some(event), body);
        self.events.push(frame.clone());
        frame
    }

    /// The answer an identity already bound gives, before any other judgment.
    fn recorded(
        &self,
        session: &str,
        command: &str,
        digest: [u8; 32],
        line: Option<&str>,
    ) -> Option<Dispatch> {
        Some(match self.ledger.get(command)? {
            Entry::Running(bound) | Entry::Done(bound, _) if *bound != digest => {
                Dispatch::Reply(Frame::refused(
                    session,
                    Refused::CommandConflict,
                    format!("the command `{command}` is bound to other bytes · nothing was done"),
                    Some(command),
                    line,
                    Some(self.snapshot()),
                ))
            }
            Entry::Running(_) => Dispatch::Accepted {
                command: command.to_owned(),
                repeated: true,
            },
            Entry::Done(_, frame) => Dispatch::Reply(frame.replayed()),
        })
    }

    fn turn_of(&mut self, command: &str) -> Option<&mut Turn> {
        self.turn.as_mut().filter(|turn| turn.command == command)
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Custody> {
        self.custody.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wake(&self) {
        self.changed.notify_all();
        self.notify.notify_waiters();
    }

    /// A named point of the worker's turn, where a test may hold it.
    #[cfg_attr(not(test), allow(clippy::unused_self))]
    fn checkpoint(&self, at: &'static str) {
        #[cfg(test)]
        {
            let pause = self
                .pause
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(pause) = pause {
                pause(at);
            }
        }
        #[cfg(not(test))]
        let _ = at;
    }

    /// One activity of the turn under way, logged under its command.
    fn activity(&self, activity: ActivityWire) {
        let mut custody = self.lock();
        if custody.life == Life::Closed {
            return;
        }
        let Some(command) = custody.turn.as_ref().map(|turn| turn.command.clone()) else {
            return;
        };
        custody.log(&self.session, Body::Activity { command, activity });
        drop(custody);
        self.wake();
    }

    /// The turn's fresh Stop token; a Stop that already won cancels it at once.
    fn arm(&self, command: &str, token: CancelCtx) {
        let mut custody = self.lock();
        if let Some(turn) = custody.turn_of(command) {
            if turn.stop {
                token.cancel();
            }
            turn.token = Some(token);
        }
    }

    /// Move the turn to `phase`; answers whether a Stop won while it prepared.
    fn phase(&self, command: &str, phase: TurnPhase) -> bool {
        let mut custody = self.lock();
        let stopped = custody.turn_of(command).is_some_and(|turn| {
            turn.phase = phase;
            turn.stop
        });
        drop(custody);
        self.wake();
        stopped
    }

    /// The turn's run starts executing, its door's Stop handle armed in the same step: a Stop
    /// that finds the run under way finds the handle too, before any child exists. A Stop the
    /// turn already took applies to this run at once.
    fn running(&self, command: &str, run: Option<Arc<dyn RunStop>>) {
        let mut custody = self.lock();
        if let Some(turn) = custody.turn_of(command) {
            turn.phase = TurnPhase::Running;
            if turn.stop
                && let Some(run) = &run
            {
                let _pending = run.stop();
            }
            turn.run = run;
        }
        drop(custody);
        self.wake();
    }

    /// The run reported its start: a Stop that waited for it has reached it now (its door sends
    /// a waiting Stop as the run starts).
    fn run_started(&self, command: &str) {
        let mut custody = self.lock();
        let Some(turn) = custody.turn_of(command) else {
            return;
        };
        if turn.stop && turn.run.is_some() && turn.phase == TurnPhase::Running {
            turn.phase = TurnPhase::Stopping;
            drop(custody);
            self.wake();
        }
    }

    /// The run returned: the turn settles and its Stop handle is disarmed in one step. Answers
    /// whether a Stop was taken for that run, and whether it reached it.
    fn ran(&self, command: &str) -> (bool, bool) {
        let mut custody = self.lock();
        let stop = custody.turn_of(command).map_or((false, false), |turn| {
            let stop = (turn.stop, turn.phase == TurnPhase::Stopping);
            turn.phase = TurnPhase::Settling;
            turn.run = None;
            stop
        });
        drop(custody);
        self.wake();
        stop
    }

    /// Publish the turn's result: the new snapshot, the command's recorded result, its event.
    fn settle(&self, command: &str, outcomes: Vec<Outcome>, mut published: Published, quit: bool) {
        let mut custody = self.lock();
        custody.publishes = custody.publishes.saturating_add(1);
        published.seq = custody.publishes;
        let before = std::mem::replace(&mut custody.current, published);
        custody.past.insert(before.handle);
        custody.turn = None;
        if quit {
            custody.life = Life::Closing;
        }
        let snapshot = custody.snapshot();
        let frame = custody.log(
            &self.session,
            Body::Submitted {
                command: command.to_owned(),
                op: "submit",
                replayed: false,
                outcomes,
                snapshot,
            },
        );
        if let Some(Entry::Running(digest)) = custody.ledger.get(command) {
            let digest = *digest;
            custody
                .ledger
                .insert(command.to_owned(), Entry::Done(digest, Box::new(frame)));
        }
        drop(custody);
        self.wake();
    }

    /// The worker ended: the runtime is gone and its history released; the log closes.
    fn finish(&self) {
        let mut custody = self.lock();
        if custody.life == Life::Closed {
            return;
        }
        custody.life = Life::Closed;
        custody.turn = None;
        let snapshot = custody.snapshot();
        custody.log(&self.session, Body::Closed { snapshot });
        drop(custody);
        self.wake();
    }
}

impl SessionHost {
    /// Host `runtime` (opened by its door, its history enabled), with the door's run port and the
    /// notices its opening said. The first event is `opened`.
    ///
    /// # Errors
    /// No entropy for the Session's identity, or the worker thread could not start.
    pub fn start(
        mut runtime: SessionRuntime,
        door: Box<dyn RunDoor>,
        notices: Vec<String>,
    ) -> std::io::Result<Self> {
        let session = random_hex()
            .map(|hex| format!("ses_{hex}"))
            .ok_or_else(|| std::io::Error::other("no entropy for a Session identity"))?;
        let mut current = publish(&runtime, &session);
        current.seq = 1;
        let mut custody = Custody {
            life: Life::Open,
            current,
            past: BTreeSet::new(),
            publishes: 1,
            events: Vec::new(),
            ledger: BTreeMap::new(),
            turn: None,
        };
        let snapshot = custody.snapshot();
        custody.log(&session, Body::Opened { snapshot, notices });
        let shared = Arc::new(Shared {
            session,
            custody: Mutex::new(custody),
            changed: Condvar::new(),
            notify: tokio::sync::Notify::new(),
            #[cfg(test)]
            pause: Mutex::new(None),
        });
        let hook = Arc::clone(&shared);
        runtime.on_activity(Arc::new(move |activity| {
            hook.activity(ActivityWire::of(activity));
        }));
        let (jobs, receive) = mpsc::channel();
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("nika-session-host".to_owned())
            .spawn(move || worker::serve(runtime, door, &worker, &receive))?;
        Ok(Self {
            shared,
            jobs: Mutex::new(Some(jobs)),
        })
    }

    /// The Session's identity on the wire (`ses_…`), minted for this incarnation.
    #[must_use]
    pub fn session(&self) -> &str {
        &self.shared.session
    }

    /// Whether the Session's log is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.lock().life == Life::Closed
    }

    /// The `opened` frame (event 1).
    #[must_use]
    pub fn opened(&self) -> Option<Frame> {
        self.shared.lock().events.first().cloned()
    }

    /// The current snapshot, with the turn under way.
    #[must_use]
    pub fn snapshot(&self) -> Frame {
        let custody = self.shared.lock();
        Frame::new(
            &self.shared.session,
            None,
            Body::Current {
                snapshot: custody.snapshot(),
            },
        )
    }

    /// The details card of the current snapshot, as the Session wrote it when it was published.
    #[must_use]
    pub fn details(&self) -> Frame {
        let custody = self.shared.lock();
        Frame::new(
            &self.shared.session,
            None,
            Body::Details {
                snapshot: custody.current.handle.clone(),
                text: custody.current.details.clone(),
            },
        )
    }

    /// Hold the worker at the named points of its turns (tests only).
    #[cfg(test)]
    pub(crate) fn pause_at(&self, pause: Pause) {
        *self
            .shared
            .pause
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(pause);
    }

    /// The current snapshot, for a refusal.
    pub(crate) fn current(&self) -> Snapshot {
        self.shared.lock().snapshot()
    }

    /// The number of the last event logged and the current snapshot, read at one instant: what a
    /// resync hands a client, so that no event after it is older than its snapshot.
    pub(crate) fn resync_point(&self) -> (u64, Snapshot) {
        let custody = self.shared.lock();
        let last = u64::try_from(custody.events.len()).unwrap_or(u64::MAX);
        (last, custody.snapshot())
    }

    /// One command. Reads and refusals answer at once; a submit reaches the Session and its
    /// result is logged; a Stop's receipt is logged at once.
    #[must_use]
    pub fn dispatch(&self, command: Command) -> Dispatch {
        let digest = command.digest();
        match command {
            Command::Submit {
                command,
                snapshot,
                line,
            } => self.submit(digest, command, &snapshot, &line),
            Command::Stop { command } => self.stop(digest, command),
            Command::Close => self.close(),
            Command::Snapshot => Dispatch::Reply(self.snapshot()),
            Command::Details => Dispatch::Reply(self.details()),
        }
    }

    fn submit(&self, digest: [u8; 32], command: String, snapshot: &str, line: &str) -> Dispatch {
        let session = self.shared.session.as_str();
        let mut custody = self.shared.lock();
        if let Some(answer) = custody.recorded(session, &command, digest, Some(line)) {
            return answer;
        }
        let refuse = |custody: &Custody, error: Refused, message: String| {
            Dispatch::Reply(Frame::refused(
                session,
                error,
                message,
                Some(&command),
                Some(line),
                Some(custody.snapshot()),
            ))
        };
        if custody.life != Life::Open {
            let message = "this Session is closing · the line was not taken".to_owned();
            return refuse(&custody, Refused::SessionNotFound, message);
        }
        if let Some(turn) = &custody.turn {
            let message = format!(
                "the command `{}` is under way · this line was not taken",
                turn.command
            );
            return refuse(&custody, Refused::Busy, message);
        }
        if snapshot != custody.current.handle {
            let (error, message) = if custody.past.contains(snapshot) {
                (
                    Refused::StaleSnapshot,
                    "the line was typed against an earlier snapshot · it was not taken · read the current one",
                )
            } else {
                (
                    Refused::UnknownSnapshot,
                    "this Session never published that snapshot · the line was not taken",
                )
            };
            return refuse(&custody, error, message.to_owned());
        }
        let shown = custody.current.waiting.clone();
        // A review answer continues a held run: no preparation is under way for a Stop.
        let phase = if matches!(shown, Waiting::RunReview { .. }) {
            TurnPhase::Running
        } else {
            TurnPhase::Preparing
        };
        let job = Job::Submit {
            command: command.clone(),
            line: line.to_owned(),
            shown,
        };
        if !self.send(job) {
            let message = "this Session has ended · the line was not taken".to_owned();
            return refuse(&custody, Refused::SessionNotFound, message);
        }
        custody
            .ledger
            .insert(command.clone(), Entry::Running(digest));
        custody.turn = Some(Turn {
            command: command.clone(),
            phase,
            token: None,
            stop: false,
            run: None,
        });
        let accepted = Body::Accepted {
            command: command.clone(),
            op: "submit",
        };
        custody.log(session, accepted);
        drop(custody);
        self.shared.wake();
        Dispatch::Accepted {
            command,
            repeated: false,
        }
    }

    fn stop(&self, digest: [u8; 32], command: String) -> Dispatch {
        let session = self.shared.session.as_str();
        let mut custody = self.shared.lock();
        if let Some(answer) = custody.recorded(session, &command, digest, None) {
            return answer;
        }
        if custody.life == Life::Closed {
            let snapshot = Some(custody.snapshot());
            return Dispatch::Reply(Frame::refused(
                session,
                Refused::SessionNotFound,
                "this Session is closed",
                Some(&command),
                None,
                snapshot,
            ));
        }
        let (receipt, target) = match custody.turn.as_mut() {
            None => ("nothing_to_stop", None),
            Some(turn) => {
                let receipt = match turn.phase {
                    TurnPhase::Running | TurnPhase::Stopping => turn.stop_run(),
                    TurnPhase::Settling => "nothing_to_stop",
                    TurnPhase::Preparing => {
                        turn.stop = true;
                        if let Some(token) = &turn.token {
                            token.cancel();
                        }
                        "stop_requested"
                    }
                };
                (receipt, Some(turn.command.clone()))
            }
        };
        let snapshot = custody.snapshot();
        let body = Body::Stopped {
            command: command.clone(),
            op: "stop",
            replayed: false,
            receipt,
            target,
            snapshot,
        };
        let frame = custody.log(session, body);
        custody
            .ledger
            .insert(command, Entry::Done(digest, Box::new(frame.clone())));
        drop(custody);
        self.shared.wake();
        Dispatch::Logged(frame)
    }

    /// Close: a preparation under way is stopped, nothing more is admitted, and the log closes
    /// once the turn under way settled and the runtime released its history.
    fn close(&self) -> Dispatch {
        let mut custody = self.shared.lock();
        if custody.life != Life::Open {
            return Dispatch::Closing;
        }
        custody.life = Life::Closing;
        if let Some(turn) = custody.turn.as_mut()
            && turn.phase == TurnPhase::Preparing
        {
            turn.stop = true;
            if let Some(token) = &turn.token {
                token.cancel();
            }
        }
        let sender = self
            .jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            // The worker closes after the turn it holds; a gone worker already closed.
            let _sent = sender.send(Job::Close);
        }
        drop(custody);
        self.shared.wake();
        Dispatch::Closing
    }

    fn send(&self, job: Job) -> bool {
        let jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        jobs.as_ref().is_some_and(|sender| sender.send(job).is_ok())
    }

    /// The recorded result of `command`, once it settled (blocking). `None` when the identity is
    /// unknown or the Session ended before it settled.
    #[must_use]
    pub fn wait_result(&self, command: &str) -> Option<Frame> {
        let mut custody = self.shared.lock();
        loop {
            match custody.ledger.get(command) {
                Some(Entry::Done(_, frame)) => return Some(Frame::clone(frame)),
                Some(Entry::Running(_)) if custody.life != Life::Closed => {}
                _ => return None,
            }
            custody = (self.shared.changed.wait(custody)).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// [`Self::wait_result`], awaited.
    pub async fn result(&self, command: &str) -> Option<Frame> {
        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let custody = self.shared.lock();
                match custody.ledger.get(command) {
                    Some(Entry::Done(_, frame)) => return Some(Frame::clone(frame)),
                    Some(Entry::Running(_)) if custody.life != Life::Closed => {}
                    _ => return None,
                }
            }
            notified.await;
        }
    }

    /// The events after event `after`, and whether the log is complete (the Session closed).
    #[must_use]
    pub fn events_after(&self, after: u64) -> (Vec<Frame>, bool) {
        let custody = self.shared.lock();
        let from = usize::try_from(after).unwrap_or(usize::MAX);
        let events = custody.events.get(from..).unwrap_or_default().to_vec();
        (events, custody.life == Life::Closed)
    }

    /// The number of the last event logged.
    #[must_use]
    pub fn last_event(&self) -> u64 {
        u64::try_from(self.shared.lock().events.len()).unwrap_or(u64::MAX)
    }

    /// The events after `after`, waiting until there is one or the log is complete (blocking).
    #[must_use]
    pub fn wait_events_after(&self, after: u64) -> (Vec<Frame>, bool) {
        let from = usize::try_from(after).unwrap_or(usize::MAX);
        let mut custody = self.shared.lock();
        while custody.events.len() <= from && custody.life != Life::Closed {
            custody = (self.shared.changed.wait(custody)).unwrap_or_else(PoisonError::into_inner);
        }
        let events = custody.events.get(from..).unwrap_or_default().to_vec();
        (events, custody.life == Life::Closed)
    }

    /// [`Self::wait_events_after`], awaited.
    pub async fn next_events(&self, after: u64) -> (Vec<Frame>, bool) {
        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (events, complete) = self.events_after(after);
            if !events.is_empty() || complete {
                return (events, complete);
            }
            notified.await;
        }
    }

    /// The `closed` frame once the log closed (blocking).
    #[must_use]
    pub fn wait_closed(&self) -> Option<Frame> {
        let mut custody = self.shared.lock();
        while custody.life != Life::Closed {
            custody = (self.shared.changed.wait(custody)).unwrap_or_else(PoisonError::into_inner);
        }
        custody.events.last().cloned()
    }

    /// [`Self::wait_closed`], awaited.
    pub async fn closed(&self) -> Option<Frame> {
        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let custody = self.shared.lock();
                if custody.life == Life::Closed {
                    return custody.events.last().cloned();
                }
            }
            notified.await;
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
pub(crate) mod tests;
