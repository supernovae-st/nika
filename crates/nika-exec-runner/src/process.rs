// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cancellation ownership for one spawn. On Linux/macOS the unreaped leader
//! reserves the dedicated process-group identity until all output is drained.
//! Never poll `Child.wait/try_wait` while that group guard is armed. Embedders
//! must not independently reap children owned by this executor.
//!
//! Drop sends SIGKILL before dropping Child. This is a termination request,
//! not an acknowledgement of all effects ending: setsid/setpgid escapes,
//! uninterruptible processes and cleanup after runtime shutdown need a
//! stronger OS boundary. Successful background commands keep their existing
//! behavior; this guard acts only when collection fails or is abandoned.
//!
//! A process that exits without running destructors never reaches that Drop:
//! every group is also listed process-wide from its spawn until its owner
//! releases it before the reap, so [`terminate_owned_groups`] can end what is
//! still owned first.

use std::io;
use std::process::ExitStatus;
use tokio::process::{Child, Command};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, waitid};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod groups;

pub(super) struct Process {
    pub(super) child: Child,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    group: Option<Pid>,
}

impl Process {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        command.process_group(0).kill_on_drop(false);
        // Under the owned list's lock: a forced end never misses the group, and
        // nothing starts once one began.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let (child, group) = groups::OWNED.spawn(|| grouped(command))?;
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let child = command.spawn()?;
        Ok(Self {
            child,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            group: Some(group),
        })
    }

    /// Observe exit without freeing the group's identity while a descendant
    /// might still hold a pipe. Register SIGCHLD before checking, so an exit
    /// between the check and recv cannot be lost; unrelated exits just recheck.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(super) async fn exited(&mut self) -> io::Result<()> {
        let pid = self
            .group
            .ok_or_else(|| io::Error::other("process group already released"))?;
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())?;
        loop {
            match waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
            ) {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => {
                    if error == rustix::io::Errno::CHILD
                        && let Some(group) = self.group.take()
                    {
                        // An external reaper took ownership: never signal the
                        // saved group number after its leader may be recycled.
                        let _ = groups::OWNED.release(group, None);
                    }
                    return Err(error.into());
                }
            }
            if signal.recv().await.is_none() {
                return Err(io::Error::other("child-exit signal stream closed"));
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) async fn exited(&mut self) -> io::Result<()> {
        self.child.wait().await.map(|_| ())
    }

    /// Called only after `exited()` AND both drains finish. Disarm (and release
    /// the group process-wide) immediately before synchronous reaping, with no
    /// cancellation point in between.
    pub(super) fn finish(&mut self) -> io::Result<ExitStatus> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(group) = self.group.take() {
            let _ = groups::OWNED.release(group, None);
        }
        self.child
            .try_wait()?
            .ok_or_else(|| io::Error::other("observed child exit was not waitable"))
    }
}

/// Spawn `command` and name its dedicated group: a child without one is
/// killed, never kept.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn grouped(command: &mut Command) -> io::Result<(Child, Pid)> {
    let child = command.spawn()?;
    let Some(group) = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .filter(|id| *id > 1)
        .and_then(Pid::from_raw)
    else {
        let mut child = child;
        let _ = child.start_kill();
        return Err(io::Error::other(
            "spawned child has no usable process-group identity",
        ));
    };
    Ok((child, group))
}

impl Drop for Process {
    fn drop(&mut self) {
        // Only while this process still owns the group: a forced end that took
        // it may have reaped its leader already.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(group) = self.group.take()
            && let Some(Err(error)) = groups::OWNED.release(group, Some(Signal::KILL))
            && error != rustix::io::Errno::SRCH
        {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "nika exec: process-group termination could not be requested: {error}"
            );
        }
        // Tokio's reaper follows. On Linux/macOS its kill_on_drop is disabled:
        // this guard is the sole signal owner, including after ECHILD disarms it.
    }
}

// ─── a forced end of every owned group ──────────────────────────────────

/// What a forced end of the owned process groups came to ([`terminate_owned_groups`]). It counts
/// processes ended, never effects undone: nothing a process did is rolled back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Terminated {
    ended: usize,
    killed: usize,
    unconfirmed: Vec<i32>,
    spared: usize,
    untracked: bool,
}

impl Terminated {
    /// The groups seen to end: each one's leader reaped, then no process left in it.
    #[must_use]
    pub const fn ended(&self) -> usize {
        self.ended
    }

    /// Of the groups signalled, those whose leader still ran after the grace, so SIGKILL ended it.
    #[must_use]
    pub const fn killed(&self) -> usize {
        self.killed
    }

    /// The groups signalled whose end was not seen within the wait (a process stuck in the
    /// kernel, or one that left its group), by number: they may still run.
    pub fn unconfirmed(&self) -> impl Iterator<Item = i32> + '_ {
        self.unconfirmed.iter().copied()
    }

    /// The groups never signalled because this process could no longer wait for their leader:
    /// their number may name another group now, and their processes may still run.
    #[must_use]
    pub const fn spared(&self) -> usize {
        self.spared
    }

    /// Whether every process group this process owned was seen to end; never on a platform
    /// without dedicated groups, where none is tracked.
    #[must_use]
    pub fn complete(&self) -> bool {
        !self.untracked && self.spared == 0 && self.unconfirmed.is_empty()
    }
}

impl std::fmt::Display for Terminated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let noun = |n: usize| if n == 1 { "group" } else { "groups" };
        if self.untracked {
            f.write_str(
                "this platform tracks no exec process group: one still running was not ended",
            )?;
        } else if self.ended == 0 && self.unconfirmed.is_empty() && self.spared == 0 {
            f.write_str("no exec process was running")?;
        } else {
            write!(f, "{} exec process {} ended", self.ended, noun(self.ended))?;
            if self.killed > 0 {
                write!(f, " ({} after SIGKILL)", self.killed)?;
            }
            for group in &self.unconfirmed {
                write!(
                    f,
                    " · process group {group} did not confirm its end and may still run"
                )?;
            }
            if self.spared > 0 {
                let spared = self.spared;
                write!(
                    f,
                    " · {spared} exec process {} could not be signalled safely and may still run",
                    noun(spared)
                )?;
            }
        }
        f.write_str(
            " · nothing is rolled back: what already ran stays done, \
             and an effect in flight has an unknown outcome",
        )
    }
}

/// End every process group this process's spawns still own, for a process about to exit without
/// running its destructors (the CLI's second Ctrl-C): SIGTERM to each group whose leader
/// is still this process's child, up to `grace` for those leaders to exit, SIGKILL to every one of
/// those groups (an exited leader can leave members behind), then up to `grace` again to reap each
/// leader and see its group empty. A group whose leader this process can no longer wait for is
/// never signalled: its number may name another group. Nothing spawns afterwards, and `grace`
/// counts at most one minute.
///
/// `then` receives the report while every group stays taken: an owner whose group ended cannot
/// go on (reap it, report its end) before `then` returns, so a process that exits in `then` lets
/// none of them run on past the end it reports.
///
/// Blocking, for a thread of its own, never an executor's. It ends processes and undoes nothing
/// they did; a descendant that left its group (setsid, setpgid) is outside it.
pub fn terminate_owned_groups(grace: std::time::Duration, then: impl FnOnce(&Terminated)) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    groups::OWNED.terminate(grace, then);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = grace;
        then(&Terminated {
            untracked: true,
            ..Terminated::default()
        });
    }
}

// ─── the helper side: one request on this process's own stdio ───────────

/// Exit codes of [`serve_stdio`]: its CPU limit could not be set (or this platform sets none),
/// or `serve` failed.
pub const SERVE_NO_CPU_LIMIT: u8 = 71;
/// See [`SERVE_NO_CPU_LIMIT`].
pub const SERVE_FAILED: u8 = 72;

/// Run `serve` over this process's stdin and stdout once its CPU time is limited to
/// `cpu_seconds`, for a helper the engine starts and collects with [`collect`]. The limit is set
/// before `serve` runs; a limit that cannot be set ends the helper before any work.
///
/// Returns the process exit code: 0 when `serve` succeeded.
#[must_use]
pub fn serve_stdio(
    cpu_seconds: u64,
    serve: impl FnOnce(&mut dyn io::Read, &mut dyn io::Write) -> io::Result<()>,
) -> u8 {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let cpu = rustix::process::Rlimit {
            current: Some(cpu_seconds),
            maximum: Some(cpu_seconds),
        };
        if rustix::process::setrlimit(rustix::process::Resource::Cpu, cpu).is_err() {
            return SERVE_NO_CPU_LIMIT;
        }
        let (mut input, mut output) = (io::stdin().lock(), io::stdout().lock());
        match serve(&mut input, &mut output) {
            Ok(()) => 0,
            Err(_) => SERVE_FAILED,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (cpu_seconds, serve);
        SERVE_NO_CPU_LIMIT
    }
}

// ─── one bounded collection, acknowledged ───────────────────────────────
//
// For a helper process the engine itself starts (never a workflow's
// command): its stdin is written, its two outputs drained under byte caps,
// and its end observed. A deadline or a cap overflow requests the group's
// SIGKILL while the unreaped leader still reserves the group, then keeps
// joining the SAME drains and the same exit observation: nothing is dropped
// mid-flight, and the leader is reaped by `finish()` before any result.

/// The byte caps of one collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Caps {
    /// The stdout bytes kept; one byte more ends the process.
    pub stdout: usize,
    /// The stderr bytes kept; one byte more ends the process.
    pub stderr: usize,
    /// How long the drains may still take once the group was killed.
    pub grace: std::time::Duration,
}

impl Caps {
    /// Caps of `stdout` and `stderr` bytes, with `grace` after a kill.
    #[must_use]
    pub const fn new(stdout: usize, stderr: usize, grace: std::time::Duration) -> Self {
        Self {
            stdout,
            stderr,
            grace,
        }
    }
}

/// Why a collection ended the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ended {
    /// The deadline passed first.
    Deadline,
    /// The process wrote more stdout than its cap.
    Stdout,
    /// The process wrote more stderr than its cap.
    Stderr,
}

/// What one collection observed.
#[derive(Debug)]
#[non_exhaustive]
pub enum Collected {
    /// The process exited on its own, within its caps and before the deadline, and was reaped.
    Exited {
        /// Its exit status.
        status: ExitStatus,
        /// Its whole stdout.
        stdout: Vec<u8>,
        /// Its whole stderr.
        stderr: Vec<u8>,
    },
    /// The collection killed the process group, drained both outputs to their end and reaped it.
    Killed {
        /// Why.
        ended: Ended,
        /// The status the reaped leader reported.
        status: ExitStatus,
    },
    /// The group was killed but its outputs did not end within the grace: the leader is not
    /// reaped by this collection and no cleanup is claimed.
    Abandoned {
        /// Why the group was killed.
        ended: Ended,
    },
}

/// One bounded collection at a time, for a host that may ask for several at once (one per tool
/// call of a run): a second one waits for the first, within its own deadline.
#[derive(Debug)]
#[non_exhaustive]
pub struct Lane {
    slot: tokio::sync::Semaphore,
}

impl Lane {
    /// A lane with no collection in it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: tokio::sync::Semaphore::new(1),
        }
    }

    /// [`collect`] once the lane is free. Waiting counts against `deadline`: a lane still busy
    /// then starts nothing.
    ///
    /// # Errors
    /// As [`collect`]; `TimedOut` when the lane stayed busy until `deadline`.
    pub async fn collect(
        &self,
        program: &std::path::Path,
        args: &[&str],
        stdin: &[u8],
        caps: Caps,
        deadline: std::time::Instant,
    ) -> io::Result<Collected> {
        let at = tokio::time::Instant::from_std(deadline);
        let _held = match tokio::time::timeout_at(at, self.slot.acquire()).await {
            Ok(Ok(held)) => held,
            Ok(Err(_)) => return Err(io::Error::other("the collection lane is closed")),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the collection lane stayed busy until the deadline",
                ));
            }
        };
        collect(program, args, stdin, caps, deadline).await
    }
}

impl Default for Lane {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `program` with `args`, an empty environment and `stdin`, its outputs capped by `caps`,
/// until `deadline`: see [`Collected`].
///
/// CANCEL SAFETY: dropping the future drops the owned process, whose guard requests the group's
/// termination (the crate's spawn law); no acknowledgement is implied then.
///
/// # Errors
/// The spawn failed, the process has no usable group identity, an output could not be read,
/// or this platform has no process-group identity (anything but Linux and macOS).
pub async fn collect(
    program: &std::path::Path,
    args: &[&str],
    stdin: &[u8],
    caps: Caps,
    deadline: std::time::Instant,
) -> io::Result<Collected> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut command = Command::new(program);
        command.args(args).env_clear();
        let deadline = tokio::time::Instant::from_std(deadline);
        Box::pin(collect_grouped(command, stdin, caps, deadline)).await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (program, args, stdin, caps, deadline);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a bounded collection needs a process-group identity",
        ))
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn collect_grouped(
    mut command: Command,
    stdin: &[u8],
    caps: Caps,
    deadline: tokio::time::Instant,
) -> io::Result<Collected> {
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use tokio::io::AsyncWriteExt as _;

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = Process::spawn(&mut command)?;
    let group = process
        .group
        .ok_or_else(|| io::Error::other("spawned child has no process group"))?;
    let (input, out, err) = (
        process.child.stdin.take(),
        process.child.stdout.take(),
        process.child.stderr.take(),
    );
    // 0 = running · 1 deadline · 2 stdout · 3 stderr: the first trigger wins.
    let ended = AtomicU8::new(0);
    let released = AtomicBool::new(false);
    let overflow = tokio::sync::Notify::new();
    let trip = |why: u8| {
        if ended
            .compare_exchange(0, why, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            overflow.notify_one();
        }
    };
    let feed = async move {
        if let Some(mut input) = input {
            // A write to an exited process is benign: its end rides the exit observation.
            let _ = input.write_all(stdin).await;
        }
    };
    let observed = async {
        let exit = process.exited().await;
        if exit.is_err() {
            released.store(true, Ordering::SeqCst);
        }
        exit
    };
    // The joined work borrows the process: it ends inside this block, before any reaping.
    let (joined, why) = {
        let work = async {
            tokio::join!(
                observed,
                capped(out, caps.stdout, || trip(2)),
                capped(err, caps.stderr, || trip(3)),
                feed,
            )
        };
        tokio::pin!(work);
        let early = tokio::select! {
            biased;
            joined = &mut work => Some(joined),
            () = tokio::time::sleep_until(deadline) => { trip(1); None }
            () = overflow.notified() => None,
        };
        match (early, ended.load(Ordering::SeqCst)) {
            (Some(joined), 0) => (Some(joined), None),
            (early, why) => {
                // The leader is unreaped, so the group number still names this group
                // (unless a forced end took it: then this never signals it).
                if !released.load(Ordering::SeqCst) {
                    let _ = groups::OWNED.signal(group, Signal::KILL);
                }
                let why = match why {
                    2 => Ended::Stdout,
                    3 => Ended::Stderr,
                    _ => Ended::Deadline,
                };
                let joined = match early {
                    Some(joined) => Some(joined),
                    None => tokio::time::timeout(caps.grace, &mut work).await.ok(),
                };
                (joined, Some(why))
            }
        }
    };
    settled(&mut process, joined, why)
}

/// What one collection's joined work comes back with: the exit observation, both drains, the feed.
#[cfg(any(target_os = "linux", target_os = "macos"))]
type Joined = (io::Result<()>, io::Result<Vec<u8>>, io::Result<Vec<u8>>, ());

/// The collection's answer: abandoned when the work never joined, otherwise the leader reaped
/// by `finish()` (its exit and both drains are over) and reported killed or exited.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn settled(
    process: &mut Process,
    joined: Option<Joined>,
    why: Option<Ended>,
) -> io::Result<Collected> {
    let Some((exit, stdout, stderr, ())) = joined else {
        // `why` is set whenever the work was not joined.
        let ended = why.unwrap_or(Ended::Deadline);
        return Ok(Collected::Abandoned { ended });
    };
    exit?;
    let (stdout, stderr) = (stdout?, stderr?);
    let status = process.finish()?;
    Ok(match why {
        Some(ended) => Collected::Killed { ended, status },
        None => Collected::Exited {
            status,
            stdout,
            stderr,
        },
    })
}

/// Read `handle` to its end, keeping at most `cap` bytes; the first byte past it calls
/// `overflow` once, and the rest is read and discarded so the writer never blocks.
#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn capped<R: tokio::io::AsyncRead + Unpin>(
    handle: Option<R>,
    cap: usize,
    overflow: impl Fn(),
) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt as _;
    let Some(mut handle) = handle else {
        return Ok(Vec::new());
    };
    let (mut kept, mut chunk, mut over) = (Vec::new(), vec![0_u8; 8192], false);
    loop {
        let read = handle.read(&mut chunk).await?;
        if read == 0 {
            return Ok(kept);
        }
        let room = cap.saturating_sub(kept.len());
        kept.extend_from_slice(&chunk[..read.min(room)]);
        if read > room && !over {
            over = true;
            overflow();
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
