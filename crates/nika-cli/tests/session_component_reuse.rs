// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A component of the embedded knowledge release reused end to end through the native machine
//! door of the real binary: the document door composes `block:json-filter-records` into a new
//! workflow, `save & run` lands and runs it, a later change rebinds its filter from `> 48` to
//! `> 72` over the saved bytes, and `save & run` runs the revision. Keyless: an isolated HOME,
//! an empty environment, a scripted seat on the loopback that answers by each request's schema.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::collections::BTreeSet;
use std::io::{BufRead as _, BufReader, Lines, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// A request the deterministic reader leaves to the seat (it settles no filter of its own).
const CREATE: &str = concat!(
    "Filter the JSON records of ./in/tickets.json down to the tickets older than 48 hours ",
    "(age_hours > 48)."
);
const EDIT: &str = "Raise the age threshold to 72 hours.";
/// Ages on both sides of each threshold: 47 and 48 are not older than 48 hours, 72 is not older
/// than 72.
const TICKETS: &str = concat!(
    r#"[{"id":"t-47","age_hours":47},{"id":"t-48","age_hours":48},"#,
    r#"{"id":"t-60","age_hours":60},{"id":"t-72","age_hours":72},{"id":"t-90","age_hours":90}]"#
);
const BLOCK: &str = "block:json-filter-records";
const HOLE: &str = "tasks.filter_records.invoke.args.expression";

/// The author's envelope: its name and the boundary the block needs, since a composed
/// component grants nothing.
const ENVELOPE: &str = r#"nika: stale-tickets
permits:
  fs: { read: ["./in/tickets.json"] }
  tools: ["nika:read", "nika:jq"]
"#;

/// The version of `BLOCK` the document door showed in `request`'s opening (its `components`,
/// one row per admitted block of the embedded release), when it showed it.
fn shown_version(request: &Value) -> Option<String> {
    (request["messages"].as_array()?.iter())
        .filter_map(|message| message["content"].as_str())
        .filter_map(|content| serde_json::from_str::<Value>(content).ok())
        .find_map(|opening| {
            (opening["components"].as_array()?.iter())
                .find(|row| row["component"]["id"] == BLOCK)
                .and_then(|row| row["component"]["version"].as_str())
                .map(str::to_owned)
        })
}

/// The document door's answer: the envelope, and the release's block composed into it, its two
/// holes bound to the request's words.
fn created(version: &str) -> String {
    let bindings = json!({"const.source_path": "./in/tickets.json",
        HOLE: "[.[] | select(.age_hours > 48)]"});
    json!({"candidate": ENVELOPE, "questions": [], "gaps": [],
        "notes": "the release's JSON filter, bound to the request",
        "operations": [{"op": "compose", "component": BLOCK, "version": version,
            "bindings_json": bindings.to_string()}]})
    .to_string()
}

/// The revision's answer over the saved document: the composed block rebound at its filter,
/// nothing else stated.
fn rebound() -> String {
    let bindings = json!({HOLE: "[.[] | select(.age_hours > 72)]"});
    json!({"notes": "the threshold only", "replace": "",
        "operations": [{"op": "rebind", "component": BLOCK,
            "bindings_json": bindings.to_string()}]})
    .to_string()
}

/// A scripted seat on the loopback, on the OpenAI-compatible wire a `vllm` choice speaks. It
/// answers by the request's schema (a route label, the document, the revision, a judgment) and
/// keeps every request body for the assertions.
struct Seat {
    port: u16,
    bodies: Arc<Mutex<Vec<String>>>,
}

impl Seat {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("seat");
        let port = listener.local_addr().expect("seat address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&bodies);
        // The seat's accept loop: a test harness thread, never production.
        #[allow(clippy::disallowed_methods)]
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(body) = request_body(&mut stream) else {
                    continue;
                };
                let content = answer(&body);
                kept.lock().expect("bodies").push(body);
                let reply = json!({
                    "id": "chatcmpl-seat", "object": "chat.completion",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
                        "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
                })
                .to_string();
                let _written = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            }
        });
        Self { port, bodies }
    }

    fn bodies(&self) -> Vec<String> {
        self.bodies.lock().expect("bodies").clone()
    }

    /// The kept choice of this seat in `home`, and the environment that points it here.
    fn chosen_in(&self, home: &Path) -> Vec<(&'static str, String)> {
        std::fs::create_dir_all(home.join(".nika")).expect("home");
        std::fs::write(
            home.join(".nika/session-intelligence.json"),
            r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/reuse-seat","chosen_at":"2026-10-09T00:00:00Z"}"#,
        )
        .expect("preference");
        vec![
            (
                "NIKA_VLLM_BASE_URL",
                format!("http://127.0.0.1:{}/v1", self.port),
            ),
            ("NIKA_AUTHORING_STRATEGY", "only".to_owned()),
        ]
    }
}

/// The answer to one request, by what it asks. The document is composed only from a version of
/// `BLOCK` the request itself showed; otherwise it states nothing to compose.
fn answer(body: &str) -> String {
    let request: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let format = &request["response_format"];
    let schema = if format["json_schema"]["schema"].is_object() {
        &format["json_schema"]["schema"]
    } else {
        &format["schema"]
    };
    let properties = &schema["properties"];
    if body.contains("You route ONE line") {
        return if body.contains(EDIT) {
            "MODIFY"
        } else {
            "NEW_WORK"
        }
        .to_owned();
    }
    if let Some(keys) = properties["choice"]["enum"].as_array() {
        let approve = ["faithful", "carried", "superseded"]
            .into_iter()
            .find(|key| keys.iter().any(|value| value == *key))
            .unwrap_or("none");
        return json!({"choice": approve}).to_string();
    }
    match (properties.get("candidate"), properties.get("operations")) {
        (Some(_), Some(_)) => {
            shown_version(&request).map_or_else(|| "{}".to_owned(), |v| created(&v))
        }
        (None, Some(_)) => rebound(),
        _ => "{}".to_owned(),
    }
}

/// One request's body, read whole: its head, then as many bytes as it declares.
fn request_body(stream: &mut TcpStream) -> Option<String> {
    let mut data = Vec::new();
    let mut chunk = [0_u8; 8192];
    let end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while data.len() < end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
    }
    Some(String::from_utf8_lossy(&data[end..]).into_owned())
}

/// `nika session --json` over a project, its seat chosen in `home`: one frame per line.
struct Door {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Lines<BufReader<ChildStdout>>,
    sent: usize,
}

impl Door {
    /// The door opened, with the snapshot its first frame carries.
    fn open(project: &Path, home: &Path, seat: &Seat) -> (Self, Value) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
        command
            .args(["session", "--json"])
            .current_dir(project)
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .env("NIKA_KEYCHAIN", "off");
        for (key, value) in seat.chosen_in(home) {
            command.env(key, value);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("nika session --json");
        let stdin = child.stdin.take();
        let lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        let mut door = Self {
            child,
            stdin,
            lines,
            sent: 0,
        };
        let opened = door.next("opened");
        (door, opened["snapshot"]["snapshot"].clone())
    }

    fn next(&mut self, kind: &str) -> Value {
        loop {
            let line = self.lines.next().expect("a frame").expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if frame["frame"] == kind {
                return frame;
            }
        }
    }

    /// `line` submitted over the snapshot `frame` carries; the result frame.
    fn submit(&mut self, frame: &Value, line: &str) -> Value {
        self.sent += 1;
        let submitted = json!({"contract": "nika/session-host@1", "op": "submit",
            "command": format!("c-{}", self.sent), "snapshot": frame, "line": line});
        let stdin = self.stdin.as_mut().expect("the door is open");
        writeln!(stdin, "{submitted}").expect("submit");
        self.next("result")
    }

    fn close(mut self) {
        drop(self.stdin.take());
        self.next("closed");
        assert!(self.child.wait().expect("exit").success());
    }
}

fn kinds(result: &Value) -> Vec<&str> {
    (result["outcomes"].as_array().expect("outcomes").iter())
        .map(|outcome| outcome["kind"].as_str().expect("kind"))
        .collect()
}

/// How the proposal in `result` was made, as its work record states it.
fn revision_of(result: &Value) -> &Value {
    &result["snapshot"]["work"]["candidate"]["revision"]
}

/// The record holds `BLOCK` of the shown release, witnessed on the proposed bytes as bound: the
/// request's source path and `expression`.
fn assert_composed(revision: &Value, version: &str, expression: &str) {
    let component = &revision["components"][0];
    assert_eq!(component["id"], BLOCK, "{revision}");
    assert_eq!(component["version"], version, "{revision}");
    assert_eq!(component["witness"], "expanded", "{revision}");
    assert_eq!(
        component["bindings"],
        json!([{"path": "const.source_path", "value": "./in/tickets.json"},
            {"path": HOLE, "value": expression}]),
        "{revision}"
    );
}

/// `save & run` saved the proposal and observed its one run succeed.
fn assert_ran(result: &Value) {
    assert_eq!(kinds(result), ["run_requested", "facts"], "{result}");
    let observed = result["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
}

/// The run journals under `root`.
fn journals(root: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(root.join(".nika/traces"))
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
                .collect()
        })
        .unwrap_or_default()
}

/// The ids the run in `journal` selected: the filter task's completed output.
fn selected(journal: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(journal).expect("journal");
    let frame: Value = text
        .lines()
        .filter(|line| line.contains("\"kind\":\"task_completed\""))
        .filter(|line| line.contains("\"value\":\"filter_records\""))
        .find_map(|line| serde_json::from_str(line).ok())
        .unwrap_or_else(|| panic!("no completed filter in {}:\n{text}", journal.display()));
    let output = (frame["fields"].as_array().expect("fields").iter())
        .find(|field| field["key"] == "output")
        .map(|field| field["value"].clone())
        .expect("its output");
    let rows: Value = match output {
        Value::String(text) => serde_json::from_str(&text).expect("the output's JSON"),
        other => other,
    };
    (rows.as_array().expect("the selected records").iter())
        .map(|row| row["id"].as_str().expect("an id").to_owned())
        .collect()
}

/// The workflow files under `root`.
fn workflows(root: &Path) -> Vec<PathBuf> {
    (std::fs::read_dir(root)
        .expect("project")
        .filter_map(Result::ok))
    .map(|entry| entry.path())
    .filter(|path| path.extension().is_some_and(|ext| ext == "nika"))
    .collect()
}

#[test]
fn a_released_block_is_composed_saved_run_rebound_and_run_again() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir_all(project.path().join("in")).expect("in");
    std::fs::write(project.path().join("in/tickets.json"), TICKETS).expect("tickets");
    let seat = Seat::start();
    let (mut door, opened) = Door::open(project.path(), home.path(), &seat);

    // CREATE: the document door shows the release's block, which the author composes, bound.
    let proposed = door.submit(&opened, CREATE);
    assert_eq!(
        kinds(&proposed),
        ["proposal"],
        "{proposed}\n{:#?}",
        seat.bodies()
    );
    let version = (seat.bodies().iter())
        .filter_map(|body| serde_json::from_str::<Value>(body).ok())
        .find_map(|request| shown_version(&request))
        .unwrap_or_else(|| panic!("the door showed {BLOCK}: {:#?}", seat.bodies()));
    let created = revision_of(&proposed);
    assert_eq!(created["mode"], "composed", "{created}");
    assert_composed(created, &version, "[.[] | select(.age_hours > 48)]");
    let ran = door.submit(&proposed["snapshot"]["snapshot"], "save & run");
    assert_ran(&ran);
    let saved = workflows(project.path());
    assert_eq!(saved.len(), 1, "one saved workflow: {saved:?}");
    let first = std::fs::read_to_string(&saved[0]).expect("the saved workflow");
    assert!(
        first.contains(r#"source_path: "./in/tickets.json""#),
        "{first}"
    );
    assert!(first.contains("select(.age_hours > 48)"), "{first}");
    let before = journals(project.path());
    let first_run = before.iter().next().expect("the run's journal");
    assert_eq!(before.len(), 1, "one run");
    assert_eq!(selected(first_run), ["t-60", "t-72", "t-90"]);

    // EDIT: the change rebinds the composed filter over the saved bytes; the revision runs.
    let revised = door.submit(&ran["snapshot"]["snapshot"], EDIT);
    assert_eq!(
        kinds(&revised),
        ["proposal"],
        "{revised}\n{:#?}",
        seat.bodies()
    );
    let rebound = revision_of(&revised);
    assert_eq!(rebound["mode"], "operations", "{rebound}");
    assert_composed(rebound, &version, "[.[] | select(.age_hours > 72)]");
    let reran = door.submit(&revised["snapshot"]["snapshot"], "save & run");
    assert_ran(&reran);
    let second = std::fs::read_to_string(&saved[0]).expect("the revised workflow");
    assert_eq!(
        second,
        first.replace("select(.age_hours > 48)", "select(.age_hours > 72)"),
        "only the bound filter changed"
    );
    let after = journals(project.path());
    let new: Vec<&PathBuf> = after.difference(&before).collect();
    assert_eq!((after.len(), new.len()), (2, 1), "one run per save & run");
    assert_eq!(selected(new[0]), ["t-90"]);
    door.close();
}
