// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An owned process held inside task `b`, independently of journal timing.

use std::fs::File;
use std::io::{self, BufRead as _, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{OFlag, open};
use nix::sys::signal::{Signal, kill};
use nix::sys::stat::Mode;
use nix::unistd::{Pid, mkfifo};

/// The declared directory grant admits the FIFO on both Unix sandboxes;
/// an exact special-file grant would be refused by Seatbelt.
pub(crate) const WORKFLOW: &str = "nika: held-cancel-probe
permits:
  tools: [\"nika:jq\"]
  exec: [\"cat\"]
  fs: { read: [\"./gate/**\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    exec: { command: [\"cat\", \"./gate/release\"] }
  c:
    with: { prev: \"${{ tasks.b.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: 2, expression: \".\" } }
";

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

type Reader = JoinHandle<io::Result<String>>;

pub(crate) struct HeldRun {
    child: Child,
    fifo: PathBuf,
    writer: Option<File>,
    stdout: Option<Reader>,
    stderr: Option<Reader>,
    stderr_lines: Receiver<String>,
    cancelling: bool,
}

impl HeldRun {
    /// The caller owns the workflow and its surrounding temporary directory.
    /// Returning proves that `cat` has opened the FIFO for reading: a
    /// nonblocking writer cannot open a FIFO without an engaged reader.
    pub(crate) fn spawn(mut command: Command, work: &Path) -> Self {
        let gate = work.join("gate");
        std::fs::create_dir(&gate).expect("create the owned gate directory");
        let fifo = gate.join("release");
        mkfifo(&fifo, Mode::S_IRUSR | Mode::S_IWUSR).expect("create the owned FIFO");
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the held run");
        let (send, stderr_lines) = mpsc::channel();
        let mut held = Self {
            child,
            fifo,
            writer: None,
            stdout: None,
            stderr: None,
            stderr_lines,
            cancelling: false,
        };
        let mut stdout = held.child.stdout.take().expect("piped stdout");
        held.stdout = Some(
            std::thread::Builder::new()
                .name("held-run-stdout".to_owned())
                .spawn(move || {
                    let mut text = String::new();
                    stdout.read_to_string(&mut text)?;
                    Ok(text)
                })
                .expect("start stdout reader"),
        );
        let stderr = held.child.stderr.take().expect("piped stderr");
        held.stderr = Some(
            std::thread::Builder::new()
                .name("held-run-stderr".to_owned())
                .spawn(move || {
                    let mut reader = BufReader::new(stderr);
                    let mut text = String::new();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line)? == 0 {
                            return Ok(text);
                        }
                        text.push_str(&line);
                        let _ = send.send(line);
                    }
                })
                .expect("start stderr reader"),
        );
        held.wait_for_reader();
        held
    }

    fn wait_for_reader(&mut self) {
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            match open(
                &self.fifo,
                OFlag::O_WRONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => {
                    self.writer = Some(File::from(fd));
                    return;
                }
                Err(Errno::ENXIO | Errno::EINTR) => {}
                Err(error) => panic!("cannot open the owned FIFO: {error}"),
            }
            if let Some(status) = self.child.try_wait().expect("child status") {
                let stdout = join_reader(self.stdout.take());
                let stderr = join_reader(self.stderr.take());
                panic!(
                    "the run exited before task b engaged the FIFO: {status}\n{stdout}\n{stderr}"
                );
            }
            assert!(Instant::now() < deadline, "task b never engaged the FIFO");
            // Only pace observation; successful open, never elapsed time,
            // establishes that b is in flight.
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    pub(crate) fn signal(&self, signal: Signal) {
        let pid = Pid::from_raw(i32::try_from(self.child.id()).expect("owned child PID"));
        kill(pid, signal).expect("signal only the owned Nika process");
    }

    /// The listener writes this line only after cancelling the run's token.
    pub(crate) fn wait_cancelling(&mut self) {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut seen = String::new();
        loop {
            let line = self
                .stderr_lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| panic!("no cancellation acknowledgement: {error}\n{seen}"));
            seen.push_str(&line);
            if line.starts_with("nika run: cancelling") {
                self.cancelling = true;
                return;
            }
        }
    }

    /// Release b for a graceful cancellation; keep it held until Nika exits
    /// when the caller is testing an immediate second-signal abort.
    pub(crate) fn finish(mut self, release: bool) -> (i32, String, String) {
        if release {
            assert!(
                self.cancelling,
                "acknowledge cancellation before releasing b"
            );
            self.writer.take();
        }
        let status = self.wait_for_exit();
        self.writer.take();
        let stdout = join_reader(self.stdout.take());
        let stderr = join_reader(self.stderr.take());
        (status.code().unwrap_or(-1), stdout, stderr)
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + EXIT_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                return status;
            }
            assert!(Instant::now() < deadline, "the held run did not exit");
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

fn join_reader(reader: Option<Reader>) -> String {
    reader
        .expect("owned reader")
        .join()
        .expect("reader thread did not panic")
        .expect("read the complete child output")
}

impl Drop for HeldRun {
    fn drop(&mut self) {
        // EOF also releases cat when Nika exits without running destructors.
        self.writer.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // A failed startup may leave a reader waiting in open(), before our
        // writer was acquired. Complete that handshake, then deliver EOF.
        let _ = open(
            &self.fifo,
            OFlag::O_RDWR | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        );
        for reader in [self.stdout.take(), self.stderr.take()]
            .into_iter()
            .flatten()
        {
            let _ = reader.join();
        }
    }
}
