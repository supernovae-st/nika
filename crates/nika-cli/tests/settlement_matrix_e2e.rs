#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::disallowed_types)]
#![cfg(unix)]
//! The cross-door matrix (OD-F10 · the one-door gauntlet): for one
//! operation, every applicable door consumes the same semantic authority,
//! reports the same lifecycle and settlement, and references the same
//! evidence. Each row runs a scenario on the real binary and compares the
//! settlement across the four local doors — the terminal frame on the
//! machine stream, the `run_settled` envelope, `trace outputs --json` read
//! back from disk, `trace ls --json` — plus the scenario's own laws (the
//! per-item rows · the recovered lineage · the gate that pauses and the
//! same run that resumes · the operator's cancel · the dead writer).
//! Semantics, never prose: a state word, a cause, a tally, a qualifier, a
//! named failure.

use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};

/// Two jq tasks · succeeds · unmetered.
const CLEAN: &str = "nika: m-clean
permits: { tools: [\"nika:jq\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: 2, expression: \".\" } }
";

/// The second task reads a file that does not exist · fails.
const FAIL: &str = "nika: m-fail
permits: { fs: { read: [\"./missing.md\"] }, tools: [\"nika:jq\", \"nika:read\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:read\", args: { path: \"./missing.md\" } }
";

/// A failing read recovered by a fallback, consumed downstream · the
/// recovered lineage.
const RECOVER: &str = "nika: m-recover
permits: { fs: { read: [\"./missing.md\"] }, tools: [\"nika:jq\", \"nika:read\"] }
const:
  fallback: \"FALLBACK\"
tasks:
  a:
    invoke: { tool: \"nika:read\", args: { path: \"./missing.md\" } }
    on_error: { recover: \"${{ const.fallback }}\" }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.prev }}\", expression: \".\" } }
";

/// A fan-out kept going over three failing items · every item's terminal.
const FAN: &str = "nika: m-fan
permits: { exec: [\"false\"] }
tasks:
  fan:
    for_each: { items: [\"alpha\", \"beta\", \"gamma\"], fail_fast: false }
    exec: { command: [\"false\"] }
";

/// A human gate after one task · pauses (no terminal: the gate parks) ·
/// the same run resumes with the answer.
const GATE: &str = "nika: m-gate
permits: { tools: [\"nika:jq\", \"nika:prompt\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  gate:
    after: { a: success }
    invoke: { tool: \"nika:prompt\", args: { message: \"ship it?\" } }
  after_gate:
    with: { ok: \"${{ tasks.gate.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.ok }}\", expression: \".\" } }
";

/// `a` settles at once, `b` waits — the window a signal lands in.
const WAIT: &str = "nika: m-wait
permits: { tools: [\"nika:wait\", \"nika:jq\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:wait\", args: { duration: \"20s\" } }
  c:
    with: { prev: \"${{ tasks.b.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: 2, expression: \".\" } }
";

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nika-matrix-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        for (file, body) in [
            ("clean.nika.yaml", CLEAN),
            ("fail.nika.yaml", FAIL),
            ("recover.nika.yaml", RECOVER),
            ("fan.nika.yaml", FAN),
            ("gate.nika.yaml", GATE),
            ("wait.nika.yaml", WAIT),
        ] {
            std::fs::write(root.join("work").join(file), body).expect("workflow");
        }
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

    /// `nika run … --json`: the exit code and every NDJSON line on stdout.
    fn run_json(&self, args: &[&str]) -> (i32, Vec<serde_json::Value>) {
        let mut full = vec!["run"];
        full.extend_from_slice(args);
        full.extend_from_slice(&["--json", "--max-cost-usd", "0.01"]);
        let out = self.command(&full).output().expect("the binary runs");
        let lines = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
            .collect();
        (out.status.code().unwrap_or(-1), lines)
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.command(args).output().expect("the binary runs");
        let text = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(text.trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }

    fn spawn_wait(&self) -> Child {
        self.command(&["run", "wait.nika.yaml", "--json", "--max-cost-usd", "0.01"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary spawns")
    }
}

/// The settlement as every door must agree on it.
#[derive(Debug, PartialEq)]
struct Verdict {
    status: String,
    cause: String,
    tasks: (u64, u64, u64, u64),
    qualifier: String,
    error_code: Option<String>,
}

fn field<'a>(frame: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if let Some(fields) = frame.get("fields").and_then(|f| f.as_array()) {
        return fields
            .iter()
            .find(|kv| kv["key"] == key)
            .and_then(|kv| kv.get("value"));
    }
    frame.get(key)
}

fn from_terminal_frame(frame: &serde_json::Value) -> Verdict {
    let int = |k: &str| {
        field(frame, k)
            .and_then(serde_json::Value::as_i64)
            .and_then(|i| u64::try_from(i).ok())
            .unwrap_or(0)
    };
    let text = |k: &str| {
        field(frame, k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    Verdict {
        status: text("status"),
        cause: text("cause"),
        tasks: (
            int("tasks_total"),
            int("tasks_ok"),
            int("tasks_failed"),
            int("tasks_recovered"),
        ),
        qualifier: text("cost_qualifier"),
        error_code: field(frame, "error_code").and_then(|v| v.as_str().map(str::to_owned)),
    }
}

fn from_settlement(doc: &serde_json::Value) -> Verdict {
    let int = |k: &str| doc["tasks"][k].as_u64().unwrap_or(0);
    Verdict {
        status: doc["status"].as_str().unwrap_or_default().to_owned(),
        cause: doc["cause"].as_str().unwrap_or_default().to_owned(),
        tasks: (int("total"), int("ok"), int("failed"), int("recovered")),
        qualifier: doc["spend"]["qualifier"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        error_code: doc["error"]["code"].as_str().map(str::to_owned),
    }
}

fn terminal_of(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|l| {
            matches!(
                l["kind"].as_str(),
                Some(
                    "workflow_completed"
                        | "workflow_failed"
                        | "workflow_cancelled"
                        | "workflow_paused"
                )
            )
        })
        .expect("a terminal frame on the stream")
}

fn settled_of(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|l| l["kind"] == "run_settled")
        .expect("the run_settled envelope")
}

fn ls_row(rig: &Rig, trace: &str) -> serde_json::Value {
    let listing = rig.json(&["trace", "ls", "--json"]);
    let name = trace.rsplit('/').next().unwrap_or(trace).to_owned();
    listing["traces"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with(&name)))
                .cloned()
        })
        .unwrap_or_else(|| panic!("the trace in the listing: {listing}"))
}

/// The four local doors agree on one run.
fn doors_agree(rig: &Rig, lines: &[serde_json::Value], expect: &Verdict) -> String {
    let settled = settled_of(lines);
    let trace = settled["receipt"]["trace_path"]
        .as_str()
        .expect("the receipt names the trace")
        .to_owned();
    assert_eq!(
        &from_terminal_frame(terminal_of(lines)),
        expect,
        "door 1 · the terminal frame"
    );
    assert_eq!(&from_settlement(settled), expect, "door 2 · run_settled");
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    assert_eq!(
        outputs["state"], expect.status,
        "door 3 · trace outputs state"
    );
    assert_eq!(
        &from_settlement(&outputs["settlement"]),
        expect,
        "door 3 · trace outputs settlement"
    );
    assert_eq!(
        ls_row(rig, &trace)["state"],
        expect.status,
        "door 4 · trace ls"
    );
    trace
}

fn unmetered(
    status: &str,
    cause: &str,
    tasks: (u64, u64, u64, u64),
    error: Option<&str>,
) -> Verdict {
    Verdict {
        status: status.to_owned(),
        cause: cause.to_owned(),
        tasks,
        qualifier: "unmetered".to_owned(),
        error_code: error.map(str::to_owned),
    }
}

#[test]
fn row_clean_every_door_says_succeeded() {
    let rig = Rig::new("clean");
    let (code, lines) = rig.run_json(&["clean.nika.yaml"]);
    assert_eq!(code, 0);
    // ADR-129 · the local door names the evidence it left, and the word
    // agrees with the receipt (this rig holds no signing key: unsealed).
    let settled = settled_of(&lines);
    let expected = if settled["receipt"]["sealed"] == true {
        "sealed"
    } else {
        "unsealed"
    };
    assert_eq!(settled["evidence"], expected, "{settled}");
    doors_agree(
        &rig,
        &lines,
        &unmetered("succeeded", "normal", (2, 2, 0, 0), None),
    );
}

#[test]
fn row_failure_every_door_names_the_same_failed_task() {
    let rig = Rig::new("fail");
    let (code, lines) = rig.run_json(&["fail.nika.yaml"]);
    assert_eq!(code, 1, "the WORKFLOW class");
    let settled = settled_of(&lines);
    let error_code = settled["error"]["code"].as_str().expect("named").to_owned();
    assert_eq!(settled["error"]["task"], "b");
    doors_agree(
        &rig,
        &lines,
        &unmetered("failed", "task_failed", (2, 1, 1, 0), Some(&error_code)),
    );
}

#[test]
fn row_recovery_the_lineage_reaches_every_door() {
    let rig = Rig::new("recover");
    let (code, lines) = rig.run_json(&["recover.nika.yaml"]);
    assert_eq!(code, 0, "a recovered task is a success");
    let trace = doors_agree(
        &rig,
        &lines,
        &unmetered("succeeded", "normal", (2, 2, 0, 1), None),
    );
    // the recovered task and the task fed by it, on the machine read
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    let tasks = outputs["tasks"].as_array().expect("rows");
    let a = tasks.iter().find(|t| t["id"] == "a").expect("a");
    assert_eq!(a["status"], "recovered", "{a}");
    assert!(
        a["recovered_from"].is_string(),
        "the original error rides: {a}"
    );
    // the prose surfaces never read clean (#1275 · #1444)
    let out = rig
        .command(&["trace", "outputs", &trace])
        .output()
        .expect("runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("recovered"), "{text}");
}

#[test]
fn row_fan_out_every_item_terminal_reaches_the_machine_read() {
    let rig = Rig::new("fan");
    let (code, lines) = rig.run_json(&["fan.nika.yaml"]);
    assert_eq!(code, 1);
    let settled = settled_of(&lines);
    let error_code = settled["error"]["code"].as_str().expect("named").to_owned();
    let trace = doors_agree(
        &rig,
        &lines,
        &unmetered("failed", "task_failed", (1, 0, 1, 0), Some(&error_code)),
    );
    // door 1 · the fan-out's terminal frame carries every item
    let items: serde_json::Value = lines
        .iter()
        .find(|l| l["kind"] == "task_failed")
        .and_then(|f| field(f, "items"))
        .and_then(|v| v.as_str())
        .map(|s| serde_json::from_str(s).expect("items JSON"))
        .expect("the fan-out's items ride its terminal");
    let rows = items.as_array().expect("rows");
    assert_eq!(rows.len(), 3, "{items}");
    assert!(rows.iter().all(|r| r["status"] == "failed"), "{items}");
    // door 3 · the same rows on the trace read
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    let fan = outputs["tasks"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["id"] == "fan").cloned())
        .expect("the fan row");
    assert_eq!(fan["status"], "failed", "{fan}");
}

#[test]
fn row_human_gate_pauses_and_the_same_run_resumes_on_every_door() {
    let rig = Rig::new("gate");
    let (code, lines) = rig.run_json(&["gate.nika.yaml"]);
    assert_eq!(code, 4, "the PAUSED class");
    // the pause is a settlement too: paused · human_gate · one task ran
    let terminal = terminal_of(&lines);
    assert_eq!(terminal["kind"], "workflow_paused");
    let paused = from_terminal_frame(terminal);
    assert_eq!(
        (paused.status.as_str(), paused.cause.as_str()),
        ("paused", "human_gate")
    );
    let settled = settled_of(&lines);
    assert_eq!(settled["status"], "paused");
    assert_eq!(settled["cause"], "human_gate");
    let trace = settled["receipt"]["trace_path"]
        .as_str()
        .expect("trace")
        .to_owned();
    assert_eq!(
        ls_row(&rig, &trace)["state"],
        "paused",
        "an obligation, on the listing"
    );
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    assert_eq!(outputs["state"], "paused");
    // the SAME run resumes with the answer
    let (code, resumed) =
        rig.run_json(&["gate.nika.yaml", "--resume", &trace, "--answer", "gate=yes"]);
    assert_eq!(code, 0, "the resumed run succeeds");
    doors_agree(
        &rig,
        &resumed,
        &unmetered("succeeded", "normal", (3, 3, 0, 0), None),
    );
    // a second answer to the decided gate is refused, never a second effect
    let out = rig
        .command(&[
            "run",
            "gate.nika.yaml",
            "--resume",
            &trace,
            "--answer",
            "gate=yes",
            "--json",
            "--max-cost-usd",
            "0.01",
        ])
        .output()
        .expect("runs");
    assert_ne!(out.status.code(), Some(0), "a decided gate stays decided");
}

#[test]
fn row_cancel_every_door_says_cancelled_by_the_operator() {
    let rig = Rig::new("cancel");
    let mut child = rig.spawn_wait();
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut seen = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("stdout readable");
        assert!(
            n > 0,
            "the stream ended before the first task settled:\n{seen}"
        );
        seen.push_str(&line);
        if line.contains("\"kind\":\"task_completed\"") {
            break;
        }
    }
    let pid = child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .expect("kill runs")
            .success(),
        "SIGINT delivered"
    );
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut reader, &mut rest).expect("the rest of the stream");
    seen.push_str(&rest);
    let status = child.wait().expect("reaped");
    assert_eq!(status.code(), Some(130), "the CANCELLED class");
    let lines: Vec<serde_json::Value> = seen
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
        .collect();
    // in-flight work completed and was counted; the unstarted task never ran
    doors_agree(
        &rig,
        &lines,
        &unmetered("cancelled", "operator", (3, 2, 0, 0), None),
    );
    let settled = settled_of(&lines);
    assert_eq!(settled["tasks"]["cancelled"], 1, "{settled}");
    assert_eq!(settled["tasks"]["never_started"], 1, "{settled}");
}

#[test]
fn row_hard_death_proves_incomplete_evidence_never_a_failed_run() {
    let rig = Rig::new("death");
    let mut child = rig.spawn_wait();
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("stdout readable");
        assert!(n > 0, "the stream ended before the first task settled");
        if line.contains("\"kind\":\"task_completed\"") {
            break;
        }
    }
    let pid = child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-KILL", &pid])
            .status()
            .expect("kill runs")
            .success()
    );
    let _ = child.wait();
    let listing = rig.json(&["trace", "ls", "--json"]);
    let row = listing["traces"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["state"] != "succeeded").cloned())
        .expect("the dead run's trace");
    assert_eq!(row["state"], "dead", "{row}");
    assert_eq!(row["liveness"], "dead", "{row}");
    let trace = row["path"].as_str().expect("path").to_owned();
    let verify = rig
        .command(&["trace", "verify", &trace, "--json"])
        .output()
        .expect("verify runs");
    assert_eq!(
        verify.status.code(),
        Some(5),
        "INCOMPLETE exits its own class"
    );
    let doc: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("one document");
    assert_eq!(doc["chain"]["headline"], "incomplete");
    assert_eq!(doc["chain"]["liveness"], "dead");
    // no door invents a terminal: the trace read says the run never settled
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    assert_eq!(
        outputs["state"], "running",
        "no terminal frame → still running on the read, dead on the listing"
    );
    assert!(outputs.get("settlement").is_none(), "{outputs}");
}
