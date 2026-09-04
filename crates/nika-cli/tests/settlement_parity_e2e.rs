#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::disallowed_types)]
//! One settlement, every door (ADR-128 · the one-door gauntlet's first
//! row): for the same run, the journal's terminal frame, the `run_settled`
//! envelope on the machine stream, `trace outputs --json` and `trace ls
//! --json` say the same state, the same cause, the same tally, the same
//! spend qualifier and the same named failure — because none of them folds
//! the run: they all read the settlement the runtime built once.

use std::process::Command;

/// A run that succeeds with no model (jq only) — unmetered spend.
const OK: &str = "nika: settle-ok
permits: { tools: [\"nika:jq\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:jq\", args: { input: 2, expression: \".\" } }
";

/// A run whose second task fails (a read of a file that does not exist).
const FAIL: &str = "nika: settle-fail
permits: { fs: { read: [\"./missing.md\"] }, tools: [\"nika:jq\", \"nika:read\"] }
tasks:
  a:
    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }
  b:
    with: { prev: \"${{ tasks.a.output }}\" }
    invoke: { tool: \"nika:read\", args: { path: \"./missing.md\" } }
";

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nika-settle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        std::fs::write(root.join("work").join("ok.nika.yaml"), OK).expect("workflow");
        std::fs::write(root.join("work").join("fail.nika.yaml"), FAIL).expect("workflow");
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

    /// `nika run <file> --json`: every NDJSON line on stdout, parsed.
    fn run_json(&self, file: &str) -> (i32, Vec<serde_json::Value>) {
        let out = self
            .command(&["run", file, "--json", "--max-cost-usd", "0.01"])
            .output()
            .expect("the binary runs");
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
}

/// The four doors' view of one run — the same shape, read from each.
#[derive(Debug, PartialEq)]
struct Verdict {
    status: String,
    cause: String,
    tasks: (u64, u64, u64),
    qualifier: String,
    error_code: Option<String>,
}

fn field<'a>(frame: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    // A journal frame keeps its fields as `[{key, value}]`; an envelope keeps
    // them flat — one accessor reads both.
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
        tasks: (int("tasks_total"), int("tasks_ok"), int("tasks_failed")),
        qualifier: text("cost_qualifier"),
        error_code: field(frame, "error_code").and_then(|v| v.as_str().map(str::to_owned)),
    }
}

fn from_settlement(doc: &serde_json::Value) -> Verdict {
    let int = |k: &str| doc["tasks"][k].as_u64().unwrap_or(0);
    Verdict {
        status: doc["status"].as_str().unwrap_or_default().to_owned(),
        cause: doc["cause"].as_str().unwrap_or_default().to_owned(),
        tasks: (int("total"), int("ok"), int("failed")),
        qualifier: doc["spend"]["qualifier"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        error_code: doc["error"]["code"].as_str().map(str::to_owned),
    }
}

fn agree(rig: &Rig, file: &str, expect: &Verdict) {
    let (_code, lines) = rig.run_json(file);
    let terminal = lines
        .iter()
        .find(|l| {
            matches!(
                l["kind"].as_str(),
                Some("workflow_completed" | "workflow_failed" | "workflow_cancelled")
            )
        })
        .expect("a terminal frame on the stream");
    let settled = lines
        .iter()
        .find(|l| l["kind"] == "run_settled")
        .expect("the run_settled envelope");
    let trace = settled["receipt"]["trace_path"]
        .as_str()
        .expect("the receipt names the trace")
        .to_owned();

    // Door 1 · the journal's terminal frame, as the stream carried it.
    assert_eq!(&from_terminal_frame(terminal), expect, "terminal frame");
    // Door 2 · the CLI's machine envelope.
    assert_eq!(&from_settlement(settled), expect, "run_settled");
    // Door 3 · the trace read back from disk through the one reader.
    let outputs = rig.json(&["trace", "outputs", &trace, "--json"]);
    assert_eq!(outputs["state"], expect.status, "trace outputs state");
    assert_eq!(
        &from_settlement(&outputs["settlement"]),
        expect,
        "trace outputs settlement"
    );
    // Door 4 · the store's listing speaks the same state word.
    let listing = rig.json(&["trace", "ls", "--json"]);
    let rows = listing
        .as_array()
        .cloned()
        .or_else(|| listing["traces"].as_array().cloned())
        .expect("trace ls --json lists traces");
    let mine = rows
        .iter()
        .find(|r| {
            r["path"]
                .as_str()
                .is_some_and(|p| p.ends_with(trace.rsplit('/').next().unwrap_or(&trace)))
        })
        .unwrap_or_else(|| panic!("the run's trace in the listing: {listing}"));
    assert_eq!(mine["state"], expect.status, "trace ls state");
}

#[test]
fn a_succeeded_run_settles_the_same_on_every_door() {
    let rig = Rig::new("ok");
    agree(
        &rig,
        "ok.nika.yaml",
        &Verdict {
            status: "succeeded".to_owned(),
            cause: "normal".to_owned(),
            tasks: (2, 2, 0),
            qualifier: "unmetered".to_owned(),
            error_code: None,
        },
    );
}

#[test]
fn a_failed_run_settles_the_same_on_every_door_and_names_its_failure() {
    let rig = Rig::new("fail");
    let (code, lines) = rig.run_json("fail.nika.yaml");
    assert_ne!(code, 0, "a failed run exits non-zero");
    let settled = lines
        .iter()
        .find(|l| l["kind"] == "run_settled")
        .expect("the run_settled envelope");
    let error_code = settled["error"]["code"]
        .as_str()
        .expect("the failure is named on the envelope")
        .to_owned();
    assert_eq!(settled["error"]["task"], "b", "the failing task is named");
    agree(
        &rig,
        "fail.nika.yaml",
        &Verdict {
            status: "failed".to_owned(),
            cause: "task_failed".to_owned(),
            tasks: (2, 1, 1),
            qualifier: "unmetered".to_owned(),
            error_code: Some(error_code),
        },
    );
}
