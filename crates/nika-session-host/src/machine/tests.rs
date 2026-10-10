// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native door over real pipes: the log in order with its event numbers, direct replies
//! without one, stdin read while a turn runs, and stdin's end closing the Session.

use std::io::Write as _;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use super::*;
use crate::host::tests::{COPY, runtime, world};
use crate::run::NoRunDoor;
use crate::wire::CONTRACT;

const WAIT: Duration = Duration::from_secs(60);

/// A writer that hands every finished line to the test.
pub(crate) struct Lines(pub(crate) mpsc::Sender<String>, pub(crate) Vec<u8>);

impl std::io::Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.1.extend_from_slice(bytes);
        while let Some(at) = self.1.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.1.drain(..=at).collect();
            let text = String::from_utf8(line).map_err(std::io::Error::other)?;
            let _ = self.0.send(text.trim_end().to_owned());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Door {
    input: Option<std::io::PipeWriter>,
    frames: mpsc::Receiver<String>,
    done: mpsc::Receiver<std::io::Result<()>>,
}

impl Door {
    fn open(root: &std::path::Path) -> Self {
        let host = Arc::new(
            SessionHost::start(runtime(root), Box::new(NoRunDoor::new("none")), Vec::new())
                .expect("host"),
        );
        let (reader, input) = std::io::pipe().expect("pipe");
        let (sender, frames) = mpsc::channel();
        let output = Arc::new(Mutex::new(Lines(sender, Vec::new())));
        let (finished, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("door".to_owned())
            .spawn(move || {
                let input = std::io::BufReader::new(reader);
                let _ = finished.send(drive(&host, input, &output));
            })
            .expect("door thread");
        Self {
            input: Some(input),
            frames,
            done,
        }
    }

    fn send(&mut self, command: &Value) {
        let input = self.input.as_mut().expect("stdin open");
        writeln!(input, "{command}").expect("stdin");
    }

    fn next(&self) -> Value {
        let line = self.frames.recv_timeout(WAIT).expect("a frame");
        serde_json::from_str(&line).expect("one JSON object per line")
    }

    /// The next frame of `kind`, with every frame passed on the way.
    fn until(&self, kind: &str) -> (Value, Vec<Value>) {
        let mut passed = Vec::new();
        loop {
            let frame = self.next();
            if frame["frame"] == kind {
                return (frame, passed);
            }
            passed.push(frame);
        }
    }
}

fn submit(command: &str, snapshot: &Value, line: &str) -> Value {
    serde_json::json!({
        "contract": CONTRACT, "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
}

#[test]
fn the_door_writes_its_log_in_order_and_replies_beside_it() {
    let root = world();
    let mut door = Door::open(root.path());
    let opened = door.next();
    assert_eq!(opened["frame"], "opened");
    assert_eq!(opened["event"], 1);
    let first = opened["snapshot"]["snapshot"].clone();
    door.send(&submit("c-1", &first, COPY));
    let (accepted, _) = door.until("accepted");
    assert_eq!(accepted["event"], 2);
    let (result, passed) = door.until("result");
    assert!(
        passed
            .iter()
            .all(|f| f["frame"] == "activity" && f["command"] == "c-1")
    );
    assert_eq!(result["outcomes"][0]["kind"], "proposal");
    let mut numbers: Vec<u64> = passed.iter().filter_map(|f| f["event"].as_u64()).collect();
    numbers.push(result["event"].as_u64().expect("event"));
    assert!(numbers.windows(2).all(|w| w[1] == w[0] + 1), "{numbers:?}");
    // Direct replies carry no event: a refusal, a read, a replay.
    door.send(&submit("c-2", &first, "yes"));
    let stale = door.next();
    assert_eq!(stale["frame"], "refused");
    assert_eq!(stale["error"], "stale_snapshot");
    assert!(stale.get("event").is_none());
    door.send(&serde_json::json!({"contract": CONTRACT, "op": "details"}));
    let details = door.next();
    assert_eq!(details["frame"], "details");
    assert!(details.get("event").is_none());
    door.send(&submit("c-1", &first, COPY));
    let replay = door.next();
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["event"], result["event"]);
    door.send(&Value::String("not json".to_owned()));
    assert_eq!(door.next()["error"], "malformed");
    // stdin's end closes the Session; the door returns once closed is written.
    door.input = None;
    let (closed, _) = door.until("closed");
    assert!(closed["event"].as_u64() > result["event"].as_u64());
    assert!(door.done.recv_timeout(WAIT).expect("door ended").is_ok());
}

/// A line the door cannot parse is refused `malformed` as a direct reply, naming the command
/// identity its JSON carries when that identity is a valid one, so the client that sent it tells
/// that refusal from another line's; a line naming none, or an invalid one, is refused naming
/// none.
#[test]
fn a_line_it_cannot_parse_is_refused_naming_the_identity_it_carries() {
    let root = world();
    let mut door = Door::open(root.path());
    assert_eq!(door.next()["frame"], "opened");
    door.send(&serde_json::json!({
        "contract": CONTRACT, "op": "rewind", "command": "c-9", "line": "use b instead",
    }));
    let named = door.next();
    assert_eq!(
        (
            named["frame"].as_str(),
            named["error"].as_str(),
            named["command"].as_str()
        ),
        (Some("refused"), Some("malformed"), Some("c-9")),
        "{named}"
    );
    assert!(named.get("event").is_none(), "a direct reply: {named}");
    door.send(&serde_json::json!({
        "contract": CONTRACT, "op": "rewind", "command": "not an identity",
    }));
    let invalid = door.next();
    assert_eq!(invalid["error"], "malformed", "{invalid}");
    assert!(invalid.get("command").is_none(), "{invalid}");
    door.send(&Value::String("not json".to_owned()));
    let unnamed = door.next();
    assert_eq!(unnamed["error"], "malformed", "{unnamed}");
    assert!(unnamed.get("command").is_none(), "{unnamed}");
    door.input = None;
    let (closed, _) = door.until("closed");
    assert_eq!(closed["frame"], "closed");
    assert!(door.done.recv_timeout(WAIT).expect("door ended").is_ok());
}

#[test]
fn stop_and_close_are_read_while_a_turn_runs() {
    let root = world();
    let gate = crate::host::tests::Gate::default();
    let host = Arc::new(
        SessionHost::start(
            runtime(root.path()),
            Box::new(NoRunDoor::new("none")),
            Vec::new(),
        )
        .expect("host"),
    );
    host.pause_at(gate.pause());
    gate.hold("returned");
    let (reader, mut input) = std::io::pipe().expect("pipe");
    let (sender, frames) = mpsc::channel();
    let output = Arc::new(Mutex::new(Lines(sender, Vec::new())));
    let driven = Arc::clone(&host);
    std::thread::Builder::new()
        .name("door".to_owned())
        .spawn(move || drive(&driven, std::io::BufReader::new(reader), &output))
        .expect("door thread");
    let next = || -> Value {
        serde_json::from_str(&frames.recv_timeout(WAIT).expect("frame")).expect("json")
    };
    let opened = next();
    let snapshot = opened["snapshot"]["snapshot"].clone();
    writeln!(input, "{}", submit("c-1", &snapshot, COPY)).expect("stdin");
    gate.reached();
    // The worker is held inside the turn; stdin is still read.
    writeln!(
        input,
        r#"{{"contract":"{CONTRACT}","op":"stop","command":"s-1"}}"#
    )
    .expect("stdin");
    let receipt = loop {
        let frame = next();
        if frame["frame"] == "result" {
            break frame;
        }
    };
    assert_eq!(receipt["op"], "stop");
    assert_eq!(receipt["receipt"], "stop_requested");
    writeln!(input, r#"{{"contract":"{CONTRACT}","op":"close"}}"#).expect("stdin");
    gate.release();
    let mut seen = Vec::new();
    loop {
        let frame = next();
        seen.push(frame["frame"].as_str().unwrap_or_default().to_owned());
        if frame["frame"] == "closed" {
            break;
        }
    }
    assert_eq!(seen.last().map(String::as_str), Some("closed"));
    assert!(
        seen.contains(&"result".to_owned()),
        "the stopped turn settled before closing"
    );
}
