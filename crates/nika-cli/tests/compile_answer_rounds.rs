// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types, clippy::panic)]

//! The answer loop is stable: the first `nika compile` of a free intent records the plan it
//! produced under `.nika/compile/<intent sha256>.plan.json`; every later `--answer` round of
//! the same intent replays that record (route `replayed plan`, zero provider calls, the same
//! candidate) and `--fresh` reads the intent again. A skeleton records nothing.
use nika_onboard::compile::{CompileRequest, compile, intent_sha256};
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Every clause explicit: the deterministic reader admits it and asks only for the model.
const INTENT: &str = "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md";

fn command(room: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .current_dir(room)
        .stdin(Stdio::null());
    cmd
}

fn call(room: &Path, args: &[&str]) -> Output {
    command(room).args(args).output().expect("CLI")
}

fn result(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn sidecar(room: &Path, sha: &str) -> std::path::PathBuf {
    room.join(".nika/compile").join(format!("{sha}.plan.json"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("file")).expect("json")
}

/// Round 1 of the answer loop: the preview of a free intent records the plan it produced.
fn first_round(room: &Path) -> Value {
    let first = call(room, &["compile", INTENT, "--json"]);
    let doc = result(&first);
    assert_eq!(first.status.code(), Some(2), "{doc}");
    assert_eq!(doc["status"], "incomplete");
    assert_eq!(doc["questions"][0]["key"], "model");
    doc
}

#[test]
fn an_answer_round_replays_the_recorded_plan() {
    let room = tempfile::tempdir().expect("room");
    let sha = intent_sha256(INTENT);
    let doc = first_round(room.path());
    assert_eq!(doc["status"], "incomplete");
    assert_eq!(doc["questions"][0]["key"], "model");
    assert_eq!(doc["provenance"]["strategy"], "hot");
    assert_eq!(doc["provenance"]["decision"]["route"], json!(["hot"]));
    assert_eq!(doc["provenance"]["decision"]["intent_sha256"], sha);
    assert!(doc.get("plan_record_error").is_none(), "{doc}");
    let path = sidecar(room.path(), &sha);
    let record = read_json(&path);
    let keys: Vec<_> = record
        .as_object()
        .expect("record")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        [
            "compile_version",
            "created_at",
            "engine",
            "intent_sha256",
            "plan",
            "strategy"
        ],
        "the record carries the plan, never the candidate or a key"
    );
    assert_eq!(record["compile_version"], 1);
    assert_eq!(record["engine"], doc["provenance"]["compiler_version"]);
    assert_eq!(record["intent_sha256"], sha);
    assert_eq!(record["strategy"], "hot");
    assert_eq!(record["plan"], doc["provenance"]["plan"]);
    assert_eq!(record["plan"]["strategy"], "hot");
    assert!(
        record["created_at"]
            .as_str()
            .is_some_and(|t| t.contains('T') && t.ends_with('Z')),
        "{record}"
    );
    assert_eq!(
        std::fs::read_to_string(room.path().join(".nika/compile/.gitignore")).expect("ignore"),
        "*\n",
        "the record directory ignores itself"
    );
    let recorded = std::fs::read_to_string(&path).expect("record bytes");
    // Round 2: the answer round replays the record, writes DEST (creating its parent).
    let second = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "out/summary.nika",
            "--json",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    let doc = result(&second);
    assert_eq!(second.status.code(), Some(0), "{doc}");
    assert_eq!(doc["status"], "ready");
    assert_eq!(doc["written"], "out/summary.nika");
    assert_eq!(
        doc["provenance"]["decision"]["route"],
        json!(["replayed plan"])
    );
    assert_eq!(doc["provenance"]["decision"]["intent_sha256"], sha);
    assert_eq!(doc["provenance"]["strategy"], "hot");
    assert_eq!(doc["provenance"]["cognition"], "deterministicOnly");
    assert!(doc["provenance"].get("authoring").is_none());
    assert_eq!(doc["compile_version"], 1);
    let expected = compile(
        &CompileRequest::create(INTENT)
            .with_workflow_id("summary")
            .answer("model", "\"mock/echo\""),
    )
    .expect("core")
    .candidate
    .expect("candidate");
    assert_eq!(
        std::fs::read_to_string(room.path().join("out/summary.nika")).expect("written"),
        expected
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("record bytes"),
        recorded,
        "a replay never rewrites the record"
    );
}

#[test]
fn fresh_reads_again_and_the_replayed_file_passes_check() {
    let room = tempfile::tempdir().expect("room");
    let sha = intent_sha256(INTENT);
    first_round(room.path());
    let path = sidecar(room.path(), &sha);
    let written = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "out/summary.nika",
            "--json",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    assert_eq!(written.status.code(), Some(0), "{}", result(&written));
    // The human surface names the replay; -o is the short destination flag.
    let human = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "-o",
            "out/again.nika",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(human.status.success(), "{text}");
    assert!(text.contains("replayed plan · .nika/compile/"), "{text}");
    assert!(room.path().join("out/again.nika").exists());
    // --fresh reads the intent again and re-records it.
    let fresh = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "--json",
            "--fresh",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    let doc = result(&fresh);
    assert_eq!(fresh.status.code(), Some(0), "{doc}");
    assert_eq!(doc["provenance"]["decision"]["route"], json!(["hot"]));
    assert_eq!(read_json(&path)["plan"], doc["provenance"]["plan"]);
    // The written file passes the real host Check.
    let check = call(room.path(), &["check", "out/summary.nika", "--json"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
}

#[test]
fn a_foreign_or_tampered_record_is_never_replayed_blindly() {
    let room = tempfile::tempdir().expect("room");
    let sha = intent_sha256(INTENT);
    let path = sidecar(room.path(), &sha);
    std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
    // Another engine's record is ignored: the round reads the intent and rewrites it.
    std::fs::write(
        &path,
        json!({"compile_version":1,"engine":"0.0.0","intent_sha256":sha,"plan":{"operations":"stale"},"strategy":"hot","created_at":"2020-01-01T00:00:00Z"}).to_string(),
    )
    .expect("stale record");
    let out = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "--json",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    let doc = result(&out);
    assert_eq!(out.status.code(), Some(0), "{doc}");
    assert_eq!(doc["provenance"]["decision"]["route"], json!(["hot"]));
    let record = read_json(&path);
    assert_eq!(record["engine"], doc["provenance"]["compiler_version"]);
    // A tampered record of this engine is replayed and refused: a finding, no candidate.
    let mut tampered = record.clone();
    tampered["plan"]["operations"][0]["evidence"] = json!("an excerpt the request never wrote");
    std::fs::write(&path, tampered.to_string()).expect("tampered record");
    let out = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "refused.nika",
            "--json",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    let doc = result(&out);
    assert_eq!(out.status.code(), Some(2), "{doc}");
    assert_eq!(doc["status"], "incomplete");
    assert!(doc["candidate"].is_null());
    assert!(
        doc["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|d| d["target"] == "recorded_plan" && d["kind"] == "unknown"),
        "{doc}"
    );
    assert!(!room.path().join("refused.nika").exists());
    assert_eq!(
        read_json(&path),
        tampered,
        "a refused replay does not rewrite the record"
    );
    // --fresh is the named way out.
    let out = call(
        room.path(),
        &[
            "compile",
            INTENT,
            "fresh.nika",
            "--json",
            "--fresh",
            "--answer",
            "model=\"mock/echo\"",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", result(&out));
    assert_eq!(read_json(&path)["plan"], record["plan"]);
    assert!(room.path().join("fresh.nika").exists());
}

#[test]
fn skeletons_and_edits_record_nothing_and_a_missing_parent_is_created() {
    let room = tempfile::tempdir().expect("room");
    let out = call(
        room.path(),
        &[
            "compile",
            "classify-and-route",
            "--json",
            "--answer",
            "const.request=\"An outage\"",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", result(&out));
    assert!(
        !room.path().join(".nika").exists(),
        "a skeleton has no plan"
    );
    let out = call(
        room.path(),
        &["compile", "hello", "deep/nested/hello.nika", "--json"],
    );
    let doc = result(&out);
    assert_eq!(out.status.code(), Some(0), "{doc}");
    assert_eq!(doc["written"], "deep/nested/hello.nika");
    assert!(room.path().join("deep/nested/hello.nika").exists());
    assert!(!room.path().join(".nika").exists());
    let help = call(room.path(), &["compile", "--help"]);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("exit codes · 0 READY"), "{text}");
    assert!(text.contains("--fresh"), "{text}");
    assert!(text.contains("-o, --output"), "{text}");
}
