// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::disallowed_types)]
#![cfg(unix)]
//! The second Ctrl-C ends the exec process groups the run owns before the CLI exits.
//! Each exec child leads a process group of its own, so neither a signal to the CLI's pid nor
//! a terminal's Ctrl-C to the CLI's group reaches it; before this, `exit(130)` ran no
//! destructor and the leaf kept running after the CLI was gone.
//!
//! The leaf holds `gate/alive` open for writing for its whole life (`cat` inherits it), marks
//! its start, then waits on `gate/release` before its second marker. After the CLI exits, an
//! end of file on the test's read end of `gate/alive` proves that no process of the leaf is
//! alive, whatever pid namespace a sandbox gave it, and only then is `gate/release` let go:
//! nothing remains to write the second marker. Both paths, the leaf at the root and one level
//! down a composed workflow.

use std::fs::File;
use std::io::{BufRead as _, BufReader, Read as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{OFlag, open};
use nix::sys::signal::{Signal, kill, killpg};
use nix::sys::stat::Mode;
use nix::unistd::{Pid, mkfifo};

const LEAF: &str = "exec 3> ./gate/alive
echo started > ./marks/started
cat ./gate/release
echo done > ./marks/done
";

/// A directory grant admits the FIFOs on both Unix sandboxes (an exact special-file grant is
/// refused by Seatbelt).
const PERMITS: &str = "permits:
  exec: [\"sh\"]
  fs: { read: [\"./gate/**\", \"./leaf.sh\"], write: [\"./gate/**\", \"./marks/**\"] }
";

const LEAF_TASK: &str = "tasks:
  leaf:
    exec: { command: [\"sh\", \"./leaf.sh\"] }
";

const CALL_TASK: &str = "tasks:
  call:
    invoke: { workflow: \"./child.nika\" }
";

const READY: Duration = Duration::from_secs(30);
const LOOK: Duration = Duration::from_millis(10);

/// How the operator's two Ctrl-C reach the CLI.
#[derive(Clone, Copy)]
enum Target {
    /// `kill(pid)`: only the CLI hears it.
    Pid,
    /// `killpg(pgid)`: the terminal's foreground group, the CLI at its head.
    Group,
}

/// Where the leaf runs.
#[derive(Clone, Copy)]
enum Depth {
    Root,
    Nested,
}

fn workflow(id: &str, tasks: &str) -> String {
    format!("nika: {id}\n{PERMITS}{tasks}")
}

fn workflows(depth: Depth) -> Vec<(&'static str, String)> {
    match depth {
        Depth::Root => vec![("wait.nika", workflow("abort-leaf", LEAF_TASK))],
        Depth::Nested => vec![
            ("wait.nika", workflow("abort-parent", CALL_TASK)),
            ("child.nika", workflow("abort-child", LEAF_TASK)),
        ],
    }
}

fn fifo(path: &Path, access: OFlag) -> Result<File, Errno> {
    open(
        path,
        access | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
}

struct Abort {
    _dir: tempfile::TempDir,
    work: PathBuf,
    child: Child,
    alive: File,
    release: Option<File>,
    stdout: Option<JoinHandle<String>>,
    stderr: Receiver<String>,
}

impl Abort {
    /// The run started and its leaf holds `gate/release` open: the leaf is in flight.
    fn start(depth: Depth, target: Target) -> Self {
        let dir = tempfile::tempdir().expect("fixture directory");
        let (work, home) = (dir.path().join("work"), dir.path().join("home"));
        for sub in [work.join("gate"), work.join("marks"), home.clone()] {
            std::fs::create_dir_all(sub).expect("fixture tree");
        }
        std::fs::write(work.join("nika.yaml"), "nika: abort-fixture\n").expect("project");
        std::fs::write(work.join("leaf.sh"), LEAF).expect("leaf");
        for (name, source) in workflows(depth) {
            std::fs::write(work.join(name), source).expect("workflow");
        }
        for name in ["alive", "release"] {
            let path = work.join("gate").join(name);
            mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).expect("fifo");
        }
        // Opened before the run: the leaf's `exec 3>` finds its reader at once.
        let alive = fifo(&work.join("gate/alive"), OFlag::O_RDONLY).expect("alive reader");
        let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
        command
            .args(["run", "wait.nika", "--json", "--max-cost-usd", "0.01"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("TERM", "dumb")
            .env("NIKA_KEYCHAIN", "off")
            .current_dir(&work)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if matches!(target, Target::Group) {
            command.process_group(0);
        }
        let mut child = command.spawn().expect("nika run");
        let mut out = child.stdout.take().expect("stdout");
        let stdout = std::thread::Builder::new()
            .name("abort-stdout".to_owned())
            .spawn(move || {
                let mut text = String::new();
                let _ = out.read_to_string(&mut text);
                text
            })
            .expect("stdout reader");
        let (send, stderr) = mpsc::channel();
        let err = BufReader::new(child.stderr.take().expect("stderr"));
        std::thread::Builder::new()
            .name("abort-stderr".to_owned())
            .spawn(move || {
                for line in err.lines().map_while(Result::ok) {
                    let _ = send.send(line);
                }
            })
            .expect("stderr reader");
        let mut abort = Self {
            _dir: dir,
            work,
            child,
            alive,
            release: None,
            stdout: Some(stdout),
            stderr,
        };
        abort.engage();
        abort
    }

    /// A nonblocking writer opens `gate/release` only once the leaf's `cat` reads it.
    fn engage(&mut self) {
        let deadline = Instant::now() + READY;
        loop {
            match fifo(&self.work.join("gate/release"), OFlag::O_WRONLY) {
                Ok(writer) => {
                    self.release = Some(writer);
                    return;
                }
                Err(Errno::ENXIO | Errno::EINTR) => {}
                Err(error) => panic!("cannot open gate/release: {error}"),
            }
            if let Some(status) = self.child.try_wait().expect("run status") {
                let said: Vec<String> = self.stderr.try_iter().collect();
                panic!("the run ended before its leaf engaged: {status}\n{said:?}");
            }
            assert!(Instant::now() < deadline, "the leaf never engaged");
            std::thread::sleep(LOOK);
        }
    }

    fn signal(&self, target: Target) {
        let pid = Pid::from_raw(i32::try_from(self.child.id()).expect("pid"));
        match target {
            Target::Pid => kill(pid, Signal::SIGINT),
            Target::Group => killpg(pid, Signal::SIGINT),
        }
        .expect("signal only the run this test started");
    }

    /// The next stderr lines, through the one `until` accepts, or to the end of the stream.
    fn read_stderr(&self, until: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + READY;
        let mut seen = String::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.stderr.recv_timeout(left) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if until(&line) {
                        return seen;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return seen,
                Err(RecvTimeoutError::Timeout) => panic!("stderr stalled:\n{seen}"),
            }
        }
    }

    fn wait_exit(&mut self) -> i32 {
        let deadline = Instant::now() + READY;
        loop {
            if let Some(status) = self.child.try_wait().expect("run status") {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < deadline, "the run never exited");
            std::thread::sleep(LOOK);
        }
    }
}

impl Drop for Abort {
    fn drop(&mut self) {
        // Lets a surviving leaf finish (the red case) and the run, if still alive, end.
        self.release.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(stdout) = self.stdout.take() {
            let _ = stdout.join();
        }
    }
}

fn forced_abort_settles_the_leaf(depth: Depth, target: Target) {
    let mut abort = Abort::start(depth, target);
    assert!(
        abort.work.join("marks/started").is_file(),
        "the leaf started"
    );
    abort.signal(target);
    let mut stderr = abort.read_stderr(|line| line.starts_with("nika run: cancelling"));
    assert!(stderr.contains("nika run: cancelling"), "{stderr}");
    abort.signal(target);
    let code = abort.wait_exit();
    // The CLI alone holds its stderr: the stream ends with it.
    stderr.push_str(&abort.read_stderr(|_| false));
    assert_eq!(code, 130, "the cancelled class:\n{stderr}");
    assert!(stderr.contains("nika run: aborted"), "{stderr}");
    // End of file: no process of the leaf holds `gate/alive` any more.
    let mut byte = [0_u8; 1];
    match abort.alive.read(&mut byte) {
        Ok(0) => {}
        other => panic!("a process of the leaf outlived the CLI ({other:?}):\n{stderr}"),
    }
    assert_eq!(
        fifo(&abort.work.join("gate/release"), OFlag::O_WRONLY).err(),
        Some(Errno::ENXIO),
        "no reader of gate/release is left"
    );
    assert!(
        stderr.contains("1 exec process group ended"),
        "the abort names the group it ended:\n{stderr}"
    );
    assert!(stderr.contains("nothing is rolled back"), "{stderr}");
    // Only now is the gate let go: nothing remains to write the second marker.
    abort.release.take();
    assert!(
        !abort.work.join("marks/done").exists(),
        "the leaf never wrote its second marker"
    );
}

#[test]
fn a_forced_abort_by_pid_ends_the_root_leaf() {
    forced_abort_settles_the_leaf(Depth::Root, Target::Pid);
}

#[test]
fn a_forced_abort_from_the_terminal_group_ends_the_root_leaf() {
    forced_abort_settles_the_leaf(Depth::Root, Target::Group);
}

#[test]
fn a_forced_abort_by_pid_ends_a_nested_leaf() {
    forced_abort_settles_the_leaf(Depth::Nested, Target::Pid);
}

#[test]
fn a_forced_abort_from_the_terminal_group_ends_a_nested_leaf() {
    forced_abort_settles_the_leaf(Depth::Nested, Target::Group);
}
