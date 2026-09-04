#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::disallowed_types)]
#![cfg(unix)]
//! Run state ≠ evidence state (ADR-129 · #1442): a run in flight and a
//! writer that died both leave a journal with no terminal frame. The
//! writer's lease tells them apart on every door — `trace ls` says
//! `running` then `dead`, `trace verify` says INCOMPLETE with the writer's
//! liveness and exits the INCOMPLETE class (5), never OK: a monitor wired
//! on the exit code cannot green a dead run.

use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};

/// `a` settles at once, `b` waits — the window the kill lands in.
const WAIT: &str = "nika: liveness-probe
permits: { tools: [\"nika:wait\", \"nika:jq\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:wait\", args: { duration: \"20s\" } }
";

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nika-liveness-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        std::fs::write(root.join("work").join("wait.nika.yaml"), WAIT).expect("workflow");
        Self { root }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
        cmd.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.root.join("home"))
            .env("TERM", "dumb")
            .current_dir(self.root.join("work"));
        cmd
    }

    fn spawn_run(&self) -> Child {
        self.command(&["run", "wait.nika.yaml", "--json", "--max-cost-usd", "0.01"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary spawns")
    }

    fn ls_json(&self) -> serde_json::Value {
        let out = self
            .command(&["trace", "ls", "--json"])
            .output()
            .expect("trace ls runs");
        serde_json::from_slice(&out.stdout).expect("trace ls --json is one document")
    }

    fn verify(&self, trace: &str) -> (i32, String, serde_json::Value) {
        let out = self
            .command(&["trace", "verify", trace])
            .output()
            .expect("trace verify runs");
        let json = self
            .command(&["trace", "verify", trace, "--json"])
            .output()
            .expect("trace verify --json runs");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            serde_json::from_slice(&json.stdout).expect("one JSON document"),
        )
    }
}

/// The newest running trace's row, once the run's first task has settled.
fn running_row(rig: &Rig) -> serde_json::Value {
    let doc = rig.ls_json();
    doc["traces"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["state"] != "succeeded").cloned())
        .unwrap_or_else(|| panic!("a running trace in the listing: {doc}"))
}

#[test]
fn a_dead_writer_reads_dead_and_verify_exits_incomplete() {
    let rig = Rig::new();
    let mut child = rig.spawn_run();
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    // The first wave settled: the journal is open, the lease is held, the
    // twenty-second wait is in flight.
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("stdout readable");
        assert!(n > 0, "the stream ended before the first task settled");
        if line.contains("\"kind\":\"task_completed\"") {
            break;
        }
    }
    // Door 1 · in flight: the store asks the lease and says alive.
    let row = running_row(&rig);
    assert_eq!(row["state"], "running", "{row}");
    assert_eq!(row["liveness"], "alive", "{row}");
    let trace = row["path"].as_str().expect("the trace path").to_owned();
    let (code, text, doc) = rig.verify(&trace);
    assert_eq!(
        code, 5,
        "INCOMPLETE exits its own class while in flight:\n{text}"
    );
    assert!(text.contains("the writer is alive"), "{text}");
    assert_eq!(doc["chain"]["headline"], "incomplete", "{doc}");
    assert_eq!(doc["chain"]["liveness"], "alive", "{doc}");

    // The writer dies without a word (SIGKILL: no handler, no terminal frame).
    let pid = child.id().to_string();
    let delivered = Command::new("kill")
        .args(["-KILL", &pid])
        .status()
        .expect("kill runs");
    assert!(delivered.success(), "kill delivered");
    let status = child.wait().expect("the child is reaped");
    assert!(!status.success(), "SIGKILL is never a success");

    // Door 2 · the same journal, the writer gone: dead, not running.
    let row = running_row(&rig);
    assert_eq!(row["path"], trace, "{row}");
    assert_eq!(row["state"], "dead", "{row}");
    assert_eq!(row["liveness"], "dead", "{row}");
    // Door 3 · verify names the dead writer and still exits INCOMPLETE.
    let (code, text, doc) = rig.verify(&trace);
    assert_eq!(code, 5, "{text}");
    assert!(text.contains("the writer is dead"), "{text}");
    assert_eq!(doc["chain"]["liveness"], "dead", "{doc}");
    assert_eq!(doc["exit"], 5, "{doc}");
    // Never a verdict on the run: no terminal frame was invented. The store
    // names the journal relative to the project (the rig's work dir).
    let journal = rig.root.join("work").join(&trace);
    assert!(
        !std::fs::read_to_string(&journal)
            .expect("the journal reads")
            .contains("workflow_failed"),
        "a dead writer proves incomplete evidence, never a failed run"
    );
}
