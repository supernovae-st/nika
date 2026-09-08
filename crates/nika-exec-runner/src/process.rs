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

use std::io;
use std::process::ExitStatus;
use tokio::process::{Child, Command};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};

pub(super) struct Process {
    pub(super) child: Child,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    group: Option<Pid>,
}

impl Process {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        command.process_group(0).kill_on_drop(false);
        let child = command.spawn()?;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
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
                    if error == rustix::io::Errno::CHILD {
                        // An external reaper took ownership: never signal the
                        // saved group number after its leader may be recycled.
                        self.group = None;
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

    /// Called only after `exited()` AND both drains finish. Disarm immediately
    /// before synchronous reaping, with no cancellation point in between.
    pub(super) fn finish(&mut self) -> io::Result<ExitStatus> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            self.group = None;
        }
        self.child
            .try_wait()?
            .ok_or_else(|| io::Error::other("observed child exit was not waitable"))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(group) = self.group.take()
            && let Err(error) = kill_process_group(group, Signal::KILL)
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

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
