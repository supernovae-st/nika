// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The process groups this process owns, for a forced end.
//!
//! A spawn owns its dedicated group from the spawn until its owner releases it, and every owner
//! releases before the leader is reaped. While a group is listed, its unreaped leader therefore
//! reserves the group's number, which still names that very group. A forced end holds the list's
//! lock from its first signal to its last wait, so no owner can reap a leader (and free its
//! number for reuse) meanwhile. A listed leader this process can no longer wait for (an outside
//! reaper took it) is spared, never signalled, and nothing spawns once a forced end began.

use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rustix::io::Errno;
use rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process_group, test_kill_process_group, waitid,
};

use super::Terminated;

/// The groups this process's spawns own.
pub(super) static OWNED: Groups = Groups::new();

/// How long a forced end waits between two looks at what it ends.
const LOOK: Duration = Duration::from_millis(5);

/// The longest grace a forced end grants, whatever it is asked.
const MOST: Duration = Duration::from_secs(60);

/// A list of owned process groups, each named by its leader (a group's number is its leader's
/// pid).
pub(super) struct Groups(Mutex<Owned>);

struct Owned {
    groups: Vec<Pid>,
    ending: bool,
}

impl Groups {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(Owned {
            groups: Vec::new(),
            ending: false,
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Owned> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Run `spawn` under the list's lock and own the group it names: a forced end that began
    /// first lets nothing new start, and one that begins after finds the group listed.
    ///
    /// # Errors
    /// A forced end began (nothing was spawned), or `spawn` failed.
    pub(super) fn spawn<T>(
        &self,
        spawn: impl FnOnce() -> io::Result<(T, Pid)>,
    ) -> io::Result<(T, Pid)> {
        let mut owned = self.lock();
        if owned.ending {
            return Err(io::Error::other(
                "this process is ending: no new process starts",
            ));
        }
        let (child, group) = spawn()?;
        owned.groups.push(group);
        Ok((child, group))
    }

    /// Stop owning `group`, first sending it `signal` while it is still owned: a group a forced
    /// end took is never signalled again. `None` when the group was no longer owned.
    pub(super) fn release(
        &self,
        group: Pid,
        signal: Option<Signal>,
    ) -> Option<rustix::io::Result<()>> {
        let mut owned = self.lock();
        let at = owned.groups.iter().position(|listed| *listed == group)?;
        let sent = signal.map_or(Ok(()), |signal| kill_process_group(group, signal));
        owned.groups.swap_remove(at);
        Some(sent)
    }

    /// Send `signal` to `group` while it is still owned. `None` when it no longer is.
    pub(super) fn signal(&self, group: Pid, signal: Signal) -> Option<rustix::io::Result<()>> {
        let owned = self.lock();
        (owned.groups.contains(&group)).then(|| kill_process_group(group, signal))
    }

    /// End every owned group, as [`super::terminate_owned_groups`] says, then hand `then` the
    /// report before the list's lock is released; nothing spawns after.
    pub(super) fn terminate(&self, grace: Duration, then: impl FnOnce(&Terminated)) {
        // Held to the end, `then` included: no owner reaps a leader while its group may still be
        // signalled, nor goes on past a group's end before `then` returns.
        let mut owned = self.lock();
        let ended = end(&mut owned, grace);
        then(&ended);
    }
}

/// The forced end itself, under the list's lock: the report of every group it took.
fn end(owned: &mut Owned, grace: Duration) -> Terminated {
    owned.ending = true;
    let mut ended = Terminated::default();
    let mut ours = Vec::new();
    for group in std::mem::take(&mut owned.groups) {
        match leader(group, WaitIdOptions::NOWAIT) {
            Leader::Gone => ended.spared += 1,
            Leader::Running | Leader::Exited => ours.push(group),
        }
    }
    for group in &ours {
        // ESRCH: the group holds no process any more (only its unreaped leader, on macOS).
        let _ = kill_process_group(*group, Signal::TERM);
    }
    let running = |group: &Pid| leader(*group, WaitIdOptions::NOWAIT) == Leader::Running;
    let deadline = after(grace);
    while ours.iter().any(running) && Instant::now() < deadline {
        std::thread::sleep(LOOK);
    }
    ended.killed = ours.iter().filter(|&group| running(group)).count();
    // An exited leader can leave members behind: every group still owned is killed.
    for group in &ours {
        let _ = kill_process_group(*group, Signal::KILL);
    }
    let deadline = after(grace);
    for group in ours {
        if reaped(group, deadline) && emptied(group, deadline) {
            ended.ended += 1;
        } else {
            ended.unconfirmed.push(group.as_raw_nonzero().get());
        }
    }
    ended
}

/// `grace` from now, at most [`MOST`]; an instant the clock cannot represent waits none.
fn after(grace: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(grace.min(MOST)).unwrap_or(now)
}

/// Where a group's leader stands, for this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leader {
    /// It runs.
    Running,
    /// It exited: unreaped, it still reserves its group's number (reaped by a wait without
    /// `NOWAIT`).
    Exited,
    /// This process can no longer wait for it (ECHILD: an outside reaper took it): its group's
    /// number may already name another group.
    Gone,
}

/// The leader of `group` (its own pid), by one non-blocking wait for its exit with `options`
/// added: `NOWAIT` leaves an exited leader unreaped, no option reaps it.
fn leader(group: Pid, options: WaitIdOptions) -> Leader {
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | options;
    loop {
        match waitid(WaitId::Pid(group), options) {
            Ok(None) => return Leader::Running,
            Ok(Some(_)) => return Leader::Exited,
            Err(Errno::INTR) => {}
            // ECHILD above all; any refusal leaves the same doubt and is never signalled.
            Err(_) => return Leader::Gone,
        }
    }
}

/// Wait until `deadline` for the leader of `group` to exit, and reap it: `true` once reaped, or
/// once an outside reaper took it.
fn reaped(group: Pid, deadline: Instant) -> bool {
    loop {
        match leader(group, WaitIdOptions::empty()) {
            Leader::Exited | Leader::Gone => return true,
            Leader::Running if Instant::now() >= deadline => return false,
            Leader::Running => std::thread::sleep(LOOK),
        }
    }
}

/// Wait until `deadline` for `group` to hold no process. Its leader is reaped by now, so its
/// number may name another group: only the null signal is sent, which disturbs none.
fn emptied(group: Pid, deadline: Instant) -> bool {
    loop {
        match test_kill_process_group(group) {
            Err(Errno::SRCH) => return true,
            _ if Instant::now() >= deadline => return false,
            _ => std::thread::sleep(LOOK),
        }
    }
}

#[cfg(test)]
mod tests;
