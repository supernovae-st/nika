// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A Session Stop reaches the Run it handed to its native door, the real binary: the Stop is the
//! Run's first signal, so the task in flight completes and is counted, the next one is
//! cancelled, the Run seals one `workflow_cancelled` terminal and its trace verifies. The result
//! says the Run stopped (sealed), never aborted, and the same Stop replayed sends nothing more.
//! Keyless: an isolated HOME and an empty environment; the workflow runs no model.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]
#![cfg(unix)]

use std::fs::File;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{OFlag, open};
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use serde_json::{Value, json};

/// Task `b` holds on the FIFO until the test lets it go; `c` waits for `b`.
const HELD: &str = "nika: held-session-stop
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

const WAIT: Duration = Duration::from_secs(60);

fn submit(command: &str, snapshot: &Value, line: &str) -> String {
    json!({
        "contract": "nika/session-host@1", "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
    .to_string()
}

fn stop(command: &str) -> String {
    json!({"contract": "nika/session-host@1", "op": "stop", "command": command}).to_string()
}

fn kinds(result: &Value) -> Vec<&str> {
    (result["outcomes"].as_array().expect("outcomes").iter())
        .map(|outcome| outcome["kind"].as_str().expect("kind"))
        .collect()
}

/// The next frame `wanted` accepts; every frame seen on the way is kept for the failure message.
fn next(frames: &Receiver<Value>, seen: &mut Vec<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let frame = frames
            .recv_timeout(left)
            .unwrap_or_else(|error| panic!("no awaited frame ({error}) after {seen:#?}"));
        seen.push(frame.clone());
        if wanted(&frame) {
            return frame;
        }
    }
}

fn result_of(command: &str) -> impl Fn(&Value) -> bool + '_ {
    move |frame| frame["frame"] == "result" && frame["command"] == command
}

/// A nonblocking writer opens the FIFO only once task `b`'s `cat` reads it.
fn engaged(fifo: &Path, frames: &Receiver<Value>, seen: &mut Vec<Value>) -> File {
    let deadline = Instant::now() + WAIT;
    loop {
        let flags = OFlag::O_WRONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
        match open(fifo, flags, Mode::empty()) {
            Ok(fd) => return File::from(fd),
            Err(Errno::ENXIO | Errno::EINTR) => {}
            Err(error) => panic!("cannot open the gate: {error}"),
        }
        seen.extend(frames.try_iter());
        assert!(
            !seen.iter().any(result_of("c-1")),
            "the run settled before task b held: {seen:#?}"
        );
        assert!(Instant::now() < deadline, "task b never held: {seen:#?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `nika session --json` in `root`, keyless, and the frames it writes (one JSON object a line).
fn session(root: &Path, home: &Path) -> (Child, ChildStdin, Receiver<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["session", "--json"])
        .current_dir(root)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_KEYCHAIN", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("nika session --json");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let (send, frames) = mpsc::channel();
    std::thread::Builder::new()
        .name("session-frames".to_owned())
        .spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
                if send.send(frame).is_err() {
                    return;
                }
            }
        })
        .expect("frame reader");
    (child, stdin, frames)
}

/// The run's journal: task `b` completed in flight, `c` was cancelled, one `workflow_cancelled`
/// terminal, and `nika trace verify` accepts the sealed chain.
fn assert_sealed_cancelled(root: &Path, home: &Path, trace: &Path) {
    let journal = std::fs::read_to_string(trace).expect("the run's journal");
    let task_event = |kind: &str, task: &str| {
        journal.lines().any(|line| {
            line.contains(&format!("\"kind\":\"{kind}\""))
                && line.contains(&format!("\"value\":\"{task}\""))
        })
    };
    assert!(
        task_event("task_completed", "b"),
        "in-flight work completes"
    );
    assert!(task_event("task_cancelled", "c"), "no new wave starts");
    assert_eq!(
        journal.matches("\"kind\":\"workflow_cancelled\"").count(),
        1
    );
    let verify = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["trace", "verify"])
        .arg(trace)
        .current_dir(root)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("nika trace verify");
    assert!(
        verify.status.success(),
        "the trace seals on its cancelled terminal:\n{}{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn a_session_stop_reaches_the_run_and_its_trace_seals_cancelled() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let root = project.path();
    std::fs::create_dir(root.join("gate")).expect("gate");
    let fifo = root.join("gate/release");
    mkfifo(&fifo, Mode::S_IRUSR | Mode::S_IWUSR).expect("the gate FIFO");
    std::fs::write(root.join("held.nika"), HELD).expect("workflow");
    let (mut child, mut stdin, frames) = session(root, home.path());
    let mut seen = Vec::new();
    let opened = next(&frames, &mut seen, |frame| frame["frame"] == "opened");
    let line = "run held.nika with a ceiling of 0.01";
    writeln!(
        stdin,
        "{}",
        submit("c-1", &opened["snapshot"]["snapshot"], line)
    )
    .expect("run");
    let writer = engaged(&fifo, &frames, &mut seen);

    writeln!(stdin, "{}", stop("s-1")).expect("stop");
    let receipt = next(&frames, &mut seen, result_of("s-1"));
    assert_eq!(receipt["receipt"], "run_stopping", "{receipt}");
    assert_eq!(receipt["target"], "c-1", "{receipt}");
    assert_eq!(
        receipt["snapshot"]["busy"]["phase"], "stopping",
        "{receipt}"
    );
    assert_eq!(receipt["snapshot"]["busy"]["stop_requested"], true);
    // Task b is in flight: it completes once its gate lets go, then no new wave starts.
    drop(writer);
    let settled = next(&frames, &mut seen, result_of("c-1"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "facts", "run_stopped"],
        "{settled}"
    );
    let observed = settled["outcomes"][1]["text"]
        .as_str()
        .expect("observation");
    assert!(observed.contains("run observed · exit 130"), "{observed}");
    let run = &settled["snapshot"]["work"]["run"];
    assert_eq!(run["end"]["end"], "interrupted", "{run}");
    assert_eq!(run["sealed"], true, "a sealed stop, never an abort: {run}");

    let trace = root.join(
        run["trace"]
            .as_str()
            .expect("the trace the settlement named"),
    );
    assert_sealed_cancelled(root, home.path(), &trace);

    // The same Stop replayed answers its record and sends nothing more.
    writeln!(stdin, "{}", stop("s-1")).expect("replay");
    let replay = next(&frames, &mut seen, |frame| {
        frame["command"] == "s-1" && frame["replayed"] == true
    });
    assert_eq!(replay["receipt"], "run_stopping", "{replay}");
    assert_eq!(replay["event"], receipt["event"], "{replay}");
    drop(stdin);
    next(&frames, &mut seen, |frame| frame["frame"] == "closed");
    assert!(child.wait().expect("exit").success());
}
