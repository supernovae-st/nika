// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Private authoring capture through the real CLI: `nika compile --private-authoring-capture`
//! on a scripted loopback seat, under the explicit semantic Sketch route. The saved file is
//! read here independently of the writer (NDJSON, then space padding only), its digests are
//! recomputed from the scripted bytes, and every public surface is searched for the raw
//! sentinels. No provider, network beyond 127.0.0.1, credential store or paid call.

use super::{LoopbackSeat, command, result};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::Output;

const INTENT: &str = "Review this customer request and harmonise the tone of the support reply";
/// A synthetic Anthropic key; it must never reach any file or stream.
const KEY: &str = "sk-ant-synthetic-capture-key";
/// The explicit semantic Sketch route with its default repairs (a typed repair count would
/// need more requests than these three authorized ones, and is refused before any request).
const ROUTE: [&str; 7] = [
    "--authoring-strategy",
    "sketch",
    "--authoring-max-calls",
    "3",
    "--authoring-timeout",
    "2",
    "--json",
];

/// A Sketch answer that decodes but states no task: refused, so the seat is asked to repair.
/// The sentinel rides the graph's `name`, which the public receipt keeps only as a digest (a
/// refused sketch's decoded `notes` are already part of the public document, capture or not).
fn rejected_sketch(sentinel: &str) -> String {
    json!({"name": sentinel, "tasks": [], "questions": [], "gaps": [], "notes": ""}).to_string()
}

/// A repair answer naming a key the Sketch schema does not know: the round ends there.
fn unknown_key(sentinel: &str) -> String {
    json!({"unknown_key_canary": sentinel}).to_string()
}

struct Run {
    out: Output,
    doc: Value,
    bodies: Vec<Value>,
}

/// One real `nika compile` on the vLLM loopback route (one Text block per answer).
fn vllm(room: &Path, script: Vec<String>, capture: bool) -> Run {
    let seat = LoopbackSeat::start(script);
    let mut cmd = command(room);
    cmd.env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", INTENT, "--authoring-model", "vllm/loopback-seat"])
        .args(ROUTE);
    if capture {
        cmd.arg("--private-authoring-capture");
    }
    let out = cmd.output().expect("CLI");
    let doc = result(&out);
    Run {
        out,
        doc,
        bodies: seat.bodies(),
    }
}

/// One real `nika compile` on the Anthropic loopback route, whose answers carry block arrays.
fn anthropic(room: &Path, blocks: &[Value]) -> Run {
    let script = blocks.iter().map(|b| format!("anthropic {b}")).collect();
    let seat = LoopbackSeat::start(script);
    let port = seat.port;
    let out = command(room)
        .env(
            "NIKA_ANTHROPIC_BASE_URL",
            format!("http://127.0.0.1:{port}/v1/messages"),
        )
        .env("NIKA_ANTHROPIC_API_KEY", KEY)
        .args([
            "compile",
            INTENT,
            "--authoring-model",
            "anthropic/claude-loopback",
        ])
        .args(ROUTE)
        .arg("--private-authoring-capture")
        .output()
        .expect("CLI");
    let doc = result(&out);
    Run {
        out,
        doc,
        bodies: seat.bodies(),
    }
}

fn capture_dir(room: &Path) -> PathBuf {
    room.join(".nika/compile/capture")
}

/// The one reserved file of a room: its name and bytes.
fn artifact(room: &Path) -> (String, Vec<u8>) {
    let mut names: Vec<String> = std::fs::read_dir(capture_dir(room))
        .expect("capture dir")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .filter(|name| name.ends_with(".capture"))
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let name = names.remove(0);
    let bytes = std::fs::read(capture_dir(room).join(&name)).expect("artifact");
    (name, bytes)
}

/// NDJSON records, then only the space padding of the 1 MiB reservation.
fn records(bytes: &[u8]) -> Vec<Value> {
    assert_eq!(bytes.len(), 1024 * 1024, "the whole reservation is kept");
    let text = std::str::from_utf8(bytes).expect("utf-8");
    let body = text.trim_end_matches(' ');
    assert!(body.ends_with('\n'));
    body.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON record per line"))
        .collect()
}

fn kinds(records: &[Value]) -> Vec<&str> {
    records
        .iter()
        .map(|r| r["kind"].as_str().expect("kind"))
        .collect()
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The observer's private framing: each Text block as `<len>:<bytes>`.
fn framed(blocks: &[&str]) -> String {
    let mut hash = Sha256::new();
    for block in blocks {
        hash.update(format!("{}:", block.len()).as_bytes());
        hash.update(block.as_bytes());
    }
    hex(hash.finalize())
}

/// The public receipt's concatenated digest.
fn concatenated(blocks: &[&str]) -> String {
    hex(Sha256::digest(blocks.concat().as_bytes()))
}

/// The opaque identity of the one local stderr status line.
fn stderr_identity(out: &Output) -> [u64; 2] {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find(|line| line.starts_with("private capture "))
        .expect("one local status line");
    let inner = line
        .split("id Some([")
        .nth(1)
        .and_then(|rest| rest.split("])").next())
        .expect("an identity");
    let mut parts = inner.split(", ").map(|n| n.parse::<u64>().expect("u64"));
    [parts.next().expect("pid"), parts.next().expect("nanos")]
}

/// Every byte the room holds outside the capture container, plus the process streams.
fn public_bytes(room: &Path, out: &Output) -> Vec<u8> {
    let mut all = out.stdout.clone();
    all.extend(&out.stderr);
    let mut stack = vec![room.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir") {
            let path = entry.expect("entry").path();
            if path == capture_dir(room) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                all.extend(std::fs::read(&path).expect("file"));
            }
        }
    }
    all
}

/// A raw answer never appears in public bytes, neither as sent nor JSON-escaped.
fn raw_absent(public: &[u8], raw: &str) {
    let escaped = serde_json::to_string(raw).expect("escape");
    assert!(!contains(public, raw), "raw answer published");
    assert!(
        !contains(public, &escaped[1..escaped.len() - 1]),
        "escaped answer published"
    );
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn phases(doc: &Value) -> Vec<String> {
    doc["provenance"]["authoring"]["context"]
        .as_array()
        .expect("context")
        .iter()
        .map(|c| c["call"].as_str().expect("call").to_owned())
        .collect()
}

#[test]
fn a_rejected_sketch_is_saved_before_its_repair_and_never_published() {
    let room = tempfile::tempdir().expect("room");
    let first = rejected_sketch("SENTINEL-REJECTED-ONE é \"q\" \\ \n line");
    let second = unknown_key("SENTINEL-REPAIR-TWO");
    let run = vllm(room.path(), vec![first.clone(), second.clone()], true);
    assert_eq!(run.out.status.code(), Some(2), "{}", run.doc);
    assert_eq!(run.doc["status"], "incomplete");
    assert_eq!(phases(&run.doc), ["sketch", "sketch-repair"]);
    assert_eq!(run.bodies.len(), 2);

    let (name, bytes) = artifact(room.path());
    let identity = stderr_identity(&run.out);
    assert_eq!(name, format!("{}-{}.capture", identity[0], identity[1]));
    let records = records(&bytes);
    assert_eq!(
        kinds(&records),
        [
            "scope",
            "call",
            "call",
            "call_metadata",
            "call_metadata",
            "close"
        ]
    );
    assert_eq!(records[0]["identity"], json!(identity));
    let context = run.doc["provenance"]["authoring"]["context"]
        .as_array()
        .expect("context");
    for (i, (role, text)) in [("sketch", &first), ("sketch-repair", &second)]
        .iter()
        .enumerate()
    {
        let call = &records[1 + i];
        assert_eq!(call["ordinal"], i + 1);
        assert_eq!(call["role"], *role);
        let response = &call["response"];
        assert_eq!(response["text"], json!([text]), "exact bytes, in order");
        assert_eq!(response["text_blocks"], 1);
        assert_eq!(response["text_bytes"], text.len());
        assert_eq!(response["framed_sha256"], framed(&[text]));
        // The public receipt's digest of the same call is the concatenation, recomputed here.
        assert_eq!(context[i]["response"]["sha256"], concatenated(&[text]));
        assert_eq!(call["returned_metadata"]["usage_reported"], true);
        assert_eq!(call["returned_metadata"]["input_tokens"], 1000);
        assert_eq!(call["returned_metadata"]["output_tokens"], 200);
        assert_eq!(
            call["returned_metadata"]["response_model"],
            "loopback-served-model"
        );
        assert_eq!(records[3 + i]["ordinal"], i + 1);
    }
    let close = &records[5];
    assert_eq!(close["outcome_returned"], true);
    assert_eq!(
        (
            close["received"].as_u64(),
            close["saved_call_records"].as_u64()
        ),
        (Some(2), Some(2))
    );
    assert_eq!(close["outcome_metadata_saved"], 2);
    assert_eq!(close["outcome_metadata_unknown"], 0);
    let file = std::fs::metadata(capture_dir(room.path()).join(&name)).expect("meta");
    assert_eq!(file.mode() & 0o777, 0o600);

    let public = public_bytes(room.path(), &run.out);
    assert!(
        !contains(&public, "SENTINEL-REJECTED-ONE"),
        "a withheld field stays private"
    );
    raw_absent(&public, &first);
    raw_absent(&public, &second);
    // A later compile in the same project never reads the capture back into a request.
    let again = vllm(
        room.path(),
        vec![rejected_sketch("LATER"), unknown_key("LATER")],
        false,
    );
    for body in &again.bodies {
        let body = body.to_string();
        assert!(!body.contains("SENTINEL-REJECTED-ONE") && !body.contains("SENTINEL-REPAIR-TWO"));
    }
    assert_eq!(
        artifact(room.path()).1,
        bytes,
        "the saved file is left as it was"
    );
}

#[test]
fn a_terminal_refusal_keeps_every_text_block_but_no_reasoning_or_key() {
    let room = tempfile::tempdir().expect("room");
    let texts = [
        "{\"name\":\"draft\",\"tasks\":[],",
        "",
        "\"notes\":\"é \\\"q\\\" ✓\"}",
    ];
    let blocks = json!([
        {"type": "text", "text": texts[0]},
        {"type": "text", "text": texts[1]},
        {"type": "thinking", "thinking": "HIDDEN-THINKING-SENTINEL"},
        {"type": "text", "text": texts[2]},
    ]);
    let run = anthropic(room.path(), &[blocks]);
    assert_eq!(run.out.status.code(), Some(2), "{}", run.doc);
    assert_eq!(
        phases(&run.doc),
        ["sketch"],
        "a terminal refusal, no repair"
    );
    let (_, bytes) = artifact(room.path());
    let records = records(&bytes);
    assert_eq!(kinds(&records), ["scope", "call", "call_metadata", "close"]);
    let response = &records[1]["response"];
    assert_eq!(
        response["text"],
        json!(texts),
        "three Text blocks, the empty one kept"
    );
    assert_eq!(response["text_blocks"], 3);
    assert_eq!(response["other_blocks"], 1);
    let total: usize = texts.iter().map(|t| t.len()).sum();
    assert_eq!(response["text_bytes"], total);
    assert_eq!(response["framed_sha256"], framed(&texts));
    let context = &run.doc["provenance"]["authoring"]["context"][0]["response"];
    assert_eq!(context["sha256"], concatenated(&texts));
    assert_ne!(
        framed(&texts),
        concatenated(&texts),
        "two distinct identities"
    );
    let public = public_bytes(room.path(), &run.out);
    for (haystack, label) in [(&bytes, "artifact"), (&public, "public")] {
        assert!(!contains(haystack, "HIDDEN-THINKING-SENTINEL"), "{label}");
        assert!(!contains(haystack, KEY), "{label}");
    }
}

#[test]
fn a_known_key_split_across_blocks_withholds_the_whole_text() {
    let room = tempfile::tempdir().expect("room");
    let texts = ["{\"notes\":\"sk-ant-synth", "etic-capture-key\"}"];
    let blocks = json!([{"type": "text", "text": texts[0]}, {"type": "text", "text": texts[1]}]);
    let run = anthropic(room.path(), &[blocks]);
    assert!(String::from_utf8_lossy(&run.out.stderr).contains("private capture Withheld"));
    let (_, bytes) = artifact(room.path());
    let records = records(&bytes);
    let response = &records[1]["response"];
    assert!(
        response["text"].is_null(),
        "the whole payload is withheld, never a prefix"
    );
    assert_eq!(response["withheld_reason"], "host_withheld");
    assert_eq!(response["text_blocks"], 2);
    assert_eq!(response["text_bytes"], texts.concat().len());
    assert_eq!(response["framed_sha256"], framed(&texts));
    assert!(!contains(&bytes, KEY) && !contains(&bytes, "sk-ant-synth"));
    assert!(!contains(&public_bytes(room.path(), &run.out), KEY));
}

#[test]
fn an_oversized_answer_is_withheld_whole_with_its_counts() {
    let room = tempfile::tempdir().expect("room");
    let big = format!("BIGPREFIX{}", "A".repeat(300 * 1024));
    let run = vllm(room.path(), vec![big.clone()], true);
    assert_eq!(run.bodies.len(), 1);
    let (_, bytes) = artifact(room.path());
    let records = records(&bytes);
    let response = &records[1]["response"];
    assert!(response["text"].is_null());
    assert_eq!(response["withheld_reason"], "returned_text_bound");
    assert_eq!(response["text_bytes"], big.len());
    assert_eq!(response["framed_sha256"], framed(&[&big]));
    assert!(
        !contains(&bytes, "BIGPREFIX"),
        "no plausible prefix is saved"
    );
}

/// The public document without the values that differ between any two runs.
fn comparable(run: &Run) -> Value {
    let mut doc = run.doc.clone();
    let authoring = &mut doc["provenance"]["authoring"];
    authoring["backend"]["host"] = Value::Null;
    authoring["elapsed_ms"] = Value::Null;
    for call in authoring["context"].as_array_mut().expect("context") {
        call["elapsed_ms"] = Value::Null;
    }
    json!({"exit": run.out.status.code(), "doc": doc, "bodies": run.bodies})
}

#[test]
fn capture_off_on_and_unavailable_leave_the_compile_identical() {
    let script = || {
        vec![
            rejected_sketch("SENTINEL-AB-NAME"),
            unknown_key("SENTINEL-AB2"),
        ]
    };
    let off = tempfile::tempdir().expect("room");
    let on = tempfile::tempdir().expect("room");
    let unsafe_room = tempfile::tempdir().expect("room");
    std::fs::create_dir_all(capture_dir(unsafe_room.path())).expect("dir");
    std::fs::set_permissions(
        capture_dir(unsafe_room.path()),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("mode");
    let refused_marker = tempfile::tempdir().expect("room");
    std::fs::create_dir_all(capture_dir(refused_marker.path())).expect("dir");
    let dir = capture_dir(refused_marker.path());
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    std::fs::write(dir.join(".gitignore"), "x\n").expect("marker");
    std::fs::set_permissions(
        dir.join(".gitignore"),
        std::fs::Permissions::from_mode(0o600),
    )
    .expect("mode");

    let runs = [
        vllm(off.path(), script(), false),
        vllm(on.path(), script(), true),
        vllm(unsafe_room.path(), script(), true),
        vllm(refused_marker.path(), script(), true),
    ];
    let reference = comparable(&runs[0]);
    for run in &runs[1..] {
        assert_eq!(
            comparable(run),
            reference,
            "capture never changes the compile"
        );
    }
    // Off writes nothing at all and prints no status line.
    assert_eq!(std::fs::read_dir(off.path()).expect("dir").count(), 0);
    assert!(!String::from_utf8_lossy(&runs[0].out.stderr).contains("private capture"));
    // On saves a real artifact.
    let (_, bytes) = artifact(on.path());
    assert_eq!(kinds(&records(&bytes)).first(), Some(&"scope"));
    assert!(String::from_utf8_lossy(&runs[1].out.stderr).contains("private capture Saved"));
    // An unsafe container is refused untouched; a wrong ignore marker withdraws the round.
    assert_eq!(
        std::fs::read_dir(capture_dir(unsafe_room.path()))
            .expect("dir")
            .count(),
        0
    );
    for run in &runs[2..] {
        assert!(String::from_utf8_lossy(&run.out.stderr).contains("private capture Unavailable"));
    }
    assert_eq!(
        std::fs::read(dir.join(".gitignore")).expect("marker"),
        b"x\n"
    );
    // The refused marker is found before any round is reserved: no lock, no 1 MiB file.
    let left: Vec<String> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .collect();
    assert_eq!(
        left,
        [".gitignore"],
        "a refused marker leaves the container as it was"
    );
    for (run, room) in runs.iter().zip([&off, &on, &unsafe_room, &refused_marker]) {
        let public = public_bytes(room.path(), &run.out);
        assert!(!contains(&public, "SENTINEL-AB-NAME"));
        for raw in script() {
            raw_absent(&public, &raw);
        }
    }
}

#[test]
fn opting_in_without_a_native_model_is_refused_before_anything() {
    let room = tempfile::tempdir().expect("room");
    let out = command(room.path())
        .args(["compile", INTENT, "--private-authoring-capture", "--json"])
        .output()
        .expect("CLI");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--authoring-model"));
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn a_cancelled_compile_keeps_the_synchronized_first_response_and_claims_nothing_more() {
    let room = tempfile::tempdir().expect("room");
    let first = rejected_sketch("SENTINEL-BEFORE-CANCEL");
    let seat = LoopbackSeat::start(vec![first.clone(), "hold".to_owned()]);
    let mut child = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", INTENT, "--authoring-model", "vllm/loopback-seat"])
        .args([
            "--authoring-strategy",
            "sketch",
            "--authoring-max-calls",
            "3",
        ])
        .args(["--authoring-timeout", "60"])
        .args(["--json", "--private-authoring-capture"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("CLI");
    // Wait until the repair request is held by the seat: the first response was returned and
    // its record synchronized before that second request was sent.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while seat.bodies().len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the repair request never arrived"
        );
        assert!(
            child.try_wait().expect("child").is_none(),
            "the compile ended early"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    child.kill().expect("cancel");
    let status = child.wait().expect("wait");
    assert!(!status.success());

    let (_, bytes) = artifact(room.path());
    let records = records(&bytes);
    assert_eq!(
        kinds(&records),
        ["scope", "call"],
        "no later response, no close, no outcome"
    );
    let call = &records[1];
    assert_eq!(call["role"], "sketch");
    assert_eq!(call["response"]["text"], json!([first]));
    let metadata = &call["returned_metadata"];
    assert_eq!(metadata["usage_reported"], true);
    assert_eq!(metadata["output_tokens"], 200);
    assert_eq!(metadata["response_model"], "loopback-served-model");
    assert_eq!(metadata["stop_reason_kind"], "end_turn");
    assert_eq!(call["physical_send"], "unknown");
    assert_eq!(
        seat.bodies().len(),
        2,
        "the held request was received, never answered"
    );
}

/// A synthetic `TypeSafe` key: the decision seat resolves it, so capture must withhold it.
const TYPESAFE_KEY: &str = "synthetic-typesafe-capture-key";

/// One real compile with an operator-named `TypeSafe` decision seat beside the vLLM author; the
/// decision seat's endpoint is a second loopback seat.
fn with_typesafe(room: &Path, answer: &str, capture: bool) -> (Run, Vec<Value>) {
    let seat = LoopbackSeat::start(vec![answer.to_owned()]);
    let decision = LoopbackSeat::start(vec!["{}".to_owned()]);
    let mut cmd = command(room);
    cmd.env("NIKA_VLLM_BASE_URL", seat.base())
        .env("TYPESAFE_API_KEY", TYPESAFE_KEY)
        .env(
            "TYPESAFE_BASE_URL",
            format!("http://127.0.0.1:{}", decision.port),
        )
        .args(["compile", INTENT, "--authoring-model", "vllm/loopback-seat"])
        .args(["--decision-model", "typesafe/jev-1.13.0"])
        .args(ROUTE);
    if capture {
        cmd.arg("--private-authoring-capture");
    }
    let out = cmd.output().expect("CLI");
    let doc = result(&out);
    let run = Run {
        out,
        doc,
        bodies: seat.bodies(),
    };
    (run, decision.bodies())
}

#[test]
fn the_resolved_typesafe_key_is_withheld_from_capture() {
    let answer = rejected_sketch(&format!("carries {TYPESAFE_KEY} here"));
    let off_room = tempfile::tempdir().expect("room");
    let on_room = tempfile::tempdir().expect("room");
    let (off, off_decisions) = with_typesafe(off_room.path(), &answer, false);
    let (on, on_decisions) = with_typesafe(on_room.path(), &answer, true);
    assert_eq!(
        comparable(&on),
        comparable(&off),
        "capture never changes the compile"
    );
    assert_eq!(
        on_decisions, off_decisions,
        "nor the decision seat's requests"
    );
    assert!(String::from_utf8_lossy(&on.out.stderr).contains("private capture Withheld"));
    let (_, bytes) = artifact(on_room.path());
    let records = records(&bytes);
    let calls: Vec<&Value> = records.iter().filter(|r| r["kind"] == "call").collect();
    assert!(!calls.is_empty());
    for call in calls {
        let response = &call["response"];
        assert!(
            response["text"].is_null(),
            "the key-bearing Text is withheld whole"
        );
        assert_eq!(response["withheld_reason"], "host_withheld");
        assert_eq!(response["text_bytes"], answer.len());
        assert_eq!(response["framed_sha256"], framed(&[&answer]));
    }
    assert!(
        !contains(&bytes, TYPESAFE_KEY),
        "the key never reaches the artifact"
    );
    assert!(!contains(
        &public_bytes(on_room.path(), &on.out),
        TYPESAFE_KEY
    ));
}
