// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness the ledger and room witnesses share.
//!
//! Every effect of a witness stays in a parent directory the test alone owns:
//! the room is `room/` in it, and every outside target (a sentinel, a symlink
//! target, an escape name) is under `outside/` in it, so even a defective
//! backend touches nothing the test does not own and no ambient file is ever
//! a target.
//!
//! Every wait is bounded, and every bound that acts is reported: a held
//! operation says whether the test released it or its own bound did, a
//! witness thread is joined once it has answered, and the FIFO watchdog says
//! whether it had to open a peer. A bound that acted is never evidence for the
//! code under test and never a pass: it reads `HARNESS_INVALID` (a bound
//! exceeded), the read the watchdog had to free included. Named channels wait at
//! most their bound; the joins and awaits of the code under test are bounded
//! only by root's process-group deadline. The Tokio timer bounds only work that
//! yields, and a thread blocked in a syscall is freed by an owned release.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use tokio::time::error::Elapsed;

use crate::{EffectLedger, OwnedDir, Phase, RoomLimits, RootedFs};

/// The bound on every wait a witness makes.
pub(crate) const BOUND: Duration = Duration::from_secs(10);

/// The watchdog's own bound when a harness self-test needs it to act.
pub(crate) const SHORT: Duration = Duration::from_millis(200);

/// How often the watchdog offers a peer again once it has acted.
const PACE: Duration = Duration::from_millis(10);

pub(crate) fn roomy() -> Arc<EffectLedger> {
    EffectLedger::new(RoomLimits::new(1 << 20, 64))
}

/// Drain the current phase and advance: the only way a phase moves.
pub(crate) async fn next_phase(ledger: &Arc<EffectLedger>) -> Phase {
    ledger.seal_and_drain().await;
    ledger.advance().unwrap()
}

/// How a held operation's wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waited {
    /// The test released it.
    Released,
    /// Its own bound elapsed first: the test did not release it in time.
    Expired,
    /// The hold was dropped without a release.
    Abandoned,
}

/// The test's hold on one blocking operation. The operation reports that it
/// entered, waits, bounded, until the test releases it, then reports how its
/// wait ended. Dropping the hold frees the operation at once.
pub(crate) struct Hold {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
    exited: mpsc::Receiver<Waited>,
    outcome: Option<Result<Waited, &'static str>>,
}

impl Hold {
    /// A hold, and the operation it holds.
    pub(crate) fn new() -> (Self, impl FnOnce() + Send + 'static) {
        let (enter, entered) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let (exit, exited) = mpsc::channel();
        let operation = move || {
            let _ = enter.send(());
            let waited = match released.recv_timeout(BOUND) {
                Ok(()) => Waited::Released,
                Err(RecvTimeoutError::Timeout) => Waited::Expired,
                Err(RecvTimeoutError::Disconnected) => Waited::Abandoned,
            };
            let _ = exit.send(waited);
        };
        let hold = Self {
            entered,
            release: Some(release),
            exited,
            outcome: None,
        };
        (hold, operation)
    }

    /// Wait, bounded, until the held operation runs.
    pub(crate) fn entered(&self) {
        self.entered
            .recv_timeout(BOUND)
            .expect("HARNESS_INVALID: the held operation did not start within the bound");
    }

    /// Release the held operation, then wait, bounded, for its own report of
    /// how its wait ended: the rendezvous is joined, never assumed.
    pub(crate) fn release(&mut self) {
        let Some(release) = self.release.take() else {
            return;
        };
        let sent = release.send(());
        let report = self.exited.recv_timeout(BOUND);
        self.outcome = Some(match (sent, report) {
            (_, Ok(waited)) => Ok(waited),
            (Ok(()), Err(_)) => {
                Err("the released operation never reported its end within the bound")
            }
            (Err(_), Err(_)) => {
                Err("the operation could not be released and never reported its end")
            }
        });
    }

    /// After `release`: the test released the operation before its own bound
    /// and saw it end. Anything else is a slow or broken harness, never
    /// evidence about the code under test.
    pub(crate) fn check(&self) {
        match self.outcome {
            Some(Ok(Waited::Released)) => {}
            Some(Ok(waited)) => {
                panic!("HARNESS_INVALID: the held operation ended {waited:?}, not released in time")
            }
            Some(Err(why)) => panic!("HARNESS_INVALID: {why}"),
            None => panic!("HARNESS_INVALID: the hold was checked before its release"),
        }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        // A failing assertion still frees the operation: without its sender its
        // wait ends at once. No wait here, since a drop may run while unwinding.
        drop(self.release.take());
    }
}

/// Run `work` on a thread of its own and wait, bounded, for its answer; the
/// thread is joined once it has answered. A thread that never answers is left
/// behind: it holds nothing of the harness, so only a deadlock in the code under
/// test keeps it, the witness fails at once, and root's process-group deadline
/// stays the last bound of the run.
pub(crate) fn answer_within<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, &'static str> {
    let (answer, answered) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .spawn(move || {
            let _ = answer.send(work());
        })
        .map_err(|_| "HARNESS_INVALID: the witness thread could not start")?;
    match answered.recv_timeout(BOUND) {
        Ok(value) => {
            thread
                .join()
                .map_err(|_| "the witness thread panicked after answering")?;
            Ok(value)
        }
        Err(RecvTimeoutError::Disconnected) => {
            let _ = thread.join();
            Err("the witness thread ended without answering: it panicked")
        }
        Err(RecvTimeoutError::Timeout) => Err("no answer within the bound"),
    }
}

/// Run `read` on a current-thread runtime of its own, under a Tokio timeout of
/// `bound` built inside that runtime, beside an owned watchdog for the FIFO
/// `pipe`. Answers the read's outcome (or its timeout) and whether the watchdog
/// had to open a peer, once the watchdog is joined and every blocking thread of
/// the runtime has ended (dropping the runtime waits for them).
///
/// The Tokio timer bounds only a read that yields. A read blocked in the open
/// on the blocking pool, or on the runtime thread itself, is freed by the
/// watchdog alone, and that read then completes: the intervention is what
/// disqualifies it, whatever it answered.
pub(crate) fn read_beside_watchdog<F: Future>(
    pipe: &Path,
    bound: Duration,
    read: F,
) -> (Result<F::Output, Elapsed>, bool) {
    let (done, finished) = mpsc::channel::<()>();
    std::thread::scope(|scope| {
        let watchdog = scope.spawn(move || watch_fifo(pipe, &finished, bound));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("HARNESS_INVALID: no runtime for the read");
        let outcome = runtime.block_on(async move { tokio::time::timeout(bound, read).await });
        drop(runtime);
        let _ = done.send(());
        let intervened = watchdog
            .join()
            .expect("HARNESS_INVALID: the watchdog panicked");
        (outcome, intervened)
    })
}

/// The owned release of a read that may block in a FIFO open: once `bound` is
/// over without `finished`, open a peer for any reader blocked there, and keep
/// offering one every `PACE` until `finished` arrives. Answers whether it ever
/// had to.
fn watch_fifo(pipe: &Path, finished: &mpsc::Receiver<()>, bound: Duration) -> bool {
    let mut intervened = false;
    loop {
        match finished.recv_timeout(if intervened { PACE } else { bound }) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return intervened,
            Err(RecvTimeoutError::Timeout) => {
                intervened = true;
                // A peer for whoever waits in the open now; closed at once.
                let _ = nix::fcntl::open(pipe, OFlag::O_WRONLY | OFlag::O_NONBLOCK, Mode::empty());
            }
        }
    }
}

/// One room in a parent directory the test alone owns.
pub(crate) struct Room {
    parent: tempfile::TempDir,
    pub(crate) fs: RootedFs,
}

impl Room {
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        Self::with_limits(files, RoomLimits::new(1 << 20, 64))
    }

    /// `files` are fixtures the test writes before the room is served, never
    /// through the backend under test.
    pub(crate) fn with_limits(files: &[(&str, &str)], limits: RoomLimits) -> Self {
        let parent = tempfile::tempdir().unwrap();
        std::fs::create_dir(parent.path().join("room")).unwrap();
        std::fs::create_dir(parent.path().join("outside")).unwrap();
        for (rel, body) in files {
            let path = parent.path().join("room").join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let room = OwnedDir::open(&parent.path().join("room")).unwrap();
        let fs = RootedFs::new(room, EffectLedger::new(limits));
        Self { parent, fs }
    }

    /// A path in the room, on the host.
    pub(crate) fn host(&self, rel: &str) -> PathBuf {
        self.parent.path().join("room").join(rel)
    }

    /// A path outside the room, inside the test's own parent.
    pub(crate) fn outside(&self, rel: &str) -> PathBuf {
        self.parent.path().join("outside").join(rel)
    }

    /// Write an outside sentinel and answer its path.
    pub(crate) fn sentinel(&self, rel: &str, body: &str) -> PathBuf {
        let path = self.outside(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        path
    }

    /// Everything outside the room, as sorted names with their mode and bytes
    /// (a symlink's target as its bytes): what a refused effect must leave
    /// exactly as it was.
    pub(crate) fn outside_snapshot(&self) -> Vec<(String, u32, Vec<u8>)> {
        let outside = self.parent.path().join("outside");
        let mut found = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(below) = stack.pop() {
            for entry in std::fs::read_dir(outside.join(&below)).unwrap() {
                let rel = below.join(entry.unwrap().file_name());
                let host = outside.join(&rel);
                let metadata = std::fs::symlink_metadata(&host).unwrap();
                let bytes = if metadata.file_type().is_symlink() {
                    std::fs::read_link(&host)
                        .unwrap()
                        .into_os_string()
                        .into_encoded_bytes()
                } else if metadata.is_dir() {
                    stack.push(rel.clone());
                    Vec::new()
                } else {
                    std::fs::read(&host).unwrap()
                };
                let mode = metadata.permissions().mode();
                found.push((rel.display().to_string(), mode, bytes));
            }
        }
        found.sort();
        found
    }

    pub(crate) async fn to_run(&self) {
        assert_eq!(next_phase(self.fs.ledger()).await, Phase::Run);
    }
}

/// The sorted names a host directory holds, hidden ones included.
pub(crate) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
