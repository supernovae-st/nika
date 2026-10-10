// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The forced end's laws over real processes, each on a list of its own: a live group ends and is
//! seen empty, a leader that ignores SIGTERM is killed after the grace, a member an exited leader
//! left behind ends too, a leader this process can no longer wait for is spared (the shape of a
//! reaped leader whose number another process took), a released group is never signalled, and
//! nothing spawns once the end began. The process-wide list is never ended here: the other tests
//! of this binary spawn through it.
#![allow(clippy::disallowed_types)]

use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::process::{getpgid, test_kill_process};

use super::*;

/// A directory of markers, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nika-groups-{name}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }

    /// The pid a fixture wrote to `name`, once it wrote it.
    fn pid(&self, name: &str) -> Result<Pid, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let written = std::fs::read_to_string(self.0.join(name)).unwrap_or_default();
            if let Some(pid) = written.trim().parse().ok().and_then(Pid::from_raw) {
                return Ok(pid);
            }
            if Instant::now() >= deadline {
                return Err(format!("the fixture never wrote {name}"));
            }
            std::thread::sleep(LOOK);
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `script` under `/bin/sh` in a group of its own, owned by `groups`.
fn spawned(groups: &Groups, script: &str) -> Result<(Child, Pid), String> {
    groups
        .spawn(|| {
            let child = Command::new("/bin/sh")
                .args(["-c", script])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let group = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
            Ok((child, group.ok_or_else(|| io::Error::other("no pid"))?))
        })
        .map_err(|error| error.to_string())
}

/// The forced end of `groups`, as its `then` is handed the report.
fn forced_end(groups: &Groups, grace: Duration) -> Terminated {
    let mut report = None;
    groups.terminate(grace, |ended| report = Some(ended.clone()));
    report.unwrap_or_default()
}

/// Ended, as the kernel says: no process by that pid any more.
fn gone(pid: Pid) -> bool {
    matches!(test_kill_process(pid), Err(Errno::SRCH))
}

#[test]
fn a_live_group_ends_on_sigterm_and_is_seen_empty() -> Result<(), String> {
    let groups = Groups::new();
    let scratch = Scratch::new("live")?;
    let script = format!("sleep 30 & echo $! > '{}'; wait", scratch.path("member"));
    let (_leader, _) = spawned(&groups, &script)?;
    let member = scratch.pid("member")?;
    let ended = forced_end(&groups, Duration::from_secs(5));
    assert_eq!(
        (ended.ended(), ended.killed(), ended.spared()),
        (1, 0, 0),
        "{ended:?}"
    );
    assert_eq!(ended.unconfirmed().count(), 0, "{ended:?}");
    assert!(ended.complete(), "{ended:?}");
    assert!(gone(member), "the member ended with its leader");
    Ok(())
}

#[test]
fn a_leader_that_ignores_sigterm_is_killed_after_the_grace() -> Result<(), String> {
    let groups = Groups::new();
    let scratch = Scratch::new("deaf")?;
    let script = format!(
        "trap '' TERM; echo $$ > '{}'; while :; do sleep 1; done",
        scratch.path("ready")
    );
    let (_leader, group) = spawned(&groups, &script)?;
    // Written after the trap: SIGTERM is ignored from here on.
    assert_eq!(scratch.pid("ready")?, group);
    let ended = forced_end(&groups, Duration::from_millis(300));
    assert_eq!(
        (ended.ended(), ended.killed(), ended.spared()),
        (1, 1, 0),
        "{ended:?}"
    );
    assert!(gone(group), "SIGKILL ended the leader and it was reaped");
    Ok(())
}

#[test]
fn a_member_an_exited_leader_left_behind_ends_too() -> Result<(), String> {
    let groups = Groups::new();
    let scratch = Scratch::new("left")?;
    // The leader exits at once; its member ignores SIGTERM and keeps the group alive.
    let script = format!(
        "(trap '' TERM; exec sleep 30) > /dev/null 2>&1 & echo $! > '{}'",
        scratch.path("member")
    );
    let (_leader, group) = spawned(&groups, &script)?;
    let member = scratch.pid("member")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while leader(group, WaitIdOptions::NOWAIT) != Leader::Exited {
        assert!(Instant::now() < deadline, "the leader never exited");
        std::thread::sleep(LOOK);
    }
    let ended = forced_end(&groups, Duration::from_millis(300));
    assert_eq!((ended.ended(), ended.spared()), (1, 0), "{ended:?}");
    assert!(gone(member), "SIGKILL reached the member left behind");
    Ok(())
}

/// Ends a process this test started, whatever the assertions said.
struct Stranger(Pid);

impl Drop for Stranger {
    fn drop(&mut self) {
        let _ = kill_process_group(self.0, Signal::KILL);
    }
}

#[test]
fn a_leader_this_process_cannot_wait_for_is_spared_never_signalled() -> Result<(), String> {
    // Job control gives the background job a group of its own and its shell exits: the job leads
    // a group this process cannot wait for, the shape of a reaped leader whose number another
    // process took. Listed by mistake, it must never be signalled.
    let output = Command::new("/bin/bash")
        .args(["-c", "set -m; sleep 30 > /dev/null 2>&1 & echo $!"])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    let raw = String::from_utf8_lossy(&output.stdout);
    let pid = raw.trim().parse().ok().and_then(Pid::from_raw);
    let stranger = Stranger(pid.ok_or_else(|| format!("no pid in {raw:?}"))?);
    assert_eq!(
        getpgid(Some(stranger.0)).map_err(|error| error.to_string())?,
        stranger.0,
        "the fixture leads its own group"
    );
    let groups = Groups::new();
    groups.lock().groups.push(stranger.0);
    let ended = forced_end(&groups, Duration::from_millis(300));
    assert_eq!(
        (ended.spared(), ended.ended(), ended.killed()),
        (1, 0, 0),
        "{ended:?}"
    );
    assert!(!ended.complete(), "a spared group may still run");
    assert!(
        test_kill_process(stranger.0).is_ok(),
        "the spared group's process was never signalled"
    );
    Ok(())
}

#[test]
fn a_released_group_is_never_signalled_and_nothing_spawns_after_the_end() -> Result<(), String> {
    let groups = Groups::new();
    let (mut child, group) = spawned(&groups, "exit 0")?;
    // Its owner releases it before the reap, as `finish` does: no signal reaches it after.
    assert!(matches!(groups.release(group, None), Some(Ok(()))));
    assert!(groups.release(group, Some(Signal::KILL)).is_none());
    assert!(groups.signal(group, Signal::KILL).is_none());
    child.wait().map_err(|error| error.to_string())?;
    let ended = forced_end(&groups, Duration::from_millis(300));
    assert_eq!(
        ended,
        Terminated::default(),
        "nothing owned, nothing signalled"
    );
    let mut asked = false;
    let refused = groups.spawn(|| -> io::Result<((), Pid)> {
        asked = true;
        Err(io::Error::other("never asked"))
    });
    assert!(refused.is_err_and(|error| error.to_string().contains("ending")));
    assert!(!asked, "nothing spawns once a forced end began");
    Ok(())
}

#[test]
fn the_report_names_what_may_still_run_and_claims_no_rollback() {
    let tail = " · nothing is rolled back: what already ran stays done, \
                and an effect in flight has an unknown outcome";
    let none = Terminated::default();
    assert_eq!(
        none.to_string(),
        format!("no exec process was running{tail}")
    );
    assert!(none.complete());
    let mixed = Terminated {
        ended: 2,
        killed: 1,
        unconfirmed: vec![4242],
        spared: 1,
        untracked: false,
    };
    assert_eq!(
        mixed.to_string(),
        format!(
            "2 exec process groups ended (1 after SIGKILL) · process group 4242 did not \
             confirm its end and may still run · 1 exec process group could not be \
             signalled safely and may still run{tail}"
        )
    );
    assert_eq!(mixed.unconfirmed().collect::<Vec<_>>(), [4242]);
    assert!(!mixed.complete());
    let untracked = Terminated {
        untracked: true,
        ..Terminated::default()
    };
    assert!(
        untracked
            .to_string()
            .starts_with("this platform tracks no exec process group")
    );
    assert!(!untracked.complete());
}
