// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Asking another `nika` for its `--version`, contained (#1614).
//!
//! The copy runs with an allow-listed environment that never carries a
//! credential-shaped variable, a PATH rebuilt from the absolute entries
//! alone, and `/` as its working directory: a launcher cannot resolve its
//! interpreter, or read a file, from the directory doctor was started in.
//! It leads its own process group, and the whole group is killed once its
//! first line is read, at the deadline, or after it exits, so nothing it
//! forked outlives the probe.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt as _;

use super::{Answer, parse_version_output};

/// The most stdout read from one answer: a version is one line.
const VERSION_OUTPUT_CAP: usize = 4096;

/// How long a killed copy may take to be reaped before it is left to the
/// runtime's orphan queue.
const REAP_GRACE: Duration = Duration::from_millis(500);

/// The most group kills after the leader is reaped, [`SWEEP_PAUSE`] apart.
const SWEEPS: usize = 40;

/// The pause between two sweeps of the group.
const SWEEP_PAUSE: Duration = Duration::from_millis(5);

/// Variables every probed copy keeps, when they are set: who and where the
/// user is, the locale and the terminal type. None is a secret channel.
const ENV_FLOOR: [&str; 7] = ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "TERM", "SHELL"];

/// Name prefixes that also ride along: the locale, the XDG directories and
/// nika's own settings (a launcher may pick its build by one of them).
const ENV_PREFIXES: [&str; 3] = ["LC_", "XDG_", "NIKA"];

/// A name that carries one of these never crosses, whatever its prefix:
/// deliberately broad, since `--version` needs none of them.
const CREDENTIAL_FRAGMENTS: [&str; 8] = [
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
    "PASSPHRASE",
];

/// The environment a probed copy runs under, from `parent`: the floor and
/// the prefixed names, never a credential-shaped one, and `PATH` rebuilt
/// from the absolute entries of `path_var` (a relative or empty entry
/// would reach the working directory). Pure.
pub(crate) fn probe_env(
    parent: &[(OsString, OsString)],
    path_var: Option<&OsStr>,
) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = parent
        .iter()
        .filter(|(name, _)| name.to_str().is_some_and(crosses))
        .cloned()
        .collect();
    let absolute = path_var.and_then(|p| {
        std::env::join_paths(std::env::split_paths(p).filter(|d| d.is_absolute())).ok()
    });
    if let Some(path) = absolute {
        env.push(("PATH".into(), path));
    }
    env
}

fn crosses(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let listed = ENV_FLOOR.contains(&name) || ENV_PREFIXES.iter().any(|p| name.starts_with(p));
    listed && !CREDENTIAL_FRAGMENTS.iter().any(|f| upper.contains(f))
}

/// Ask each `(index, path)` for its version, concurrently, on a private
/// runtime. A caller already inside a runtime cannot block on one: every
/// copy is then reported as not asked, never a panic.
pub(super) fn ask_all(
    asked: Vec<(usize, PathBuf)>,
    deadline: Duration,
    env: &[(OsString, OsString)],
) -> Vec<(usize, Answer)> {
    if asked.is_empty() {
        return Vec::new();
    }
    let refuse = |asked: Vec<(usize, PathBuf)>, why: &str| -> Vec<(usize, Answer)> {
        asked
            .into_iter()
            .map(|(i, _)| (i, Answer::Failed(why.to_owned())))
            .collect()
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        return refuse(asked, "not asked from inside an async runtime");
    }
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return refuse(asked, "the probe runtime did not start");
    };
    runtime.block_on(async move {
        let mut set = tokio::task::JoinSet::new();
        for (i, path) in asked {
            let env = env.to_vec();
            set.spawn(async move { (i, ask_version(&path, deadline, &env).await) });
        }
        let mut answers = Vec::new();
        while let Some(joined) = set.join_next().await {
            answers.push(joined.unwrap_or_else(|e| (usize::MAX, Answer::Failed(e.to_string()))));
        }
        answers
    })
}

/// How the read of one answer stopped.
enum Stop {
    /// The first non-empty line is complete.
    Line,
    /// The copy closed its stdout.
    Eof,
    /// [`VERSION_OUTPUT_CAP`] bytes and still no complete line.
    Full,
}

/// `<path> --version`, bounded and contained: stdin closed, stderr
/// discarded, at most [`VERSION_OUTPUT_CAP`] bytes of stdout read, the
/// answer taken from its first line, the whole exchange within `deadline`.
///
/// CANCEL SAFETY: dropping this future kills the direct child
/// (`kill_on_drop`); every path that completes kills the whole group.
async fn ask_version(path: &Path, deadline: Duration, env: &[(OsString, OsString)]) -> Answer {
    let until = tokio::time::Instant::now() + deadline;
    // tokio's `Command`, outside the workspace ban on the std one: this
    // probe is async, bounded and contained here, the same class as the
    // harness's adapter version probes.
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--version")
        .env_clear()
        .envs(env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    contain(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return Answer::Failed(format!("could not start: {e}")),
    };
    let group = child.id();
    let Some(mut stdout) = child.stdout.take() else {
        end(&mut child, group).await;
        return Answer::Failed("no stdout".to_owned());
    };
    let mut bytes = Vec::new();
    let read = tokio::time::timeout_at(until, read_answer(&mut stdout, &mut bytes)).await;
    // Closing the pipe ends a copy that keeps writing past its answer.
    drop(stdout);
    let answer = match read {
        Err(_) => Answer::TimedOut,
        Ok(Err(e)) => Answer::Failed(e.to_string()),
        Ok(Ok(Stop::Line | Stop::Eof | Stop::Full)) => {
            match parse_version_output(&String::from_utf8_lossy(&bytes)) {
                // What it does after its version line is not its identity.
                Some(build) => Answer::Reported(build),
                None => no_version(&mut child, until).await,
            }
        }
    };
    end(&mut child, group).await;
    answer
}

/// Read stdout into `bytes` until its first non-empty line is complete,
/// it closes, or the cap is reached.
async fn read_answer(
    stdout: &mut tokio::process::ChildStdout,
    bytes: &mut Vec<u8>,
) -> std::io::Result<Stop> {
    let mut chunk = [0_u8; 512];
    loop {
        let read = stdout.read(&mut chunk).await?;
        if read == 0 {
            return Ok(Stop::Eof);
        }
        let room = VERSION_OUTPUT_CAP.saturating_sub(bytes.len());
        bytes.extend_from_slice(chunk.get(..read.min(room)).unwrap_or_default());
        if first_line_done(bytes) {
            return Ok(Stop::Line);
        }
        if bytes.len() >= VERSION_OUTPUT_CAP {
            return Ok(Stop::Full);
        }
    }
}

/// The first line holding something other than whitespace has ended.
fn first_line_done(bytes: &[u8]) -> bool {
    bytes
        .split(|b| *b == b'\n')
        .rev()
        .skip(1)
        .any(|line| line.iter().any(|b| !b.is_ascii_whitespace()))
}

/// No version line: the exit status says why, when it comes in time.
async fn no_version(child: &mut tokio::process::Child, until: tokio::time::Instant) -> Answer {
    match tokio::time::timeout_at(until, child.wait()).await {
        Ok(Ok(status)) if !status.success() => Answer::Failed(status.to_string()),
        Ok(Err(e)) => Answer::Failed(e.to_string()),
        _ => Answer::Failed("printed no `nika <version>` line".to_owned()),
    }
}

/// Run the copy from `/`, as the leader of its own process group: a
/// background group, so a read of the terminal stops it instead of
/// taking the person's keystrokes.
#[cfg(unix)]
fn contain(command: &mut tokio::process::Command) {
    command.current_dir("/").process_group(0);
}

#[cfg(not(unix))]
fn contain(_command: &mut tokio::process::Command) {}

/// End the copy and everything it forked: the group first, then reap the
/// leader, then sweep the group until it is empty. A process forked while
/// the first kill was being delivered can miss it (the kill lands right as
/// a copy forks after its answer), but it still carries the group id, and
/// a group with a member cannot name another one. While the leader is
/// unreaped its id cannot be reused either; once the group is empty the
/// sweep stops at the first refusal, the harness probes' same best effort.
async fn end(child: &mut tokio::process::Child, group: Option<u32>) {
    kill_group(group);
    let _ = child.start_kill();
    let _ = tokio::time::timeout(REAP_GRACE, child.wait()).await;
    for _ in 0..SWEEPS {
        if !kill_group(group) {
            return;
        }
        tokio::time::sleep(SWEEP_PAUSE).await;
    }
}

/// Kill the whole group `group`: `true` when the group still had a member.
#[cfg(unix)]
fn kill_group(group: Option<u32>) -> bool {
    let Some(pid) = group.and_then(|g| i32::try_from(g).ok()).filter(|p| *p > 1) else {
        return false;
    };
    nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    )
    .is_ok()
}

#[cfg(not(unix))]
fn kill_group(_group: Option<u32>) -> bool {
    false
}
