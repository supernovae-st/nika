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
    // A stop on the call ceiling or on a failed provider call is an unfinished outcome (2),
    // named by its diagnostics and receipt; the help never promises it the environment's 3.
    assert!(
        text.contains("an authoring stop on the call ceiling or on a failed provider call"),
        "{text}"
    );
    assert!(!text.contains("provider/limit failure"), "{text}");
    assert!(text.contains("--fresh"), "{text}");
    assert!(text.contains("-o, --output"), "{text}");
}

/// A source change between the question and its answer (R4 A6 · D6-S1): the stale answer is
/// refused and asked again over the fresh keys, and the record is refreshed to the observation
/// that question showed, so the next explicit answer binds. The same answer sent again is not
/// taken for the new file's key; a refresh that cannot be written leaves the old record intact.
#[test]
fn a_renamed_column_refreshes_the_record_and_a_fresh_answer_binds() {
    const WORD: &str =
        "Read ./tickets.csv, keep only the rows whose state is open and write them to ./open.csv";
    const KEY: &str = "const.rule_field_1";
    let room = tempfile::tempdir().expect("room");
    let csv = |header: &str| {
        std::fs::write(
            room.path().join("tickets.csv"),
            format!("{header}\n1,open,10\n2,closed,20\n3,open,30\n"),
        )
        .expect("csv");
    };
    csv("id,status,amount");
    let path = sidecar(room.path(), &intent_sha256(WORD));
    let offered = |doc: &Value| -> Vec<String> {
        doc["questions"]
            .as_array()
            .and_then(|qs| qs.iter().find(|q| q["key"] == KEY))
            .map(|q| {
                q["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["key"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let says = |doc: &Value, text: &str| {
        doc["diagnostics"].as_array().is_some_and(|ds| {
            ds.iter()
                .any(|d| d["message"].as_str().is_some_and(|m| m.contains(text)))
        })
    };
    let first = result(&call(room.path(), &["compile", WORD, "--json"]));
    assert_eq!(offered(&first), ["id", "status", "amount"], "{first}");
    csv("id,state,amount");
    let before = std::fs::read(&path).expect("record");
    // A refresh that cannot be written: the old record stays byte for byte, the error is named.
    let dir = room.path().join(".nika/compile");
    let mut locked = std::fs::metadata(&dir).expect("dir").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut locked, 0o555);
    std::fs::set_permissions(&dir, locked).expect("lock");
    let answer = |literal: &str| {
        call(
            room.path(),
            &[
                "compile",
                WORD,
                "--json",
                "--answer",
                &format!("{KEY}=\"{literal}\""),
            ],
        )
    };
    let blocked = result(&answer("status"));
    assert!(blocked.get("plan_record_error").is_some(), "{blocked}");
    assert_eq!(std::fs::read(&path).expect("record"), before);
    let mut open = std::fs::metadata(&dir).expect("dir").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut open, 0o755);
    std::fs::set_permissions(&dir, open).expect("unlock");
    // The stale answer: refused, asked again over the fresh keys, the record refreshed.
    let stale = answer("status");
    let doc = result(&stale);
    assert_eq!(stale.status.code(), Some(2), "{doc}");
    assert!(says(&doc, "changed since this question was asked"), "{doc}");
    assert!(!says(&doc, "No current question owns this answer"), "{doc}");
    assert_eq!(offered(&doc), ["id", "state", "amount"], "{doc}");
    assert!(!room.path().join("open.csv").exists());
    let record = read_json(&path);
    assert_eq!(record["plan"]["reasked"], json!([KEY]), "{record}");
    assert_eq!(
        record["plan"]["observed_world"]["observed"][0]["columns"],
        json!(["id", "state", "amount"]),
        "{record}"
    );
    // The old spelling is not the new file's key; the fresh explicit answer binds.
    let old = result(&answer("status"));
    assert_eq!(old["status"], "incomplete", "{old}");
    assert!(says(&old, "Choose an observed field"), "{old}");
    let bound = answer("state");
    let doc = result(&bound);
    assert_eq!(bound.status.code(), Some(0), "{doc}");
    assert_eq!(doc["status"], "ready");
    assert!(
        doc["candidate"].as_str().is_some_and(
            |c| c.contains(".state == \\\"open\\\"") || c.contains(".state == \"open\"")
        ),
        "{doc}"
    );
}

/// A plan recorded before value kinds were observed (a pre-A5 record, R4 A6): its first answer
/// round is asked over the kinds, truthfully (the file did not change), and refreshes the
/// record; the next answer binds.
#[test]
fn a_pre_kinds_record_refreshes_on_its_first_answer() {
    const ABOVE: &str = "Read ./tickets.json, keep only the rows whose amount is above 100 and write them to ./big.json";
    const KEY: &str = "const.rule_number_1";
    let room = tempfile::tempdir().expect("room");
    std::fs::write(
        room.path().join("tickets.json"),
        r#"[{"id":1,"amount":120},{"id":2,"amount":"150"},{"id":3,"amount":null},{"id":4,"amount":"n-a"},{"id":5,"amount":90}]"#,
    )
    .expect("json");
    let first = result(&call(room.path(), &["compile", ABOVE, "--json"]));
    assert_eq!(first["questions"][0]["key"], KEY, "{first}");
    let path = sidecar(room.path(), &intent_sha256(ABOVE));
    let mut record = read_json(&path);
    let world = record["plan"]["observed_world"]
        .as_object_mut()
        .expect("observed world");
    assert!(
        world.remove("kinds").is_some(),
        "the fresh record carries kinds"
    );
    std::fs::write(&path, serde_json::to_string_pretty(&record).expect("json")).expect("legacy");
    let skip = || {
        call(
            room.path(),
            &[
                "compile",
                ABOVE,
                "--json",
                "--answer",
                &format!("{KEY}=\"skip\""),
            ],
        )
    };
    let refused = result(&skip());
    assert_eq!(refused["status"], "incomplete", "{refused}");
    let messages = refused["diagnostics"].to_string();
    assert!(messages.contains("predates the value kinds"), "{refused}");
    assert!(!messages.contains("changed since"), "{refused}");
    assert!(read_json(&path)["plan"]["observed_world"]["kinds"].is_object());
    let bound = skip();
    let doc = result(&bound);
    assert_eq!(bound.status.code(), Some(0), "{doc}");
    assert_eq!(doc["status"], "ready");
}

/// An incomplete compile given a destination that already holds something (R4 A6): it stays
/// byte for byte (an earlier candidate, an arbitrary file, a dangling link, `--force` or not),
/// the JSON names that caller-named path beside `written: null` and the text says it remains,
/// teaching no run. An absent destination stays absent and is never named, even after another
/// destination was.
#[test]
fn an_incomplete_compile_names_the_destination_it_left_in_place() {
    let room = tempfile::tempdir().expect("room");
    let at = |name: &str| room.path().join(name);
    let hello = call(room.path(), &["compile", "hello", "earlier.nika", "--json"]);
    let wrote = result(&hello);
    assert_eq!(hello.status.code(), Some(0), "{wrote}");
    assert_eq!(wrote["written"], "earlier.nika");
    assert!(wrote.get("existing_destination").is_none(), "{wrote}");
    let earlier = std::fs::read(at("earlier.nika")).expect("the earlier candidate");
    std::fs::write(at("other.nika"), "not a workflow\n").expect("file");
    std::os::unix::fs::symlink("gone.nika", at("link.nika")).expect("link");
    for (dest, force) in [
        ("earlier.nika", false),
        ("earlier.nika", true),
        ("other.nika", false),
        ("link.nika", false),
    ] {
        let mut args = vec!["compile", INTENT, dest, "--json"];
        if force {
            args.push("--force");
        }
        let out = call(room.path(), &args);
        let doc = result(&out);
        assert_eq!(out.status.code(), Some(2), "{doc}");
        assert_eq!(doc["status"], "incomplete");
        assert_eq!(doc["written"], Value::Null);
        assert_eq!(doc["existing_destination"], dest, "{doc}");
    }
    assert_eq!(std::fs::read(at("earlier.nika")).expect("kept"), earlier);
    assert_eq!(
        std::fs::read(at("other.nika")).expect("kept"),
        b"not a workflow\n"
    );
    assert_eq!(
        std::fs::read_link(at("link.nika")).expect("still a link"),
        Path::new("gone.nika")
    );
    assert!(!at("gone.nika").exists(), "the link was never followed");
    let absent = result(&call(
        room.path(),
        &["compile", INTENT, "absent.nika", "--json"],
    ));
    assert!(absent.get("existing_destination").is_none(), "{absent}");
    assert!(!at("absent.nika").exists());
    let human = call(room.path(), &["compile", INTENT, "earlier.nika"]);
    let text = String::from_utf8_lossy(&human.stdout);
    assert_eq!(human.status.code(), Some(2), "{text}");
    assert!(
        text.contains(
            "existing destination remains at earlier.nika; this compile did not write or remove it"
        ),
        "{text}"
    );
    assert!(!text.contains("nika run"), "{text}");
    assert_eq!(std::fs::read(at("earlier.nika")).expect("kept"), earlier);
}
